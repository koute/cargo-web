#![deny(
    missing_debug_implementations,
    trivial_numeric_casts,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

extern crate clap;
extern crate notify;
extern crate hyper;
extern crate futures;
extern crate tempdir;
extern crate reqwest;
extern crate pbr;
extern crate libflate;
extern crate tar;
extern crate sha1;
extern crate sha2;
extern crate digest;
extern crate toml;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate handlebars;
extern crate unicode_categories;
extern crate indexmap;
extern crate websocket;
extern crate regex;
extern crate walkdir;
extern crate base_x;

extern crate parity_wasm;
#[macro_use]
extern crate log;
extern crate rustc_demangle;
extern crate env_logger;

extern crate cargo_metadata;
extern crate ansi_term;

extern crate semver;
extern crate memmap;

use std::process::exit;
use std::env;

use clap::{
    Arg,
    App,
    AppSettings,
    SubCommand
};

#[allow(dead_code)]
mod app_dirs;

mod cargo_shim;

#[macro_use]
mod utils;
mod http_utils;
mod config;
mod package;
mod build;
mod deployment;
mod error;
mod wasm;
mod wasm_gc;
mod wasm_inline_js;
mod wasm_export_main;
mod wasm_export_table;
mod wasm_hook_grow;
mod wasm_runtime;
mod wasm_context;
mod wasm_intrinsics;
mod wasm_js_export;
mod emscripten;
mod test_chromium;
mod chrome_devtools;
mod cmd_build;
mod cmd_start;
mod cmd_test;
mod cmd_deploy;
mod cmd_prepare_emscripten;

fn add_shared_build_params< 'a, 'b >( app: App< 'a, 'b > ) -> App< 'a, 'b > {
    return app
        .arg(
            Arg::with_name( "package" )
                .short( "p" )
                .long( "package" )
                .help( "Package to build" )
                .value_name( "NAME" )
                .takes_value( true )
        )
        .arg(
            Arg::with_name( "features" )
                .long( "features" )
                .help( "Space-separated list of features to also build" )
                .value_name( "FEATURES" )
                .takes_value( true )
        )
        .arg(
            Arg::with_name( "all-features" )
                .long( "all-features" )
                .help( "Build all available features" )
                // Technically Cargo doesn't treat it as conflicting,
                // but it seems less confusing to *not* allow these together.
                .conflicts_with_all( &[ "features", "no-default-features" ] )
        )
        .arg(
            Arg::with_name( "no-default-features" )
                .long( "no-default-features" )
                .help( "Do not build the `default` feature" )
        )
        .arg(
            Arg::with_name( "use-system-emscripten" )
                .long( "use-system-emscripten" )
                .help( "Won't try to download Emscripten; will always use the system one" )
        )
        .arg(
            Arg::with_name( "release" )
                .long( "release" )
                .help( "Build artifacts in release mode, with optimizations" )
        )
        .arg(
            Arg::with_name( "target" )
                .long( "target" )
                .takes_value( true )
                .value_name( "TRIPLE" )
                .help( "Build for the target [default: asmjs-unknown-emscripten]" )
                .possible_values( &[ "asmjs-unknown-emscripten", "wasm32-unknown-emscripten", "wasm32-unknown-unknown" ] )
                .conflicts_with_all( &["target-asmjs-emscripten", "target-webasm-emscripten", "target-webasm"] )
        )
        .arg(
            Arg::with_name( "verbose" )
                .short( "v" )
                .long( "verbose" )
                .help( "Use verbose output" )
        )
        // These three are legacy options kept for compatibility.
        .arg(
            Arg::with_name( "target-asmjs-emscripten" )
                .long( "target-asmjs-emscripten" )
                .help( "Generate asmjs through Emscripten (default)" )
                .overrides_with_all( &["target-webasm-emscripten", "target-webasm"] )
                .hidden( true )
        )
        .arg(
            Arg::with_name( "target-webasm-emscripten" )
                .long( "target-webasm-emscripten" )
                .help( "Generate webasm through Emscripten" )
                .overrides_with_all( &["target-asmjs-emscripten", "target-webasm"] )
                .hidden( true )
        )
        .arg(
            Arg::with_name( "target-webasm" )
                .long( "target-webasm" )
                .help( "Generates webasm through Rust's native backend (HIGHLY EXPERIMENTAL!)" )
                .overrides_with_all( &["target-asmjs-emscripten", "target-webasm-emscripten"] )
                .hidden( true )
        );
}

