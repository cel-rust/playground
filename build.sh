#!/usr/bin/env sh
# Builds the site into dist/: the files of web/ and the wasm module, which is
# what `cargo run --example localhost` serves too.
set -eu
cd "$(dirname "$0")"

cargo build --release --target wasm32-unknown-unknown "$@"

rm -rf dist
mkdir dist
cp web/* dist/
cp target/wasm32-unknown-unknown/release/cel_playground.wasm dist/
echo "built dist/ ($(wc -c <dist/cel_playground.wasm) bytes of wasm)"
