# Implementation Plan: Fix vfs_open + do_filp_open eBPF verifier rejection

**Branch**: `211-vfs-open-verifier-fix` | **Date**: 2026-07-20 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/211-vfs-open-verifier-fix/spec.md`

## Summary

Two kprobes in `mikebom-ebpf/src/programs/file_ops.rs` (`vfs_open_entry` per spec + `do_filp_open_entry` per Clarification Q2) fail the aarch64 eBPF verifier at `BPF_PROG_LOAD` time on Colima Linux 6.8. When they fail, `predicate.file_access.operations[]` stays empty on every emitted attestation — silently dropping the entire file-op dataset that milestone 210's `mikebom:source-read-set` (C130) needs to populate. Fix pattern is analogous to milestone-210's `sched_process_exec` rewrite (`5a3cdc6`): eliminate large fixed-size zero-init loops the verifier can't reason about efficiently, replace with either `bpf_d_path`-writes-directly-into-reserved-buffer semantics or word-wide stores instead of byte-wise assignment. Preserve the existing FileEvent wire shape (FR-003) and amd64 6.5+ compatibility (FR-004). Verified via the milestone-210 `Dockerfile.ebpf-test` + `scripts/ebpf-integration-test.sh` harness against the SC-001 fixture.

## Technical Context

**Language/Version**: Rust nightly (eBPF target via `aya-ebpf`) + Rust stable (workspace toolchain inherited from milestones 001–210; no new nightly features required beyond what m020 pinned).

**Primary Dependencies**: Existing only — `aya-ebpf = 0.1.1` (kernel-side; `helpers::{bpf_d_path, bpf_probe_read_kernel, bpf_probe_read_kernel_str_bytes, bpf_probe_read_user_str_bytes, bpf_ktime_get_ns}`), `aya = 0.13` (user-space eBPF loader; already at workspace); `mikebom-common` `FileEvent` type (unchanged per FR-003). **No new Cargo dependencies.** No changes to Rust MSRV. No changes to `bpf-linker` version pin.

**Storage**: N/A — in-process per scan; file events flow via the existing `FILE_EVENTS` ring buffer (`mikebom-ebpf/src/maps.rs:19`, 128 MB capacity, unchanged).

**Testing**:
- Unit tests: none — eBPF program bytecode isn't unit-testable in mikebom's stable-Rust workspace (aya-ebpf targets `bpfel-unknown-none`, non-hostable).
- Container integration test: `Dockerfile.ebpf-test` + `scripts/ebpf-integration-test.sh` (from milestone 210). Reused verbatim; the harness's existing assertions cover both kprobe attachment + attestation-content checks.
- Regression guard: extend `scripts/ebpf-integration-test.sh` with two new assertions matching SC-001 (`file_access.operations | length > 100`) + SC-003 (`mikebom:source-read-set` populates ≥50% of `pkg:cargo/*` components).
- CI test coverage: the ebpf-tracing feature-flagged CI lane (`lint-and-test-ebpf` job per `.github/workflows/ci.yml`) already runs on ubuntu-latest amd64. Confirm the container harness runs there OR add a manual amd64 verification step.

**Target Platform**: Linux only (eBPF is Linux-native). Per FR-001 clarification: Colima aarch64 Linux 6.8 (dev) + Ubuntu 22.04/24.04 amd64 Linux 6.5+ (CI). Older kernels (5.15 LTS, 5.10) are best-effort with `kprobe_attach_failures` surfacing.

**Project Type**: eBPF-instrumented CLI tool. Three-crate architecture (Principle VI): `mikebom-ebpf` (kernel-side, nightly + bpf-linker) is the only crate that changes structurally; `mikebom-cli` may need loader-side warning-rate-limiting for FR-008; `mikebom-common` `FileEvent` shape stays frozen.

**Performance Goals**: FR-007 — file events reach userspace within the same trace window with no batching delay > 100 ms beyond current behavior. The kprobe fires on EVERY file-open syscall on the host, so per-event cost matters — the fix must not add >200 ns per invocation on the hot path.

**Constraints**:
- **eBPF verifier**: 1 M-instruction budget per program (kernel default). Current vfs_open bytecode blows this budget on the aarch64 6.8 verifier via a memzero loop that gets expanded into state-tracked branches. The fix must fit well under the budget on BOTH Colima aarch64 6.8 AND Ubuntu 22.04+ amd64 6.5+ kernels.
- **eBPF stack limit**: 512 bytes per program frame. Current `let path = [0u8; 256]` stack allocations consume 256 bytes of that; any fix that adds another 256-byte stack alloc is a dead end.
- **Constitution Principle II (eBPF-Only Observation)**: no LD_PRELOAD, no ptrace, no `/proc` polling. The fix must stay in-kernel.
- **Wire-shape**: FR-003 locks the JSON serialization of `FileEvent`. Internal kernel-side struct MAY change but userspace serialization output MUST NOT (byte-identity for consumers of the attestation JSON is preserved).

**Scale/Scope**:
- Two eBPF programs modified (`vfs_open_entry` + `do_filp_open_entry`).
- Potentially one shared helper extracted (verifier-friendly `emit_file_open_event` inline function).
- One test-harness assertion extension (`scripts/ebpf-integration-test.sh`).
- One documentation touch (`docs/architecture/attestations.md` — new subsection on file-op-tracing verifier hardening, cross-referencing m210's pattern).
- Total estimated diff: 100–300 LOC in `mikebom-ebpf`, ~30 LOC in the harness, ~50 LOC docs.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C**: ✅ No C. Fix stays in Rust + eBPF-bytecode (aya-ebpf emits bytecode from Rust via bpf-linker; no C toolchain touched).
- **II. eBPF-Only Observation**: ✅ CANONICAL principle — this fix IS eBPF hardening for file-op observation. No new observation mechanism introduced.
- **III. Fail Closed**: ✅ Fix preserves the existing fail-mode: if the fixed kprobe still rejects (older kernel), the trace continues with `kprobe_attach_failures` surfacing the degradation — the operator SEES that file-op capture is unavailable rather than silently getting an incomplete SBOM. Enforced by FR-008 (500-byte WARN) + FR-009 (kprobe_attach_failures reporting) + SC-005 (log volume gate).
- **IV. Type-Driven Correctness**: ✅ `FileEvent` type stays unchanged (FR-003). If we introduce a scratch struct for kernel-internal use, it stays `#[repr(C)]`.
- **V. Specification Compliance**: ✅ `bpf_d_path` semantics (BPF helper allowlist), `bpf_probe_read_kernel_str_bytes` semantics, and eBPF verifier constraints are the "spec" this fix complies with. Documented per-program.
- **VI. Three-Crate Architecture**: ✅ Only `mikebom-ebpf` changes structurally. `mikebom-cli`'s loader may gain a WARN-rate-limiter (FR-008) but doesn't change crate structure. `mikebom-common` FileEvent shape frozen.
- **VII. Test Isolation**: ✅ Container harness runs in `--privileged` Docker isolated from host; existing test-fixture stay-set applies. Regression tests reuse `two_binaries_diverge` fixture from milestone 210.
- **VIII. Completeness**: ✅ Fix ADDS coverage (previously empty `file_access.operations[]` → populated). No feature dropped.
- **IX. Accuracy**: ✅ Fix produces MORE accurate SBOMs (real source-read-set attribution vs. always-"unknown"). No fabrication introduced.
- **X. Transparency**: ✅ FR-008 (500-byte WARN when fix rejects) + FR-009 (kprobe_attach_failures reporting) are the transparency signals. Consumers can distinguish "no file activity because it was a hermetic build" from "no file activity because our kprobe failed."
- **XI. Enrichment**: N/A — no enrichment layer touched.
- **XII. External Data Source Enrichment**: N/A — no external sources touched.

**Strict Boundaries**: no violations detected. eBPF stays in `mikebom-ebpf`. Only `mikebom-common::events::FileEvent` is a cross-crate shared type; frozen per FR-003.

**Verdict**: ✅ Constitution check passes; no violations to justify. Proceed to Phase 0.

## Project Structure

### Documentation (this feature)

```text
specs/211-vfs-open-verifier-fix/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output — fix-pattern analysis + verifier-error attribution + CI-strategy
├── data-model.md        # Phase 1 output — FileEvent frozen shape + kernel-internal scratch structs
├── quickstart.md        # Phase 1 output — end-to-end verification recipe (container harness invocation + assertions)
├── contracts/
│   ├── verifier-acceptance.md  # Per-kprobe verifier-acceptance contract (both programs pass + specific rejection modes to surface if not)
│   └── attestation-shape.md    # Cross-references m210's attestation contract — asserts FileEvent JSON stays byte-identical
└── tasks.md             # Phase 2 output (/speckit.tasks command — NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
mikebom-ebpf/
└── src/
    └── programs/
        └── file_ops.rs           # PRIMARY EDIT SURFACE — vfs_open_entry + do_filp_open_entry
                                    #   Possibly extract a shared verifier-friendly `emit_file_open`
                                    #   helper if the fix pattern generalizes cleanly.

mikebom-common/
└── src/
    └── events.rs                 # FROZEN per FR-003 — FileEvent shape must NOT change

mikebom-cli/
└── src/
    └── trace/
        └── loader.rs             # Possibly touched — FR-008 rate-limit the WARN to ≤500 B
                                    # when a kprobe fails to attach (strip verifier-dump body)

docs/
└── architecture/
    └── attestations.md           # New subsection "File-op tracing verifier gaps" — cross-refs to
                                    # feedback_ebpf_container_test_gap.md pattern recipe

scripts/
└── ebpf-integration-test.sh     # Extended assertions (SC-001 file_access count + SC-003 C130 populate)

Dockerfile.ebpf-test              # Reused verbatim from m210 — no changes anticipated
```

**Structure Decision**: single-crate edit primarily (`mikebom-ebpf/src/programs/file_ops.rs`); this is a bug fix + verifier-hardening, not a new milestone-scale feature.  No new crates, no new modules within existing crates. Companion touches (loader rate-limiting, docs, test harness) are pure additions to existing files. The three-crate architecture stays intact.

## Complexity Tracking

> No Constitution violations. Complexity tracking section unused.
