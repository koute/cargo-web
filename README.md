[![Build Status](https://api.travis-ci.org/koute/cargo-web.svg)](https://travis-ci.org/koute/cargo-web)

# A cargo subcommand for the client-side Web

This cargo subcommand aims to make it easy and convenient to build, develop
and deploy client-side Web applications written in Rust.

## Donate

[![Become a patron](https://koute.github.io/img/become_a_patron_button.png)](https://www.patreon.com/koute)

## Patrons

This software was brought to you thanks to these wonderful people:
  * Embark Studios
  * Joe Narvaez
  * Eduard Knyshov
  * Anselm Eickhoff
  * Johan Andersson
  * Stephen Sugden
  * is8ac

Thank you!

## Features

Currently it supports the following features:

  * `cargo web build` - will build your project using one of Rust's three Web backends:
    * [WebAssembly] using Rust's native WebAssembly backend (when you pass `--target=wasm32-unknown-unknown`; default)
    * [WebAssembly] using Emscripten (when you pass `--target=wasm32-unknown-emscripten`)
    * [asm.js] using Emscripten (when you pass `--target=asmjs-unknown-emscripten`)
  * `cargo web check` - will typecheck your project
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
  * Will automatically install the relevant Rust target through `rustup`

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
if [ "$(uname -s)" == "Darwin" ]; then
  CARGO_WEB_HOST_TRIPLE="x86_64-apple-darwin"
else
  CARGO_WEB_HOST_TRIPLE="x86_64-unknown-linux-gnu"
fi
CARGO_WEB_URL="https://github.com/koute/cargo-web/releases/download/$CARGO_WEB_VERSION/cargo-web-$CARGO_WEB_HOST_TRIPLE.gz"


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

## Custom runtime (`wasm32-unknown-unknown`-only)

When building a project by default `cargo-web` generates a standalone runtime
runtime for you. What this means is that the `.js` file which is generated
can be immediately put inside of a `<script>` tag or launched with Node.js
without having to load it manually or do anything extra, however this does
limit you when it comes to customizability.

If you'd like to have a little more control on how your module is loaded
then you can tell `cargo-web` to generate a non-standalone, library-like
module for you with the `--runtime library-es6` option. This will result
in a `.js` file which exports a factory function with the following interface:

```js
export default function() {
    return {
        imports: { ... },
        initialize: function( instance ) { ... }
    };
}
```

Here you have to instantiate the WebAssembly module yourself; in this
case you have to pass `imports` as its imports, and then immediately
after instantiating it call `initialize`.

For example, assuming you'll name your module generated by the `cargo-web`
as `my-module.mjs` and `my-module.wasm` you can instantiate it like this from Node.js:

```js
import fs from "fs";
import factory from "my-module.mjs";

// We read in the `.wasm` module.
const bytecode = fs.readFileSync( "my-module.wasm" );
const wasm = new WebAssembly.Module( bytecode );

// We instantiate it.
const instance = factory();
const compiled = new WebAssembly.Instance( wasm, instance.imports );

// This will initialize the module and call your `main`, if you have one.
const exports = instance.initialize( compiled );

// In the object it returns you can find any functions which
// you've exported with `stdweb`'s `#[js_export]` macro.
console.log( exports.add( 1, 2 ) );
```

Then you can run it with `node --experimental-modules run.mjs`.

This is useful if you want to load your `.wasm` file from a custom URL or
you want to integrate the output with a JavaScript bundler, or anything
else which requires you to load the module yourself.

## Changelog
   * `0.6.24`
      * Conditional dependencies of form `[target.'cfg(...)'.dependencies]` are now properly supported
      * You can now use `cfg(cargo_web)` to detect whenever your crate is being compiled under `cargo-web`
      * Artifacts matching `target/wasm32-unknown-unknown/*/deps/*.wasm` are now ignored; this should prevent
        `cargo-web` from processing superfluous `.wasm` artifacts generated due to dependencies also being `cdylib`s
      * `cargo-web` is now available as a library through a `structopt`-based interface
   * `0.6.23`
      * New subcommand: `cargo web check`
      * The `wasm32-unknown-unknown` target is now the default
   * `0.6.22`
      * Running tests through Chrome should now work out-of-box on macOS
      * The `deploy` subcommand can now be told where to deploy using the `-o`/`--output` parameter
      * Static files with spaces in their names are now properly served
      * `Access-Control-Allow-Origin: *` is now always sent by the embedded webserver
      * Debug builds on `wasm32-unknown-unknown` are now supported provided a recent enough `stdweb` is used
   * `0.6.21`
      * Emscripten was updated to `1.38.19`; the Emscripten-based targets should now work again on nightly
      * Broken output redirection in the test runner is now fixed
      * The generated JS snippets and imports under `wasm32-unknown-unknown` are now sorted
      * Compatibility with *really* old nightlies was removed for `wasm32-unknown-unknown`
   * `0.6.20`
      * Installation through `cargo install` should now work again
      * Most of the dependencies were updated to newer versions
      * `deploy` should not panic when it doesn't find a valid target
   * `0.6.19`
      * `cargo install` should now compile instead of failing in some environments
      * Minimum required Rust version is now `1.26.2`
   * `0.6.18`
      * Default `index.html` doesn't have a newline before its doctype anymore
   * `0.6.17`
      * OpenSSL is now vendored; this should fix the compilation in some environments
      * Minimum required Rust version is now `1.25.0`
   * `0.6.16`
      * The runtime for `wasm32-unknown-unknown` now uses `WebAssembly.instantiateStreaming` when available
      * Running tests under headless Chromium is now supported for the `wasm32-unknown-unknown` target
      * Color codes are no longer emitted when the output of `cargo-web` is redirected
      * Improved coloring; a lot more messages should now be colored
      * Initial experimental support for asynchronous tests

## License

Licensed under either of

  * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
  * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
