#!/bin/bash
# m210 eBPF integration test harness — runs inside a --privileged
# container built by Dockerfile.ebpf-test. Asserts that the emitted
# attestation carries the compiler-pipeline metadata section.
#
# Exits 0 on success, non-zero on any assertion failure. Prints
# structured diagnostics on failure so `docker run` output tells you
# what broke.
set -euo pipefail

MIKEBOM=/mikebom/target/release/mikebom
FIXTURE=/mikebom/mikebom-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge
OUTPUT=/tmp/m210-integration.attestation.json

echo "==> m210 integration test: trace the SC-001 fixture build"
echo "    fixture: $FIXTURE"
echo "    output:  $OUTPUT"
echo

# Run the trace against a `cargo build` of the fixture. The `--path`
# below is intentionally the fixture's Cargo workspace root; the
# `trace run -- cargo build --manifest-path ...` shape mirrors how
# operators would invoke this in the wild.
#
# --output-format=mikebom-v1 gets us the native BuildTracePredicate
# shape with the `compiler_pipeline` field — witness-v0.1 wraps that
# in the attestation-collection envelope which makes jq inspection
# more indirect.
#
# Stay at the mikebom workspace root so the loader's CWD-relative eBPF
# object-path resolution (`mikebom-ebpf/target/bpfel-unknown-none/release/
# mikebom-ebpf`) matches xtask's build output. Point cargo at the
# fixture's manifest instead of cd'ing into it, and give it a scratch
# target dir so we don't clobber mikebom's own compile cache.
cd /mikebom

set +e
# `trace capture` (not `trace run`) emits ONLY the attestation. `trace
# run` also does SBOM generation which fails on hermetic cargo builds
# ("resolution produced zero components from attestation") because the
# generate step needs signed subjects or network downloads to enumerate
# components — neither of which a vendored offline `cargo build` provides.
# Using `capture` sidesteps the m211 US2 combined-workflow gap without
# affecting m212's ring_buffer_overflows verification.
"$MIKEBOM" trace capture \
    --attestation-format mikebom-v1 \
    --output "$OUTPUT" \
    -- cargo build --release \
        --manifest-path "$FIXTURE/Cargo.toml" \
        --target-dir /tmp/m210-fixture-target
TRACE_STATUS=$?
set -e

if [[ $TRACE_STATUS -ne 0 ]]; then
    echo "FAIL: mikebom trace run exited $TRACE_STATUS"
    exit 1
fi

echo
echo "==> Attestation emitted; verifying compiler_pipeline field"

if [[ ! -f "$OUTPUT" ]]; then
    echo "FAIL: attestation file not written at $OUTPUT"
    exit 1
fi

# The attestation JSON should have `predicate.compiler_pipeline` when
# the trace captured any compiler invocations. Absent compiler_pipeline
# = the sched_process_exec tracepoint didn't fire (kernel too old, or
# tracepoint attach failed, or fixture's cargo build didn't invoke
# a whitelisted compiler — all real bugs at this integration level).
if ! jq -e '.predicate.compiler_pipeline' "$OUTPUT" > /dev/null 2>&1; then
    echo "FAIL: predicate.compiler_pipeline missing from attestation"
    echo "----- attestation JSON (first 2 KB) -----"
    head -c 2048 "$OUTPUT"
    echo
    exit 1
fi

INVOCATION_COUNT=$(jq '.predicate.compiler_pipeline.invocations | length' "$OUTPUT")
echo "    captured $INVOCATION_COUNT compiler invocations"

if [[ "$INVOCATION_COUNT" -eq 0 ]]; then
    echo "FAIL: zero compiler invocations captured"
    echo "----- compiler_pipeline block -----"
    jq '.predicate.compiler_pipeline' "$OUTPUT"
    exit 1
fi

