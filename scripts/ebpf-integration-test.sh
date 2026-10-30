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

# #1185 — a control, before the fixture build and on the same runner in the
# same run. `kernel_exit_without_request` counts an open-syscall exit whose
# entry was never stored. The open probes attach exit-before-entry on
# purpose (an entry program live before its exit program would stash
# requests nothing can join, losing successful opens outright), and the
# probes are system-wide, so every open anywhere on the machine during that
# window exits with nothing stored. Userspace baselines the slot, but a
# syscall that *entered* during the gap *exits* after the baseline read, so
# it lands in the delta however the baseline is timed — measured on x86-64,
# baselining removed only ~124 of 1237.
#
# So the number alone cannot say whether the build lost entry events. This
# run gives it the missing comparison: a process that does almost no work.
# What this control reports in that slot is the ambient floor, and only the
# fixture build's excess over it is attributable to the build. The floor is
# measured here and not carried from another host, because it is a property
# of this runner's load during one attach window.
#
# Diagnostic, not asserted: no threshold is claimed here because none has
# been measured yet (CLAUDE.md, "measure external behaviour before designing
# around it"). What *is* asserted is that the control ran — a print from a
# run that failed would be a floor measured on nothing.
#
# `--include-system-reads` is on because `trace capture` refuses a capture
# that aggregated nothing ("no dependency activity observed"), and the only
# file `cat /etc/hostname` reads is one the m213 System category drops. It
# cannot bias the comparison: the noise classifier runs *after* the
# entry/exit join, inside `emit_open` (`open_syscalls.rs`, the
# `classify_and_drop_if_noise` call), so a filtered path has already been
# joined and counted no drop. The flag changes which records reach the ring
# buffer, never whether an exit found its entry.
CONTROL_OUTPUT=/tmp/i1185-control.attestation.json
echo "==> #1185 control: trace a trivial command before the fixture build"
set +e
"$MIKEBOM" trace capture \
    --attestation-format waybill-v1 \
    --include-system-reads \
    --output "$CONTROL_OUTPUT" \
    -- cat /etc/hostname >/tmp/i1185-control.log 2>&1
CONTROL_STATUS=$?
set -e
if [[ $CONTROL_STATUS -ne 0 ]]; then
    echo "FAIL: the #1185 control trace exited $CONTROL_STATUS; its floor would be measured on nothing"
    tail -40 /tmp/i1185-control.log
    exit 1
fi
if [[ ! -f "$CONTROL_OUTPUT" ]]; then
    echo "FAIL: the #1185 control trace wrote no attestation at $CONTROL_OUTPUT"
    exit 1
fi
# Only `event_drop_reasons` is a floor for the fixture build's: the join
# this control baselines happens before the noise classifier, so the widen
# flag cannot move it. The other two figures are filter-dependent and are
# NOT comparable to the fixture build's — the widen flag admits every
# ambient System-category read on the machine, which is most of what they
# count. Labelled rather than dropped, because a bare number next to the
# fixture's invites exactly that comparison. Two consecutive runs of this
# same block, minutes apart on one image, reported 239 operations / 192
# unresolved and then 415 / 258, against a fixture build of 108 / 5 both
# times: these two rows track the container's ambient load, not the traced
# command, so they are a floor for nothing.
CONTROL_DROPS=$(jq -c '.predicate.trace_integrity.event_drop_reasons // {}' "$CONTROL_OUTPUT")
CONTROL_OPS=$(jq '.predicate.file_access.operations | length' "$CONTROL_OUTPUT")
CONTROL_UNRESOLVED=$(jq '.predicate.trace_integrity.unresolved_relative_opens // 0' "$CONTROL_OUTPUT")
echo "    control event_drop_reasons:  $CONTROL_DROPS   <- the floor; comparable"
echo "    control file operations:     $CONTROL_OPS   (widened; NOT comparable)"
echo "    control unresolved_relative: $CONTROL_UNRESOLVED   (widened; NOT comparable)"
echo

# =============================================================================
# The capture's scope. #1185 — the first check of what the probes are
# pointed AT, rather than of what they lost.
# =============================================================================
#
# Every number this feature has ever produced -- 1237, 1113, 918, 415, 239,
# 108, 102, 56, 5 -- was taken with a capture that admits operations from
# every process on the machine. `should_trace()` (waybill-ebpf/src/helpers.rs)
# excludes the tracer's own pid and nothing else. So those figures measure the
# build PLUS the host, with nothing in them to separate the two, which is why
# they do not reconcile across runs or across architectures.
#
# Three scoping mechanisms exist and none is wired to a reader:
#
#   PID_FILTER            written at trace/loader.rs:89     read by nobody
#   TraceConfig.trace_children  written at trace/loader.rs:114  read by nobody
#   COMPILER_INVOCATIONS  written by programs/compiler_exec.rs, with fork
#                         propagation -- read by no file-op program, though
#                         its own doc comment in maps.rs says "Consumed by
#                         every file-op kprobe (see file_ops.rs)".
#
# This block exists so that fact is a number in a log rather than a claim in
# a comment. It does not fix anything.
#
# ---- why `comm` and not `pid` ----
#
# The pids cannot be compared. The tracer runs in the container's pid
# namespace and reports `Child started pid=23`; the kernel reports the same
# cargo as `pid=497887`, because `bpf_get_current_pid_tgid()` returns the
# initial namespace's view. `process.pid` in the attestation is therefore
# host-namespace and the traced child's pid, as the tracer knows it, is not.
# Translating them needs `NSpid` from /proc and belongs in the fix, not in
# the measurement.
#
# So the probe is a copy of `cat` under a name nothing else on the host has,
# and an operation is foreign when its `comm` is not that name. That makes
# the count a LOWER BOUND on foreign capture -- it cannot overstate, because
# every operation it counts demonstrably came from a process that is not the
# traced command.
#
# ---- what the widen flag does and does not change ----
#
# `--include-system-reads` is required: the probe's only read is an /etc path
# that the m213 System category drops, and `trace capture` refuses a capture
# that aggregated nothing. It changes the MAGNITUDE of the foreign count and
# not the verdict, because the m213 classifier filters on path category and
# runs inside `emit_open` after the entry/exit join -- it has never had a pid
# in its hand.
# #1185 — the scope check is now an assertion, not a record.
#
# It was non-blocking while the capture was known to be machine-wide:
# reddening a lane over a documented state adds noise. The subtree gate is
# in, so a foreign operation here is a hole in the gate, which is the one
# thing that must not pass quietly.
export WAYBILL_SCOPE_MUST_PASS=1
SCOPE_OUTPUT=/tmp/i1185-scope.attestation.json
SCOPE_PROBE=/tmp/wbscopeprobe   # <= 15 chars, so `comm` is not truncated
cp "$(command -v cat)" "$SCOPE_PROBE"   # resolved, not hardcoded to /bin
echo "==> #1185 scope: whose operations does the capture admit?"
set +e
"$MIKEBOM" trace capture \
    --attestation-format waybill-v1 \
    --include-system-reads \
    --output "$SCOPE_OUTPUT" \
    -- "$SCOPE_PROBE" /etc/hostname >/tmp/i1185-scope.log 2>&1
