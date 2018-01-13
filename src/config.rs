use std::io;
use std::path::{Path, PathBuf};
use toml;
use semver::Version;
use cargo_shim::CargoPackage;
use utils::read;
use error::Error;

#[derive(Debug, Default)]
pub struct Config {
    crate_name: Option< String >,
    config_path: Option< PathBuf >,

    pub minimum_cargo_web_version: Option< Version >,
    pub link_args: Option< Vec< String > >
}

impl Config {
    pub fn source( &self ) -> String {
        if let Some( ref name ) = self.crate_name {
            format!( "`{}`'s Web.toml", name )
        } else if let Some( ref path ) = self.config_path {
            format!( "{:?}", path )
        } else {
            "Web.toml".into()
        }
    }
}

pub enum Warning {
    UnknownKey( String )
}

impl Config {
    pub fn load_from_file< P >(
            path: P,
            crate_name: Option< String >
        ) -> Result< Option< (Self, Vec< Warning >) >, Error > where P: AsRef< Path >
    {
        let path = path.as_ref();

        let mut config = Config::default();
        config.config_path = Some( path.into() );
        config.crate_name = crate_name.clone();

        let config_toml = match read( path ) {
            Ok( config ) => config,
            Err( error ) => {
                if error.kind() == io::ErrorKind::NotFound {
                    return Ok( None );
                } else {
                    return Err( format!( "cannot load {}: {}", config.source(), error ).into() );
                }
            }
        };

        debug!( "Loading {:?}...", path );

        let raw: toml::Value = toml::from_str( config_toml.as_str() ).unwrap();
        let mut warnings = Vec::new();
        match raw {
            toml::Value::Table( table ) => {
                for (key, value) in table {
                    match key.as_str() {
                        "link-args" => {
                            config.link_args = Some(
                                value.try_into().map_err( |_| format!( "{}: 'link-args' is not a string", config.source() ) )?
                            );
                        },
                        "cargo-web" => {
                            let subtable: toml::value::Table =
                                value.try_into()
                                .map_err( |_| format!( "{}: 'cargo-web' should be a section", config.source() ) )?;

                            for (key, value) in subtable {
                                match key.as_str() {
                                    "minimum-version" => {
                                        let version: String = value.try_into().map_err( |_| format!( "{}; 'cargo-web.minimum-version' is not a string", config.source() ) )?;
                                        let version = Version::parse( &version ).map_err( |_| format!( "{}: 'cargo-web.minimum-version' is not a valid version", config.source() ) )?;
                                        config.minimum_cargo_web_version = Some( version );
                                    },
                                    _ => {
                                        warnings.push( Warning::UnknownKey( format!( "cargo-web.{}", key ) ) );
                                    }
                                }
                            }
                        },
                        _ => {
                            warnings.push( Warning::UnknownKey( key.into() ) );
                        }
                    }
                }
            },
            _ => panic!()
        }

        Ok( Some( (config, warnings) ) )
    }

    pub fn load_for_package( package: &CargoPackage ) -> Result< Option< (Self, Vec< Warning >) >, Error > {
        let path = package.manifest_path.with_file_name( "Web.toml" );
        let config = match Config::load_from_file( path, Some( package.name.clone() ) )? {
            None => return Ok( None ),
            Some( config ) => config
        };

        Ok( Some( config ) )
    }

    pub fn load_for_package_printing_warnings( package: &CargoPackage ) -> Result< Option< Self >, Error > {
        let (config, warnings) = match Config::load_for_package( package )? {
            Some( (config, warnings) ) => (config, warnings),
            None => return Ok( None )
        };

        for warning in warnings {
            match warning {
                Warning::UnknownKey( key ) => {
                    println_err!( "warning: unknown key in {}: {}", config.source(), key );
                }
            }
        }

        Ok( Some( config ) )
    }
}
