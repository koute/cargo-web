use std::env;
use std::path::Path;
use std::process::{Command, ExitStatus, Child};
use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::fs::File;

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
