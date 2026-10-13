#!/bin/bash
# m210 eBPF integration test harness — runs inside a --privileged
# container built by Dockerfile.ebpf-test. Asserts that the emitted
# attestation carries the compiler-pipeline metadata section.
#
# Exits 0 on success, non-zero on any assertion failure. Prints
# structured diagnostics on failure so `docker run` output tells you
# what broke.
set -euo pipefail

MIKEBOM=/waybill/target/release/waybill
FIXTURE=/waybill/waybill-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge
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
# --output-format=waybill-v1 gets us the native BuildTracePredicate
# shape with the `compiler_pipeline` field — witness-v0.1 wraps that
# in the attestation-collection envelope which makes jq inspection
# more indirect.
#
# Stay at the waybill workspace root so the loader's CWD-relative eBPF
# object-path resolution (`waybill-ebpf/target/bpfel-unknown-none/release/
# waybill-ebpf`) matches xtask's build output. Point cargo at the
# fixture's manifest instead of cd'ing into it, and give it a scratch
# target dir so we don't clobber waybill's own compile cache.
cd /waybill

# Milestone 1070 (#614): build inside the workspace. A target dir under
# /tmp is dropped whole by the Ephemeral noise category, which made every
# write invisible; the in-tree layout is also what real builds use.
FIXTURE_TARGET="$FIXTURE/target"
rm -rf "$FIXTURE_TARGET"

set +e
# `trace capture` (not `trace run`) emits ONLY the attestation. `trace
# run` also does SBOM generation which fails on hermetic cargo builds
# ("resolution produced zero components from attestation") because the
# generate step needs signed subjects or network downloads to enumerate
# components — neither of which a vendored offline `cargo build` provides.
# Using `capture` sidesteps the m211 US2 combined-workflow gap without
# affecting m212's ring_buffer_overflows verification.
"$MIKEBOM" trace capture \
    --attestation-format waybill-v1 \
    --output "$OUTPUT" \
    -- cargo build --release \
        --manifest-path "$FIXTURE/Cargo.toml" \
        --target-dir "$FIXTURE_TARGET"
TRACE_STATUS=$?
set -e

if [[ $TRACE_STATUS -ne 0 ]]; then
    echo "FAIL: waybill trace run exited $TRACE_STATUS"
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
# Milestone 213 SC-002 target: ≤ 10000. Container-level noise floor:
# the trace captures ALL host opens (no per-process pid filter) so
# docker log churn, systemd, and containerd generate ~5000-8000
# unavoidable events per trace. The m213 kernel-side filter targets
# cargo/rust-toolchain noise (System/UserCache/Ephemeral/CargoFingerprint
# categories) which the empirical Colima aarch64 6.8 run reduces from
# ~14000 (pre-filter baseline per #614 investigation) to ~8800-9500
# (a ~30-40% reduction). Absolute-zero is unreachable without process-
# scoped tracing — deferred to a follow-up feature. This threshold
# validates the filter is measurably active without demanding perfection
# against host-noise the filter architecturally can't reach.
# Milestone 1070 (SC-003): zero. The m213 threshold of 10000 predates #991,
# which made the drain keep up: the fixture measured 0 overflows at 34
# events and at 1,801 (specs/1070-trace-relative-source-reads/measurements).
if [[ "$OVERFLOWS" -ne 0 ]]; then
    echo "FAIL: ring_buffer_overflows=$OVERFLOWS on the SC-001 fixture (m1070 SC-003: 0)"
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
# before the ring buffer, freeing capacity for the actual compiler
# signal. Assert:
#   (a) compiler_pipeline captured ≥1 rustc invocation — m210's
#       separate COMPILER_EXEC_EVENTS ring buffer is unaffected by
#       FILE_EVENTS pressure, so this is the reliable rustc signal
#   (b) NO file-access events reference /fingerprint/, /deps/, or
#       /incremental/ paths (proves the filter fired for those categories)
#
# Note on rustc file_access.operations: rustc's individual file opens
# can still get displaced from FILE_EVENTS when cargo's auto-discovery
# scan of tests/benches/examples/src/bin dirs (unavoidable per
# spec.md — matching those component names risks false-positive drops
# of legitimate source files) generates dominant kprobe load. The
# reliable rustc *execution* signal lives in compiler_pipeline; rustc
# *inputs* capture is a follow-up requiring process-scoped tracing.
if jq -e '.predicate.file_access.operations' "$OUTPUT" > /dev/null 2>&1; then
    # NOTE: comm lives at `.process.comm`, not `.comm` — attestation shape
    # puts process metadata under a nested `process` object.
    RUSTC_FILE_COUNT=$(jq '[.predicate.file_access.operations[] | select(.process.comm == "rustc")] | length' "$OUTPUT")
    LINKER_FILE_COUNT=$(jq '[.predicate.file_access.operations[] | select(.process.comm == "ld" or .process.comm == "ld.lld" or .process.comm == "mold")] | length' "$OUTPUT")
    # Milestone 1070: `deps/` holds build outputs and inputs, not bookkeeping,
    # so it is no longer filtered. The old `/fingerprint/` never matched
    # cargo's `.fingerprint/` directory; the check now uses the real name.
    FP_LEAK_COUNT=$(jq '[.predicate.file_access.operations[] | select(.path | contains("/.fingerprint/") or contains("/incremental/"))] | length' "$OUTPUT")
    echo "    m213 signal-recovery: rustc-files=$RUSTC_FILE_COUNT linker-files=$LINKER_FILE_COUNT fingerprint-leaks=$FP_LEAK_COUNT"
    if [[ "$FP_LEAK_COUNT" -gt 0 ]]; then
        echo "FAIL: $FP_LEAK_COUNT file-access events reference cargo fingerprint paths — m213 filter leak"
        jq '[.predicate.file_access.operations[] | select(.path | contains("/.fingerprint/") or contains("/incremental/")) | .path] | .[:5]' "$OUTPUT"
        exit 1
    fi