SCOPE_STATUS=$?
set -e
if [[ $SCOPE_STATUS -ne 0 || ! -f "$SCOPE_OUTPUT" ]]; then
    echo "FAIL: the #1185 scope probe exited $SCOPE_STATUS with no attestation"
    tail -40 /tmp/i1185-scope.log
    exit 1
fi

SCOPE_TOTAL=$(jq '.predicate.file_access.operations | length' "$SCOPE_OUTPUT")
SCOPE_OWN=$(jq '[.predicate.file_access.operations[]
                 | select(.process.comm == "wbscopeprobe")] | length' "$SCOPE_OUTPUT")
SCOPE_FOREIGN=$(jq '[.predicate.file_access.operations[]
                     | select(.process.comm != "wbscopeprobe")] | length' "$SCOPE_OUTPUT")

# Non-vacuity first, and these are hard failures. A scope test that measured
# nothing, or that missed the traced command itself, reports a clean or a
# total violation for a reason that has nothing to do with scope -- and a
# vacuous scope test is worse than none, because it reads like evidence.
if [[ "$SCOPE_TOTAL" -eq 0 ]]; then
    echo "FAIL: the scope probe captured zero operations, so it measured nothing"
    echo "      Neither a clean nor a violating result can be read from this."
    exit 1
fi
if [[ "$SCOPE_OWN" -eq 0 ]]; then
    echo "FAIL: the scope probe captured $SCOPE_TOTAL operations and none of them"
    echo "      came from the traced command itself (comm=wbscopeprobe)."
    echo "      Every operation would count as foreign for the wrong reason."
    jq -r '[.predicate.file_access.operations[].process.comm] | unique | join(", ")' \
        "$SCOPE_OUTPUT" | sed 's/^/      comms seen: /'
    exit 1
fi

echo "    operations captured:          $SCOPE_TOTAL"
echo "    from the traced command:      $SCOPE_OWN"
echo "    from other processes:         $SCOPE_FOREIGN   <- lower bound on foreign capture"
if [[ "$SCOPE_FOREIGN" -gt 0 ]]; then
    echo "    the other processes, by operation count:"
    jq -r '[.predicate.file_access.operations[]
            | select(.process.comm != "wbscopeprobe") | .process.comm]
           | group_by(.) | map({comm: .[0], ops: length})
           | sort_by(-.ops) | .[:10][] | "      \(.ops)\t\(.comm)"' "$SCOPE_OUTPUT"
fi

# The verdict. Non-blocking by default: the eBPF lane does not gate a merge
# (`ci-ok` excludes it) and reddening it on a known, recorded state would add
# noise to a lane that is already pending demotion. `WAYBILL_SCOPE_MUST_PASS=1`
# makes it an assertion, which is the switch to flip when scoping lands --
# no code change needed to make it blocking.
if [[ "$SCOPE_FOREIGN" -gt 0 ]]; then
    echo "    SCOPE: UNSCOPED -- $SCOPE_FOREIGN of $SCOPE_TOTAL operations came from"
    echo "           processes other than the traced command. The capture is"
    echo "           machine-wide, so no figure taken from it is a measurement"
    echo "           of a build (#1185)."
    if [[ "${WAYBILL_SCOPE_MUST_PASS:-0}" == "1" ]]; then
        echo "FAIL: WAYBILL_SCOPE_MUST_PASS=1 and the capture is not scoped"
        exit 1
    fi
    echo "           Recorded, not asserted. Set WAYBILL_SCOPE_MUST_PASS=1 to"
    echo "           make this blocking."
else
    echo "    SCOPE: CLEAN -- every one of $SCOPE_TOTAL operations came from the"
    echo "           traced command. Trustworthy because the two checks above"
    echo "           passed: operations were captured, and the traced command"
    echo "           was among them."
    echo "           If scoping has landed, update the baseline in"
    echo "           specs/1270-stable-probe-tracepoints/measurements/findings.md"
    echo "           and make this blocking with WAYBILL_SCOPE_MUST_PASS=1."
fi
echo

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
# #1252: the discard record is on for this run and the output is teed to
# a file, so a read-set entry that is missing can be checked against the
# requests the kernel discarded. It costs one log line per failed open
# and changes nothing the attestation carries.
WAYBILL_TRACE_OPEN_DIAG=1 "$MIKEBOM" trace capture \
    --attestation-format waybill-v1 \
    --output "$OUTPUT" \
    -- cargo build --release \
        --manifest-path "$FIXTURE/Cargo.toml" \
        --target-dir "$FIXTURE_TARGET" 2>&1 | tee /tmp/i1252-trace.log
TRACE_STATUS=${PIPESTATUS[0]}
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

# Assert the completeness signal is present and is one of the states the
# type defines. Not that it is `complete`: since #1185 wired the measured
# drop counters into the claim, `degraded` is the correct answer on a host
# that lost events, and failing here would be failing the honesty rather
# than the capture. The forced control further down is where `complete` is
# asserted NOT to hold.
COMPLETENESS=$(jq -r '.predicate.compiler_pipeline.completeness.state' "$OUTPUT")
echo "    completeness: $COMPLETENESS"

