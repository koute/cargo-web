use std::collections::HashSet;
use std::process::{Command, Stdio};
use std::path::{Path, PathBuf};
use std::io::{BufRead, BufReader};
use std::ffi::OsString;
use std::env;
use std::thread;

use cargo_metadata;
use serde_json;

mod cargo_output;
mod rustc_diagnostic;
mod diagnostic_formatter;

use self::cargo_output::CargoOutput;

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
    Bin,
    Example,
    Test,
    Bench
}

#[derive(Clone, Debug)]
pub struct CargoProject {
    pub packages: Vec< CargoPackage >
}

#[derive(Clone, Debug)]
pub struct CargoPackage {
    pub name: String,
    pub manifest_path: PathBuf,
    pub crate_root: PathBuf,
    pub targets: Vec< CargoTarget >,
    pub is_workspace_member: bool,
    pub is_default: bool
}

#[derive(Clone, Debug)]
pub struct CargoTarget {
    pub name: String,
    pub kind: TargetKind,
    pub source_directory: PathBuf
}

impl CargoProject {
    pub fn new( manifest_path: Option< &str > ) -> Result< CargoProject, cargo_metadata::Error > {
        let cwd = env::current_dir().expect( "cannot get current working directory" );

        let metadata = cargo_metadata::metadata_deps( manifest_path.map( |path| Path::new( path ) ), true )?;

        let mut workspace_members = HashSet::new();
        for member in metadata.workspace_members {
            workspace_members.insert( member.name );
        }

        let mut project = CargoProject {
            packages: metadata.packages.into_iter().map( |package| {
                let manifest_path: PathBuf = package.manifest_path.into();
                let is_workspace_member = workspace_members.contains( &package.name );
                CargoPackage {
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
                                "cdylib" => TargetKind::Lib,
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
                    }).collect()
                }
            }).collect()
        };

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

        let default_package_index = default_package
            .expect( "internal error: cannot figure out which package is the default; please report this!" )
            .0;

        project.packages[ default_package_index ].is_default = true;
        Ok( project )
    }

    pub fn default_package( &self ) -> &CargoPackage {
        self.packages.iter().find( |package| package.is_default ).unwrap()
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
    Json
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
    pub is_verbose: bool
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
        TargetKind::Bin => BuildTarget::Bin( target.name.clone(), profile ),
        TargetKind::Example => BuildTarget::ExampleBin( target.name.clone() ),
        TargetKind::Test => BuildTarget::IntegrationTest( target.name.clone() ),
        TargetKind::Bench => BuildTarget::IntegrationBench( target.name.clone() )
    }
}

impl BuildConfig {
    fn as_command( &self ) -> Command {
        let mut command = Command::new( "cargo" );
        command.arg( "rustc" );
        command.arg( "--message-format" );
        command.arg( "json" );

        if cfg!( unix ) {
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

    pub fn build< F >( &self, mut extra_artifact_generator: Option< F > ) -> CargoResult
        where F: for <'a> FnMut( &'a Path ) -> Vec< PathBuf >
    {
        let mut result = self.build_internal( &mut extra_artifact_generator );
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
                result = self.build_internal( &mut extra_artifact_generator );
            }
        }

        return result;
    }

    fn build_internal< F >( &self, extra_artifact_generator: &mut Option< F > ) -> CargoResult
        where F: for <'a> FnMut( &'a Path ) -> Vec< PathBuf >
    {
        let mut command = self.as_command();

        let env_paths = env::var_os( "PATH" )
            .map( |paths| env::split_paths( &paths ).collect() )
            .unwrap_or( Vec::new() );

        let mut paths = Vec::new();
        paths.extend( self.extra_paths.clone().into_iter() );
        paths.extend( env_paths.into_iter() );

        let new_paths = env::join_paths( paths ).unwrap();
        debug!( "Will launch cargo with PATH: {:?}", new_paths );
        command.env( "PATH", new_paths );

        let mut rustflags = env::var_os( "RUSTFLAGS" ).unwrap_or( OsString::new() );
        for flag in &self.extra_rustflags {
            if !rustflags.is_empty() {
                rustflags.push( " " );
            }
            rustflags.push( flag );
        }
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
            Err( _ ) => {
                return CargoResult {
                    status: None,
                    artifacts: Vec::new()
                };
            }
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

            let json: serde_json::Value = serde_json::from_str( &line ).expect( "failed to parse cargo output" );
            let line = serde_json::to_string_pretty( &json ).unwrap();
            if let Some( output ) = CargoOutput::parse( &line ) {
                match output {
                    CargoOutput::Message( message ) => {
                        match self.message_format {
                            MessageFormat::Human => diagnostic_formatter::print( &message ),
                            MessageFormat::Json => {
                                println!( "{}", serde_json::to_string( &message.to_json_value() ).unwrap() );
                            }
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
                        }
                    }
                }
            }
        }

        let result = child.wait();
        let status = result.unwrap().code().expect( "failed to grab cargo status code" );
        debug!( "Cargo finished with status: {}", status );

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
                BuildTarget::Bin( _, Profile::Test ) | BuildTarget::Lib( _, Profile::Test ) => {
                    if find_artifact( &artifacts, "wasm" ).is_none() {
                        if let Some( (artifact_index, filename_index) ) = find_artifact( &artifacts, "js" ) {
                            let wasm_path = {
                                let main_artifact = Path::new( &artifacts[ artifact_index ].filenames[ filename_index ] );
                                let filename = main_artifact.file_name().unwrap();
                                main_artifact.parent().unwrap().join( "deps" ).join( filename ).with_extension( "wasm" )
                            };

                            assert!( wasm_path.exists(), "internal error: wasm doesn't exist where I expected it to be" );
                            artifacts[ artifact_index ].filenames.push( wasm_path.to_str().unwrap().to_owned() );
                        }
                    }
                },
                _ => {}
            }
        }

        let mut artifact_paths = Vec::new();
        for mut artifact in artifacts {
            if let Some( ref mut callback ) = extra_artifact_generator.as_mut() {
                let mut extra_filenames = Vec::new();
                for filename in &artifact.filenames {
                    extra_filenames.extend(
                        callback( Path::new( &filename ) ).into_iter().map( |artifact| artifact.to_str().unwrap().to_owned() )
                    );
                }
                artifact.filenames.extend( extra_filenames );
            }

            match self.message_format {
                MessageFormat::Human => {},
                MessageFormat::Json => {
                    println!( "{}", serde_json::to_string( &artifact.to_json_value() ).unwrap() );
                }
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
