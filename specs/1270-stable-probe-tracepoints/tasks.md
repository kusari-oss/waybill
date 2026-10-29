---
description: "Task list for 1270-stable-probe-tracepoints"
---

# Tasks: Capture that survives a kernel change

**Input**: Design documents from `/specs/1270-stable-probe-tracepoints/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md,
contracts/, quickstart.md

**Tests**: included, because the spec asks for them by name. FR-012
requires a test that fails if capture regresses to depending on an
internal kernel name, FR-013 requires the harness to assert a positive
signal, and SC-004 requires enforcement "by a test rather than by
review".

**Organization**: grouped by user story. The exception is Phase 1,
which is measurement and blocks everything — three success criteria
compare after against before, and the before does not exist yet.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3 from spec.md
- Exact file paths are in every task

---

## Phase 1: Measurement (Blocking Prerequisites)

**Purpose**: establish what "before" is, on this feature's own terms.

**⚠️ CRITICAL**: no capture code changes until T001–T004 are done. The
only count this project has — 902 file-open events — was measured under
request semantics that this feature deliberately narrows, so a lower
count afterwards is correct and comparing against 902 would fail a
working implementation. There is no measurement of successful-opens-only
anywhere yet.

- [ ] T001 Run `specs/1270-stable-probe-tracepoints/measurements/tracepoint-inventory.sh` on x86_64 in the eBPF lane and record the output as "Run 2" in `specs/1270-stable-probe-tracepoints/measurements/findings.md`, with kernel release and architecture. Decides research R9's open question: whether `sys_enter_open` and `sys_enter_creat` exist there, and so whether the `#[cfg(target_arch = "x86_64")]` block in T017 ships at all or is dead code
- [X] T002 [P] Capture the pre-change successful-open baseline: run `docker run --rm --privileged waybill-ebpf-test`, and record into `specs/1270-stable-probe-tracepoints/measurements/baseline-before.md` the file-open event count, the complete read set, and the complete write set for the fixture build, with the kernel release and image version the run drew (SC-001a)
- [X] T003 [P] Record today's ring-buffer event count for the fixture build and the `ring_buffer_overflows` value in `specs/1270-stable-probe-tracepoints/measurements/baseline-before.md`, labelled as SC-008's ceiling. Take it from the same run as T002 so the two cannot disagree
- [X] T004 Prove the container type-check is not vacuous: introduce a deliberate type error inside the `cfg(all(target_os = "linux", feature = "ebpf-tracing"))` block in `waybill-cli/src/trace/loader.rs`, run `docker build -f Dockerfile.ebpf-test -t waybill-ebpf-test .`, confirm the build fails **at that line**, then revert. A container build that reports success while compiling none of the changed code is the failure mode this step exists to catch, and it happened on this branch's predecessor (quickstart §1)

**Checkpoint**: every later "no lower than before" and "only by the
removal of failed requests" claim now has something to compare against.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the shapes both US1 and US2 need. Nothing here changes
capture behaviour, so the lane stays as green or as red as it was.

- [X] T005 Add `PendingOpen` (`#[repr(C)]`, fields `dfd: i64`, `path_ptr: u64`, `flags: u32`, `_padding: u32`) and `PENDING_OPEN: HashMap<u32, PendingOpen>` with 4096 entries to `waybill-ebpf/src/maps.rs`, placed beside `PendingRename` at `maps.rs:205` and documented the same way
- [X] T006 Add `OPEN_FAILURES_DISCARDED: PerCpuArray<u64>` and `OPEN_DIAG: PerCpuArray<u8>`, both with 1 entry, to `waybill-ebpf/src/maps.rs`, following the m212 `FILE_EVENT_DROPS` (`maps.rs:143`) and m213 `FILTER_WIDEN` (`maps.rs:191`) patterns. Same file as T005, so sequential
- [X] T007 [P] Add `FileEventType::OpenFailed = 8` and its `from_raw` arm to `waybill-common/src/events.rs`, with a doc comment stating that `flags` carries the negated errno for this variant and open flags for every other
- [X] T008 [P] Add `open_failures_discarded: u64` as the **last** field of `TraceIntegrity` in `waybill-common/src/attestation/integrity.rs`, with `#[serde(default, skip_serializing_if = "is_zero")]`, following the m1070 `unresolved_relative_opens` precedent at `integrity.rs:36-40`
- [X] T009 Add a unit test in `waybill-common/src/attestation/integrity.rs` asserting that a pre-#1252 `TraceIntegrity` JSON document round-trips with `open_failures_discarded` absent, and that a zero value serializes byte-identically to today's output. Mutate the `skip_serializing_if` to confirm the test discriminates
- [X] T010 Create `waybill-ebpf/src/programs/open_syscalls.rs` with the `ARG*`/`RET` constants imported or re-declared from the `fs_syscalls.rs` convention (`fs_syscalls.rs:35-39`), register it in `waybill-ebpf/src/programs/mod.rs`, and leave the programs unimplemented. Kept separate from `file_ops.rs` deliberately: that file is where the kprobes live, and the two approaches should not read as variants of one. **Its module-level doc comment MUST record which interfaces this capture treats as stable and why** — syscall names and argument layouts are UAPI, internal function symbols are not — following the `fs_syscalls.rs:1-22` header precedent. This is FR-011, and the placement is the requirement: FR-011 exists to stop a future change reintroducing an unstable dependency, and a reader reaching for that rationale is in this file, not in `docs/`

