use std::error;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    ConfigurationError( String ),
    EnvironmentError( String ),
    RuntimeError( String, Box< error::Error > ),
    BuildError,
    Other( Box< error::Error > )
}

impl error::Error for Error {
    fn description( &self ) -> &str {
        match *self {
            Error::ConfigurationError( ref message ) => &message,
            Error::EnvironmentError( ref message ) => &message,
            Error::RuntimeError( ref message, _ ) => &message,
            Error::BuildError => "build failed",
            Error::Other( ref error ) => error.description()
        }
    }
}

impl From< Box< error::Error > > for Error {
    fn from( err: Box< error::Error > ) -> Self {
        Error::Other( err )
    }
}

impl From< String > for Error {
    fn from( err: String ) -> Self {
        Error::Other( err.into() )
    }
}

impl< 'a > From< &'a str > for Error {
    fn from( err: &'a str ) -> Self {
        Error::Other( err.into() )
    }
}

impl fmt::Display for Error {
    fn fmt( &self, formatter: &mut fmt::Formatter ) -> fmt::Result {
        use std::error::Error as StdError;
        match self {
            &Error::RuntimeError( _, ref inner ) => write!( formatter, "{}: {}", self.description(), inner ),
            &Error::Other( ref inner ) => write!( formatter, "{}", inner ),
            _ => write!( formatter, "{}", self.description() )
        }
    }
}
