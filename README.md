[![Build Status](https://api.travis-ci.org/koute/cargo-web.svg)](https://travis-ci.org/koute/cargo-web)

# A cargo subcommand for the client-side Web

This cargo subcommand aims to make it easy and convenient to build, develop
and deploy client-side Web applications written in Rust.

## Donate

[![Become a patron](https://koute.github.io/img/become_a_patron_button.png)](https://www.patreon.com/koute)

## Patrons

This software was brought to you thanks to these wonderful people:
  * Ben Berman

Thank you!

## Features

Currently it supports the following features:

  * `cargo web build` - will build your project using one of Rust's three Web backends:
    * [asm.js] using Emscripten (when you pass `--target-asmjs-emscripten`; default)
    * [WebAssembly] using Emscripten (when you pass `--target-webasm-emscripten`)
    * [WebAssembly] using Rust's native WebAssembly backend (when you pass `--target-webasm`)
  * `cargo web test` - will run your tests either under:
    * Under a headless instance of Google Chrome (default)
    * Under [Node.js] (when you pass `--nodejs`)
  * `cargo web start` - will build your project, start an embedded webserver and will continuously
    rebuild it if necessary; supports automatic reloading with `--auto-reload`.
  * Will automatically download and install Emscripten for you (if necessary) on the following platforms:
    * Linux x86-64
    * Linux x86
  * Will automatically garbage-collect your WebAssembly artifacts.

[asm.js]: https://en.wikipedia.org/wiki/Asm.js
[WebAssembly]: https://en.wikipedia.org/wiki/WebAssembly
[Node.js]: https://nodejs.org/en/

Before compiling anything you will have to install the corresponding targets
with `rustup` yourself:

  * For compiling to asmjs through Emscripten:
        `rustup target add asmjs-unknown-emscripten`
  * For compiling to WebAssembly through Emscripten:
        `rustup target add wasm32-unknown-emscripten`
  * For compiling to WebAssembly through Rust's native backend:
        `rustup target add wasm32-unknown-unknown`

It's also highly recommended that you check out the [stdweb] crate if you want
to interact with the JavaScript world in your project. (In fact, `cargo-web`
is what makes it possible to use `stdweb`'s `js!` macro on Rust's native WebAssembly
backend.)

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