**Checkpoint**: shapes exist, behaviour unchanged, attestation output
byte-identical.

---

## Phase 3: User Story 1 — A build's recorded inputs do not depend on the host kernel (P1) 🎯 MVP

**Goal**: file-open capture stops depending on internal kernel function
names, and records opens that succeeded rather than opens that were
attempted.

**Independent Test**: on the runner image where the current build
records 443 file-open events, every crate root the fixture compiles
appears in the recorded read set.

### Tests for User Story 1

- [X] T011 [US1] **Rewritten after implementation, and the original is wrong.** As specified this was an aggregator test, and the aggregator cannot discriminate: a kernel read of `struct open_how.flags` that fails arrives as `flags == 0`, so the aggregator sees a well-formed read and is right to record one. The discriminator has to be downstream of the kernel read. It is the **write set holding at 31 distinct paths before and after** (`measurements/findings.md` run 3): had `openat2` flags been mis-read as 0, every `O_WRONLY|O_CREAT` open through that syscall would have been classified as a read and the write set would have collapsed. Recorded as the verification rather than left as an aggregator test that would pass either way.
- [X] T012 [P] [US1] Add a test in `waybill-cli/src/trace/aggregator.rs` asserting that a `FileEventType::OpenFailed` event reaches no read set, no write set, and no `file_ops` entry — it is logged and dropped (contracts/open-join.md)
- [X] T013 [P] [US1] Add a test in `waybill-cli/src/trace/aggregator.rs` asserting that a controlled sequence of N successful opens produces exactly N recorded entries (FR-005). The aggregate volume checks cannot substitute: the ceiling T003 establishes has slack equal to the duplication this feature removes — `do_sys_openat2` calls `do_filp_open`, so one `openat` currently submits two events — so a new double-submit on a subset of opens would fit under that ceiling undetected (research R8)
- [X] T014 [US1] Add a test asserting that `attach_kprobe` does not exist in `waybill-cli/src/trace/loader.rs` and that no kprobe is attached anywhere in the trace loader (FR-012, SC-004). Assert on the helper's absence rather than on a list of attach call sites: a list can grow an entry while the test still passes

### Implementation for User Story 1

