//! A `cargo` subcommand for the client-side Web.

#![deny(
    missing_debug_implementations,
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

mod cargo_shim;

#[macro_use]
mod utils;
mod build;
mod chrome_devtools;
mod cmd_build;
mod cmd_deploy;
mod cmd_prepare_emscripten;
mod cmd_start;
mod cmd_test;
mod config;
mod deployment;
mod emscripten;
mod error;
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

use std::ffi::OsStr;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::PathBuf;

use build::{Backend, BuildArgs};
pub use cargo_shim::MessageFormat;
use error::Error;
use wasm_runtime::RuntimeKind;

/// CLI for `cargo-web`
#[derive(Debug, StructOpt)]
#[structopt(name = "cargo-web")]
#[structopt(about = "A `cargo` subcommand for the client-side web.")]
#[structopt(raw(global_setting = "structopt::clap::AppSettings::ColoredHelp"))]
#[structopt(raw(setting = "structopt::clap::AppSettings::VersionlessSubcommands"))]
#[structopt(rename_all = "kebab-case")]
pub enum CargoWebOpts {
    /// Compile a local package and all of its dependencies
    Build(BuildOpts),
    /// Typecheck a local package and all of its dependencies
    Check(CheckOpts),
    /// Deploys your project so that it's ready to be served statically
    Deploy(DeployOpts),
    /// Fetches and installs prebuilt Emscripten packages
    PrepareEmscripten(PrepareEmscriptenOpts),
    /// Runs an embedded web server, which serves the built project
    Start(StartOpts),
    /// Compiles and runs tests
    Test(TestOpts),
    #[doc(hidden)]
    #[structopt(raw(setting = "structopt::clap::AppSettings::Hidden"))]
    __Nonexhaustive,
}

/// Run a subcommand based on a configuration
pub fn run(cfg: CargoWebOpts) -> Result<(), Error> {
    match cfg {
        CargoWebOpts::Build(BuildOpts {
            build_args,
            build_target,
            ext,
        }) => cmd_build::command_build(BuildArgs::new(build_args, ext, build_target)?),
        CargoWebOpts::Check(CheckOpts {
            build_args,
            build_target,
            ext,
        }) => cmd_build::command_check(BuildArgs::new(build_args, ext, build_target)?),
        CargoWebOpts::Deploy(DeployOpts { build_args, output }) => {
            cmd_deploy::command_deploy(build_args.into(), output)
        }
        CargoWebOpts::PrepareEmscripten(_) => cmd_prepare_emscripten::command_prepare_emscripten(),
        CargoWebOpts::Start(StartOpts {
            build_args,
            build_target,
            auto_reload,
            open,
            port,
            host,
        }) => cmd_start::command_start(
            BuildArgs::from(build_args).with_target(build_target),
            host,
            port,
            open,
            auto_reload,
        ),
        CargoWebOpts::Test(TestOpts {
            build_args,
            nodejs,
            no_run,
            passthrough,
        }) => {
            let pass_os = passthrough.iter().map(OsStr::new).collect::<Vec<_>>();
            cmd_test::command_test(build_args.into(), nodejs, no_run, &pass_os)
        }
        CargoWebOpts::__Nonexhaustive => unreachable!(),
    }
}

/// Options for `cargo web build`
#[derive(Debug, StructOpt)]
pub struct BuildOpts {
    #[structopt(flatten)]
    build_args: Build,
    #[structopt(flatten)]
    ext: BuildExt,
    #[structopt(flatten)]
    build_target: Target,
}

/// Options for `cargo web check`
pub type CheckOpts = BuildOpts;

/// Options for `cargo web deploy`
#[derive(Debug, StructOpt)]
pub struct DeployOpts {
    /// Output directory; the default is `$CARGO_TARGET_DIR/deploy`
    #[structopt(short = "o", long, parse(from_os_str))]
    output: Option<PathBuf>,
    #[structopt(flatten)]
    build_args: Build,
}

/// Options for `cargo web prepare-emscripten`
#[derive(Debug, StructOpt)]
pub struct PrepareEmscriptenOpts {}

/// Options for `cargo web start`
#[derive(Debug, StructOpt)]
pub struct StartOpts {
    /// Bind the server to this address
    #[structopt(
        long,
        parse(try_from_str = "resolve_host"),
        default_value = "localhost"
    )]
    host: IpAddr,
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
}

/// Options for `cargo web test`
#[derive(Debug, StructOpt)]
pub struct TestOpts {
    /// Compile, but don't run tests
    #[structopt(long)]
    no_run: bool,
    /// Uses Node.js to run the tests
    #[structopt(long)]
    nodejs: bool,
    #[structopt(flatten)]
    build_args: Build,
    /// all additional arguments will be passed through to the test runner
    passthrough: Vec<String>,
}

