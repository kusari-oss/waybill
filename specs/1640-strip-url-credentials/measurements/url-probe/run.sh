#!/bin/sh
# Build probe.rs against the workspace's pinned `url` version in a
# temporary crate (a Cargo.toml here would join the workspace and match
# FR-010's Cargo.toml check). Usage: sh run.sh   (from this directory)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../../.." && pwd)
v=$(grep -A1 '^name = "url"$' "$root/Cargo.lock" | sed -n 's/version = "\(.*\)"/\1/p')
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/src"
printf '[package]\nname = "urlprobe"\nversion = "0.0.0"\nedition = "2021"\n[workspace]\n[dependencies]\nurl = "=%s"\n' "$v" > "$tmp/Cargo.toml"
cp "$here/probe.rs" "$tmp/src/main.rs"
echo "url $v"
(cd "$tmp" && cargo run -q --offline)
