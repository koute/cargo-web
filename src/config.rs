use std::error::Error;
use std::io;
use std::path::Path;
use toml;
use cargo_shim::CargoPackage;
use utils::read;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub link_args: Option< Vec< String > >
}

pub enum Warning {
    UnknownKey( String )
}

impl Config {
    pub fn load_from_file< P: AsRef< Path > >( path: P ) -> Result< Option< (Self, Vec< Warning >) >, Box< Error > > {
        let config_toml = match read( path ) {
            Ok( config ) => config,
            Err( error ) => {
                if error.kind() == io::ErrorKind::NotFound {
                    return Ok( None );
                } else {
                    return Err( error.into() );
                }
            }
        };

        let config = toml::from_str( config_toml.as_str() )?;

        // It seems bizzare that I have to do this manually.
        let raw: toml::Value = toml::from_str( config_toml.as_str() )?;
        let mut warnings = Vec::new();
        match raw {
            toml::Value::Table( table ) => {
                for (key, _) in table {
                    if key == "link-args" {
                        continue;
                    } else {
                        warnings.push( Warning::UnknownKey( key.into() ) );
                    }
                }
            },
            _ => panic!()
        }

        Ok( Some( (config, warnings) ) )
    }

    pub fn load_for_package( package: &CargoPackage ) -> Result< Option< (Self, Vec< Warning >) >, Box< Error > > {
        let path = package.manifest_path.with_file_name( "Web.toml" );
        Config::load_from_file( path )
    }

    pub fn load_for_package_printing_warnings( package: &CargoPackage ) -> Result< Option< Self >, Box< Error > > {
        let (config, warnings) = match Config::load_for_package( package )? {
            Some( (config, warnings) ) => (config, warnings),
            None => return Ok( None )
        };

        for warning in warnings {
            match warning {
                Warning::UnknownKey( key ) => {
                    println_err!( "warning: unknown key in Web.toml: {}", key );
                }
            }
        }

        Ok( Some( config ) )
    }
}
