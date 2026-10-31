# Implementation Plan: Capture that survives a kernel change

**Branch**: `1270-stable-probe-tracepoints` | **Date**: 2026-10-29 |
**Spec**: [spec.md](./spec.md)
**Input**: Feature specification from
`/specs/1270-stable-probe-tracepoints/spec.md`

## Summary

Two kprobes on internal kernel functions carry this project's file-open
capture, and one carries its network capture. On GitHub's `ubuntu-24.04`
image `20261025.353` the open pair stopped attaching: file-open events
for an identical fixture build fell from 902 to 443, and the attestation
reported itself complete anyway. #1310 made that loss visible. This
feature removes it, by moving capture onto syscall tracepoints, whose
names and argument layouts are UAPI rather than internal symbols.

The approach is not novel in this codebase — it is m1070's, applied to
opens. `waybill-ebpf/src/programs/fs_syscalls.rs` already pairs syscall
enter and exit tracepoints, holds arguments per thread, reads strings at
exit into ring-buffer memory, and reports a call only when it succeeded.
The open join is that pattern with a different payload, which is also
what makes the clarified narrowing to successful opens cheap: the result
is already in hand at the moment the kernel decides whether to emit.

Three consequences land with it, all decided in clarification and each
carrying its own verification:

- **Read sets shrink** by the open requests that failed, which they
  carry today and should never have. Every removed entry must be
  attributable to a failed request, checked against a kernel-side
  discard record that names the path.
- **IPv6 connections become visible.** The current capture reaches only
  IPv4 by construction; `IpAddr` already carries 16 octets, so this is a
  coverage change with no wire change.
- **The old kprobes are deleted, not kept as a fallback**, along with
  the `attach_kprobe` helper — which is what makes "no capture depends
  on an internal kernel name" a structural property rather than a
  review habit.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain pinned by `rust-toolchain.toml`, 1.98.1) for user space, plus the dated nightly in `waybill-ebpf/rust-toolchain.toml` for the kernel-side crate. No new toolchain features.
**Primary Dependencies**: Existing only — `aya` 0.13 (`TracePoint::attach`, `PerCpuArray` reads), `aya-ebpf` 0.1.1 (`#[tracepoint]`, `TracePointContext::read_at`, `HashMap`, `RingBuf`, `bpf_probe_read_user_str_bytes`), `serde`/`serde_json`, `tracing`, `anyhow`. **Zero new Cargo dependencies.**
**Storage**: N/A — per-thread pending entries live in a kernel hash map for one syscall, counters are per-CPU u64 slots read once at trace end, and nothing persists beyond the emitted attestation.
**Testing**: `cargo test --workspace --no-fail-fast --features
ebpf-tracing` for the unit and loader tests; the container harness
(`Dockerfile.ebpf-test` + `scripts/ebpf-integration-test.sh`) for the
end-to-end assertions, run in the `lint-and-test-ebpf` lane and locally
under Colima.
**Target Platform**: Linux, x86_64 and aarch64. The open family differs
between them — measured, not assumed.
**Project Type**: CLI with a kernel-side eBPF component; three-crate
workspace (Principle VI).
**Performance Goals**: Ring-buffer event volume for the fixture build
must not exceed today's, and `ring_buffer_overflows` must stay 0. Both
reductions in R8 point the same way, so this is a ceiling the design
expects to beat rather than a budget it spends.
**Constraints**: The host kernel is not under the project's control and
cannot be pinned; a container shares it, so containerising buys no
insulation. A pinned-kernel VM would, and is deliberately out of scope.
**Scale/Scope**: up to 9 kernel-side programs added — five unconditional
(the `openat` and `openat2` pairs plus `sys_enter_connect`) and four
pending the x86_64 inventory (the `open` and `creat` pairs) — and 4
removed; one loader attach sequence rewritten; one new attestation
field. The measurements owed are named in `measurements/findings.md`.

## Constitution Check

*GATE: must pass before Phase 0. Re-checked after Phase 1 — see the
bottom of this section.*

