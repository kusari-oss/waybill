#!/usr/bin/env bash
# Scan the five nixpkgs wrapper flakes with a given waybill binary.
# Usage: scan-wrappers.sh <waybill-bin> <wrappers-dir> <out-dir>
set -uo pipefail
BIN="$1"; W="$2"; OUT="$3"
for t in curl openssh httpie gh ripgrep; do
  o="$OUT/$t"; mkdir -p "$o"
  RUST_LOG=info "$BIN" sbom scan --nix-closure --no-deps-dev --no-clearly-defined \
    --path "$W/$t" \
    --format cyclonedx-json,spdx-2.3-json,spdx-3-json \
    --output "cyclonedx-json=$o/cdx.json" \
    --output "spdx-2.3-json=$o/spdx.json" \
    --output "spdx-3-json=$o/spdx3.json" \
    --output "openvex=$o/waybill.openvex.json" \
    > "$o/stdout.log" 2> "$o/stderr.log"
  echo "$t exit=$?"
done
