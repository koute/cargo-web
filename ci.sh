#!/bin/bash

set -euo pipefail
IFS=$'\n\t'

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
CARGO_WEB=$SCRIPT_DIR/target/debug/cargo-web

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

cd $SCRIPT_DIR

echo "++ Is Rust from nightly: $IS_NIGHTLY"

cargo test
cargo build

rustup target add asmjs-unknown-emscripten
rustup target add wasm32-unknown-emscripten

if [ "$IS_NIGHTLY" = "1" ]; then
    rustup target add wasm32-unknown-unknown
fi

assert_compiles () {
    echo "++ Checking that it succeeds: $1 for $2 '$3'..."
    echo ""
    pushd test-crates/$3 > /dev/null
    if [ "$2" == "all" ] || [ "$2" == "asmjs-emscripten" ]; then
        $CARGO_WEB $1 --target-asmjs-emscripten
    fi
    if [ "$2" == "all" ] || [ "$2" == "wasm-emscripten" ]; then
        $CARGO_WEB $1 --target-webasm-emscripten
    fi
    if [ "$2" == "all" ] || [ "$2" == "wasm" ]; then
        if [ "$IS_NIGHTLY" = "1" ]; then
            $CARGO_WEB $1 --target-webasm
        fi
    fi
    popd > /dev/null
    echo ""
}

assert_aborts () {
    echo "++ Checking that it fails: $1 for $2 '$3'..."
    echo ""
    pushd test-crates/$3 > /dev/null
    if [ "$2" == "all" ] || [ "$2" == "asmjs-emscripten" ]; then
        (! $CARGO_WEB $1 --target-asmjs-emscripten)
    fi
    if [ "$2" == "all" ] || [ "$2" == "wasm-emscripten" ]; then
        (! $CARGO_WEB $1 --target-webasm-emscripten)
    fi
    if [ "$2" == "all" ] || [ "$2" == "wasm" ]; then
        if [ "$IS_NIGHTLY" = "1" ]; then
            (! $CARGO_WEB $1 --target-webasm)
        fi
    fi
    popd > /dev/null
    echo ""
}

echo ""

assert_compiles "build" "all" "workspace"
assert_compiles "build" "all" "conflicting-versions"

assert_compiles "build" "all" "requires-old-cargo-web"
assert_compiles "build" "all" "requires-future-cargo-web-through-disabled-dep"
assert_compiles "build" "all" "requires-future-cargo-web-through-dev-dep"
assert_compiles "build" "all" "requires-future-cargo-web-through-dep-dev-dep"
assert_compiles "build" "all" "requires-future-cargo-web-through-build-dep"

assert_compiles "build" "asmjs-emscripten" "requires-future-cargo-web-through-target-dep"
assert_aborts "build" "wasm-emscripten" "requires-future-cargo-web-through-target-dep"

assert_aborts "build" "all" "requires-future-cargo-web"
assert_aborts "build" "all" "requires-future-cargo-web-through-dep"
assert_aborts "build" "all" "requires-future-cargo-web-through-dep-dep"
assert_aborts "build" "all" "requires-future-cargo-web-through-dep-and-dev-dep"
assert_aborts "test" "all" "requires-future-cargo-web-through-dev-dep"

if [ "$IS_NIGHTLY" = "1" ]; then
    assert_compiles "build" "wasm" "native-webasm"

    pushd test-crates/native-webasm > /dev/null
    node run.js
    popd > /dev/null
fi

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
