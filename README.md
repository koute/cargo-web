[![Build Status](https://api.travis-ci.org/koute/cargo-web.svg)](https://travis-ci.org/koute/cargo-web)

# A cargo subcommand for the client-side Web

This cargo subcommand aims to make it easy and convenient to build, develop
and deploy client-side Web applications written in Rust.

It's currently very early in development; for now it supports
the following sub-subcommands:

  * `cargo web build` - a poor alias for `cargo build --target=asmjs-unknown-emscripten`
  * `cargo web test` - automatically runs your tests in a web browser
  * `cargo web start` - builds the project, starts an embedded webserver
                        and rebuilds as needed

It will also automatically download Emscripten for you (i686 and x86_64 Linux only for now).

Other features which are (eventually) planned but are yet not here:

  * Fully headless test running.
  * Feature parity with cargo.
  * Built-in minification.
  * Possibly a bridge into the `npm` ecosystem to fetch JavaScript libraries.
  * Anything else you might expect from a tool like this (suggestions welcome!).

## Installation

    $ cargo install cargo-web

To upgrade:

    $ cargo install --force cargo-web

Or clone and build with `$ cargo build` then place in your $PATH.

## License

Licensed under either of

  * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
  * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
