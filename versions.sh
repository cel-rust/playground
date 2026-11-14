#!/usr/bin/env sh
# The versions of cel the playground offers, newest first, one per line: the
# crate's feature that builds with it, `cel-0-N` (see Cargo.toml), and the
# version of the 0.N line in Cargo.lock. build.sh builds each; CI tests each.
#
# Its arguments go to `cargo pkgid`: `--locked` fails where Cargo.lock isn't
# up to date, rather than updating it.
set -eu
cd "$(dirname "$0")"

features=$(sed -n 's/^\(cel-[0-9]*-[0-9]*\) = \[.*/\1/p' Cargo.toml | sort -t- -k2,2nr -k3,3nr)
if [ -z "$features" ]; then
  echo "no cel-0-N features in Cargo.toml" >&2
  exit 1
fi
for feature in $features; do
  line=$(echo "${feature#cel-}" | tr - .)
  id=$(cargo pkgid --quiet "$@" "cel@$line")
  echo "$feature ${id##*@}"
done
