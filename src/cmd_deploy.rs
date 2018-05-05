use std::fs;

use clap;

use cargo_shim::{
    Profile,
    TargetKind
};

use build::BuildArgs;
use deployment::{ Deployment, DeployWithServePath};
use error::Error;

pub fn command_deploy< 'a >( matches: &clap::ArgMatches< 'a > ) -> Result< (), Error > {
    let build_args = BuildArgs::new( matches )?;
    let project = build_args.load_project()?;

    let package = project.package();
    let targets = project.target_or_select( |target| {
        target.kind == TargetKind::Bin ||
        (target.kind == TargetKind::CDyLib && project.backend().is_native_wasm())
    })?;

    let config = project.aggregate_configuration( Profile::Main )?;
    let target = targets[ 0 ];
    let result = project.build( &config, target )?;

    let target_config = match project.config_of_default_target() {
        Some(config) => config.clone(),
        None => ::config::PerTargetConfig::default()
    };
    let deploy_options = DeployWithServePath::new( &target_config.serve_path )?;

    let deployment = Deployment::new( package, target, &result, Some(deploy_options) )?;
    let directory = if let Some(ref deploy_path) = target_config.deploy_path {
        // Resolve deploy_path to the actual folder on filesystem, relative to crate_root
        // The path must exist
        package.crate_root.join( deploy_path ).canonicalize()
            .map_err( |error| Error::ConfigurationError(
                format!( "Deploy path '{}' is invalid: {}", deploy_path, error)
            ))?
    } else {
        project.target_directory().join( "deploy" )
    };
    if directory.exists() {
        let entries = fs::read_dir( &directory ).map_err( |error| Error::CannotRemoveDirectory( directory.clone(), error ) )?; // TODO: Another error?
        for entry in entries {
            let entry = entry.expect( "cannot unwrap directory entry" );
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all( &path ).map_err( |error| Error::CannotRemoveDirectory( path.clone(), error ) )?;
            } else {
                fs::remove_file( &path ).map_err( |error| Error::CannotRemoveFile( path.clone(), error ) )?;
            }
        }
    }

    deployment.deploy_to( &directory )?;

    eprintln!( "The `{}` was deployed to {:?}!", target.name, directory );
    Ok(())
}
