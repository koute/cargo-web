#!/bin/bash

set -euo pipefail
IFS=$'\n\t'

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
CARGO_WEB=$SCRIPT_DIR/target/debug/cargo-web

export RUST_BACKTRACE=1
export CARGO_WEB_LOG=cargo_web=debug

set +e
echo "$(rustc --version)" | grep -q "nightly"
if [ "$?" = "0" ]; then
    export IS_NIGHTLY=1
else
    export IS_NIGHTLY=0
fi
set -e

cd $SCRIPT_DIR

echo "Is Rust from nightly: $IS_NIGHTLY"

cargo test
cargo build

if [ -d "../stdweb" ]; then
    cd ../stdweb
else
    git clone --depth 1 https://github.com/koute/stdweb.git
    cd stdweb
fi

rustup target add asmjs-unknown-emscripten
$CARGO_WEB test --nodejs --target-asmjs-emscripten
$CARGO_WEB test --target-asmjs-emscripten

rustup target add wasm32-unknown-emscripten
$CARGO_WEB test --nodejs --target-webasm-emscripten
$CARGO_WEB test --target-asmjs-emscripten

if [ "$IS_NIGHTLY" = "1" ]; then
    rustup target add wasm32-unknown-unknown
    $CARGO_WEB test --nodejs --target-webasm

    cd examples/hasher
    $CARGO_WEB build --target-webasm
    node example.js

    cd $SCRIPT_DIR/test-crates/native-webasm
    $CARGO_WEB build --target-webasm
    node run.js
fi
