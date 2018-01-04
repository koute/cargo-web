use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::{Mutex, Arc};
use std::time::Duration;
use std::thread;
use std::mem;
use std::net::{self, ToSocketAddrs};
use std::ops::DerefMut;

use notify::{
    RecommendedWatcher,
    Watcher,
    RecursiveMode,
    DebouncedEvent
};

use clap;
use rouille;

use cargo_shim::{
    Profile,
    CargoPackage,
    CargoProject,
    TargetKind,
    CargoTarget,
    CargoResult
};

use build::{
    BuildArgsMatcher,
    Builder
};
use config::Config;
use error::Error;
use utils::{
    read,
    read_bytes
};

const DEFAULT_INDEX_HTML: &'static str = r#"
<!DOCTYPE html>
<head>
    <meta charset="utf-8" />
    <meta http-equiv="X-UA-Compatible" content="IE=edge" />
    <meta content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=1" name="viewport" />
    <script>
        var Module = {};
        var __cargo_web = {};
        Object.defineProperty( Module, 'canvas', {
            get: function() {
                if( __cargo_web.canvas ) {
                    return __cargo_web.canvas;
                }

                var canvas = document.createElement( 'canvas' );
                document.querySelector( 'body' ).appendChild( canvas );
                __cargo_web.canvas = canvas;

                return canvas;
            }
        });
    </script>
</head>
<body>
    <script src="js/app.js"></script>
</body>
</html>
"#;

struct Output {
    path: PathBuf,
    data: Vec< u8 >
}

impl AsRef< Path > for Output {
    fn as_ref( &self ) -> &Path {
        &self.path
    }
}

impl Output {
    fn has_extension( &self, extension: &str ) -> bool {
        self.path.extension().map( |ext| ext == extension ).unwrap_or( false )
    }

    fn is_js( &self ) -> bool {
        self.has_extension( "js" )
    }
}

fn result_to_outputs( result: CargoResult ) -> Vec< Output > {
    assert!( result.is_ok() );

    let mut outputs = Vec::new();
    for artifact in result.artifacts() {
        if let Ok( data ) = read_bytes( &artifact ) {
            outputs.push( Output {
                path: artifact.clone(),
                data
            });
        }
    }

    outputs
}

fn monitor_for_changes_and_rebuild(
    package: &CargoPackage,
    target: &CargoTarget,
    builder: Builder,
    outputs: Arc< Mutex< Vec< Output > > >
) -> RecommendedWatcher {
    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new( tx, Duration::from_millis( 500 ) ).unwrap();

    // TODO: Support local dependencies.
    // TODO: Support Cargo.toml reloading.
    watcher.watch( &target.source_directory, RecursiveMode::Recursive ).unwrap();
    watcher.watch( &package.manifest_path, RecursiveMode::NonRecursive ).unwrap();
    thread::spawn( move || {
        let rx = rx;
        while let Ok( event ) = rx.recv() {
            match event {
                DebouncedEvent::Create( _ ) |
                DebouncedEvent::Remove( _ ) |
                DebouncedEvent::Rename( _, _ ) |
                DebouncedEvent::Write( _ ) => {},
                _ => continue
            };

            println_err!( "==== Triggering `cargo build` ====" );
            let new_result = builder.run();
            if let Ok( new_result ) = new_result {
                let mut new_outputs = result_to_outputs( new_result );
                mem::swap( outputs.lock().unwrap().deref_mut(), &mut new_outputs )
            }
        }
    });

    watcher
}

fn address_or_default< 'a >( matches: &clap::ArgMatches< 'a > ) -> net::SocketAddr {
    let host = matches.value_of( "host" ).unwrap_or( "localhost" );
    let port = matches.value_of( "port" ).unwrap_or( "8000" );
    format!( "{}:{}", host, port ).to_socket_addrs().unwrap().next().unwrap()
}