REASON=""
case "$COMPLETENESS" in
    complete)
        # OK
        ;;
    degraded)
        DROPPED=$(jq -r '.predicate.compiler_pipeline.completeness.dropped' "$OUTPUT")
        # Non-fatal, and no longer ring-buffer-specific: since #1185 the
        # figure is the sum of every measured loss mechanism. The breakdown
        # is in trace_integrity.event_drop_reasons, printed below.
        echo "    (degraded: dropped=$DROPPED — measured loss; see event_drop_reasons for the mechanism)"
        jq -c '.predicate.trace_integrity.event_drop_reasons // {}' "$OUTPUT" | sed 's/^/      /'
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

# #1252 — the honesty invariant. Not a check on the kernel: it passes on a
# kernel where every probe attaches and on one where none do. It fails only
# if the artifact's two statements about its own fidelity disagree.
#
# It exists because they did disagree, silently, for every run on GitHub's
# ubuntu-24.04 image 20261025.353: `do_filp_open` would not attach, file
# events fell from 902 to 443 on an identical build, and the attestation
# still carried `kprobe_attach_failures: []` with `completeness: complete`.
# The only thing that noticed was the SC-001 read-set assertion below, which
# names a missing file and says nothing about why.
ATTACH_FAILURES=$(jq -c '.predicate.trace_integrity.kprobe_attach_failures' "$OUTPUT")
ATTACH_FAILURE_COUNT=$(jq '.predicate.trace_integrity.kprobe_attach_failures | length' "$OUTPUT")
echo "    kprobe_attach_failures: $ATTACH_FAILURES"

if [[ "$ATTACH_FAILURE_COUNT" -gt 0 && "$COMPLETENESS" == "complete" ]]; then
    echo "FAIL: $ATTACH_FAILURE_COUNT probe(s) did not attach, yet completeness reads 'complete' (#1252)"
    echo "      failures: $ATTACH_FAILURES"
    exit 1
fi
if [[ "$COMPLETENESS" == "partial" && "$REASON" == "probe_attach_failed" \
      && "$ATTACH_FAILURE_COUNT" -eq 0 ]]; then
    echo "FAIL: completeness reads 'partial/probe_attach_failed' but kprobe_attach_failures is empty (#1252)"
    exit 1
fi
echo "    #1252 honesty invariant: completeness and kprobe_attach_failures agree"

# #1252 — the discarded-open counter, as a POSITIVE CONTROL.
#
# Capture now records opens that succeeded rather than opens that were
# attempted, which means a failed open leaves no trace in the read set.
# A zero here is therefore ambiguous on its face: either the build
# attempted no failed opens, or the exit tracepoint is not firing and
# every open is being dropped. Those look identical in the output and
# only one of them is healthy.
#
# The fixture resolves the ambiguity. A rustc + linker build probes for
# headers, libraries and optional manifests across every search path it
# has, and almost all of those opens fail by design — measured at 599
# discards for this fixture on a host where capture is intact. So zero
# here is a statement about the capture, not about the build.
DISCARDED=$(jq '.predicate.trace_integrity.open_failures_discarded // 0' "$OUTPUT")
echo "    open_failures_discarded: $DISCARDED"
if [[ "$DISCARDED" -eq 0 ]]; then
    echo "FAIL: zero discarded opens for a compiler build (#1252)"
    echo "      A compiler attempts failed opens constantly across its header"
    echo "      and library search paths. Zero means the sys_exit_open*"
    echo "      tracepoints are not reporting results — in which case every"
    echo "      open is being discarded, not just the failed ones — or the"
    echo "      OPEN_FAILURES_DISCARDED counter map did not load."
    echo "      Check kprobe_attach_failures above for the counter's name."
    exit 1
fi

# #1252 — the request/result join must not cost ring-buffer volume.
#
# FR-005b: an open that did not succeed must not occupy ring-buffer space
# at all, and per-build volume must not increase. The ceiling is the
# pre-change count for this fixture (573 events, measured; see
# specs/1270-stable-probe-tracepoints/measurements/baseline-before.md).
# An implementation that sent both the request and the result to
# userspace and joined them there would pass every content-based check
# above while roughly doubling volume, so this is measured rather than
# argued.
#
# The margin is wide on purpose: the same host produced 573 and 575 on
# two runs of the same fixture, so the count is not bit-stable.
FILE_OP_COUNT=$(jq '.predicate.file_access.operations | length' "$OUTPUT")
echo "    file operations: $FILE_OP_COUNT (pre-#1252 ceiling: 600)"
if [[ "$FILE_OP_COUNT" -gt 600 ]]; then
    echo "FAIL: $FILE_OP_COUNT file operations exceeds the pre-#1252 ceiling (#1252 FR-005b/SC-008)"
    echo "      The kernel-side join should REDUCE volume: one syscall is now"
    echo "      one event where do_sys_openat2 and do_filp_open both fired,"
    echo "      and failed opens are discarded before the ring buffer."
    echo "      A count above the ceiling suggests both observations are"
    echo "      reaching userspace."
    exit 1
fi

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
        # #1252 diagnostic: this assertion names a missing file and says
        # nothing about why. The capture is small enough to print whole,
        # and the three candidate causes are distinguishable from it:
        #   - the open was resolved to a relative path we cannot match
        #     -> unresolved_relative_opens > 0 and the raw path appears
        #   - the open was observed and classified as a failure
        #     -> the path appears in the discard log above
        #   - the open was never observed at all
        #     -> neither, and the total is far below the other arch's
        echo "      --- diagnostic ---"
        echo "      total file operations: $(jq '.predicate.file_access.operations | length' "$OUTPUT")"
        echo "      unresolved_relative_opens: $(jq '.predicate.trace_integrity.unresolved_relative_opens // 0' "$OUTPUT")"
        echo "      operations flagged unresolved: $(jq '[.predicate.file_access.operations[] | select(.unresolved_relative)] | length' "$OUTPUT")"
        echo "      open_failures_discarded: $(jq '.predicate.trace_integrity.open_failures_discarded // 0' "$OUTPUT")"
        echo "      rustc read sets, every entry:"
        reads_of 'select(.compiler == "rustc")' | sort -u | sed 's/^/        /'
        echo "      every recorded operation:"
        jq -r '.predicate.file_access.operations[]
               | "        \(.operation) \(if .unresolved_relative then "UNRESOLVED " else "" end)\(.path)"' \
            "$OUTPUT" | sort -u
        echo "      did the discard record claim it? (needs WAYBILL_TRACE_OPEN_DIAG=1 above)"
        grep -F "$root" /tmp/i1252-trace.log 2>/dev/null | sed 's/^/        /' | head -5 \
            || echo "        (no discard line names it)"
        echo "      --- end diagnostic ---"
        if [[ "$ATTACH_FAILURE_COUNT" -gt 0 ]]; then
            echo "      probe(s) did not attach on this kernel: $ATTACH_FAILURES"
            echo "      a read set assembled without an open-family kprobe is"
            echo "      incomplete by construction — see #1252 before reading this"
            echo "      as a reader or fixture defect."
        fi
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