else
    echo "WARN: no file_access.operations in attestation — cannot assert m213 SC-001 fingerprint-leak"
fi
# SC-001 primary rustc signal: compiler_pipeline invocation count
# (via COMPILER_EXEC_EVENTS ring buffer — unaffected by FILE_EVENTS
# pressure). This is the reliable rustc-ran signal.
RUSTC_INVOCATION_COUNT=$(jq '[.predicate.compiler_pipeline.invocations[]? | select(.compiler == "rustc")] | length' "$OUTPUT")
echo "    m213 SC-001: rustc invocations captured via compiler_pipeline = $RUSTC_INVOCATION_COUNT"
if [[ "$RUSTC_INVOCATION_COUNT" -lt 1 ]]; then
    echo "FAIL: 0 rustc compiler_pipeline invocations — m213 SC-001 signal-recovery regression"
    exit 1
fi

# Milestone 1070 (#614) — the read and write sets themselves, not only the
# fact that compilers ran (FR-007). Before m1070 every set was empty: the
# kernel dropped relative paths (cargo compiles workspace members relative
# to the workspace root), every open was classified as a read, and `deps/`
# was filtered as fingerprint noise.
echo
echo "==> m1070: compiler read and write sets"
reads_of() { # $1 = jq filter selecting invocations
    jq -r "[.predicate.compiler_pipeline.invocations[] | $1 | .read_set[].path] | .[]" "$OUTPUT"
}
all_writes() {
    jq -r '[.predicate.compiler_pipeline.invocations[].write_set[].path] | .[]' "$OUTPUT"
}
# US1 / SC-001: each crate root, as an absolute path, in a rustc read set.
for root in libsafe/src/lib.rs libvuln/src/lib.rs binaries/safe-only/src/main.rs binaries/vuln-included/src/main.rs; do
    if ! reads_of 'select(.compiler == "rustc")' | grep -qx "$FIXTURE/$root"; then
        echo "FAIL: $FIXTURE/$root is in no rustc read set (m1070 SC-001)"
        exit 1
    fi
    echo "    read set has $root"
done
# US2 / SC-002: the linked binaries and the libraries, under final names.
for pat in 'deps/safe_only-[0-9a-f]+$' 'deps/vuln_included-[0-9a-f]+$' \
           'deps/liblibsafe-[0-9a-f]+\.rlib$' 'deps/liblibsafe-[0-9a-f]+\.rmeta$' \
           'deps/liblibvuln-[0-9a-f]+\.rlib$' 'deps/liblibvuln-[0-9a-f]+\.rmeta$'; do
    if ! all_writes | grep -Eq "$pat"; then
        echo "FAIL: no write set holds an output matching $pat (m1070 SC-002)"
        all_writes | sed 's/^/      /' | head -20
        exit 1
    fi
    echo "    write set has $pat"
