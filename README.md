[![Build Status](https://api.travis-ci.org/koute/cargo-web.svg)](https://travis-ci.org/koute/cargo-web)

# A cargo subcommand for the client-side Web

This cargo subcommand aims to make it easy and convenient to build, develop
and deploy client-side Web applications written in Rust.

It's currently very early in development; for now it supports
the following sub-subcommands:

  * `cargo web build` - builds your project
  * `cargo web test [--nodejs]` - automatically runs your tests in a web browser (experimental)
                                  or under Nodejs
  * `cargo web start` - builds the project, starts an embedded webserver
                        and rebuilds as needed

It supports all three of Rust's Web backends when passed one of the following parameters:

  * `--target-asmjs-emscripten` - builds for `asmjs-unknown-emscripten` (default)
  * `--target-webasm-emscripten` - builds for `wasm32-unknown-emscripten`
  * `--target-webasm` - builds for Rust's native `wasm32-unknown-unknown`, requires Rust nightly

Before compiling anything you will have to install the corresponding targets
with `rustup` yourself, e.g.:

    $ rustup target add asmjs-unknown-emscripten

On i686 and x86_64 Linux it will also automatically download Emscripten for you
when building for the `*-emscripten` targets.

It's also highly recommended that you check out the [stdweb] crate if you want
to interact with the JavaScript world in your project.

Other features which are (eventually) planned but are yet not here:

  * Fully headless test running.
  * Feature parity with cargo.
  * Built-in minification.
  * Possibly a bridge into the `npm` ecosystem to fetch JavaScript libraries.
  * Anything else you might expect from a tool like this (suggestions welcome!).

[stdweb]: https://github.com/koute/stdweb

## Installation

    $ cargo install cargo-web

To upgrade:

    $ cargo install --force cargo-web

Or clone and build with `$ cargo build --release` then place in your $PATH.

## License

Licensed under either of

  * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
  * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
