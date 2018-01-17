use clap;

use cargo_shim::{
    Profile,
    TargetKind
};

use build::BuildArgs;
use error::Error;

pub fn command_build< 'a >( matches: &clap::ArgMatches< 'a > ) -> Result< (), Error > {
    let build_args = BuildArgs::new( matches )?;
    let project = build_args.load_project()?;

    let package = project.package();
    let targets = project.target_or_select( None, |target| {
        target.kind == TargetKind::Lib || target.kind == TargetKind::Bin
    })?;

    let config = project.aggregate_configuration( package, Profile::Main )?;
    for target in targets {
        project.build( &config, package, target )?;
    }

    Ok(())
}