done
# FR-010: a library compile writes its metadata to rmeta*/full.rmeta and
# renames it to lib<crate>-<hash>.rmeta, so its write set must hold the
# final name and not the temporary one. (A binary compile also writes
# rmeta*/full.rmeta but deletes it rather than renaming it, so that entry is
# a real, short-lived write and stays.)
if jq -e '[.predicate.compiler_pipeline.invocations[]
          | select(any(.write_set[].path; test("deps/lib[^/]*-[0-9a-f]+\\.rmeta$")))
          | .write_set[].path | select(test("/rmeta[^/]*/full\\.rmeta$"))] | length > 0' "$OUTPUT" > /dev/null; then
    echo "FAIL: a library compile still holds its temporary rmeta name — rename not applied (m1070 FR-010)"
    exit 1
fi
echo "    library compiles hold final rmeta names only (FR-010)"
# US2 scenario 4: each binary compile read the libraries it depends on.
check_lib_read() { # $1 = binary crate root, $2 = library crate
    if ! reads_of "select(.read_set | map(.path) | index(\"$FIXTURE/$1\") != null)" \
            | grep -Eq "deps/lib$2-[0-9a-f]+\.(rlib|rmeta)$"; then
        echo "FAIL: the compile of $1 read no $2 library (m1070 US2 scenario 4)"
        exit 1
    fi
    echo "    $1 compile read lib$2"
}
check_lib_read binaries/safe-only/src/main.rs libsafe
check_lib_read binaries/vuln-included/src/main.rs libsafe
check_lib_read binaries/vuln-included/src/main.rs libvuln
# US3 / SC-004: the unresolved count is the number of flagged operations.
UNRESOLVED_COUNT=$(jq '.predicate.trace_integrity.unresolved_relative_opens // 0' "$OUTPUT")
FLAGGED_COUNT=$(jq '[.predicate.file_access.operations[] | select(.unresolved_relative == true)] | length' "$OUTPUT")
echo "    unresolved_relative_opens=$UNRESOLVED_COUNT flagged=$FLAGGED_COUNT"
if [[ "$UNRESOLVED_COUNT" -ne "$FLAGGED_COUNT" ]]; then
    echo "FAIL: unresolved_relative_opens ($UNRESOLVED_COUNT) != flagged operations ($FLAGGED_COUNT) (m1070 SC-004)"
    exit 1
fi
if jq -e '[.predicate.compiler_pipeline.invocations[] | (.read_set + .write_set)[].path | select(startswith("/") | not)] | length > 0' "$OUTPUT" > /dev/null; then
    echo "FAIL: a relative path reached a read or write set (m1070 FR-003)"
    exit 1
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

# Milestone 213 (issue #616) — SC-006 widen-flag assertion. Run a
# second `waybill trace capture` with --include-system-reads and assert
# that the emitted attestation:
# (a) does NOT contain "System" in filter_categories_applied
# (b) shows /etc/ paths in file_access.operations (if the traced
#     process actually reads any — cat /etc/hostname does)
# The widen flag ONLY affects the System category per FR-010; UserCache
# / Ephemeral / CargoFingerprint remain filtered in the widened run.
echo
echo "==> m213 SC-006 widen-flag verification"
WIDENED_OUTPUT=/tmp/m213-widened.attestation.json
set +e
"$MIKEBOM" trace capture \
    --attestation-format waybill-v1 \
    --output "$WIDENED_OUTPUT" \
    --include-system-reads \
    -- cat /etc/hostname
WIDENED_STATUS=$?
set -e
if [[ $WIDENED_STATUS -ne 0 ]]; then
    echo "FAIL: widened trace capture failed with exit code $WIDENED_STATUS"
    exit 1
fi
if ! jq -e '.predicate.trace_integrity.filter_categories_applied | type == "array"' "$WIDENED_OUTPUT" > /dev/null 2>&1; then
    echo "FAIL: widened run's filter_categories_applied is not a JSON array"
    exit 1
fi
WIDEN_SYSTEM_PRESENT=$(jq -r '.predicate.trace_integrity.filter_categories_applied | contains(["System"])' "$WIDENED_OUTPUT")
if [[ "$WIDEN_SYSTEM_PRESENT" != "false" ]]; then
    echo "FAIL: widened run's filter_categories_applied still contains 'System' — --include-system-reads did not disable System category (FR-010 violation)"
    jq '.predicate.trace_integrity.filter_categories_applied' "$WIDENED_OUTPUT"
    exit 1
