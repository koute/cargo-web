use std::collections::BTreeMap;
use std::process::{Command, Stdio, exit};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Mutex, Arc};
use std::time::Duration;
use std::thread;
use std::time::Instant;
use std::iter;
use std::env;
use std::io::Read;
use std::ffi::OsStr;

use clap;
use rouille;
use tempdir::TempDir;
use handlebars::Handlebars;

use cargo_shim::{
    Profile,
    CargoProject,
    CargoResult,
    TargetKind
};

use build::BuildArgsMatcher;
use config::Config;
use error::Error;
use utils::{
    CommandExt,
    read,
    read_bytes,
    check_if_command_exists
};

const DEFAULT_TEST_INDEX_HTML: &'static str = r#"
<!DOCTYPE html>
<head>
    <meta charset="utf-8" />
    <meta http-equiv="X-UA-Compatible" content="IE=edge" />
    <meta content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=1" name="viewport" />
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
        Module['arguments'] = [{{#each arguments}} "{{{ this }}}", {{/each}}];
    </script>
</head>
<body>
    <script src="js/app.js"></script>
</body>
</html>
"#;

fn test_in_nodejs(
    build_matcher: &BuildArgsMatcher,
    build: CargoResult,
    arg_passthrough: &Vec< &OsStr >,
    any_failure: &mut bool
) -> Result< (), Error > {

    let nodejs_name =
        if cfg!( windows ) && check_if_command_exists( "node.exe", None ) {
            "node.exe"
        } else if check_if_command_exists( "nodejs", None ) {
            "nodejs"
        } else if check_if_command_exists( "node", None ) {
            "node"
        } else {
            return Err( Error::EnvironmentError( "node.js not found; please install it!".into() ) );
        };

    let artifact = build.artifacts().iter()
        .find( |artifact| artifact.extension().map( |ext| ext == "js" ).unwrap_or( false ) )
        .expect( "internal error: no .js file found" );

    let test_args = iter::once( artifact.as_os_str() )
        .chain( arg_passthrough.iter().cloned() );

    let previous_cwd = env::current_dir().unwrap();
    if build_matcher.targeting_emscripten_wasm() {
        // On the Emscripten target the `.wasm` file is in a different directory.
        let wasm_artifact = build.artifacts().iter()
            .find( |artifact| artifact.extension().map( |ext| ext == "wasm" ).unwrap_or( false ) )
            .expect( "internal error: no .wasm file found" );

        env::set_current_dir( wasm_artifact.parent().unwrap() ).unwrap();
    } else {
        env::set_current_dir( artifact.parent().unwrap() ).unwrap();
    }

    let status = Command::new( nodejs_name ).args( test_args ).run();
    *any_failure = *any_failure || !status.is_ok();

    env::set_current_dir( previous_cwd ).unwrap();

    Ok(())
}

fn test_in_chromium(
    build_matcher: &BuildArgsMatcher,
    build: CargoResult,
    arg_passthrough: &Vec< &OsStr >,
    any_failure: &mut bool
) -> Result< (), Error > {

    let chromium_executable = if cfg!( windows ) && check_if_command_exists( "chrome.exe", None ) {
        "chrome.exe"
    } else if check_if_command_exists( "chromium", None ) {
        "chromium"
    } else if check_if_command_exists( "google-chrome", None ) {
        "google-chrome"
    } else if check_if_command_exists( "google-chrome-stable", None ) {
        "google-chrome-stable"
    } else {
        return Err( Error::EnvironmentError( "you need to have either Chromium or Chrome installed and in your PATH to run the tests!".into() ) );
    };

    let app_js = Arc::new( Mutex::new( String::new() ) );
    let (tx, rx) = channel();
    let server_app_js = app_js.clone();
    let handlebars = Handlebars::new();
    let mut template_data = BTreeMap::new();
    let arg_passthrough: Vec<_> = arg_passthrough.iter().map( |arg| arg.to_str().unwrap() ).collect();
    template_data.insert( "arguments", arg_passthrough );
    let test_index = handlebars.template_render( DEFAULT_TEST_INDEX_HTML, &template_data ).unwrap();
    let app_wasm: Arc< Mutex< Option< Vec< u8 > > > > = Arc::new( Mutex::new( None ) );
    let wasm_url = Arc::new( Mutex::new( None ) );

    let server_app_wasm = app_wasm.clone();
    let server_wasm_url = wasm_url.clone();

    let tx = Mutex::new( tx ); // Since rouille requires the Sync trait.
    let server = rouille::Server::new( "localhost:0", move |request| {
        let url = request.url();
        let response = if url == "/" || url == "index.html" {
            rouille::Response::html( test_index.clone() )
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
            match *server_wasm_url.lock().unwrap() {
                Some( ref wasm_url ) if url == *wasm_url => {
                    let data = server_app_wasm.lock().unwrap().as_ref().unwrap().clone();
                    rouille::Response::from_data( "application/wasm", data )
                },
                _ => rouille::Response::empty_404()
            }
        };

        response.with_no_cache()
    }).unwrap();

    let server_address = server.server_addr();
    thread::spawn( move || {
        server.run();
    });

    let artifact = build.artifacts().iter()
        .find( |artifact| artifact.extension().map( |ext| ext == "js" ).unwrap_or( false ) )
        .expect( "internal error: no .js file found" );

    if build_matcher.targeting_wasm() {
        let wasm_artifact = build.artifacts().iter()
            .find( |artifact| artifact.extension().map( |ext| ext == "wasm" ).unwrap_or( false ) )
            .expect( "internal error: no .wasm file found" );

        *wasm_url.lock().unwrap() = Some( format!( "/{}", wasm_artifact.file_name().unwrap().to_str().unwrap() ) );
        *app_wasm.lock().unwrap() = Some( read_bytes( wasm_artifact ).unwrap() );
    }

    *app_js.lock().unwrap() = read( artifact ).unwrap();

    let tmpdir = TempDir::new( "cargo-web-chromium-profile" ).unwrap();
    let tmpdir = tmpdir.path().to_string_lossy();
    let mut command = Command::new( chromium_executable );
    command
        // TODO: Switch to headless.
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
                    *any_failure = true;
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
        *any_failure = true;
    }

    child.kill().unwrap();
    child.wait().unwrap();

    Ok(())
}

pub fn command_test< 'a >( matches: &clap::ArgMatches< 'a >, project: &CargoProject ) -> Result< (), Error > {
    let build_matcher = BuildArgsMatcher {
        matches: matches,
        project: project
    };

    let use_nodejs = matches.is_present( "nodejs" );
    let no_run = matches.is_present( "no-run" );
    if build_matcher.targeting_native_wasm() && !use_nodejs {
        return Err( Error::ConfigurationError( "running tests for the native wasm target is currently only supported with `--nodejs`".into() ) );
    }

    let arg_passthrough = matches.values_of_os( "passthrough" )
        .map_or( vec![], |args| args.collect() );

    let package = build_matcher.package_or_default()?;
    let config = Config::load_for_package_printing_warnings( &package ).unwrap().unwrap_or_default();
    let targets = build_matcher.target_or_select( package, |target| {
        target.kind == TargetKind::Lib || target.kind == TargetKind::Bin || target.kind == TargetKind::Test
    })?;

    let mut builds = Vec::new();
    for target in targets {
        let builder = build_matcher.prepare_builder( &config, package, target, Profile::Test );
        builds.push( builder.run()? );
    }

    if no_run {
        exit( 0 );
    }

    let mut any_failure = false;
    if use_nodejs {
        for build in builds {
            test_in_nodejs( &build_matcher, build, &arg_passthrough, &mut any_failure )?;
        }
    } else {
        for build in builds {
            test_in_chromium( &build_matcher, build, &arg_passthrough, &mut any_failure )?;
        }
    }

    if any_failure {
        exit( 101 );
    } else {
        if build_matcher.targeting_native_wasm() {
            println_err!( "All tests passed!" );
            // At least **I hope** that's the case; there are no prints
            // when running those tests, so who knows what happens. *shrug*
        }
    }

    Ok(())
}
