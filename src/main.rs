#![deny(
    missing_debug_implementations,
    trivial_numeric_casts,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

extern crate clap;
extern crate notify;
extern crate rouille;
extern crate tempdir;
extern crate cargo_shim;

use std::process::{Command, Stdio, exit};
use std::path::Path;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Mutex, Arc};
use std::time::Duration;
use std::thread;
use std::io::{Read, Write, stderr};
use std::fs;
use std::time::Instant;
use std::error;
use std::fmt;

use notify::{
    RecommendedWatcher,
    Watcher,
    RecursiveMode,
    DebouncedEvent
};

use clap::{
    Arg,
    App,
    AppSettings,
    SubCommand
};

use tempdir::TempDir;

use cargo_shim::*;

mod utils;
use utils::*;

macro_rules! println_err(
    ($($arg:tt)*) => { {
        writeln!( &mut stderr(), $($arg)* ).expect( "writeln to stderr failed" );
    }}
);

const DEFAULT_INDEX_HTML: &'static str = "
<!DOCTYPE html>
<head>
    <meta charset=\"utf-8\" />
    <meta http-equiv=\"X-UA-Compatible\" content=\"IE=edge\" />
    <meta content=\"width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=1\" name=\"viewport\" />
</head>
<body>
    <script src=\"js/app.js\"></script>
</body>
</html>
";

const DEFAULT_TEST_INDEX_HTML: &'static str = "
<!DOCTYPE html>
<head>
    <meta charset=\"utf-8\" />
    <meta http-equiv=\"X-UA-Compatible\" content=\"IE=edge\" />
    <meta content=\"width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=1\" name=\"viewport\" />
    <script>
        var __cargo_web = {};
        __cargo_web.print_counter = 0;
        __cargo_web.xhr_queue = [];
        __cargo_web.xhr_in_progress = 0;
        __cargo_web.flush_xhr = function() {
            if( __cargo_web.xhr_queue.length === 0 ) {
                return;
            }
            var next_callback = __cargo_web.xhr_queue.shift();
            next_callback();
        };
        __cargo_web.send_xhr = function( url, data ) {
            var callback = function() {
                __cargo_web.xhr_in_progress++;
                var xhr = new XMLHttpRequest();
                xhr.open( 'PUT', url );
                xhr.setRequestHeader( 'Content-Type', 'text/plain' );
                xhr.onload = function() {
                    __cargo_web.xhr_in_progress--;
                    __cargo_web.flush_xhr();
                };
                xhr.send( data );
            };
            __cargo_web.xhr_queue.push( callback );
            if( __cargo_web.xhr_in_progress === 0 ) {
                __cargo_web.flush_xhr();
            }
        };
        __cargo_web.print = function( message ) {
            __cargo_web.print_counter++;
            if( (__cargo_web.print_counter === 1 && /pre-main prep time/.test( message )) ||
                (__cargo_web.print_counter === 2 && message === '') ) {
                return;
            }

            __cargo_web.send_xhr( '/__cargo_web/print', message );
        };
        __cargo_web.on_exit = function( status ) {
            __cargo_web.send_xhr( '/__cargo_web/exit', status );
        };
        var Module = {};
        Module['print'] = __cargo_web.print;
        Module['printErr'] = __cargo_web.print;
        Module['onExit'] = __cargo_web.on_exit;
    </script>
</head>
<body>
    <script src=\"js/app.js\"></script>
</body>
</html>
";

fn monitor_for_changes_and_rebuild(
    package: &CargoPackage,
    target: &CargoTarget,
    output_path: &Path,
    build: BuildConfig,
    output: Arc< Mutex< String > >
) -> RecommendedWatcher {
    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new( tx, Duration::from_millis( 500 ) ).unwrap();

    // TODO: Support local dependencies.
    // TODO: Support Cargo.toml reloading.
    watcher.watch( &target.source_directory, RecursiveMode::Recursive ).unwrap();
    watcher.watch( &package.manifest_path, RecursiveMode::NonRecursive ).unwrap();

    let output_path = output_path.to_owned();
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
            if build.as_command().run().is_ok() {
                if let Ok( data ) = read( &output_path ) {
                    *output.lock().unwrap() = data;
                }
            }
        }
    });

    watcher
}

fn check_if_command_exists( command: &str, extra_path: Option< &str > ) -> bool {
    let mut command = Command::new( command );
    command.arg( "--version" );
    if let Some( extra_path ) = extra_path {
        command.append_to_path( extra_path );
    }

    command
        .stdout( Stdio::null() )
        .stderr( Stdio::null() )
        .stdin( Stdio::null() );

    return command.spawn().is_ok()
}

