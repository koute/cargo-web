use clap;

use cargo_shim::{
    Profile,
    TargetKind
};

use build::BuildArgsMatcher;
use error::Error;

pub fn command_build< 'a >( matches: &clap::ArgMatches< 'a > ) -> Result< (), Error > {
    let build_matcher = BuildArgsMatcher::new( matches );

    let package = build_matcher.package_or_default()?;
    let config = build_matcher.aggregate_configuration( package, Profile::Main )?;
    let targets = build_matcher.target_or_select( package, |target| {
        target.kind == TargetKind::Lib || target.kind == TargetKind::Bin
    })?;

    for target in targets {
        let builder = build_matcher.prepare_builder( &config, package, target, Profile::Main );
        builder.run()?;
    }

    Ok(())
}
