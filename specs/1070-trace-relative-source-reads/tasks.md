---
description: "Tasks for milestone 1070: trace captures relative source reads and compiler writes"
---

# Tasks: Trace captures relative source reads and compiler writes

**Input**: `specs/1070-trace-relative-source-reads/` (spec.md, plan.md, research.md, data-model.md, contracts/attestation.md, quickstart.md)

**Tests**: Required. FR-007 makes the eBPF integration harness assert outcomes, and plan.md lists unit tests for every pure function. Write each test before the code it covers, and see it fail.

**Organization**: grouped by user story. Shared records and pure logic sit in Foundational because all three stories need them.

## Format: `[ID] [P?] [Story] Description`

Paths are relative to the repository root. `WC` = `waybill-common/src`, `WE` = `waybill-ebpf/src`, `WL` = `waybill-cli/src`.

---

## Phase 1: Setup

- [X] T001 Confirm the kernel object builds and loads on this branch before any change: run `scripts/verify-ebpf.sh` (Linux, or inside the Colima VM). Note the result in `specs/1070-trace-relative-source-reads/measurements/after.txt` as the "before" line.

---

## Phase 2: Foundational (blocks all stories)

- [X] T002 In `WC/events.rs`:
  - add `FileEventType` variants `Fork = 4`, `Chdir = 5`, `Fchdir = 6` and `Rename = 7`, leaving `Open/Read/Write/Close = 0..3` unchanged;
  - add `pub dfd: i32` to `FileEvent`;
  - add `#[repr(C)]` structs `LineageEvent { event_type, timestamp_ns, parent_pid, child_pid }` and `RenameEvent { event_type, timestamp_ns, pid, tid, comm, old_dfd, new_dfd, old_path: [u8; 256], new_path: [u8; 256], old_truncated, new_truncated, padding }`, with the same derives and `Pod`/`Zeroable` treatment as `FileEvent`. `event_type` is the first field of every record, so userspace can dispatch on it;
  - update the `size_of::<FileEvent>() == 352` pin test to the new size, and add size pins for the two new structs.
- [X] T003 [P] In `WC/filter.rs`, remove both parts of the milestone 213 rule (research R1, R7):
  - the relative-path rule (`if !widen_system && path[0] != b'/'`);
  - the `/deps/` CargoFingerprint pattern.

  Update the module and function doc comments, citing #614 and `specs/1070-.../research.md`. Replace `t007_relative_paths_classified_as_system` with a test that a relative path returns `None` for both values of `widen_system`. In `t007_cargo_fingerprint_paths_classified`, assert that `.../target/release/deps/libfoo.rlib` returns `None`, and add a real cargo fingerprint path, `.../target/release/.fingerprint/foo-abc/invoked.timestamp`. If that real path is not classified (the existing `/fingerp` pattern needs `/` directly before `f`, while cargo's directory is `.fingerprint`), add `pack_pattern(b"/.finger", 8, CAT_CARGO_FINGERPRINT, false)`, and record the gap in research.md R7.
- [X] T004 [P] In `WC/attestation/file.rs`, add `#[serde(default, skip_serializing_if = "core::ops::Not::not")] pub unresolved_relative: bool` to `FileOperation`. In `WC/attestation/integrity.rs`, add `#[serde(default, skip_serializing_if = "is_zero")] pub unresolved_relative_opens: u64` to `TraceIntegrity`. Update every struct-literal construction site: about 28 for `TraceIntegrity` and 10 for `FileOperation` across `WC` and `WL` (`grep -rn "TraceIntegrity {"`, `"FileOperation {"`). Add a round-trip test showing both fields are omitted at their defaults (SC-005).
- [X] T005 Create `WL/trace/cwd.rs` (registered in `WL/trace/mod.rs`) per data-model.md:
  - `enum WorkingDir { Known(PathBuf), Unknown }`;
  - `struct CwdTracker` with `fork(parent, child)`, `chdir(pid, path: &str, truncated: bool)`, `fchdir(pid)` and `resolve(pid, dfd, path) -> Resolution`;
  - `enum Resolution { Resolved(PathBuf), Unresolved }`;
  - `pub fn is_write(flags: u32) -> bool`, using local Linux constants: `O_ACCMODE = 0o3`, `O_WRONLY = 0o1`, `O_RDWR = 0o2`, `O_CREAT = 0o100`, `O_TRUNC = 0o1000`, the same values on x86-64 and arm64; `pub const AT_FDCWD: i32 = -100`.

  Joins are lexical (`Path::join`), never canonicalised, and do no filesystem access. Doc comments cite research R2 and R5.
