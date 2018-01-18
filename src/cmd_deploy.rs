use std::fs;

use clap;

use cargo_shim::{
    Profile,
    TargetKind
};

use build::BuildArgs;
use deployment::Deployment;
use error::Error;

pub fn command_deploy< 'a >( matches: &clap::ArgMatches< 'a > ) -> Result< (), Error > {
    let build_args = BuildArgs::new( matches )?;
    let project = build_args.load_project()?;

    let package = project.package();
    let targets = project.target_or_select( None, |target| {
        target.kind == TargetKind::Bin
    })?;

    let config = project.aggregate_configuration( package, Profile::Main )?;
    let target = targets[ 0 ];
    let result = project.build( &config, package, target )?;

    let deployment = Deployment::new( package, target, &result )?;
    let directory = package.crate_root.join( "target" ).join( "deploy" );
    match fs::remove_dir_all( &directory ) {
        Ok(()) => {},
        Err( error ) => {
            if directory.exists() {
                return Err( Error::CannotRemoveDirectory( directory, error ) );
            }
        }
    }

    deployment.deploy_to( &directory )?;

    eprintln!( "The `{}` was deployed to {:?}!", target.name, directory );
    Ok(())
}
