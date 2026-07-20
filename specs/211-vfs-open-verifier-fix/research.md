# Research: vfs_open kprobe verifier-fix patterns

**Milestone**: 211
**Date**: 2026-07-20
**Status**: Phase 0 complete

## R1 — Verifier failure attribution

**Question**: Which specific instruction pattern in `try_vfs_open` fails the aarch64 Linux 6.8 eBPF verifier?

**Decision**: The failing pattern is the eager 256-byte `[0u8; 256]` zero-init of the reserved ring-buffer entry's `path` field BEFORE the `bpf_d_path` helper call overwrites it. LLVM's eBPF backend lowers the array assignment to a bounded byte-store loop (~256 iterations of `st_byte + branch + index-increment`); the verifier symbolically tracks each iteration as a state fork, exploding past its 1 M-instruction budget. Milestone-210's `sched_process_exec` rejection at commit `5a3cdc6` showed the identical failure mode with a smaller (128-byte) buffer — the ~30 KB verifier dump had `147: (73) *(u8 *)(r5 +0) = r2` loop instructions dominating the trace.

**Rationale**:
- The zero-init is **semantically dead**: `bpf_d_path` (line 244) writes over `event.path[..n]` immediately afterward, so the pre-init bytes never surface to userspace. Removing the zero-init loses nothing.
- The other kprobes in `file_ops.rs` (`vfs_read`, `vfs_write`, `do_filp_open`, `do_sys_openat2`) share the same FileEvent shape but were not observed WARN-ing on Colima. That's because they zero-init a STACK-local `let path = [0u8; 256]` before copying into the event — the stack alloc is inline-unrollable enough for the verifier when there's no `bpf_d_path` helper immediately after. `vfs_open`'s combined "zero-init 256 bytes + call `bpf_d_path` (helper with its own verifier machinery) + branch on returned n" was the specific combination that tipped the state-fork explosion past budget. Independent verification available at run-time by removing ONLY the pre-init and re-testing.

**Alternatives considered**:
- **Alt A: shrink `path` field from 256 → 128 bytes.** REJECTED. Violates FR-003 (wire shape unchanged). Also loses ~half of typical Linux paths (e.g., `~/.cargo/registry/src/index.crates.io-.../my-crate-1.2.3/src/foo.rs` is ~90+ bytes; wrappers can push beyond).
- **Alt B: use `core::ptr::write_bytes` (memset intrinsic).** DEFERRED. LLVM's eBPF backend MAY lower this to inline stores rather than the loop pattern — this was our first-choice fix in m210 for `argv0_hint` before we switched to shrinking. Worth testing as a fallback if simply removing the zero-init proves insufficient.
- **Alt C: split the write across multiple ring-buffer reservations.** REJECTED. Complexity-heavy; doesn't address the root cause.
- **Alt D: use `bpf_get_current_task_btf()` + CO-RE to walk `struct path` manually instead of `bpf_d_path`.** REJECTED. Requires BPF CO-RE machinery mikebom deliberately avoids per Assumptions (aya-ebpf 0.1.1 has limited CO-RE support), and `bpf_d_path` is functionally correct — the issue is only the surrounding init pattern.

## R2 — Zero-init strategy for the ring-buffer entry

**Question**: After removing the eager `path` zero-init, what happens to the OTHER field-inits and their verifier interaction?

**Decision**: Rely on the **BPF ringbuf-reserve zero-fill contract** — `RingBuf::reserve<T>` on aya-ebpf allocates zeroed memory. Fields we don't explicitly overwrite remain zero. This means:
- `event_type`, `timestamp_ns`, `pid`, `tid`, `comm`, `flags`, `bytes_transferred`, `inode` — explicitly overwritten (kept as-is).
- `path_truncated`, `_path_padding`, `content_hash`, `path` — REMOVE the explicit `= 0` / `= [0u8; 256]` / `= [0; 32]` assignments; rely on ringbuf zero-fill.
- After `bpf_d_path` writes `n` bytes into `path`, set `path_truncated = 1` iff `n == 256` (indicates the path was longer than the buffer; the trailing byte is a NUL that may cut mid-character).

**Rationale**:
- The aya-ebpf `RingBuf::reserve` contract (verified in aya-ebpf source at 0.1.1) guarantees zeroed memory on success. Explicit `= 0` assignments are defensive-but-redundant AND expensive in verifier-instruction budget.
- On modern kernels (6.4+), the verifier can prove that unwritten ringbuf bytes are zero from the reserve helper's post-state.
- Reduces the per-invocation zero-init cost from ~300 bytes (path 256 + content_hash 32 + smaller fields ~12) to ZERO explicit zero-init instructions.

