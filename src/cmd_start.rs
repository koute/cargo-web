use std::collections::BTreeMap;
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::{Mutex, Arc};
use std::time::{Instant, Duration};
use std::thread;
use std::net;
use std::hash::Hash;
use std::time::{SystemTime, UNIX_EPOCH};
use std::path::{Path, PathBuf};

use notify::{
    RecommendedWatcher,
    Watcher,
    RecursiveMode,
    DebouncedEvent
};

use handlebars::Handlebars;
use percent_encoding::percent_decode;

use hyper::StatusCode;

use cargo_shim::{
    Profile,
    TargetKind,
    CargoTarget
};

use build::{BuildArgs, Project, PathKind, ShouldTriggerRebuild};
use http_utils::{
    SimpleServer,
    response_from_data,
    response_from_status,
    response_from_file
};

use deployment::{Deployment, ArtifactKind};
use error::Error;

fn auto_reload_code( hash: u32 ) -> String {
    // TODO: We probably should do this with with Websockets.
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
    handlebars.render_template( TEMPLATE, &template_data ).unwrap()
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

impl From< PathKind > for RecursiveMode {
    fn from( mode: PathKind ) -> Self {
        match mode {
            PathKind::File => RecursiveMode::NonRecursive,
            PathKind::Directory => RecursiveMode::Recursive
        }
    }
}

struct LastBuild {
    counter: Counter,
    deployment: Deployment,
    project: Project,
    target: CargoTarget
}

fn select_target( project: &Project ) -> Result< CargoTarget, Error > {
    let target = {
        let targets = project.target_or_select( |target| {
            target.kind == TargetKind::Bin ||
            (target.kind == TargetKind::CDyLib && project.backend().is_native_wasm())
        })?;

        if targets.is_empty() {
            return Err(
                Error::ConfigurationError( format!( "cannot start a webserver for a crate which is a library!" ) )
            );
        }

        targets[ 0 ].clone()
    };

    Ok( target )
}

impl LastBuild {
    fn new( project: Project, target: CargoTarget, counter: Counter ) -> Result< Self, Error > {
        let config = project.aggregate_configuration( Profile::Main )?;
        let result = project.build( &config, &target )?;
        let deployment = Deployment::new( project.package(), &target, &result )?;

        Ok( LastBuild {
            counter,
            deployment,
            project,
            target
        })
    }

    fn get_build_hash( &self ) -> u32 {
        self.counter.get_hash()
    }
}

fn should_rebuild( paths_to_watch: &[(PathBuf, PathKind, ShouldTriggerRebuild)], path: &Path ) -> bool {
    paths_to_watch.iter()
        .filter( |&&(_, _, should_rebuild)| should_rebuild == ShouldTriggerRebuild::Yes )
        .any( |&(ref root, _, _)| path.starts_with( root ) )
}

fn watch_paths( watcher: &mut RecommendedWatcher, paths_to_watch: &[(PathBuf, PathKind, ShouldTriggerRebuild)] ) {
    for &(ref path, ref mode, _) in paths_to_watch {
        // TODO: Handle paths that currently don't exist which *will* be created.
        match watcher.watch( &path, (*mode).into() ) {
            Ok( _ ) => trace!( "Watching {:?} ({:?})", path, mode ),
            Err( error ) => trace!( "Failed to watch {:?} ({:?}): {:?}", path, mode, error )
        }
    }
}

fn monitor_for_changes_and_rebuild(
    last_build: Arc< Mutex< LastBuild > >
) -> Arc< Mutex< RecommendedWatcher > > {
    let event_timeout = Duration::from_millis( 500 );
    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new( tx, event_timeout ).unwrap();

    let last_paths_to_watch = {
        let last_build = last_build.lock().unwrap();
        let paths_to_watch = last_build.project.paths_to_watch( &last_build.target );
        debug!( "Found paths to watch: {:#?}", paths_to_watch );

        watch_paths( &mut watcher, &paths_to_watch );
        paths_to_watch.clone()
    };

    let watcher = Arc::new( Mutex::new( watcher ) );
    let weak_watcher = Arc::downgrade( &watcher );

    thread::spawn( move || {
        let rx = rx;
        let mut last_paths_to_watch = last_paths_to_watch;

        fn event_triggers_rebuild( event: DebouncedEvent, paths_to_watch: &Vec< ( PathBuf, PathKind, ShouldTriggerRebuild ) > ) -> bool {
            match event {
                DebouncedEvent::Create( ref path ) => should_rebuild( paths_to_watch, path ),
                DebouncedEvent::Remove( ref path ) => should_rebuild( paths_to_watch, path ),
                DebouncedEvent::Rename( ref old_path, ref new_path ) => {
                    should_rebuild( paths_to_watch, old_path ) ||
                    should_rebuild( paths_to_watch, new_path )
                },
                DebouncedEvent::Write( ref path ) => should_rebuild( paths_to_watch, path ),
                _ => false
            }
        }

        fn record_inconsequential_change(last_build: &Arc< Mutex< LastBuild > >) {
            trace!( "Nothing of consequence changed; bumping build counter without rebuilding" );
            let mut last_build = last_build.lock().unwrap();
            let counter = last_build.counter.next();
            last_build.counter = counter;
        }

        'outer: while let Ok( event ) = rx.recv() {
            trace!( "Watch event: {:?}", event );
            if !event_triggers_rebuild( event, &last_paths_to_watch ) {
                record_inconsequential_change( &last_build );
                continue;
            }

            trace!( "Starting build in {}.{:0>3}s if no more changes detected", event_timeout.as_secs(), event_timeout.subsec_nanos() / 1_000_000 );
            let mut deadline = Instant::now() + event_timeout;
            while Instant::now() < deadline {
                match rx.recv_timeout( deadline - Instant::now() ) {
                    Ok( event ) => {
                        trace!( "Watch event: {:?}", event );
                        if !event_triggers_rebuild( event, &last_paths_to_watch ) {
                            record_inconsequential_change( &last_build );
                            continue;
                        }

                        trace!( "Noticed follow-up change; waiting additional {}.{:0>3}s for more", event_timeout.as_secs(), event_timeout.subsec_nanos() / 1_000_000 );
                        deadline = Instant::now() + event_timeout;
                    }
                    Err( RecvTimeoutError::Timeout ) => {
                        trace!( "Timed out waiting for follow-up changes; proceeding to build" );
                        break;
                    }
                    Err( RecvTimeoutError::Disconnected ) => {
                        break 'outer;
                    }
                }
            }

            eprintln!( "==== Triggering `cargo build` ====" );
            let (counter, build_args) = {
                let last_build = last_build.lock().unwrap();
                let counter = last_build.counter.next();
                let build_args = last_build.project.build_args().clone();
                (counter, build_args)
            };

            if let Ok( project ) = build_args.load_project() {
                if let Ok( target ) = select_target( &project ) {
                    let new_paths_to_watch = project.paths_to_watch( &target );

                    if new_paths_to_watch != last_paths_to_watch {
                        debug!( "Paths to watch have changed; new paths to watch: {:#?}", new_paths_to_watch );
                        if let Some( watcher ) = weak_watcher.upgrade() {
                            let mut watcher = watcher.lock().expect( "watcher was poisoned" );
                            for (path, _, _) in last_paths_to_watch {
                                let _ = watcher.unwatch( path );
                            }

                            watch_paths( &mut watcher, &new_paths_to_watch );
                        }
                        last_paths_to_watch = new_paths_to_watch;
                    }

                    if let Ok( new_build ) = LastBuild::new( project, target, counter ) {
                        *last_build.lock().unwrap() = new_build;
                    }
                }
            }
        }
    });

    watcher
}