fn check_for_emcc() -> Option< &'static str > {
    if check_if_command_exists( "emcc", None ) {
        return None;
    }

    if Path::new( "/usr/lib/emscripten/emcc" ).exists() {
        if check_if_command_exists( "emcc", Some( "/usr/lib/emscripten" ) ) {
            // Arch package doesn't put Emscripten anywhere in the $PATH, but
            // it's there and it works.
            return Some( "/usr/lib/emscripten" );
        }
    }

    println_err!( "error: you don't have Emscripten installed!" );
    println_err!( "" );

    if Path::new( "/usr/bin/pacman" ).exists() {
        println_err!( "You can most likely install it like this:" );
        println_err!( "  sudo pacman -S emscripten" );
    } else if Path::new( "/usr/bin/apt-get" ).exists() {
        println_err!( "You can most likely install it like this:" );
        println_err!( "  sudo apt-get install emscripten" );
    } else if cfg!( target_os = "linux" ) {
        println_err!( "You can most likely find it in your distro's repositories." );
    }

    if cfg!( unix ) {
        if cfg!( target_os = "linux" ) {
            println_err!( "If not you can install it manually like this:" );
        } else {
            println_err!( "You can install it manually like this:" );
        }
        println_err!( "  curl -O https://s3.amazonaws.com/mozilla-games/emscripten/releases/emsdk-portable.tar.gz" );
        println_err!( "  tar -xzf emsdk-portable.tar.gz" );
        println_err!( "  source emsdk_portable/emsdk_env.sh" );
        println_err!( "  emsdk update" );
        println_err!( "  emsdk install sdk-incoming-64bit" );
        println_err!( "  emsdk activate sdk-incoming-64bit" );
    }

    exit( 101 );
}

#[derive(Debug)]
enum Error {
    ConfigurationError( String ),
    EnvironmentError( String ),
    BuildError
}

impl error::Error for Error {
    fn description( &self ) -> &str {
        match *self {
            Error::ConfigurationError( ref message ) => &message,
            Error::EnvironmentError( ref message ) => &message,
            Error::BuildError => "build failed"
        }
    }
}

impl fmt::Display for Error {
    fn fmt( &self, formatter: &mut fmt::Formatter ) -> fmt::Result {
        use error::Error;
        write!( formatter, "{}", self.description() )
    }
}

struct BuildArgsMatcher< 'a > {
    matches: &'a clap::ArgMatches< 'a >,
    project: &'a CargoProject
}

impl< 'a > BuildArgsMatcher< 'a > {
    fn build_type( &self ) -> BuildType {
        if self.matches.is_present( "release" ) {
            BuildType::Release
        } else {
            BuildType::Debug
        }
    }

    fn package( &self ) -> Result< Option< &CargoPackage >, Error > {
        if let Some( name ) = self.matches.value_of( "package" ) {
            match self.project.packages.iter().find( |package| package.name == name ) {
                None => Err( Error::ConfigurationError( format!( "package `{}` not found", name ) ) ),
                package => Ok( package )
            }
        } else {
            Ok( None )
        }
    }

    fn package_or_default( &self ) -> Result< &CargoPackage, Error > {
        Ok( self.package()?.unwrap_or_else( || self.project.default_package() ) )
    }

    fn target( &'a self, package: &'a CargoPackage ) -> Result< Option< &'a CargoTarget >, Error > {
        let targets = &package.targets;
        if self.matches.is_present( "lib" ) {
            match targets.iter().find( |target| target.kind == TargetKind::Lib ) {
                None => return Err( Error::ConfigurationError( format!( "no library targets found" ) ) ),
                target => Ok( target )
            }
        } else if let Some( name ) = self.matches.value_of( "bin" ) {
            match targets.iter().find( |target| target.kind == TargetKind::Bin && target.name == name ) {
                None => return Err( Error::ConfigurationError( format!( "no bin target named `{}`", name ) ) ),
                target => Ok( target )
            }
        } else if let Some( name ) = self.matches.value_of( "example" ) {
            match targets.iter().find( |target| target.kind == TargetKind::Example && target.name == name ) {
                None => return Err( Error::ConfigurationError( format!( "no example target named `{}`", name ) ) ),
                target => Ok( target )
            }
        } else if let Some( name ) = self.matches.value_of( "bench" ) {
            match targets.iter().find( |target| target.kind == TargetKind::Bench && target.name == name ) {
                None => return Err( Error::ConfigurationError( format!( "no bench target named `{}`", name ) ) ),
                target => Ok( target )
            }
        } else {
            Ok( None )
        }
    }

    fn target_or_select< F >( &'a self, package: &'a CargoPackage, filter: F ) -> Result< Vec< &'a CargoTarget >, Error >
        where for< 'r > F: Fn( &'r CargoTarget ) -> bool
    {
        Ok( self.target( package )?.map( |target| vec![ target ] ).unwrap_or_else( || {
            package.targets.iter().filter( |target| filter( target ) ).collect()
        }))
    }

    fn triplet_or_default( &self ) -> &str {
        "asmjs-unknown-emscripten"
    }

    fn build_config( &self, package: &CargoPackage, target: &CargoTarget, profile: Profile ) -> BuildConfig {
        BuildConfig {
            build_target: target_to_build_target( target, profile ),
            build_type: self.build_type(),
            triplet: Some( self.triplet_or_default().into() ),
            package: Some( package.name.clone() )
        }
    }
}