- [X] T006 [P] Unit tests in `WL/trace/cwd.rs`, under `#[cfg(test)] #[cfg_attr(test, allow(clippy::unwrap_used))]`, one per data-model.md row:
  - fork inherits; fork from an unknown parent is unknown;
  - absolute chdir; relative chdir against a known or an unknown directory; a truncated chdir gives unknown;
  - fchdir gives unknown; an unseen pid is unknown;
  - absolute paths always resolve; `AT_FDCWD` + known resolves; a real dfd is unresolved;
  - `..` is kept verbatim;
  - `is_write` for `O_RDONLY`, `O_WRONLY`, `O_RDWR`, `O_RDONLY|O_CREAT` and `O_RDONLY|O_TRUNC`.

**Checkpoint**: `cargo +stable test -p waybill-common -p waybill` passes. Behaviour is unchanged until the kernel and wiring tasks land.

---

## Phase 3: User Story 1 — Compiler read sets include the workspace's own sources (P1) 🎯 MVP

**Goal**: relative opens are kept and resolved, so each crate compile's read set contains its crate root.
**Independent Test**: quickstart.md "US1": 4 absolute crate-root paths in the `rustc` read sets of a default trace of `two_binaries_diverge`.

- [X] T007 [US1] In `WE/programs/file_ops.rs`, both `try_openat2` and `try_do_filp_open`:
  - read `dfd` from `ctx.arg(0)` (as `i32`) and store it in the new `FileEvent.dfd`;
  - set `path_truncated = 1` when the string copy filled the 256-byte buffer (its returned length is 255 with no terminator inside; analysis I1).

  In `WL/trace/cwd.rs`, `resolve` returns `Unresolved` for a truncated relative path, with a unit test. The relative-path drop is gone with T003's classifier change; leave a comment citing #614.
