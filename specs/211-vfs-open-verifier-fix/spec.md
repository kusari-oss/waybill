# Feature Specification: Fix vfs_open kprobe eBPF verifier rejection

**Feature Branch**: `211-vfs-open-verifier-fix`
**Created**: 2026-07-19
**Status**: Draft
**Input**: User description: "611" (GitHub issue #611)

## Clarifications

### Session 2026-07-19

- Q: Target kernel version matrix (which kernels MUST the fix work on)? → A: Colima aarch64 6.8 (dev) + Ubuntu 22.04/24.04 amd64 (CI). Older kernels are best-effort with `kprobe_attach_failures` reporting; expanding coverage beyond what CI validates is a promise we can't keep.
- Q: Is `do_filp_open` in scope for this milestone, or genuinely deferred to a follow-up? → A: In scope. Both kprobes share the FileEvent construction pattern; separate milestones would duplicate the container-test cycle for near-identical work. Added as US-1b acceptance scenario.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Trace-mode SBOM carries observed file operations (Priority: P1)

An operator runs `mikebom trace run -- cargo build --release` on a Linux host. They want the emitted attestation to enumerate every file the traced build read or wrote — the same way it enumerates network connections and compiler invocations. Today the attestation's `predicate.file_access.operations[]` array is empty on every trace on aarch64 kernels (Colima's Linux 6.8), silently missing the entire file-operations dataset. Operators consuming the trace can't answer "what source files went into this binary?" — even though the trace ran to completion and reported no fatal errors.

**Why this priority**: File-op capture is the load-bearing input for milestone 210's `mikebom:source-read-set` (C130) per-component attribution. Without it, m210's headline promise ("this binary was built from these source files") returns "unknown" for every component. Every downstream reachability-analysis use case depends on this signal.

**Independent Test**: Run `mikebom trace run --attestation-format mikebom-v1 --attestation-output /tmp/out.json -- <any-shell-command-that-reads-a-file>` on a Linux host with `CAP_BPF`. Parse `/tmp/out.json`. Assert `.predicate.file_access.operations | length > 0` and at least one operation references a real read path.

**Acceptance Scenarios**:

1. **Given** a Linux host with an aarch64 kernel (Colima 6.8) and the container harness from milestone 210, **When** the operator runs `docker run --rm --privileged -v /sys/kernel/debug:/sys/kernel/debug mikebom-ebpf-test`, **Then** the trace startup log emits no WARN about vfs_open kprobe attachment failure.
1b. **Given** the same environment as scenario 1, **When** the trace starts, **Then** the `do_filp_open` kprobe ALSO attaches cleanly with no WARN. (Both kprobes share the FileEvent construction pattern; a fix that only clears vfs_open is incomplete.)
2. **Given** the same environment as scenario 1, **When** the traced `cargo build --release` runs against the milestone-210 SC-001 fixture, **Then** the emitted attestation's `.predicate.file_access.operations[]` contains at least one entry referencing a file the compiler actually read (e.g. `.rs` source or `.rlib`).
3. **Given** the fix applied, **When** the operator runs the same trace on an amd64 kernel (Linux 6.6+), **Then** both vfs_open and do_filp_open kprobes still attach cleanly (no regression against the platform where the earlier code presumably worked).

---

### User Story 2 — Milestone 210's C130 source-read-set populates on real components (Priority: P2)

