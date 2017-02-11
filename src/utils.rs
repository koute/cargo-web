use std::process::Command;
use std::path::Path;
use std::fs::File;
use std::io::Read;
use std::error::Error;
use std::env;

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

pub fn read< P: AsRef< Path > >( path: P ) -> Result< String, Box< Error > > {
    let mut fp = File::open( path.as_ref() )?;
    let mut output = String::new();
    fp.read_to_string( &mut output )?;
    Ok( output )
}
