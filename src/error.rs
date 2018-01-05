use std::error;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    ConfigurationError( String ),
    EnvironmentError( String ),
    RuntimeError( String, Box< error::Error > ),
    BuildError
}

impl error::Error for Error {
    fn description( &self ) -> &str {
        match *self {
            Error::ConfigurationError( ref message ) => &message,
            Error::EnvironmentError( ref message ) => &message,
            Error::RuntimeError( ref message, _ ) => &message,
            Error::BuildError => "build failed"
        }
    }
}

impl fmt::Display for Error {
    fn fmt( &self, formatter: &mut fmt::Formatter ) -> fmt::Result {
        use std::error::Error as StdError;
        match self {
            &Error::RuntimeError( _, ref inner ) => write!( formatter, "{}: {}", self.description(), inner ),
            _ => write!( formatter, "{}", self.description() )
        }
    }
}