- [X] T008 [US1] In `WE/programs/compiler_exec.rs`, `sched_process_fork` keeps its existing `PID_TO_PPID` work and additionally reserves a `LineageEvent { Fork, ts, parent_pid, child_pid }` in `FILE_EVENTS`. **`parent_pid` is `bpf_get_current_pid_tgid() >> 32`** (the parent's process id, since the tracepoint runs in the parent). It is not the tracepoint's `parent_pid` argument at offset 24, which is a thread id (analysis U1). `child_pid` stays the offset-44 argument. In `WL/trace/cwd.rs`, add a unit test: a fork whose parent is a non-main thread's process still inherits the process's directory. On reserve failure it calls `increment_drop_counter(&FILE_EVENT_DROPS)`. Import `FILE_EVENTS` and `FILE_EVENT_DROPS` from `crate::maps`.
- [X] T009 [US1] Create `WE/programs/fs_syscalls.rs` (registered in `WE/programs/mod.rs` and wherever programs are declared in `WE/main.rs`). Add a per-thread pending map in `WE/maps.rs`: `PENDING_CHDIR: HashMap<u32 /*tid*/, [u8; 256]>`, 4096 entries.
  - `sys_enter_chdir`: copy the user path argument with `bpf_probe_read_user_str_bytes` into the map, keyed by tid.
  - `sys_exit_chdir`: look up and remove the entry; if `ret == 0`, reserve a `FileEvent { Chdir, path, dfd: AT_FDCWD, path_truncated }` (truncated when the copy filled the buffer).
  - `sys_exit_fchdir`: if `ret == 0`, emit `FileEvent { Fchdir }` with an empty path.
  - Every reserve failure increments `FILE_EVENT_DROPS`. Apply `should_trace()` as the other probes do. The noise classifier does **not** apply to these records.
- [X] T010 [US1] In `WL/trace/loader.rs`, attach the new chdir/fchdir enter/exit tracepoints with the existing `attach_tracepoint(bpf, prog, "syscalls", name)` helper. **An attach failure is fatal** (Principle III; analysis C1): return an error naming the tracepoint, so `waybill trace` exits non-zero. Do not use the `warn!`-and-continue pattern the m210 `sched_process_*` tracepoints use. Add a short comment noting that those stay best-effort, as a follow-up.
- [X] T011 [US1] In `WL/cli/scan.rs`:
  - `drain_file` reads the leading `event_type` (`u32`) of each record and dispatches. `Fork` → `cwd.fork`, `Chdir` → `cwd.chdir`, `Fchdir` → `cwd.fchdir`, each after checking the record's own size. `Open`/`Read`/`Write` → resolve through `cwd.resolve(ev.pid, ev.dfd, path)` before calling `agg.handle_file_event` and `compiler_agg.handle_file_event`.
  - Own the `CwdTracker` alongside the two aggregators, in the drain loop and in the settling drain.
  - Count unresolved opens, and set `TraceIntegrity.unresolved_relative_opens` wherever this function builds `TraceIntegrity`, for both attestation formats.
- [X] T012 [US1] In `WL/cli/scan.rs`, give the child spawn (`Command::new(&cmd[0])`, about line 231) `.current_dir(std::env::current_dir()?)`. The child then performs a `chdir` that T009 observes, which seeds the root's directory without PID-namespace mapping (research R2). Add a comment explaining this, since the call looks redundant.
- [X] T013 [US1] In `WL/trace/aggregator.rs`, change `handle_file_event` to take the resolved path and an `unresolved: bool`, as one small input struct rather than a re-cast `FileEvent`. Record `FileOperation.path` = the resolved path (or the raw path if unresolved) with `unresolved_relative` set. Update the call sites and tests.
- [X] T014 [US1] In `WL/trace/compiler_pipeline.rs`, `handle_file_event` takes the same input. Unresolved operations are never inserted into `read_set` or `write_set` (FR-003). Resolved relative paths go through the FR-016 filter and the read-set insert exactly as absolute paths do (FR-009).
- [X] T015 [P] [US1] Unit tests in `WL/trace/aggregator.rs` and `WL/trace/compiler_pipeline.rs`:
  - a resolved relative open lands in the invocation's read set as the absolute path;
  - an unresolved open appears in the file operations with `unresolved_relative: true` and in no read set.

**Checkpoint**: US1 is done when T025 shows SC-001 (4 of 4 crate roots).

---

## Phase 4: User Story 2 — Write sets record the files each invocation produced (P1)

**Goal**: opens-for-write and renames become writes, so link and library compiles have their outputs, under final names, in their write sets.
**Independent Test**: quickstart.md "US2".

- [X] T016 [US2] In `WE/programs/file_ops.rs`, record the open flags (research R5):
  - `try_do_filp_open` reads the `int` at offset 0 of `ctx.arg(2)` (`const struct open_flags *`) with `bpf_probe_read_kernel`;
  - `try_openat2` reads the `u64` at offset 0 of `ctx.arg(2)` (`struct open_how *`, a kernel pointer), truncated to `u32`;
  - store the value in `FileEvent.flags`; on a read failure store 0, which is treated as read-only.
- [X] T017 [US2] In `WE/programs/fs_syscalls.rs`, add the rename tracepoints `sys_enter_renameat` / `sys_exit_renameat` and `sys_enter_renameat2` / `sys_exit_renameat2`, plus `sys_enter_rename` / `sys_exit_rename` where the architecture has it (x86-64; arm64 does not).
  - Enter stores `{old_dfd, old_path, new_dfd, new_path}` in a per-tid pending map (`PENDING_RENAME` in `WE/maps.rs`, 4096 entries). For `rename`, the dfds are `AT_FDCWD`.
  - Exit emits a `RenameEvent` when `ret == 0`, and always removes the entry.
  - Apply `classify_and_drop_if_noise` to the **new** path only. Reserve failure increments `FILE_EVENT_DROPS`.
- [X] T018 [US2] In `WL/trace/loader.rs`, attach the rename tracepoints. `renameat` and `renameat2` enter/exit are **fatal on failure**, like T010. `sys_enter_rename` / `sys_exit_rename` are required only where the architecture has the syscall (x86-64); on arm64 their absence is not a failure. Select by `cfg(target_arch)`.
- [X] T019 [US2] In `WL/trace/aggregator.rs`, classify `Open` as `Write` when `cwd::is_write(flags)`, and as `Read` otherwise. The input struct from T013 carries the flags. A `Rename` input records a `Write` of the resolved new path, flagged unresolved if it could not be resolved.
- [X] T020 [US2] In `WL/trace/compiler_pipeline.rs`, an `Open` with `is_write(flags)` inserts into `write_set` (the existing `Write` branch), not `read_set`. A `Rename` by a pid that belongs to an invocation inserts the new path into its `write_set`, carrying the old entry's value if the old path was present, and removes the old path. This applies even when the old path was never observed (FR-010, analysis U3). Unresolved paths never touch either set.
- [X] T021 [US2] In `WL/cli/scan.rs` `drain_file`, dispatch `Rename`: check the `RenameEvent` size, resolve both paths with `cwd.resolve`, count any unresolved side, and pass the result to both aggregators.
- [X] T022 [P] [US2] Unit tests:
  - `is_write` drives Read/Write in `aggregator.rs`;
  - in `compiler_pipeline.rs`: a write-mode open lands in `write_set`; a read-only open lands in no `write_set` (US2 scenario 3); a rename moves an own entry from `deps/rmetaX/full.rmeta` to `deps/libfoo-h.rmeta`; a rename whose old path was never observed still adds the new path;
  - `witness_builder.rs` products now list written files, and operations flagged `unresolved_relative` appear in neither materials nor products (contracts/attestation.md; analysis U2). Implement that exclusion in `WL/attestation/witness_builder.rs`.

**Checkpoint**: US2 is done when T025 shows SC-002.

---

## Phase 5: User Story 3 — Nothing is discarded silently (P2)

**Goal**: every unresolved relative open is visible and counted.
**Independent Test**: quickstart.md "US3": the count equals the number of flagged operations.

- [X] T023 [US3] Trace every `TraceIntegrity` construction reachable from `trace capture` / `trace run` in both formats: `WL/cli/scan.rs`, `WL/attestation/witness_builder.rs`, `WL/attestation/builder.rs`. Make `unresolved_relative_opens` equal the number of `FileOperation`s with `unresolved_relative: true`, and derive it from them at finalisation rather than counting separately, so the two cannot drift (the m776 lesson: count the emitted set).
- [X] T024 [P] [US3] Unit tests: a pid with no observed directory (attach mode), after `fchdir`, and with a directory-fd-relative open, each produces a flagged operation, and the count equals the flagged-operation count.

---

## Phase 6: Harness, measurement and kernel verification

- [X] T025 Update `scripts/ebpf-integration-test.sh` (FR-007):
  - build into an in-tree, freshly removed `$FIXTURE/target` instead of `/tmp/m210-fixture-target`;
  - assert that the 4 absolute crate roots appear in `rustc` read sets (quickstart US1);
  - assert that link invocations' `write_set`s contain `deps/safe_only-*` and `deps/vuln_included-*`;
  - assert that the library compiles' `write_set`s contain final `deps/liblibsafe-*.rlib` / `.rmeta` and `deps/liblibvuln-*.rlib` / `.rmeta`, and no `rmeta*/full.rmeta` entry remains;
  - assert that the `vuln-included` and `safe-only` compiles' `read_set`s contain the `deps/lib*.rlib` or `.rmeta` of each library they depend on (US2 scenario 4; analysis G1);
  - assert `trace_integrity.unresolved_relative_opens` equals the flagged-operation count;
  - assert `ring_buffer_overflows == 0` (SC-003), replacing the `≤ 10000` threshold;
  - narrow the fingerprint-leak check from `/fingerprint/|/deps/|/incremental/` to `/.fingerprint/|/incremental/`. Note in a comment that the old `/fingerprint/` never matched cargo's `.fingerprint` directory.
- [X] T026 Run `scripts/verify-ebpf.sh`: the kernel object builds and the verifier accepts every new program, including the classifier loop budget after T003. Then run the Colima harness (`docker build -f Dockerfile.ebpf-test -t waybill-ebpf-test . && scripts/ebpf-container-run.sh waybill-ebpf-test 720`) and `measurements/probe.sh`. Record SC-001…SC-004 and the event counts in `measurements/after.txt`, against `relative_paths.txt`. If `.fingerprint/` bookkeeping now floods the trace, fix the pattern (T003) and re-run.

---

## Phase 7: Polish

- [X] T027 [P] Update `docs/architecture/attestations.md`:
  - file operations: resolved relative paths, `unresolved_relative`, write classification from open flags, renames as writes;
  - `trace_integrity.unresolved_relative_opens`;
  - witness products now populated;
  - the removed m213 rules.

  Cite #614 and #1142 (C130 still not emitted).
- [X] T028 [P] Add `CHANGELOG.md` `[Unreleased]` entries: trace read sets include relative source reads (#614); write sets and witness products are populated from opens-for-write and renames; kernel filter no longer drops relative paths or `deps/` outputs.
- [X] T029 Run `./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo EXIT=$?`. Require EXIT=0, `>>> all pre-PR checks passed.`, and every `test result: ok`. The feature-on (`ebpf-tracing`) build is gated by CI's eBPF lane, which also runs T025's harness.
- [ ] T030 After merge, `cargo clean`.

---

## Dependencies & Execution Order

- **T001** → **Phase 2 (T002–T006)** → stories.
- **T002** blocks T004's tests, T005, and every kernel task.
- **US1 (T007–T015)** before **US2 (T016–T022)**: both edit `file_ops.rs`, `fs_syscalls.rs`, `scan.rs`, `aggregator.rs` and `compiler_pipeline.rs`. US2's rename resolution also uses US1's `CwdTracker` wiring.
- **US3 (T023–T024)** after US1, since it needs the unresolved path through the pipeline. It can proceed alongside US2 apart from the `scan.rs` edits.
- **T025–T026** after US1–US3. **T027–T028** at any point after the design is stable. **T029** last.

## Parallel Opportunities

- Phase 2: T003 (`filter.rs`), T004 (`attestation/*`) and T006 (tests) run alongside one another once T002 lands. T005 precedes T006.
- US1: T015 tests alongside T013/T014 once their signatures are fixed.
- Polish: T027 ∥ T028.

## Implementation Strategy

**MVP = Phase 2 + US1.** It delivers SC-001: the 4 crate roots in read sets, from 0 today, and closes #614's stated symptom. US2 then adds writes (SC-002), and US3 hardens the accounting (SC-004). Each story's checkpoint is verified with T026 on Colima, because only a real kernel shows these outcomes.
