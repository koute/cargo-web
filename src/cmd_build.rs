use cargo_shim::{
    Profile,
    TargetKind
};

use build::BuildArgs;
use error::Error;

pub fn command_build_or_check(build_args: BuildArgs, should_build: bool) -> Result<(), Error> {
    let project = build_args.load_project()?;

    let targets = project.target_or_select( |target| {
        target.kind == TargetKind::Lib || target.kind == TargetKind::CDyLib || target.kind == TargetKind::Bin
    })?;

    let config = project.aggregate_configuration( Profile::Main )?;
    for target in targets {
        if should_build {
            project.build( &config, target )?;
        } else {
            project.check( &config, target )?;
        }
    }

    Ok(())
}

pub fn command_build(args: BuildArgs) -> Result<(), Error> {
    command_build_or_check(args, true)
}

pub fn command_check(args: BuildArgs) -> Result<(), Error> {
    command_build_or_check(args, false)
}
