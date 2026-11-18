#!/usr/bin/env bash
# Spec 1613: apply each mutation, run the spec's tests, restore with cp.
# A mutation is caught when at least one test fails. Usage, from the repo root:
#   specs/1613-dot-directory-manifests/measurements/mutate-run.sh
set -uo pipefail
M=specs/1613-dot-directory-manifests/measurements
BK=$(mktemp -d -t waybill1613-mut)
for name in ${MUTATIONS:-M1-pip-no-descend M2-case-sensitive M3-venv-listed M4-no-ancestor-rule M5-selector-not-widened M6-tox-accepted M7-no-r5-pass M8-any-not-all M9-overwrites-scope M10-optional-pair-not-kept M11-cross-root-first-wins M12-explicit-runtime-skipped}; do
  file=$(python3 -I -B $M/mutate.py "$name") || { echo "$name: anchor failed"; continue; }
  # mutate.py printed the file after editing it; back up from git's copy.
  git show "HEAD:$file" > "$BK/orig"
  out=$( { cargo test -q -p waybill --test scan_dot_directory_manifests 2>&1; \
           cargo test -q -p waybill --bins deduplicator 2>&1; \
           cargo test -q -p waybill --bins s1613_ 2>&1; \
           cargo test -q -p waybill --bins ci_scope 2>&1; \
           cargo test -q -p waybill --bins project_roots::tests 2>&1; } )
  failed=$(printf '%s\n' "$out" | grep -E '^test result: FAILED' | wc -l | tr -d ' ')
  compile=$(printf '%s\n' "$out" | grep -cE '^error\[E[0-9]+\]')
  reds=$(printf '%s\n' "$out" | grep -oE "^thread '[^']+'" | sed "s/^thread '//; s/'$//" | sed 's/.*:://' | sort -u | tr '\n' ' ')
  cp "$BK/orig" "$file"
  cmp -s "$BK/orig" "$file" && restored=yes || restored=NO
  echo "$name | failed-binaries=$failed | compile-errors=$compile | restored=$restored | red: $reds"
done
git status --short waybill-cli/src
