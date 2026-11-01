#!/usr/bin/env bash
# #1185 — how often does the traced root actually get established?
#
# A measurement, not a gate. Nothing here asserts a threshold, because no
# rate has been measured (CLAUDE.md, "measure external behaviour before
# designing around it"). The output is the result.
#
# Why this exists. Run 12 found the x86_64 handshake is *intermittent*, not
# absent: one establishment in three traces, with traces 1 and 3 being the
# same probe with the same command 16 seconds apart. Run 13 then eliminated
# every encoding hypothesis — both markers absent while the entry program
# demonstrably ran — and did so from two draws, both failures, which is
# exactly the outcome run 12 predicted would be indistinguishable from a
# real negative.
#
# So every further experiment on this handshake is unfalsifiable until the
# rate is known: a change that does nothing passes two runs in three. This
# loop is that number, plus the one other quantity run 12 named as the
# first candidate cause.
#
# Two things measured per iteration:
#
#   1. Did the root get established? `trace capture` fails closed when
#      TRACED_PIDS is empty, so a zero exit IS an establishment and the
#      count needs no log parsing to be trustworthy.
#   2. How long did the attempt take, in milliseconds. The bail is a 2s
#      timeout and the only measured quantity of the same order in run 12
#      was a 2.47s uprobe attach, so establishment-versus-duration is the
#      correlation worth having. It falls out of the same loop for free.
#
# Usage: ebpf-handshake-rate.sh [iterations]   (default 25)
set -uo pipefail

ITERATIONS="${1:-25}"
MIKEBOM=/waybill/target/release/waybill
LOGDIR=/tmp/handshake-rate
mkdir -p "$LOGDIR"
cd /waybill

if [[ ! -x "$MIKEBOM" ]]; then
    echo "FAIL: no binary at $MIKEBOM; the loop would measure nothing"
    exit 1
fi

echo "==> #1185 handshake establishment rate"
echo "    iterations: $ITERATIONS"
echo "    kernel:     $(uname -r) $(uname -m)"
echo "    binary:     $MIKEBOM"
echo

established=0
attempted=0
declare -a ESTABLISHED_MS=()
declare -a FAILED_MS=()

for ((i = 1; i <= ITERATIONS; i++)); do
    out="$LOGDIR/iter-$i.attestation.json"
    log="$LOGDIR/iter-$i.log"
    start=$(date +%s%3N)
    # Same invocation as the `#1185 control` in ebpf-integration-test.sh:
    # --include-system-reads because `trace capture` refuses a capture that
    # aggregated nothing, and the only file `cat /etc/hostname` reads is one
    # the m213 System category drops. It cannot bias this measurement — the
    # noise classifier runs after the entry/exit join, so it changes which
    # records reach the ring buffer, never whether a root was established.
    "$MIKEBOM" trace capture \
        --attestation-format waybill-v1 \
        --include-system-reads \
        --output "$out" \
        -- cat /etc/hostname > "$log" 2>&1
    status=$?
    elapsed=$(( $(date +%s%3N) - start ))
    attempted=$((attempted + 1))

    if [[ $status -eq 0 ]]; then
        established=$((established + 1))
        ESTABLISHED_MS+=("$elapsed")
        printf 'iter %3d  ESTABLISHED  %6dms\n' "$i" "$elapsed"
    else
        FAILED_MS+=("$elapsed")
        # The diagnosis line, not the whole log: one line per failure keeps
        # N=25 readable, and it is the line that says which link broke.
        diag=$(grep -o '#1185 handshake diagnosis -- .*' "$log" | head -1)
        printf 'iter %3d  failed(%d)   %6dms  %s\n' \
            "$i" "$status" "$elapsed" "${diag:-<no diagnosis line emitted>}"
    fi
done