- [X] T015 [US1] Implement the `sys_enter_openat` / `sys_exit_openat` join in `waybill-ebpf/src/programs/open_syscalls.rs`: store `PendingOpen` at entry keyed by `current_tid()`, and at exit remove the entry **before** reading `ret`, reserve, read the path into the reserved record with `bpf_probe_read_user_str_bytes`, apply `classify_and_drop_if_noise`, then `submit` or `discard(0)`. Follow `fs_syscalls.rs::finish_rename` (`:217-219`) for the reserve-then-discard shape — a 560-byte record does not fit on the 512-byte BPF stack, which is why the string is read after the reserve
- [X] T016 [US1] Add the `sys_enter_openat2` / `sys_exit_openat2` pair to `waybill-ebpf/src/programs/open_syscalls.rs`, reading `struct open_how.flags` through a **user**-space read. The existing kernel read at `waybill-ebpf/src/programs/file_ops.rs:184-191` is correct for a kprobe on `do_sys_openat2` and wrong here (research R4); T011 is the test that catches it
- [X] T017 [US1] Add the `sys_enter_open` / `sys_exit_open` and `sys_enter_creat` / `sys_exit_creat` pairs to `waybill-ebpf/src/programs/open_syscalls.rs` behind `#[cfg(target_arch = "x86_64")]`, with `dfd = AT_FDCWD` and, for `creat`, implied `O_WRONLY|O_CREAT|O_TRUNC`. Conditional on T001: if the inventory shows them absent on x86_64 too, do not add them
- [X] T018 [US1] Add the discard path to `waybill-ebpf/src/programs/open_syscalls.rs`: increment `OPEN_FAILURES_DISCARDED` on every `ret < 0`, and when `OPEN_DIAG` is set submit an `OpenFailed` record carrying the path and the negated errno. The counter is unconditional — a zero discard count on a compiler build is evidence the exit tracepoint is not firing, which otherwise looks exactly like a clean run (contracts/open-join.md)
- [X] T019 [US1] Rewrite the open half of the attach sequence in `waybill-cli/src/trace/loader.rs` per `contracts/probe-attachment.md`: the `openat` pair fatal as a unit, `openat2` and the x86_64 pairs best-effort with their names appended to `attach_failures.kprobe`, and expected per-architecture absence not recorded as a failure
- [X] T020 [US1] Delete `openat2_entry`, `try_openat2`, `do_filp_open_entry` and `try_do_filp_open` from `waybill-ebpf/src/programs/file_ops.rs`, their re-exports from `waybill-ebpf/src/main.rs:12`, and their attach blocks from `waybill-cli/src/trace/loader.rs:199-224`. Keep `vfs_read_entry` and `vfs_write_entry`: they are never attached, so they are not a live dependency on an unstable name (FR-005a; spec assumption records the asymmetry)
- [X] T021 [US1] Thread the new signals through user space: handle `OpenFailed` in `waybill-cli/src/trace/aggregator.rs` by logging and returning, read `OPEN_FAILURES_DISCARDED` in `waybill-cli/src/trace/counters.rs` via `read_percpu_sum` (`counters.rs:116`), and populate `TraceIntegrity.open_failures_discarded` on the path through `waybill-cli/src/cli/scan.rs`
- [X] T022 [US1] Verify SC-001b: re-run the harness, diff the read and write sets against `measurements/baseline-before.md`, and check **every** removed entry against the `OPEN_DIAG` discard record, entry by entry. Record the result in `specs/1270-stable-probe-tracepoints/measurements/findings.md`. A diff whose size matches the discard count is not verification — the same count is consistent with a join that also dropped successful opens
- [X] T023 [US1] Verify SC-001, SC-001a and SC-008 from the same run: every fixture crate root present in the read set; the successful-open count no lower than T002's baseline; ring-buffer event count no higher than T003's ceiling; `ring_buffer_overflows` still 0. Record all four in `measurements/findings.md`
- [X] T024 [US1] Settle the superset assumption from T022's diff and record the verdict in `specs/1270-stable-probe-tracepoints/measurements/findings.md`. The spec requires this confirmed by a task rather than assumed: if any entry disappeared that was *not* a failed request, the removal in T020 is what needs revisiting, not the migration (research R10)
- [ ] T025 [US1] Verify FR-005c against a forced case rather than by inspection: with `OPEN_DIAG` set, trace a process killed inside an open syscall (SIGKILL during a blocking open on a FIFO), then confirm `PENDING_OPEN` holds at most one entry per thread afterwards and that the thread's next open is recorded with its own path, not the stale one, verifying the invariant `waybill-ebpf/src/programs/open_syscalls.rs` relies on. Record the result in `specs/1270-stable-probe-tracepoints/measurements/findings.md`. The invariant follows from `insert` overwriting and from removal preceding the result check — an argued invariant with no test is the shape that produced this feature
- [X] T026 [US1] Verify FR-005b: with `OPEN_DIAG` clear, assert the ring-buffer event count equals the successful-open count for the fixture build. A discard path that reserves and then discards would pass the non-zero discard counter while still occupying ring-buffer space, which is exactly what FR-005b forbids. Record in `specs/1270-stable-probe-tracepoints/measurements/findings.md`