/// Select a target to build
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

impl Default for Target {
    fn default() -> Self {
        Self {
            lib: false,
            bin: None,
            example: None,
            test: None,
            bench: None,
        }
    }
}

impl Target {
    /// Only build the library portion of the selected package
    pub fn only_lib(mut self) -> Self {
        self.lib = true;
        self.bin.take();
        self.example.take();
        self.test.take();
        self.bench.take();

        self
    }

    /// Only build the specified binary
    pub fn only_bin(mut self, s: &str) -> Self {
        self.lib = false;
        self.bin = Some(s.to_string());
        self.example.take();
        self.test.take();
        self.bench.take();

        self
    }

    /// Only build the specified example
    pub fn only_example(mut self, s: &str) -> Self {
        self.lib = false;
        self.bin.take();
        self.example = Some(s.to_string());
        self.test.take();
        self.bench.take();

        self
    }

    /// Only build the specified test case
    pub fn only_test(mut self, s: &str) -> Self {
        self.lib = false;
        self.bin.take();
        self.example.take();
        self.test = Some(s.to_string());
        self.bench.take();

        self
    }

    /// Only build the specified benchmark
    pub fn only_bench(mut self, s: &str) -> Self {
        self.lib = false;
        self.bin.take();
        self.example.take();
        self.test.take();
        self.bench = Some(s.to_string());

        self
    }
}

/// Specify additional build options
#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct BuildExt {
    /// Selects the stdout output format
    #[structopt(
        long,
        default_value = "human",
        parse(try_from_str),
        raw(possible_values = "&[\"human\", \"json\"]"),
        raw(set = "structopt::clap::ArgSettings::NextLineHelp")
    )]
    message_format: MessageFormat,
    /// Selects the type of JavaScript runtime which will be generated
    /// (Only valid when targeting `wasm32-unknown-unknown`).
    #[structopt(
        long,
        parse(try_from_str),
        raw(possible_values = "&[\"standalone\", \"library-es6\", \"web-extension\"]"),
        raw(set = "structopt::clap::ArgSettings::NextLineHelp")
    )]
    runtime: Option<RuntimeKind>,
}

impl Default for BuildExt {
    fn default() -> Self {
        Self {
            message_format: MessageFormat::Json,
            runtime: None,
        }
    }
}

impl BuildExt {
    /// Set the message format (for progress messages on stdout).
    pub fn with_message_fmt(mut self, fmt: MessageFormat) -> Self {
        self.message_format = fmt;
        self
    }
}

/// Build configuration for one or more targets
#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct Build {
    /// Package to build
    #[structopt(short = "p", long)]
    pub package: Option<String>,
    /// Additional features to build (space-delimited list)
    #[structopt(long, group = "build_features")]
    pub features: Option<String>,
    /// Build all available features
    #[structopt(long, group = "build_features")]
    pub all_features: bool,
    /// Do not build the `default` feature
    #[structopt(long, group = "build_features")]
    pub no_default_features: bool,
    /// Won't try to download Emscripten; will always use the system one
    #[structopt(long)]
    pub use_system_emscripten: bool,
    /// Build artifacts in release mode, with optimizations
    #[structopt(long)]
    pub release: bool,
    /// Target triple to build for, overriding setting in `Web.toml`. If not
    /// specified in `Web.toml`, default target is `wasm32-unknown-unknown`.
    #[structopt(
        long,
        parse(try_from_str),
        raw(
            possible_values = "&[\"wasm32-unknown-unknown\", \"wasm32-unknown-emscripten\", \"asmjs-unknown-emscripten\"]"
        ),
        raw(set = "structopt::clap::ArgSettings::NextLineHelp")
    )]
    pub target: Option<Backend>,
    /// Use verbose output
    #[structopt(short = "v", long)]
    pub verbose: bool,
}

impl Default for Build {
    /// Returns a sensible default config.
    ///
    /// # Note
    /// If you want to change the target triple, use `Into`, e.g.
    /// `target: "asmjs-unknown-emscripten".into()`
    fn default() -> Self {
        Self {
            package: None,
            features: None,
            all_features: false,
            no_default_features: false,
            use_system_emscripten: false,
            release: false,
            target: None,
            verbose: false,
        }
    }
}

/// Resolve hostname to IP address
fn resolve_host(host: &str) -> std::io::Result<IpAddr> {
    (host, 0)
        .to_socket_addrs()
        .map(|itr| itr.map(|a| a.ip()).collect::<Vec<_>>()[0])
}
