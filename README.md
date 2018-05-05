[![Build Status](https://api.travis-ci.org/koute/cargo-web.svg)](https://travis-ci.org/koute/cargo-web)

# A cargo subcommand for the client-side Web

This cargo subcommand aims to make it easy and convenient to build, develop
and deploy client-side Web applications written in Rust.

## Donate

[![Become a patron](https://koute.github.io/img/become_a_patron_button.png)](https://www.patreon.com/koute)

## Patrons

This software was brought to you thanks to these wonderful people:
  * Daniel Norman
  * Ben Berman
  * Stephen Sugden

Thank you!

## Features

Currently it supports the following features:

  * `cargo web build` - will build your project using one of Rust's three Web backends:
    * [asm.js] using Emscripten (when you pass `--target=asmjs-unknown-emscripten`; default)
    * [WebAssembly] using Emscripten (when you pass `--target=wasm32-unknown-emscripten`)
    * [WebAssembly] using Rust's native WebAssembly backend (when you pass `--target=wasm32-unknown-unknown`, requires Rust nightly)
  * `cargo web test` - will run your tests either under:
    * Under a headless instance of Google Chrome (default)
    * Under [Node.js] (when you pass `--nodejs`)
  * `cargo web start` - will build your project, start an embedded webserver and will continuously
    rebuild it if necessary; supports automatic reloading with `--auto-reload`.
  * `cargo web deploy` - will build your project and emit all of the necessary files so that
    you can easily serve them statically.
  * Will automatically download and install Emscripten for you (if necessary) on the following platforms:
    * Linux x86-64
    * Linux x86
  * Will automatically garbage-collect your WebAssembly artifacts.

[asm.js]: https://en.wikipedia.org/wiki/Asm.js
[WebAssembly]: https://en.wikipedia.org/wiki/WebAssembly
[Node.js]: https://nodejs.org/en/

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

On Linux the installation can fail with a message that it can't find OpenSSL,
in which case you most likely need to install the `-dev` package for OpenSSL
from your distribution's repositories. (On Ubuntu it's called `libssl-dev`.)

## `Web.toml`

`cargo-web` has its own configuration file which you can put next to `cargo`'s [`Cargo.toml`].

Here's an example configuration showing every supported key:

```toml
# The default value of `--target` used when building this crate
# in cases where it's not specified on the command line.
default-target = "wasm32-unknown-unknown"

# This will prepend a given JavaScript file to the resulting `.js` artifact.
# You can put any initialization code here which you'd like to have executed
# when your `.js` file first loads.
#
# This accepts either a string (as shown here), or an array of strings,
# in which case it will prepend all of the specified files in their
# order of appearance.
prepend-js = "src/runtime.js"

# `deploy-path` specifies the location where the files are copied when
# running `cargo web deploy`. It can be an absolute path, or relative path
# to the location of Cargo.toml. If specified, the folder must exist.
# The default value of `deploy-path` is `target/deploy`.
#
# Please remember that the **current content of `deploy-path` will be REMOVED**
# during the deployment.
deploy-path = "path/to/deploy/folder/"

# `serve-path` specifies the location (sub and relative to `deploy-path`) where
# `.js` and `.wasm` files are output to.
# The default value of `serve-path` is "/" (means the root of `deploy-path`)
serve-path = "js/and/wasm/folder"

[cargo-web]
# Asserts the minimum required version of `cargo-web` necessary
# to compile this crate; supported since 0.6.0.
minimum-version = "0.6.0"

# These will only take effect on *-emscripten targets.
[target.emscripten]
# You can have a target-specific `prepend-js` key.
prepend-js = "src/emscripten_runtime.js"
# This will enable Emscripten's SDL2 port. Consult Emscripten's documentation
# for more details.
link-args = ["-s", "USE_SDL=2"]

# You can specify `deploy-path` and `serve-path` that are specific to a target.
# (If this is specified, then the global `deploy-path` and `serve-path` is not allowed)
deploy-path = "path/to/deploy/folder/"
serve-path = "js/and/wasm/folder"

# You can also specify the target by its full name.
[target.wasm32-unknown-unknown]
prepend-js = "src/native_runtime.js"
```

If you use any external crates which have a `Web.toml` then `cargo-web`
**will** load it and use it.

A few restrictions concerning the `Web.toml`:

  * You can't have overlapping `prepend-js` keys. You can either define
    a single global `prepend-js`, or multiple per-target ones.
  * The `link-args` currently can't have any spaces in them.
  * The order in which `cargo-web` will process the `Web.toml` files
    from multiple crates is deterministic yet unspecified. This means
    that you shouldn't depend on this order in any way.

[`Cargo.toml`]: https://doc.rust-lang.org/cargo/reference/manifest.html

## Static files

Any static files you'd like to have served when running `cargo web start` or deployed
when running `cargo web deploy` can be put in a directory called `static` in the root
of your crate. No static artifacts are required by default; an `index.html` file will
be automatically generated for you if it's missing. You can, of course, put your own `static/index.html`
file, in which case it will be used instead of the autogenerated one.

## Detecting `cargo-web` during compilation

If during compilation you'd like to detect that your project is being built with `cargo-web`
you can check the `COMPILING_UNDER_CARGO_WEB` environment variable, which will be set to `1`.

## Using `cargo-web` on Travis

### Precompiled binaries

You can use the following script to download and install the latest `cargo-web`:

```shell
CARGO_WEB_RELEASE=$(curl -L -s -H 'Accept: application/json' https://github.com/koute/cargo-web/releases/latest)
CARGO_WEB_VERSION=$(echo $CARGO_WEB_RELEASE | sed -e 's/.*"tag_name":"\([^"]*\)".*/\1/')
CARGO_WEB_URL="https://github.com/koute/cargo-web/releases/download/$CARGO_WEB_VERSION/cargo-web-x86_64-unknown-linux-gnu.gz"

echo "Downloading cargo-web from: $CARGO_WEB_URL"
curl -L $CARGO_WEB_URL | gzip -d > cargo-web
chmod +x cargo-web

mkdir -p ~/.cargo/bin
mv cargo-web ~/.cargo/bin
```

### Running tests under headless Chrome

By default `cargo web test` will run your tests under headless Chrome. To be able to use this on Travis
you need to add something like this to your `.travis.yml`:

```yaml
addons:
  chrome: stable
```

## License

Licensed under either of

  * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
  * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
