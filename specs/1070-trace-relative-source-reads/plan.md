# Implementation Plan: Trace captures relative source reads and compiler writes

**Branch**: `1070-trace-relative-source-reads` | **Date**: 2026-10-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1070-trace-relative-source-reads/spec.md`

## Summary

`waybill trace` attestations get correct compiler read sets and write sets. Today both are empty on every workspace build.

**Reads.** The kernel stops discarding relative paths (R1). Userspace resolves each relative open against the opening process's working directory. That directory comes from a model built only from observed events: fork inherits, a successful `chdir` sets it, `fchdir` makes it unknown, and the traced root is seeded by a `chdir` the tracer causes (R2). What can't be resolved is kept in the file operations, flagged, and counted. It never enters a read or write set (FR-003).

**Writes.** The open probes record the open flags and the directory fd (R5), so opens-for-write become writes. Successful renames are captured, so a library compile's write set holds its final `.rlib`/`.rmeta` rather than the temporary names (R6). The milestone 213 `/deps/` pattern stops discarding build outputs and inputs (R7).

C130/C131 emission is out of scope (#1142), and so is data-flow attribution (#1141).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain, `rust-toolchain.toml`) for user space. The pinned nightly in `waybill-ebpf/rust-toolchain.toml` for the kernel crate. No new toolchain features.
**Primary Dependencies**: Existing only: `aya` / `aya-ebpf` (kprobes, tracepoints, ring buffer, `HashMap`), `serde` / `serde_json`, `tracing`. **Zero new Cargo dependencies.**
**Storage**: N/A, in-process per trace. `CwdTracker` lives for one trace. The kernel's pending-syscall map is keyed by thread and cleared at syscall exit.
**Testing**:
- unit tests for `CwdTracker` (fork / chdir / fchdir / unknown / relative chdir / truncation), `is_write`, rename handling in the compiler aggregator, and the classifier change in `waybill-common/src/filter.rs`;
- the eBPF integration harness, with FR-007's outcome assertions, which CI runs in the eBPF lane;
- `./scripts/pre-pr.sh`, plus `WAYBILL_PREPR_EBPF=1` locally for the feature-on build;
- a re-measurement with `measurements/probe.sh` on Colima for SC-003 and SC-004.

**Target Platform**: Linux (eBPF trace mode). The tracepoint set differs by architecture: arm64 has no `rename`.
**Project Type**: CLI, `waybill-cli` + `waybill-common` + `waybill-ebpf` (Constitution VI).
**Performance Goals**: zero ring-buffer overflows on the fixture (SC-003). Added kernel events are one per fork, one per successful `chdir`/`fchdir`/`rename`, and the relative opens: 0.5–0.9% of opens (R1).
**Constraints**:
- trace mode observes only (Constitution II): no `/proc` reads, no manifests;
- unresolved is reported, never guessed (III, X);
- attestation changes are additive and omitted at their default (SC-005);
- no new CLI flag.

**Scale/Scope**:
- `waybill-common`: `events.rs` (variants, `dfd`, `RenameEvent`, `LineageEvent`, size pin); `filter.rs` (drop the relative rule and `/deps/`); `attestation/file.rs` and `integrity.rs` (two additive fields).
- `waybill-ebpf`: `programs/file_ops.rs` (flags and dfd); `programs/compiler_exec.rs` (fork record); new syscall tracepoints for chdir, fchdir and rename; `maps.rs` (the pending map).
- `waybill-cli`: `trace/loader.rs` (attach the new tracepoints and report failures); `cli/scan.rs` (decode by type; route lineage events to `CwdTracker`; resolve before both aggregators; seed the root's directory); `trace/aggregator.rs` (write classification, the unresolved flag); `trace/compiler_pipeline.rs` (resolved paths only, writes, rename); new `trace/cwd.rs`.
- `scripts/ebpf-integration-test.sh` (in-tree target dir; FR-007 assertions; leak check narrowed).
- `docs/` (the trace attestation reference) and `CHANGELOG.md`.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Pure Rust | ✅ No new crates. |
| II. eBPF-only observation (trace mode) | ✅ The working directory comes only from observed fork, chdir and fchdir events. The root is seeded by a `chdir` the probes observe, not by reading state (R2). No `/proc`, no manifests. |
| III. Fail closed (trace mode) | ✅ Nothing is guessed. An unresolvable path is recorded, flagged and counted, never joined with a presumed directory. A missing tracepoint is reported like an attach failure. No fallback to static analysis. |
| IV. Type-driven | ✅ `WorkingDir::{Known, Unknown}` and `Resolved \| Unresolved` are enums. Record types are decoded by tag, each with its own size check. No `unwrap` in production paths. |
| VI. Three-crate architecture | ✅ Records and filters in `waybill-common`, probes in `waybill-ebpf`, model and aggregation in `waybill-cli`. |
| VII. Test isolation | ✅ The pure logic is unit-tested. Kernel behaviour is tested by the containerised harness CI already runs. |
| VIII. Completeness | ✅ The feature: source reads and outputs that were silently missing are now in the attestation. |
| IX. Accuracy | ✅ Writes are classified from the kernel's own open flags. Renames move only entries the invocation itself wrote. |
| X. Transparency | ✅ `unresolved_relative` per operation, plus a trace-level count. The witness products that appear are documented in the contract. |
| Measurement rule | ✅ Every number comes from `measurements/`. Milestone 213's figures are quoted only as history. |

No violations.

## Project Structure

### Documentation (this feature)

```text
specs/1070-trace-relative-source-reads/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/attestation.md
├── measurements/   (probe.sh, relative_paths.txt, strace_summary.txt, raw strace)
└── checklists/requirements.md
```

### Source Code (repository root)

```text
waybill-common/src/
├── events.rs                    # FileEventType variants, FileEvent.dfd, RenameEvent, LineageEvent
├── filter.rs                    # relative-path rule removed; /deps/ removed from CargoFingerprint
└── attestation/{file.rs,integrity.rs}   # unresolved_relative, unresolved_relative_opens
waybill-ebpf/src/
├── maps.rs                      # pending syscall args, keyed by tid
└── programs/
    ├── file_ops.rs              # flags + dfd on both open probes
    ├── compiler_exec.rs         # fork tracepoint also emits a LineageEvent
    └── fs_syscalls.rs           # new: chdir / fchdir / rename enter+exit tracepoints
waybill-cli/src/
├── trace/cwd.rs                 # new: CwdTracker, resolve(), is_write()
├── trace/loader.rs              # attach the new tracepoints
├── trace/aggregator.rs          # write classification, unresolved flag
├── trace/compiler_pipeline.rs   # resolved paths only, rename handling
└── cli/scan.rs                  # decode by type, route, seed the root's directory
scripts/ebpf-integration-test.sh # in-tree target; outcome assertions
```

**Structure Decision**: the existing three-crate layout. One new kernel program file and one new userspace module.

## Phasing

1. **Records and pure logic** (`waybill-common`, `trace/cwd.rs`), with unit tests. Nothing behaves differently yet.
2. **Kernel**: flags and dfd on the open probes, the fork record, the syscall tracepoints, the classifier change. Load and attach are checked with `scripts/verify-ebpf.sh`.
3. **Userspace wiring**: decode, route, resolve, classify, rename, seed the root.
4. **Harness and measurement**: update the assertions, then re-run `probe.sh` for SC-001…SC-004.
5. **Docs and CHANGELOG.**

## Complexity Tracking

None.
