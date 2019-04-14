use std::collections::HashMap;
use std::process::{Command, Stdio, exit};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::env;

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
use serde_json;
use walkdir::WalkDir;
use atty::{self, Stream};

use config::Config;
use emscripten::initialize_emscripten;
use error::Error;
use utils::{read, find_cmd};
use wasm;

use wasm_runtime::RuntimeKind;

const ASMJS_UNKNOWN_EMSCRIPTEN: &str = "asmjs-unknown-emscripten";
const WASM32_UNKNOWN_EMSCRIPTEN: &str = "wasm32-unknown-emscripten";
const WASM32_UNKNOWN_UNKNOWN: &str = "wasm32-unknown-unknown";

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PathKind {
    File,
    Directory
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ShouldTriggerRebuild {
    Yes,
    No
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum Backend {
    EmscriptenWebAssembly,
    EmscriptenAsmJs,
    WebAssembly
}

impl FromStr for Backend {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            WASM32_UNKNOWN_UNKNOWN => Ok(Backend::WebAssembly),
            WASM32_UNKNOWN_EMSCRIPTEN => Ok(Backend::EmscriptenWebAssembly),
            ASMJS_UNKNOWN_EMSCRIPTEN => Ok(Backend::EmscriptenAsmJs),
            _ => Err(Error::ConfigurationError(format!(
                "{} is not a valid target triple.",
                s
            ))),
        }
    }
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

    pub fn triplet( &self ) -> &str {
        match *self {
            Backend::EmscriptenAsmJs => ASMJS_UNKNOWN_EMSCRIPTEN,
            Backend::EmscriptenWebAssembly => WASM32_UNKNOWN_EMSCRIPTEN,
            Backend::WebAssembly => WASM32_UNKNOWN_UNKNOWN,
        }
    }
}

#[derive(Clone)]
enum TargetName {
    Lib,
    Bin( String ),
    Example( String ),
    Bench( String )
}

impl TargetName {
    pub fn from(t: super::Target) -> Option<Self> {
        if t.lib {
            return Some(TargetName::Lib);
        }

        if let Some(bin) = t.bin {
            return Some(TargetName::Bin(bin));
        }

        if let Some(example) = t.example {
            return Some(TargetName::Example(example));
        }

        if let Some(bench) = t.bench {
            return Some(TargetName::Bench(bench));
        }

        None
    }
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

    backend: Option< Backend >,
    runtime: RuntimeKind,

    package_name: Option< String >,
    target_name: Option< TargetName >
}

pub struct AggregatedConfig {
    uses_old_stdweb: bool,
    profile: Profile,
    pub link_args: Vec< String >,
    pub prepend_js: Vec< (PathBuf, String) >
}

impl From<super::Build> for BuildArgs {
    fn from(b: super::Build) -> Self {
        Self {
            features: b.features.unwrap_or_default().split(' ').map(String::from).collect(),
            no_default_features: b.no_default_features,
            enable_all_features: b.all_features,
            build_type: if b.release { BuildType::Release } else { BuildType::Debug },
            use_system_emscripten: b.use_system_emscripten,
            is_verbose: b.verbose,
            message_format: MessageFormat::Human,
            backend: b.target,
            runtime: RuntimeKind::Standalone,
            package_name: b.package,
            target_name: None,
        }
    }
}

impl BuildArgs {
    pub(crate) fn new(build: super::Build, ext: super::BuildExt, target: super::Target) -> Result<Self, Error> {
        let mut out = Self::from(build).with_target(target);
        out.message_format = ext.message_format;

        if let Some(rt) = ext.runtime {
            match out.backend {
                None | Some(Backend::WebAssembly) => {
                    out.runtime = rt;
                }
                Some(be) => {
                    return Err(Error::ConfigurationError(format!(
                        "JavaScript runtime can only be specified for target `{}`. (Current target is `{}`)",
                        WASM32_UNKNOWN_UNKNOWN,
                        be.triplet()
                    )));
                }
            }
        }

        Ok(out)
    }

