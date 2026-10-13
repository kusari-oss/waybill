#!/usr/bin/env bash
# Milestone 1072 (#1144): is SC-008's 5% ratio actually held on a real
# workspace build, and does the m213 filter's reach change it?
#
# WHY THIS EXISTS SEPARATELY FROM budget-probe.sh. Two single runs of that
# probe disagreed by 5.7x on wall-clock for identical work -- 392 crates,
# 1,311 invocations, the same 1,520 MiB of artifacts, 685.0s vs 118.9s --
# while hashing throughput moved 119 -> 317 MB/s. SC-008's ratio is the
# quotient of exactly those two moving numbers, and one of the runs put it
# at 5.2% against a 5% ceiling. A ratio computed from two unpaired runs on
# a host whose speed moved by 5.7x is not evidence either way.
#
# So: alternate both configurations inside ONE container, fresh target dir
# each time, in the order F R R F so each configuration gets one early and
# one late slot. Page cache and host contention then bias both arms
# similarly instead of landing entirely on one.
#
#   F = filtered   CARGO_HOME/RUSTUP_HOME under /usr/local, as this image
#                  ships them. m213's System filter drops the `/usr/`
#                  prefix, so dependency-source and sysroot reads never
#                  reach the pass.
#   R = relocated  Both under /root/, as ~/.cargo and ~/.rustup are on an
#                  ordinary host. Those paths match no filter pattern, so
#                  the reads do reach the pass.
#
#   docker run --rm --privileged \
#     -v /sys/kernel/debug:/sys/kernel/debug \
#     -v "$PWD/specs/1072-compiler-set-close-time/measurements/budget-probe-paired.sh:/paired.sh" \
#     --entrypoint bash waybill-ebpf-test /paired.sh
set -euo pipefail

WAYBILL="${WAYBILL:-/waybill/target/release/waybill}"
PROBE_BUDGET="${PROBE_BUDGET:-68719476736}"
BUILD_ARGS="${BUILD_ARGS:---release --workspace --all-targets}"
SEQUENCE="${SEQUENCE:-F R R F}"
OUT=/tmp/m1072-paired
mkdir -p "$OUT"

echo "relocating a copy of the toolchain out of /usr/local..." >&2
cp -a /usr/local/cargo /root/.cargo
cp -a /usr/local/rustup /root/.rustup
PATH="/root/.cargo/bin:$PATH" CARGO_HOME=/root/.cargo RUSTUP_HOME=/root/.rustup \
    cargo --version >&2

echo "## Environment"
echo
echo "- kernel: \`$(uname -sr)\`"
echo "- arch: \`$(uname -m)\`"
echo "- waybill: \`$("$WAYBILL" --version 2>/dev/null || echo unknown)\`"
echo "- traced command: \`cargo build $BUILD_ARGS\` (fresh target dir every run)"
echo "- probe budget: $PROBE_BUDGET bytes (set high so nothing truncates)"
echo "- sequence: $SEQUENCE  (F = toolchain under /usr/, filtered; R = under /root/, visible)"
echo
echo "## Runs"
echo
# events_dropped / ring_buffer_overflows are in the table because the
# first paired run left the central question unanswerable: path capture
# varied 15029..23200 between runs of one build, and whether that is
# DISCLOSED loss or SILENT non-determinism decides whether the system is
# behaving correctly. The attestations went with the container, so the
# counters have to be in the table, not in a file.
echo "| # | config | wall (s) | pass (ms) | ratio | paths | hashed | bytes hashed | read entries | dropped | overflows |"
echo "|---|--------|----------|-----------|-------|-------|--------|--------------|--------------|---------|-----------|"

i=0
for cfg in $SEQUENCE; do
    i=$(( i + 1 ))
    case "$cfg" in
        F) ch=/usr/local/cargo; rh=/usr/local/rustup ;;
        R) ch=/root/.cargo;     rh=/root/.rustup ;;
        *) echo "unknown config $cfg" >&2; exit 2 ;;
    esac

    td="/waybill/target-paired-$i"
    att="$OUT/run-$i.json"
    log="$OUT/run-$i.log"
    rm -rf "$td"

    start_ns=$(date +%s%N)
    env CARGO_HOME="$ch" RUSTUP_HOME="$rh" PATH="$ch/bin:$PATH" \
        RUST_LOG=waybill::trace::close_time=debug,warn \
        "$WAYBILL" trace capture \
        --attestation-format waybill-v1 \
        --compiler-hash-budget-bytes "$PROBE_BUDGET" \
        --output "$att" \
        -- cargo build $BUILD_ARGS \
            --manifest-path /waybill/Cargo.toml \
            --target-dir "$td" \
        > "$log" 2>&1 || echo "NOTE: run $i exited non-zero" >&2
    end_ns=$(date +%s%N)
    wall_ms=$(( (end_ns - start_ns) / 1000000 ))

    summary=$(sed 's/\x1b\[[0-9;]*m//g' "$log" \
        | grep -o 'paths_total=[0-9]* paths_hashed=[0-9]* paths_absent=[0-9]* paths_skipped=[0-9]* bytes_hashed=[0-9]* elapsed_ms=[0-9]*' \
        | tail -1 || true)
    if [[ -z "$summary" ]]; then
        echo "| $i | $cfg | $(( wall_ms / 1000 )) | **ABSENT** | — | — | — | — | — |"
        rm -rf "$td"
        continue
    fi
    get() { echo "$summary" | tr ' ' '\n' | grep "^$1=" | cut -d= -f2; }
    reads=$(jq '[.predicate.compiler_pipeline.invocations[].read_set | length] | add' "$att" 2>/dev/null || echo '?')
    dropped=$(jq -r '.predicate.trace_integrity.events_dropped' "$att" 2>/dev/null || echo '?')
    overflows=$(jq -r '.predicate.trace_integrity.ring_buffer_overflows' "$att" 2>/dev/null || echo '?')
    invs=$(jq '.predicate.compiler_pipeline.invocations | length' "$att" 2>/dev/null || echo '?')
    compiling=$(grep -c '^ *Compiling ' "$log" || true)
    ratio_milli=$(( $(get elapsed_ms) * 1000 / wall_ms ))

    printf '| %d | %s | %d.%03d | %s | %d.%d%% | %s | %s | %s | %s | %s | %s |\n' \
        "$i" "$cfg" "$(( wall_ms / 1000 ))" "$(( wall_ms % 1000 ))" \
        "$(get elapsed_ms)" "$(( ratio_milli / 10 ))" "$(( ratio_milli % 10 ))" \
        "$(get paths_total)" "$(get paths_hashed)" "$(get bytes_hashed)" "$reads" \
        "$dropped" "$overflows"
    echo "<!-- run $i: invocations=$invs compiling=$compiling -->"

    # Reclaim the ~1.5 GB target dir before the next run; the image's
    # overlay has ~37 GB and four of these would be tight alongside the
    # relocated toolchain copies.
    rm -rf "$td"
done

echo
echo "Per-run attestations and logs under $OUT."
echo "SC-008's ceiling is 5%. Read the two F rows against the two R rows:"
echo "a ratio difference between configurations is a real effect of what"
echo "the filter hides; a difference between slots is host state."
