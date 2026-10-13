#!/usr/bin/env bash
# Milestone 1072 (#1144) follow-up probe: what does a REAL workspace
# build demand of the close-time hashing budget?
#
# `measurements/cost.md` sized DEFAULT_COMPILER_HASH_BUDGET_BYTES (2 GiB)
# against the `two_binaries_diverge` fixture -- four crates, 955,494
# bytes -- and quoted the headroom as ~2,200x. A four-crate fixture is
# the wrong denominator for a global default. This probe traces a clean
# release build of waybill's own binary (a mid-sized Rust workspace,
# several hundred crates in the dependency closure) and reports the
# pass's actual byte demand.
#
# The budget flag is set deliberately HIGH so the pass hashes everything
# and reports true demand rather than clamping at the default. Whether
# the default would truncate then follows from the measured total:
# the pass charges cumulatively, so demand > budget truncates and
# demand < budget does not.
#
# CROSS-CHECKS, not just the number (v2). The first run of this probe
# reported 496 paths and 9.97 MB for a 156-second build, far below the
# artifact mass a build that size leaves on disk. A number that small
# may mean the budget is ample OR that the trace captured a fraction of
# the invocations -- and those two readings call for opposite decisions.
# So the probe now records, in the same run:
#
#   - crates cargo says it compiled (`Compiling` lines) vs compiler
#     invocations the trace captured;
#   - artifact bytes left in the fresh target dir vs bytes the pass
#     hashed;
#   - the attestation itself, kept for inspection.
#
# A pass that hashed 1% of the build's artifacts is not evidence about
# the budget. It is evidence about capture.
#
#   docker run --rm --privileged \
#     -v /sys/kernel/debug:/sys/kernel/debug \
#     -v "$PWD/specs/1072-compiler-set-close-time/measurements/budget-probe.sh:/budget-probe.sh" \
#     -v "$PWD/specs/1072-compiler-set-close-time/measurements/.probe-out:/probe-out" \
#     --entrypoint bash waybill-ebpf-test /budget-probe.sh
set -euo pipefail

WAYBILL="${WAYBILL:-/waybill/target/release/waybill}"
# 64 GiB: far above anything this build can demand, so the pass never
# truncates and `bytes_hashed` is the full demand.
PROBE_BUDGET="${PROBE_BUDGET:-68719476736}"
# NOT under /tmp. The m213 kernel noise filter drops the `/tmp/` prefix
# as category `Ephemeral` (waybill-common/src/filter.rs:139), so a target
# dir there has every artifact write discarded before userspace sees it.
# v2 of this probe did exactly that and measured ZERO write-set entries
# across a 358-crate build -- an experiment artifact that reads exactly
# like a capture defect. The guard below refuses to repeat it.
TARGET_DIR="${TARGET_DIR:-/waybill/target-m1072-probe}"
# What to build. The default is the shipped binary. Override to measure a
# wider build -- `--workspace --all-targets` is what CI compiles, and it
# is the case the default budget has to survive.
BUILD_ARGS="${BUILD_ARGS:---release --bin waybill}"
# RELOCATE_TOOLCHAIN=1 moves CARGO_HOME and RUSTUP_HOME out from under
# /usr/local, which the m213 System filter drops wholesale via its `/usr/`
# prefix. In this image both live there, so a build's reads of dependency
# sources (299 MB) and of the Rust sysroot (1.1 GB) never reach a read set
# and never reach the budget. On an ordinary host they are at ~/.cargo and
# ~/.rustup, which match NO filter pattern, so they do. Without this the
# measured demand is a lower bound shaped by the harness, not by the build.
RELOCATE_TOOLCHAIN="${RELOCATE_TOOLCHAIN:-0}"
OUT="${OUT:-/probe-out}"
ATT="$OUT/attestation.json"
LOG="$OUT/trace.log"

