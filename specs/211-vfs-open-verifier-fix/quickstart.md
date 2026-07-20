# Quickstart: end-to-end verification for m211

**Milestone**: 211
**Date**: 2026-07-20
**Purpose**: Concrete, copy-pasteable recipe to reproduce the m211 acceptance criteria from a clean check-out. This is the exact sequence a reviewer runs to satisfy themselves the fix works.

## Prerequisites

- macOS + Colima running an aarch64 Linux VM (`colima start` if not already running).
- Colima's docker storage has ≥ 30 GB free (`colima ssh -- df -h /mnt/lima-colima`). If low, truncate the busiest compose-stack container logs (`bash /tmp/free-colima.sh` if the m210 script is still present) — this doesn't affect m211's own state.
- The mikebom repo is checked out at `/Users/mlieberman/Projects/mikebom` (or adjust paths in the invocations below).
- Working tree is on branch `211-vfs-open-verifier-fix` at commit `HEAD` (whatever the fix landed as).

## Step 1: Build the container image

```
docker build -f Dockerfile.ebpf-test -t mikebom-ebpf-test .
```

First run: ~10–15 minutes (pulls `rust:1.88-bookworm`, installs `clang`/`llvm`/`libelf-dev`/`bpf-linker`, compiles the eBPF kernel bytecode + mikebom-cli release binary with `--features ebpf-tracing`). Subsequent rebuilds: minutes when the cargo cache is warm.

**Expected exit**: 0. Any error indicates the code doesn't compile under the ebpf-tracing feature or the eBPF-side bytecode fails at bpf-linker time — either is a m211 regression.

## Step 2: Run the harness

```
docker run --rm --privileged \
  -v /sys/kernel/debug:/sys/kernel/debug \
  -v /tmp:/host-out \
  --entrypoint bash mikebom-ebpf-test -c '
    cd /mikebom && \
    /mikebom/target/release/mikebom trace run \
      --attestation-format mikebom-v1 \
      --attestation-output /host-out/m211-verify.json \
      -- cargo build --release \
        --manifest-path /mikebom/mikebom-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge/Cargo.toml \
        --target-dir /tmp/m211-verify-target
  ' 2>&1
```

**Note**: the `--rm --privileged` + `-v /sys/kernel/debug` mount is what m210 established for eBPF+tracepoint access. `-v /tmp:/host-out` lets us inspect the attestation from the host (Colima VM's `/tmp`) via `colima ssh` in Step 3.

## Step 3: Verify acceptance scenarios

### C-1 + C-2: neither kprobe emitted a WARN

```
docker run ... 2>&1 | grep -E 'could not attach (vfs_open|do_filp_open)'
```

**Expected**: empty output. Any match indicates a verifier rejection on the current kernel — MVP acceptance fails.

### C-3: file_access.operations populates

```
colima ssh -- sudo cat /tmp/m211-verify.json | jq '.predicate.file_access.operations | length'
```

**Expected**: value > 100. Typical cargo builds of the SC-001 fixture produce 3000–8000 file operations.

### C-7: kprobe_attach_failures does not include vfs_open or do_filp_open

```
colima ssh -- sudo cat /tmp/m211-verify.json | jq '.predicate.trace_integrity.kprobe_attach_failures'
```

**Expected**: empty array `[]` OR an array that does NOT contain `"vfs_open"` or `"do_filp_open"`.

### SC-003: C130 populates on real components

```
docker run --rm --privileged \
  -v /sys/kernel/debug:/sys/kernel/debug \
  -v /tmp:/host-out \
  --entrypoint bash mikebom-ebpf-test -c '
    cd /mikebom && \
    /mikebom/target/release/mikebom sbom generate \
      --attestation /host-out/m211-verify.json \
      --path /mikebom/mikebom-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge \
      --format cyclonedx-json \
      > /host-out/m211-verify.cdx.json
  '

# Then inspect from host:
colima ssh -- sudo jq '
  [.components[] |
    select(.purl | startswith("pkg:cargo/")) |
    select(.properties[]? | .name == "mikebom:source-read-set" and .value != "null")
  ] | length
' /tmp/m211-verify.cdx.json
```

**Expected**: value ≥ 50 % of the total `pkg:cargo/*` component count in the CDX SBOM. For the SC-001 fixture (~4 crates), that's ≥ 2 components carrying `mikebom:source-read-set` with real read-sets.

## Step 4: Byte-identity spot-check (C-4)

```
# Baseline: pre-m211 attestation from milestone 210's post-mortem
BASELINE=/tmp/m610-final.json  # last artifact from the m210 debug cycle

# Post-m211: from Step 2 above
POST=/tmp/m211-verify.json

colima ssh -- "sudo diff <(jq 'del(.predicate.file_access, .predicate.trace_integrity.kprobe_attach_failures)' ${BASELINE}) <(jq 'del(.predicate.file_access, .predicate.trace_integrity.kprobe_attach_failures)' ${POST})"
```

**Expected**: empty diff. Any output indicates wire-shape drift on a field OTHER than the intentionally-changed ones — FR-003 violation.

## Step 5 (optional): FR-008 log-volume check

Simulate a verifier rejection on a hypothetical unsupported kernel — this can be tested via a unit test rather than a live invocation:

```
cargo test -p mikebom --bin mikebom -- trace::loader::tests::warn_line_stays_under_500_bytes
```

**Expected**: test passes. Test fixture constructs a synthetic aya error with a 20 KB message body and asserts the resulting WARN emission is under 500 bytes.

## Cleanup

```
docker image prune -f  # drop dangling images from the iterative build cycle
colima ssh -- sudo rm -f /tmp/m211-verify.json /tmp/m211-verify.cdx.json
```

## When something fails

- **Docker build fails with "no space left on device"**: `colima ssh -- df -h /mnt/lima-colima` — if the compose-stack json logs have refilled to > 50 GB, truncate via `bash /tmp/free-colima.sh` (the m210 script).
- **`vfs_open` still WARNs**: reproduce, capture the full verifier trace via `RUST_LOG=aya=debug docker run ...` for the aya inner logs, and post to #611 with the tail lines. The root cause is likely a fix pattern that works on 6.5+ amd64 but not on aarch64 6.8 (or vice versa).
- **`file_access.operations` populates but has < 100 entries**: the fix loads but is dropping events somewhere — check `trace_integrity.ring_buffer_overflows` and `trace_integrity.events_dropped`.
- **C130 doesn't populate on ≥ 50 %**: the fix cascades correctly through the trace but m210's aggregator or map-to-source-read-set logic has a bug in the "real read_set → C130 payload" path. Escalate to a m210 hotfix.
