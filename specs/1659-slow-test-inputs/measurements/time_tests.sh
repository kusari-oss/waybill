#!/usr/bin/env bash
# Time each named test alone, 3 runs, with nextest's per-test duration. Usage: time_tests.sh PHASE TEST...
set -u
cd "$(git rev-parse --show-toplevel)"
M=specs/1659-slow-test-inputs/measurements
phase=$1; shift
for t in "$@"; do
    bin=${t%%::*}; name=${t#*::}
    for n in 1 2 3; do
        line=$(cargo nextest run -p waybill --color never --test "$bin" -E "test(=$name)" 2>&1 | grep -E '^\s+(PASS|FAIL) \[' | tail -1)
        secs=$(sed -E 's/.*\[ *([0-9.]+)s\].*/\1/' <<<"$line")
        status=$(awk '{print $1}' <<<"$line")
        printf '%s\t%s\t%s\t%s\t%s\n' "$t" "$phase" "$n" "$secs" "$status" >> "$M/runs.tsv"
    done
done