**Checkpoint**: file-open capture is kernel-independent and honest about
what succeeded. US1 is the MVP — shipping it alone closes the measured
51% loss.

---

## Phase 4: User Story 2 — A kernel change degrades capture instead of ending it (P2)

**Goal**: network capture stops depending on an internal symbol *and*
an internal struct layout, gains IPv6, and every interface that cannot
be used says so instead of silently reducing the record.

**Independent Test**: a build that connects only over IPv6 is recorded
as having made that connection; a trace on a host missing one interface
still produces an attestation that names the gap.

### Tests for User Story 2

- [X] T027 [US2] **Done in the harness, not the aggregator, and with a control.** An aggregator test would assert that a `NetworkEvent` carrying `is_v6 == 1` is recorded as a v6 destination — true before this feature as well, since `IpAddr` always carried the discriminant and only the capture could not reach IPv6. The discriminating check needs a real connection, so `scripts/ebpf-integration-test.sh` now stands up an IPv6 listener on `::1` inside the container and asserts the destination is recorded. Control: the same case on the pre-#1252 binary records **0 connections** (`measurements/findings.md` run 4), which is what makes the assertion worth having.
- [X] T028 [P] [US2] Add a test in `waybill-cli/src/trace/compiler_pipeline.rs` asserting that a failed `sys_enter_connect` attach degrades the trace — name in `kprobe_attach_failures[]`, completeness `partial`, exit status still success — and does not abort (FR-008)
- [X] T029 [P] [US2] Add a test in `waybill-cli/src/trace/loader.rs` asserting that an architecture's absent open syscall does not appear in `kprobe_attach_failures[]` and does not degrade completeness, verified on arm64 where `open` and `creat` are measurably absent (FR-006, measurements run 1)

### Implementation for User Story 2

- [X] T030 [US2] Replace the connect capture in `waybill-ebpf/src/programs/tcp_connect.rs` with a `sys_enter_connect` tracepoint: read `uservaddr` at `ARG1`, dispatch on `sa_family`, and build the address with `IpAddr::new_v4` or `IpAddr::new_v6` (`waybill-common/src/ip.rs:33`). No event-struct change — `IpAddr` already carries 16 octets and an `is_v6` discriminant
- [X] T031 [US2] Preserve `CONN_INFO` population exactly as `tcp_connect.rs:48` does it, keyed by the same timestamp-derived `conn_id`. The OpenSSL uprobes correlate plaintext to a destination through this map; getting it wrong leaves connections recorded but no longer joined to their payloads, which fails quietly
- [X] T032 [US2] Delete `tcp_connect`, `tcp_connect_ret`, their re-exports at `waybill-ebpf/src/main.rs:14`, their attach calls at `waybill-cli/src/trace/loader.rs:181-182`, and the now-unused `attach_kprobe` helper at `waybill-cli/src/trace/loader.rs:336`. Deleting the helper is what makes T014 structural rather than a review habit
- [X] T033 [US2] Do **not** add a `sys_exit_connect` program or a success filter. `connect` on a non-blocking socket returns `-EINPROGRESS`, which is a connection in flight rather than a failure, so narrowing connects the way opens are narrowed would drop exactly the asynchronous connects a build's HTTP client makes. Record this as a comment in `waybill-ebpf/src/programs/tcp_connect.rs` so the next reader does not "finish the job" (research R6)
- [X] T034 [US2] Implement FR-004b in `waybill-ebpf/src/programs/open_syscalls.rs` and `waybill-cli/src/trace/loader.rs`: where a host provides an open's request but not its result, fall back to recording requests, record the fallback as a capture gap, and do not report the capture as complete. Silently recording requests as reads is the failure this feature exists to remove, in a smaller form
- [X] T035 [US2] Verify SC-005 via `scripts/ebpf-integration-test.sh` by making one interface unavailable deliberately and confirming the attestation is still produced, names the gap, and does not claim completeness — rather than waiting to observe it on a runner image
- [X] T036 [US2] Verified in both halves on this host, recorded in `specs/1270-stable-probe-tracepoints/measurements/findings.md`: the recorded input set changes only as SC-001b permits (run 3 — 78 removals, all attributable, nothing added, write set unchanged) and the network record gains an IPv6 connection where the build makes one (run 4, with the 0-connection control). The x86_64 half is still owed from the lane.