**Alternatives considered**:
- **Alt A: keep explicit inits, apply `core::ptr::write_bytes` on `path` only.** DEFERRED as fallback per R1 Alt B.
- **Alt B: `unsafe { core::ptr::write_bytes(event as *mut u8, 0, size_of::<FileEvent>()) }` once at the top.** REJECTED. Still emits a `~size_of<FileEvent>`-byte memset — the whole point of not zero-initing is to avoid this cost.

## R3 — FR-003 wire-shape verification

**Question**: Does the fix change the wire shape of `FileEvent` as serialized in the emitted attestation JSON?

**Decision**: No changes to `mikebom-common::events::FileEvent`. Kernel-side field-init ordering changes but the struct's `#[repr(C)]` layout is untouched. Userspace deserializes the same byte sequence into the same fields. JSON serialization is unaffected.

**Rationale**:
- The verifier fix is **behavioral** (removing dead-write instructions) not **structural** (adding/removing/resizing fields).
- Regression guard: golden-diff two attestations from adjacent commits (last-known-good `sched_process_exec` fix at `5a3cdc6` vs. post-m211 fix). Field names + types identical; only file-op OPERATION COUNT changes (from 0 to non-zero).

**Alternatives considered**:
- **Alt A: bump `FileEvent` version + emit both shapes.** REJECTED as premature — no consumer surface breaks with pure field-init reordering.

## R4 — Runtime-time verification strategy

**Question**: How do we prove the fix works before merging?

**Decision**: Reuse the milestone-210 `Dockerfile.ebpf-test` + `scripts/ebpf-integration-test.sh` container harness, extended with two new jq assertions:
1. `.predicate.file_access.operations | length > 100` (per SC-001)
2. On kernels where the fix succeeds, `.predicate.trace_integrity.kprobe_attach_failures | contains(["vfs_open"]) | not` (per SC-002)

Plus a companion test asserting the milestone-210 SC-001-fixture SBOM now carries non-empty `mikebom:source-read-set` payloads on at least 50 % of `pkg:cargo/*` components (per SC-003) — this validates the end-to-end path from vfs_open capture → aggregator → C130 emission.

**Rationale**:
- Container harness proven at m210; iteration time is ~10 min per docker rebuild.
- macOS `cargo check --all-targets` is systematically blind to `#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]`-gated code per the `feedback_ebpf_container_test_gap` memory. Container harness is the ONLY reliable pre-merge signal.
- SC-003 assertion is the actual USER-VISIBLE regression guard: proves the fix cascades correctly into m210's emission code path.

**Alternatives considered**:
- **Alt A: emulate the verifier via `bpftool prog load` in userspace CI.** REJECTED — requires kernel-image-with-BTF matching production, more complex than the container harness we already have.
- **Alt B: mock the eBPF program with a userspace shim.** REJECTED — doesn't test the actual verifier path, which is the entire point.

## R5 — CI regression coverage on amd64

**Question**: How do we ensure the fix doesn't regress the amd64 platform where the pre-fix code presumably worked?

**Decision**: The existing `.github/workflows/ci.yml` `lint-and-test-ebpf` job runs on `ubuntu-latest` (amd64) with `--features ebpf-tracing` enabled. Extend that job to invoke the container harness (`docker build -f Dockerfile.ebpf-test .` + `docker run --rm --privileged -v /sys/kernel/debug:/sys/kernel/debug mikebom-ebpf-test`) as a post-`cargo test` step. Assert exit code 0 + non-zero `file_access.operations` count.

**Rationale**:
- GitHub Actions ubuntu-latest currently runs Linux 6.5+ kernels (ubuntu-24.04 as of 2026-07). Container's `--privileged` mode + host-kernel access covers both eBPF loading + verifier acceptance on amd64.
- No new CI matrix needed; the existing amd64 job is sufficient.
- Post-merge, m211 becomes the canary for future eBPF-verifier regressions on amd64 kernels the same way m210 was for aarch64.

**Alternatives considered**:
- **Alt A: add a matrix over kernel versions (5.15, 6.1, 6.5, 6.8).** REJECTED for MVP — the operator target matrix per Clarification Q1 is only 6.5+ amd64 + 6.8 aarch64. Matrix-over-older-kernels is best-effort per FR-001 and doesn't warrant CI investment yet.
- **Alt B: manual amd64 verification on the developer's Mac.** REJECTED — mikebom dev happens on macOS; no native amd64 Linux target.

