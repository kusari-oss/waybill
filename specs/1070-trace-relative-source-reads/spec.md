# Feature Specification: Trace captures relative source reads and compiler writes

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

**Writes are never captured at all (found in planning, from the code).** Both open probes report the open with its flags zeroed, so userspace classifies every open as a read (`trace/aggregator.rs`). The write probe reports no path, so its events are skipped. Write sets are therefore empty on every trace, whatever the build directory. C130 is emitted only for a component whose path is in some invocation's write set, so it has never been emitted.

**A third defect is split out.** C130 unions read sets along *process ancestry*. A library's sources are read by a sibling compiler process and reach a binary only through the library file it writes, so even complete read and write sets could not put `libvuln`'s sources into `vuln-included`'s C130. That changes what C130 means and is tracked in #1141.

## Clarifications

### Session 2026-10-06

- Q: (analysis remediation) A rename by an invocation whose original write was not observed? → A: **Record the new name in its write set anyway**, and drop the old name if present (FR-010). A noise-filtered original open must not lose the real output.

- Q: How is a kept relative open recorded? → A: **Resolved to an absolute path** against the opening process's working directory, in both the attestation's file operations and the read and write sets. The original relative spelling is not kept.
- Q: What happens when the working directory cannot be established? → A: The open **stays in the attestation's file operations, flagged as unresolved, and is left out of the read and write sets**. A trace-level count of unresolved relative opens is reported.
- Q: Scope, once planning found writes are never captured and C130 follows process ancestry only? → A: **Option B.** Capture relative reads (the original scope) **and writes** in this milestone. Cross-invocation (data-flow) attribution goes to #1141. US2 is restated accordingly.
- Q: Planning then found that no production path emits C130 at all: SBOM generation never receives the compiler pipeline, and locally built binaries are never components. Scope? → A: **Option B′.** This milestone makes the *attestation* correct: read and write sets. Emitting C130/C131 in the SBOM moves to #1142, which #1141 depends on. US2 drops its SBOM scenario.

## Out of Scope

