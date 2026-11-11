#!/usr/bin/env bash
# Dump `nix derivation show -r` for each wrapper-flake target.
# Usage: TARGETS_DIR=<dir> DRV_DIR=<dir> drv-dump.sh
set -uo pipefail
T="${TARGETS_DIR:?set TARGETS_DIR}"
D="${DRV_DIR:?set DRV_DIR}"
mkdir -p "$D"
for t in curl openssh httpie gh ripgrep moat; do
  nix derivation show -r "$T/$t#default" > "$D/$t.json" 2> "$D/$t.err"
  echo "$t exit=$? bytes=$(wc -c < "$D/$t.json" | tr -d ' ')"
done
