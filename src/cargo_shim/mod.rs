use std::process::Command;
use std::path::{Path, PathBuf};

use cargo_metadata;
use regex::Regex;

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
        let metadata = cargo_metadata::metadata( manifest_path ).unwrap();
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
    pub enable_all_features: bool
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
    pub fn as_command( &self ) -> Command {
        let mut command = Command::new( "cargo" );
        command.arg( "rustc" );

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

    pub fn output_directory< P: AsRef< Path > >( &self, crate_root: P ) -> PathBuf {
        let crate_root = crate_root.as_ref();
        let mut directory = crate_root.join( "target" );
        if let Some( ref triplet ) = self.triplet {
            directory = directory.join( &triplet );
        }

        directory = match self.build_type {
            BuildType::Debug => directory.join( "debug" ),
            BuildType::Release => directory.join( "release" )
        };

        match self.build_target {
            BuildTarget::ExampleBin( _ ) => directory.join( "examples" ),
            _ => directory
        }
    }

    // Ugh... this is really dumb, and probably buggy, but Cargo doesn't support a seemingly fundamental
    // feature like being able to tell us what exactly it generated, so we have to guess. Hopefully
    // it will get fixed eventually.
    pub fn potential_artifacts< P: AsRef< Path > >( &self, crate_root: P ) -> Vec< PathBuf > {
        let mut matchers = Vec::new();

        macro_rules! matcher {
            ($regex:expr) => {
                matchers.push( Regex::new( $regex.as_ref() ).unwrap() )
            }
        };

        macro_rules! prefix_matcher {
            ($prefix:expr) => {
                matcher!( format!( "^{}-", $prefix.replace("-", "_") ) )
            }
        };

        let is_web_target = self.triplet.as_ref().map( |triplet| {
            let triplet = triplet.as_str();
            triplet == "asmjs-unknown-emscripten" ||
            triplet == "wasm32-unknown-emscripten"
        }).unwrap_or( false );

        if is_web_target {
            matcher!( "\\.js$" );
        } if cfg!( target_os = "windows" ) {
            match self.build_target {
                BuildTarget::Lib( _, Profile::Main ) => matcher!( "\\.(lib|dll)$" ),
                _ => {}
            }
        } if cfg!( target_os = "linux" ) {
            match self.build_target {
                BuildTarget::Lib( _, Profile::Main ) => matcher!( "\\.(a|so)$" ),
                _ => {}
            }
        }

        match self.build_target {
            BuildTarget::Lib( _, Profile::Main ) => {},
            BuildTarget::Lib( ref name, Profile::Test ) => prefix_matcher!( name ),
            BuildTarget::Lib( ref name, Profile::Bench ) => prefix_matcher!( name ),
            BuildTarget::Bin( ref name, profile ) => {
                match profile {
                    Profile::Main => matcher!( format!( "^{}(\\.|$)", name ) ),
                    Profile::Test | Profile::Bench => prefix_matcher!( name )
                }
            },
            BuildTarget::ExampleBin( ref name ) => matcher!( format!( "^{}(\\.|$)", name ) ),
            BuildTarget::IntegrationTest( ref name ) => prefix_matcher!( name ),
            BuildTarget::IntegrationBench( ref name ) => prefix_matcher!( name )
        }

        let crate_root = crate_root.as_ref();
        let output_directory = self.output_directory( crate_root );

        let mut output = Vec::new();
        let output_directory_iter = match output_directory.read_dir() {
            Ok( iter ) => iter,
            Err( _ ) => return output
        };

        for entry in output_directory_iter {
            let entry = entry.unwrap();
            let filename = entry.file_name();
            let filename = filename.to_string_lossy().into_owned();

            if !matchers.iter().all( |regex| regex.is_match( filename.as_str() ) ) {
                continue;
            }

            output.push( output_directory.join( filename ) );
        }

        output
    }
}