fi
echo "    widened filter_categories_applied: $(jq -r '.predicate.trace_integrity.filter_categories_applied | join(",")' "$WIDENED_OUTPUT")"
ETC_HOSTNAME_COUNT=$(jq '[.predicate.file_access.operations[]? | select(.path | test("/etc/hostname"))] | length' "$WIDENED_OUTPUT")
# (widen assertion doesn't gate on comm — path-based check is correct here)
echo "    /etc/hostname events in widened run: $ETC_HOSTNAME_COUNT"
# Note: assertion is soft — the widened trace SHOULD show /etc/hostname
# events, but if the harness invoked cat before the eBPF probes fully
# armed (rare timing edge), the event might not have been captured.
# The primary SC-006 assertion is the filter_categories_applied check
# above; the /etc/hostname count is diagnostic.
if [[ "$ETC_HOSTNAME_COUNT" -lt 1 ]]; then
    echo "WARN: widened run's file_access.operations has 0 /etc/hostname events — expected ≥1 (may indicate trace-attach timing race, not a widen-flag bug)"
fi

# ---------------------------------------------------------------------
# Milestone 1072 (#1144) — close-time facts for compiler read/write sets.
#
# These assertions live here, not in a cfg-gated Rust integration test,
# because Constitution VII requires an eBPF-loading test to be gated
# behind a root/CAP_BPF *runtime* check: ci.yml runs
# `cargo test --features ebpf-tracing` UNPRIVILEGED, so a cfg-only gate
# would compile, run and fail there. This harness is the privileged path.
#
# Asserted over the $OUTPUT capture above — no second trace needed.
echo
echo "==> m1072 US1: close-time facts (SC-001, SC-002, SC-003)"

# SC-001 positive control. At least one write-set entry must report
# existed_at_close: false. A zero here is a FAILURE, not a clean result:
# it is indistinguishable from the pass never having run, which is
# exactly the state #1144 reported. rustc's deps/rmeta* temporaries are
# gone by trace end, so the expected count is >= 1.
ABSENT_WRITES=$(jq '[.predicate.compiler_pipeline.invocations[]?
                     | .write_set[]? | select(.existed_at_close == false)] | length' "$OUTPUT")
if [[ "$ABSENT_WRITES" -lt 1 ]]; then
    echo "FAIL: zero write-set entries report existed_at_close: false (SC-001)"
    echo "      Before m1072 every entry claimed survival. A zero here cannot be"
    echo "      told apart from the close-time pass never running."
    jq '[.predicate.compiler_pipeline.invocations[]? | .write_set[]?
         | {path, existed_at_close}] | .[0:10]' "$OUTPUT"
    exit 1
fi
echo "    write-set entries reporting absent at close: $ABSENT_WRITES"

# SC-001's other half: a true must be true. Every entry claiming the name
# resolved must still resolve now. `-e || -L` mirrors the contract's
# definition -- existed_at_close is a symlink_metadata success, so a
# dangling symlink counts as resolved (C-1072-2).
UNRESOLVED=0
CHECKED=0
while IFS= read -r p; do
    [[ -z "$p" ]] && continue
    CHECKED=$((CHECKED + 1))
    if [[ ! -e "$p" && ! -L "$p" ]]; then
        echo "FAIL: $p reports existed_at_close: true but does not resolve"
        UNRESOLVED=$((UNRESOLVED + 1))
    fi
done < <(jq -r '[.predicate.compiler_pipeline.invocations[]?
                 | (.read_set[]?, .write_set[]?)
                 | select(.existed_at_close == true) | .path] | unique | .[]' "$OUTPUT")
if [[ "$UNRESOLVED" -ne 0 ]]; then
    echo "FAIL: $UNRESOLVED paths claim they resolved but do not (SC-001)"
    exit 1
fi
# A jq typo or a renamed field would empty that loop and the check above
# would "pass" having examined nothing. A real rustc trace reads its own
# source, so the count is never legitimately zero.
if [[ "$CHECKED" -lt 1 ]]; then
    echo "FAIL: zero entries report existed_at_close: true — nothing was checked"
    exit 1
fi
echo "    entries claiming resolution, re-checked on disk: $CHECKED"

