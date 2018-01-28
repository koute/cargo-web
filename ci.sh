#!/bin/bash

set -euo pipefail
IFS=$'\n\t'

export REPOSITORY_ROOT="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
export CARGO_WEB=$REPOSITORY_ROOT/target/debug/cargo-web

ONLY_LOCAL=0
for ARG in "$@"
do
    if [ "$ARG" == "--only-local" ]; then
        ONLY_LOCAL=1
    else
        echo "Unknown argument: '$ARG'"
        exit 1
    fi
done

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

cd $REPOSITORY_ROOT

echo "++ Is Rust from nightly: $IS_NIGHTLY"

cargo test
cargo build

echo ""

cd integration-tests
cargo run
cd ..

echo "++ Basic test crate tests passed!"

if [ "$ONLY_LOCAL" == "1" ]; then
    echo "++ Will not run further tests since I was called with '--only-local'!"
    exit 0
fi

if [ -d "../stdweb" ]; then
    cd ../stdweb
else
    git clone --depth 1 https://github.com/koute/stdweb.git
    cd stdweb
fi

$CARGO_WEB test --nodejs --target-asmjs-emscripten
$CARGO_WEB test --target-asmjs-emscripten
$CARGO_WEB test --nodejs --target-webasm-emscripten
$CARGO_WEB test --target-asmjs-emscripten

if [ "$IS_NIGHTLY" = "1" ]; then
    $CARGO_WEB test --nodejs --target-webasm

    cd examples/hasher
    $CARGO_WEB build --target-webasm
    node example.js
fi