# Assert at least one invocation was matched to rustc (fixture is a
# Rust workspace; if we captured NO rustc invocations, either the
# whitelist filter is wrong or the tracepoint isn't firing).
RUSTC_COUNT=$(jq '[.predicate.compiler_pipeline.invocations[] | select(.compiler == "rustc")] | length' "$OUTPUT")
echo "    of which rustc: $RUSTC_COUNT"

if [[ "$RUSTC_COUNT" -eq 0 ]]; then
    echo "FAIL: no rustc invocations captured (fixture is a Rust workspace)"
    exit 1
fi

# Assert the completeness signal is present + not degraded.
COMPLETENESS=$(jq -r '.predicate.compiler_pipeline.completeness.state' "$OUTPUT")
echo "    completeness: $COMPLETENESS"

case "$COMPLETENESS" in
    complete)
        # OK
        ;;
    degraded)
        DROPPED=$(jq -r '.predicate.compiler_pipeline.completeness.dropped' "$OUTPUT")
        echo "    (degraded: dropped=$DROPPED — non-fatal but note ring-buffer overflow occurred)"
        ;;
    partial)
        REASON=$(jq -r '.predicate.compiler_pipeline.completeness.reason' "$OUTPUT")
        echo "    (partial: reason=$REASON)"
        ;;
    *)
        echo "FAIL: unexpected completeness state: $COMPLETENESS"
        exit 1
        ;;
esac

# Milestone 212 (issue #615) — assert the real ring_buffer_overflows
# counter is present as a JSON number type. Pre-m212 the field was
# hardcoded to 0 at every emission site; post-m212 it carries real
# per-CPU drop-counter values.
#
# Milestone 213 (issue #616) UPDATE: pre-m213 the SC-001 fixture
# reported >100 drops (cargo fingerprint spam saturating the ring
# buffer). Post-m213 the kernel-side noise filter drops that spam
# BEFORE the ring buffer, so overflows should be ≤ 10 per SC-002.
# The > 100 assertion is removed and replaced with the ≤ 10
# assertion below.
if ! jq -e '.predicate.trace_integrity.ring_buffer_overflows | type == "number"' "$OUTPUT" > /dev/null 2>&1; then
    echo "FAIL: predicate.trace_integrity.ring_buffer_overflows is not a JSON number type"
    exit 1
fi
OVERFLOWS=$(jq '.predicate.trace_integrity.ring_buffer_overflows' "$OUTPUT")
echo "    ring_buffer_overflows: $OVERFLOWS"
if [[ "$OVERFLOWS" -gt 10 ]]; then
    echo "FAIL: ring_buffer_overflows=$OVERFLOWS is >10 on the SC-001 fixture (m213 SC-002 target: ≤10)"
    echo "      Either the m213 kernel-side filter regressed OR the fixture generates NEW drop patterns not covered by any of the 4 filter categories."
    exit 1
fi

# Assert the m212 Q3 disambiguation signal: no counter map should
# appear in kprobe_attach_failures on a supported kernel. If any
# *_drops entry appears, the reported overflow count is a floor, not
# a total — flag as WARN but don't fail (kernel version might legitimately
# not support one of the maps).
COUNTER_FAILURES=$(jq '[.predicate.trace_integrity.kprobe_attach_failures[] | select(endswith("_drops"))] | length' "$OUTPUT")
if [[ "$COUNTER_FAILURES" -gt 0 ]]; then
    echo "WARN: $COUNTER_FAILURES counter map(s) failed to attach — ring_buffer_overflows is a partial sum"
    jq '.predicate.trace_integrity.kprobe_attach_failures[] | select(endswith("_drops"))' "$OUTPUT"
fi

