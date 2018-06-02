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

// deploy-path can be relative or absolute
fn is_valid_deploy_path( _path: &str ) -> Result< (), Error > {
    // Currently, deploy-path is not checked here
    // Can deploy-path can be check for validity here?
    Ok(())
}

// js-wasm-path is relative to deploy-path.
// It is not allow to contains '..' or `//` or `\\`.
// It can start with `/` (but treated as sub of deploy-path).
fn is_valid_js_wasm_path( path: &str ) -> Result< (), Error > {
    use std::path::MAIN_SEPARATOR as SEP;

    let double_sep = format!("{0}{0}", SEP);
    if path.contains( &double_sep ) || path.contains( ".." ) {
        return Err( Error::ConfigurationError( format!("js-wasm-path is invalid: {}", path) ) );
    }
    Ok(())
}

// serve-url is the url from which the browser can get `.js` and `.wasm` file.
// It is not allow to contains '..' or `//` or `\\`.
// It can start with `/`.
//
// Is there differences in validity of js-wasm-path vs serve-url??? (I don't know yet)
fn is_valid_serve_url( path: &str ) -> Result< (), Error > {
    use std::path::MAIN_SEPARATOR as SEP;

    let double_sep = format!("{0}{0}", SEP);
    if path.contains( &double_sep ) || path.contains( ".." ) {
        return Err( Error::ConfigurationError( format!("serve-url is invalid: {}", path) ) );
    }
    Ok(())
}


#[derive(Clone, Debug, Default)]
pub struct PerTargetConfig {
    pub link_args: Option< Vec< String > >,
    pub prepend_js: Option< Vec< String > >,
    // Location, can be an absolute path or relative to location of Cargo.toml,
    // where you want to copy all things from `/static/*`
    pub deploy_path: Option< String >,
    // Location, relative to `deploy_path`, to output `.js` and `.wasm`
    pub js_wasm_path: Option< String >,
    // The url that `.js` and `.wasm` files are served by server
    pub serve_url: Option< String >,
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

    pub fn get_prepend_js( &self, backend: Backend ) -> Option< &Vec< String > > {
        self.per_target.get( &backend ).and_then( |per_target| per_target.prepend_js.as_ref() )
    }
}

pub enum Warning {
    UnknownKey( String ),
    InvalidValue( String ),
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

macro_rules! create_add_string_fn {
    ($fname:ident, $field_ident:ident, $path_checker:ident) => {
        fn $fname(
                &mut self,
                backends: &[Backend],
                $field_ident: toml::Value,
                path_in_toml: &str
            ) -> Result< (), Error > {
            let string_value = $field_ident.try_into().map(|val: String| val.trim().to_string()).map_err(|_|
                format!( "{}: '{}' is not a string", self.source(), path_in_toml)
            )?;

            $path_checker(&string_value)?;

            // Manually iter over backends via `loop` to avoid error on borrowing immutably and
            // mutably `self` in the same scope.
            // `loop` will break with `true` if it found multiple value for a single key on a single target
            // otherwise it return false
            let mut iter = backends.iter();
            let error = loop {
                if let Some(backend) = iter.next(){
                    let per_target = self.per_target.entry( *backend ).or_insert( Default::default() );
                    if per_target.$field_ident.is_none() {
                        per_target.$field_ident = Some( string_value.clone() );
                    }else{
                        break true;
                    }
                }else{
                    break false;
                }
            };
            if error {
                Err( format!( "{}: you can't have multiple '{}' defined for a single target", self.source(), stringify!($field_ident)).into())
            }else{
                Ok(())
            }
        }
    }
}

const ALL_BACKENDS: &'static [Backend] = &[
    Backend::EmscriptenAsmJs,
    Backend::EmscriptenWebAssembly,
    Backend::WebAssembly
];

impl Config {
    create_add_string_fn!(add_deploy_path, deploy_path, is_valid_deploy_path);
    create_add_string_fn!(add_js_wasm_path, js_wasm_path, is_valid_js_wasm_path);
    create_add_string_fn!(add_serve_url, serve_url, is_valid_serve_url);

    fn collect_target_config(
            &mut self,
            toplevel_value: toml::Value,
            warnings: &mut Vec<Warning>,
            is_main_crate: bool,
        ) -> Result< (), Error > 
    {
        let mut config = self;
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
                    "prepend-js" => {
                        let per_target_value = from_string_or_array_of_strings( &path_in_toml, &config, per_target_value )?;
                        for backend in backends.iter().cloned() {
                            add_prepend_js( &mut config, backend, per_target_value.clone() )?;
                        }
                    },
                    "deploy-path" => if is_main_crate {
                        config.add_deploy_path(backends, per_target_value, &path_in_toml)?;
                    },
                    "js-wasm-path" => if is_main_crate {
                        config.add_js_wasm_path(backends, per_target_value, &path_in_toml)?;
                    },
                    "serve-url" => if is_main_crate {
                        config.add_serve_url(backends, per_target_value, &path_in_toml)?;
                    },
                    _ => {
                        warnings.push( Warning::UnknownKey( path_in_toml ) );
                    }
                }
            }
        }
        Ok(())
    }

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

        let raw: toml::Value = match toml::from_str( config_toml.as_str() ) {
            Ok(value) => value,
            Err(error) => return Err( Error::ConfigurationError(
                format!( "Failed to parse Web.toml: {}", error )
            ))
        };
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
                                        warnings.push( Warning::InvalidValue( toplevel_key.clone() ) );
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
                        "deploy-path" => if is_main_crate {
                            config.add_deploy_path(ALL_BACKENDS, toplevel_value, "deploy-path")?;
                        },
                        "js-wasm-path" => if is_main_crate {
                            config.add_js_wasm_path(ALL_BACKENDS, toplevel_value, "js-wasm-path")?;
                        },
                        "serve-url" => if is_main_crate {
                            config.add_serve_url(ALL_BACKENDS, toplevel_value, "serve-url")?;
                        },
                        "target" => config.collect_target_config(toplevel_value, &mut warnings, is_main_crate)?,
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
            }
        }

        Ok( Some( config ) )
    }
}
