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
    pub targets: Vec< CargoTarget >
}

#[derive(Clone, Debug)]
pub struct CargoTarget {
    pub name: String,
    pub kind: TargetKind,
    pub source_directory: PathBuf
}

impl CargoProject {
    pub fn new( manifest_path: Option< &str > ) -> CargoProject {
        let metadata = cargo_metadata::metadata( manifest_path.map( |path| Path::new( path ) ) ).unwrap();
        CargoProject {
            packages: metadata.packages.into_iter().map( |package| {
                let manifest_path: PathBuf = package.manifest_path.into();
                CargoPackage {
                    name: package.name,
                    crate_root: manifest_path.parent().unwrap().into(),
                    manifest_path: manifest_path,
                    targets: package.targets.into_iter().filter_map( |target| {
                        Some( CargoTarget {
                            name: target.name,
                            kind: match target.kind[ 0 ].as_str() {
                                "lib" => TargetKind::Lib,
                                "bin" => TargetKind::Bin,
                                "example" => TargetKind::Example,
                                "test" => TargetKind::Test,
                                "bench" => TargetKind::Bench,
                                "custom-build" => return None,
                                _ => panic!( "Unknown target kind: '{}'", target.kind[ 0 ] )
                            },
                            source_directory: Into::< PathBuf >::into( target.src_path ).parent().unwrap().into()
                        })
                    }).collect()
                }
            }).collect()
        }
    }

    pub fn default_package( &self ) -> &CargoPackage {
        &self.packages[ 0 ]
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
    pub extra_environment: Vec< (String, String) >
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
        command.arg( "--color" );
        command.arg( "always" );

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

        command
    }

    pub fn build( &self ) -> CargoResult {
        let mut result = self.build_internal();
        if result.is_ok() == false {
            return result;
        }

        // HACK: For some reason when you install emscripten for the first time
        // the first build is always a dud (it produces no artifacts), so we retry once.
        let is_emscripten = self.triplet.as_ref().map( |triplet| {
            triplet == "wasm32-unknown-emscripten" || triplet == "asmjs-unknown-emscripten"
        }).unwrap_or( false );

        if is_emscripten {
            let no_js_generated = result
                .artifacts()
                .iter()
                .find( |artifact| artifact.extension().map( |ext| ext == "js" ).unwrap_or( false ) )
                .is_none();

            if no_js_generated {
                debug!( "No artifacts were generated yet build succeeded; retrying..." );
                result = self.build_internal();
            }
        }

        return result;
    }

    fn build_internal( &self ) -> CargoResult {
        let mut command = self.as_command();

        let mut paths = env::var_os( "PATH" )
            .map( |paths| env::split_paths( &paths ).collect() )
            .unwrap_or( Vec::new() );

        for path in &self.extra_paths {
            paths.push( path.into() );
        }

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
                if line.trim() == "Caused by:" {
                    skip += 1;
                    continue;
                }

                eprintln!( "{}", line );
            }
        });

        let mut artifacts: Vec< PathBuf > = Vec::new();
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
                        diagnostic_formatter::print( &message );
                    },
                    CargoOutput::Artifact( artifact ) => {
                        for filename in artifact.filenames {
                            debug!( "Built artifact: {}", filename );
                            // NOTE: Since we extract the paths from the JSON
                            //       we get a list of artifacts as `String`s instead of `PathBuf`s.
                            artifacts.push( filename.into() );
                        }
                    },
                    _ => {}
                }
            }
        }

        let result = child.wait();
        let status = result.unwrap().code().expect( "failed to grab cargo status code" );
        debug!( "Cargo finished with status: {}", status );

        // For some reason when building tests cargo doesn't treat
        // the `.wasm` file as an artifact.
        if status == 0 && self.triplet.as_ref().map( |triplet| triplet == "wasm32-unknown-emscripten" ).unwrap_or( false ) {
            match self.build_target {
                BuildTarget::Bin( _, Profile::Test ) | BuildTarget::Lib( _, Profile::Test ) => {
                    let wasm_path = {
                        let main_artifact = artifacts.iter()
                            .find( |artifact| artifact.extension().map( |ext| ext == "js" ).unwrap_or( false ) );

                        if let Some( main_artifact ) = main_artifact {
                            let filename = main_artifact.file_name().unwrap();
                            let wasm_path = main_artifact.parent().unwrap().join( "deps" ).join( filename ).with_extension( "wasm" );
                            assert!( wasm_path.exists(), "internal error: wasm doesn't exist where I expected it to be" );

                            Some( wasm_path )
                        } else {
                            None
                        }
                    };

                    if let Some( wasm_path ) = wasm_path {
                        artifacts.push( wasm_path );
                    }
                },
                _ => {}
            }
        }

        CargoResult {
            status: Some( status ),
            artifacts
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

    pub fn add_artifact< P: AsRef< Path > >( &mut self, artifact: P ) {
        self.artifacts.push( artifact.as_ref().to_owned() );
    }
}