# --- US4: an operator can see how much was hashed --------------------
echo
echo "==> m1072 US4: coverage record (FR-013, FR-014)"

COVERAGE=$(jq -c '.predicate.trace_integrity.compiler_hash_coverage' "$OUTPUT")
if [[ "$COVERAGE" == "null" || -z "$COVERAGE" ]]; then
    echo "FAIL: no compiler_hash_coverage record, though this trace captured"
    echo "      $INVOCATION_COUNT compiler invocations (FR-014)"
    exit 1
fi
echo "    $COVERAGE"

# FR-013's partition, on real data. Computed in jq rather than asserted
# field-by-field so a miscount cannot hide behind a plausible-looking
# record.
PARTITION_OK=$(jq '.predicate.trace_integrity.compiler_hash_coverage
                   | (.paths_hashed + .paths_absent + .paths_skipped) == .paths_total' "$OUTPUT")
if [[ "$PARTITION_OK" != "true" ]]; then
    echo "FAIL: hashed + absent + skipped != total (FR-013)"
    jq '.predicate.trace_integrity.compiler_hash_coverage' "$OUTPUT"
    exit 1
fi

# The record must agree with the sets it describes: paths_total is over
# DISTINCT paths across both sets. A record that drifts from the document
# it sits in would be worse than no record.
DISTINCT_IN_SETS=$(jq '[.predicate.compiler_pipeline.invocations[]?
                        | (.read_set[]?, .write_set[]?)
                        | select((.kind | type) != "object")
                        | .path] | unique | length' "$OUTPUT")
RECORD_TOTAL=$(jq '.predicate.trace_integrity.compiler_hash_coverage.paths_total' "$OUTPUT")
if [[ "$RECORD_TOTAL" -ne "$DISTINCT_IN_SETS" ]]; then
    echo "FAIL: coverage says $RECORD_TOTAL paths, the sets contain $DISTINCT_IN_SETS distinct non-stdin paths (FR-013)"
    exit 1
fi
echo "    paths_total agrees with the sets: $RECORD_TOTAL distinct paths"

# A stock run must say so, and paths_hashed must match the number of
# entries that actually carry a hash.
BUDGET_SOURCE=$(jq -r '.predicate.trace_integrity.compiler_hash_coverage.budget_source' "$OUTPUT")
if [[ "$BUDGET_SOURCE" != "default" ]]; then
    echo "FAIL: budget_source is '$BUDGET_SOURCE' on a run with no --compiler-hash-budget-bytes (C-1072-8)"
    exit 1
fi
HASHED_PATHS=$(jq '[.predicate.compiler_pipeline.invocations[]?
                    | (.read_set[]?, .write_set[]?)
                    | select(.sha256_at_close != null) | .path] | unique | length' "$OUTPUT")
RECORD_HASHED=$(jq '.predicate.trace_integrity.compiler_hash_coverage.paths_hashed' "$OUTPUT")
if [[ "$RECORD_HASHED" -ne "$HASHED_PATHS" ]]; then
    echo "FAIL: coverage says $RECORD_HASHED hashed, $HASHED_PATHS distinct paths carry a hash"
    exit 1
fi
echo "    paths_hashed agrees with the entries carrying a hash: $RECORD_HASHED"

# SC-006: budget 0 truncates, discloses, and still exits zero. `0` is the
# cheapest possible truncation test -- no large fixture required.
echo
echo "==> m1072 US4: budget truncation (SC-006)"
TRUNC_OUTPUT=/tmp/m1072-truncated.attestation.json
rm -rf "$FIXTURE_TARGET"
set +e
"$MIKEBOM" trace capture \
    --attestation-format waybill-v1 \
    --compiler-hash-budget-bytes 0 \
    --output "$TRUNC_OUTPUT" \
    -- cargo build --release \
        --manifest-path "$FIXTURE/Cargo.toml" \
        --target-dir "$FIXTURE_TARGET"
TRUNC_STATUS=$?
set -e
if [[ $TRUNC_STATUS -ne 0 ]]; then
    echo "FAIL: a zero budget must not fail the command — hashing is advisory"
    echo "      and truncating it loses no observation (C-1072-10). exit=$TRUNC_STATUS"
    exit 1
fi
if [[ ! -f "$TRUNC_OUTPUT" ]]; then
    echo "FAIL: no attestation written under a zero budget (C-1072-10 step 3)"
    exit 1
fi
TRUNC_OK=$(jq '.predicate.trace_integrity.compiler_hash_coverage
               | .truncated == true and .paths_hashed == 0
                 and .budget_source == "operator"
                 and ((.paths_hashed + .paths_absent + .paths_skipped) == .paths_total)' "$TRUNC_OUTPUT")
if [[ "$TRUNC_OK" != "true" ]]; then
    echo "FAIL: zero-budget coverage record is wrong (SC-006)"
    jq '.predicate.trace_integrity.compiler_hash_coverage' "$TRUNC_OUTPUT"
    exit 1
fi
TRUNC_HASHES=$(jq '[.predicate.compiler_pipeline.invocations[]?
                    | (.read_set[]?, .write_set[]?)
                    | select(.sha256_at_close != null)] | length' "$TRUNC_OUTPUT")
if [[ "$TRUNC_HASHES" -ne 0 ]]; then
    echo "FAIL: $TRUNC_HASHES entries carry a hash under a zero budget (SC-006)"
    exit 1
fi
# Existence is NOT truncated -- only hashing is. This is the half of
# C-1072-10 that says no observation is lost.
TRUNC_ABSENT=$(jq '.predicate.trace_integrity.compiler_hash_coverage.paths_absent' "$TRUNC_OUTPUT")
if [[ "$TRUNC_ABSENT" -lt 1 ]]; then
    echo "FAIL: a zero budget suppressed existence reporting too (C-1072-10)"
    exit 1
fi
echo "    exit 0, truncated: true, paths_hashed: 0, budget_source: operator"
echo "    existence still reported: paths_absent=$TRUNC_ABSENT"

# SC-005a: the close-time tier introduces no nondeterminism of its own.
#
# NOT whole-payload byte-identity. That is unachievable and predates this
# milestone: `invocation_id` is bpf_ktime_get_ns() at exec by m210's
# design, and rustc's temporaries are mkdtemp-random, so deps/rmetaA36mif
# in one run is deps/rmeta35kQFf in the next. Both sit in the observation
# tier, which the pass may not touch (Principle II). Contract C-1072-6
# records the measurement that settled this.
#
# What the pass controls is asserted instead: the coverage record, and
# every path's close-time facts for the paths both runs observed.
echo
echo "==> m1072 US4: close-time determinism (SC-005a)"
DET_A=/tmp/m1072-det-a.json
DET_B=/tmp/m1072-det-b.json
for out in "$DET_A" "$DET_B"; do
    # Clean between runs or the second build is a no-op, and two empty
    # traces agree just as well as two correct ones.
    rm -rf "$FIXTURE_TARGET"
    "$MIKEBOM" trace capture \
        --attestation-format waybill-v1 \
        --output "$out" \
        -- cargo build --release \
            --manifest-path "$FIXTURE/Cargo.toml" \
            --target-dir "$FIXTURE_TARGET" > /dev/null 2>&1
done

COV_A=$(jq -cS '.predicate.trace_integrity.compiler_hash_coverage' "$DET_A")
COV_B=$(jq -cS '.predicate.trace_integrity.compiler_hash_coverage' "$DET_B")
if [[ "$COV_A" != "$COV_B" ]]; then
    echo "FAIL: the coverage record differs between two runs (SC-005a)"
    echo "      run A: $COV_A"
    echo "      run B: $COV_B"
    exit 1
fi
DET_PATHS=$(jq '.predicate.trace_integrity.compiler_hash_coverage.paths_total // 0' "$DET_A")
if [[ "$DET_PATHS" -lt 1 ]]; then
    echo "FAIL: determinism run captured no paths — nothing was compared (SC-005a)"
    exit 1
fi
echo "    coverage record identical across runs: $COV_A"

# Per-path close-time facts, over the paths both runs saw. The random
# temporaries differ by name and simply do not intersect, which is why the
# comparison is an intersection rather than a diff.
facts() {
    jq -r '[.predicate.compiler_pipeline.invocations[]?
            | (.read_set[]?, .write_set[]?)
            | "\(.path)\t\(.existed_at_close)\t\(.sha256_at_close.value // "none")"]
           | unique | .[]' "$1"
}
facts "$DET_A" | sort > /tmp/m1072-facts-a
facts "$DET_B" | sort > /tmp/m1072-facts-b
SHARED=$(comm -12 <(cut -f1 /tmp/m1072-facts-a | sort -u) <(cut -f1 /tmp/m1072-facts-b | sort -u) | wc -l | tr -d ' ')
CONFLICTS=0
while IFS= read -r p; do
    a=$(grep -F "$p	" /tmp/m1072-facts-a | head -1)
    b=$(grep -F "$p	" /tmp/m1072-facts-b | head -1)
    if [[ "$a" != "$b" ]]; then
        echo "FAIL: close-time facts differ between runs for $p"
        echo "      A: $a"
        echo "      B: $b"
        CONFLICTS=$((CONFLICTS + 1))
    fi
done < <(comm -12 <(cut -f1 /tmp/m1072-facts-a | sort -u) <(cut -f1 /tmp/m1072-facts-b | sort -u))
if [[ "$CONFLICTS" -ne 0 ]]; then
    echo "FAIL: $CONFLICTS paths disagree across runs (SC-005a)"
    exit 1
fi
# A tiny intersection would make the comparison above nearly vacuous: the
# fixture's real sources and artifacts have stable names and must dominate.
if [[ "$SHARED" -lt 20 ]]; then
    echo "FAIL: only $SHARED paths appear in both runs — too few to call this a check (SC-005a)"
    exit 1
fi
echo "    close-time facts identical for all $SHARED paths both runs observed"

# --- SC-007: a trace with no compiler invocations is unchanged --------
#
# The m213 widen run above traced `cat /etc/hostname`, which invokes no
# compiler, so it is exactly the zero-invocation case FR-014 is about.
# Reusing it costs nothing and tests the real emission path.
echo
echo "==> m1072 SC-007: zero-compiler trace is byte-unchanged"

if jq -e '.predicate.trace_integrity | has("compiler_hash_coverage")' "$WIDENED_OUTPUT" > /dev/null 2>&1; then
    echo "FAIL: a trace with no compiler invocations emitted a coverage record (FR-014)"
    jq '.predicate.trace_integrity' "$WIDENED_OUTPUT"
    exit 1
fi

# Stronger than "no new key": every key must be one the pre-m1072 struct
# already had, so an unrelated addition cannot slip through either. The
# list comes from `git show 33561a91:waybill-common/src/attestation/integrity.rs`
# -- the merge base of this branch -- not from the current struct, which
# would make the check circular.
PRE_M1072_FIELDS='["bloom_filter_capacity","bloom_filter_false_positive_rate","events_dropped","filter_categories_applied","kprobe_attach_failures","partial_captures","ring_buffer_overflows","unresolved_relative_opens","uprobe_attach_failures"]'
# Fields this branch adds on purpose, listed one by one rather than by
# widening the check, so an addition nobody intended still fails here.
#
#   open_failures_discarded (#1252) — a trace of `cat` reaches this
#     legitimately: the loader probes several paths for libc and most of
#     those opens fail, which is the whole point of counting them.
#   event_drop_reasons (#618) — absent on a lossless capture, so it is
#     usually not present at all; admitted because it may be.
#
# Found by running the harness to completion on arm64. The x86-64 lane
# exits at the m1070 SC-001 check above, so CI has never reached this
# assertion and never reported the conflict.
THIS_BRANCH_FIELDS='["open_failures_discarded","event_drop_reasons","pending_open_entries_at_close","traced_subtree_opens"]'
ALLOWED_FIELDS=$(jq -cn --argjson a "$PRE_M1072_FIELDS" --argjson b "$THIS_BRANCH_FIELDS" '$a + $b')
# `index($k)` is an exact membership test. The earlier form,
# `[.] | inside($pre)`, matched substrings -- jq's `inside` on strings is
# substring containment -- so a stray `ring_buffer` key would have passed
# against `ring_buffer_overflows`. Exact matching makes this strictly
# stronger than before.
UNEXPECTED=$(jq --argjson pre "$ALLOWED_FIELDS" \
    '[.predicate.trace_integrity | keys[] | . as $k | select($pre | index($k) | not)]' \
    "$WIDENED_OUTPUT")
if [[ "$UNEXPECTED" != "[]" ]]; then
    echo "FAIL: trace_integrity carries keys the pre-m1072 struct did not have (SC-007)"
    echo "      $UNEXPECTED"
    exit 1
fi
echo "    trace_integrity keys: $(jq -c '.predicate.trace_integrity | keys' "$WIDENED_OUTPUT")"
echo "    all present in the pre-m1072 struct or deliberately added by this branch;"
echo "    no coverage record emitted"

echo "==> #618 drop counters: the plumbing is self-verifying"
# A counter that cannot fire is worse than no counter: it reports a
# confident zero. This forces one drop path and asserts the count is
# exact, so every OTHER zero in event_drop_reasons is readable.
#
# `openat(AT_FDCWD, NULL, 0)` is the only way to reach the null-path
# branch. Called through glibc's wrapper rather than a raw syscall
# number -- openat is 257 on x86_64 and 56 on aarch64, and the first
# version of this control called whatever 257 is on arm64 and silently
# measured nothing.
DROPS_OUTPUT=/tmp/i618-control.attestation.json
cat > /tmp/i618-nullopen.py <<'PYEOF'
import ctypes
libc = ctypes.CDLL("libc.so.6", use_errno=True)
libc.openat.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int]
for _ in range(5):
    libc.openat(-100, None, 0)
