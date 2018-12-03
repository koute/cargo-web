use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use toml;
use semver::Version;
use cargo_shim::CargoPackage;

use build::Backend;
use utils::read;
use error::Error;

fn is_all_strings( array: &[toml::Value] ) -> bool {
    array.iter().all( |element| element.is_str() )
}

fn from_string_or_array_of_strings( path_in_toml: &str, config: &Config, prepend_js: toml::Value ) -> Result< Vec< String >, Error > {
    match prepend_js {
        toml::Value::String( path ) => Ok( vec![ path ] ),
        toml::Value::Array( ref array ) if is_all_strings( &array ) => {
            Ok( array.iter().map( |value| value.clone().try_into().unwrap() ).collect() )
        },
        _ => {
            Err( format!(
                "{}: '{}' must be either a string or an array of strings",
                config.source(),
                path_in_toml
            ).into() )
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PerTargetConfig {
    pub link_args: Option< Vec< String > >,
    pub mount_path: Option< String >,
    pub prepend_js: Option< Vec< String > >
}

#[derive(Clone, Debug, Default)]
pub struct Config {
    crate_name: Option< String >,
    pub config_path: Option< PathBuf >,

    pub minimum_cargo_web_version: Option< Version >,
    pub per_target: HashMap< Backend, PerTargetConfig >,
    pub default_target: Option< Backend >
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

    pub fn get_mount_path( &self, backend: Backend ) -> Option< &String > {
        self.per_target.get( &backend ).and_then( |per_target| per_target.mount_path.as_ref() )
    }

    pub fn get_prepend_js( &self, backend: Backend ) -> Option< &Vec< String > > {
        self.per_target.get( &backend ).and_then( |per_target| per_target.prepend_js.as_ref() )
    }
}

pub enum Warning {
    UnknownKey( String ),
    InvalidValue( String ),
    Deprecation( String, Option< String > ),
    Custom( String ),
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

fn add_mount_path( config: &mut Config, backend: Backend, mount_path: String ) -> Result< (), Error > {
    {
        let per_target = config.per_target.entry( backend ).or_insert( Default::default() );
        if per_target.mount_path.is_none() {
            per_target.mount_path = Some( mount_path );
            return Ok(());
        }
    }

    return Err( format!( "{}: you can't have multiple 'mount-path' defined for a single target", config.source() ).into() );
}

fn add_prepend_js( config: &mut Config, backend: Backend, prepend_js: Vec< String > ) -> Result< (), Error > {
    {
        let per_target = config.per_target.entry( backend ).or_insert( Default::default() );
        if per_target.prepend_js.is_none() {
            per_target.prepend_js = Some( prepend_js );
            return Ok(());
        }
    }

    return Err( format!( "{}: you can't have multiple 'prepend-js' defined for a single target", config.source() ).into() );
}

const ALL_BACKENDS: &'static [Backend] = &[
    Backend::EmscriptenAsmJs,
    Backend::EmscriptenWebAssembly,
    Backend::WebAssembly
];

impl Config {
    pub fn load_from_file< P >(
            path: P,
            crate_name: Option< String >,
            is_main_crate: bool
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

                            for backend in ALL_BACKENDS.iter().cloned() {
                                add_link_args( &mut config, backend, link_args.clone() )?;
                            }
                        },
                        "mount-path" => {
                            let mount_path: String =
                                toplevel_value.try_into().map_err( |_|
                                    format!( "{}: 'mount-path' is not a string", config.source()
                                ))?;

                            if mount_path.chars().last().unwrap() != '/' {
                                warnings.push( Warning::Custom(
                                    "mount-path should end with a slash".to_owned()
                                ));
                            }

                            for backend in ALL_BACKENDS.iter().cloned() {
                                add_mount_path( &mut config, backend, mount_path.clone() )?;
                            }
                        },
                        "prepend-js" => {
                            let toplevel_value = from_string_or_array_of_strings( &toplevel_key, &config, toplevel_value )?;
                            for backend in ALL_BACKENDS.iter().cloned() {
                                add_prepend_js( &mut config, backend, toplevel_value.clone() )?;
                            }
                        },
                        "default-target" => {
                            let default_target: String =
                                toplevel_value.try_into().map_err( |_|
                                    format!( "{}: 'default-target' is not a string", config.source()
                                ))?;

                            let default_target = match default_target.as_str() {
                                "wasm32-unknown-unknown" => Backend::WebAssembly,
                                "wasm32-unknown-emscripten" => Backend::EmscriptenWebAssembly,
                                "asmjs-unknown-emscripten" => Backend::EmscriptenAsmJs,
                                _ => {
                                    if is_main_crate {
                                        return Err( format!( "{}: `default-target` has an invalid value: `{}`", config.source(), default_target ).into() );
                                    } else {
                                        warnings.push( Warning::Custom( toplevel_key.clone() ) );
                                    }

                                    continue;
                                }
                            };

                            config.default_target = Some( default_target );
                        },
                        "cargo-web" => {
                            let cargo_web_table: toml::value::Table =
                                toplevel_value.try_into()
                                .map_err( |_| format!( "{}: 'cargo-web' should be a section", config.source() ) )?;

                            for (cargo_web_key, cargo_web_value) in cargo_web_table {
                                match cargo_web_key.as_str() {
                                    "minimum-version" => {
                                        let version: String = cargo_web_value.try_into().map_err( |_| format!( "{}: 'cargo-web.minimum-version' is not a string", config.source() ) )?;
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
                                    let path_in_toml = format!( "target.{}.{}", target_key, per_target_key );
                                    match per_target_key.as_str() {
                                        "link-args" => {
                                            let link_args: Vec< String > =
                                                per_target_value.try_into().map_err( |_|
                                                    format!(
                                                        "{}: '{}' is not an array of strings",
                                                        config.source(),
                                                        path_in_toml
                                                    )
                                                )?;

                                            for backend in backends.iter().cloned() {
                                                add_link_args( &mut config, backend, link_args.clone() )?;
                                            }
                                        },
                                        "mount-path" => {
                                            let mount_path: String =
                                                per_target_value.try_into().map_err( |_|
                                                    format!(
                                                        "{}: '{}' is not a string",
                                                        config.source(),
                                                        path_in_toml
                                                    )
                                                )?;

                                            if mount_path.chars().last().unwrap() != '/' {
                                                warnings.push( Warning::InvalidValue(
                                                    format!("{}: '{}' should end with a slash", config.source(), path_in_toml)
                                                ));
                                            }


                                            for backend in backends.iter().cloned() {
                                                add_mount_path( &mut config, backend, mount_path.clone() )?;
                                            }
                                        },
                                        "prepend-js" => {
                                            let per_target_value = from_string_or_array_of_strings( &path_in_toml, &config, per_target_value )?;
                                            for backend in backends.iter().cloned() {
                                                add_prepend_js( &mut config, backend, per_target_value.clone() )?;
                                            }
                                        },
                                        _ => {
                                            warnings.push( Warning::UnknownKey( path_in_toml ) );
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

    pub fn load_for_package( package: &CargoPackage, is_main_crate: bool ) -> Result< Option< (Self, Vec< Warning >) >, Error > {
        let path = package.manifest_path.with_file_name( "Web.toml" );
        let config = match Config::load_from_file( path, Some( package.name.clone() ), is_main_crate )? {
            None => return Ok( None ),
            Some( config ) => config
        };

        Ok( Some( config ) )
    }

    pub fn load_for_package_printing_warnings( package: &CargoPackage, is_main_crate: bool ) -> Result< Option< Self >, Error > {
        let (config, warnings) = match Config::load_for_package( package, is_main_crate )? {
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
                },
                Warning::InvalidValue( key ) => {
                    eprintln!( "warning: key `{}` in {} has an invalid value", key, config.source() );
                }
                Warning::Custom( msg ) => {
                    eprintln!( "warning: {} in {}", msg, config.source() );
                }
            }
        }

        Ok( Some( config ) )
    }
}
