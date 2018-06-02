use std::env;
use std::path::Path;
use std::process::{Command, ExitStatus, Child};
use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::fs::File;
use std::thread;
use std::time::{Duration, Instant};
use reqwest;

use CARGO_WEB;

#[cfg(windows)]
pub const RUSTC_EXE: &'static str = "rustc.exe";

#[cfg(not(windows))]
pub const RUSTC_EXE: &'static str = "rustc";

lazy_static! {
    pub static ref IS_NIGHTLY: bool = {
        run_and_capture( RUSTC_EXE, &["--version"][..] ).stdout.contains( "nightly" )
    };
}

pub fn get_var( name: &str ) -> String {
    match env::var( name ) {
        Ok( value ) => value,
        Err( error ) => panic!( "Cannot get the '{}' environment variable: {:?}", name, error )
    }
}

pub fn cd< P: AsRef< Path > >( path: P ) {
    let path = path.as_ref();

    if let Ok( cwd ) = env::current_dir() {
        if path.canonicalize().unwrap() == cwd.canonicalize().unwrap() {
            return;
        }
    }

    eprintln!( "> cd {}", path.to_string_lossy() );
    match env::set_current_dir( &path ) {
        Ok(()) => {},
        Err( error ) => panic!( "Cannot change current directory to {:?}: {:?}", path, error )
    }
}

pub fn in_directory< P: AsRef< Path >, R, F: FnOnce() -> R >( path: P, callback: F ) -> R {
    let cwd = env::current_dir().unwrap();
    cd( path );
    let result = callback();
    cd( cwd );

    result
}

fn run_internal< R, I, E, S, F >( executable: E, args: I, callback: F ) -> R
    where I: IntoIterator< Item = S >,
          E: AsRef< OsStr >,
          S: AsRef< OsStr >,
          F: FnOnce( Command ) -> Result< R, io::Error >
{
    let executable = executable.as_ref();
    let args: Vec< _ > = args.into_iter().map( |arg| arg.as_ref().to_owned() ).collect();

    let mut invocation: String = executable.to_string_lossy().into_owned();
    for arg in &args {
        invocation.push_str( " " );
        invocation.push_str( &arg.to_string_lossy() );
    }

    eprintln!( "> {}", invocation );

    let mut cmd = Command::new( executable );
    cmd.args( args );

    match callback( cmd ) {
        Ok( value ) => {
            value
        },
        Err( error ) => {
            panic!( "Failed to launch `{}`: {:?}", executable.to_string_lossy(), error );
        }
    }
}

pub struct Output {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String
}

pub fn run_and_capture< I, E, S >( executable: E, args: I ) -> Output
    where I: IntoIterator< Item = S >,
          E: AsRef< OsStr >,
          S: AsRef< OsStr >
{
    let output = run_internal( executable, args, |mut cmd| cmd.output() );
    if !output.status.success() {
        panic!( "Command exited with a status of {:?}!", output.status.code() );
    }

    Output {
        status: output.status,
        stdout: String::from_utf8_lossy( &output.stdout ).into_owned(),
        stderr: String::from_utf8_lossy( &output.stderr ).into_owned()
    }
}

#[must_use]
pub struct CommandResult {
    status: ExitStatus
}

impl CommandResult {
    pub fn assert_success( self ) {
        if !self.status.success() {
            panic!( "Command exited with a status of {:?}!", self.status.code() );
        }
    }

    pub fn assert_failure( self ) {
        if self.status.success() {
            panic!( "Command unexpectedly succeeded!" );
        }
    }
}

pub fn run< E, S >( executable: E, args: &[S] ) -> CommandResult
    where E: AsRef< OsStr >,
          S: AsRef< OsStr >
{
    let status = run_internal( executable, args, |mut cmd| cmd.status() );
    CommandResult {
        status
    }
}

pub struct ChildHandle {
    child: Child
}

impl Drop for ChildHandle {
    fn drop( &mut self ) {
        let _ = self.child.kill();
    }
}

pub fn run_in_the_background< E, S >( executable: E, args: &[S] ) -> ChildHandle
    where E: AsRef< OsStr >,
          S: AsRef< OsStr >
{
    run_internal( executable, args, |mut cmd| cmd.spawn().map( |child| ChildHandle { child } ) )
}

pub fn has_cmd( cmd: &str ) -> bool {
    let path = env::var_os( "PATH" ).unwrap_or( OsString::new() );
    env::split_paths( &path ).map( |p| {
        p.join( &cmd )
    }).any( |p| {
        p.exists()
    })
}

pub fn find_cmd< 'a >( cmds: &[ &'a str ] ) -> Option< &'a str > {
    cmds.into_iter().map( |&s| s ).filter( |&s| has_cmd( s ) ).next()
}

