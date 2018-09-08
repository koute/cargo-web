extern crate rustc_version;
use rustc_version::{version_meta, Channel};

fn main() {
    match version_meta().unwrap().channel {
        Channel::Nightly => {
            println!( "cargo:rustc-cfg=test_wasm32_unknown_unknown" );
        },
        _ => {}
    }
}