## R6 — Rate-limited WARN when kprobe still rejects (FR-008)

**Question**: How to reduce the ~20 KB verifier-dump WARN inline in the loader log to <500 bytes per FR-008?

**Decision**: In `mikebom-cli/src/trace/loader.rs::attach_kprobe`, when the aya `attach()` call returns an error, log a summary line with the kprobe name + terminal verifier error string only (extract the last `verification time NNN usec` / `processed NNN insns` line + the immediately-preceding rejection reason). Skip the ~20 KB of intermediate register-state dumps. Downstream operators can enable `RUST_LOG=aya=debug` to get the full dump if needed.

**Rationale**:
- The current implementation calls `format!("{e}")` on the aya error, which returns the entire verifier trace unstructured. Truncating to the tail is safe because the actual rejection reason is always at the tail (verifier is a forward-analysis pass; the failure line is where analysis terminated).
- `RUST_LOG=aya=debug` escape hatch preserves debuggability for developers.
- 500-byte limit is easy to enforce via `text.chars().take(500).collect()` on the summary line.

**Alternatives considered**:
- **Alt A: parse the verifier output structurally (regex the failure line + include just that).** REJECTED — brittle across kernel versions; ~20 KB tail-strip is more robust.
- **Alt B: write the full dump to `/tmp/mikebom-verifier-<timestamp>.txt` + log a pointer to the file.** DEFERRED as post-MVP UX polish — the tail-truncated summary is enough for most operators.

## R7 — do_filp_open verifier-fix generalizability

**Question**: Does the same pattern from R1 (remove eager zero-init before ringbuf-write) apply to `do_filp_open`?

**Decision**: Almost certainly yes — but with a nuance. `do_filp_open`'s pattern differs from `vfs_open`: it zero-inits a STACK-LOCAL `let mut path = [0u8; 256];`, then reads via `bpf_probe_read_kernel_str_bytes`, then copies stack→event via `(*event).path = path`. The verifier issue may or may not surface here since the stack-local memzero is a different pattern.

If `do_filp_open` continues to load cleanly post-vfs_open-fix, keep it unchanged (Constitution Principle IX — accuracy over premature refactoring). If the container test surfaces a verifier rejection on `do_filp_open` after the vfs_open fix, apply the same "rely on ringbuf-reserve zero-fill" pattern: skip the stack-local `path` buffer entirely, read `bpf_probe_read_kernel_str_bytes` directly into `(*event).path`.

**Rationale**:
- Empirical observation from milestone 210: `do_filp_open` was NOT among the WARN-ing programs during our 6-attempt debug cycle. Only `vfs_open` (and `sched_process_exec`, which we fixed) were rejected on aarch64. So the topologies differ enough that we shouldn't blanket-refactor.
- Per Clarification Q2, the SPEC scope INCLUDES `do_filp_open` in the acceptance criteria. This just means we VERIFY it continues to load (US-1b acceptance scenario) — it doesn't automatically mean we CHANGE it.

**Alternatives considered**:
- **Alt A: preemptively rewrite `do_filp_open` with the same pattern as `vfs_open`.** REJECTED per rationale above — premature refactoring without a verifier signal to justify.

## R8 — Path-truncation semantics

**Question**: Post-fix, when `bpf_d_path` returns `n == 256` (buffer full), how do we distinguish "path was exactly 256 bytes" from "path was longer than 256 bytes and got truncated"?

**Decision**: Set `path_truncated = 1` iff `n == 256`. Userspace consumers treating `path_truncated` as a signal MAY have false positives on paths that happen to be exactly 256 bytes long. Post-fix, if operator surfaces the false-positive rate as a real concern, extend the field to `path_truncated: u32` and encode `256` as "unknown" vs `<256` as "not truncated" — but that's a wire-shape change and FR-003 blocks it.

**Rationale**:
- Current code sets `path_truncated = 0` unconditionally, meaning ANY truncated path shows as non-truncated. Post-fix, we conservatively flag paths that fill the buffer as potentially truncated.
- Consumer-facing false-positive rate: paths exactly 256 bytes are rare in typical Linux filesystems (max PATH_MAX is 4096 but typical paths are <100 bytes). Acceptable trade-off.

**Alternatives considered**:
- **Alt A: use `bpf_d_path`'s error return semantics to distinguish.** REJECTED — `bpf_d_path` returns byte count only; it doesn't have a "truncated" signal.