# SC-002: no all-zero hash anywhere in the document. The m210 sentinel was
# 64 zeros; FR-006 forbids the value outright, and after the type change
# there is no field left to hold it.
ZERO_HASHES=$(grep -c '"value": *"0\{64\}"' "$OUTPUT" || true)
if [[ "$ZERO_HASHES" -ne 0 ]]; then
    echo "FAIL: $ZERO_HASHES all-zero hashes in the attestation (SC-002)"
    grep -n '"value": *"0\{64\}"' "$OUTPUT" | head -5
    exit 1
fi
echo "    all-zero hashes: 0"

# SC-003: waybill's hashes must agree with an independent implementation.
# Sample up to ten and compare against sha256sum. A sample of zero is a
# failure for the same reason as SC-001's zero: nothing was checked.
SAMPLED=0
MISMATCHED=0
while IFS=' ' read -r claimed path; do
    [[ -z "$claimed" || -z "$path" ]] && continue
    [[ -f "$path" ]] || continue
    actual=$(sha256sum "$path" | cut -d' ' -f1)
    SAMPLED=$((SAMPLED + 1))
    if [[ "$actual" != "$claimed" ]]; then
        echo "FAIL: hash mismatch for $path"
        echo "      waybill:    $claimed"
        echo "      sha256sum:  $actual"
        MISMATCHED=$((MISMATCHED + 1))
    fi
done < <(jq -r '[.predicate.compiler_pipeline.invocations[]?
                 | (.read_set[]?, .write_set[]?)
                 | select(.sha256_at_close != null)
                 | "\(.sha256_at_close.value) \(.path)"] | unique | .[0:10] | .[]' "$OUTPUT")
if [[ "$MISMATCHED" -ne 0 ]]; then
    echo "FAIL: $MISMATCHED of $SAMPLED sampled hashes disagree with sha256sum (SC-003)"
    exit 1
fi
if [[ "$SAMPLED" -lt 1 ]]; then
    echo "FAIL: zero hashes available to sample (SC-003)"
    echo "      Either nothing was hashed or sha256_at_close is never emitted;"
    echo "      a vacuous pass here would hide both."
    jq '.predicate.trace_integrity.compiler_hash_coverage' "$OUTPUT"
    exit 1
fi
echo "    hashes verified against sha256sum: $SAMPLED/$SAMPLED agree"

# --- US2: a read-set entry never claims a hash it does not have --------
echo
echo "==> m1072 US2: read-set honesty (FR-001)"

# The type no longer has the field, so this cannot regress without a
# deliberate change -- which is exactly why it is worth asserting on the
# emitted bytes rather than trusting the type. `"sha256":` with the colon,
# so `sha256_at_close` does not satisfy it.
BARE_SHA256=$(jq '[.predicate.compiler_pipeline.invocations[]?
                   | (.read_set[]?, .write_set[]?)
                   | select(has("sha256"))] | length' "$OUTPUT")
if [[ "$BARE_SHA256" -ne 0 ]]; then
    echo "FAIL: $BARE_SHA256 set entries carry an unqualified sha256 key (FR-001)"
    exit 1
fi
echo "    entries carrying a bare sha256 key: 0"

# Every read-set hash must also agree with sha256sum, not just the write
# sets sampled above -- a read set is where a consumer would most likely
# mistake the value for "what the compiler read".
READ_SAMPLED=0
READ_BAD=0
while IFS=' ' read -r claimed path; do
    [[ -z "$claimed" || -z "$path" ]] && continue
    [[ -f "$path" ]] || continue
    actual=$(sha256sum "$path" | cut -d' ' -f1)
    READ_SAMPLED=$((READ_SAMPLED + 1))
    if [[ "$actual" != "$claimed" ]]; then
        echo "FAIL: read-set hash mismatch for $path ($claimed vs $actual)"
        READ_BAD=$((READ_BAD + 1))
    fi
done < <(jq -r '[.predicate.compiler_pipeline.invocations[]? | .read_set[]?
                 | select(.sha256_at_close != null)
                 | "\(.sha256_at_close.value) \(.path)"] | unique | .[0:10] | .[]' "$OUTPUT")
if [[ "$READ_BAD" -ne 0 || "$READ_SAMPLED" -lt 1 ]]; then
    echo "FAIL: read-set hash check — $READ_BAD mismatched of $READ_SAMPLED sampled"
    exit 1
fi
echo "    read-set hashes verified against sha256sum: $READ_SAMPLED/$READ_SAMPLED agree"

