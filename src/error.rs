use std::error;
use std::fmt;
use std::io;
use std::path::PathBuf;

use cargo_shim;

#[derive(Debug)]
pub enum Error {
    ConfigurationError( String ),
    EnvironmentError( String ),
    RuntimeError( String, Box< error::Error > ),
    BuildError,
    CargoShimError( cargo_shim::Error ),
    CannotLoadFile( PathBuf, io::Error ),
    Other( Box< error::Error > )
}

impl error::Error for Error {
    fn description( &self ) -> &str {
        match *self {
            Error::ConfigurationError( ref message ) => &message,
            Error::EnvironmentError( ref message ) => &message,
            Error::RuntimeError( ref message, _ ) => &message,
            Error::BuildError => "build failed",
            Error::CargoShimError( ref error ) => error.description(),
            Error::CannotLoadFile( .. ) => "cannot load file",
            Error::Other( ref error ) => error.description()
        }
    }
}

impl From< cargo_shim::Error > for Error {
    fn from( err: cargo_shim::Error ) -> Self {
        Error::CargoShimError( err )
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
            &Error::CargoShimError( cargo_shim::Error::CargoFailed( ref message ) ) => write!( formatter, "{}", message ),
            &Error::CargoShimError( ref inner ) => write!( formatter, "{}", inner ),
            &Error::CannotLoadFile( ref path, ref inner ) => write!( formatter, "cannot load file {:?}: {}", path, inner ),
            &Error::Other( ref inner ) => write!( formatter, "{}", inner ),
            _ => write!( formatter, "{}", self.description() )
        }
    }
}
