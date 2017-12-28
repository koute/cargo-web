use std::error;
use std::fmt;

#[derive(Debug)]
pub enum Error {
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
        use std::error::Error;
        write!( formatter, "{}", self.description() )
    }
}
