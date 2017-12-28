use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::{Mutex, Arc};
use std::time::Duration;
use std::thread;
use std::net::{self, ToSocketAddrs};

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
    BuildConfig
};

use build::{
    BuildArgsMatcher,
    set_link_args,
    run_with_broken_first_build_hack
};
use config::Config;
use emscripten::check_for_emcc;
use error::Error;
use utils::{
    CommandExt,
    read,
    read_bytes
};
use wasm;

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

    fn is_wasm( &self ) -> bool {
        self.has_extension( "wasm" )
    }
}

fn monitor_for_changes_and_rebuild(
    package: &CargoPackage,
    target: &CargoTarget,
    build: BuildConfig,
    extra_path: Option< &Path >,
    outputs: Arc< Mutex< Vec< Output > > >
) -> RecommendedWatcher {
    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new( tx, Duration::from_millis( 500 ) ).unwrap();

    // TODO: Support local dependencies.
    // TODO: Support Cargo.toml reloading.
    watcher.watch( &target.source_directory, RecursiveMode::Recursive ).unwrap();
    watcher.watch( &package.manifest_path, RecursiveMode::NonRecursive ).unwrap();

    let extra_path = extra_path.map( |path| path.to_owned() );
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

            let mut command = build.as_command();
            if let Some( ref extra_path ) = extra_path {
                command.append_to_path( extra_path );
            }

            if command.run().is_ok() {
                let mut outputs = outputs.lock().unwrap();
                wasm::process_wasm_files( &build, &outputs );

                for output in outputs.iter_mut() {
                    if let Ok( data ) = read_bytes( &output.path ) {
                        output.data = data;
                    }
                }
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
    let use_system_emscripten = matches.is_present( "use-system-emscripten" );
    let targeting_webasm_unknknown_unknown = matches.is_present( "target-webasm" );
    let targeting_webasm = matches.is_present( "target-webasm-emscripten" ) || targeting_webasm_unknknown_unknown;
    let extra_path = if matches.is_present( "target-webasm" ) { None } else { check_for_emcc( use_system_emscripten, targeting_webasm ) };

    let build_matcher = BuildArgsMatcher {
        matches: matches,
        project: project
    };

    let package = build_matcher.package_or_default()?;
    let config = Config::load_for_package_printing_warnings( &package ).unwrap().unwrap_or_default();
    set_link_args( &config );

    let targets = build_matcher.target_or_select( package, |target| {
        target.kind == TargetKind::Bin
    })?;

    if targets.is_empty() {
        return Err(
            Error::ConfigurationError( format!( "cannot start a webserver for a crate which is a library!" ) )
        );
    }

    let target = &targets[ 0 ];
    let build_config = build_matcher.build_config( package, target, Profile::Main );

    let mut command = build_config.as_command();
    if let Some( ref extra_path ) = extra_path {
        command.append_to_path( extra_path );
    }

    run_with_broken_first_build_hack( package, &build_config, &mut command )?;

    let artifacts = build_config.potential_artifacts( &package.crate_root );

    let output_path = &artifacts[ 0 ];
    let wasm_path = output_path.with_extension( "wasm" );
    let wasm_url = format!( "/{}", wasm_path.file_name().unwrap().to_str().unwrap() );
    let mut outputs = vec![
        Output {
            path: output_path.to_owned(),
            data: read_bytes( output_path ).unwrap()
        }
    ];

    if targeting_webasm {
        outputs.push( Output {
            path: wasm_path.clone(),
            data: read_bytes( wasm_path ).unwrap(),
        });
    }

    if targeting_webasm_unknknown_unknown {
        let js_path = output_path.with_extension( "js" );
        outputs.push( Output {
            path: js_path.clone(),
            data: read_bytes( js_path ).unwrap()
        });
    }

    let outputs = Arc::new( Mutex::new( outputs ) );

    #[allow(unused_variables)]
    let watcher = monitor_for_changes_and_rebuild( &package, &target, build_config, extra_path.as_ref().map( |path| path.as_path() ), outputs.clone() );

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
                return response;
            }
        }

        response = rouille::match_assets( &request, &crate_static_path );
        if response.is_success() {
            return response;
        }

        let url = request.url();
        response = if url == "/" || url == "index.html" {
            let data = target_static_path.as_ref().and_then( |path| {
                read( path.join( "index.html" ) ).ok()
            }).or_else( || {
                read( crate_static_path.join( "index.html" ) ).ok()
            });

            if let Some( data ) = data {
                rouille::Response::html( data )
            } else {
                rouille::Response::html( DEFAULT_INDEX_HTML )
            }
        } else if url == "/js/app.js" {
            let data = outputs.lock().unwrap().iter().find( |output| output.is_js() ).unwrap().data.clone();
            rouille::Response::from_data( "application/javascript", data )
        } else if url == wasm_url {
            let data = outputs.lock().unwrap().iter().find( |output| output.is_wasm() ).unwrap().data.clone();
            rouille::Response::from_data( "application/wasm", data )
        } else {
            rouille::Response::empty_404()
        };

        response.with_no_cache()
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
