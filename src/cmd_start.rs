use std::collections::BTreeMap;
use std::sync::mpsc::channel;
use std::sync::{Mutex, Arc};
use std::time::Duration;
use std::thread;
use std::net::{self, ToSocketAddrs};
use std::hash::Hash;
use std::time::{SystemTime, UNIX_EPOCH};
use std::path::PathBuf;

use notify::{
    RecommendedWatcher,
    Watcher,
    RecursiveMode,
    DebouncedEvent
};

use clap;
use rouille;
use handlebars::Handlebars;

use cargo_shim::{
    Profile,
    CargoPackage,
    TargetKind,
    CargoTarget
};

use build::{BuildArgs, Project};
use deployment::{Deployment, ArtifactKind};
use error::Error;

fn auto_reload_code( hash: u32 ) -> String {
    // TODO: We probably should do this with with Websockets,
    // but it isn't possible when using rouille as a web server. ):
    const TEMPLATE: &'static str = r##"
        window.addEventListener( "load", function() {
            var current_build_hash = {{{current_build_hash}}};
            function try_reload() {
                var req = new XMLHttpRequest();
                req.addEventListener( "load" , function() {
                    if( req.responseText != current_build_hash ) {
                        window.location.reload( true );
                    }
                });
                req.addEventListener( "loadend", function() {
                    setTimeout( try_reload, 500 );
                });
                req.open( "GET", "/__cargo-web__/build_hash" );
                req.send();
            }
            try_reload();
        });
    "##;

    let handlebars = Handlebars::new();
    let mut template_data = BTreeMap::new();
    template_data.insert( "current_build_hash", hash );
    handlebars.template_render( TEMPLATE, &template_data ).unwrap()
}

fn hash< T: Hash >( value: T ) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    let mut hasher = DefaultHasher::new();
    value.hash( &mut hasher );
    hasher.finish()
}

#[derive(Clone)]
struct Counter {
    seed: u64,
    value: u64
}

impl Counter {
    fn new() -> Self {
        let timestamp = SystemTime::now().duration_since( UNIX_EPOCH ).unwrap();
        let seed = hash( timestamp.as_secs() ) ^ hash( timestamp.subsec_nanos() );

        Counter {
            seed,
            value: 0
        }
    }

    fn next( &self ) -> Self {
        Counter {
            seed: self.seed,
            value: self.value + 1
        }
    }

    fn get_hash( &self ) -> u32 {
        hash( self.seed + self.value ) as u32
    }
}

// `notify`'s RecursiveMode is neither Copy nor Clone. ):
// TODO: Remove this once it is.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Mode {
    NonRecursive,
    Recursive
}

impl From< Mode > for RecursiveMode {
    fn from( mode: Mode ) -> Self {
        match mode {
            Mode::NonRecursive => RecursiveMode::NonRecursive,
            Mode::Recursive => RecursiveMode::Recursive
        }
    }
}

struct LastBuild {
    counter: Counter,
    deployment: Deployment,
    project: Project,
    package: CargoPackage,
    target: CargoTarget
}

fn get_paths_to_watch( project: &Project, main_package: &CargoPackage, target: &CargoTarget ) -> Vec< (PathBuf, Mode) > {
    let mut paths_to_watch = Vec::new();
    paths_to_watch.push( (target.source_directory.clone(), Mode::Recursive) );

    let packages = project.used_packages( main_package, Profile::Main );
    for package in packages {
        paths_to_watch.push( (package.manifest_path.clone(), Mode::NonRecursive) );
        if let Some( lib_target ) = package.targets.iter().find( |target| target.kind == TargetKind::Lib ) {
            paths_to_watch.push( (lib_target.source_directory.clone(), Mode::Recursive) );
        }
    }

    paths_to_watch
}

fn select_package_and_target( project: &Project ) -> Result< (CargoPackage, CargoTarget), Error > {
    let package = project.package().clone();
    let target = {
        let targets = project.target_or_select( None, |target| {
            target.kind == TargetKind::Bin
        })?;

        if targets.is_empty() {
            return Err(
                Error::ConfigurationError( format!( "cannot start a webserver for a crate which is a library!" ) )
            );
        }

        targets[ 0 ].clone()
    };

    Ok( (package, target) )
}

impl LastBuild {
    fn new( project: Project, package: CargoPackage, target: CargoTarget, counter: Counter ) -> Result< Self, Error > {
        let config = project.aggregate_configuration( &package, Profile::Main )?;
        let result = project.build( &config, &package, &target )?;
        let deployment = Deployment::new( &package, &target, &result )?;

        Ok( LastBuild {
            counter,
            deployment,
            project,
            package,
            target
        })
    }

    fn get_build_hash( &self ) -> u32 {
        self.counter.get_hash()
    }
}