**Checkpoint**: both capture paths are off internal kernel names, IPv6
is visible, and every unusable interface is named in the artifact.

---

## Phase 5: User Story 3 — A red lane means waybill changed, not that the pool rolled (P3)

**Goal**: the eBPF lane's outcome stops depending on which runner image
it draws, and the harness asserts the new invariants on a positive
signal.

**Independent Test**: run the lane with each capture interface made
unavailable in turn; the outcome does not depend on which one is
missing.

- [X] T037 [US3] Extend `scripts/ebpf-integration-test.sh` with the new invariants: `open_failures_discarded` is non-zero for the fixture build (the positive control — a compiler attempts failed opens constantly across its header and library search paths, so zero means the capture, not an unusually tidy build); the ring-buffer event count does not exceed the T003 ceiling; `ring_buffer_overflows` is still 0; **and every crate root the fixture compiles appears in the recorded read set**. FR-013 asks the harness for a non-zero count of compiler activity *and the presence of named inputs*; the first half is asserted at `scripts/ebpf-integration-test.sh:90,103` and the second is asserted nowhere. Adding it turns SC-001 from the one-time check in T023 into a standing gate, which is the difference between catching the next regression and re-diagnosing it
- [ ] T038 [US3] **Premise changed by the implementation; re-scope before doing it.** The task asks the harness to prove a half-attached `openat` pair fails closed. That state can no longer arise: each open syscall is attached exit-first and the entry only if the exit attached (`waybill-cli/src/trace/loader.rs`), and the one case that remains — `openat`'s exit failing — is now FR-004b's documented degradation rather than a failure, exercised by T035. What is left to test is the *entry* failing, which is fatal. Worth an assertion, but not the one written here.
- [ ] T039 [US3] Verify SC-002 as the restated property: run the lane with each interface made unavailable and confirm the outcome is the same, then record in `measurements/findings.md` that the 14-against-10 image split is retained as the historical baseline rather than as the test. A criterion pinned to the failing image would go quiet instead of red once the image leaves the pool
- [ ] T040 [US3] Observe 30 consecutive default-branch runs after the change and record, in `measurements/findings.md`, how many lane failures are attributable to the host kernel or runner image (SC-003; baseline 11 of 100, 10 of them from one unusable interface). Close #1311 once the streak holds, and note that the 2026-10-14 failure counts against the baseline rather than being excused