echo
echo "--- distribution of failure diagnoses ---"
# Grouped by the counter shape rather than listed, because 25 near-identical
# lines hide whether they are in fact identical. A second shape appearing
# here is more informative than any single line.
#
# Run 14 is why this takes three precautions rather than one. Normalising
# `nonce_readback` alone produced 13 groups for 15 diagnosed failures and
# grouped nothing:
#
#   - `openat_entered` and `non_tracer_opens` vary per trace by
#     construction -- they count every open on the machine during the
#     window -- so leaving them in the key makes every failure its own
#     group. Their *spread* is a finding; their presence in a grouping key
#     is noise. Normalised, and reported separately below.
#   - each failure emits the line twice, once as the ERROR log and once
#     inside the `Error:` message, so the counts came out at exactly 2x.
#     One line per log file, not one per match.
# Captured, then tested for emptiness, rather than `pipeline || echo`.
# Run 15 printed "(none -- every iteration established)" directly beneath
# a list of seventeen failures: `set -o pipefail` plus the inner
# `grep -o ... | head -1` exiting 1 for the one log with no diagnosis line
# makes the whole pipeline non-zero, so `||` fires on a pipeline that
# produced perfectly good output. A fallback that triggers on exit status
# states something false about the data; one that triggers on empty output
# cannot.
diag_groups=$(
    grep -l . "$LOGDIR"/iter-*.log 2>/dev/null \
        | while read -r f; do grep -o '#1185 handshake diagnosis -- .*' "$f" | head -1; done \
        | sed -e 's/nonce_readback=[0-9]*/nonce_readback=<varies>/' \
              -e 's/openat_entered=[0-9]*/openat_entered=<varies>/' \
              -e 's/non_tracer_opens=[0-9]*/non_tracer_opens=<varies>/' \
        | sort | uniq -c | sort -rn
) || true
if [ -n "$diag_groups" ]; then
    printf '%s\n' "$diag_groups"
else
    echo "    (none — no iteration emitted a diagnosis)"
fi

# The spread that grouping had to normalise away, reported as its own
# measurement. `openat_entered` is incremented ungated as the entry
# program's first statement, for every openat on the machine, so a trace
# that saw single digits saw a machine that cannot have been that quiet.
echo
echo "--- openat_entered across failing traces (machine-wide opens seen) ---"
entered_spread=$(
    grep -l . "$LOGDIR"/iter-*.log 2>/dev/null \
        | while read -r f; do grep -o 'openat_entered=[0-9]*' "$f" | head -1; done \
        | cut -d= -f2 | sort -n | uniq -c \
        | awk '{printf "    seen %6s open(s): %d trace(s)\n", $2, $1}'
) || true
if [ -n "$entered_spread" ]; then
    printf '%s\n' "$entered_spread"
else
    echo "    (no diagnosed failures)"
fi

summary() {
    local -n arr=$1
    if [[ ${#arr[@]} -eq 0 ]]; then echo "n/a"; return; fi
    printf '%s\n' "${arr[@]}" | sort -n | awk '
        {v[NR]=$1}
        END {printf "min=%dms median=%dms max=%dms n=%d", v[1], v[int((NR+1)/2)], v[NR], NR}'
}

echo
echo "--- duration, established versus failed ---"
echo "    established: $(summary ESTABLISHED_MS)"
echo "    failed:      $(summary FAILED_MS)"

echo
# The positive signal. A loop that exited early and one that ran to
# completion with zero establishments both print zeroes otherwise, and a
# check that did not run looks identical to one that passed.
echo "handshake-rate: iterations=$ITERATIONS attempted=$attempted established=$established"
if [[ $attempted -ne $ITERATIONS ]]; then
    echo "FAIL: the loop attempted $attempted of $ITERATIONS iterations; the rate below is measured on the wrong denominator"
    exit 1
fi
awk -v e="$established" -v n="$ITERATIONS" \
    'BEGIN {printf "handshake-rate: %d/%d established = %.1f%%\n", e, n, (100 * e / n)}'

# Exit 0 whatever the rate: this is a measurement. A zero rate is a result,
# not a failure of the probe, and making it an error would mean the only
# runs that report are the ones that already worked.
exit 0
