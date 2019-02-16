extern crate cargo_web;
extern crate env_logger;
extern crate structopt;

use std::env::var;
use std::process::exit;

use cargo_web::CargoWeb;
use structopt::StructOpt;

fn main() {
    if let Ok(value) = var("CARGO_WEB_LOG") {
        let mut builder = env_logger::Builder::new();
        builder.parse(&value);
        builder.init();
    }

    if let Err(error) = CargoWeb::from_args().run() {
        eprintln!("error: {}", error);
        exit(101);
    }
}
