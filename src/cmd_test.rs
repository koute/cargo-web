use std::collections::BTreeMap;
use std::process::{Command, Stdio, exit};
use std::path::PathBuf;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Mutex, Arc};
use std::time::Duration;
use std::thread;
use std::fs;
use std::time::Instant;
use std::iter;
use std::env;
use std::io::Read;

use clap;
use rouille;
use tempdir::TempDir;
use handlebars::Handlebars;

use cargo_shim::{
    Profile,
    CargoProject,
    TargetKind
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

pub fn command_test< 'a >( matches: &clap::ArgMatches< 'a >, project: &CargoProject ) -> Result< (), Error > {
    let use_nodejs = matches.is_present( "nodejs" );
    let use_system_emscripten = matches.is_present( "use-system-emscripten" );
    let targeting_webasm = matches.is_present( "target-webasm-emscripten" );
    let targeting_native_webasm = matches.is_present( "target-webasm" );
    let extra_path = if targeting_native_webasm {
        if !use_nodejs {
            return Err( Error::ConfigurationError( "running tests for the native wasm target is currently only supported with `--nodejs`".into() ) );
        }

        None
    } else {
        check_for_emcc( use_system_emscripten, targeting_webasm )
    };

    let no_run = matches.is_present( "no-run" );
    let arg_passthrough = matches.values_of_os( "passthrough" )
        .map_or(vec![], |args| args.collect());

    let mut chromium_executable = "";
    if !use_nodejs {
        chromium_executable = if cfg!( any(windows) ) && check_if_command_exists( "chrome.exe", None ) {
            "chrome.exe"
        } else if check_if_command_exists( "chromium", None ) {
            "chromium"
        } else if check_if_command_exists( "google-chrome", None ) {
            "google-chrome"
        } else if check_if_command_exists( "google-chrome-stable", None ) {
            "google-chrome-stable"
        } else {
            return Err( Error::EnvironmentError( "you need to have either Chromium or Chrome installed and in your PATH to run the tests!".into() ) );
        }
    }

    let build_matcher = BuildArgsMatcher {
        matches: matches,
        project: project
    };

    let package = build_matcher.package_or_default()?;
    let config = Config::load_for_package_printing_warnings( &package ).unwrap().unwrap_or_default();
    set_link_args( &config );

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

        run_with_broken_first_build_hack( package, &build_config, &mut command )?;

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

            fn is_js( artifact: &PathBuf ) -> bool {
                artifact.extension().map( |ext| ext == "js" ).unwrap_or( false )
            }
            let mut new_artifacts: Vec< _ > = new_artifacts.into_iter().filter( is_js ).collect();
            let mut modified_artifacts: Vec< _ > = modified_artifacts.into_iter().filter( is_js ).collect();

            if new_artifacts.len() == 1 {
                new_artifacts.pop().unwrap()
            } else if new_artifacts.len() > 1 {
                panic!( "internal error: new_artifacts have {} elements; please report this!", new_artifacts.len() );
            } else if modified_artifacts.len() == 1 {
                modified_artifacts.pop().unwrap()
            } else if modified_artifacts.len() > 1 {
                panic!( "internal error: modified_artifacts have {} elements; please report this!", new_artifacts.len() );
            } else {
                panic!( "internal error: nothing changed so I don't know which artifact corresponds to this build; change something and try again!" );
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
            let nodejs_name =
                if cfg!( any(windows) ) && check_if_command_exists( "node.exe", None ) {
                    "node.exe"
                } else if check_if_command_exists( "nodejs", None ) {
                    "nodejs"
                } else if check_if_command_exists( "node", None ) {
                    "node"
                } else {
                    return Err( Error::EnvironmentError( "node.js not found; please install it!".into() ) );
                };

            let test_args = iter::once( artifact.as_os_str() )
               .chain( arg_passthrough.iter().cloned() );

            let previous_cwd = if targeting_webasm || targeting_native_webasm {
                // This is necessary when targeting webasm so that
                // Node.js can load the `.wasm` file.
                let previous_cwd = env::current_dir().unwrap();
                env::set_current_dir( artifact.parent().unwrap().join( "deps" ) ).unwrap();
                Some( previous_cwd )
            } else {
                None
            };

            let status = Command::new( nodejs_name ).args( test_args ).run();
            any_failure = any_failure || !status.is_ok();

            if let Some( previous_cwd ) = previous_cwd {
                env::set_current_dir( previous_cwd ).unwrap();
            }
        }
    } else {
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

        for artifact in post_artifacts_per_build {
            if targeting_webasm {
                let wasm_filename = artifact.with_extension( "wasm" ).file_name().unwrap().to_str().unwrap().to_owned();
                let wasm_path = artifact.parent().unwrap().join( "deps" ).join( &wasm_filename );
                *wasm_url.lock().unwrap() = Some( format!( "/{}", wasm_filename ) );
                *app_wasm.lock().unwrap() = Some( read_bytes( wasm_path ).unwrap() );
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
    } else {
        if targeting_native_webasm {
            println_err!( "All tests passed!" );
            // At least **I hope** that's the case; there are no prints
            // when running those tests, so who knows what happens. *shrug*
        }
    }

    Ok(())
}
