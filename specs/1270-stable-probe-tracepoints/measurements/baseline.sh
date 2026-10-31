#!/usr/bin/env bash
# #1185 Phase 3 — the measurement protocol for a scoped capture.
#
# Every earlier figure from this feature was a single run of a slightly
# different command: widen on or off, different fixtures, a forced flag,
# whatever the host was doing at the time. That is why they never
# reconciled. Two runs of one identical control reported 239 and then 415
# operations minutes apart, and a lone `0` was read as a property twice in
# one session when the same configuration later returned 396.
#
# So this fixes the command and repeats it. One fixture, one flag set, one
# field list, N runs, and min/median/max per field rather than a number.
# A figure quoted from a single run of this is a sample, and the point of
# committing the script is that the next person can re-run it rather than
# trust the table.
#
# Runs inside the eBPF test container, where the probes can attach:
#
#   docker build -f Dockerfile.ebpf-test -t waybill-ebpf-test:wip .
#   docker run --rm -i --privileged -v /sys/kernel/debug:/sys/kernel/debug \
#       --entrypoint bash waybill-ebpf-test:wip -s \
#       < specs/1270-stable-probe-tracepoints/measurements/baseline.sh
#
# RUNS=3 shortens it for a smoke check. The committed baseline is N=10.
set -uo pipefail

RUNS="${RUNS:-10}"
WAYBILL="${WAYBILL:-/waybill/target/release/waybill}"
FIXTURE="${FIXTURE:-/waybill/waybill-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge}"
TARGET="$FIXTURE/target"
RAW=/tmp/baseline-runs.jsonl
: > "$RAW"

# Provenance, because a range is only attributable with the host attached
# to it. The ambient term in this capture was measured at 0-396 operations
# before scoping, so "which machine, how loaded" is part of the result.
echo "## Provenance"
echo
echo "    arch:    $(uname -m)"
echo "    kernel:  $(uname -r)"
echo "    cpus:    $(nproc)"
echo "    waybill: $("$WAYBILL" --version 2>/dev/null | head -1)"
echo "    fixture: $FIXTURE"
echo "    runs:    $RUNS"
echo

# The flags are fixed and deliberately minimal: no --include-system-reads,
# because widening changes which records reach the ring buffer and so
# changes every count here. A widened baseline is a different measurement,
# not a more complete one.
run_once() {
    local i="$1" att="/tmp/baseline-$i.json" log="/tmp/baseline-$i.log"

    # Without this the build is a no-op from run 2 onward and the protocol
    # measures cargo deciding there is nothing to do. The first version of
    # this script did exactly that.
    rm -rf "$TARGET"

    local t0 t1 st
    t0=$(date +%s%N)
    "$WAYBILL" trace capture \
        --attestation-format waybill-v1 \
        --output "$att" \
        -- cargo build --release \
            --manifest-path "$FIXTURE/Cargo.toml" \
            --target-dir "$TARGET" >"$log" 2>&1
    st=$?
    t1=$(date +%s%N)

    if [[ $st -ne 0 || ! -f "$att" ]]; then
        # A failed run must not contribute a row. A range computed over
        # runs that did not happen looks exactly like a range over runs
        # that did.
        echo "    run $i: FAILED (exit $st) -- excluded" >&2
        tail -5 "$log" >&2
        return 1
    fi

    jq -c --argjson run "$i" --argjson ms "$(( (t1 - t0) / 1000000 ))" '
        .predicate as $p
      | $p.trace_integrity as $ti
      | {
          run: $run,
          elapsed_ms: $ms,
          file_operations:               ($p.file_access.operations | length),
          compiler_invocations:          ($p.compiler_pipeline.invocations // [] | length),
          rustc_invocations:             ([$p.compiler_pipeline.invocations // [] | .[]
                                           | select(.compiler // "" | test("rustc"))] | length),
          traced_subtree_opens:          ($ti.traced_subtree_opens // null),
          outside_traced_subtree:        ($ti.event_drop_reasons.kernel_outside_traced_subtree // 0),
          unresolved_relative_opens:     ($ti.unresolved_relative_opens // 0),
          pending_open_entries_at_close: ($ti.pending_open_entries_at_close // null),
          ring_buffer_overflows:         ($ti.ring_buffer_overflows // 0),
          open_failures_discarded:       ($ti.open_failures_discarded // 0),
          exit_without_request:          ($ti.event_drop_reasons.kernel_exit_without_request // 0),
          pending_insert_failed:         ($ti.event_drop_reasons.kernel_pending_insert_failed // 0),
          null_path:                     ($ti.event_drop_reasons.kernel_null_path // 0),
          path_unreadable:               ($ti.event_drop_reasons.kernel_path_unreadable // 0),
          traced_insert_failed:          ($ti.event_drop_reasons.kernel_traced_insert_failed // 0)
        }' "$att" >> "$RAW"
}

ok=0
for i in $(seq 1 "$RUNS"); do
    if run_once "$i"; then
        ok=$((ok + 1))
        printf '.' >&2
    fi
done
printf '\n' >&2

if [[ "$ok" -eq 0 ]]; then
    echo "FAIL: no run produced an attestation; there is nothing to report" >&2
    exit 1
fi

echo "## Baseline ($ok of $RUNS runs contributed)"
echo
jq -rs '
    def med:
        sort
      | if length == 0 then null
        elif length % 2 == 1 then .[(length / 2) | floor]
        else ((.[length / 2 - 1] + .[length / 2]) / 2)
        end;

    # Field order is the reading order, not alphabetical: what the capture
    # saw, then what it declined, then what it lost.
    [ "elapsed_ms", "file_operations", "compiler_invocations",
      "rustc_invocations", "traced_subtree_opens", "outside_traced_subtree",
      "unresolved_relative_opens", "pending_open_entries_at_close",
      "ring_buffer_overflows", "open_failures_discarded",
      "exit_without_request", "pending_insert_failed", "null_path",
      "path_unreadable", "traced_insert_failed" ] as $fields
  | . as $rows
  | "| field | min | median | max | distinct values |",
    "|---|---|---|---|---|",
    ( $fields[]
      | . as $f
      | [ $rows[] | .[$f] ] as $vs
      | ([ $vs[] | select(. != null) ]) as $nums
      | if ($nums | length) == 0 then
            "| \($f) | - | - | - | unmeasured in every run |"
        else
            "| \($f) | \($nums | min) | \($nums | med) | \($nums | max) | \($nums | unique | length) |"
        end )
' "$RAW"

echo
echo "### Stability"
echo
# A field that never moves is as much a result as one that does: it is what
# makes a later change in it meaningful, and it says which figures are
# trustworthy from a single run.
jq -rs '
    [ "file_operations", "compiler_invocations", "rustc_invocations",
      "traced_subtree_opens", "unresolved_relative_opens",
      "outside_traced_subtree", "pending_open_entries_at_close" ] as $fields
  | . as $rows
  | ( $fields[]
      | . as $f
      | ([ $rows[] | .[$f] | select(. != null) ] | unique) as $u
      | if ($u | length) <= 1 then
            "    deterministic: \($f) = \($u[0] // "unmeasured")"
        else
            "    varies:        \($f) across \($u | length) values, \($u | min)-\($u | max)"
        end )
' "$RAW"

echo
# The rows, not just the summary. The first run of this script wrote them
# to /tmp inside a `--rm` container and they died with it, so the
# distribution behind a 28-324 range could not be recovered from the table
# that reported it. A summary is not evidence if the evidence is gone.
echo "### Per-run rows"
echo
echo '```'
cat "$RAW"
echo '```'

