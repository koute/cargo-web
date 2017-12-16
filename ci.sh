#!/bin/sh

set -euo pipefail
IFS=$'\n\t'

if [ "$TARGET" = "none" ]; then
    cargo test --verbose
    exit 0
fi

set +e
echo "$(rustc --version)" | grep -q "nightly"
if [ "$?" = "0" ]; then
    export IS_NIGHTLY=1
else
    export IS_NIGHTLY=0
fi
set -e

echo "Is Rust from nightly: $IS_NIGHTLY"

if [ "$IS_NIGHTLY" = "0" ]; then
    if [ "$TARGET" = "wasm32-unknown-unknown" ]; then
        echo "Skipping tests; wasm32-unknown-unknown is only supported on nightly"
        exit 0
    fi
fi

cargo install -f
git clone https://github.com/koute/stdweb.git
cd stdweb

if [ "$TARGET" = "asmjs-unknown-emscripten" ]; then
    rustup target add asmjs-unknown-emscripten
    cargo web test --nodejs --target-asmjs-emscripten
fi

if [ "$TARGET" = "wasm32-unknown-emscripten" ]; then
    rustup target add wasm32-unknown-emscripten
    cargo web test --nodejs --target-webasm-emscripten
fi

if [ "$TARGET" = "wasm32-unknown-unknown" ]; then
    rustup target add wasm32-unknown-unknown
    cargo web test --nodejs --target-webasm
    cd examples/hasher
    cargo web build --target-webasm
    node example.js
fi
