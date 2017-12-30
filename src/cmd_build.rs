use clap;

use cargo_shim::{
    Profile,
    CargoProject,
    TargetKind
};

use build::{
    BuildArgsMatcher,
    set_rust_flags,
    run_with_broken_first_build_hack
};
use config::Config;
use error::Error;
use emscripten::check_for_emcc;
use utils::CommandExt;

pub fn command_build< 'a >( matches: &clap::ArgMatches< 'a >, project: &CargoProject ) -> Result< (), Error > {
    let use_system_emscripten = matches.is_present( "use-system-emscripten" );
    let targeting_webasm = matches.is_present( "target-webasm-emscripten" ) || matches.is_present( "target-webasm" );
    let extra_path = if matches.is_present( "target-webasm" ) { None } else { check_for_emcc( use_system_emscripten, targeting_webasm ) };

    let build_matcher = BuildArgsMatcher {
        matches: matches,
        project: project
    };

    let package = build_matcher.package_or_default()?;
    let config = Config::load_for_package_printing_warnings( &package ).unwrap().unwrap_or_default();
    set_rust_flags( &config, &build_matcher );

    let targets = build_matcher.target_or_select( package, |target| {
        target.kind == TargetKind::Lib || target.kind == TargetKind::Bin
    })?;

    for target in targets {
        let build_config = build_matcher.build_config( package, target, Profile::Main );
        let mut command = build_config.as_command();
        if let Some( ref extra_path ) = extra_path {
            command.append_to_path( extra_path );
        }

        run_with_broken_first_build_hack( package, &build_config, &mut command )?;
    }

    Ok(())
}