fn command_build< 'a >( matches: &clap::ArgMatches< 'a >, project: &CargoProject ) -> Result< (), Error > {
    let extra_path = check_for_emcc();

    let build_matcher = BuildArgsMatcher {
        matches: matches,
        project: project
    };

    let package = build_matcher.package_or_default()?;
    let targets = build_matcher.target_or_select( package, |target| {
        target.kind == TargetKind::Lib || target.kind == TargetKind::Bin
    })?;

    for target in targets {
        let build_config = build_matcher.build_config( package, target, Profile::Main );
        let mut command = build_config.as_command();
        if let Some( ref extra_path ) = extra_path {
            command.append_to_path( extra_path );
        }

        if command.run().is_ok() == false {
            return Err( Error::BuildError );
        }
    }

    Ok(())
}

fn command_test< 'a >( matches: &clap::ArgMatches< 'a >, project: &CargoProject ) -> Result< (), Error > {
    let extra_path = check_for_emcc();

    let no_run = matches.is_present( "no-run" );
    let use_nodejs = matches.is_present( "nodejs" );

    let mut chromium_executable = "";
    if !use_nodejs {
        chromium_executable = if check_if_command_exists( "chromium", None ) {
            "chromium"
        } else if check_if_command_exists( "google-chrome", None ) {
            "google-chrome"
        } else {
            return Err( Error::EnvironmentError( "you need to have either Chromium or Chrome installed to run the tests!".into() ) );
        }
    }

    let build_matcher = BuildArgsMatcher {
        matches: matches,
        project: project
    };

    let package = build_matcher.package_or_default()?;
    let targets = build_matcher.target_or_select( package, |target| {
        target.kind == TargetKind::Lib || target.kind == TargetKind::Bin || target.kind == TargetKind::Test
    })?;

    let builds: Vec< _ > = targets.iter().map( |target| {
        let build_config = build_matcher.build_config( package, target, Profile::Test );
        let artifacts: Vec< _ > = build_config.potential_artifacts( &package.crate_root ).into_iter().map( |artifact| {
            let modified = fs::metadata( &artifact ).unwrap().modified().unwrap();
            (artifact, modified)
        }).collect();

        (build_config, artifacts)
    }).collect();

    let mut post_artifacts_per_build = Vec::new();
    for &(ref build_config, ref pre_artifacts) in &builds {
        let mut command = build_config.as_command();
        if let Some( ref extra_path ) = extra_path {
            command.append_to_path( extra_path );
        }

        if command.run().is_ok() == false {
            return Err( Error::BuildError );
        }

        let mut post_artifacts = build_config.potential_artifacts( &package.crate_root );
        let artifact =
        if post_artifacts.len() == 1 {
            post_artifacts.pop().unwrap()
        } else if post_artifacts.is_empty() {
            panic!( "internal error: post_artifacts are empty; please report this!" );
        } else {
            let mut new_artifacts = Vec::new();
            let mut modified_artifacts = Vec::new();

            for post_artifact in post_artifacts {
                if let Some( &(_, pre_modified) ) = pre_artifacts.iter().find( |&&(ref pre_artifact, _)| *pre_artifact == post_artifact ) {
                    let post_modified = fs::metadata( &post_artifact ).unwrap().modified().unwrap();
                    if post_modified > pre_modified {
                        modified_artifacts.push( post_artifact );
                    }
                } else {
                    new_artifacts.push( post_artifact );
                }
            }

            if new_artifacts.len() == 1 {
                new_artifacts.pop().unwrap()
            } else if new_artifacts.len() > 1 {
                panic!( "internal error: new_artifacts have {} elements; please report this!", new_artifacts.len() );
            } else if modified_artifacts.len() == 1 {
                modified_artifacts.pop().unwrap()
            } else if modified_artifacts.len() > 1 {
                panic!( "internal error: modified_artifacts have {} elements; please report this!", new_artifacts.len() );
            } else {
                unimplemented!();
            }
        };

        post_artifacts_per_build.push( artifact );
    }

    if no_run {
        exit( 0 );
    }

    let mut any_failure = false;
    if use_nodejs {
        for artifact in &post_artifacts_per_build {
            let status = Command::new( "node" ).arg( artifact ).run();
            any_failure = any_failure || !status.is_ok();
        }
    } else {
        let app_js = Arc::new( Mutex::new( String::new() ) );
        let (tx, rx) = channel();
        let server_app_js = app_js.clone();
        let tx = Mutex::new( tx ); // Since rouille requires the Sync trait.
        let server = rouille::Server::new( "localhost:0", move |request| {
            let url = request.url();
            let response = if url == "/" || url == "index.html" {
                rouille::Response::html( DEFAULT_TEST_INDEX_HTML )
            } else if url == "/js/app.js" {
                let data = server_app_js.lock().unwrap().clone();
                rouille::Response::from_data( "application/javascript", data )
            } else if url == "/__cargo_web/print" {
                let mut data = String::new();
                request.data().unwrap().read_to_string( &mut data ).unwrap();
                println!( "{}", data );
                rouille::Response::text( "" )
            } else if url == "/__cargo_web/exit" {
                let mut status = String::new();
                request.data().unwrap().read_to_string( &mut status ).unwrap();

                let status: u32 = status.parse().unwrap();
                tx.lock().unwrap().send( status ).unwrap();
                rouille::Response::text( "" )
            } else {
                rouille::Response::empty_404()
            };

            response.with_no_cache()
        }).unwrap();

        let server_address = server.server_addr();
        thread::spawn( move || {
            server.run();
        });

        for artifact in post_artifacts_per_build {
            *app_js.lock().unwrap() = read( artifact ).unwrap();

            let tmpdir = TempDir::new( "cargo-web-chromium-profile" ).unwrap();
            let tmpdir = tmpdir.path().to_string_lossy();
            let mut command = Command::new( chromium_executable );
            command
                // https://chromium.googlesource.com/chromium/src/+/master/headless/README.md
                // This doesn't work on my machine though. Maybe my Chromium was compiled
                // without it or it isn't yet merged?
                .arg( "--headless" )
                .arg( format!( "--app=http://localhost:{}", server_address.port() ) )
                .arg( "--disable-gpu" )
                .arg( "--no-first-run" )
                .arg( "--disable-restore-session-state" )
                .arg( "--no-default-browser-check" )
                .arg( "--disable-java" )
                .arg( "--disable-client-side-phishing-detection" )
                .arg( format!( "--user-data-dir={}", tmpdir ) );

            command
                .stdout( Stdio::null() )
                .stderr( Stdio::null() )
                .stdin( Stdio::null() );

            let mut child = command.spawn().unwrap();
            let start_time = Instant::now();
            let mut finished = false;
            while start_time.elapsed().as_secs() < 60 {
                match rx.recv_timeout( Duration::from_secs( 1 ) ) {
                    Ok( status ) => {
                        if status != 0 {
                            println_err!( "error: process exited with a status of {}", status );
                            any_failure = true;
                        }
                        finished = true;
                        break;
                    },
                    Err( RecvTimeoutError::Timeout ) => {
                        continue;
                    },
                    Err( RecvTimeoutError::Disconnected ) => unreachable!()
                }
            }
            if !finished {
                println_err!( "error: tests timed out!" );
                any_failure = true;
            }

            child.kill().unwrap();
            child.wait().unwrap();
        }
    }

    if any_failure {
        exit( 101 );
    }

    Ok(())
}