case "$TARGET_DIR" in
    /tmp/*|/var/tmp/*)
        echo "REFUSING: TARGET_DIR=$TARGET_DIR is under an Ephemeral-filtered" >&2
        echo "prefix (m213). Every artifact write would be dropped in-kernel" >&2
        echo "and the probe would report a demand of zero bytes." >&2
        exit 2 ;;
esac

if [[ "$RELOCATE_TOOLCHAIN" == "1" ]]; then
    echo "relocating toolchain out of /usr/local (see RELOCATE_TOOLCHAIN)..." >&2
    cp -a "${CARGO_HOME:-/usr/local/cargo}" /root/.cargo
    cp -a "${RUSTUP_HOME:-/usr/local/rustup}" /root/.rustup
    export CARGO_HOME=/root/.cargo
    export RUSTUP_HOME=/root/.rustup
    export PATH="/root/.cargo/bin:$PATH"
    command -v cargo >/dev/null && cargo --version >&2 \
        || { echo "relocated cargo is not runnable; aborting" >&2; exit 3; }
fi

mkdir -p "$OUT"
# A warm target/ makes the build a no-op, and an empty trace agrees with
# itself just as well as a correct one -- the vacuous-pass trap.
rm -rf "$TARGET_DIR"

echo "## Environment"
echo
echo "- kernel: \`$(uname -sr)\`"
echo "- arch: \`$(uname -m)\`"
echo "- waybill: \`$("$WAYBILL" --version 2>/dev/null || echo unknown)\`"
echo "- traced command: \`cargo build $BUILD_ARGS\` (fresh target dir)"
echo "- probe budget: $PROBE_BUDGET bytes ($(( PROBE_BUDGET / 1024 / 1024 / 1024 )) GiB)"
echo "- default under test: 17179869184 bytes (16 GiB)"
echo "- CARGO_HOME: \`${CARGO_HOME:-unset}\`, RUSTUP_HOME: \`${RUSTUP_HOME:-unset}\`"
echo "  (both under \`/usr/\` means the m213 System filter hides every"
echo "  dependency-source and sysroot read from the pass)"
echo

start_ns=$(date +%s%N)
RUST_LOG=waybill::trace::close_time=debug,warn \
    "$WAYBILL" trace capture \
    --attestation-format waybill-v1 \
    --compiler-hash-budget-bytes "$PROBE_BUDGET" \
    --output "$ATT" \
    -- cargo build $BUILD_ARGS \
        --manifest-path /waybill/Cargo.toml \
        --target-dir "$TARGET_DIR" \
    > "$LOG" 2>&1 || echo "NOTE: traced command exited non-zero; see trace.log"
end_ns=$(date +%s%N)

wall_ms=$(( (end_ns - start_ns) / 1000000 ))
echo "## Measured"
echo
echo "- trace wall-clock: $(( wall_ms / 1000 )).$(printf '%03d' $(( wall_ms % 1000 )))s"

# Strip ANSI first: tracing_subscriber::fmt colourises even to a file,
# so a grep for `paths_total=` finds nothing against raw bytes.
summary=$(sed 's/\x1b\[[0-9;]*m//g' "$LOG" \
    | grep -o 'paths_total=[0-9]* paths_hashed=[0-9]* paths_absent=[0-9]* paths_skipped=[0-9]* bytes_hashed=[0-9]* elapsed_ms=[0-9]*' \
    | tail -1 || true)

if [[ -z "$summary" ]]; then
    echo
    echo "**No close-time pass summary in the log.** The pass did not run;"
    echo "do not quote a number from this probe."
    tail -40 "$LOG"
    exit 1
fi

get() { echo "$summary" | tr ' ' '\n' | grep "^$1=" | cut -d= -f2; }
paths=$(get paths_total)
hashed=$(get paths_hashed)
absent=$(get paths_absent)
skipped=$(get paths_skipped)
bytes=$(get bytes_hashed)
pass_ms=$(get elapsed_ms)

echo "- pass elapsed: ${pass_ms}ms"
if [[ "$wall_ms" -gt 0 ]]; then
    ratio_milli=$(( pass_ms * 1000 / wall_ms ))
    echo "- ratio of trace wall-clock: $(( ratio_milli / 10 )).$(( ratio_milli % 10 ))% (SC-008 ceiling: 5%)"
fi
echo "- paths_total: $paths (hashed=$hashed absent=$absent skipped=$skipped)"
echo "- bytes_hashed: $bytes ($(( bytes / 1048576 )) MiB)"
echo

echo "## Cross-check 1: invocations captured vs crates compiled"
echo
compiling=$(grep -c '^ *Compiling ' "$LOG" || true)
invocations=$(jq '.predicate.compiler_pipeline.invocations | length' "$ATT" 2>/dev/null || echo '?')
rustc=$(jq '[.predicate.compiler_pipeline.invocations[] | select(.tool == "rustc")] | length' "$ATT" 2>/dev/null || echo '?')
completeness=$(jq -r '.predicate.compiler_pipeline.completeness' "$ATT" 2>/dev/null || echo '?')
echo "- cargo \`Compiling\` lines (lower bound on rustc processes): $compiling"
echo "- compiler invocations captured: $invocations (rustc: $rustc)"
echo "- compiler_pipeline.completeness: $completeness"
if [[ "$invocations" != "?" && "$compiling" -gt 0 ]]; then
    echo "- captured/compiled: $(( invocations * 100 / compiling ))%"
fi
echo

echo "## Cross-check 2: bytes hashed vs artifact bytes on disk"
echo
echo "The pass hashes paths an eBPF event witnessed. Artifact bytes the"
echo "build left behind bound what a complete capture would have faced."
deps_bytes=$(find "$TARGET_DIR" -type f -print0 2>/dev/null | xargs -0 stat -c '%s' 2>/dev/null | awk '{s+=$1} END {print s+0}')
deps_files=$(find "$TARGET_DIR" -type f 2>/dev/null | wc -l | tr -d ' ')
echo "- files in the fresh target dir: $deps_files"
echo "- bytes in the fresh target dir: $deps_bytes ($(( deps_bytes / 1048576 )) MiB)"
if [[ "$deps_bytes" -gt 0 ]]; then
    echo "- bytes_hashed / target-dir bytes: $(( bytes * 1000 / deps_bytes ))‰"
fi
echo

echo "## Cross-check 3: are there write-set entries at all?"
echo
echo "#1144 exists to resolve write-set entries that used to claim they"
echo "survived the trace window. Zero write entries means the probe"
echo "measured nothing about that, whatever bytes_hashed says."
jq -r '[.predicate.compiler_pipeline.invocations[] | {r: (.read_set|length), w: (.write_set|length)}] as $i
  | "- invocations: \($i|length)",
    "- with a non-empty write set: \($i | map(select(.w>0)) | length)",
    "- total write entries: \($i | map(.w) | add)",
    "- total read entries: \($i | map(.r) | add)",
    "- filters applied: \(input.predicate.trace_integrity.filter_categories_applied | join(","))"' \
  "$ATT" "$ATT" 2>/dev/null || echo '(could not read the attestation)'
echo

echo "## Verdict against the 16 GiB default"
echo
DEFAULT=17179869184
if [[ "$bytes" -gt "$DEFAULT" ]]; then
    echo "**Demand EXCEEDS the default.** $bytes > $DEFAULT. A normal clean"
    echo "build of this workspace WOULD truncate under the shipped default."
    echo "Demand is $(( bytes * 100 / DEFAULT ))% of the default."
else
    headroom_milli=$(( DEFAULT * 1000 / bytes ))
    echo "**Demand fits the default.** $bytes < $DEFAULT, headroom"
    echo "$(( headroom_milli / 1000 )).$(( (headroom_milli % 1000) / 100 ))x --"
    echo "compare the 2 GiB this default replaced."
    echo
    echo "READ THIS WITH CROSS-CHECK 1 AND 2. If capture is a fraction of"
    echo "the build, the headroom is a fact about what was observed, not"
    echo "about what a complete observation would demand."
fi
echo

echo "## Ring-buffer health"
echo
echo "A drop means the trace saw fewer paths than the build touched, which"
echo "understates demand. Stated so the number is not read as a ceiling."
jq -c '{events_dropped: .predicate.trace_integrity.events_dropped,
        ring_buffer_overflows: .predicate.trace_integrity.ring_buffer_overflows,
        partial_captures: .predicate.trace_integrity.partial_captures,
        coverage: .predicate.trace_integrity.compiler_hash_coverage}' "$ATT" 2>/dev/null \
    || echo '(could not read the attestation)'
echo
echo "Attestation and trace log kept under $OUT for inspection."
