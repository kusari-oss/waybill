#!/usr/bin/env bash
# Milestone 1072 cost measurement (#1144), tasks T034 + T035.
#
# Answers the two numbers SC-008 and contract C-1072-9 refuse to let the
# spec guess:
#
#   T034  the baseline -- total `waybill trace capture` wall-clock for the
#         two_binaries_diverge fixture, plus the distinct-path count and
#         the total hashable bytes the close-time pass faces.
#   T035  the pass itself -- its own wall-clock, as a RATIO of that
#         baseline, over at least three runs.
#
# The ratio is what SC-008 is quoted against. Its 5% ceiling is fixed in
# spec.md ahead of this measurement, deliberately: a ceiling the
# measurement sets cannot be missed.
#
# RUNS INSIDE THE PRIVILEGED CONTAINER. The pass only executes where the
# trace does, which is Linux + `--features ebpf-tracing` + CAP_BPF:
#
#   docker build -f Dockerfile.ebpf-test -t waybill-ebpf-test .
#   docker run --rm --privileged \
#     -v /sys/kernel/debug:/sys/kernel/debug \
#     -v "$PWD/specs/1072-compiler-set-close-time/measurements/cost.sh:/cost.sh" \
#     --entrypoint bash waybill-ebpf-test /cost.sh
#
# The debugfs mount is not optional -- without it every tracepoint attach
# fails with `tracefs not found`. See quickstart.md.
#
# Output is a Markdown table on stdout, to be pasted into cost.md with the
# kernel, arch and fixture recorded alongside, as findings.md does.
set -euo pipefail

RUNS="${RUNS:-3}"
WAYBILL="${WAYBILL:-/waybill/target/release/waybill}"
FIXTURE="${FIXTURE:-/waybill/waybill-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge}"
FIXTURE_TARGET="$FIXTURE/target"
OUT=/tmp/m1072-cost

mkdir -p "$OUT"

echo "## Environment"
echo
echo "- kernel: \`$(uname -sr)\`"
echo "- arch: \`$(uname -m)\`"
echo "- fixture: \`${FIXTURE#/waybill/}\`"
echo "- waybill: \`$("$WAYBILL" --version 2>/dev/null || echo unknown)\`"
echo "- commit: \`$(git -C /waybill rev-parse --short HEAD 2>/dev/null || echo 'not a git checkout')\`"
echo "- runs: $RUNS"
echo
echo "## Runs"
echo
echo "| run | trace wall-clock (s) | pass (ms) | ratio | paths | hashed | bytes hashed |"
echo "|-----|---------------------|-----------|-------|-------|--------|--------------|"

total_ratio_milli=0
measured=0

for i in $(seq 1 "$RUNS"); do
    # A warm target/ makes the build a no-op, and two empty traces agree
    # just as well as two correct ones -- the vacuous-pass trap.
    rm -rf "$FIXTURE_TARGET"

    log="$OUT/run-$i.log"
    att="$OUT/run-$i.json"

    start_ns=$(date +%s%N)
    # DEBUG so the pass's own summary line is emitted; it is debug-level
    # precisely so that measuring it needs no code change.
    RUST_LOG=waybill::trace::close_time=debug,warn \
        "$WAYBILL" trace capture \
        --attestation-format waybill-v1 \
        --output "$att" \
        -- cargo build --release \
            --manifest-path "$FIXTURE/Cargo.toml" \
            --target-dir "$FIXTURE_TARGET" \
        > "$log" 2>&1
    end_ns=$(date +%s%N)

    # Integer nanoseconds throughout: `bc` is not installed in the image,
    # and shell arithmetic cannot do floats.
    wall_ms=$(( (end_ns - start_ns) / 1000000 ))
    wall_s="$(( wall_ms / 1000 )).$(printf '%03d' $(( wall_ms % 1000 )))"

    # The summary line is `close-time pass complete` with structured
    # fields. Missing line => the pass did not run => the measurement is
    # meaningless, so say so rather than reporting a zero.
    # Strip ANSI first. `tracing_subscriber::fmt` colourises even when
    # stdout is a file, so the raw line reads
    # `ESC[3mpaths_total ESC[0m ESC[2m= ESC[0m 102` -- a grep for
    # `paths_total=` finds nothing and the measurement silently reports
    # ABSENT. Cost one failed run to discover.
    summary=$(sed 's/\x1b\[[0-9;]*m//g' "$log" \
        | grep -o 'paths_total=[0-9]* paths_hashed=[0-9]* paths_absent=[0-9]* paths_skipped=[0-9]* bytes_hashed=[0-9]* elapsed_ms=[0-9]*' \
        | tail -1 || true)
    if [[ -z "$summary" ]]; then
        echo "| $i | $wall_s | **ABSENT** | — | — | — | — |"
        continue
    fi

    get() { echo "$summary" | tr ' ' '\n' | grep "^$1=" | cut -d= -f2; }
    paths=$(get paths_total)
    hashed=$(get paths_hashed)
    bytes=$(get bytes_hashed)
    pass_ms=$(get elapsed_ms)

    # Ratio in per-mille, integer-only.
    if [[ "$wall_ms" -gt 0 ]]; then
        ratio_milli=$(( pass_ms * 1000 / wall_ms ))
        total_ratio_milli=$(( total_ratio_milli + ratio_milli ))
        measured=$(( measured + 1 ))
        ratio_pct="$(( ratio_milli / 10 )).$(( ratio_milli % 10 ))"
    else
        ratio_pct="n/a"
    fi

    printf '| %d | %s | %s | %s%% | %s | %s | %s |\n' \
        "$i" "$wall_s" "$pass_ms" "$ratio_pct" "$paths" "$hashed" "$bytes"
done

echo
if [[ "$measured" -lt 1 ]]; then
    echo "**No run produced a pass summary.** The measurement failed; do not"
    echo "quote a number from it."
    exit 1
fi

mean_milli=$(( total_ratio_milli / measured ))
mean_pct="$(( mean_milli / 10 )).$(( mean_milli % 10 ))"
echo "**Mean ratio over $measured runs: ${mean_pct}% of total trace wall-clock.**"
echo
if [[ "$mean_milli" -gt 50 ]]; then
    echo "This EXCEEDS SC-008's 5% ceiling. Per spec.md, record the overrun in"
    echo "findings.md and re-decide the ceiling deliberately -- do not"
    echo "re-baseline it silently."
else
    echo "Within SC-008's 5% ceiling."
fi