An operator runs `mikebom trace run -- cargo build --release`, then runs `mikebom sbom generate --attestation /tmp/out.json --path <fixture>`. They want each generated binary component in the emitted SBOM to carry a `mikebom:source-read-set` annotation listing the source files that contributed. Today (post-m210, pre-#611) the annotation would fire on any component whose file path intersected a compiler invocation's write_set — but write_sets are empty because vfs_open is broken, so C130 never populates on real components. C131 falls back to `"unknown"` for every component.

**Why this priority**: Second-order value that depends on US1 landing. The m210 emission pipeline is already built and unit-tested; it just needs real read_set/write_set data from US1 to produce meaningful annotations end-to-end.

**Independent Test**: Same trace-mode invocation as US1, plus running `mikebom sbom generate --attestation /tmp/out.json --path <fixture>` on the emitted attestation. Assert that the generated SBOM has at least one component with a `mikebom:source-read-set` property whose value is a non-empty JSON payload (parsed via `jq '.components[] | .properties[]? | select(.name == "mikebom:source-read-set")'`).

**Acceptance Scenarios**:

1. **Given** a trace produced under US1 with populated `file_access.operations`, **When** `mikebom sbom generate` runs against a matching source-tree scan, **Then** at least one component in the generated SBOM carries `mikebom:source-read-set` with a non-empty `read_set` array.
2. **Given** the same trace, **When** examining the generated SBOM, **Then** every component carries `mikebom:read-set-source` with value `"traced"` (previously all `"unknown"`).

---

### User Story 3 — Verifier rejection produces actionable operator diagnostics (Priority: P3)

An operator on a kernel where the fix still doesn't verify (older kernel, disabled BPF feature, exotic architecture) needs to understand that file-op capture is degraded WITHOUT reading 20 KB of raw verifier output in a WARN log. Today the mikebom log emits a ~20 KB verifier dump inline in a WARN message that gets truncated by log-collectors and drowns readable text.

**Why this priority**: Nice-to-have UX; doesn't affect the primary success path but reduces operator frustration when the fix is incomplete on some kernel version. Deferrable to a follow-up if it slips scope.

**Independent Test**: Simulate verifier rejection (test-only feature flag or intentional injection). Assert the emitted log is under 500 bytes, mentions "vfs_open", "verifier rejected", and includes a stable link/reference to a diagnostic doc.

**Acceptance Scenarios**:

1. **Given** a kernel that still rejects the fixed vfs_open kprobe, **When** the trace starts, **Then** mikebom emits a single WARN line under 500 bytes summarizing "vfs_open unavailable on this kernel; file_access will be empty" with a reference to `docs/architecture/attestations.md#file-op-tracing-gaps`.
2. **Given** the same scenario, **When** the attestation is emitted, **Then** the attestation's `trace_integrity.kprobe_attach_failures[]` array contains an entry for `vfs_open` (already existing field — verify it's being populated).

---

### Edge Cases

- **What happens when the traced command touches thousands of files?** The 128 MB `FILE_EVENTS` ring buffer must not overflow silently. Overflow should surface via `trace_integrity.ring_buffer_overflows` (already-existing counter).
- **What happens on kernels that lack `vfs_open` as a kprobe target?** (Some hardened kernel configs mark kernel symbols non-kprobable.) mikebom must fall back to `do_filp_open` (already implemented alongside vfs_open per `file_ops.rs`) and log ONCE, not per-event.
- **What happens when a traced process opens `/dev/urandom` a million times?** Kernel dedup via `SEEN_HASHES` bloom filter (already exists at `maps.rs:36`) should collapse duplicates; verifier must accept the vfs_open code path that consults the bloom filter.
- **What happens when a file path exceeds 256 bytes?** The `FileEvent.path: [u8; 256]` field truncates; `path_truncated` flag should be set. Verify this still works post-fix.
- **What happens on the earlier amd64 kernel where the code presumably worked?** The fix must not regress kernels that already accept the current bytecode.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: mikebom's `vfs_open` AND `do_filp_open` kprobes MUST load successfully on Colima Linux 6.8 aarch64 kernels (dev environment) AND Ubuntu 22.04/24.04 amd64 Linux 6.5+ kernels (CI environment) without WARN-level messages from the loader about `BPF_PROG_LOAD` verifier rejection. Older kernels (5.15 LTS, 5.10, etc.) remain best-effort: the loader surfaces failures via `trace_integrity.kprobe_attach_failures[]` (per FR-009) but the trace itself continues without a fatal error.
- **FR-002**: When the trace is invoked against a command that reads at least one file, the emitted attestation's `predicate.file_access.operations[]` array MUST contain at least one entry referencing that file's fully-resolved path.
- **FR-003**: The fix MUST NOT change the wire shape of `FileEvent` (userspace serialization + attestation JSON schema stay byte-identical to pre-fix consumers of the same feature-flagged build).
- **FR-004**: The fix MUST NOT regress on amd64 Linux 6.6+ kernels — vfs_open kprobe must continue to load cleanly on those platforms.
- **FR-005**: The fix MUST preserve every existing runtime feature: kernel-side path resolution via `bpf_d_path`, deduplication via `SEEN_HASHES` bloom filter, PID-filtering via `PID_FILTER`, and cost gating via `should_trace()`.
- **FR-006**: When both `vfs_open` and `do_filp_open` kprobes attach cleanly, mikebom MUST NOT double-emit file events for the same open (dedup already handled downstream, but the fix must not exacerbate).
- **FR-007**: The fix MUST land on the trace critical path — file events observed during a traced build under `--features ebpf-tracing` must reach userspace within the same trace window (no batching delays > 100 ms beyond current behavior).
- **FR-008**: When a verifier rejection persists on some kernel post-fix, mikebom MUST log a single WARN line under 500 bytes (instead of the current ~20 KB verifier dump inline in the log line).
- **FR-009**: The `trace_integrity.kprobe_attach_failures[]` field in the emitted attestation MUST include an entry for any kprobe that failed to attach — regardless of whether it's `vfs_open`, `do_filp_open`, or a future addition.

