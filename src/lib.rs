#![deny(
    // missing_debug_implementations,
    trivial_numeric_casts,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

#[macro_use]
extern crate structopt;
extern crate clap;
extern crate digest;
extern crate futures;
extern crate http;
extern crate hyper;
extern crate libflate;
extern crate notify;
extern crate pbr;
extern crate reqwest;
extern crate serde;
extern crate sha1;
extern crate sha2;
extern crate tar;
extern crate tempfile;
extern crate toml;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate base_x;
extern crate handlebars;
extern crate indexmap;
extern crate regex;
extern crate unicode_categories;
extern crate walkdir;
extern crate websocket;
#[macro_use]
extern crate lazy_static;
extern crate directories;
extern crate percent_encoding;

extern crate parity_wasm;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate rustc_demangle;

extern crate ansi_term;
extern crate cargo_metadata;

extern crate memmap;
extern crate semver;

extern crate atty;
extern crate open;
#[macro_use]
extern crate failure;

pub mod cargo_shim;

#[macro_use]
mod utils;
pub mod build;
mod chrome_devtools;
pub mod cmd_build;
pub mod cmd_deploy;
pub mod cmd_prepare_emscripten;
pub mod cmd_start;
pub mod cmd_test;
mod config;
pub mod deployment;
mod emscripten;
pub mod error;
mod http_utils;
mod package;
mod project_dirs;
mod test_chromium;
mod wasm;
mod wasm_context;
mod wasm_export_main;
mod wasm_export_table;
mod wasm_gc;
mod wasm_hook_grow;
mod wasm_inline_js;
mod wasm_intrinsics;
mod wasm_js_export;
mod wasm_js_snippet;
mod wasm_runtime;

use std::path::PathBuf;

pub use error::Error;

#[derive(Debug, StructOpt)]
#[structopt(name = "cargo-web")]
#[structopt(raw(setting = "structopt::clap::AppSettings::ColoredHelp"))]
#[structopt(rename_all = "kebab-case")]
pub enum SubCmds {
    /// Compile a local package and all of its dependencies
    Build {
        #[structopt(flatten)]
        build_args: Build,
        #[structopt(flatten)]
        ext: BuildExt,
    },
    /// Typecheck a local package and all of its dependencies
    Check {
        #[structopt(flatten)]
        build_args: Build,
        #[structopt(flatten)]
        ext: BuildExt,
    },
    /// Deploys your project so that it's ready to be served statically
    Deploy {
        /// Output directory; the default is `$CARGO_TARGET_DIR/deploy`
        #[structopt(short = "o", long, parse(from_os_str))]
        output: Option<PathBuf>,
        #[structopt(flatten)]
        build_args: Build,
    },
    /// Fetches and installs prebuilt Emscripten packages
    PrepareEmscripten,
    /// Runs an embedded web server, which serves the built project
    Start {
        /// Bind the server to this address
        #[structopt(long, default_value = "localhost")]
        host: String,
        /// Bind the server to this port
        #[structopt(long, default_value = "8000")]
        port: u16,
        /// Open browser after server starts
        #[structopt(long)]
        open: bool,
        /// Will try to automatically reload the page on rebuild
        #[structopt(long)]
        auto_reload: bool,
        #[structopt(flatten)]
        build_target: Target,
        #[structopt(flatten)]
        build_args: Build,
    },
    /// Compiles and runs tests
    Test {
        /// Compile, but don't run tests
        #[structopt(long)]
        no_run: bool,
        /// Uses Node.js to run the tests
        #[structopt(long)]
        nodejs: bool,
        #[structopt(flatten)]
        build_args: Build,
        /// -- followed by anything will pass the arguments to the test runner
        passthrough: Vec<String>,
    },
}

impl SubCmds {
    pub fn run(self) -> Result<(), Error> {
        // cmd_build::command_build( matches )
        // cmd_build::command_check( matches )
        // cmd_test::command_test( matches )
        // cmd_start::command_start( matches )
        // cmd_deploy::command_deploy( matches )
        match self {
            SubCmds::PrepareEmscripten => cmd_prepare_emscripten::command_prepare_emscripten(),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct Target {
    /// Build only this package's library
    #[structopt(long, group = "target_type")]
    lib: bool,
    /// Build only the specified binary
    #[structopt(long, group = "target_type")]
    bin: Option<String>,
    /// Build only the specified example
    #[structopt(long, group = "target_type")]
    example: Option<String>,
    /// Build only the specified test target
    #[structopt(long, group = "target_type")]
    test: Option<String>,
    /// Build only the specified benchmark target
    #[structopt(long, group = "target_type")]
    bench: Option<String>,
}

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct BuildExt {
    /// Selects the stdout output format
    #[structopt(long, default_value = "human")]
    message_format: String,
    /// Selects the type of JavaScript runtime which will be generated
    ///
    /// Valid only for the `wasm32-unknown-unknown` target.
    #[structopt(long, default_value = "standalone")]
    runtime: String,
    #[structopt(flatten)]
    build_target: Target,
}

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct Build {
    /// Package to build
    #[structopt(short = "p", long)]
    package: Option<String>,
    /// Additional features to build
    #[structopt(long, group = "build_features")]
    features: Vec<String>,
    /// Build all available features
    #[structopt(long, group = "build_features")]
    all_features: bool,
    /// Do not build the `default` feature
    #[structopt(long, group = "build_features")]
    no_default_features: bool,
    /// Won't try to download Emscripten; will always use the system one
    #[structopt(long)]
    use_system_emscripten: bool,
    /// Build artifacts in release mode, with optimizations
    #[structopt(long)]
    release: bool,
    /// Build for the target
    #[structopt(
        long,
        group = "target_platform",
        default_value = "wasm32-unknown-unknown"
    )]
    target: String,
    /// Use verbose output
    #[structopt(short = "v", long)]
    verbose: bool,

    // These three are legacy options kept for compatibility.
    /// Generate asmjs through Emscripten (default)
    #[structopt(long, group = "target_platform")]
    target_asmjs_emscripten: bool,
    /// Generate webasm through Emscripten
    #[structopt(long, group = "target_platform")]
    target_webasm_emscripten: bool,
    /// Generates webasm through Rust's native backend (HIGHLY EXPERIMENTAL!)
    #[structopt(long, group = "target_platform")]
    target_webasm: bool,
}