PYEOF
"$MIKEBOM" trace capture --attestation-format waybill-v1 \
    --output "$DROPS_OUTPUT" -- python3 /tmp/i618-nullopen.py > /dev/null 2>&1
NULL_PATH_DROPS=$(jq '.predicate.trace_integrity.event_drop_reasons.kernel_null_path // 0' \
    "$DROPS_OUTPUT")
echo "    kernel_null_path after 5 forced openat(NULL): $NULL_PATH_DROPS"
if [[ "$NULL_PATH_DROPS" -ne 5 ]]; then
    echo "FAIL: expected exactly 5 kernel_null_path drops, got $NULL_PATH_DROPS (#618)"
    echo "      The kernel-side EVENT_DROPS slots are not reaching the"
    echo "      attestation, which makes every kernel-side zero in"
    echo "      event_drop_reasons unreadable -- the exact ambiguity #618"
    echo "      exists to remove."
    jq -c '.predicate.trace_integrity.event_drop_reasons' "$DROPS_OUTPUT" | sed 's/^/      /'
    exit 1
fi
echo "    #618 control: the drop counters fire, and the count is exact"

echo "==> #1185 control: the one drop that had no counter"
# `store_open` discarding a failed `PENDING_OPEN.insert` was the only
# early return in the open path that recorded nothing, and the open it
# dropped is lost outright -- the exit has no request to join. The map is
# bounded (4096) and machine-wide, so saturation is reachable and nothing
# said when it happened.
#
# Saturating a 4096-entry map on demand is not something a test can
# arrange, so the branch is forced instead, the same way
# WAYBILL_TRACE_FORCE_REQUEST_ONLY forces the FR-004b fallback. This is a
# control over the plumbing: that the count reaches the attestation, and
# that losing the request loses the open.
#
# It does NOT establish that a genuinely full map returns an error here.
# That rests on bpf_map_update_elem rejecting an insert into a full hash
# map, and only real saturation confirms it. Labelled, not implied.
#
# `rustc --version` rather than a trivial command because every open is
# lost by construction, and `trace capture` refuses a capture that
# aggregated nothing. The compiler exec arrives on a different ring buffer
# and is unaffected, so the capture is admitted with an empty read set --
# which is the point being demonstrated.
#
# `--include-system-reads` strengthens the zero-operations assertion below
# rather than weakening it: with the m213 System category filtered, a
# surviving ambient open would be dropped downstream and the assertion
# would pass on a flag that had covered only some open variants. Widened,
# anything that survives is visible.
FORCED_OUTPUT=/tmp/i1185-forced.attestation.json
set +e
WAYBILL_TRACE_FORCE_PENDING_INSERT_FAIL=1 "$MIKEBOM" trace capture \
    --attestation-format waybill-v1 \
    --include-system-reads \
    --output "$FORCED_OUTPUT" \
    -- rustc --version >/tmp/i1185-forced.log 2>&1
