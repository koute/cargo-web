use std::process::exit;
use std::path::PathBuf;
use std::env;

use clap;
use cargo_shim::{
    Profile,
    CargoPackage,
    CargoProject,
    CargoTarget,
    BuildType,
    BuildConfig,
    TargetKind,
    CargoResult,
    MessageFormat,
    target_to_build_target
};
use semver::Version;

use config::Config;
use emscripten::initialize_emscripten;
use error::Error;
use wasm;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Backend {
    EmscriptenWebAssembly,
    EmscriptenAsmJs,
    WebAssembly
}

impl Backend {
    pub fn is_emscripten_asmjs( self ) -> bool {
        self == Backend::EmscriptenAsmJs
    }

    pub fn is_emscripten_wasm( self ) -> bool {
        self == Backend::EmscriptenWebAssembly
    }

    pub fn is_native_wasm( self ) -> bool {
        self == Backend::WebAssembly
    }

    pub fn is_any_wasm( self ) -> bool {
        self.is_emscripten_wasm() || self.is_native_wasm()
    }

    pub fn is_emscripten( self ) -> bool {
        self.is_emscripten_wasm() || self.is_emscripten_asmjs()
    }
}

#[derive(Clone)]
enum TargetName {
    Lib,
    Bin( String ),
    Example( String ),
    Bench( String )
}

#[derive(Clone)]
pub struct BuildArgs {
    features: Vec< String >,
    no_default_features: bool,
    enable_all_features: bool,

    build_type: BuildType,
    use_system_emscripten: bool,

    is_verbose: bool,
    message_format: MessageFormat,

    backend: Backend,

    package_name: Option< String >,
    target_name: Option< TargetName >
}

pub struct AggregatedConfig {
    profile: Profile,
    pub link_args: Vec< String >
}

impl BuildArgs {
    pub fn new( matches: &clap::ArgMatches ) -> Result< Self, Error > {
        let features = if let Some( features ) = matches.value_of( "features" ) {
            features.split_whitespace().map( |feature| feature.to_owned() ).collect()
        } else {
            Vec::new()
        };

        let no_default_features = matches.is_present( "no-default-features" );
        let enable_all_features = matches.is_present( "all-features" );

        let build_type = if matches.is_present( "release" ) {
            BuildType::Release
        } else {
            BuildType::Debug
        };

        let use_system_emscripten = matches.is_present( "use-system-emscripten" );
        let is_verbose = matches.is_present( "verbose" );
        let message_format = if let Some( name ) = matches.value_of( "message-format" ) {
            match name {
                "human" => MessageFormat::Human,
                "json" => MessageFormat::Json,
                _ => unreachable!()
            }
        } else {
            MessageFormat::Human
        };

        let backend = if matches.is_present( "target-webasm-emscripten" ) {
            eprintln!( "warning: `--target-webasm-emscripten` argument is deprecated; please use `--target wasm32-unknown-emscripten` instead" );
            Backend::EmscriptenWebAssembly
        } else if matches.is_present( "target-webasm" ) {
            eprintln!( "warning: `--target-webasm` argument is deprecated; please use `--target wasm32-unknown-unknown` instead" );
            Backend::WebAssembly
        } else if matches.is_present( "target-asmjs-emscripten" ) {
            eprintln!( "warning: `--target-asmjs-emscripten` argument is deprecated; please use `--target asmjs-unknown-emscripten` instead" );
            Backend::EmscriptenAsmJs
        } else {
            let triplet = matches.value_of( "target" );
            match triplet {
                Some( "asmjs-unknown-emscripten" ) | None => Backend::EmscriptenAsmJs,
                Some( "wasm32-unknown-emscripten" ) => Backend::EmscriptenWebAssembly,
                Some( "wasm32-unknown-unknown" ) => Backend::WebAssembly,
                _ => unreachable!( "Unknown target: {:?}", triplet )
            }
        };

        let package_name = matches.value_of( "package" ).map( |name| name.to_owned() );
        let target_name = if matches.is_present( "lib" ) {
            Some( TargetName::Lib )
        } else if let Some( name ) = matches.value_of( "bin" ) {
            Some( TargetName::Bin( name.to_owned() ) )
        } else if let Some( name ) = matches.value_of( "example" ) {
            Some( TargetName::Example( name.to_owned() ) )
        } else if let Some( name ) = matches.value_of( "bench" ) {
            Some( TargetName::Bench( name.to_owned() ) )
        } else {
            None
        };

        Ok( BuildArgs {
            features,
            no_default_features,
            enable_all_features,
            build_type,
            use_system_emscripten,
            is_verbose,
            message_format,
            backend,
            package_name,
            target_name
        })
    }

