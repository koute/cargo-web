use std::process::{Command, exit};
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
    target_to_build_target
};

use config::Config;
use error::Error;
use utils::CommandExt;
use wasm;

pub struct BuildArgsMatcher< 'a > {
    pub matches: &'a clap::ArgMatches< 'a >,
    pub project: &'a CargoProject
}

impl< 'a > BuildArgsMatcher< 'a > {
    fn build_type( &self ) -> BuildType {
        let build_type = if self.matches.is_present( "release" ) {
            BuildType::Release
        } else {
            BuildType::Debug
        };

        if self.matches.is_present( "target-webasm" ) && build_type == BuildType::Debug {
            // TODO: Remove this in the future.
            println_err!( "warning: debug builds on the wasm-unknown-unknown are currently totally broken" );
            println_err!( "         forcing a release build" );
            return BuildType::Release;
        }

        build_type
    }

    fn package( &self ) -> Result< Option< &CargoPackage >, Error > {
        if let Some( name ) = self.matches.value_of( "package" ) {
            match self.project.packages.iter().find( |package| package.name == name ) {
                None => Err( Error::ConfigurationError( format!( "package `{}` not found", name ) ) ),
                package => Ok( package )
            }
        } else {
            Ok( None )
        }
    }

    pub fn package_or_default( &self ) -> Result< &CargoPackage, Error > {
        Ok( self.package()?.unwrap_or_else( || self.project.default_package() ) )
    }

    fn target( &'a self, package: &'a CargoPackage ) -> Result< Option< &'a CargoTarget >, Error > {
        let targets = &package.targets;
        if self.matches.is_present( "lib" ) {
            match targets.iter().find( |target| target.kind == TargetKind::Lib ) {
                None => return Err( Error::ConfigurationError( format!( "no library targets found" ) ) ),
                target => Ok( target )
            }
        } else if let Some( name ) = self.matches.value_of( "bin" ) {
            match targets.iter().find( |target| target.kind == TargetKind::Bin && target.name == name ) {
                None => return Err( Error::ConfigurationError( format!( "no bin target named `{}`", name ) ) ),
                target => Ok( target )
            }
        } else if let Some( name ) = self.matches.value_of( "example" ) {
            match targets.iter().find( |target| target.kind == TargetKind::Example && target.name == name ) {
                None => return Err( Error::ConfigurationError( format!( "no example target named `{}`", name ) ) ),
                target => Ok( target )
            }
        } else if let Some( name ) = self.matches.value_of( "bench" ) {
            match targets.iter().find( |target| target.kind == TargetKind::Bench && target.name == name ) {
                None => return Err( Error::ConfigurationError( format!( "no bench target named `{}`", name ) ) ),
                target => Ok( target )
            }
        } else {
            Ok( None )
        }
    }

    pub fn target_or_select< F >( &'a self, package: &'a CargoPackage, filter: F ) -> Result< Vec< &'a CargoTarget >, Error >
        where for< 'r > F: Fn( &'r CargoTarget ) -> bool
    {
        Ok( self.target( package )?.map( |target| vec![ target ] ).unwrap_or_else( || {
            package.targets.iter().filter( |target| filter( target ) ).collect()
        }))
    }

    fn triplet_or_default( &self ) -> &str {
        if self.matches.is_present( "target-webasm") {
            "wasm32-unknown-unknown"
        } else if self.matches.is_present( "target-webasm-emscripten" ) {
            "wasm32-unknown-emscripten"
        } else {
            "asmjs-unknown-emscripten"
        }
    }

    pub fn build_config( &self, package: &CargoPackage, target: &CargoTarget, profile: Profile ) -> BuildConfig {
        BuildConfig {
            build_target: target_to_build_target( target, profile ),
            build_type: self.build_type(),
            triplet: Some( self.triplet_or_default().into() ),
            package: Some( package.name.clone() )
        }
    }
}

pub fn run_with_broken_first_build_hack( package: &CargoPackage, build_config: &BuildConfig, command: &mut Command ) -> Result< (), Error > {
    if command.run().is_ok() == false {
        return Err( Error::BuildError );
    }

    let artifacts = build_config.potential_artifacts( &package.crate_root );
    wasm::process_wasm_files( build_config, &artifacts );

    // HACK: For some reason when you install emscripten for the first time
    // the first build is always a dud (it produces no artifacts), so we do this.
    if artifacts.is_empty() {
        if command.run().is_ok() == false {
            return Err( Error::BuildError );
        }
    }

    Ok(())
}

pub fn set_link_args( config: &Config ) {
    if let Some( ref link_args ) = config.link_args {
        let mut rustflags = String::new();
        if let Ok( flags ) = env::var( "RUSTFLAGS" ) {
            rustflags.push_str( flags.as_str() );
            rustflags.push_str( " " );
        }

        for arg in link_args {
            if arg.contains( " " ) {
                // Not sure how to handle spaces, as `-C link-arg="{}"` doesn't work.
                println_err!( "error: you have a space in one of the entries in `link-args` in your `Web.toml`;" );
                println_err!( "       this is currently unsupported - aborting!" );
                exit( 101 );
            }
            rustflags.push_str( format!( "-C link-arg={} ", arg ).as_str() );
        }

        env::set_var( "RUSTFLAGS", rustflags.trim() );
    }
}