FORCED_STATUS=$?
set -e
if [[ $FORCED_STATUS -ne 0 || ! -f "$FORCED_OUTPUT" ]]; then
    echo "FAIL: the #1185 forced-failure control exited $FORCED_STATUS with no attestation"
    echo "      A control that did not run proves nothing about the counter."
    tail -40 /tmp/i1185-forced.log
    exit 1
fi
FORCED_INSERT_FAILS=$(jq '.predicate.trace_integrity.event_drop_reasons.kernel_pending_insert_failed // 0' "$FORCED_OUTPUT")
FORCED_ORPHANS=$(jq '.predicate.trace_integrity.event_drop_reasons.kernel_exit_without_request // 0' "$FORCED_OUTPUT")
FORCED_OPS=$(jq '.predicate.file_access.operations | length' "$FORCED_OUTPUT")
FORCED_HELD=$(jq '.predicate.trace_integrity.pending_open_entries_at_close // "unmeasured"' "$FORCED_OUTPUT")
# `.predicate.compiler_pipeline.completeness.state`, the path the m210
# check at the top of this script uses. The first version of this line
# looked under `trace_integrity` and printed "absent", which reads as
# "the capture declined to claim completeness" -- the opposite of the
# finding. A probe that names the wrong field says something false about
# the system with full confidence.
FORCED_COMPLETENESS=$(jq -r '.predicate.compiler_pipeline.completeness.state // "absent"' "$FORCED_OUTPUT")
FORCED_DROPPED=$(jq -r '.predicate.compiler_pipeline.completeness.dropped // "-"' "$FORCED_OUTPUT")
FORCED_REASON=$(jq -r '.predicate.compiler_pipeline.completeness.reason // "-"' "$FORCED_OUTPUT")
echo "    kernel_pending_insert_failed: $FORCED_INSERT_FAILS"
echo "    kernel_exit_without_request:  $FORCED_ORPHANS   (the same losses, seen at the other end)"
echo "    file operations captured:     $FORCED_OPS   (expected 0 -- every request refused)"
echo "    pending_open_entries_at_close: $FORCED_HELD"
if [[ "$FORCED_INSERT_FAILS" -le 0 ]]; then
    echo "FAIL: the forced insert failure counted nothing (#1185)"
    echo "      Either the flag did not reach the kernel or the new EVENT_DROPS"
    echo "      slot is past the end of the map, in which case get_ptr_mut"
    echo "      returns None and the counter reads zero forever -- a counter"
    echo "      that cannot see its own subject."
    jq -c '.predicate.trace_integrity.event_drop_reasons' "$FORCED_OUTPUT" | sed 's/^/      /'
    exit 1
