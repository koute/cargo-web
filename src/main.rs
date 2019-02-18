extern crate cargo_web;
extern crate env_logger;
extern crate structopt;

use std::env::{args, var};
use std::process::exit;

use cargo_web::{run, CargoWebOpts};
use structopt::StructOpt;

macro_rules! target_arg {
    ( $opt:expr, $arch:ident $abi:ident ) => {{
        let triple = concat!(stringify!($arch), "-unknown-", stringify!($abi));

        eprintln!(
            "The `--target-{}` flag is DEPRECATED. Please use the `--target` \
             option with the full triple (`{}`)",
            $opt, triple
        );

        format!("--target={}", triple)
    }};
}

fn main() {
    if let Ok(value) = var("CARGO_WEB_LOG") {
        let mut builder = env_logger::Builder::new();
        builder.parse(&value);
        builder.init();
    }

    let argv = args().into_iter().map(|arg| match arg.as_ref() {
        "--target-webasm" => target_arg!("webasm", wasm32 unknown),
        "--target-webasm-emscripten" => target_arg!("webasm-emscripten", wasm32 emscripten),
        "--target-asmjs-emscripten" => target_arg!("asmjs-emscripten", asmjs emscripten),
        _ => arg,
    });

    if let Err(error) = run(CargoWebOpts::from_iter(argv)) {
        eprintln!("error: {}", error);
        exit(101);
    }
}