    pub fn backend( &self ) -> Backend {
        self.backend
    }

    fn triplet( &self ) -> &str {
        match self.backend {
            Backend::EmscriptenAsmJs => "asmjs-unknown-emscripten",
            Backend::EmscriptenWebAssembly => "wasm32-unknown-emscripten",
            Backend::WebAssembly => "wasm32-unknown-unknown"
        }
    }

    pub fn load_project( &self ) -> Result< Project, Error > {
        Project::new( self.clone() )
    }
}

#[derive(Clone)]
pub struct Project {
    build_args: BuildArgs,
    project: CargoProject,
    default_package: usize,
    default_target: Option< usize >
}

fn get_package< 'a >( name: Option< &str >, project: &'a CargoProject ) -> Result< usize, Error > {
    if let Some( name ) = name {
        match project.packages.iter().position( |package| package.name == name ) {
            None => Err( Error::ConfigurationError( format!( "package `{}` not found", name ) ) ),
            Some( index ) => Ok( index )
        }
    } else {
        let index = project.packages.iter().position( |package| package.is_default ).unwrap();
        Ok( index )
    }
}

fn get_target< 'a >( kind: &Option< TargetName >, package: &'a CargoPackage ) -> Result< Option< usize >, Error > {
    let kind = match *kind {
        Some( ref kind ) => kind,
        None => return Ok( None )
    };

    let targets = &package.targets;
    match *kind {
        TargetName::Lib => {
            match targets.iter().position( |target| target.kind == TargetKind::Lib ) {
                None => return Err( Error::ConfigurationError( format!( "no library targets found" ) ) ),
                index => Ok( index )
            }
        },
        TargetName::Bin( ref name ) => {
            match targets.iter().position( |target| target.kind == TargetKind::Bin && target.name == *name ) {
                None => return Err( Error::ConfigurationError( format!( "no bin target named `{}`", name ) ) ),
                index => Ok( index )
            }
        },
        TargetName::Example( ref name ) => {
            match targets.iter().position( |target| target.kind == TargetKind::Example && target.name == *name ) {
                None => return Err( Error::ConfigurationError( format!( "no example target named `{}`", name ) ) ),
                index => Ok( index )
            }
        },
        TargetName::Bench( ref name ) => {
            match targets.iter().position( |target| target.kind == TargetKind::Bench && target.name == *name ) {
                None => return Err( Error::ConfigurationError( format!( "no bench target named `{}`", name ) ) ),
                index => Ok( index )
            }
        }
    }
}

impl Project {
    pub fn new( args: BuildArgs ) -> Result< Self, Error > {
        let project = CargoProject::new( None, args.no_default_features, args.enable_all_features, &args.features )?;

        let default_package = get_package( args.package_name.as_ref().map( |name| name.as_str() ), &project )?;
        let default_target = get_target( &args.target_name, &project.packages[ default_package ] )?;

        Ok( Project {
            build_args: args,
            project,
            default_package,
            default_target
        })
    }

    pub fn build_args( &self ) -> &BuildArgs {
        &self.build_args
    }

    pub fn package( &self ) -> &CargoPackage {
        &self.project.packages[ self.default_package ]
    }

