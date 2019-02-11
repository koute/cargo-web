#![deny(
    // missing_debug_implementations,
    trivial_numeric_casts,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

extern crate clap;
extern crate notify;
extern crate hyper;
extern crate http;
extern crate futures;
extern crate tempfile;
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
#[macro_use]
extern crate lazy_static;
extern crate directories;
extern crate percent_encoding;

extern crate parity_wasm;
#[macro_use]
extern crate log;
extern crate rustc_demangle;
extern crate env_logger;

extern crate cargo_metadata;
extern crate ansi_term;

extern crate semver;
extern crate memmap;

extern crate open;
extern crate atty;
#[macro_use]
extern crate failure;

pub mod cargo_shim;

#[macro_use]
mod utils;
mod project_dirs;
mod http_utils;
mod config;
mod package;
pub mod build;
pub mod deployment;
pub mod error;
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
mod wasm_js_snippet;
mod emscripten;
mod test_chromium;
mod chrome_devtools;
pub mod cmd_build;
pub mod cmd_start;
pub mod cmd_test;
pub mod cmd_deploy;
pub mod cmd_prepare_emscripten;

pub use build::BuildArgs;
pub use deployment::Deployment;
pub use error::Error;
