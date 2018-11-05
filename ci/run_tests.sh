#!/bin/bash

set -euo pipefail
IFS=$'\n\t'

export REPOSITORY_ROOT="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )/.."
export CARGO_WEB=$REPOSITORY_ROOT/target/debug/cargo-web

TEST_SUBSET=${TEST_SUBSET:-0}
CHECK_ONLY=${CHECK_ONLY:-0}
WITHOUT_CARGO_LOCK=${WITHOUT_CARGO_LOCK:-0}

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

if [ "$WITHOUT_CARGO_LOCK" == "1" ]; then
    echo "++ Will compile itself without a preset Cargo.lock!"
    rm -f Cargo.lock
fi

if [ "$CHECK_ONLY" == "1" ]; then
    echo "++ Will only check whenever we compile!"
    cargo check
    exit 0
fi

cargo build

if [[ "$TEST_SUBSET" == 0 || "$TEST_SUBSET" == 1 ]]; then
    cargo test

    rustup target add x86_64-apple-darwin
    rustup target add i686-pc-windows-gnu

    cargo check --target=x86_64-apple-darwin
    cargo check --target=i686-pc-windows-gnu

    echo ""

    cd integration-tests
    cargo test -- --test-threads=1
    cd ..

    echo "++ Basic test crate tests passed!"
fi

if [[ "$TEST_SUBSET" == 0 || "$TEST_SUBSET" == 2 ]]; then
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

    ./ci/run_tests.sh
fi
