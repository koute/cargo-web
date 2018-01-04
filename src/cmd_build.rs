use clap;

use cargo_shim::{
    Profile,
    CargoProject,
    TargetKind
};

use build::BuildArgsMatcher;
use config::Config;
use error::Error;

pub fn command_build< 'a >( matches: &clap::ArgMatches< 'a >, project: &CargoProject ) -> Result< (), Error > {
    let build_matcher = BuildArgsMatcher {
        matches: matches,
        project: project
    };

    let package = build_matcher.package_or_default()?;
    let config = Config::load_for_package_printing_warnings( &package ).unwrap().unwrap_or_default();
    let targets = build_matcher.target_or_select( package, |target| {
        target.kind == TargetKind::Lib || target.kind == TargetKind::Bin
    })?;

    for target in targets {
        let builder = build_matcher.prepare_builder( &config, package, target, Profile::Main );
        builder.run()?;
    }

    Ok(())
}
