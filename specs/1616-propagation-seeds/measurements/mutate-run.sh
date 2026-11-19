#!/usr/bin/env bash
# Spec 1616: apply each mutation, run the spec's tests, restore with cp from
# HEAD, and rebuild after the last restore. A mutation is caught when at
# least one test fails. Usage, from the repository root:
#   specs/1616-propagation-seeds/measurements/mutate-run.sh
set -uo pipefail
M=specs/1616-propagation-seeds/measurements
BK=$(mktemp -d -t waybill1616-mut)
ALL="M1-no-added-seeds M2-non-runtime-main-modules-seeded M3-dot-directory-main-modules-seeded M4-added-seeds-replace-seeds M5-count-includes-base-protection M6-added-seeds-cross-excluded-edges"
for name in ${MUTATIONS:-$ALL}; do
  file=$(python3 -I -B $M/mutate.py "$name") || { echo "$name: anchor failed"; continue; }
  git show "HEAD:$file" > "$BK/orig"
  out=$( { cargo test -q -p waybill --test propagation_seeds 2>&1; \
           cargo test -q -p waybill --bins scope_propagation:: 2>&1; } )
  failed=$(printf '%s\n' "$out" | grep -E '^test result: FAILED' | wc -l | tr -d ' ')
  compile=$(printf '%s\n' "$out" | grep -cE '^error\[E[0-9]+\]')
  reds=$(printf '%s\n' "$out" | grep -oE "^thread '[^']+'" | sed "s/^thread '//; s/'$//" | sed 's/.*:://' | sort -u | tr '\n' ' ')
  cp "$BK/orig" "$file"
  cmp -s "$BK/orig" "$file" && restored=yes || restored=NO
  echo "$name | failed-binaries=$failed | compile-errors=$compile | restored=$restored | red: $reds"
done
git status --short waybill-cli/src
# The last test run built the last mutation's binary; rebuild from disk.
cargo build -q -p waybill
