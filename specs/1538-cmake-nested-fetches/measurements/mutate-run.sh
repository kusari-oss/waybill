#!/bin/bash
# Spec 1538 T025: apply each mutation, run the tests that guard it, record
# the failing tests, restore every touched file with cp, and check the tree
# is clean against the commit. Usage: mutate-run.sh <scratch-dir>
set -u
REPO=/Users/mlieberman/Projects/mikebom
M=$REPO/specs/1538-cmake-nested-fetches/measurements/mutate.py
D=$1
mkdir -p "$D"
cd "$REPO"
: > "$D/results.tsv"
for m in M1 M2 M3 M4 M5 M6 M7 M8 M9 M10 M11 M12 M13 M14 M15 M16 M17 M18; do
  paths=$(python3 -I -B "$M" "$REPO" --paths "$m")
  for p in $paths; do mkdir -p "$D/backup/$m/$(dirname "$p")"; cp "$REPO/$p" "$D/backup/$m/$p"; done
  nfail=0; compile=0; failed=""
  if ! python3 -I -B "$M" "$REPO" "$m" > /dev/null; then
    echo -e "$m\tANCHOR-MISS" >> "$D/results.tsv"
  else
    {
      cargo test -p waybill --bin waybill -- cmake:: cmake_fetch_attach cmake_version
      cargo test -p waybill --test cmake_nested_fetches --no-fail-fast
    } > "$D/$m.log" 2>&1
    failed=$(grep -E '^test .* FAILED$' "$D/$m.log" | sed -E 's/^test (.*) \.\.\. FAILED$/\1/' | sort -u | tr '\n' ' ')
    nfail=$(grep -cE '^test .* FAILED$' "$D/$m.log")
    compile=$(grep -cE '^error(\[|:)' "$D/$m.log")
  fi
  for p in $paths; do cp "$D/backup/$m/$p" "$REPO/$p"; done
  clean=$(git -C "$REPO" diff --quiet -- $paths && echo clean || echo DIRTY)
  echo -e "$m\tfailing=$nfail\tcompile_errors=$compile\t$clean\t$paths\t$failed" >> "$D/results.tsv"
done
echo DONE >> "$D/results.tsv"
