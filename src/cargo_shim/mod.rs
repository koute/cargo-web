use std::collections::{HashSet, HashMap};
use std::process::{Command, Stdio};
use std::path::{Path, PathBuf};
use std::io::{self, BufRead, BufReader};
use std::ffi::{OsString, OsStr};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::cell::Cell;
use std::env;
use std::thread;
use std::str::{self, FromStr};
use std::error;
use std::fmt;
use std::iter;

use cargo_metadata;
use serde_json;

mod cargo;
mod cargo_output;
mod rustc_diagnostic;
mod diagnostic_formatter;

use self::cargo::cfg::{Cfg, CfgExpr};
use self::cargo_output::{CargoOutput, PackageId};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BuildType {
    Debug,
    Release
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Profile {
    Main,
    Test,
    Bench
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TargetKind {
    Lib,
    CDyLib,
    Bin,
    Example,
    Test,
    Bench
}

#[derive(Clone, Debug)]
pub struct CargoProject {
    pub packages: Vec< CargoPackage >,
    pub target_directory: String
}

#[derive(Clone, Debug)]
pub struct CargoPackageId( PackageId );

// TODO: Fix this upstream.
impl PartialEq for CargoPackageId {
    fn eq( &self, rhs: &CargoPackageId ) -> bool {
        self.0.name() == rhs.0.name() &&
        self.0.version() == rhs.0.version() &&
        self.0.url() == rhs.0.url()
    }
}

impl Eq for CargoPackageId {}

impl Hash for CargoPackageId {
    fn hash< H: Hasher >( &self, state: &mut H ) {
        self.0.name().hash( state );
        self.0.version().hash( state );
        self.0.url().hash( state );
    }
}

impl CargoPackageId {
    fn new( id: &str ) -> Option< Self > {
        let value = serde_json::Value::String( id.to_owned() );
        match serde_json::from_value( value ).ok() {
            Some( package_id ) => Some( CargoPackageId( package_id ) ),
            None => None
        }
    }
}

impl Deref for CargoPackageId {
    type Target = PackageId;
    fn deref( &self ) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct CargoPackage {
    pub id: CargoPackageId,
    pub name: String,
    pub manifest_path: PathBuf,
    pub crate_root: PathBuf,
    pub targets: Vec< CargoTarget >,
    pub dependencies: Vec< CargoDependency >,
    pub is_workspace_member: bool,
    pub is_default: bool
}

#[derive(Clone, PartialEq, Debug)]
pub struct CargoTarget {
    pub name: String,
    pub kind: TargetKind,
    pub source_directory: PathBuf
}

#[derive(Clone, PartialEq, Debug)]
pub enum CargoDependencyKind {
    Normal,
    Development,
    Build
}

#[derive(Clone, PartialEq, Debug)]
pub enum CargoDependencyTarget {
    Expr( CfgExpr ),
    Target( String )
}

struct TargetCfg {
    triplet: String,
    map: HashMap< String, String >,
    set: HashSet< String >
}

impl TargetCfg {
    fn new( triplet: &str, rustflags: &OsStr ) -> Self {
        let rustc =
            if cfg!( windows ) {
                "rustc.exe"
            } else {
                "rustc"
            };

        // NOTE: Currently this will always emit the "debug_assertion" config
        // even if we're compiling in release mode, and it won't contain "test"
        // even if we're building tests.
        //
        // Cargo works the same way:
        //    https://github.com/rust-lang/cargo/issues/5777

        debug!( "Querying the target config..." );
        let mut command = Command::new( rustc );
        command.arg( "--target" );
        command.arg( triplet );
        command.arg( "--print" );
        command.arg( "cfg" );

        let rustflags = rustflags
            .to_string_lossy()
            .into_owned();

        let args = rustflags
            .split( ' ' )
            .map( str::trim )
            .filter( |s| !s.is_empty() )
            .map( str::to_string );

        for arg in args {
            command.arg( arg );
        }

        let mut map = HashMap::new();
        let mut set = HashSet::new();

        command.stdout( Stdio::piped() );
        command.stderr( Stdio::inherit() );
        command.stdin( Stdio::null() );
        let result = command.output().expect( "could not launch rustc" );
        if !result.status.success() {
            panic!( "Failed to grab the target configuration from rustc!" );
        }

        for line in BufReader::new( result.stdout.as_slice() ).lines() {
            let line = line.unwrap();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some( index ) = line.chars().position( |byte| byte == '=' ) {
                let key: String = line.chars().take( index ).collect();
                let mut value: String = line.chars().skip( index + 2 ).collect();
                value.pop().unwrap();
                debug!( "Target config: '{}' = '{}'", key, value );
                map.insert( key, value );
            } else {
                debug!( "Target config: '{}'", line );
                set.insert( line.to_owned() );
            }
        }

        TargetCfg {
            triplet: triplet.to_owned(),
            map,
            set
        }
    }

    fn matches( &self, expr: &CfgExpr ) -> bool {
        match *expr {
            CfgExpr::Not( ref inner ) => !self.matches( &inner ),
            CfgExpr::All( ref inner ) => inner.iter().all( |inner| self.matches( &inner ) ),
            CfgExpr::Any( ref inner ) => inner.iter().any( |inner| self.matches( &inner ) ),
            CfgExpr::Value( ref inner ) => self.matches_value( &inner )
        }
    }

    fn matches_value( &self, value: &Cfg ) -> bool {
        match *value {
            Cfg::Name( ref name ) => {
                self.set.contains( name )
            },
            Cfg::KeyPair( ref key, ref expected_value ) => {
                self.map.get( key ).map( |value| value == expected_value ).unwrap_or( false )
            }
        }
    }
}

impl CargoDependencyTarget {
    fn matches( &self, target_cfg: &TargetCfg ) -> bool {
        match *self {
            CargoDependencyTarget::Target( ref target ) => *target == target_cfg.triplet,
            CargoDependencyTarget::Expr( ref expr ) => target_cfg.matches( expr )
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct CargoDependency {
    pub name: String,
    pub kind: CargoDependencyKind,
    pub target: Option< CargoDependencyTarget >,
    pub resolved_to: Option< CargoPackageId >
}

#[derive(Debug)]
pub enum Error {
    CannotLaunchCargo( io::Error ),
    CargoFailed( String ),
    CannotParseCargoOutput( serde_json::Error )
}


impl error::Error for Error {
    fn description( &self ) -> &str {
        match *self {
            Error::CannotLaunchCargo( _ ) => "cannot launch cargo",
            Error::CargoFailed( _ ) => "cargo failed",
            Error::CannotParseCargoOutput( _ ) => "cannot parse cargo output"
        }
    }
}

impl fmt::Display for Error {
    fn fmt( &self, formatter: &mut fmt::Formatter ) -> fmt::Result {
        use std::error::Error as StdError;
        match *self {
            Error::CannotLaunchCargo( ref err ) => write!( formatter, "{}: {}", self.description(), err ),
            Error::CargoFailed( ref err ) => write!( formatter, "{}: {}", self.description(), err ),
            Error::CannotParseCargoOutput( ref err ) => write!( formatter, "{}: {}", self.description(), err )
        }
    }
}

impl CargoProject {
    pub fn new(
        manifest_path: Option< &str >,
        no_default_features: bool,
        enable_all_features: bool,
        features: &[String]
    ) -> Result< CargoProject, Error >
    {
        let cwd = env::current_dir().expect( "cannot get current working directory" );
        let cargo = env::var( "CARGO" ).unwrap_or_else( |_|
            if cfg!( windows ) {
                "cargo.exe"
            } else {
                "cargo"
            }.to_owned()
        );

        let mut command = Command::new( cargo );
        command.arg( "metadata" );

        if no_default_features {
            command.arg( "--no-default-features" );
        }

        if enable_all_features {
            command.arg( "--all-features" );
        }

        if !features.is_empty() {
            command.arg( "--features" );
            command.arg( &features.join( " " ) );
        }

        command.arg( "--format-version" );
        command.arg( "1" );

        if let Some( manifest_path ) = manifest_path {
            command.arg( "--manifest-path" );
            command.arg( manifest_path );
        }

        if cfg!( unix ) {
            command.arg( "--color" );
            command.arg( "always" );
        }

        debug!( "Launching: {:?}", command );

        let output = command.output().map_err( |err| Error::CannotLaunchCargo( err ) )?;
        if !output.status.success() {
            return Err( Error::CargoFailed( String::from_utf8_lossy( &output.stderr ).into_owned() ) );
        }
        let metadata = str::from_utf8( &output.stdout ).expect( "cargo output is not valid UTF-8" );
        let metadata: cargo_metadata::Metadata =
            serde_json::from_str( metadata ).map_err( |err| Error::CannotParseCargoOutput( err ) )?;

        let mut workspace_members = HashSet::new();
        for member in metadata.workspace_members {
            workspace_members.insert( member.name().to_owned() );
        }

        let mut project = CargoProject {
            target_directory: metadata.target_directory,
            packages: metadata.packages.into_iter().map( |package| {
                let manifest_path: PathBuf = package.manifest_path.into();
                let is_workspace_member = workspace_members.contains( &*package.name );
                CargoPackage {
                    id: CargoPackageId::new( &package.id ).expect( "unparsable package id" ),
                    name: package.name,
                    crate_root: manifest_path.parent().unwrap().into(),
                    manifest_path: manifest_path,
                    is_workspace_member,
                    is_default: false,
                    targets: package.targets.into_iter().filter_map( |target| {
                        Some( CargoTarget {
                            name: target.name,
                            kind: match target.kind[ 0 ].as_str() {
                                "lib" => TargetKind::Lib,
                                "rlib" => TargetKind::Lib,
                                "cdylib" => TargetKind::CDyLib,
                                "dylib" => TargetKind::Lib,
                                "staticlib" => TargetKind::Lib,
                                "bin" => TargetKind::Bin,
                                "example" => TargetKind::Example,
                                "test" => TargetKind::Test,
                                "bench" => TargetKind::Bench,
                                "custom-build" => return None,
                                "proc-macro" => return None,
                                _ => panic!( "Unknown target kind: '{}'", target.kind[ 0 ] )
                            },
                            source_directory: Into::< PathBuf >::into( target.src_path ).parent().unwrap().into()
                        })
                    }).collect(),
                    dependencies: package.dependencies.into_iter().map( |dependency| {
                        // TODO: Make the `target` field public in `cargo_metadata`.
                        let json: serde_json::Value = serde_json::from_str( &serde_json::to_string( &dependency ).unwrap() ).unwrap();
                        let target = match json.get( "target" ).unwrap() {
                            &serde_json::Value::Null => None,
                            &serde_json::Value::String( ref target ) => {
                                let target = if target.starts_with( "cfg(" ) && target.ends_with( ")" ) {
                                    let cfg = target[ 4..target.len() - 1 ].trim().parse().expect( "cannot parse target specification in a Cargo.toml" );
                                    CargoDependencyTarget::Expr( cfg )
                                } else {
                                    CargoDependencyTarget::Target( target.clone() )
                                };

                                Some( target )
                            },
                            _ => unreachable!()
                        };

                        CargoDependency {
                            name: dependency.name,
                            kind: match dependency.kind {
                                cargo_metadata::DependencyKind::Normal => CargoDependencyKind::Normal,
                                cargo_metadata::DependencyKind::Development => CargoDependencyKind::Development,
                                cargo_metadata::DependencyKind::Build => CargoDependencyKind::Build,
                                other => panic!( "Unknown dependency kind: {:?}", other )
                            },
                            target,
                            resolved_to: None
                        }
                    }).collect()
                }
            }).collect()
        };

        let mut package_map = HashMap::new();
        for (index, package) in project.packages.iter().enumerate() {
            package_map.insert( package.id.clone(), index );
        }

        for node in metadata.resolve.expect( "missing `resolve` metadata section" ).nodes {
            let id = CargoPackageId::new( &node.id ).expect( "unparsable package id in the `resolve` metadata section" );
            let package_index = *package_map.get( &id ).expect( "extra entry in the `resolve` metadata section" );
            let package = &mut project.packages[ package_index ];
            for dependency_id in node.dependencies {
                let dependency_id = CargoPackageId::new( &dependency_id ).expect( "unparsable dependency package id" );

                let mut dependency_found = false;
                for dependency in package.dependencies.iter_mut().filter( |dep| dep.name == dependency_id.name() ) {
                    assert!( dependency.resolved_to.is_none(), "duplicate dependency" );
                    dependency.resolved_to = Some( dependency_id.clone() );
                    dependency_found = true;
                }

                assert!( dependency_found, "dependency missing from packages" );
            }
        }

        let mut default_package: Option< (usize, usize) > = None;
        for (package_index, package) in project.packages.iter().enumerate() {
            if !package.is_workspace_member {
                continue;
            }

            let package_directory = package.manifest_path.parent().unwrap();
            if !cwd.starts_with( package_directory ) {
                continue;
            }

            let common_length = cwd.components().zip( package_directory.components() ).take_while( |&(a, b)| a == b ).count();
            if default_package == None || default_package.unwrap().1 < common_length {
                default_package = Some( (package_index, common_length) );
            }
        }

        if let Some( (default_package_index, _) ) = default_package {
            project.packages[ default_package_index ].is_default = true;
        }

        Ok( project )
    }

    pub fn default_package( &self ) -> Option< &CargoPackage > {
        self.packages.iter().find( |package| package.is_default )
    }

    pub fn used_packages( &self, triplet: &str, main_package: &CargoPackage, profile: Profile ) -> Vec< &CargoPackage > {
        self.used_packages_with_rustflags( triplet, main_package, profile, iter::empty() )
    }

    pub fn used_packages_with_rustflags< 'a, I >(
        &self,
        triplet: &str,
        main_package: &CargoPackage,
        profile: Profile,
        extra_rustflags: I
    ) -> Vec< &CargoPackage >
        where I: IntoIterator< Item = &'a str >
    {
        let mut package_map = HashMap::new();
        for (index, package) in self.packages.iter().enumerate() {
            package_map.insert( package.id.clone(), index );
        }

        struct Entry< 'a > {
            package: &'a CargoPackage,
            is_used: Cell< bool >
        }

        let mut queue = Vec::new();
        let entries: Vec< Entry > = self.packages.iter().enumerate().map( |(index, package)| {
            let is_main_package = package == main_package;
            if is_main_package {
                queue.push( index );
            }

            Entry {
                package,
                is_used: Cell::new( is_main_package )
            }
        }).collect();

        let rustflags = gather_rustflags( extra_rustflags );
        let target_cfg = TargetCfg::new( triplet, &rustflags );
        while let Some( index ) = queue.pop() {
            for dependency in &entries[ index ].package.dependencies {
                if let Some( ref target ) = dependency.target {
                    if !target.matches( &target_cfg ) {
                        continue;
                    }
                }

                match profile {
                    Profile::Main => {
                        match dependency.kind {
                            CargoDependencyKind::Normal => {},
                            CargoDependencyKind::Development |
                            CargoDependencyKind::Build => continue
                        }
                    },
                    Profile::Test |
                    Profile::Bench => {
                        match dependency.kind {
                            CargoDependencyKind::Normal |
                            CargoDependencyKind::Development => {},
                            CargoDependencyKind::Build => continue
                        }
                    }
                }

                let dependency_id = match dependency.resolved_to {
                    Some( ref dependency_id ) => dependency_id,
                    None => continue
                };

                let dependency_index = *package_map.get( dependency_id ).unwrap();
                if entries[ dependency_index ].is_used.get() {
                    continue;
                }

                entries[ dependency_index ].is_used.set( true );
                queue.push( dependency_index );
            }
        }

        entries.into_iter().filter( |entry| entry.is_used.get() ).map( |entry| entry.package ).collect()
    }
}

#[derive(Clone, Debug)]
pub enum BuildTarget {
    Lib( String, Profile ),
    Bin( String, Profile ),
    ExampleBin( String ),
    IntegrationTest( String ),
    IntegrationBench( String )
}

impl BuildTarget {
    fn is_executable( &self ) -> bool {
        match *self {
            BuildTarget::Lib( _, Profile::Main ) => false,
            _ => true
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MessageFormat {
    Human,
    Json,
    #[doc(hidden)]
    __Nonexhaustive,
}

impl FromStr for MessageFormat {
    type Err = super::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "human" => Ok(MessageFormat::Human),
            "json" => Ok(MessageFormat::Json),
            _ => Err(super::Error::ConfigurationError(format!(
                "{} is not a valid message format. Possible values are `human` and `json`.",
                s
            ))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BuildConfig {
    pub build_target: BuildTarget,
    pub build_type: BuildType,
    pub triplet: Option< String >,
    pub package: Option< String >,
    pub features: Vec< String >,
    pub no_default_features: bool,
    pub enable_all_features: bool,
    pub extra_paths: Vec< PathBuf >,
    pub extra_rustflags: Vec< String >,
    pub extra_environment: Vec< (String, String) >,
    pub message_format: MessageFormat,
    pub is_verbose: bool,
    pub use_color: bool
}

fn profile_to_arg( profile: Profile ) -> &'static str {
    match profile {
        Profile::Main => "dev",
        Profile::Test => "test",
        Profile::Bench => "bench"
    }
}

pub fn target_to_build_target( target: &CargoTarget, profile: Profile ) -> BuildTarget {
    match target.kind {
        TargetKind::Lib => BuildTarget::Lib( target.name.clone(), profile ),
        TargetKind::CDyLib => BuildTarget::Lib( target.name.clone(), profile ),
        TargetKind::Bin => BuildTarget::Bin( target.name.clone(), profile ),
        TargetKind::Example => BuildTarget::ExampleBin( target.name.clone() ),
        TargetKind::Test => BuildTarget::IntegrationTest( target.name.clone() ),
        TargetKind::Bench => BuildTarget::IntegrationBench( target.name.clone() )
    }
}

fn gather_rustflags< 'a, I >( extra_rustflags: I ) -> OsString
    where I: IntoIterator< Item = &'a str >
{
    let mut rustflags = OsString::new();
    for flag in extra_rustflags {
        if !rustflags.is_empty() {
            rustflags.push( " " );
        }
        rustflags.push( flag );
    }

    if let Some( env_rustflags ) = env::var_os( "RUSTFLAGS" ) {
        if !rustflags.is_empty() {
            rustflags.push( " " );
        }
        rustflags.push( env_rustflags );
    }

    rustflags
}

impl BuildConfig {
    fn as_command( &self, should_build: bool ) -> Command {
        let mut command = Command::new( "cargo" );
        if should_build {
            command.arg( "rustc" );
        } else {
            command.arg( "check" );
        }

        command.arg( "--message-format" );
        command.arg( "json" );

        if cfg!( unix ) && self.use_color {
            command.arg( "--color" );
            command.arg( "always" );
        }

        if let Some( ref triplet ) = self.triplet {
            command.arg( "--target" ).arg( triplet.as_str() );
        }

        if let Some( ref package ) = self.package {
            command.arg( "--package" ).arg( package.as_str() );
        }

        match self.build_type {
            BuildType::Debug => {},
            BuildType::Release => {
                command.arg( "--release" );
            }
        }

        match self.build_target {
            BuildTarget::Lib( _, _ ) if !should_build => {
                command.arg( "--lib" );
            },
            BuildTarget::Bin( ref name, _ ) if !should_build => {
                command.arg( "--bin" ).arg( name.as_str() );
            },
            BuildTarget::Lib( _, profile ) => {
                command
                    .arg( "--profile" ).arg( profile_to_arg( profile ) )
                    .arg( "--lib" );
            },
            BuildTarget::Bin( ref name, profile ) => {
                command
                    .arg( "--profile" ).arg( profile_to_arg( profile ) )
                    .arg( "--bin" ).arg( name.as_str() );
            },
            BuildTarget::ExampleBin( ref name ) => {
                command.arg( "--example" ).arg( name.as_str() );
            },
            BuildTarget::IntegrationTest( ref name ) => {
                command.arg( "--test" ).arg( name.as_str() );
            },
            BuildTarget::IntegrationBench( ref name ) => {
                command.arg( "--bench" ).arg( name.as_str() );
            }
        }

        if self.no_default_features {
            command.arg( "--no-default-features" );
        }

        if self.enable_all_features {
            command.arg( "--all-features" );
        }

        if !self.features.is_empty() {
            command.arg( "--features" );
            command.arg( &self.features.join( " " ) );
        }

        if self.is_verbose {
            command.arg( "--verbose" );
        }

        command
    }

    pub fn check( &self ) -> CargoResult {
        let status = self.launch_cargo( false ).map( |(status, _)| status );
        CargoResult {
            status,
            artifacts: Vec::new()
        }
    }

    pub fn build< F >( &self, mut postprocess: Option< F > ) -> CargoResult
        where F: for <'a> FnMut( Vec< PathBuf > ) -> Vec< PathBuf >
    {
        let mut result = self.build_internal( &mut postprocess );
        if result.is_ok() == false {
            return result;
        }

        // HACK: For some reason when you install emscripten for the first time
        // the first build is always a dud (it produces no artifacts), so we retry once.
        let is_emscripten = self.triplet.as_ref().map( |triplet| {
            triplet == "wasm32-unknown-emscripten" || triplet == "asmjs-unknown-emscripten"
        }).unwrap_or( false );

        if is_emscripten && self.build_target.is_executable() {
            let no_js_generated = result
                .artifacts()
                .iter()
                .find( |artifact| artifact.extension().map( |ext| ext == "js" ).unwrap_or( false ) )
                .is_none();

            if no_js_generated {
                debug!( "No artifacts were generated yet build succeeded; retrying..." );
                result = self.build_internal( &mut postprocess );
            }
        }

        return result;
    }

    fn launch_cargo( &self, should_build: bool ) -> Option< (i32, Vec< cargo_output::Artifact >) > {
        let mut command = self.as_command( should_build );

        let env_paths = env::var_os( "PATH" )
            .map( |paths| env::split_paths( &paths ).collect() )
            .unwrap_or( Vec::new() );

        let mut paths = Vec::new();
        paths.extend( self.extra_paths.clone().into_iter() );
        paths.extend( env_paths.into_iter() );

        let new_paths = env::join_paths( paths ).unwrap();
        debug!( "Will launch cargo with PATH: {:?}", new_paths );
        command.env( "PATH", new_paths );

        let rustflags = gather_rustflags( self.extra_rustflags.iter().map( |flag| flag.as_str() ) );
        debug!( "Will launch cargo with RUSTFLAGS: {:?}", rustflags );
        command.env( "RUSTFLAGS", rustflags );

        for &(ref key, ref value) in &self.extra_environment {
            debug!( "Will launch cargo with variable \"{}\" set to \"{}\"", key, value );
            command.env( key, value );
        }

        command.stdout( Stdio::piped() );
        command.stderr( Stdio::piped() );

        debug!( "Launching cargo: {:?}", command );
        let mut child = match command.spawn() {
            Ok( child ) => child,
            Err( _ ) => return None
        };

        let stderr = BufReader::new( child.stderr.take().unwrap() );
        let stdout = BufReader::new( child.stdout.take().unwrap() );

        let is_verbose = self.is_verbose;
        thread::spawn( move || {
            let mut skip = 0;
            for line in stderr.lines() {
                let line = match line {
                    Ok( line ) => line,
                    Err( _ ) => break
                };

                if skip > 0 {
                    skip -= 1;
                    continue;
                }

                // This is really ugly, so let's skip it.
                if line.trim() == "Caused by:" && !is_verbose {
                    skip += 1;
                    continue;
                }

                eprintln!( "{}", line );
            }
        });

        let mut artifacts = Vec::new();
        for line in stdout.lines() {
            let line = match line {
                Ok( line ) => line,
                Err( _ ) => break
            };

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some( output ) = CargoOutput::parse( line ) {
                match output {
                    CargoOutput::Message( message ) => {
                        match self.message_format {
                            MessageFormat::Human => diagnostic_formatter::print( self.use_color, &message ),
                            MessageFormat::Json => {
                                println!( "{}", serde_json::to_string( &message.to_json_value() ).unwrap() );
                            }
                            MessageFormat::__Nonexhaustive => unreachable!(),
                        }
                    },
                    CargoOutput::Artifact( artifact ) => {
                        for filename in &artifact.filenames {
                            debug!( "Built artifact: {}", filename );
                        }

                        artifacts.push( artifact );
                    },
                    CargoOutput::BuildScriptExecuted( executed ) => {
                        match self.message_format {
                            MessageFormat::Human => {},
                            MessageFormat::Json => {
                                println!( "{}", serde_json::to_string( &executed.to_json_value() ).unwrap() );
                            }
                            MessageFormat::__Nonexhaustive => unreachable!(),
                        }
                    }
                }
            }
        }

        let result = child.wait();
        let status = result.unwrap().code().expect( "failed to grab cargo status code" );
        debug!( "Cargo finished with status: {}", status );

        Some( (status, artifacts) )
    }

    fn build_internal< F >( &self, postprocess: &mut Option< F > ) -> CargoResult
        where F: for <'a> FnMut( Vec< PathBuf > ) -> Vec< PathBuf >
    {
        let (status, mut artifacts) = match self.launch_cargo( true ) {
            Some( result ) => result,
            None => {
                return CargoResult {
                    status: None,
                    artifacts: Vec::new()
                }
            }
        };

        fn has_extension< P: AsRef< Path > >( path: P, extension: &str ) -> bool {
            path.as_ref().extension().map( |ext| ext == extension ).unwrap_or( false )
        }

        fn find_artifact( artifacts: &[cargo_output::Artifact], extension: &str ) -> Option< (usize, usize) > {
            artifacts.iter().enumerate().filter_map( |(artifact_index, artifact)| {
                if let Some( filename_index ) = artifact.filenames.iter().position( |filename| has_extension( filename, extension ) ) {
                    Some( (artifact_index, filename_index) )
                } else {
                    None
                }
            }).next()
        }

        // For some reason when building tests cargo doesn't treat
        // the `.wasm` file as an artifact.
        if status == 0 && self.triplet.as_ref().map( |triplet| triplet == "wasm32-unknown-emscripten" ).unwrap_or( false ) {
            match self.build_target {
                BuildTarget::Bin( _, Profile::Test ) |
                BuildTarget::Lib( _, Profile::Test ) |
                BuildTarget::IntegrationTest( _ ) => {
                    if find_artifact( &artifacts, "wasm" ).is_none() {
                        if let Some( (artifact_index, filename_index) ) = find_artifact( &artifacts, "js" ) {
                            let wasm_path = {
                                let main_artifact = Path::new( &artifacts[ artifact_index ].filenames[ filename_index ] );
                                let filename = main_artifact.file_name().unwrap();
                                main_artifact.parent().unwrap().join( "deps" ).join( filename ).with_extension( "wasm" )
                            };

                            assert!( wasm_path.exists(), "internal error: wasm doesn't exist where I expected it to be" );
                            artifacts[ artifact_index ].filenames.push( wasm_path.to_str().unwrap().to_owned() );
                            debug!( "Found `.wasm` test artifact: {:?}", wasm_path );
                        }
                    }
                },
                _ => {}
            }
        }

        let mut artifact_paths = Vec::new();
        for mut artifact in &mut artifacts {
            if let Some( ref mut callback ) = postprocess.as_mut() {
                let filenames = artifact.filenames.iter().map( |filename| Path::new( &filename ).to_owned() ).collect();
                let filenames = callback( filenames );
                artifact.filenames = filenames.into_iter().map( |filename| filename.to_str().unwrap().to_owned() ).collect();
            }
        }

        for mut artifact in artifacts {
            if artifact.filenames.is_empty() {
                continue;
            }

            match self.message_format {
                MessageFormat::Human => {},
                MessageFormat::Json => {
                    println!( "{}", serde_json::to_string( &artifact.to_json_value() ).unwrap() );
                }
                MessageFormat::__Nonexhaustive => unreachable!(),
            }

            for filename in artifact.filenames {
                // NOTE: Since we extract the paths from the JSON
                //       we get a list of artifacts as `String`s instead of `PathBuf`s.
                artifact_paths.push( filename.into() )
            }
        }

        CargoResult {
            status: Some( status ),
            artifacts: artifact_paths
        }
    }
}

#[derive(Debug)]
pub struct CargoResult {
    status: Option< i32 >,
    artifacts: Vec< PathBuf >
}

impl CargoResult {
    pub fn is_ok( &self ) -> bool {
        self.status == Some( 0 )
    }

    pub fn artifacts( &self ) -> &[PathBuf] {
        &self.artifacts
    }
}
