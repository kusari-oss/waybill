#!/usr/bin/env bash
# T016 / T020: the new pre-pr.sh through gate.sh with both C2 expectations, 3 runs.
set -u
cd "$(git rev-parse --show-toplevel)"
M=specs/1658-nextest-trial/measurements
now() { python3 -I -B -c 'import time; print(f"{time.time():.2f}")'; }
for n in 1 2 3; do
    t0=$(now)
    ./scripts/gate.sh \
      --expect '>>> all pre-PR checks passed\.' \
      --expect '^ +Summary \[ *[0-9.]+s\] [1-9][0-9]* tests? run: [1-9][0-9]* passed(, [0-9]+ skipped)?$' \
      "prepr-after-$n" -- ./scripts/pre-pr.sh > "$M/pre-pr-after-$n.log" 2>&1
    rc=$?
    t1=$(now)
    printf 'pre-pr-after\t%s\t%.1f\t%s\n' "$n" "$(python3 -I -B -c "print($t1-$t0)")" "$rc" >> "$M/wall.tsv"
done
echo AFTER-DONE >> "$M/wall.tsv"