fi
if [[ "$FORCED_OPS" -ne 0 ]]; then
    echo "FAIL: $FORCED_OPS operations survived a trace where every request was refused"
    echo "      The forced flag is not covering every open variant, so the"
    echo "      control measures less than it claims."
    exit 1
fi
echo "    #1185 control: the counter fires, and refusing the request loses the open"
# Asserted, not merely printed. This block used to end here with a note
# saying the claim was #1185's remaining defect and that tightening it was
# a later change -- and it is now that change. `derive_completeness` read
# one input, `events_dropped`, which production hardcodes to 0, so the
# `Degraded` arm was unreachable and THIS run -- in which every single open
# was refused -- reported `complete`.
#
# The assertion is deliberately negative on one value rather than positive
# on another: `partial` is also an acceptable answer here (a missing probe
# outranks a count), and pinning the exact state would make this check fail
# on any kernel where a probe does not attach, for a reason that has
# nothing to do with what it is testing. What must never hold is `complete`.
echo "    completeness with a total capture loss: $FORCED_COMPLETENESS   (dropped=$FORCED_DROPPED reason=$FORCED_REASON)"
if [[ "$FORCED_COMPLETENESS" == "complete" ]]; then
    echo "FAIL: a capture that lost every open still claims 'complete' (#1185)"
    echo "      kernel_pending_insert_failed=$FORCED_INSERT_FAILS and"
    echo "      file operations captured=$FORCED_OPS, so the loss is measured"
    echo "      and the document disagrees with its own counters. The claim"
    echo "      is derived in trace/compiler_pipeline.rs::derive_completeness;"
    echo "      check that cli/scan.rs still calls record_measured_loss."
    exit 1
fi
if [[ "$FORCED_COMPLETENESS" == "absent" ]]; then
    echo "FAIL: no completeness state in the forced control's attestation"
    echo "      This check cannot distinguish an honest claim from a missing"
    echo "      one, so an absent field fails rather than passing quietly."
    exit 1
fi
echo "    #1185 claim: a total capture loss is not reported as 'complete'"

# Report what the real build dropped. Not an assertion: on a healthy host
# `user_no_invocation_mapping` is non-zero because cargo's own file
# activity belongs to no compiler invocation. It is printed so a CI log
# names the mechanism when a capture comes up short (#1185).
# #1185 step 1 — did the traced subtree get established, and did it reach
# past its root?
#
# With the gate still off, capture is byte-identical whether the subtree
# holds one process or ninety, so the attestation cannot answer this and the
# kernel counters are the only signal. `inside` counts opens from processes
# in the subtree; `outside` counts the rest, and is what the gate will drop.
#
# A zero `inside` is the failure that matters. It means the root announced
# itself -- the trace would have refused to start otherwise -- and no
# descendant inherited, so the propagation in `try_sched_process_fork` is
# not working. Enabling a gate on top of that would drop the entire build.
# Read from the attestation, not from the log. The first version of this
# grepped `inside=` out of the trace log and never matched: the container
# colours its output, so the log holds `inside` and `=` separated by escape
# sequences. Worse, with `set -euo pipefail` the no-match made the
# assignment exit 1 and killed this script *before* the FAIL branch below
# could print -- a check whose failure path was unreachable because the line
# feeding it killed the shell first.
INSIDE=$(jq -r '.predicate.trace_integrity.traced_subtree_opens // "absent"' "$OUTPUT")
OUTSIDE=$(jq -r '.predicate.trace_integrity.event_drop_reasons.kernel_outside_traced_subtree // 0' "$OUTPUT")
echo "==> #1185 traced subtree (fixture build)"
if [[ "$INSIDE" == "absent" ]]; then
    echo "FAIL: traced_subtree_opens is absent from the attestation"
    echo "      The field is emitted whenever the counter could be read, so"
    echo "      its absence means the counter was unreadable and the subtree"
    echo "      is unmeasured -- which is not the same as a subtree of zero."
    jq -c '.predicate.trace_integrity.kprobe_attach_failures' "$OUTPUT" | sed 's/^/      attach failures: /'
    exit 1
fi
echo "    opens admitted (inside the subtree): $INSIDE"
echo "    opens declined (other processes):    $OUTSIDE"
if [[ "$INSIDE" -eq 0 ]]; then
    echo "FAIL: the traced subtree admitted zero opens"
    echo "      The root announced itself -- the trace refuses to start"
    echo "      otherwise -- so this means fork propagation is broken, the"
    echo "      subtree never grew past its root, and the gate has dropped"
    echo "      the entire build while the attestation still describes it"
    echo "      (#1185, the exact failure this work exists to remove)."
    exit 1
