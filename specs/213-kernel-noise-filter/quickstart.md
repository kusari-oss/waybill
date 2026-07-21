# Quickstart: Verify the kernel-side trace-noise filter

**Feature**: 213-kernel-noise-filter
**Date**: 2026-07-21
**Prerequisites**: Colima or a Linux host with `docker`; a mikebom checkout at HEAD post-m212 merge; the m212 fixture `mikebom-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge` (already committed).

## The 60-second verification (Success Criteria SC-001 + SC-002 + SC-003)

```bash
# From mikebom repo root, on Colima or a Linux host:
docker build -f Dockerfile.ebpf-test -t mikebom-ebpf-test .
docker run --rm --privileged \
    -v /sys/kernel/debug:/sys/kernel/debug \
    mikebom-ebpf-test \
    /mikebom/scripts/ebpf-integration-test.sh
```

**Expected output** (m213 additions marked ▲):

```
==> m210 integration test: trace the SC-001 fixture build
    fixture: /mikebom/mikebom-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge
    output:  /tmp/m210-integration.attestation.json

... (existing m210 + m212 output) ...

    ring_buffer_overflows: 6                          ▲ (was: 13636 pre-m213)
    filter_categories_applied: [                      ▲ NEW
      "CargoFingerprint",                             ▲ NEW
      "Ephemeral",                                    ▲ NEW
      "System"                                        ▲ NEW
    ]                                                 ▲ NEW
    of which rustc: 4                                 ▲ (was: 0 pre-m213)

>>> m210 + m212 + m213 integration test PASSED
```

**Pass criteria** (from FR-014, FR-015, SC-001, SC-002, SC-003):

- `ring_buffer_overflows ≤ 10` (down from ≥100 pre-m213 baseline).
- `filter_categories_applied[]` contains `"CargoFingerprint"` (guaranteed by cargo build).
- At least one entry in `file_access.operations[]` has `comm == "rustc"`.
- At least one entry in `file_access.operations[]` has `comm ∈ {"ld", "ld.lld", "mold"}`.
- No entry in `file_access.operations[]` has a path under `target/*/fingerprint/`, `target/*/deps/`, or `target/incremental/`.

## Verify the widening flag (Success Criterion SC-006)

```bash
# Run twice — with and without --include-system-reads:
docker run --rm --privileged \
    -v /sys/kernel/debug:/sys/kernel/debug \
    mikebom-ebpf-test \
    bash -c "
        /mikebom/target/release/mikebom trace capture \
            --attestation-format mikebom-v1 \
            --output /tmp/default.attestation.json \
            -- cat /etc/hostname &&
        /mikebom/target/release/mikebom trace capture \
            --attestation-format mikebom-v1 \
            --include-system-reads \
            --output /tmp/widened.attestation.json \
            -- cat /etc/hostname &&
        echo '--- default (System filter active) ---' &&
        jq '.predicate.trace_integrity.filter_categories_applied' /tmp/default.attestation.json &&
        echo '--- widened (System filter disabled) ---' &&
        jq '.predicate.trace_integrity.filter_categories_applied' /tmp/widened.attestation.json &&
        echo '--- widened /etc/ operations ---' &&
        jq '[.predicate.file_access.operations[] | select(.path | startswith(\"/etc/\"))]' /tmp/widened.attestation.json
    "
```

**Expected**:
- Default run: `["System"]` in `filter_categories_applied` (System matched for `/etc/hostname`); no `/etc/` operations in `file_access.operations`.
- Widened run: `[]` or no `"System"` entry in `filter_categories_applied`; `/etc/hostname` present in `file_access.operations`.

## Verify the empty-category behavior (FR-009)

```bash
# On a zero-syscall command:
docker run --rm --privileged \
    -v /sys/kernel/debug:/sys/kernel/debug \
    mikebom-ebpf-test \
    bash -c "
        /mikebom/target/release/mikebom trace capture \
            --attestation-format mikebom-v1 \
            --output /tmp/empty.attestation.json \
            -- true &&
        jq '.predicate.trace_integrity.filter_categories_applied' /tmp/empty.attestation.json
    "
```

**Expected**: `[]` (empty JSON array), NOT `null`, NOT the field being absent.

## Local (non-container) unit tests

For fast iteration during development, unit tests exercise the userspace
aggregator + wire round-trip without needing a kernel:

```bash
# From mikebom repo root:
cargo test -p mikebom --lib counters::                # E4 tests (~5 tests)
cargo test -p mikebom-common --lib integrity::        # E5 tests (~1 new test)
```

**Expected**: all pass; new `filter_category_hits_*` and `trace_integrity_serde_populated_filter_categories_applied` tests appear as green.

## Verifier acceptance check (SC-004)

Any change to `path_matches_filter_category` MUST re-verify verifier
acceptance on every SC-003 kernel (5.15, 6.1, 6.6, 6.8). The container
harness above exercises 6.8 (Colima aarch64); CI's `lint-and-test-ebpf`
job exercises the amd64 kernels via `nick-fields/retry@v3`-guarded runs.
Rejection on any kernel is a merge-blocker per SC-004.

## Debugging: manually inspect the counter maps mid-trace

If a trace produces unexpected `filter_categories_applied` values, dump
the raw counter values via `bpftool`:

```bash
docker exec -it <container> bpftool map show
# find the FILTER_CATEGORY_HITS map id, then:
docker exec -it <container> bpftool map dump id <map_id>
```

Each slot's value is the per-CPU u64 count. Slot 0=System, 1=UserCache,
2=Ephemeral, 3=CargoFingerprint per `contracts/filter-hits-map.md`.

## Rollback recipe

If a kernel rejects the classifier or the container harness starts
failing SC-001/SC-002/SC-003 assertions:

1. Revert `mikebom-ebpf/src/programs/file_ops.rs` classifier addition (single commit).
2. Revert `mikebom-cli/src/trace/counters.rs` reader extension (single commit).
3. Revert `mikebom-common/src/attestation/integrity.rs` field addition (single commit).
4. Verify m212 tests + harness still pass: `cargo test --workspace && bash scripts/ebpf-integration-test.sh`.

The rollback is safe — the m213 changes are purely additive; pre-m213
behavior is preserved when the changes are removed.
