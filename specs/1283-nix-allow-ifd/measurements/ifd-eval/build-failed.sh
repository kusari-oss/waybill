#!/bin/sh
# T017 (#1283): nix's stderr when import-from-derivation is ALLOWED and the
# derivation built during evaluation fails, beside the same expression with
# IFD refused. A local derivation whose builder exits 1; nothing is fetched.
#
# usage: build-failed.sh <output-dir>
# writes: build-failed.err, build-failed.refused.err, build-failed.txt
set -u
out="$1"
expr='import (derivation { name = "waybill-fixture-ifd-broken"; system = "aarch64-darwin"; builder = "/bin/sh"; args = [ "-c" "echo waybill-fixture-ifd deliberately fails >&2; exit 1" ]; })'
nix --version > "$out/build-failed.txt"
nix eval --json --option allow-import-from-derivation true --expr "$expr" > /dev/null 2> "$out/build-failed.err"
echo "allowed: exit=$?" >> "$out/build-failed.txt"
nix eval --json --option allow-import-from-derivation false --expr "$expr" > /dev/null 2> "$out/build-failed.refused.err"
echo "refused: exit=$?" >> "$out/build-failed.txt"
