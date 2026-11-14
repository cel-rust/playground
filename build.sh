#!/usr/bin/env sh
# Builds the site into dist/: the files of web/, a wasm module for each version
# of cel the playground offers (see versions.sh), cel-<version>.wasm, and
# versions.json, which lists those versions newest first. That's what
# `cargo run --example localhost` serves too.
set -eu
cd "$(dirname "$0")"

# Cargo.lock as it is, given --locked: not updated to list them
lock=
for arg in "$@"; do
  case $arg in --locked | --frozen | --offline) lock="$lock $arg" ;; esac
done
# shellcheck disable=SC2086 # one word per flag
versions=$(./versions.sh $lock)

rm -rf dist
mkdir dist
cp web/* dist/
echo "$versions" | while read -r feature version; do
  cargo build --release --target wasm32-unknown-unknown --no-default-features --features "$feature" "$@"
  cp target/wasm32-unknown-unknown/release/cel_playground.wasm "dist/cel-$version.wasm"
  echo "built dist/cel-$version.wasm ($(wc -c <"dist/cel-$version.wasm") bytes of wasm)"
done
echo "$versions" | awk '{ printf "%s\"%s\"", NR == 1 ? "[" : ", ", $2 } END { print "]" }' >dist/versions.json
echo "built dist/versions.json: $(cat dist/versions.json)"
