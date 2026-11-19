#!/usr/bin/env bash
# Spec 1615: apply each mutation, run the spec's tests, restore with cp from
# HEAD, and rebuild after the last restore. A mutation is caught when at
# least one test fails. Usage, from the repository root:
#   specs/1615-nested-req-scope/measurements/mutate-run.sh
set -uo pipefail
M=specs/1615-nested-req-scope/measurements
BK=$(mktemp -d -t waybill1615-mut)
ALL="M1-no-doc-word M2-no-long-forms M3-no-tests-word M4-no-docs-directory-names M5-no-test-directory-names M6-farthest-first M7-dot-directories-searched-first M8-root-name-counts M9-directory-prefix-match M10-reader-keeps-old-words M11-c155-kept-on-runtime M12-optional-beats-runtime-declaration"
for name in ${MUTATIONS:-$ALL}; do
  file=$(python3 -I -B $M/mutate.py "$name") || { echo "$name: anchor failed"; continue; }
  git show "HEAD:$file" > "$BK/orig"
  out=$( { cargo test -q -p waybill --test scan_nested_req_scope 2>&1; \
           cargo test -q -p waybill --bins requirements_txt:: 2>&1; } )
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
