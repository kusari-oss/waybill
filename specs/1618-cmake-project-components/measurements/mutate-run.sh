#!/usr/bin/env bash
# Spec 1618: apply each mutation, run the spec's tests, restore with cp from
# HEAD, and rebuild after the last restore. A mutation is caught when at
# least one test fails. Usage, from the repository root:
#   specs/1618-cmake-project-components/measurements/mutate-run.sh
set -uo pipefail
M=specs/1618-cmake-project-components/measurements
BK=$(mktemp -d -t waybill1618-mut)
ALL="M1-cmake-always-root-candidate M2-cmake-rank-above-main-modules M3-no-enclosing-edge M4-no-search-pairing M5-expression-emitted-as-absence M6-test-directory-unscoped M7-reach-ignored M8-include-vendored-not-merged M9-conditional-search-makes-optional M10-paired-project-serialised-at-zero M11-collisions-not-dropped M12-vendored-rule-either-signal M13-cmake-root-not-held-back"
for name in ${MUTATIONS:-$ALL}; do
  file=$(python3 -I -B $M/mutate.py "$name") || { echo "$name: anchor failed"; continue; }
  git show "HEAD:$file" > "$BK/orig"
  out=$( { cargo test -q -p waybill --test cmake_project_components 2>&1; \
           cargo test -q -p waybill --bins -- cmake::projects:: declared_attach:: root_selector:: cmake_search_pair:: 2>&1; } )
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