pub fn read_to_string< P: AsRef< Path > >( path: P ) -> String {
    let path = path.as_ref();
    let mut fp = match File::open( path ) {
        Ok( fp ) => fp,
        Err( error ) => panic!( "Cannot open {:?}: {}", path, error )
    };

    let mut output = String::new();
    if let Err( error ) = fp.read_to_string( &mut output ) {
        panic!( "Cannot read {:?}: {:?}", path, error );
    }

    output
}

pub fn read_to_bytes< P: AsRef< Path > >( path: P ) -> Vec<u8> {
    let path = path.as_ref();
    let mut fp = match File::open( path ) {
        Ok( fp ) => fp,
        Err( error ) => panic!( "Cannot open {:?}: {}", path, error )
    };

    let mut output = Vec::new();
    if let Err( error ) = fp.read_to_end( &mut output ) {
        panic!( "Cannot read {:?}: {:?}", path, error );
    }

    output
}

pub fn assert_file_contains< P: AsRef< Path > >( path: P, pattern: &str ) {
    let path = path.as_ref();
    let contents = read_to_string( path );
    if !contents.contains( pattern ) {
        panic!( "File {:?} doesn't contain the expected string: {:?}", path, pattern );
    }
}

pub fn assert_file_exists< P: AsRef< Path > >( path: P ) {
    let path = path.as_ref();
    if !path.exists() {
        panic!( "File {:?} doesn't exist", path );
    }
}

pub fn assert_file_missing< P: AsRef< Path > >( path: P ) {
    let path = path.as_ref();
    if path.exists() {
        panic!( "File {:?} exists", path );
    }
}

pub fn assert_wasm_emscripten_js_file_content(mut response: reqwest::Response, serve_url: &str, local_filename: &str) {
    let body_text = if serve_url != "" {
        // Remove the inserted serve_url from response text because content
        // load from disk is the original, that does not contain serve_url yet.
        // While response text is served from memory, which have serve_url inserted.
        response.text().unwrap().replace( serve_url, "" )
    } else {
        response.text().unwrap()
    };
    assert_eq!( body_text, read_to_string( local_filename ) );
}

pub fn assert_text_file_content(mut response: reqwest::Response, _: &str, local_filename: &str) {
    assert_eq!( response.text().unwrap(), read_to_string( local_filename ) );
}

pub fn assert_binary_file_content(response: reqwest::Response, _: &str, local_filename: &str) {
    use std::io::Read;
    assert_eq!(
        response.bytes().collect::<Result<Vec<u8>, ::std::io::Error>>().unwrap(),
        read_to_bytes( local_filename )
    );
}

// Extracted from `in_directory( "test-crates/static-files", || {`
// TODO: Make call to this fn (in the original source, where this is extracted)
pub fn cargo_web_start( release: bool, target: Option<&str> ) -> ChildHandle {
    use reqwest::header::ContentType;
    use reqwest::StatusCode;
    
    let mut args = if release { vec!["build", "--release"] } else { vec!["build"] };
    if let Some(target) = target {
        args.push("--target");
        args.push(target);
    }
    run( &*CARGO_WEB, &args ).assert_success();
    args[0] = "start";
    let _child = run_in_the_background( &*CARGO_WEB, &args );

    let start = Instant::now();
    let mut response = None;
    while start.elapsed() < Duration::from_secs( 10 ) && response.is_none() {
        thread::sleep( Duration::from_millis( 100 ) );
        response = reqwest::get( "http://localhost:8000" ).ok();
    }

    let response = response.unwrap();
    assert_eq!( response.status(), StatusCode::Ok );
    assert_eq!( *response.headers().get::< ContentType >().unwrap(), ContentType::html() );
    _child
}

// Also extracted from `in_directory( "test-crates/static-files", || {`
pub fn test_get_file<T>(filename: &str, fileext: &str, mimetype: &str, serve_url: Option<&str>, local_path: &str, assertor: T )
where T: FnOnce(reqwest::Response, &str, &str)
{
    use std::str::FromStr;
    use reqwest::header::ContentType;
    use reqwest::StatusCode;
    use reqwest::mime::Mime;

    let serve_url = serve_url.map( |val| format!( "{}/", val) ).unwrap_or( "".to_string() );

    let response = reqwest::get( &format!( "http://localhost:8000/{}{}.{}", serve_url.trim_left_matches("/"), filename, fileext ) ).unwrap();
    assert_eq!( response.status(), StatusCode::Ok );
    assert_eq!( *response.headers().get::< ContentType >().unwrap(), ContentType( Mime::from_str( mimetype ).unwrap() ) );

    assertor( response, &serve_url, &format!( "{}/{}.{}", local_path, filename, fileext) );
}
