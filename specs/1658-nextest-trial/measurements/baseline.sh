#!/usr/bin/env bash
# T003, T016a, T004 and the doctest count, run serially on an idle machine.
# Each timed command's wall time is appended to $M/wall.tsv as: label<TAB>run<TAB>seconds<TAB>exit
set -u
cd "$(git rev-parse --show-toplevel)"
M=specs/1658-nextest-trial/measurements
now() { python3 -I -B -c 'import time; print(f"{time.time():.2f}")'; }
timed() { # label run logfile cmd...
    local label=$1 run=$2 log=$3; shift 3
    local t0; t0=$(now)
    "$@" > "$log" 2>&1; local rc=$?
    local t1; t1=$(now)
    printf '%s\t%s\t%.1f\t%s\n' "$label" "$run" "$(python3 -I -B -c "print($t1-$t0)")" "$rc" >> "$M/wall.tsv"
}
printf 'label\trun\twall_s\texit\n' > "$M/wall.tsv"
# Warm build: every test target and the doctests' deps, so no timed run compiles.
timed warm-build 0 "$M/warm-build.log" cargo test --workspace --no-run
timed warm-nextest 0 "$M/warm-nextest.log" cargo nextest run --workspace --no-run --color never
for n in 1 2 3; do
    timed pre-pr-before "$n" "$M/pre-pr-before-$n.log" ./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' "prepr-before-$n" -- bash "$M/pre-pr.before.sh"
done
for n in 1 2 3; do
    timed cargo-test "$n" "$M/cargo-test-$n.log" cargo test --workspace --no-fail-fast
done
for n in 1 2 3; do
    timed nextest "$n" "$M/nextest-$n.log" cargo nextest run --workspace --no-fail-fast --color never
done
timed doctests 1 "$M/doctests-1.log" cargo test --workspace --doc
echo BASELINE-DONE >> "$M/wall.tsv"
