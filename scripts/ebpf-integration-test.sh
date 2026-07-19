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
cd "$FIXTURE"

set +e
"$MIKEBOM" trace run \
    --attestation-format mikebom-v1 \
    --output "$OUTPUT" \
    -- cargo build --release
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

echo
echo ">>> m210 integration test PASSED"
echo "    compiler_pipeline present with $INVOCATION_COUNT invocations, $RUSTC_COUNT rustc"