- `events_dropped`, the counter for events still queued when a trace ends (#618).
- Restricting tracing to the build's own process tree (the "process-scoped tracing" follow-up named in milestone 213).
- The other noise categories (System prefixes, UserCache, Ephemeral, CargoFingerprint), except where a test needs a build directory outside them.
- File reads and writes reported without a path. The read and write probes report no path today; that is unchanged. Writes are captured from the open that precedes them.
- Attribution across compiler invocations: a library's sources reaching the binary that links it (#1141). C130's closure rule is unchanged here.
- Emitting C130/C131 in the SBOM: wiring the trace's compiler pipeline into generation, and making locally built binaries components (#1142).
- Hard links the build tool makes to user-facing names (`release/safe-only` from `deps/safe_only-<hash>`). They are not compiler writes. Matching them is #1142's concern.
- Tracer self-exclusion inside PID namespaces (#1143).

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

### User Story 2 - Compiler write sets record the files each invocation produced (Priority: P1)

Each compiler or linker invocation's write set lists the files it created or overwrote. That is the half of the record that ties a binary to the invocation that produced it. Without it, C130 is never emitted, however complete the reads are.

**Why this priority**: the write set is what ties an output to the invocation that produced it, and it is empty on every trace today. Every use of attribution, including C130 once #1142 wires it into the SBOM, needs it. This milestone makes it correct in the attestation.

**Independent Test**: trace the fixture built into a directory inside the workspace, and read the invocations' write sets in the attestation.

**Acceptance Scenarios**:

1. **Given** the fixture built into an in-tree directory, **When** traced, **Then** the link invocation that produced each of the two binaries has that binary's output path in its write set.
2. **Given** the same trace, **When** each library compile is read, **Then** its write set contains the library files it produced under their final names (`deps/lib<crate>-<hash>.rlib` and `.rmeta`). The compiler writes these to a temporary name and renames them into place (`measurements/strace_summary.txt`).
3. **Given** a file opened only for reading, **When** traced, **Then** it appears in no write set.
4. **Given** the same trace, **When** each binary compile's read set is read, **Then** it contains the library files the compile consumed. Build inputs under the build directory are not discarded as fingerprint noise.

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
- **A path longer than the per-event path limit** (255 bytes as captured in the kernel): the open is marked truncated. Today the open probes never set this flag, which this feature fixes. A truncated relative path is not resolved, since joining a cut-off name would produce a wrong absolute path. It is recorded as unresolved.
- **The tracer and the build in different PID namespaces** (observed: the child was pid 11 inside the container and 24020 on the host): the working directory is attributed by the same process identity the events carry.
- **Directory-entry walks** (a process opening each entry of a directory by name relative to that directory): these are relative opens too. If they are excluded as noise, the exclusion must not also exclude a file open.
- **Relative opens by non-compiler processes** (`cargo`, `ld`): treated the same way, since the file-operation record is not compiler-specific.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A default trace MUST NOT discard a file open because its path is relative.
- **FR-002**: Every relative open that is kept MUST be recorded as an absolute path, resolved against the opening process's working directory at the time of the open, in both the attestation's file operations and the compiler invocation's read or write set. The original relative spelling is not retained.
- **FR-003**: When the working directory of a relative open cannot be established, the trace MUST keep the open in the attestation's file operations, flagged as unresolved, and MUST leave it out of every read and write set. It MUST report a trace-level count of such opens. Read and write sets, and C130 derived from them, therefore contain only absolute paths.
- **FR-004**: Noise reduction for relative opens MAY remain only for opens that are positively identified as directory-entry walks. No relative file open of a source or build input may be excluded by it. None is retained (research R1).
- **FR-005**: `--include-system-reads` MUST continue to disable every System-category exclusion, including any retained directory-walk exclusion (milestone 213, FR-010).
- **FR-006**: On the `two_binaries_diverge` fixture, a default trace MUST report zero buffer overflows, as it does today.
- **FR-007**: The eBPF integration harness MUST assert the outcome, not only the presence of compiler invocations: the four crate roots in the read sets (US1), and the outputs in the write sets (US2). The harness MUST build into a directory the noise filter does not exclude, so that writes are observable.
- **FR-008**: A file opened for writing MUST be recorded as a write: opened write-only or read-write, or with creation or truncation requested. It goes into the opening invocation's write set and is recorded as a write in the attestation's file operations. A file opened only for reading MUST continue to be recorded as a read. (The cause and the decision are recorded in Context and Clarifications.)
- **FR-009**: A path resolved by FR-002 MUST be treated the same as an absolute path for both reads and writes.
- **FR-010**: When an invocation renames a file, its write set MUST record the file under the new name, and MUST drop the old name if present. This holds whether or not the original write was observed: the original open may have been discarded by a noise category, and the renamed file is still that invocation's output. Every successful rename is also recorded in the attestation's file operations as a write of the new name.
- **FR-011**: The kernel noise filter MUST NOT discard build outputs or build inputs under the build directory's `deps/` directory (compiled libraries, metadata, object files and linked binaries). Fingerprint and incremental-compilation bookkeeping remain filtered (milestone 213).

### Key Entities

- **File operation**: one observed open, read or write, with its process, time and path. This feature changes which ones are kept and how a relative path is recorded.
- **Compiler invocation**: one compiler or linker run, with its read set and write set; C130 is derived from these.
- **Unresolved relative open**: a relative open whose working directory could not be established, counted at trace level.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the `two_binaries_diverge` fixture, a default trace puts all 4 crate-root source files in the read sets of the invocations that compiled them. The measured baseline is 0 of 4.
- **SC-002**: On the same fixture built in-tree, each of the 2 binaries is in the write set of the invocation that linked it, and each of the 2 library compiles has its final `.rlib` and `.rmeta` in its write set. The measured baseline is 0 write-set entries on every trace.
- **SC-003**: On the same fixture, the default trace reports zero buffer overflows. The measured baseline is 0 at 34 events (default) and at 1,801 events (System category disabled).
- **SC-004**: Every relative open the build makes is accounted for: it appears in the trace, it is identified as a directory-walk exclusion, or it is in the unresolved count. In the measured run there were 22, and none may go unaccounted for.
- **SC-005**: For a build that opens no relative paths, the only differences in the attestation are opens-for-write now recorded as writes (FR-008), apart from the timestamps and process identifiers that already vary between runs.

## Assumptions

- The measurement was taken on one VM (Colima aarch64, kernel 6.8, 2 CPUs) with one fixture. The plan re-measures the relative-open volume on a larger workspace build before concluding the directory-walk volume is small in general. Any threshold the plan sets is expressed as a ratio against that measurement, not as an absolute taken from milestone 213.
- Trace-mode principles apply (Constitution II, III and XII.1): the working directory must come from observation of the build, not from reading manifests.
- The C130/C131 catalogue rows keep their names and wire shape. The read sets they are derived from gain entries (the resolved relative opens) but keep the same form: absolute paths only.
