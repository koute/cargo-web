use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use reqwest;

use utils::*;

lazy_static! {
    static ref CARGO_WEB: PathBuf = get_var( "CARGO_WEB" ).into();
    static ref REPOSITORY_ROOT: PathBuf = get_var( "REPOSITORY_ROOT" ).into();
    static ref NODEJS: PathBuf = {
        use utils::find_cmd;
        find_cmd( &[ "nodejs", "node", "nodejs.exe", "node.exe" ] ).expect( "nodejs not found" ).into()
    };
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Target {
    AsmjsUnknownEmscripten,
    Wasm32UnknownEmscripten,
    Wasm32UnknownUnknown
}

impl Target {
    fn to_str( self ) -> &'static str {
        match self {
            Target::AsmjsUnknownEmscripten => "asmjs-unknown-emscripten",
            Target::Wasm32UnknownEmscripten => "wasm32-unknown-emscripten",
            Target::Wasm32UnknownUnknown => "wasm32-unknown-unknown"
        }
    }
}

use self::Target::*;

fn crate_path( crate_name: &str ) -> PathBuf {
    REPOSITORY_ROOT.join( "test-crates" ).join( crate_name )
}

fn assert_builds( target: Target, crate_name: &str ) {
    run( crate_path( crate_name ), &*CARGO_WEB, &["build", "--target", target.to_str()] ).assert_success();
}

fn assert_fails_to_build( target: Target, crate_name: &str ) {
    run( crate_path( crate_name ), &*CARGO_WEB, &["build", "--target", target.to_str()] ).assert_failure();
}

fn assert_tests_build( target: Target, crate_name: &str ) {
    run( crate_path( crate_name ), &*CARGO_WEB, &["test", "--no-run", "--target", target.to_str()] ).assert_success();
}

fn assert_tests_fail_to_build( target: Target, crate_name: &str ) {
    run( crate_path( crate_name ), &*CARGO_WEB, &["test", "--no-run", "--target", target.to_str()] ).assert_failure();
}

fn assert_tests_succeed( target: Target, crate_name: &str ) {
    run( crate_path( crate_name ), &*CARGO_WEB, &["test", "--nodejs", "--target", target.to_str()] ).assert_success();
}

fn assert_tests_fail( target: Target, crate_name: &str ) {
    run( crate_path( crate_name ), &*CARGO_WEB, &["test", "--nodejs", "--target", target.to_str()] ).assert_failure();
}

