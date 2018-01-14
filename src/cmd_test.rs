use std::process::{Command, exit};
use std::iter;
use std::env;
use std::ffi::OsStr;

use clap;

use cargo_shim::{
    Profile,
    CargoResult,
    TargetKind
};

use build::BuildArgsMatcher;
use error::Error;
use utils::{
    CommandExt,
    find_cmd
};
use test_chromium::test_in_chromium;

fn test_in_nodejs(
    build_matcher: &BuildArgsMatcher,
    build: CargoResult,
    arg_passthrough: &Vec< &OsStr >,
    any_failure: &mut bool
) -> Result< (), Error > {

    let possible_commands =
        if cfg!( windows ) {
            &[ "node.exe" ][..]
        } else {
            &[ "nodejs", "node" ][..]
        };

    let nodejs_name = find_cmd( possible_commands ).ok_or_else( || {
        Error::EnvironmentError( "node.js not found; please install it!".into() )
    })?;

    let artifact = build.artifacts().iter()
        .find( |artifact| artifact.extension().map( |ext| ext == "js" ).unwrap_or( false ) )
        .expect( "internal error: no .js file found" );

    let test_args = iter::once( artifact.as_os_str() )
        .chain( arg_passthrough.iter().cloned() );

    let previous_cwd = env::current_dir().unwrap();
    if build_matcher.targeting_emscripten_wasm() {
        // On the Emscripten target the `.wasm` file is in a different directory.
        let wasm_artifact = build.artifacts().iter()
            .find( |artifact| artifact.extension().map( |ext| ext == "wasm" ).unwrap_or( false ) )
            .expect( "internal error: no .wasm file found" );

        env::set_current_dir( wasm_artifact.parent().unwrap() ).unwrap();
    } else {
        env::set_current_dir( artifact.parent().unwrap() ).unwrap();
    }

    let status = Command::new( nodejs_name ).args( test_args ).run();
    *any_failure = *any_failure || !status.is_ok();

    env::set_current_dir( previous_cwd ).unwrap();

    Ok(())
}

pub fn command_test< 'a >( matches: &clap::ArgMatches< 'a > ) -> Result< (), Error > {
    let build_matcher = BuildArgsMatcher::new( matches );

    let use_nodejs = matches.is_present( "nodejs" );
    let no_run = matches.is_present( "no-run" );
    if build_matcher.targeting_native_wasm() && !use_nodejs {
        return Err( Error::ConfigurationError( "running tests for the native wasm target is currently only supported with `--nodejs`".into() ) );
    }

    let arg_passthrough = matches.values_of_os( "passthrough" )
        .map_or( vec![], |args| args.collect() );

    let package = build_matcher.package_or_default()?;
    let config = build_matcher.aggregate_configuration( package, Profile::Test )?;
    let targets = build_matcher.target_or_select( package, |target| {
        target.kind == TargetKind::Lib || target.kind == TargetKind::Bin || target.kind == TargetKind::Test
    })?;

    let mut builds = Vec::new();
    for target in targets {
        let builder = build_matcher.prepare_builder( &config, package, target, Profile::Test );
        builds.push( builder.run()? );
    }

    if no_run {
        exit( 0 );
    }

    let mut any_failure = false;
    if use_nodejs {
        for build in builds {
            test_in_nodejs( &build_matcher, build, &arg_passthrough, &mut any_failure )?;
        }
    } else {
        for build in builds {
            test_in_chromium( &build_matcher, build, &arg_passthrough, &mut any_failure )?;
        }
    }

    if any_failure {
        exit( 101 );
    } else {
        if build_matcher.targeting_native_wasm() {
            println_err!( "All tests passed!" );
            // At least **I hope** that's the case; there are no prints
            // when running those tests, so who knows what happens. *shrug*
        }
    }

    Ok(())
}
