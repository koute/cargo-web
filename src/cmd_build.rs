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

    let targets = project.target_or_select( |target| {
        target.kind == TargetKind::Lib || target.kind == TargetKind::CDyLib || target.kind == TargetKind::Bin
    })?;

    let config = project.aggregate_configuration( Profile::Main )?;
    for target in targets {
        project.build( &config, target )?;
    }

    Ok(())
}