macro_rules! common_tests { (($($attr:tt)*) $namespace:ident, $target:expr) => { mod $namespace {
    use super::*;

    $($attr)*
    #[test]
    fn build_rlib() {
        assert_builds( $target, "rlib" );
    }

    $($attr)*
    #[test]
    fn build_dev_depends_on_dylib() {
        assert_builds( $target, "dev-depends-on-dylib" );
    }

    $($attr)*
    #[test]
    fn build_staticlib() {
        assert_builds( $target, "staticlib" );
    }

    $($attr)*
    #[test]
    fn build_workspace() {
        assert_builds( $target, "workspace" );
    }

    $($attr)*
    #[test]
    fn build_conflicting_versions() {
        assert_builds( $target, "conflicting-versions" );
    }

    $($attr)*
    #[test]
    fn build_requires_old_cargo_web() {
        assert_builds( $target, "requires-old-cargo-web" );
    }

    $($attr)*
    #[test]
    fn build_requires_future_cargo_web_disabled_dep() {
        assert_builds( $target, "req-future-cargo-web-disabled-dep" );
    }

    $($attr)*
    #[test]
    fn build_requires_future_cargo_web_dev_dep() {
        assert_builds( $target, "req-future-cargo-web-dev-dep" );
    }

    $($attr)*
    #[test]
    fn build_requires_future_cargo_web_dep_dev_dep() {
        assert_builds( $target, "req-future-cargo-web-dep-dev-dep" );
    }

    $($attr)*
    #[test]
    fn build_requires_future_cargo_web_build_dep() {
        assert_builds( $target, "req-future-cargo-web-build-dep" );
    }

    $($attr)*
    #[test]
    fn build_compiling_under_cargo_web_env_var() {
        assert_builds( $target, "compiling-under-cargo-web-env-var" );
    }

    $($attr)*
    #[test]
    fn build_depends_on_default_target_invalid() {
        assert_builds( $target, "depends-on-default-target-invalid" );
    }

    $($attr)*
    #[test]
    fn test_crate_with_tests() {
        assert_tests_build( $target, "crate-with-tests" );
        for _ in 0..2 {
            assert_tests_succeed( $target, "crate-with-tests" )
        }
    }

    #[test]
    fn test_crate_with_integration_tests() {
        assert_tests_build( $target, "crate-with-integration-tests" );
        for _ in 0..2 {
            assert_tests_succeed( $target, "crate-with-integration-tests" );
        }
    }

    $($attr)*
    #[test]
    fn failed_build_requires_future_cargo_web() {
        assert_fails_to_build( $target, "requires-future-cargo-web" );
    }

    $($attr)*
    #[test]
    fn failed_build_requires_future_cargo_web_dep() {
        assert_fails_to_build( $target, "req-future-cargo-web-dep" );
    }

    $($attr)*
    #[test]
    fn failed_build_requires_future_cargo_web_dep_dep() {
        assert_fails_to_build( $target, "req-future-cargo-web-dep-dep" );
    }

    $($attr)*
    #[test]
    fn failed_build_requires_future_cargo_web_dep_and_dev_dep() {
        assert_fails_to_build( $target, "req-future-cargo-web-dep-and-dev-dep" );
    }

    $($attr)*
    #[test]
    fn failed_test_requires_future_cargo_web_dev_dep() {
        assert_tests_fail_to_build( $target, "req-future-cargo-web-dev-dep" );
    }

    $($attr)*
    #[test]
    fn prepend_js() {
        let cwd = crate_path( "prepend-js" );
        assert_builds( $target, "prepend-js" );
        // TODO: We should run cargo-web with `--message-format=json` and grab this path automatically.
        let build_dir = if $target == Wasm32UnknownUnknown { "release" } else { "debug" };
        let output = cwd.join( "target" ).join( $target.to_str() ).join( build_dir ).join( "prepend-js.js" );
        assert_file_contains( output, "alert('THIS IS A TEST');" );
    }

    $($attr)*
    #[test]
    fn virtual_manifest() {
        let cwd = crate_path( "virtual-manifest" );
        run( &cwd, &*CARGO_WEB, &["build", "--target", $target.to_str()] ).assert_failure();
        run( &cwd, &*CARGO_WEB, &["build", "-p", "child", "--target", $target.to_str()] ).assert_success();

        run( &cwd, &*CARGO_WEB, &["test", "--no-run", "--target", $target.to_str()] ).assert_failure();
        run( &cwd, &*CARGO_WEB, &["test", "--no-run", "-p", "child", "--target", $target.to_str()] ).assert_success();

        run( &cwd, &*CARGO_WEB, &["deploy", "--target", $target.to_str()] ).assert_failure();
        run( &cwd, &*CARGO_WEB, &["deploy", "-p", "child", "--target", $target.to_str()] ).assert_success();

        assert_file_missing( cwd.join( "child/target/deploy" ) );
        assert_file_exists( cwd.join( "target/deploy" ) );
    }

    $($attr)*
    #[test]
    fn failing_test() {
        assert_tests_build( $target, "failing-test" );
        assert_tests_fail( $target, "failing-test" );
    }

    $($attr)*
    #[test]
    fn failing_integration_test() {
        assert_tests_build( $target, "failing-integration-test" );
        assert_tests_fail( $target, "failing-integration-test" );
    }

    $($attr)*
    #[test]
    fn failing_integration_test_crate_types() {
        assert_tests_build( $target, "failing-integration-test-crate-types" );
        assert_tests_fail( $target, "failing-integration-test-crate-types" );
    }

    $($attr)*
    #[test]
    fn async_normal_test_with_nodejs() {
        let crate_name = "async-tests";
        assert_tests_build( $target, crate_name );
        let result = run( crate_path( crate_name ), &*CARGO_WEB, &["test", "--nodejs", "--target", $target.to_str(), "--", "normal_test"] );
        assert!( !result.output().contains( "async test(s)" ) );
        if $target != Wasm32UnknownUnknown {
            // Normal tests don't output anything on this target.
            assert!( result.output().contains( "test normal_test ... ok" ) );
            assert!( result.output().contains( "test result (async): ok. 0 passed; 0 failed" ) );
        }
        result.assert_success();
    }

    $($attr)*
    #[test]
    fn async_test_ok_with_nodejs() {
        let crate_name = "async-tests";
        assert_tests_build( $target, crate_name );
        let result = run( crate_path( crate_name ), &*CARGO_WEB, &["test", "--nodejs", "--target", $target.to_str(), "--", "ok"] );
        assert!( result.output().contains( "running 1 async test(s)" ) );
        assert!( result.output().contains( "test ok ... ok" ) );
        assert!( result.output().contains( "test result (async): ok. 1 passed; 0 failed" ) );
        assert!( !result.output().contains( "Redirected console.log!" ) );
        result.assert_success();
    }

    $($attr)*
    #[test]
    fn async_test_panic_with_nodejs() {
        let crate_name = "async-tests";
        assert_tests_build( $target, crate_name );
        let result = run( crate_path( crate_name ), &*CARGO_WEB, &["test", "--nodejs", "--target", $target.to_str(), "--", "panic"] );
        assert!( result.output().contains( "running 1 async test(s)" ) );
        assert!( result.output().contains( "test panic ... FAILED" ) );
        assert!( result.output().contains( "test result (async): FAILED. 0 passed; 1 failed" ) );
        assert!( result.output().contains( "Redirected console.log!" ) );
        result.assert_failure();
    }

    $($attr)*
    #[test]
    fn async_test_timeout_with_nodejs() {
        let crate_name = "async-tests";
        assert_tests_build( $target, crate_name );
        let result = run( crate_path( crate_name ), &*CARGO_WEB, &["test", "--nodejs", "--target", $target.to_str(), "--", "timeout"] );
        assert!( result.output().contains( "running 1 async test(s)" ) );
        assert!( result.output().contains( "test timeout ... FAILED" ) );
        assert!( result.output().contains( "test result (async): FAILED. 0 passed; 1 failed" ) );
        result.assert_failure();
    }

    $($attr)*
    #[test]
    fn async_normal_test_with_chromium() {
        let crate_name = "async-tests";
        assert_tests_build( $target, crate_name );
        let result = run( crate_path( crate_name ), &*CARGO_WEB, &["test", "--target", $target.to_str(), "--", "normal_test"] );
        assert!( !result.output().contains( "async test(s)" ) );
        if $target != Wasm32UnknownUnknown {
            assert!( result.output().contains( "test normal_test ... ok" ) );
            assert!( result.output().contains( "test result (async): ok. 0 passed; 0 failed" ) );
        }
        result.assert_success();
    }

    $($attr)*
    #[test]
    fn async_test_ok_with_chromium() {
        let crate_name = "async-tests";
        assert_tests_build( $target, crate_name );
        let result = run( crate_path( crate_name ), &*CARGO_WEB, &["test", "--target", $target.to_str(), "--", "ok"] );
        assert!( result.output().contains( "running 1 async test(s)" ) );
        assert!( result.output().contains( "test ok ... ok" ) );
        assert!( result.output().contains( "test result (async): ok. 1 passed; 0 failed" ) );
        result.assert_success();
    }

    $($attr)*
    #[test]
    fn async_test_panic_with_chromium() {
        let crate_name = "async-tests";
        assert_tests_build( $target, crate_name );
        let result = run( crate_path( crate_name ), &*CARGO_WEB, &["test", "--target", $target.to_str(), "--", "panic"] );
        assert!( result.output().contains( "running 1 async test(s)" ) );
        assert!( result.output().contains( "test panic ... FAILED" ) );
        assert!( result.output().contains( "test result (async): FAILED. 0 passed; 1 failed" ) );
        result.assert_failure();
    }

    $($attr)*
    #[test]
    fn async_test_timeout_with_chromium() {
        let crate_name = "async-tests";
        assert_tests_build( $target, crate_name );
        let result = run( crate_path( crate_name ), &*CARGO_WEB, &["test", "--target", $target.to_str(), "--", "timeout"] );
        assert!( result.output().contains( "running 1 async test(s)" ) );
        assert!( result.output().contains( "test timeout ... FAILED" ) );
        assert!( result.output().contains( "test result (async): FAILED. 0 passed; 1 failed" ) );
        result.assert_failure();
    }
}}}