fi
# No threshold on the ratio. Nothing has measured what it should be on this
# fixture, and a number invented here would be exactly the kind of figure
# that passes every test written around it (CLAUDE.md, "measure external
# behaviour before designing around it").
#
# The real guards on this change are elsewhere and already in this script:
# m1070 SC-001, the 9-invocation / 7-rustc counts, and the m1072 read and
# write set checks. If the gate dropped something the build needed, those
# fail -- which is the assertion a ratio threshold would only approximate.
echo "    (declined opens are other processes', not loss by this capture)"
echo

echo "    pending_open_entries_at_close (fixture build): $(jq '.predicate.trace_integrity.pending_open_entries_at_close // "unmeasured"' "$OUTPUT")"
echo "      ^ of 4096. Read as accumulated leakage, not peak: an entry is"
echo "        removed when its syscall exits, so what remains are threads"
echo "        that died inside an open. Near the cap means saturation is"
echo "        reachable and kernel_pending_insert_failed is what it costs."
echo "    event_drop_reasons (fixture build):"
jq -r '(.predicate.trace_integrity.event_drop_reasons // {})
       | to_entries | sort_by(-.value) | .[] | "      \(.key) \(.value)"' "$OUTPUT"

# #1185 — the same slots beside the control measured at the top of this run,
# and the excess the build is answerable for. A slot the control already
# holds is ambient traffic on this runner, not loss by the build; only the
# difference says anything about the capture. Negative differences are shown
# as they are rather than clamped: the control is one sample, so a build
# below it means the slots are noise-dominated, which is itself the finding.
echo "    event_drop_reasons (control / fixture / excess):"
jq -rn --slurpfile c "$CONTROL_OUTPUT" --slurpfile f "$OUTPUT" '
    ($c[0].predicate.trace_integrity.event_drop_reasons // {}) as $ctl
  | ($f[0].predicate.trace_integrity.event_drop_reasons // {}) as $fix
  | (($ctl | keys) + ($fix | keys) | unique)
  | .[]
  | . as $k
  | (($ctl[$k] // 0)) as $a
  | (($fix[$k] // 0)) as $b
  | "      \($k) \($a) / \($b) / \($b - $a)"'

echo "==> #1252 SC-007: an IPv6-only connection is recorded"
# The old capture was a kprobe on `tcp_v4_connect`, which IPv6 connections
# never reach, so a build that connected only over IPv6 was recorded as
# having made no connection at all — indistinguishable from an offline
# build. The destination now comes from the `sockaddr` the process passed,
# so the address family is read rather than inferred.
#
# Verified against a deliberately IPv6-only case, not inferred from a build
# that happens to use both. The listener is on ::1 inside the container, so
# this needs no network egress and cannot flake on DNS or a firewall.
#
# The compiler fixture cannot carry this check: it is hermetic and makes no
# connection at all, so every other run in this harness reports
# connections=0 and the IPv6 path would stay unobserved while compiling,
# loading and attaching perfectly well.
V6_OUTPUT=/tmp/i1270-ipv6.attestation.json
if ! python3 -c 'import socket; socket.socket(socket.AF_INET6, socket.SOCK_STREAM).close()' 2>/dev/null; then
    echo "FAIL: no IPv6 support in this container; the SC-007 case cannot run"
    echo "      A skip here would read like a pass. If IPv6 is genuinely"
    echo "      unavailable on the runner, the check needs a different host,"
    echo "      not a silent bypass."
    exit 1
fi

# A listener on ::1, and a client that connects to it and nothing else.
python3 - > /tmp/i1270-port <<'PYEOF'
import socket
s = socket.socket(socket.AF_INET6, socket.SOCK_STREAM)
s.bind(("::1", 0))
s.listen(1)
print(s.getsockname()[1])
import os, sys
sys.stdout.flush()
# Hold the socket open in a child so the port stays bound for the trace.
if os.fork() == 0:
    try:
        c, _ = s.accept()
        c.close()
    except Exception:
        pass
    os._exit(0)
PYEOF
V6_PORT=$(cat /tmp/i1270-port)
echo "    IPv6 listener on [::1]:$V6_PORT"

set +e
"$MIKEBOM" trace capture \
    --attestation-format waybill-v1 \
    --output "$V6_OUTPUT" \
    -- python3 -c "import socket; s=socket.socket(socket.AF_INET6, socket.SOCK_STREAM); s.connect(('::1', $V6_PORT)); s.close()"
V6_STATUS=$?
set -e
if [[ $V6_STATUS -ne 0 ]]; then
    echo "FAIL: the IPv6 trace exited $V6_STATUS"
    exit 1
fi

# `destination.ip` is a string; an IPv6 literal is the only form that
# contains a colon, so this discriminates without parsing.
V6_CONNS=$(jq '[.predicate.network_trace.connections[]
               | select(.destination.ip | contains(":"))] | length' "$V6_OUTPUT")
echo "    connections with a v6 destination: $V6_CONNS"
if [[ "$V6_CONNS" -eq 0 ]]; then
    echo "FAIL: an IPv6 connection to [::1]:$V6_PORT was not recorded (#1252 SC-007)"
    echo "      This is the exact case the old tcp_v4_connect kprobe could"
    echo "      not see. All connections recorded:"
    jq -c '.predicate.network_trace.connections[].destination' "$V6_OUTPUT" | sed 's/^/        /'
    exit 1
fi
echo "    #1252 SC-007: the IPv6 destination is recorded"

echo
echo ">>> m210 + m212 + m213 + m1072 integration test PASSED"
echo "    compiler_pipeline: $INVOCATION_COUNT invocations, $RUSTC_COUNT rustc"
echo "    trace_integrity: ring_buffer_overflows=$OVERFLOWS"
echo "    filter_categories_applied (default run): [$APPLIED]"
echo "    filter_categories_applied (widened run): [$(jq -r '.predicate.trace_integrity.filter_categories_applied | join(",")' "$WIDENED_OUTPUT")]"
