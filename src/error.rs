use std::error;
use std::fmt;
use std::io;
use std::path::PathBuf;

use cargo_shim;

#[derive(Debug)]
pub enum Error {
    ConfigurationError( String ),
    EnvironmentError( String ),
    RuntimeError( String, Box< dyn error::Error > ),
    BuildError,
    NoDefaultPackage,
    EmscriptenNotAvailable,
    CargoShimError( cargo_shim::Error ),
    CannotLoadFile( PathBuf, io::Error ),
    CannotRemoveDirectory( PathBuf, io::Error ),
    CannotRemoveFile( PathBuf, io::Error ),
    CannotCreateFile( PathBuf, io::Error ),
    CannotWriteToFile( PathBuf, io::Error ),
    CannotCopyFile( PathBuf, PathBuf, io::Error ),
    Other( Box< dyn error::Error > )
}

impl error::Error for Error {}

impl From< cargo_shim::Error > for Error {
    fn from( err: cargo_shim::Error ) -> Self {
        Error::CargoShimError( err )
    }
}

impl From< Box< dyn error::Error > > for Error {
    fn from( err: Box< dyn error::Error > ) -> Self {
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
    fn fmt( &self, fmt: &mut fmt::Formatter ) -> fmt::Result {
        match *self {
            Error::ConfigurationError( ref message ) => write!( fmt, "{}", message ),
            Error::EnvironmentError( ref message ) => write!( fmt, "{}", message ),
            Error::RuntimeError( ref message, ref inner ) => write!( fmt, "{}: {}", message, inner ),
            Error::BuildError => write!( fmt, "build failed" ),
            Error::NoDefaultPackage => write!( fmt, "no default package; you can specify a crate to use with the `-p` argument" ),
            Error::EmscriptenNotAvailable => write!( fmt, "prepackaged Emscripten is not available for this platform" ),
            Error::CargoShimError( cargo_shim::Error::CargoFailed( ref message ) ) => write!( fmt, "{}", message ),
            Error::CargoShimError( ref inner ) => write!( fmt, "{}", inner ),
            Error::CannotLoadFile( ref path, ref inner ) => write!( fmt, "cannot load file {:?}: {}", path, inner ),
            Error::CannotRemoveDirectory( ref path, ref inner ) => write!( fmt, "cannot remove directory {:?}: {}", path, inner ),
            Error::CannotRemoveFile( ref path, ref inner ) => write!( fmt, "cannot remove file {:?}: {}", path, inner ),
            Error::CannotCreateFile( ref path, ref inner ) => write!( fmt, "cannot create file {:?}: {}", path, inner ),
            Error::CannotWriteToFile( ref path, ref inner ) => write!( fmt, "cannot write to file {:?}: {}", path, inner ),
            Error::CannotCopyFile( ref src_path, ref dst_path, ref inner ) => write!( fmt, "cannot copy file from {:?} to {:?}: {}", src_path, dst_path, inner ),
            Error::Other( ref inner ) => write!( fmt, "{}", inner ),
        }
    }
}