| Principle | Assessment |
|---|---|
| **II. eBPF-Only Observation** | PASS. Capture stays entirely in eBPF; a syscall tracepoint is a kernel-side eBPF attachment, not a new dependency source. No lockfile, manifest or external source becomes a dependency source. **Wording note below.** |
| **III. Fail Closed** | PASS, and strengthened. Loss of all open capture stops the trace (FR-007); loss of connect capture degrades and is reported (FR-008). The feature's whole subject is a case where the tool did *not* fail closed. |
| **IV. Type-Driven Correctness** | PASS. `PendingOpen` is a `#[repr(C)]` struct, not a tuple of raw integers. Kernel-side code uses `unwrap_or` with explicit fallbacks, as the existing programs do — `.unwrap()` appears nowhere. One `unwrap_or(0)` is called out in research R4 as a trap rather than a convenience. |
| **VI. Three-Crate Architecture** | PASS. Shapes shared between kernel and user space stay in `waybill-common`; the pending map stays in `waybill-ebpf`, beside `PendingRename`. |
| **VII. Test Isolation** | PASS. No new env-var mutation; the container harness is already isolated. |
| **VIII. Completeness** | PASS, with the one objection worth stating. Removing failed opens from a read set is not a completeness regression: a failed open read nothing, so no dependency leaves the output. The feature *increases* completeness on two axes — every open on a kernel that lost `do_filp_open`, and every IPv6 connection. Where the result of an open cannot be determined, FR-004b degrades to requests and records the gap rather than guessing. |
| **IX. Accuracy** | PASS, and the point of the feature. A failed open in a read set is a false positive by the set's own claim. |
| **X. Transparency** | PASS. Attach failures reach `kprobe_attach_failures[]` and degrade completeness (#1310); discarded failed opens are counted into `TraceIntegrity`; the FR-004b fallback is recorded as a gap. |
| **Pre-PR gate** | Both commands, no `+toolchain`, via `scripts/gate.sh` with `--expect`. |

**Wording note on Principle II, non-blocking.** The principle's
normative sentence is "all dependency discovery MUST occur through eBPF
tracing of live build processes", which this feature satisfies
unchanged. Two following sentences are descriptive, and one says "File
operations are traced via kernel probes." After this feature they are
traced via syscall tracepoints. Nothing normative is diluted — the
mechanism is still kernel-side eBPF — but the descriptive clause will
be stale, and a reader could mistake it for a requirement to use
kprobes specifically. That is a constitution text refresh, separate from
this feature and filed as such; it is not a licence to reinterpret a
MUST, because no MUST is in question.

**Post-Phase-1 re-check**: unchanged. The design introduces no new
dependency source, no fallback to static analysis, no new external
service, and no silent degradation path — FR-004b is the one
degradation and it is required to record itself.

## Project Structure

### Documentation (this feature)

```text
specs/1270-stable-probe-tracepoints/
├── plan.md                 # This file
├── spec.md                 # Feature specification (clarified)
├── research.md             # Phase 0 — R1..R10
├── data-model.md           # Phase 1 — maps, event kinds, attestation field
├── quickstart.md           # Phase 1 — the dev loop, and how not to fool it
├── contracts/
│   ├── probe-attachment.md # What attaches; what a failure means
│   └── open-join.md        # The request/result join; the discard record
├── measurements/
│   ├── tracepoint-inventory.sh  # The probe, committed so it can be re-run
│   ├── baseline-before.md       # Pre-change counts and sets; SC-008's ceiling
│   └── findings.md              # Run 1 (aarch64); verdicts; what is owed
├── checklists/
│   └── requirements.md     # Spec quality checklist
└── tasks.md                # Phase 2 — NOT created by /speckit.plan
```

### Source Code (repository root)

```text
waybill-ebpf/src/
├── maps.rs                       # + PendingOpen, PENDING_OPEN,
│                                 #   OPEN_FAILURES_DISCARDED, OPEN_DIAG
├── main.rs                       # program re-exports: 4 removed, ~9 added
└── programs/
    ├── file_ops.rs               # − openat2_entry, − do_filp_open_entry
    │                             #   (vfs_read/vfs_write kprobes stay,
    │                             #    unattached and out of scope)
    ├── fs_syscalls.rs            # the join template; ARG*/RET constants
    ├── open_syscalls.rs          # NEW — the open enter/exit join
    ├── tcp_connect.rs            # − tcp_connect, − tcp_connect_ret;
    │                             #   + sys_enter_connect, both families
    └── compiler_exec.rs          # unchanged (sched_* tracepoints)

waybill-common/src/
├── events.rs                     # + FileEventType::OpenFailed = 8
├── ip.rs                         # unchanged — already carries IPv6
└── attestation/integrity.rs      # + open_failures_discarded (last field)

waybill-cli/src/trace/
├── loader.rs                     # attach sequence rewritten;
│                                 #   attach_kprobe DELETED
├── aggregator.rs                 # OpenFailed logged and dropped
└── counters.rs                   # + read the new counter

scripts/ebpf-integration-test.sh  # + assertions for the new invariants
```

**Structure Decision**: the existing three-crate layout, unchanged. The
open join goes in a new `programs/open_syscalls.rs` rather than into
`file_ops.rs`, because `file_ops.rs` is where the kprobes live and the
two should not be read as variants of one approach — what remains there
after the deletions is the unattached vfs pair and the shared noise
classifier, which the new module imports exactly as `fs_syscalls.rs`
does today.

## Delivery shape

Four changes, each independently verifiable, in this order. The order
matters: the baseline must be taken before capture changes, or SC-001b
has nothing to compare against.

| # | Change | Verified by |
|---|---|---|
| 1 | Measurements: x86_64 tracepoint inventory, successful-open baseline, today's event volume | `measurements/findings.md` gains run 2; `measurements/baseline-before.md` gains the counts and sets every later comparison reads. Baselines live in the second file, verdicts in the first, so a reader knows which answers which question. No production code moves |
| 2 | Open capture on tracepoints with the kernel-side join, old open kprobes deleted | Read sets differ only by failed requests, checked per entry against the discard record; fixture crate roots all present; volume no higher |
| 3 | Connect capture on `sys_enter_connect`, both address families, old kprobes and `attach_kprobe` deleted | An IPv6-only connect is recorded; TLS payload correlation still joins; the helper's absence is enforced by test |
| 4 | Harness and contract assertions | Half-attached pair fails closed; an architecture's missing syscall is not a reported failure; the discard counter has a positive control |

Change 1 is not optional overhead. Every criterion in the spec that
compares before against after depends on a baseline that does not yet
exist, and the one number this project already has — 902 — was measured
under semantics this feature deliberately changes.

## Complexity Tracking

No constitution violations to justify. The one deviation on record is
the spec checklist's deliberate failure of "success criteria are
technology-agnostic", which is argued in
`checklists/requirements.md`: the measurable outcome of this feature is
that observation no longer depends on the host environment, and stating
that without naming the environment would make it unverifiable.