fn monitor_for_changes_and_rebuild(
    last_build: Arc< Mutex< LastBuild > >
) -> Arc< Mutex< RecommendedWatcher > > {
    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new( tx, Duration::from_millis( 500 ) ).unwrap();

    let last_paths_to_watch = {
        let last_build = last_build.lock().unwrap();
        let paths_to_watch = get_paths_to_watch( &last_build.project, &last_build.package, &last_build.target );
        debug!( "Found paths to watch: {:#?}", paths_to_watch );

        for &(ref path, ref mode) in &paths_to_watch {
            watcher.watch( &path, (*mode).into() ).unwrap()
        }

        paths_to_watch.clone()
    };

    let watcher = Arc::new( Mutex::new( watcher ) );
    let weak_watcher = Arc::downgrade( &watcher );

    thread::spawn( move || {
        let rx = rx;
        let mut last_paths_to_watch = last_paths_to_watch;

        while let Ok( event ) = rx.recv() {
            match event {
                DebouncedEvent::Create( _ ) |
                DebouncedEvent::Remove( _ ) |
                DebouncedEvent::Rename( _, _ ) |
                DebouncedEvent::Write( _ ) => {},
                _ => continue
            };

            eprintln!( "==== Triggering `cargo build` ====" );
            let (counter, build_args) = {
                let last_build = last_build.lock().unwrap();
                let counter = last_build.counter.next();
                let build_args = last_build.project.build_args().clone();
                (counter, build_args)
            };

            if let Ok( project ) = build_args.load_project() {
                if let Ok( (package, target) ) = select_package_and_target( &project ) {
                    let mut new_paths_to_watch = get_paths_to_watch(
                        &project,
                        &package,
                        &target
                    );

                    if new_paths_to_watch != last_paths_to_watch {
                        debug!( "Paths to watch have changed; new paths to watch: {:#?}", new_paths_to_watch );
                        if let Some( watcher ) = weak_watcher.upgrade() {
                            let mut watcher = watcher.lock().expect( "watcher was poisoned" );
                            for (path, _) in last_paths_to_watch {
                                let _ = watcher.unwatch( path );
                            }

                            for &(ref path, ref mode) in &new_paths_to_watch {
                                watcher.watch( &path, (*mode).into() ).unwrap()
                            }
                        }
                        last_paths_to_watch = new_paths_to_watch;
                    }

                    if let Ok( new_build ) = LastBuild::new( project, package, target, counter ) {
                        *last_build.lock().unwrap() = new_build;
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

pub fn command_start< 'a >( matches: &clap::ArgMatches< 'a > ) -> Result< (), Error > {
    let auto_reload = matches.is_present( "auto-reload" );
    let build_args = BuildArgs::new( matches )?;
    let project = build_args.load_project()?;

    let last_build = {
        let (package, target) = select_package_and_target( &project )?;
        LastBuild::new( project, package, target, Counter::new() )?
    };

    let last_build = Arc::new( Mutex::new( last_build ) );
    let target = last_build.lock().unwrap().target.clone();

    let _watcher = monitor_for_changes_and_rebuild( last_build.clone() );

    let address = address_or_default( matches );
    let server = rouille::Server::new( &address, move |request| {
        let url = request.url();
        let last_build = last_build.lock().unwrap();

        if url == "/__cargo-web__/build_hash" {
            let data = format!( "{}", last_build.get_build_hash() );
            return rouille::Response::from_data( "application/text", data ).with_no_cache();
        }

        debug!( "Received a request for {:?}", url );
        if let Some( mut artifact ) = last_build.deployment.get_by_url( &url ) {
            if auto_reload && (url == "/" || url == "/index.html") {
                let result = artifact.map_text( |text| {
                    let injected_code = auto_reload_code( last_build.get_build_hash() );
                    text.replace( "<head>", &format!( "<head><script>{}</script>", injected_code ) )
                });
                artifact = match result {
                    Ok( artifact ) => artifact,
                    Err( error ) => {
                        warn!( "Cannot read {:?}: {:?}", url, error );
                        return rouille::Response::text( "Internal Server Error" ).with_status_code( 500 ).with_no_cache();
                    }
                }
            }

            match artifact.kind {
                ArtifactKind::Data( mut data ) => {
                    rouille::Response::from_data( artifact.mime_type, data ).with_no_cache()
                },
                ArtifactKind::File( fp ) => {
                    rouille::Response::from_file( artifact.mime_type, fp ).with_no_cache()
                }
            }
        } else {
            rouille::Response::empty_404().with_no_cache()
        }
    }).unwrap();

    eprintln!( "" );
    eprintln!( "If you need to serve any extra files put them in the 'static' directory" );
    eprintln!( "in the root of your crate; they will be served alongside your application." );
    match target.kind {
        TargetKind::Example => eprintln!( "You can also put a '{}-static' directory in your 'examples' directory.", target.name ),
        TargetKind::Bin => eprintln!( "You can also put a 'static' directory in your 'src' directory." ),
        _ => unreachable!()
    };
    eprintln!( "" );
    eprintln!( "Your application is being served at '/js/app.js'. It will be automatically" );
    eprintln!( "rebuilt if you make any changes in your code." );
    eprintln!( "" );
    eprintln!( "You can access the web server at `http://{}`.", &address );

    server.run();

    Ok(())
}