    pub(crate) fn with_target(mut self, target: super::Target) -> Self {
        self.target_name = TargetName::from(target);
        self
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
    default_target: Option< usize >,
    main_config: Option< Config >
}

fn get_package< 'a >( name: Option< &str >, project: &'a CargoProject ) -> Result< usize, Error > {
    if let Some( name ) = name {
        match project.packages.iter().position( |package| package.name == name ) {
            None => Err( Error::ConfigurationError( format!( "package `{}` not found", name ) ) ),
            Some( index ) => Ok( index )
        }
    } else {
        project.packages.iter().position( |package| package.is_default ).ok_or( Error::NoDefaultPackage )
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

        let main_config = Config::load_for_package_printing_warnings( &project.packages[ default_package ], true )?;

        let project = Project {
            build_args: args,
            project,
            default_package,
            default_target,
            main_config
        };

        if project.build_args.runtime != RuntimeKind::Standalone && !project.backend().is_native_wasm() {
            return Err( format!( "`--runtime` can be only used with `--target=wasm32-unknown-unknown`" ).into() );
        }

        Ok( project )
    }

    pub fn backend( &self ) -> Backend {
        self.build_args.backend
            .or_else( || self.main_config.as_ref().and_then( |config| config.default_target ) )
            .unwrap_or( Backend::WebAssembly )
    }

    pub fn build_args( &self ) -> &BuildArgs {
        &self.build_args
    }

    pub fn package( &self ) -> &CargoPackage {
        &self.project.packages[ self.default_package ]
    }

    pub fn target_directory( &self ) -> &Path {
        self.project.target_directory.as_ref()
    }

    pub fn target_or_select< 'a, F >( &'a self, filter: F ) -> Result< Vec< &'a CargoTarget >, Error >
        where for< 'r > F: Fn( &'r CargoTarget ) -> bool
    {
        let package = self.package();
        Ok( self.default_target.map( |target| vec![ &package.targets[ target ] ] ).unwrap_or_else( || {
            package.targets.iter().filter( |target| filter( target ) ).collect()
        }))
    }

    fn used_packages( &self, profile: Profile ) -> Vec< &CargoPackage > {
        let main_package = self.package();
        let mut packages = self.project.used_packages(
            self.backend().triplet(),
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

        for package in &packages {
            trace!( "Used package: {}", package.name );
        }

        assert_eq!( *packages[ 0 ], *main_package );
        packages
    }

    pub fn aggregate_configuration( &self, profile: Profile ) -> Result< AggregatedConfig, Error > {
        let main_package = self.package();
        let mut aggregated_config = AggregatedConfig {
            uses_old_stdweb: false,
            profile,
            link_args: Vec::new(),
            prepend_js: Vec::new()
        };

        let packages = self.used_packages( profile );
        let mut maximum_minimum_version = None;
        let mut configs = Vec::new();

        for package in &packages {
            if package.name == "stdweb" && package.id.version() < Version::parse( "0.4.11" ).unwrap() {
                debug!( "Using old `stdweb`!" );
                aggregated_config.uses_old_stdweb = true;
            } else {
                debug!( "Using new `stdweb`!" );
            }

            let config = if package.id == main_package.id {
                self.main_config.clone()
            } else {
                Config::load_for_package_printing_warnings( package, false )?
            };

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
                if let Some( ref link_args ) = config.get_link_args( self.backend() ) {
                    debug!( "{} defines the following link-args: {:?}", config.source(), link_args );
                    aggregated_config.link_args.extend( link_args.iter().cloned() );
                }

                if let Some( ref prepend_js ) = config.get_prepend_js( self.backend() ) {
                    debug!( "{} wants to prepend the following JS files: {:?}", config.source(), prepend_js );
                    let config_dir = config.config_path.as_ref().unwrap().parent().unwrap();
                    for path in prepend_js.iter() {
                        let full_path = config_dir.join( Path::new( path ) );
                        if !full_path.exists() {
                            return Err( format!( "{}: file specified by 'prepare-js' not found: {:?}", config.source(), path ).into() )
                        }

                        let contents = read( &full_path )
                            .map_err( |err| format!( "{}: cannot read {:?}: {}", config.source(), path, err ) )?;

                        aggregated_config.prepend_js.push( (full_path, contents) );
                    }
                }
            }
        }

        Ok( aggregated_config )
    }

    fn prepare_build_config( &self, config: &AggregatedConfig, target: &CargoTarget ) -> BuildConfig {
        let package = self.package();
        let mut extra_paths = Vec::new();
        let mut extra_rustflags = Vec::new();
        let mut extra_environment = Vec::new();
        let mut extra_emmaken_cflags = Vec::new();

        let vanilla_emscripten_build =
            env::var( "CARGO_WEB_VANILLA_EMSCRIPTEN_BUILD" ).map( |value| value == "1" ).unwrap_or( false ) &&
            self.backend().is_emscripten();

        if vanilla_emscripten_build {
            info!( "Vanilla Emscripten build mode enabled (this is only for testing!)" );
        }

        if self.backend().is_emscripten() {
            if let Some( emscripten ) = initialize_emscripten( self.build_args.use_system_emscripten, self.backend().is_emscripten_wasm() ) {
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
            let no_exit_runtime = config.profile == Profile::Main && !vanilla_emscripten_build;

            extra_rustflags.push( "-C".to_owned() );
            extra_rustflags.push( "link-arg=-s".to_owned() );
            extra_rustflags.push( "-C".to_owned() );
            extra_rustflags.push( format!( "link-arg=NO_EXIT_RUNTIME={}", no_exit_runtime as u32 ) );

            if !vanilla_emscripten_build {
                // This will allow the initially preallocated chunk
                // of memory to grow. On asm.js this has a performance
                // impact which is why we don't turn it on by default there,
                // however according to the Emscripten documentation the WASM
                // backend doesn't have that problem, so we enable it there.
                //
                // See more here:
                //   https://kripken.github.io/emscripten-site/docs/optimizing/Optimizing-Code.html#memory-growth
                let allow_memory_growth = self.backend().is_emscripten_wasm();

                extra_rustflags.push( "-C".to_owned() );
                extra_rustflags.push( "link-arg=-s".to_owned() );
                extra_rustflags.push( "-C".to_owned() );
                extra_rustflags.push( format!( "link-arg=ALLOW_MEMORY_GROWTH={}", allow_memory_growth as u32 ) );

                for &(ref path, _) in &config.prepend_js {
                    let path_str = path.to_str().expect( "invalid 'prepend-js' path" );
                    extra_emmaken_cflags.push( "--pre-js" );
                    extra_emmaken_cflags.push( path_str );
                }
            }
        }

        if !vanilla_emscripten_build {
            for arg in &config.link_args {
                if arg.contains( " " ) {
                    // Not sure how to handle spaces, as `-C link-arg="{}"` doesn't work.
                    eprintln!( "error: you have a space in one of the entries in `link-args` in your `Web.toml`;" );
                    eprintln!( "       this is currently unsupported - aborting!" );
                    exit( 101 );
                }

                extra_rustflags.push( "-C".to_owned() );
                extra_rustflags.push( format!( "link-arg={}", arg ) );
            }
        }

        if self.backend().is_native_wasm() && self.build_args.build_type == BuildType::Debug && config.uses_old_stdweb {
            extra_rustflags.push( "-C".to_owned() );
            extra_rustflags.push( "debuginfo=2".to_owned() );
        }

        if self.backend().is_native_wasm() {
            // Incremental compilation currently doesn't work very well with
            // this target, so disable it.
            if env::var_os( "CARGO_INCREMENTAL" ).is_some() {
                extra_environment.push( ("CARGO_INCREMENTAL".to_owned(), "0".to_owned()) );
            }
        }

        let build_type = self.build_args.build_type;
        let build_type = if self.backend().is_native_wasm() && build_type == BuildType::Debug && config.uses_old_stdweb {
            // TODO: Remove this in the future.
            eprintln!( "warning: debug builds on the wasm32-unknown-unknown are currently totally broken" );
            eprintln!( "         forcing a release build" );
            BuildType::Release
        } else {
            build_type
        };

        if !extra_emmaken_cflags.is_empty() && !vanilla_emscripten_build {
            // We need to do this through EMMAKEN_CFLAGS since Rust can't handle linker args with spaces.
            // https://github.com/rust-lang/rust/issues/30947
            let emmaken_cflags: Vec< _ > = extra_emmaken_cflags.into_iter().map( |flag| format!( "\"{}\"", flag ) ).collect();
            let mut emmaken_cflags = emmaken_cflags.join( " " );
            if let Ok( user_emmaken_cflags ) = env::var( "EMMAKEN_CFLAGS" ) {
                emmaken_cflags = format!( "{} {}", emmaken_cflags, user_emmaken_cflags );
            }

            extra_environment.push( ("EMMAKEN_CFLAGS".to_owned(), emmaken_cflags) );
        }

        if !vanilla_emscripten_build {
            extra_environment.push( ("COMPILING_UNDER_CARGO_WEB".to_owned(), "1".to_owned()) );
        }

        extra_environment.push( ("CARGO_WEB_TARGET_DIR".to_owned(), self.target_directory().to_str().unwrap().to_owned()) );
        extra_environment.push( ("CARGO_WEB_TARGET".to_owned(), self.backend().triplet().to_owned()) );

        BuildConfig {
            build_target: target_to_build_target( target, config.profile ),
            build_type,
            triplet: Some( self.backend().triplet().into() ),
            package: Some( package.name.clone() ),
            features: self.build_args.features.clone(),
            no_default_features: self.build_args.no_default_features,
            enable_all_features: self.build_args.enable_all_features,
            extra_paths,
            extra_rustflags,
            extra_environment,
            message_format: self.build_args.message_format,
            is_verbose: self.build_args.is_verbose,
            use_color: atty::is(Stream::Stdout) && atty::is(Stream::Stderr)
        }
    }

    pub fn static_paths( package: &CargoPackage, target: &CargoTarget ) -> Vec< PathBuf > {
        let crate_static_path = package.crate_root.join( "static" );
        let target_static_path = match target.kind {
            TargetKind::Example => Some( target.source_directory.join( format!( "{}-static", target.name ) ) ),
            TargetKind::Bin => Some( target.source_directory.join( "static" ) ),
            _ => None
        };

        let mut output = Vec::new();
        if let Some( target_static_path ) = target_static_path {
            output.push( target_static_path );
        }
        output.push( crate_static_path );
        output
    }

    pub fn paths_to_watch( &self, target: &CargoTarget ) -> Vec< (PathBuf, PathKind, ShouldTriggerRebuild) > {
        // TODO: `Web.toml` and `prepend-js` support.
        let mut paths = Vec::new();
        paths.push( (target.source_directory.clone(), PathKind::Directory, ShouldTriggerRebuild::Yes) );

        let packages = self.used_packages( Profile::Main );
        for package in packages {
            paths.push( (package.manifest_path.clone(), PathKind::File, ShouldTriggerRebuild::Yes) );
            if let Some( lib_target ) = package.targets.iter().find( |target| target.kind == TargetKind::Lib || target.kind == TargetKind::CDyLib ) {
                paths.push( (lib_target.source_directory.clone(), PathKind::Directory, ShouldTriggerRebuild::Yes) );
            }
        }

        let main_package = self.package();
        paths.extend(
            Project::static_paths( main_package, target )
                .into_iter()
                .map( |path| (path, PathKind::Directory, ShouldTriggerRebuild::No) )
        );

        paths
    }

    fn install_target_if_necessary( &self ) -> Result< (), Error > {
        let rustup = match find_cmd( &[ "rustup", "rustup.exe" ] ) {
            Some( path ) => path,
            // If the user installed Rust not through rustup then they're on their own.
            None => return Ok(())
        };

        let output = Command::new( rustup )
            .args( &[ "target", "list" ] )
            .output()
            .map_err( |err| Error::RuntimeError( "cannot get the target list through rustup".into(), err.into() ) )?;

        if !output.status.success() {
            return Err( "cannot get the target list through rustup: rustup invocation failed".into() );
        }

        let mut targets = HashMap::new();
        let stdout = String::from_utf8_lossy( &output.stdout );
        for line in stdout.trim().split( "\n" ) {
            let target = &line[ 0..line.find( " " ).unwrap_or( line.len() ) ];
            let is_installed = line.ends_with( "(installed)" );

            trace!( "Target `{}`: {}", target, is_installed );
            targets.insert( target.to_owned(), is_installed );
        }

        match targets.get( self.backend().triplet() ).cloned() {
            Some( false ) => {
                debug!( "Trying to install target `{}`...", self.backend().triplet() );
                let result = Command::new( rustup )
                    .args( &[ "target", "add", self.backend().triplet() ] )
                    .stdout( Stdio::null() )
                    .stderr( Stdio::inherit() )
                    .status();
                let result = result.map_err( |err| {
                    Error::RuntimeError(
                        format!( "installation of target `{}` through rustup failed", self.backend().triplet() ),
                        err.into()
                    )
                })?;

                if !result.success() {
                    return Err( format!( "installation of target `{}` through rustup failed", self.backend().triplet() ).into() );
                }

                Ok(())
            },
            Some( true ) => {
                Ok(())
            },
            None => {
                Err( format!(
                    "target `{}` is not available for this Rust toolchain; maybe try Rust nighly?",
                    self.backend().triplet()
                ).into() )
            }
        }
    }

    fn build_or_check( &self, config: &AggregatedConfig, target: &CargoTarget, should_build: bool ) -> Result< CargoResult, Error > {
        self.install_target_if_necessary()?;

        let build_config = self.prepare_build_config( config, target );
        let mut prepend_js = String::new();
        if self.backend().is_native_wasm() {
            for &(_, ref contents) in &config.prepend_js {
                prepend_js.push_str( &contents );
                prepend_js.push_str( "\n" );
            }
        }

        if self.build_args.message_format == MessageFormat::Json {
            let mut paths = Vec::new();
            for (path, kind, trigger_rebuild) in self.paths_to_watch( target ) {
                if trigger_rebuild == ShouldTriggerRebuild::No {
                    // TODO: Emit these too maybe?
                    continue;
                }

                match kind {
                    PathKind::File => {
                        paths.push( json!({ "path": path.to_string_lossy() }) );
                    },
                    PathKind::Directory => {
                        for entry in WalkDir::new( path ) {
                            if let Ok( entry ) = entry {
                                let path = entry.path();
                                if path.is_file() {
                                    paths.push( json!({ "path": path.to_string_lossy() }) );
                                }
                            }
                        }
                    }
                }
            }

            let message = json!({
                "reason": "cargo-web-paths-to-watch",
                "paths": paths
            });

            println!( "{}", serde_json::to_string( &message ).unwrap() );
        }

        let target_dir = self.target_directory();
        let result = if !should_build {
            build_config.check()
        } else {
            let is_wasm32_unknown_unknown =
                build_config.triplet.as_ref()
                    .map( |triplet| triplet == "wasm32-unknown-unknown" )
                    .unwrap_or( false );

            build_config.build( Some( |artifacts: Vec< PathBuf >| {
                let mut out = Vec::new();
                for path in artifacts {
                    let skip =
                        is_wasm32_unknown_unknown &&
                        path.extension().map( |ext| ext == "wasm" ).unwrap_or( false ) &&
                        path.parent().and_then( |parent| parent.file_name() ).map( |dir| dir == "deps" ).unwrap_or( false );

                    if skip {
                        continue;
                    }

                    if let Some( artifact ) = wasm::process_wasm_file( config.uses_old_stdweb, self.build_args.runtime, &build_config, &prepend_js, target_dir, &path ) {
                        debug!( "Generated artifact: {:?}", artifact );
                        out.push( artifact );
                    }

                    out.push( path );
                }

                out
            }))
        };

        if result.is_ok() == false {
            return Err( Error::BuildError );
        }

        Ok( result )
    }

    pub fn build( &self, config: &AggregatedConfig, target: &CargoTarget ) -> Result< CargoResult, Error > {
        self.build_or_check( config, target, true )
    }

    pub fn check( &self, config: &AggregatedConfig, target: &CargoTarget ) -> Result< CargoResult, Error > {
        self.build_or_check( config, target, false )
    }
}
