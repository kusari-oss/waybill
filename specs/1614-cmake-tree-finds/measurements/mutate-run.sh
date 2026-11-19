#!/usr/bin/env bash
# Spec 1614: apply each mutation, run the spec's tests, restore with cp from
# HEAD, and rebuild after the last restore. A mutation is caught when at
# least one test fails. Usage, from the repository root:
#   specs/1614-cmake-tree-finds/measurements/mutate-run.sh
set -uo pipefail
M=specs/1614-cmake-tree-finds/measurements
BK=$(mktemp -d -t waybill1614-mut)
ALL="M1-unconditional-edge-does-not-win M2-first-path-not-union M3-closed-cycle-conditional M4-narrow-file-set M5-include-reached-files-not-read M6-reach-seed-not-applied M7-search-scope-ignores-conditions M8-tool-list-emptied M9-python-rule-ignores-development M10-fetch-pairing-removed M11-installed-pairing-removed M12-submodule-pairing-removed M13-in-tree-before-pairing M14-in-tree-rule-removed M15-config-template-patterns-removed M16-search-attachment-removed M17-c57-not-widened M18-record-scope-ignores-searches M19-module-names-unresolved M20-search-evidence-before-merge"
for name in ${MUTATIONS:-$ALL}; do
  file=$(python3 -I -B $M/mutate.py "$name") || { echo "$name: anchor failed"; continue; }
  git show "HEAD:$file" > "$BK/orig"
  out=$( { cargo test -q -p waybill --test scan_cmake_tree_finds 2>&1; \
           cargo test -q -p waybill --bins cmake:: 2>&1; \
           cargo test -q -p waybill --bins cmake_fetch_attach 2>&1; } )
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