pub fn command_start< 'a >( matches: &clap::ArgMatches< 'a >, project: &CargoProject ) -> Result< (), Error > {
    let build_matcher = BuildArgsMatcher {
        matches: matches,
        project: project
    };

    let package = build_matcher.package_or_default()?;
    let config = Config::load_for_package_printing_warnings( &package ).unwrap().unwrap_or_default();
    let targets = build_matcher.target_or_select( package, |target| {
        target.kind == TargetKind::Bin
    })?;

    if targets.is_empty() {
        return Err(
            Error::ConfigurationError( format!( "cannot start a webserver for a crate which is a library!" ) )
        );
    }

    let target = &targets[ 0 ];
    let builder = build_matcher.prepare_builder( &config, package, target, Profile::Main );
    let result = builder.run()?;
    let outputs = result_to_outputs( result );
    let outputs = Arc::new( Mutex::new( outputs ) );

    #[allow(unused_variables)]
    let watcher = monitor_for_changes_and_rebuild( &package, &target, builder, outputs.clone() );

    let crate_static_path = package.crate_root.join( "static" );
    let target_static_path = match target.kind {
        TargetKind::Example => Some( target.source_directory.join( format!( "{}-static", target.name ) ) ),
        TargetKind::Bin => Some( target.source_directory.join( "static" ) ),
        _ => None
    };

    let address = address_or_default( matches );
    let server = rouille::Server::new( &address, move |request| {
        let mut response;

        if let Some( ref target_static_path ) = target_static_path {
            response = rouille::match_assets( &request, target_static_path );
            if response.is_success() {
                return response.with_no_cache();
            }
        }

        response = rouille::match_assets( &request, &crate_static_path );
        if response.is_success() {
            return response.with_no_cache();
        }

        let url = request.url();
        if url == "/" || url == "index.html" {
            let data = target_static_path.as_ref().and_then( |path| {
                read( path.join( "index.html" ) ).ok()
            }).or_else( || {
                read( crate_static_path.join( "index.html" ) ).ok()
            });

            if let Some( data ) = data {
                return rouille::Response::html( data ).with_no_cache();
            } else {
                return rouille::Response::html( DEFAULT_INDEX_HTML ).with_no_cache();
            }
        }

        if url == "/js/app.js" {
            let data = outputs.lock().unwrap().iter().find( |output| output.is_js() ).unwrap().data.clone();
            return rouille::Response::from_data( "application/javascript", data ).with_no_cache();
        }

        let requested_file = if url.starts_with( '/' ) {
            &url[ 1.. ]
        } else {
            &url
        };

        let outputs_guard = outputs.lock().unwrap();
        let output = outputs_guard.iter().find( |output| {
            output.path.file_name().map( |filename| requested_file == filename ).unwrap_or( false )
        });

        if let Some( output ) = output {
            let mime = match output.path.extension().map( |ext| ext.to_str().unwrap() ) {
                Some( "wasm" ) => "application/wasm",
                Some( "js" ) => "application/javascript",
                _ => "application/octet-stream"
            };

            return rouille::Response::from_data( mime, output.data.clone() ).with_no_cache();
        }

        rouille::Response::empty_404().with_no_cache()
    }).unwrap();

    println_err!( "" );
    println_err!( "If you need to serve any extra files put them in the 'static' directory" );
    println_err!( "in the root of your crate; they will be served alongside your application." );
    match target.kind {
        TargetKind::Example => println_err!( "You can also put a '{}-static' directory in your 'examples' directory.", target.name ),
        TargetKind::Bin => println_err!( "You can also put a 'static' directory in your 'src' directory." ),
        _ => unreachable!()
    };
    println_err!( "" );
    println_err!( "Your application is being served at '/js/app.js'. It will be automatically" );
    println_err!( "rebuilt if you make any changes in your code." );
    println_err!( "" );
    println_err!( "You can access the web server at `http://{}`.", &address );

    server.run();

    Ok(())
}