fn command_start< 'a >( matches: &clap::ArgMatches< 'a >, project: &CargoProject ) -> Result< (), Error > {
    let extra_path = check_for_emcc();

    let build_matcher = BuildArgsMatcher {
        matches: matches,
        project: project
    };

    let package = build_matcher.package_or_default()?;
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

    if command.run().is_ok() == false {
        return Err( Error::BuildError );
    }

    let artifacts = build_config.potential_artifacts( &package.crate_root );
    let output_path = &artifacts[ 0 ];
    let app_js = read( output_path ).unwrap();
    let app_js = Arc::new( Mutex::new( app_js ) );

    #[allow(unused_variables)]
    let watcher = monitor_for_changes_and_rebuild( &package, &target, output_path, build_config, app_js.clone() );

    let crate_static_path = package.crate_root.join( "static" );
    let target_static_path = match target.kind {
        TargetKind::Example => Some( target.source_directory.join( format!( "{}-static", target.name ) ) ),
        TargetKind::Bin => Some( target.source_directory.join( "static" ) ),
        _ => None
    };

    let server = rouille::Server::new( "localhost:8000", move |request| {
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
            let data = app_js.lock().unwrap().clone();
            rouille::Response::from_data( "application/javascript", data )
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
    println_err!( "You can access the web server at `http://localhost:8000`." );

    server.run();
    Ok(())
}

fn main() {
    let matches = App::new( "cargo-web" )
        .version( env!( "CARGO_PKG_VERSION" ) )
        .setting( AppSettings::SubcommandRequiredElseHelp )
        .setting( AppSettings::VersionlessSubcommands )
        .subcommand(
            SubCommand::with_name( "build" )
                .about( "Compile a local package and all of its dependencies" )
                .arg(
                    Arg::with_name( "release" )
                        .long( "release" )
                        .help( "Build artifacts in release mode, with optimizations" )
                )
                .arg(
                    Arg::with_name( "package" )
                        .short( "p" )
                        .long( "package" )
                        .help( "Package to build" )
                        .value_name( "NAME" )
                        .takes_value( true )
                )
                .arg(
                    Arg::with_name( "lib" )
                        .long( "lib" )
                        .help( "Build only this package's library" )
                )
                .arg(
                    Arg::with_name( "bin" )
                        .long( "bin" )
                        .help( "Build only the specified binary" )
                        .value_name( "NAME" )
                        .takes_value( true )
                )
                .arg(
                    Arg::with_name( "example" )
                        .long( "example" )
                        .help( "Build only the specified example" )
                        .value_name( "NAME" )
                        .takes_value( true )
                )
                .arg(
                    Arg::with_name( "test" )
                        .long( "test" )
                        .help( "Build only the specified test target" )
                        .value_name( "NAME" )
                        .takes_value( true )
                )
                .arg(
                    Arg::with_name( "bench" )
                        .long( "bench" )
                        .help( "Build only the specified benchmark target" )
                        .value_name( "NAME" )
                        .takes_value( true )
                )
        )
        .subcommand(
            SubCommand::with_name( "test" )
                .about( "Compiles and runs tests" )
                .arg(
                    Arg::with_name( "no-run" )
                        .long( "no-run" )
                        .help( "Compile, but don't run tests" )
                )
                .arg(
                    Arg::with_name( "package" )
                        .short( "p" )
                        .long( "package" )
                        .help( "Package to build" )
                        .value_name( "NAME" )
                        .takes_value( true )
                )
                .arg(
                    Arg::with_name( "release" )
                        .long( "release" )
                        .help( "Build artifacts in release mode, with optimizations" )
                )
                .arg(
                    Arg::with_name( "nodejs" )
                        .long( "nodejs" )
                        .help( "Uses Node.js to run the tests" )
                )
        )
        .subcommand(
            SubCommand::with_name( "start" )
                .about( "Runs an embedded web server serving the built project" )
                .arg(
                    Arg::with_name( "release" )
                        .long( "release" )
                        .help( "Build artifacts in release mode, with optimizations" )
                )
                .arg(
                    Arg::with_name( "package" )
                        .short( "p" )
                        .long( "package" )
                        .help( "Package to build" )
                        .value_name( "NAME" )
                        .takes_value( true )
                )
                .arg(
                    Arg::with_name( "bin" )
                        .long( "bin" )
                        .help( "Build only the specified binary" )
                        .value_name( "NAME" )
                        .takes_value( true )
                )
                .arg(
                    Arg::with_name( "example" )
                        .long( "example" )
                        .help( "Serves the specified example" )
                        .value_name( "NAME" )
                        .takes_value( true )
                )
                .arg(
                    Arg::with_name( "test" )
                        .long( "test" )
                        .help( "Build only the specified test target" )
                        .value_name( "NAME" )
                        .takes_value( true )
                )
                .arg(
                    Arg::with_name( "bench" )
                        .long( "bench" )
                        .help( "Build only the specified benchmark target" )
                        .value_name( "NAME" )
                        .takes_value( true )
                )
        )
        .get_matches();

    let project = CargoProject::new( None );
    let result = if let Some( matches ) = matches.subcommand_matches( "build" ) {
        command_build( matches, &project )
    } else if let Some( matches ) = matches.subcommand_matches( "test" ) {
        command_test( matches, &project )
    } else if let Some( matches ) = matches.subcommand_matches( "start" ) {
        command_start( matches, &project )
    } else {
        return;
    };

    match result {
        Ok( _ ) => {},
        Err( error ) => {
            println_err!( "error: {}", error );
            exit( 101 );
        }
    }
}