fn main() {
    if let Ok( value ) = env::var( "CARGO_WEB_LOG" ) {
        let mut builder = env_logger::Builder::new();
        builder.parse( &value );
        builder.init();
    }

    let args = {
        // To allow running both as 'cargo-web' and as 'cargo web'.
        let mut args = env::args();
        let mut filtered_args = Vec::new();
        filtered_args.push( args.next().unwrap() );

        match args.next() {
            None => {},
            #[cfg(any(unix))]
            Some( ref arg ) if filtered_args[ 0 ].ends_with( "cargo-web" ) && arg == "web" => {},
            #[cfg(any(windows))]
            Some( ref arg ) if filtered_args[ 0 ].ends_with( "cargo-web.exe" ) && arg == "web" => {},
            Some( arg ) => filtered_args.push( arg )
        }

        filtered_args.extend( args );
        filtered_args
    };

    let mut build_subcommand =
        SubCommand::with_name( "build" )
            .about( "Compile a local package and all of its dependencies" )
            .arg(
                Arg::with_name( "lib" )
                    .long( "lib" )
                    .help( "Build only this package's library" )
            )
            .arg(
                Arg::with_name( "bin" )
                    .long( "bin" )
                    .help( "Build only the specified binary" )
                    .value_name( "NAME" )
                    .takes_value( true )
            )
            .arg(
                Arg::with_name( "example" )
                    .long( "example" )
                    .help( "Build only the specified example" )
                    .value_name( "NAME" )
                    .takes_value( true )
            )
            .arg(
                Arg::with_name( "test" )
                    .long( "test" )
                    .help( "Build only the specified test target" )
                    .value_name( "NAME" )
                    .takes_value( true )
            )
            .arg(
                Arg::with_name( "bench" )
                    .long( "bench" )
                    .help( "Build only the specified benchmark target" )
                    .value_name( "NAME" )
                    .takes_value( true )
            )
            .arg(
                Arg::with_name( "message-format" )
                    .long( "message-format" )
                    .help( "Selects the stdout output format" )
                    .value_name( "FMT" )
                    .takes_value( true )
                    .default_value( "human" )
                    .possible_values( &[
                        "human",
                        "json"
                    ])
            )
            .arg(
                Arg::with_name( "runtime" )
                    .long( "runtime" )
                    .takes_value( true )
                    .value_name( "RUNTIME" )
                    .help( "Selects the type of JavaScript runtime which will be generated; valid only for the `wasm32-unknown-unknown` target" )
                    .possible_values( &["standalone", "experimental-only-loader"] )
                    .default_value_if(
                        "target", Some( "wasm32-unknown-unknown" ),
                        "standalone"
                    )
                    .hidden( true ) // This is experimental for now.
            );

    let mut test_subcommand =
        SubCommand::with_name( "test" )
            .about( "Compiles and runs tests" )
            .arg(
                Arg::with_name( "no-run" )
                    .long( "no-run" )
                    .help( "Compile, but don't run tests" )
            )
            .arg(
                Arg::with_name( "nodejs" )
                    .long( "nodejs" )
                    .help( "Uses Node.js to run the tests" )
            )
            .arg(
                Arg::with_name( "passthrough" )
                    .help( "-- followed by anything will pass the arguments to the test runner")
                    .multiple( true )
                    .takes_value( true )
                    .last( true )
            );

    let mut start_subcommand =
        SubCommand::with_name( "start" )
            .about( "Runs an embedded web server serving the built project" )
            .arg(
                Arg::with_name( "bin" )
                    .long( "bin" )
                    .help( "Build only the specified binary" )
                    .value_name( "NAME" )
                    .takes_value( true )
            )
            .arg(
                Arg::with_name( "example" )
                    .long( "example" )
                    .help( "Serves the specified example" )
                    .value_name( "NAME" )
                    .takes_value( true )
            )
            .arg(
                Arg::with_name( "test" )
                    .long( "test" )
                    .help( "Build only the specified test target" )
                    .value_name( "NAME" )
                    .takes_value( true )
            )
            .arg(
                Arg::with_name( "bench" )
                    .long( "bench" )
                    .help( "Build only the specified benchmark target" )
                    .value_name( "NAME" )
                    .takes_value( true )
            )
            .arg(
                Arg::with_name( "host" )
                    .long( "host" )
                    .help( "Bind the server to this address, default `localhost`")
                    .value_name( "HOST" )
                    .takes_value( true )
            )
            .arg(
                Arg::with_name( "port" )
                    .long( "port" )
                    .help( "Bind the server to this port, default 8000" )
                    .value_name( "PORT" )
                    .takes_value( true )
            )
            .arg(
                Arg::with_name( "auto-reload" )
                    .long( "auto-reload" )
                    .help( "Will try to automatically reload the page on rebuild" )
            );

    let mut deploy_subcommand =
        SubCommand::with_name( "deploy" )
            .about( "Deploys your project so that its ready to be served statically" );

    let prepare_emscripten_subcommand =
        SubCommand::with_name( "prepare-emscripten" )
            .about( "Fetches and installs prebuilt Emscripten packages" );

    build_subcommand = add_shared_build_params( build_subcommand );
    test_subcommand = add_shared_build_params( test_subcommand );
    start_subcommand = add_shared_build_params( start_subcommand );
    deploy_subcommand = add_shared_build_params( deploy_subcommand );

    let matches = App::new( "cargo-web" )
        .version( env!( "CARGO_PKG_VERSION" ) )
        .setting( AppSettings::SubcommandRequiredElseHelp )
        .setting( AppSettings::VersionlessSubcommands )
        .subcommand( build_subcommand )
        .subcommand( test_subcommand )
        .subcommand( start_subcommand )
        .subcommand( deploy_subcommand )
        .subcommand( prepare_emscripten_subcommand )
        .get_matches_from( args );

    let result = if let Some( matches ) = matches.subcommand_matches( "build" ) {
        cmd_build::command_build( matches )
    } else if let Some( matches ) = matches.subcommand_matches( "test" ) {
        cmd_test::command_test( matches )
    } else if let Some( matches ) = matches.subcommand_matches( "start" ) {
        cmd_start::command_start( matches )
    } else if let Some( matches ) = matches.subcommand_matches( "deploy" ) {
        cmd_deploy::command_deploy( matches )
    } else if let Some( _ ) = matches.subcommand_matches( "prepare-emscripten" ) {
        cmd_prepare_emscripten::command_prepare_emscripten()
    } else {
        return;
    };

    match result {
        Ok( _ ) => {},
        Err( error ) => {
            eprintln!( "error: {}", error );
            exit( 101 );
        }
    }
}