    pub fn target_or_select< 'a, F >( &'a self, package: Option< &'a CargoPackage >, filter: F ) -> Result< Vec< &'a CargoTarget >, Error >
        where for< 'r > F: Fn( &'r CargoTarget ) -> bool
    {
        let (package, index) = if let Some( package ) = package {
            (package, get_target( &self.build_args.target_name, &package )?)
        } else {
            (self.package(), self.default_target)
        };

        Ok( index.map( |target| vec![ &package.targets[ target ] ] ).unwrap_or_else( || {
            package.targets.iter().filter( |target| filter( target ) ).collect()
        }))
    }

    pub fn aggregate_configuration( &self, main_package: &CargoPackage, profile: Profile ) -> Result< AggregatedConfig, Error > {
        let mut aggregated_config = AggregatedConfig {
            profile,
            link_args: Vec::new()
        };

        let mut packages = self.project.used_packages(
            self.build_args.triplet(),
            main_package,
            profile
        );

        packages.sort_by( |a, b| {
            (
                !(*a == main_package),
                !a.is_workspace_member,
                &a.name
            ).cmp( &(
                !(*b == main_package),
                !b.is_workspace_member,
                &b.name
            ))
        });

        assert_eq!( *packages[ 0 ], *main_package );

        let mut maximum_minimum_version = None;
        let mut configs = Vec::new();
        for package in &packages {
            let config = Config::load_for_package_printing_warnings( package )?;
            if let Some( ref config ) = config {
                if let Some( ref new_requirement ) = config.minimum_cargo_web_version {
                    debug!( "{} requires cargo-web {}", config.source(), new_requirement );

                    match maximum_minimum_version.take() {
                        Some( (_, ref previous_requirement) ) if *new_requirement > *previous_requirement => {
                            maximum_minimum_version = Some( (config.source(), new_requirement.clone()) );
                        },
                        Some( previous ) => maximum_minimum_version = Some( previous ),
                        None => maximum_minimum_version = Some( (config.source(), new_requirement.clone()) )
                    }
                }
            }

            configs.push( config );
        }

        let current_version = Version::parse( env!( "CARGO_PKG_VERSION" ) ).unwrap();
        if let Some( (ref requirement_source, ref minimum_version) ) = maximum_minimum_version {
            if current_version < *minimum_version {
                return Err( format!( "{} requires at least `cargo-web` {}; please update", requirement_source, minimum_version ).into() )
            }
        }

        for config in configs.iter().rev() {
            if let Some( ref config ) = *config {
                if let Some( ref link_args ) = config.link_args {
                    debug!( "{} defines the following link-args: {:?}", config.source(), link_args );
                    aggregated_config.link_args.extend( link_args.iter().cloned() );
                }
            }
        }

        Ok( aggregated_config )
    }

    fn prepare_build_config( &self, config: &AggregatedConfig, package: &CargoPackage, target: &CargoTarget ) -> BuildConfig {
        let mut extra_paths = Vec::new();
        let mut extra_rustflags = Vec::new();
        let mut extra_environment = Vec::new();

        if self.build_args.backend.is_emscripten() {
            if let Some( emscripten ) = initialize_emscripten( self.build_args.use_system_emscripten, self.build_args.backend.is_emscripten_wasm() ) {
                extra_paths.push( emscripten.emscripten_path.clone() );

                let emscripten_path = emscripten.emscripten_path.to_string_lossy().into_owned();
                let emscripten_llvm_path = emscripten.emscripten_llvm_path.to_string_lossy().into_owned();

                extra_environment.push( ("EMSCRIPTEN".to_owned(), emscripten_path) );
                extra_environment.push( ("EMSCRIPTEN_FASTCOMP".to_owned(), emscripten_llvm_path.clone()) );
                extra_environment.push( ("LLVM".to_owned(), emscripten_llvm_path) );
                if let Some( binaryen_path ) = emscripten.binaryen_path {
                    let binaryen_path = binaryen_path.to_string_lossy().into_owned();
                    extra_environment.push( ("BINARYEN".to_owned(), binaryen_path) );
                }
            }

            // When compiling tests we want the exit runtime,
            // when compiling for the Web we don't want it
            // since that's more efficient.
            let exit_runtime = config.profile == Profile::Main;

            extra_rustflags.push( "-C".to_owned() );
            extra_rustflags.push( "link-arg=-s".to_owned() );
            extra_rustflags.push( "-C".to_owned() );
            extra_rustflags.push( format!( "link-arg=NO_EXIT_RUNTIME={}", exit_runtime as u32 ) );

            // This will allow the initially preallocated chunk
            // of memory to grow. On asm.js this has a performance
            // impact which is why we don't turn it on by default there,
            // however according to the Emscripten documentation the WASM
            // backend doesn't have that problem, so we enable it there.
            //
            // See more here:
            //   https://kripken.github.io/emscripten-site/docs/optimizing/Optimizing-Code.html#memory-growth
            let allow_memory_growth = self.build_args.backend.is_emscripten_wasm();

            extra_rustflags.push( "-C".to_owned() );
            extra_rustflags.push( "link-arg=-s".to_owned() );
            extra_rustflags.push( "-C".to_owned() );
            extra_rustflags.push( format!( "link-arg=ALLOW_MEMORY_GROWTH={}", allow_memory_growth as u32 ) );
        }

        for arg in &config.link_args {
            if arg.contains( " " ) {
                // Not sure how to handle spaces, as `-C link-arg="{}"` doesn't work.
                println_err!( "error: you have a space in one of the entries in `link-args` in your `Web.toml`;" );
                println_err!( "       this is currently unsupported - aborting!" );
                exit( 101 );
            }

            extra_rustflags.push( "-C".to_owned() );
            extra_rustflags.push( format!( "link-arg={}", arg ) );
        }

        if self.build_args.backend.is_native_wasm() && self.build_args.build_type == BuildType::Debug {
            extra_rustflags.push( "-C".to_owned() );
            extra_rustflags.push( "debuginfo=2".to_owned() );
        }

        if self.build_args.backend.is_native_wasm() {
            // Incremental compilation currently doesn't work very well with
            // this target, so disable it.
            if env::var_os( "CARGO_INCREMENTAL" ).is_some() {
                extra_environment.push( ("CARGO_INCREMENTAL".to_owned(), "0".to_owned()) );
            }
        }

        let build_type = self.build_args.build_type;
        let build_type = if self.build_args.backend.is_native_wasm() && build_type == BuildType::Debug {
            // TODO: Remove this in the future.
            println_err!( "warning: debug builds on the wasm32-unknown-unknown are currently totally broken" );
            println_err!( "         forcing a release build" );
            BuildType::Release
        } else {
            build_type
        };

        BuildConfig {
            build_target: target_to_build_target( target, config.profile ),
            build_type,
            triplet: Some( self.build_args.triplet().into() ),
            package: Some( package.name.clone() ),
            features: self.build_args.features.clone(),
            no_default_features: self.build_args.no_default_features,
            enable_all_features: self.build_args.enable_all_features,
            extra_paths,
            extra_rustflags,
            extra_environment,
            message_format: self.build_args.message_format,
            is_verbose: self.build_args.is_verbose
        }
    }

    pub fn build( &self, config: &AggregatedConfig, package: &CargoPackage, target: &CargoTarget ) -> Result< CargoResult, Error > {
        let build_config = self.prepare_build_config( config, package, target );
        let result = build_config.build( Some( |artifacts: Vec< PathBuf >| {
            let mut out = Vec::new();
            for path in artifacts {
                if let Some( artifact ) = wasm::process_wasm_file( &build_config, &path ) {
                    debug!( "Generated artifact: {:?}", artifact );
                    out.push( artifact );
                }

                out.push( path );
            }

            out
        }));

        if result.is_ok() == false {
            return Err( Error::BuildError );
        }

        Ok( result )
    }
}
