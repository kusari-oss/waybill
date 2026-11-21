#!/usr/bin/env bash
# Spec 1638: apply each mutation, run the spec's tests, restore with cp from
# HEAD, and rebuild after the last restore. A mutation is caught when at
# least one test fails and nothing failed to compile. Usage, from the
# repository root:
#   specs/1638-unresolved-github-fetch/measurements/mutate-run.sh
set -uo pipefail
M=specs/1638-unresolved-github-fetch/measurements
BK=$(mktemp -d -t waybill1638-mut)
ALL="M1-github-branch-off M2-literal-repo-check-off M3-no-download-url M4-wrapper-mask-off M5-reconciler-guard-off M6-url-fetch-gets-url"
for name in ${MUTATIONS:-$ALL}; do
  file=$(python3 -I -B $M/mutate.py "$name") || { echo "$name: anchor failed"; continue; }
  git show "HEAD:$file" > "$BK/orig"
  out=$( { cargo test -q -p waybill --test cmake_unresolved_github_fetch 2>&1; \
           cargo test -q -p waybill --bins -- s1638_unresolved_shapes wrapper_recognition_is_unchanged a_versionless_github_cmake_fetch 2>&1; } )
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
