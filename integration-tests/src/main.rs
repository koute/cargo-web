#[macro_use]
extern crate lazy_static;

use std::path::PathBuf;

mod utils;
use utils::*;

lazy_static! {
    static ref CARGO_WEB: PathBuf = get_var( "CARGO_WEB" ).into();
    static ref REPOSITORY_ROOT: PathBuf = get_var( "REPOSITORY_ROOT" ).into();
    static ref NODEJS: PathBuf = {
        use utils::find_cmd;
        find_cmd( &[ "nodejs", "node", "nodejs.exe", "node.exe" ] ).expect( "nodejs not found" ).into()
    };
}

fn each_target< F: FnMut( &'static str ) >( mut callback: F ) {
    callback( "asmjs-unknown-emscripten" );
    callback( "wasm32-unknown-emscripten" );
    if *IS_NIGHTLY {
        callback( "wasm32-unknown-unknown" );
    }
}

fn main() {
    eprintln!( "Running on nightly: {}", *IS_NIGHTLY );

    cd( &*REPOSITORY_ROOT );

    for name in &[
        "workspace",
        "conflicting-versions",
        "requires-old-cargo-web",
        "requires-future-cargo-web-through-disabled-dep",
        "requires-future-cargo-web-through-dev-dep",
        "requires-future-cargo-web-through-dep-dev-dep",
        "requires-future-cargo-web-through-build-dep"
    ] {
        in_directory( &format!( "test-crates/{}", name ), || {
            each_target( |target| {
                run( &*CARGO_WEB, &["build", "--target", target] ).assert_success();
            });
        });
    }

    in_directory( "test-crates/requires-future-cargo-web-through-target-dep", || {
        run( &*CARGO_WEB, &["build", "--target", "asmjs-unknown-emscripten"] ).assert_success();
        run( &*CARGO_WEB, &["build", "--target", "wasm32-unknown-emscripten"] ).assert_failure();
    });

    for name in &[
        "requires-future-cargo-web",
        "requires-future-cargo-web-through-dep",
        "requires-future-cargo-web-through-dep-dep",
        "requires-future-cargo-web-through-dep-and-dev-dep"
    ] {
        in_directory( &format!( "test-crates/{}", name ), || {
            each_target( |target| {
                run( &*CARGO_WEB, &["build", "--target", target] ).assert_failure();
            });
        });
    }

    in_directory( "test-crates/requires-future-cargo-web-through-dev-dep", || {
        each_target( |target| {
            run( &*CARGO_WEB, &["test", "--target", target] ).assert_failure();
        });
    });

    if *IS_NIGHTLY {
        in_directory( "test-crates/native-webasm", || {
            run( &*CARGO_WEB, &["build", "--target", "wasm32-unknown-unknown"] ).assert_success();
            run( &*NODEJS, &["run.js"] ).assert_success();
        });
    }
}