common_tests!( () asmjs_unknown_emscripten, Target::AsmjsUnknownEmscripten );
common_tests!( () wasm32_unknown_emscripten, Target::Wasm32UnknownEmscripten );
common_tests!( (#[cfg_attr(not(test_wasm32_unknown_unknown), ignore)]) wasm32_unknown_unknown, Target::Wasm32UnknownUnknown );

#[test]
fn build_requires_future_cargo_web_target_dep() {
    assert_builds( AsmjsUnknownEmscripten, "req-future-cargo-web-target-dep" );
    assert_fails_to_build( Wasm32UnknownEmscripten, "req-future-cargo-web-target-dep" );
}

#[test]
fn link_args_per_target() {
    let cwd = crate_path( "link-args-per-target" );
    // In Web.toml of the test crate we set a different `EXPORT_NAME` link-arg
    // for each target and we check if it's actually used by Emscripten.
    assert_builds( AsmjsUnknownEmscripten, "link-args-per-target" );
    assert_file_contains( cwd.join( "target/asmjs-unknown-emscripten/debug/link-args-per-target.js" ), "CustomExportNameAsmJs" );

    assert_builds( Wasm32UnknownEmscripten, "link-args-per-target" );
    assert_file_contains( cwd.join( "target/wasm32-unknown-emscripten/debug/link-args-per-target.js" ), "CustomExportNameWasm" );

    if cfg!( test_wasm32_unknown_unknown ) {
        // This has no flags set, but still should compile.
        assert_builds( Wasm32UnknownUnknown, "link-args-per-target" );
    }
}


#[test]
fn link_args_for_emscripten() {
    let cwd = crate_path( "link-args-for-emscripten" );
     // Here we set the same flag for both targets in a single target section.
    assert_builds( AsmjsUnknownEmscripten, "link-args-for-emscripten" );
    assert_file_contains( cwd.join( "target/asmjs-unknown-emscripten/debug/link-args-for-emscripten.js" ), "CustomExportNameEmscripten" );

    assert_builds( Wasm32UnknownEmscripten, "link-args-for-emscripten" );
    assert_file_contains( cwd.join( "target/wasm32-unknown-emscripten/debug/link-args-for-emscripten.js" ), "CustomExportNameEmscripten" );

    if cfg!( test_wasm32_unknown_unknown ) {
        // This has no flags set, but still should compile.
        assert_builds( Wasm32UnknownUnknown, "link-args-for-emscripten" );
    }
}

#[test]
fn build_depends_on_prepend_js_two_targets() {
    let cwd = crate_path( "depends-on-prepend-js-two-targets" );
    run( &cwd, &*CARGO_WEB, &["build", "--target", "asmjs-unknown-emscripten"] ).assert_success();
    assert_file_contains( cwd.join( "target/asmjs-unknown-emscripten/debug/depends-on-prepend-js-two-targets.js" ), "alert('THIS IS A TEST');" );

    run( &cwd, &*CARGO_WEB, &["build", "--target", "wasm32-unknown-emscripten"] ).assert_success();
    assert_file_contains( cwd.join( "target/wasm32-unknown-emscripten/debug/depends-on-prepend-js-two-targets.js" ), "alert('THIS IS A TEST');" );
}

#[test]
fn default_target_asmjs_unknown_emscripten() {
    let cwd = crate_path( "default-target-asmjs-unknown-emscripten" );
    run( &cwd, &*CARGO_WEB, &["build"] ).assert_success();
    assert_file_exists( cwd.join( "target/asmjs-unknown-emscripten/debug/default-target-asmjs-unknown-emscripten.js" ) );
    run( &cwd, &*CARGO_WEB, &["test", "--no-run"] ).assert_success();
    run( &cwd, &*CARGO_WEB, &["deploy"] ).assert_success();
}

#[test]
fn default_target_wasm32_unknown_emscripten() {
    let cwd = crate_path( "default-target-wasm32-unknown-emscripten" );
    run( &cwd, &*CARGO_WEB, &["build"] ).assert_success();
    assert_file_exists( cwd.join( "target/wasm32-unknown-emscripten/debug/default-target-wasm32-unknown-emscripten.js" ) );
    run( &cwd, &*CARGO_WEB, &["test", "--no-run"] ).assert_success();
    run( &cwd, &*CARGO_WEB, &["deploy"] ).assert_success();
}

#[test]
fn default_target_invalid() {
    let cwd = crate_path( "default-target-invalid" );
    run( &cwd, &*CARGO_WEB, &["build"] ).assert_failure();
    run( &cwd, &*CARGO_WEB, &["test", "--no-run"] ).assert_failure();
    run( &cwd, &*CARGO_WEB, &["deploy"] ).assert_failure();
}

#[cfg_attr(not(test_wasm32_unknown_unknown), ignore)]
#[test]
fn build_and_run_native_wasm() {
    let cwd = crate_path( "native-webasm" );
    assert_builds( Target::Wasm32UnknownUnknown, "native-webasm" );
    run( &cwd, &*NODEJS, &["run.js"] ).assert_success();
}

#[cfg_attr(not(test_wasm32_unknown_unknown), ignore)]
#[test]
fn cdylib() {
    let cwd = crate_path( "cdylib" );
    run( &cwd, &*CARGO_WEB, &["build", "--target", "wasm32-unknown-unknown"] ).assert_success();
    run( &cwd, &*CARGO_WEB, &["deploy", "--target", "wasm32-unknown-unknown"] ).assert_success();
    run( &cwd, &*NODEJS, &[cwd.join( "target/wasm32-unknown-unknown/release/cdylib.js" )] ).assert_success();
}

#[cfg_attr(not(test_wasm32_unknown_unknown), ignore)]
#[test]
fn default_target_wasm32_unknown_unknown() {
    let cwd = crate_path( "default-target-wasm32-unknown-unknown" );
    run( &cwd, &*CARGO_WEB, &["build"] ).assert_success();
    assert_file_exists( cwd.join( "target/wasm32-unknown-unknown/release/default-target-wasm32-unknown-unknown.js" ) );
    run( &cwd, &*CARGO_WEB, &["deploy"] ).assert_success();
}

#[cfg_attr(not(test_wasm32_unknown_unknown), ignore)]
#[test]
fn prepend_js_includable_only_once() {
    let cwd = crate_path( "prepend-js-includable-only-once" );
    run( &cwd, &*CARGO_WEB, &["build", "--release", "--target", "wasm32-unknown-unknown"] ).assert_success();
    run( &cwd, &*NODEJS, &[cwd.join( "target/wasm32-unknown-unknown/release/prepend-js-includable-only-once.js" )] ).assert_success();
}

#[test]
fn static_files() {
    let cwd = crate_path( "static-files" );
    use reqwest::header::CONTENT_TYPE;
    use reqwest::StatusCode;

    run( &cwd, &*CARGO_WEB, &["build"] ).assert_success();
    let _child = run_in_the_background( &cwd, &*CARGO_WEB, &["start"] );
    let start = Instant::now();
    let mut response = None;
    while start.elapsed() < Duration::from_secs( 10 ) && response.is_none() {
        thread::sleep( Duration::from_millis( 100 ) );
        response = reqwest::get( "http://localhost:8000" ).ok();
    }

    let response = response.unwrap();
    assert_eq!( response.status(), StatusCode::OK );
    assert_eq!( *response.headers().get(CONTENT_TYPE).unwrap(), "text/html" );

    let mut response = reqwest::get( "http://localhost:8000/subdirectory/dummy.json" ).unwrap();
    assert_eq!( response.status(), StatusCode::OK );
    assert_eq!( *response.headers().get(CONTENT_TYPE).unwrap(), "application/json" );
    assert_eq!( response.text().unwrap(), "{}" );

    let mut response = reqwest::get( "http://localhost:8000/static-files.js" ).unwrap();
    assert_eq!( response.status(), StatusCode::OK );
    assert_eq!( *response.headers().get(CONTENT_TYPE).unwrap(), "application/javascript" );
    assert_eq!( response.text().unwrap(), read_to_string( cwd.join( "target/asmjs-unknown-emscripten/debug/static-files.js" ) ) );

    // TODO: Move this to its own test?
    let mut response = reqwest::get( "http://localhost:8000/__cargo-web__/build_hash" ).unwrap();
    assert_eq!( response.status(), StatusCode::OK );
    let build_hash = response.text().unwrap();

    let mut response = reqwest::get( "http://localhost:8000/__cargo-web__/build_hash" ).unwrap();
    assert_eq!( response.status(), StatusCode::OK );
    assert_eq!( response.text().unwrap(), build_hash ); // Hash didn't change.

    touch_file( cwd.join( "src/main.rs" ) );

    let start = Instant::now();
    let mut found = false;
    while start.elapsed() < Duration::from_secs( 10 ) && !found {
        thread::sleep( Duration::from_millis( 100 ) );
        let mut response = reqwest::get( "http://localhost:8000" ).unwrap();
        assert_eq!( response.status(), StatusCode::OK );

        let new_build_hash = response.text().unwrap();
        found = new_build_hash != build_hash;
    }

    assert!( found, "Touching a source file didn't change the build hash!" );
}

#[test]
fn requires_future_cargo_web_cfg_dep() {
    assert_builds( Wasm32UnknownUnknown, "req-future-cargo-web-cfg-dep" );
    assert_fails_to_build( Wasm32UnknownEmscripten, "req-future-cargo-web-cfg-dep" );
}

#[test]
fn requires_future_cargo_web_cfg_not_dep() {
    assert_fails_to_build( Wasm32UnknownUnknown, "req-future-cargo-web-cfg-not-dep" );
    assert_builds( Wasm32UnknownEmscripten, "req-future-cargo-web-cfg-not-dep" );
}

#[test]
fn mount_path_fails_for_per() {
    // only applies to wasm32-unknown-unknown backend
    let cwd = crate_path( "mount-path" );
    assert_fails_to_build( Wasm32UnknownEmscripten, "mount-path" );
    assert_fails_to_build( AsmjsUnknownEmscripten, "mount-path" );
    assert_fails_to_build( Wasm32UnknownUnknown, "mount-path" );
}


#[test]
fn mount_path_for_wasm32_unknown_unknown() {
    // only applies to wasm32-unknown-unknown backend
    let cwd = crate_path( "mount-path-wasm32-unknown-unknown" );
    assert_builds( Wasm32UnknownUnknown, "mount-path-wasm32-unknown-unknown" );
    let build_dir = "release";
    let output = cwd.join( "target" ).join( Wasm32UnknownUnknown.to_str() ).join( build_dir ).join( "mount-path.js" );
    assert_file_contains( output, "/custom/static/" );
}