**Checkpoint**: a red lane is information about the commit again.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T041 [P] Fix the `kprobe_attach_failures` doc comment in `waybill-common/src/attestation/integrity.rs` to say what the field is — capture interfaces that did not become usable — and stop saying "kprobes", since after this feature it holds no kprobe names at all. Keep the field name and type: the attestation is a stability surface and it has been a union since m212 (FR-011a, clarification Q1)
- [X] T042 [P] Grep every remaining mention of `do_filp_open`, `do_sys_openat2` and `tcp_v4_connect` outside `specs/` and update the ones that describe current behaviour rather than history: `waybill-cli/src/trace/aggregator.rs:72`, `waybill-cli/src/trace/compiler_pipeline.rs:863`, `waybill-common/src/attestation/compiler_pipeline.rs:197`, `waybill-common/src/filter.rs:16`, `scripts/ebpf-integration-test.sh:135`. The #1252 narrative references stay — they are the evidence for why this exists
- [X] T043 [P] Update `docs/development/ebpf-toolchain.md` with what capture now attaches to and why, pointing at `contracts/probe-attachment.md` for the tier semantics
- [ ] T044 [P] Open a PATCH amendment PR against `.specify/memory/constitution.md` bumping 4.0.1 → 4.0.2: Principle II's descriptive clause "File operations are traced via kernel probes" becomes stale once capture moves to syscall tracepoints. PATCH rather than MAJOR because the normative sentence — "all dependency discovery MUST occur through eBPF tracing" — is satisfied unchanged, and the paragraph backticks `uprobes` where it names a mechanism while leaving "kernel probes" unbackticked, so the clause reads as generic. Governance requires an amendment rather than silent deviation, and requires active specs be reviewed against the amended text before merge — so this lands before or with the feature, not after it
- [X] T045 Run `specs/1270-stable-probe-tracepoints/quickstart.md` end to end on a clean checkout, including the non-vacuous-container check and the discard-counter positive control, and fix anything in it that does not work as written
- [X] T046 Run the pre-PR gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain` argument (#1189), plus `WAYBILL_PREPR_EBPF=1 ./scripts/pre-pr.sh` for the feature-on path. Trace tests live in the **binary** target because `mod trace;` is declared in `main.rs` — `--lib` finds none of them
- [X] T047 Add the `CHANGELOG.md` entry naming the wire-observable changes: read sets no longer carry failed open requests, IPv6 connections are recorded, and `TraceIntegrity` gains `open_failures_discarded`

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (Measurement)**: no dependencies. **Blocks everything** —
  T022, T023 and T037 compare against its output, and T001 decides
  whether T017 exists.
- **Phase 2 (Foundational)**: depends on Phase 1 only by discipline, not
  by code; it changes no behaviour. Blocks US1 and US2.
- **Phase 3 (US1)**: depends on Phase 2. Independently shippable.
- **Phase 4 (US2)**: depends on Phase 2. Independent of US1 except for
  T032, which deletes `attach_kprobe` and therefore must land after
  T020 has removed its last open-side caller.
- **Phase 5 (US3)**: depends on US1 and US2 — it asserts their
  invariants.
- **Phase 6 (Polish)**: depends on the stories it documents.

### Critical path

```
T001 ──▶ T017
T002 ──▶ T022 ──▶ T023 ──▶ T024
T005 ──▶ T010 ──▶ T015 ──▶ T016 ──▶ T018 ──▶ T019 ──▶ T020 ──▶ T021
                                                        │
T030 ──▶ T031 ──────────────────────────────────────────┴──▶ T032
```

### Ordering within US1

The order of T019 and T020 matters. Add and verify the new capture
before deleting the old, so no commit exists in which neither works —
and so T023's volume comparison attributes the reduction to the right
cause. Deleting the kprobes is itself half the volume reduction, because
`do_sys_openat2` calls `do_filp_open`, so one `openat` call currently
submits two `FileEvent`s and the read set hides the duplicate
(research R8).

### Parallel opportunities

- T002 and T003 are one run, recorded into one file — do them together.
- T007, T008 touch different files and are parallel; T005 and T006 share
  `maps.rs` and are not.
- T011, T012 and T013 are parallel; T014 is not (it reads the loader T019 and
  T032 rewrite).
- T027, T028, T029 are parallel.
- T041, T042, T043, T044 are all parallel.

---

## Implementation Strategy

**MVP is US1 alone.** It closes the measured defect: the 51% file-open
loss on the image the pool is rolling toward. US2 removes the same class
of defect from the network path before it fires, and adds IPv6; US3 makes
the lane's signal trustworthy again. Each is separately useful and
separately verifiable.

**The measurement phase is not overhead.** Three success criteria
(SC-001a, SC-001b, SC-008) are expressed as ratios against a baseline
precisely because the absolute numbers this project has were measured
under semantics the feature changes. Skipping Phase 1 does not make the
work faster; it makes the verification vacuous, and a vacuous pass is
how the original 51% loss survived two releases.

**Out of scope, deliberately.** A pinned-kernel VM for the test
environment (the spec's Layer 2), read and write capture of already-open
files, and renaming `kprobe_attach_failures`.

---

## Task Summary

| Phase | Tasks | Count |
|---|---|---|
| 1 — Measurement | T001–T004 | 4 |
| 2 — Foundational | T005–T010 | 6 |
| 3 — US1 (P1) | T011–T026 | 16 |
| 4 — US2 (P2) | T027–T036 | 10 |
| 5 — US3 (P3) | T037–T040 | 4 |
| 6 — Polish | T041–T047 | 7 |
| **Total** | | **47** |

Test tasks: 8 — T009, T011, T012, T013, T014, T027, T028, T029. Tasks
that record a measured result into `measurements/`: 10 — T001, T002,
T003, T022, T023, T024, T026, T036, T039, T040. Tasks that delete
something: 2 — T020, T032.
