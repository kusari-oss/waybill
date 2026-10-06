# Feature Specification: Trace captures source files opened through relative paths

**Feature Branch**: `1070-trace-relative-source-reads`
**Created**: 2026-10-06
**Status**: Draft
**Input**: Issue #614: "File-op kprobe silently stops emitting after initial ~15 ms burst — read_set/write_set stays empty on every cargo-build trace".

## Context

`waybill trace` records which files each compiler invocation read and wrote. Source-to-binary attribution depends on it: the `waybill:source-read-set` annotation (catalogue row C130) tells a consumer which source files went into each binary. On the fixture built for exactly this purpose (`two_binaries_diverge`: two binaries, one of which links a vulnerable library), every compiler invocation's read set is empty, so the attribution never appears.

#614 attributed this to a userspace drain too slow for the kernel's event rate. That cause has since been fixed (#991). Re-measured on current `main` (`measurements/relative_paths.txt`, Colima aarch64, kernel 6.8):

| trace | file events | from the compiler | compiler read sets | buffer overflows |
|---|---|---|---|---|
| default | 34, all from `cargo`, within 0.2 ms | 0 | all 9 empty | 0 |
| with the System noise category disabled | 1,801 | 994 | 51–95 entries each | 0 |

**The current cause is the kernel-side noise filter (milestone 213).** It discards every file open whose path does not begin with `/`, on the stated assumption that compilers always read sources through absolute paths. Cargo does not do that for the members of a workspace. It runs the compiler from the workspace root with paths such as `libsafe/src/lib.rs`, so every source read of a workspace member is discarded before it leaves the kernel.

**The rule's original reason no longer holds.** It was added while the drain was slow and the buffer overflowed about 14,000 times per run, to suppress an estimated ~12,000 relative directory-entry opens. With the drain fixed, the same build makes **22 relative opens in total**: 8 are the four crate roots, each read twice, and the other 14 are toolchain and linker lookups. The trace carries all 1,801 events with zero overflows. Milestone 213's own spec deferred "full rustc-inputs capture" to a follow-up; this is that follow-up.

**Writes are a separate gap.** Write sets were empty in both runs, but the harness builds into `/tmp`, which the Ephemeral noise category discards by design. Whether writes are captured for an in-tree build directory has not been measured.

## Clarifications

### Session 2026-10-06

- Q: How is a kept relative open recorded? → A: **Resolved to an absolute path** against the opening process's working directory, in both the attestation's file operations and the read and write sets. The original relative spelling is not kept.
- Q: What happens when the working directory cannot be established? → A: The open **stays in the attestation's file operations, flagged as unresolved, and is left out of the read and write sets**. A trace-level count of unresolved relative opens is reported.

## Out of Scope