### Key Entities

- **FileEvent** (existing type in `mikebom-common/src/events.rs`): the wire record carrying one file-open observation. Fields include `event_type`, `timestamp_ns`, `pid`, `tid`, `comm`, `path: [u8; 256]`, `path_truncated`, `flags`, `bytes_transferred`, `content_hash: [u8; 32]`, `inode`. Shape stays unchanged per FR-003.
- **FILE_EVENTS** (existing map at `mikebom-ebpf/src/maps.rs:19`): 128 MB ring buffer that the vfs_open kprobe writes into. Sized for high-throughput builds; capacity unchanged.
- **kprobe_attach_failures** (existing field on `TraceIntegrity`): array of kprobe names that failed to attach at trace start. Post-fix, `vfs_open` should be absent from this list on supported kernels.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On Colima Linux 6.8 aarch64 via the milestone-210 container harness (`Dockerfile.ebpf-test` + `scripts/ebpf-integration-test.sh`), running the trace against the SC-001 fixture produces an attestation where `.predicate.file_access.operations | length > 100` (a `cargo build --release` for 4 tiny crates opens at least ~100 files: sources, `.rlib`s, target/incremental, /etc/, /proc/, etc.).
- **SC-002**: Zero `WARN`-level log lines about vfs_open kprobe attachment failure during a trace startup on supported kernels.
- **SC-003**: When followed by `mikebom sbom generate --attestation <trace> --path <fixture>`, at least 50 % of `pkg:cargo/*` components in the generated SBOM carry a `mikebom:source-read-set` property with a non-empty `read_set` array. (Threshold accounts for components whose write_sets don't intersect any captured invocation's file path — e.g. transitive deps that the fixture uses without recompiling.)
- **SC-004**: On amd64 Linux 6.6+ kernels, an existing successful trace + emit workflow (pre-fix) MUST continue to succeed byte-identically on the attestation's non-file-op fields — `network_trace`, `compiler_pipeline`, `trace_integrity` (excluding the removed kprobe_attach_failure entry) all serialize identically.
- **SC-005**: When operator runs `mikebom trace run` on a kernel where the fix still rejects (aarch64 kernel older than 6.4, or hardened config), the total log volume from mikebom's own tracing is under 5 KB (vs. current ~20 KB from the single verifier dump).

## Assumptions

- The verifier rejection is caused by the same class of issue as milestone 210's `sched_process_exec` rejection: LLVM eBPF backend emits a bounded byte-store loop for large fixed-size zero-init (`[u8; 256] path` initialization in the FileEvent construction, plus `content_hash: [u8; 32]`, `comm: [u8; 16]`, and other fields). The verifier expands the loop into state-tracked branches, blowing the 1 M-instruction budget on stricter kernels.
- The fix pattern is analogous: shrink fixed-size fields where possible, use word-wide stores instead of byte-wise assignment, avoid slice-of-slice patterns anywhere in the hot path. Milestone 210's post-mortem in `feedback_ebpf_container_test_gap.md` covers the recipe.
- The fix does NOT require BPF CO-RE or nightly-only aya-ebpf features — same posture as m210 (aya-ebpf 0.1.1, bpf-linker, nightly for the eBPF target only).
- `do_filp_open` is IN scope for this milestone per Clarification Q2 — its own 328-byte FileEvent construction is functionally identical to vfs_open's, so whichever verifier-friendly pattern fixes vfs_open should apply verbatim. Fixing both in one PR avoids duplicating the entire container-test cycle for near-identical work. If in the course of implementation `do_filp_open` turns out to require a materially different fix pattern, the plan-phase artifacts document the divergence and the implementation may split into two commits under this same milestone (not a scope-drop).
- Container harness from milestone 210 (`Dockerfile.ebpf-test` + `scripts/ebpf-integration-test.sh`) is the verification substrate. No new harness infrastructure needed.
- amd64 Linux 6.6+ regression validation happens in CI (existing ubuntu-latest runner) — not requiring a manual amd64 test on the developer's Mac.
- The compose-stack log-noise problem from milestone 210's testing (need to truncate `/var/lib/docker/containers/*/json-log` periodically to keep Colima disk free) recurs; operator handles via the earlier `/tmp/free-colima.sh` script. Not this spec's responsibility.
