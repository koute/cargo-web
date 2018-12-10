use std::env;
use std::path::Path;
use std::process::{Command, ExitStatus, Child, Stdio};
use std::ffi::{OsStr, OsString};
use std::io::{self, Read, BufRead, BufReader};
use std::fs::{File, OpenOptions};
use std::thread;
use std::sync::{Mutex, Arc};
use std::mem;

fn run_internal< R, I, C, E, S, F >( cwd: C, executable: E, args: I, callback: F ) -> R
    where I: IntoIterator< Item = S >,
          C: AsRef< Path >,
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
    cmd.current_dir( cwd );

    match callback( cmd ) {
        Ok( value ) => {
            value
        },
        Err( error ) => {
            panic!( "Failed to launch `{}`: {:?}", executable.to_string_lossy(), error );
        }
    }
}

#[must_use]
pub struct CommandResult {
    status: ExitStatus,
    output: String
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

    pub fn output( &self ) -> &str {
        &self.output
    }
}

fn print_stream< T: Read + Send + 'static >( fp: T, output: Arc< Mutex< String > > ) -> thread::JoinHandle< () > {
    let fp = BufReader::new( fp );
    thread::spawn( move || {
        for line in fp.lines() {
            let line = match line {
                Ok( line ) => line,
                Err( _ ) => break
            };

            let mut output = output.lock().unwrap();
            output.push_str( &line );
            output.push_str( "\n" );
        }
    })
}

pub struct ChildHandle {
    output: Arc< Mutex< String > >,
    stdout_join: Option< thread::JoinHandle< () > >,
    stderr_join: Option< thread::JoinHandle< () > >,
    child: Child
}

impl ChildHandle {
    pub fn wait( mut self ) -> CommandResult {
        let status = self.child.wait().unwrap();
        let output = self.flush_output();

        CommandResult {
            status,
            output
        }
    }

    fn flush_output( &mut self ) -> String {
        if let Some( stdout_join ) = self.stdout_join.take() {
            let _ = stdout_join.join();
        }

        if let Some( stderr_join ) = self.stderr_join.take() {
            let _ = stderr_join.join();
        }

        let mut output = String::new();
        mem::swap( &mut output, &mut self.output.lock().unwrap() );
        print!( "{}", output );

        output
    }
}

impl Drop for ChildHandle {
    fn drop( &mut self ) {
        let _ = self.child.kill();
        self.flush_output();
    }
}

pub fn run_in_the_background< C, E, S >( cwd: C, executable: E, args: &[S] ) -> ChildHandle
    where C: AsRef< Path >,
          E: AsRef< OsStr >,
          S: AsRef< OsStr >
{
    run_internal( cwd, executable, args, |mut cmd| {
        let output = Arc::new( Mutex::new( String::new() ) );
        cmd.stdin( Stdio::null() );
        cmd.stdout( Stdio::piped() );
        cmd.stderr( Stdio::piped() );

        let mut child = cmd.spawn()?;
        let stdout_join = print_stream( child.stdout.take().unwrap(), output.clone() );
        let stderr_join = print_stream( child.stderr.take().unwrap(), output.clone() );

        Ok( ChildHandle {
            output,
            stdout_join: Some( stdout_join ),
            stderr_join: Some( stderr_join ),
            child
        })
    })
}

pub fn run< C, E, S >( cwd: C, executable: E, args: &[S] ) -> CommandResult
    where C: AsRef< Path >,
          E: AsRef< OsStr >,
          S: AsRef< OsStr >
{
    run_in_the_background( cwd, executable, args ).wait()
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

pub fn touch_file< P: AsRef< Path > >( path: P ) {
    let path = path.as_ref();
    if let Err( error ) = OpenOptions::new().append( true ).open( path ) {
        panic!( "Cannot touch {:?}: {:?}", path, error );
    }
}
