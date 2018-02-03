#!/bin/bash

set -euo pipefail

PROJECT_NAME=$(cat Cargo.toml | ruby -e 'STDIN.read =~ /name *= *"(.+?)"/; puts $1')

if [ -z "${DEPLOY_TARGETS-}" ]; then
    echo "DEPLOY_TARGETS is empty; aborting!"
    exit 1
fi

echo "Building artifacts for deployment..."

rm -Rf travis-deployment
mkdir -p travis-deployment

for TARGET in $DEPLOY_TARGETS; do
    echo "Target: $TARGET"
    rustup target add $TARGET || true
    cargo build --release --target=$TARGET

    FILE=target/$TARGET/release/$PROJECT_NAME
    strip $FILE || true
    cat $FILE | gzip > travis-deployment/$(basename $FILE)-$TARGET.gz
done