# Milestone 213 (issue #616) — SC-001 signal recovery. With the
# kernel-side noise filter active, cargo's fingerprint spam is dropped
# before the ring buffer, freeing capacity for the actual rustc + linker
# events. Assert:
#   (a) at least 1 rustc file-access event appears (baseline: 0)
#   (b) at least 1 linker file-access event appears — ld / ld.lld / mold
#   (c) NO file-access events reference /fingerprint/, /deps/, or
#       /incremental/ paths (proves the filter fired for those categories)
if jq -e '.predicate.file_access.operations' "$OUTPUT" > /dev/null 2>&1; then
    RUSTC_FILE_COUNT=$(jq '[.predicate.file_access.operations[] | select(.comm == "rustc")] | length' "$OUTPUT")
    LINKER_FILE_COUNT=$(jq '[.predicate.file_access.operations[] | select(.comm == "ld" or .comm == "ld.lld" or .comm == "mold")] | length' "$OUTPUT")
    FP_LEAK_COUNT=$(jq '[.predicate.file_access.operations[] | select(.path | contains("/fingerprint/") or contains("/deps/") or contains("/incremental/"))] | length' "$OUTPUT")
    echo "    m213 signal-recovery: rustc=$RUSTC_FILE_COUNT linker=$LINKER_FILE_COUNT fingerprint-leaks=$FP_LEAK_COUNT"
    if [[ "$RUSTC_FILE_COUNT" -lt 1 ]]; then
        echo "FAIL: 0 rustc file-access events — m213 SC-001 signal-recovery regression (was baseline pre-fix)"
        exit 1
    fi
    if [[ "$LINKER_FILE_COUNT" -lt 1 ]]; then
        echo "FAIL: 0 linker file-access events (ld / ld.lld / mold) — m213 SC-001 signal-recovery regression"
        exit 1
    fi
    if [[ "$FP_LEAK_COUNT" -gt 0 ]]; then
        echo "FAIL: $FP_LEAK_COUNT file-access events reference cargo fingerprint paths — m213 filter leak"
        jq '[.predicate.file_access.operations[] | select(.path | contains("/fingerprint/") or contains("/deps/") or contains("/incremental/")) | .path] | .[:5]' "$OUTPUT"
        exit 1
    fi
else
    echo "WARN: no file_access.operations in attestation — cannot assert m213 SC-001"
fi

# Milestone 213 (issue #616) — SC-003 transparent-aggregate assertion.
# TraceIntegrity.filter_categories_applied[] MUST be present as a JSON
# array (never null, never absent — FR-009) AND MUST contain "CargoFingerprint"
# on any cargo build (the fingerprint spam guaranteed to fire).
if ! jq -e '.predicate.trace_integrity.filter_categories_applied | type == "array"' "$OUTPUT" > /dev/null 2>&1; then
    echo "FAIL: predicate.trace_integrity.filter_categories_applied is not a JSON array (m213 FR-006/FR-009 violation)"
    jq '.predicate.trace_integrity.filter_categories_applied' "$OUTPUT" || true
    exit 1
fi
APPLIED=$(jq -r '.predicate.trace_integrity.filter_categories_applied | join(",")' "$OUTPUT")
echo "    filter_categories_applied: [$APPLIED]"
if ! jq -e '.predicate.trace_integrity.filter_categories_applied | index("CargoFingerprint") != null' "$OUTPUT" > /dev/null 2>&1; then
    echo "FAIL: filter_categories_applied does NOT contain 'CargoFingerprint' on a cargo build (m213 US2 signal missing)"
    exit 1
fi
# FR-008 defensive: every value in the array MUST be a pure alphabetic
# category name — no filesystem paths (which would leak host info).
if ! jq -e '.predicate.trace_integrity.filter_categories_applied | all(. | test("^[A-Za-z]+$"))' "$OUTPUT" > /dev/null 2>&1; then
    echo "FAIL: filter_categories_applied contains non-alphabetic entries — possible path leakage (FR-008)"
    jq '.predicate.trace_integrity.filter_categories_applied' "$OUTPUT"
    exit 1
fi

echo
echo ">>> m210 + m212 + m213 integration test PASSED"
echo "    compiler_pipeline: $INVOCATION_COUNT invocations, $RUSTC_COUNT rustc"
echo "    trace_integrity: ring_buffer_overflows=$OVERFLOWS"