# SC-010: one path, one answer. Two entries naming the same path must agree
# on both close-time fields, because the pass visits a path once and fans
# the result out (FR-011). Disagreement would mean the fan-out missed one.
DISAGREEMENTS=$(jq '[.predicate.compiler_pipeline.invocations[]?
                     | (.read_set[]?, .write_set[]?)
                     | {path, e: .existed_at_close, h: (.sha256_at_close.value // "none")}]
                    | group_by(.path) | map(select((map({e, h}) | unique | length) > 1))
                    | length' "$OUTPUT")
if [[ "$DISAGREEMENTS" -ne 0 ]]; then
    echo "FAIL: $DISAGREEMENTS paths have entries that disagree on close-time facts (SC-010)"
    jq '[.predicate.compiler_pipeline.invocations[]? | (.read_set[]?, .write_set[]?)
         | {path, e: .existed_at_close, h: (.sha256_at_close.value // "none")}]
        | group_by(.path) | map(select((map({e, h}) | unique | length) > 1)) | .[0:3]' "$OUTPUT"
    exit 1
fi

# SC-010's second half: counts are over distinct paths, so on this fixture
# the entry count must EXCEED the distinct-path count --
# deps/liblibsafe-<hash>.rlib is read by three rustc invocations
# (measurements/findings.md Q3). Equality would mean either the fan-out is
# per-entry or the fixture stopped sharing, and both deserve a look.
ENTRY_COUNT=$(jq '[.predicate.compiler_pipeline.invocations[]?
                   | (.read_set[]?, .write_set[]?)] | length' "$OUTPUT")
DISTINCT_COUNT=$(jq '[.predicate.compiler_pipeline.invocations[]?
                      | (.read_set[]?, .write_set[]?) | .path] | unique | length' "$OUTPUT")
if [[ "$ENTRY_COUNT" -le "$DISTINCT_COUNT" ]]; then
    echo "FAIL: $ENTRY_COUNT entries vs $DISTINCT_COUNT distinct paths — expected entries > paths (SC-010)"
    echo "      This fixture shares .rlib reads across three rustc invocations."
    exit 1
fi
echo "    entries: $ENTRY_COUNT over $DISTINCT_COUNT distinct paths, all agreeing"

# --- US3: a renamed artifact is one entry with its history --------------
echo
echo "==> m1072 US3: rename history (SC-004)"

# No entry's path may appear in another entry's renamed_from. If it does,
# the same artifact is recorded twice -- once under its temporary name and
# once under its final one -- which is the duplication FR-009 forbids.
SHADOWED=$(jq '[.predicate.compiler_pipeline.invocations[]?
                | .write_set[]? | .path] as $paths
               | [.predicate.compiler_pipeline.invocations[]? | .write_set[]?
                  | .renamed_from[]? | select(. as $r | $paths | index($r))]
               | unique | length' "$OUTPUT")
if [[ "$SHADOWED" -ne 0 ]]; then
    echo "FAIL: $SHADOWED intermediate names also exist as their own write-set entries (SC-004)"
    exit 1
fi

# rustc renames libraries into place, so this fixture must show history
# somewhere. A zero means either the Rename arm stopped accumulating or
# the rename events stopped arriving -- and an all-zero result here would
# otherwise look identical to a clean pass.
WITH_HISTORY=$(jq '[.predicate.compiler_pipeline.invocations[]?
                    | .write_set[]? | select((.renamed_from // []) | length > 0)] | length' "$OUTPUT")
if [[ "$WITH_HISTORY" -lt 1 ]]; then
    echo "FAIL: no write-set entry carries renamed_from (SC-004)"
    echo "      rustc renames libraries into place; measurements/findings.md Q2."
    jq '[.predicate.compiler_pipeline.invocations[]? | .write_set[]? | .path] | .[0:10]' "$OUTPUT"
    exit 1
fi
echo "    write-set entries carrying rename history: $WITH_HISTORY"
echo "    intermediate names shadowing a real entry: 0"

echo
echo ">>> m210 + m212 + m213 + m1072 integration test PASSED"
echo "    compiler_pipeline: $INVOCATION_COUNT invocations, $RUSTC_COUNT rustc"
echo "    trace_integrity: ring_buffer_overflows=$OVERFLOWS"
echo "    filter_categories_applied (default run): [$APPLIED]"
echo "    filter_categories_applied (widened run): [$(jq -r '.predicate.trace_integrity.filter_categories_applied | join(",")' "$WIDENED_OUTPUT")]"
