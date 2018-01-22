use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use toml;
use semver::Version;
use cargo_shim::CargoPackage;

use build::Backend;
use utils::read;
use error::Error;

#[derive(Debug, Default)]
pub struct PerTargetConfig {
    pub link_args: Option< Vec< String > >
}

#[derive(Debug, Default)]
pub struct Config {
    crate_name: Option< String >,
    config_path: Option< PathBuf >,

    pub minimum_cargo_web_version: Option< Version >,
    pub per_target: HashMap< Backend, PerTargetConfig >
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

    pub fn get_link_args( &self, backend: Backend ) -> Option< &Vec< String > > {
        self.per_target.get( &backend ).and_then( |per_target| per_target.link_args.as_ref() )
    }
}

pub enum Warning {
    UnknownKey( String ),
    Deprecation( String, Option< String > )
}

fn add_link_args( config: &mut Config, backend: Backend, link_args: Vec< String > ) -> Result< (), Error > {
    {
        let per_target = config.per_target.entry( backend ).or_insert( Default::default() );
        if per_target.link_args.is_none() {
            per_target.link_args = Some( link_args.clone() );
            return Ok(());
        }
    }

    return Err( format!( "{}: you can't have multiple 'link-args' defined for a single target", config.source() ).into() );
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

        // TODO: This is getting way too long. Split it into multiple functions.
        trace!( "Loaded config: {:#?}", raw );
        match raw {
            toml::Value::Table( table ) => {
                for (toplevel_key, toplevel_value) in table {
                    match toplevel_key.as_str() {
                        // TODO: Remove this in the future.
                        "link-args" => {
                            let link_args: Vec< String > =
                                toplevel_value.try_into().map_err( |_|
                                    format!( "{}: 'link-args' is not an array of strings", config.source()
                                ))?;

                            warnings.push( Warning::Deprecation(
                                "link-args".to_owned(),
                                Some( "it should be moved to the '[target.emscripten]' section".to_owned() )
                            ));

                            let backends = [
                                Backend::EmscriptenAsmJs,
                                Backend::EmscriptenWebAssembly,
                                Backend::WebAssembly
                            ];

                            for backend in backends.iter().cloned() {
                                add_link_args( &mut config, backend, link_args.clone() )?;
                            }
                        },
                        "cargo-web" => {
                            let cargo_web_table: toml::value::Table =
                                toplevel_value.try_into()
                                .map_err( |_| format!( "{}: 'cargo-web' should be a section", config.source() ) )?;

                            for (cargo_web_key, cargo_web_value) in cargo_web_table {
                                match cargo_web_key.as_str() {
                                    "minimum-version" => {
                                        let version: String = cargo_web_value.try_into().map_err( |_| format!( "{}; 'cargo-web.minimum-version' is not a string", config.source() ) )?;
                                        let version = Version::parse( &version ).map_err( |_| format!( "{}: 'cargo-web.minimum-version' is not a valid version", config.source() ) )?;
                                        config.minimum_cargo_web_version = Some( version );
                                    },
                                    cargo_web_key => {
                                        warnings.push( Warning::UnknownKey( format!( "cargo-web.{}", cargo_web_key ) ) );
                                    }
                                }
                            }
                        },
                        "target" => {
                            let target_table: toml::value::Table =
                                toplevel_value.try_into()
                                .map_err( |_| format!( "{}: 'target' should be a section", config.source() ) )?;

                            for (target_key, target_value) in target_table {
                                let backends = match target_key.as_str() {
                                    "wasm32-unknown-unknown" => &[Backend::WebAssembly][..],
                                    "wasm32-unknown-emscripten" => &[Backend::EmscriptenWebAssembly][..],
                                    "asmjs-unknown-emscripten" => &[Backend::EmscriptenAsmJs][..],
                                    "emscripten" => &[Backend::EmscriptenWebAssembly, Backend::EmscriptenAsmJs][..],
                                    target_key => {
                                        warnings.push( Warning::UnknownKey( format!( "target.{}", target_key ) ) );
                                        continue;
                                    }
                                };

                                let target_subtable: toml::value::Table =
                                    target_value.try_into()
                                    .map_err( |_| format!( "{}: 'target.{}' should be a section", config.source(), target_key ) )?;

                                for (per_target_key, per_target_value) in target_subtable {
                                    match per_target_key.as_str() {
                                        "link-args" => {
                                            let link_args: Vec< String > =
                                                per_target_value.try_into().map_err( |_|
                                                    format!(
                                                        "{}: 'target.{}.link-args' is not an array of strings",
                                                        config.source(),
                                                        target_key
                                                    )
                                                )?;

                                            for backend in backends.iter().cloned() {
                                                add_link_args( &mut config, backend, link_args.clone() )?;
                                            }
                                        },
                                        per_target_key => {
                                            warnings.push( Warning::UnknownKey( format!( "target.{}.{}", target_key, per_target_key ) ) );
                                        }
                                    }
                                }
                            }
                        },
                        toplevel_key => {
                            warnings.push( Warning::UnknownKey( toplevel_key.into() ) );
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
                    eprintln!( "warning: unknown key in {}: {}", config.source(), key );
                },
                Warning::Deprecation( key, None ) => {
                    eprintln!( "warning: key in {} is deprecated: {}", config.source(), key );
                },
                Warning::Deprecation( key, Some( description ) ) => {
                    eprintln!( "warning: key in {} is deprecated: {} ({})", config.source(), key, description );
                }
            }
        }

        Ok( Some( config ) )
    }
}
