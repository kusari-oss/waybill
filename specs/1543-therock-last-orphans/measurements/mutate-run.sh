#!/bin/bash
# Spec 1543 T024: apply each mutation, run the tests that guard it, record
# the failing tests, restore every touched file with cp, and check the tree
# is clean against the commit. Usage: mutate-run.sh <scratch-dir>
set -u
REPO=/Users/mlieberman/Projects/mikebom
M=$REPO/specs/1543-therock-last-orphans/measurements/mutate.py
D=$1
mkdir -p "$D"
cd "$REPO"
: > "$D/results.tsv"
for m in $(python3 -I -B "$M" "$REPO" --ids); do
  paths=$(python3 -I -B "$M" "$REPO" --paths "$m")
  for p in $paths; do mkdir -p "$D/backup/$m/$(dirname "$p")"; cp "$REPO/$p" "$D/backup/$m/$p"; done
  nfail=0; compile=0; failed=""
  if ! python3 -I -B "$M" "$REPO" "$m" > /dev/null; then
    echo -e "$m\tANCHOR-MISS" >> "$D/results.tsv"
  else
    {
      cargo test -p waybill --bin waybill --no-fail-fast -- s1543 cmake::links bun_lock
      cargo test -p waybill --test scan_therock_last_orphans --no-fail-fast -- w1 w2 w3 w4 w5 w6 w7 w8 l1 l2 l3 l4 l5 l6 l7 l8 l9 l10 l11 l12
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