pub fn command_start(
    build_args: BuildArgs,
    host: net::IpAddr,
    port: u16,
    open: bool,
    auto_reload: bool
) -> Result<(), Error> {
    let project = build_args.load_project()?;

    let last_build = {
        let target = select_target( &project )?;
        LastBuild::new( project, target, Counter::new() )?
    };

    let last_build = Arc::new( Mutex::new( last_build ) );
    let (target, js_url) = {
        let last_build = last_build.lock().unwrap();
        let target = last_build.target.clone();
        let js_url = last_build.deployment.js_url().to_owned();
        (target, js_url)
    };

    let _watcher = monitor_for_changes_and_rebuild( last_build.clone() );

    let target_name = target.name.clone();
    let address = net::SocketAddr::new(host, port);
    let server = SimpleServer::new(&address, move |request| {
        let path = request.uri().path();
        let path = percent_decode( path.as_bytes() ).decode_utf8().unwrap();
        let last_build = last_build.lock().unwrap();

        if path == "/__cargo-web__/build_hash" {
            let data = format!( "{}", last_build.get_build_hash() );
            return response_from_data(&"application/text".parse().unwrap(), data.into_bytes());
        }

        if path == "/js/app.js" {
            eprintln!( "!!!!!!!!!!!!!!!!!!!!!" );
            eprintln!( "WARNING: `/js/app.js` is deprecated; you should update your HTML file to use `/{}.js` instead!", target_name );
            eprintln!( "!!!!!!!!!!!!!!!!!!!!!" );
        }

        debug!( "Received a request for {:?}", path );
        if let Some( mut artifact ) = last_build.deployment.get_by_url(&path) {
            if auto_reload && (path == "/" || path == "/index.html") {
                let result = artifact.map_text( |text| {
                    let injected_code = auto_reload_code( last_build.get_build_hash() );
                    text.replace( "<head>", &format!( "<head><script>{}</script>", injected_code ) )
                });
                artifact = match result {
                    Ok( artifact ) => artifact,
                    Err( error ) => {
                        warn!( "Cannot read {:?}: {:?}", path, error );
                        return response_from_status(StatusCode::INTERNAL_SERVER_ERROR);
                    }
                }
            }

            match artifact.kind {
                ArtifactKind::Data( data ) => {
                    return response_from_data(&artifact.mime_type, data);
                },

                ArtifactKind::File( fp ) => {
                    return response_from_file(&artifact.mime_type, fp);
                }
            }
        } else {
            response_from_status(StatusCode::NOT_FOUND)
        }
    });

    eprintln!( "" );
    eprintln!( "If you need to serve any extra files put them in the 'static' directory" );
    eprintln!( "in the root of your crate; they will be served alongside your application." );
    match target.kind {
        TargetKind::Example => eprintln!( "You can also put a '{}-static' directory in your 'examples' directory.", target.name ),
        TargetKind::Bin | TargetKind::CDyLib => eprintln!( "You can also put a 'static' directory in your 'src' directory." ),
        _ => unreachable!()
    };
    eprintln!( "" );
    eprintln!( "Your application is being served at '/{}'. It will be automatically", js_url );
    eprintln!( "rebuilt if you make any changes in your code." );
    eprintln!( "" );
    eprintln!( "You can access the web server at `http://{}`.", &address );

    if open {
        thread::spawn( move || {
            // Wait for server to start
            let start = Instant::now();
            let check_url = format!( "http://{}/__cargo-web__/build_hash", &address );
            let mut response = None;
            while start.elapsed() < Duration::from_secs( 10 ) && response.is_none() {
                thread::sleep( Duration::from_millis( 100 ) );
                response = ::reqwest::get( &check_url ).ok();
            }

            ::open::that( &format!( "http://{}", &address ) ).expect( "Failed to open browser" );
        });
    }

    server.run();

    Ok(())
}
