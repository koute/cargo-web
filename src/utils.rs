use std::process::{Command, Stdio};
use std::path::Path;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::env;

use libflate::gzip;
use tar;

macro_rules! println_err(
    ($($arg:tt)*) => {{
        use std::io::{Write, stderr};
        writeln!( &mut stderr(), $($arg)* ).expect( "writeln to stderr failed" );
    }}
);

pub struct ExecutionStatus {
    status: Option< i32 >
}

impl ExecutionStatus {
    pub fn is_ok( &self ) -> bool {
        self.status == Some( 0 )
    }
}

pub trait CommandExt {
    fn run( &mut self ) -> ExecutionStatus;
    fn append_to_path< P: AsRef< Path > >( &mut self, path: P ) -> &mut Self;
}

impl CommandExt for Command {
    fn run( &mut self ) -> ExecutionStatus {
        let mut child = match self.spawn() {
            Ok( child ) => child,
            Err( _ ) => {
                return ExecutionStatus {
                    status: None
                };
            }
        };
        let result = child.wait();
        let status = result.unwrap().code().unwrap();
        ExecutionStatus {
            status: Some( status )
        }
    }

    fn append_to_path< P: AsRef< Path > >( &mut self, path: P ) -> &mut Self {
        let mut paths = env::var_os( "PATH" ).map( |paths| env::split_paths( &paths ).collect() ).unwrap_or( Vec::new() );
        paths.push( path.as_ref().into() );
        let new_path = env::join_paths( paths ).unwrap();

        self.env( "PATH", new_path );
        self
    }
}

pub fn read< P: AsRef< Path > >( path: P ) -> Result< String, io::Error > {
    let mut fp = File::open( path.as_ref() )?;
    let mut output = String::new();
    fp.read_to_string( &mut output )?;
    Ok( output )
}

pub fn read_bytes< P: AsRef< Path > >( path: P ) -> Result< Vec< u8 >, io::Error > {
    let mut fp = File::open( path.as_ref() )?;
    let mut output = Vec::new();
    fp.read_to_end( &mut output )?;
    Ok( output )
}

pub fn write< P: AsRef< Path > >( path: P, string: &str ) -> Result< (), io::Error > {
    let mut fp = File::create( path )?;
    fp.write_all( string.as_bytes() )?;
    Ok( () )
}

pub fn check_if_command_exists( command: &str, extra_path: Option< &str > ) -> bool {
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

pub fn unpack< I: AsRef< Path >, O: AsRef< Path > >( input_path: I, output_path: O ) -> Result< (), Box< io::Error > > {
    let output_path = output_path.as_ref();
    let file = fs::File::open( input_path )?;
    let decoder = gzip::Decoder::new( file )?;
    let mut archive = tar::Archive::new( decoder );
    archive.unpack( output_path )?;

    Ok(())
}