- `events_dropped`, the counter for events still queued when a trace ends (#618).
- Restricting tracing to the build's own process tree (the "process-scoped tracing" follow-up named in milestone 213).
- The other noise categories (System prefixes, UserCache, Ephemeral, CargoFingerprint), except where a test needs a build directory outside them.
- File reads and writes reported without a path. The read and write probes report no path today; that is unchanged.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Compiler read sets include the workspace's own sources (Priority: P1)

An operator traces a build of a Cargo workspace. Today each compiler invocation's read set is empty, because the compiler opened the workspace members' sources through relative paths and the trace discarded them. After this feature, each invocation's read set lists the source files it compiled.

**Why this priority**: it is the issue. Every downstream use of the read set (C130 attribution, the build-trace evidence) depends on it, and it is empty for every workspace build.

**Independent Test**: trace a default build of the `two_binaries_diverge` fixture. The invocation that compiles each of its four crates has that crate's root source file in its read set.

**Acceptance Scenarios**:

1. **Given** the fixture, **When** it is traced with default settings, **Then** the compiler invocation for `libsafe` has `libsafe/src/lib.rs` in its read set, and likewise for `libvuln`, `safe-only` and `vuln-included`.
2. **Given** the same trace, **When** the attestation's file operations are read, **Then** the compiler's opens of those four files appear there too.
3. **Given** a build that opens sources only through absolute paths, **When** it is traced, **Then** the result is unchanged from today.

---

### User Story 2 - Attribution tells the two binaries apart (Priority: P1)

The fixture exists to prove one thing: the binary that links the vulnerable library is distinguishable from the one that does not. With read and write sets populated, the source read set of `vuln-included` contains `libvuln`'s source and that of `safe-only` does not.

**Why this priority**: this is the outcome attribution is for. Populated read sets that do not produce the distinction would not close the issue.

**Independent Test**: trace the fixture with its build directory inside the workspace and generate the SBOM. Compare the two binaries' C130 annotations.

**Acceptance Scenarios**:

1. **Given** the fixture built into an in-tree directory, **When** traced and turned into an SBOM, **Then** `vuln-included`'s source read set includes `libvuln`'s crate-root source and `safe-only`'s does not.
2. **Given** the same SBOM, **When** C131 (`waybill:read-set-source`) is read for the two binaries, **Then** it says `traced`.

---

### User Story 3 - Nothing is discarded silently (Priority: P2)

A relative path is meaningful only together with the directory it was opened from. When the trace cannot establish that directory, it says so instead of dropping the read or presenting an unanchored path as if it were complete.

**Why this priority**: under Constitution Principle III (trace mode fails closed and reports lost evidence) and Principle X, a gap must be visible. Today's silent discard is exactly what made #614 look like an emission failure.

**Independent Test**: induce a relative open whose directory cannot be established, and confirm the attestation reports it.

**Acceptance Scenarios**:

1. **Given** a traced process that opens a relative path, **When** its working directory cannot be established, **Then** the open appears in the attestation's file operations flagged as unresolved, it is absent from every read and write set, and a trace-level count of unresolved relative opens is non-zero.
2. **Given** a trace in which every relative open was resolved, **When** the attestation is read, **Then** that count is zero or absent.

---

### Edge Cases

- **A process changes directory** between two relative opens: each open resolves against the directory current at that open.
- **`..` segments and symlinked directories**: a resolved path is reported as the build saw it. It is not canonicalised against a filesystem the trace no longer has access to.
- **A resolved path longer than the per-event path limit**: reported as truncated, as absolute paths over the limit already are.
- **The tracer and the build in different PID namespaces** (observed: the child was pid 11 inside the container and 24020 on the host): the working directory is attributed by the same process identity the events carry.
- **Directory-entry walks** (a process opening each entry of a directory by name relative to that directory): these are relative opens too. If they are excluded as noise, the exclusion must not also exclude a file open.
- **Relative opens by non-compiler processes** (`cargo`, `ld`): treated the same way, since the file-operation record is not compiler-specific.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A default trace MUST NOT discard a file open because its path is relative.
- **FR-002**: Every relative open that is kept MUST be recorded as an absolute path, resolved against the opening process's working directory at the time of the open, in both the attestation's file operations and the compiler invocation's read or write set. The original relative spelling is not retained.
- **FR-003**: When the working directory of a relative open cannot be established, the trace MUST keep the open in the attestation's file operations, flagged as unresolved, and MUST leave it out of every read and write set. It MUST report a trace-level count of such opens. Read and write sets, and C130 derived from them, therefore contain only absolute paths.
- **FR-004**: Noise reduction for relative opens MAY remain only for opens that are positively identified as directory-entry walks. No relative file open of a source or build input may be excluded by it.
- **FR-005**: `--include-system-reads` MUST continue to disable every System-category exclusion, including any retained directory-walk exclusion (milestone 213, FR-010).
- **FR-006**: On the `two_binaries_diverge` fixture, a default trace MUST report zero buffer overflows, as it does today.
- **FR-007**: The eBPF integration harness MUST assert the outcome, not only the presence of compiler invocations. The four crate roots must be in the read sets (US1), and the two binaries' source read sets must differ as US2 describes. The harness MUST build into a directory the noise filter does not exclude, so that writes are observable.
- **FR-008**: Before relying on write sets for US2, the plan MUST measure whether writes to an in-tree build directory are captured. If they are not, the cause MUST be recorded and the write capture scoped as its own requirement or a follow-up issue, not assumed.

### Key Entities

- **File operation**: one observed open, read or write, with its process, time and path. This feature changes which ones are kept and how a relative path is recorded.
- **Compiler invocation**: one compiler or linker run, with its read set and write set; C130 is derived from these.
- **Unresolved relative open**: a relative open whose working directory could not be established, counted at trace level.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the `two_binaries_diverge` fixture, a default trace puts all 4 crate-root source files in the read sets of the invocations that compiled them. The measured baseline is 0 of 4.
- **SC-002**: On the same fixture built in-tree, `vuln-included`'s C130 read set contains `libvuln`'s crate root and `safe-only`'s does not.
- **SC-003**: On the same fixture, the default trace reports zero buffer overflows. The measured baseline is 0 at 34 events (default) and at 1,801 events (System category disabled).
- **SC-004**: Every relative open the build makes is accounted for: it appears in the trace, it is identified as a directory-walk exclusion, or it is in the unresolved count. In the measured run there were 22, and none may go unaccounted for.
- **SC-005**: For a build that opens no relative paths, the attestation is unchanged apart from the timestamps and process identifiers that already vary between runs.

## Assumptions

- The measurement was taken on one VM (Colima aarch64, kernel 6.8, 2 CPUs) with one fixture. The plan re-measures the relative-open volume on a larger workspace build before concluding the directory-walk volume is small in general. Any threshold the plan sets is expressed as a ratio against that measurement, not as an absolute taken from milestone 213.
- Trace-mode principles apply (Constitution II, III and XII.1): the working directory must come from observation of the build, not from reading manifests.
- The C130/C131 catalogue rows keep their names and wire shape. The read sets they are derived from gain entries (the resolved relative opens) but keep the same form: absolute paths only.
