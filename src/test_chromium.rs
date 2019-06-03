use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::sync::mpsc::channel;
use std::sync::{Mutex, Arc};
use std::time::Duration;
use std::thread;
use std::time::Instant;
use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::ffi::OsStr;
use std::path::Path;

use hyper::StatusCode;
use tempfile;
use handlebars::Handlebars;
use serde_json::{self, Value};
use regex::Regex;

use cargo_shim::CargoResult;

use build::Backend;
use error::Error;
use utils::{
    read,
    read_bytes,
    find_cmd,
};
use http_utils::{
    SimpleServer,
    response_from_status,
    response_from_data
};
use chrome_devtools::{Connection, Reply, ReplyError, ConsoleApiCalledBody, ExceptionThrownBody};
use cmd_test::TEST_RUNNER;

const DEFAULT_TEST_INDEX_HTML: &'static str = r#"
<!DOCTYPE html>
<head>
    <meta charset="utf-8" />
    <meta http-equiv="X-UA-Compatible" content="IE=edge" />
    <meta content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=1" name="viewport" />
    <script>
        var __cargo_web = {};
        var Module = {};
        __cargo_web.status = new Promise( function( resolve ) { Module['onExit'] = resolve; } );
        __cargo_web.target = "{{{ target }}}";
        Module['arguments'] = [{{#each arguments}} "{{{ this }}}", {{/each}}];
    </script>
    <script src="/__cargo-web__/test_runner.js"></script>
</head>
<body>
    <script src="js/app.js"></script>
</body>
</html>
"#;

pub fn test_in_chromium(
    backend: Backend,
    build: CargoResult,
    arg_passthrough: &Vec< &OsStr >,
    any_failure: &mut bool
) -> Result< (), Error > {
    let possible_commands =
        if cfg!( windows ) {
            &[ "chrome.exe" ][..]
        } else {
            &[ "chromium", "chromium-browser", "google-chrome", "google-chrome-stable", "Google Chrome" ][..]
        };

    let chromium_executable = find_cmd( possible_commands )
        .or_else( || {
            let path = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
            if Path::new( path ).exists() {
                Some( path )
            } else {
                None
            }
        })
        .ok_or_else( || {
            Error::EnvironmentError( "you need to have either Chromium or Chrome installed and in your PATH to run the tests!".into() )
        })?;

    let app_js = Arc::new( Mutex::new( String::new() ) );
    let server_app_js = app_js.clone();
    let handlebars = Handlebars::new();
    let mut template_data: BTreeMap< &str, Value > = BTreeMap::new();
    let arg_passthrough: Vec<_> = arg_passthrough.iter().map( |arg| arg.to_str().unwrap() ).collect();
    template_data.insert( "arguments", arg_passthrough.into() );
    template_data.insert( "target", backend.triplet().into() );
    let test_index = handlebars.render_template( DEFAULT_TEST_INDEX_HTML, &template_data ).unwrap();
    let app_wasm: Arc< Mutex< Option< Vec< u8 > > > > = Arc::new( Mutex::new( None ) );
    let wasm_url = Arc::new( Mutex::new( None ) );

    let server_app_wasm = app_wasm.clone();
    let server_wasm_url = wasm_url.clone();

    let (addr_tx, addr_rx) = channel();
    thread::spawn( move || {
        let server = SimpleServer::new(&"127.0.0.1:0".parse().unwrap(), move |request| {
            let path = request.uri().path();
            if path == "/" || path == "index.html" {
                response_from_data( &"text/html".parse().unwrap(), test_index.clone().into_bytes() )
            } else if path == "/js/app.js" {
                let data = server_app_js.lock().unwrap().clone();
                response_from_data( &"application/javascript".parse().unwrap(), data.into_bytes() )
            } else if path == "/__cargo-web__/test_runner.js" {
                response_from_data(
                    &"application/javascript".parse().unwrap(),
                    TEST_RUNNER.as_bytes().to_vec() )
            } else {
                match *server_wasm_url.lock().unwrap() {
                    Some( ref server_wasm_url ) if path == *server_wasm_url => {
                        let data = server_app_wasm.lock().unwrap().as_ref().unwrap().clone();
                        response_from_data( &"application/wasm".parse().unwrap(), data )
                    },
                    _ => response_from_status(StatusCode::NOT_FOUND)
                }
            }
        });
        addr_tx.send(server.server_addr()).unwrap();
        server.run();
    });
    let server_address: SocketAddr = addr_rx.recv().unwrap();

    let artifact = build.artifacts().iter()
        .find( |artifact| artifact.extension().map( |ext| ext == "js" ).unwrap_or( false ) )
        .expect( "internal error: no .js file found" );

    if backend.is_any_wasm() {
        let wasm_artifact = build.artifacts().iter()
            .find( |artifact| artifact.extension().map( |ext| ext == "wasm" ).unwrap_or( false ) )
            .expect( "internal error: no .wasm file found" );

        *wasm_url.lock().unwrap() = Some( format!( "/{}", wasm_artifact.file_name().unwrap().to_str().unwrap() ) );
        *app_wasm.lock().unwrap() = Some( read_bytes( wasm_artifact ).unwrap() );
    }

    *app_js.lock().unwrap() = read( artifact ).unwrap();

    let tmpdir = tempfile::Builder::new().prefix( "cargo-web-chromium-profile" ).tempdir().unwrap();
    let tmpdir = tmpdir.path().to_string_lossy();
    let mut command = Command::new( chromium_executable );
    command
        .arg( "--disable-gpu" )
        .arg( "--no-first-run" )
        .arg( "--no-sandbox" )
        .arg( "--disable-restore-session-state" )
        .arg( "--no-default-browser-check" )
        .arg( "--disable-java" )
        .arg( "--disable-client-side-phishing-detection" )
        .arg( "--headless" )
        .arg( "--remote-debugging-port=0" )
        .arg( format!( "--user-data-dir={}", tmpdir ) )
        .arg( "about:blank" );

    command
        .stdout( Stdio::null() )
        .stderr( Stdio::piped() )
        .stdin( Stdio::null() );

    debug!( "Launching chromium..." );
    let mut child = command.spawn()
        .map_err( |err| Error::RuntimeError( "cannot launch chromium".into(), err.into() ) )?;

    let stderr = BufReader::new( child.stderr.take().unwrap() );
    let devtools_regex = Regex::new( r"DevTools listening on (ws://[^:]+:\d+)" ).unwrap();
    let (url_tx, url_rx) = channel();
    thread::spawn( move || {
        for line in stderr.lines() {
            let line = match line {
                Ok( line ) => line,
                Err( _ ) => break
            };

            debug!( "Chromium stderr: {:?}", line );
            if let Some( captures ) = devtools_regex.captures( &line ) {
                let url = captures.get( 1 ).unwrap().as_str().to_owned();
                let _ = url_tx.send( url );
                break;
            }
        }
    });

    let url = url_rx.recv_timeout( Duration::from_secs( 10 ) )
        .map_err( |err| Error::RuntimeError( "timeout while waiting for chromium to start".into(), err.into() ) )?;

    debug!( "Chromium in listening on: {}", url );
    let mut connection = Connection::connect( &format!( "{}/json", url ) )
        .map_err( |err| Error::RuntimeError( "devtools connection to chromium failed".into(), err.into() ) )?;

    connection.send_cmd( "Page.enable", Value::Null );
    connection.send_cmd( "Runtime.enable", Value::Null );
    connection.send_cmd(
        "Page.navigate",
        json!({
            "url": format!( "http://localhost:{}", server_address.port() )
        })
    );

    let mut print_counter = 0;
    let mut finished = false;
    let start = Instant::now();
    let time_limit = Duration::from_secs( 60 );
    let mut get_status_req = None;
    loop {
        let elapsed = start.elapsed();
        if elapsed >= time_limit {
            break;
        }
        let remaining = time_limit - elapsed;

        let reply = connection.try_recv( Some( remaining ) );
        let reply = match reply {
            Ok( reply ) => reply,
            Err( ReplyError::Timeout ) => {
                if finished {
                    break;
                } else {
                    continue;
                }
            },
            Err( err ) => {
                return Err( Error::RuntimeError( "error while communicating with chromium".into(), err.into() ) );
            }
        };

        match reply {
            Reply::Result { ref id, ref body } if Some( *id ) == get_status_req => {
                finished = true;
                let status = body.get( "result" ).unwrap().get( "value" ).unwrap().as_u64().unwrap();
                if status != 0 {
                    eprintln!( "error: process exited with a status of {}", status );
                    *any_failure = true;
                }
                break;
            },
            Reply::Event { ref method, .. } if method == "Page.frameStoppedLoading" => {
                let id = connection.send_cmd(
                    "Runtime.evaluate",
                    json!({
                        "expression": "__cargo_web.status",
                        "awaitPromise": true
                    })
                );

                get_status_req = Some( id );
            },
            Reply::Event { ref method, ref body } if method == "Runtime.exceptionThrown" => {
                let body: ExceptionThrownBody = serde_json::from_value( body.clone() ).expect( "Failed to parse `Runtime.exceptionThrown` event" );
                eprintln!( "error: unhandled exception thrown" );
                if let Some( exception ) = body.exception_details.exception {
                    if let Some( description ) = exception.description {
                        eprintln!( "error:     {}", description );
                    }
                }
                if let Some( url ) = body.exception_details.url {
                    eprintln!( "error: source: {}:{}:{}", url, body.exception_details.line_number, body.exception_details.column_number );
                }
                // TODO: Better error message.
                *any_failure = true;
                finished = true;
                break;
            },
            Reply::Event { ref method, ref body } if method == "Runtime.consoleAPICalled" => {
                let body: ConsoleApiCalledBody = serde_json::from_value( body.clone() ).unwrap();
                match body.kind.as_str() {
                    "log" | "debug" | "info" | "error" | "warning" => {
                        let mut output = String::new();
                        for arg in body.args {
                            if !output.is_empty() {
                                output.push_str( " " );
                            }

                            if arg.kind == "string" {
                                output.push_str( arg.value.unwrap().as_str().unwrap() );
                            } else {
                                output.push_str( "<" );
                                if let Some( class_name ) = arg.class_name {
                                    output.push_str( &class_name );
                                } else {
                                    output.push_str( &arg.kind );
                                }
                                output.push_str( ">" );
                            }
                        }

                        if backend.is_emscripten() {
                            if print_counter == 0 && output.starts_with( "pre-main" ) {
                                continue;
                            } else if print_counter == 1 && output.trim().is_empty() {
                                continue;
                            }
                        }

                        println!( "{}", output );
                        print_counter += 1;
                    },
                    _ => {}
                }
            },
            Reply::Error { ref message, .. } => {
                return Err( Error::RuntimeError( "chromium returned an error".into(), message.clone().into() ) );
            },
            _ => {}
        }
    }

    if !finished {
        eprintln!( "error: tests timed out!" );
        *any_failure = true;
    }

    debug!( "Testing finished; waiting for chromium to die..." );
    child.kill().unwrap();
    child.wait().unwrap();

    Ok(())
}
