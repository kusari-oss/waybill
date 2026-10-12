# Tasks: Close-time facts for compiler read and write sets

**Input**: Design documents from `/specs/1072-compiler-set-close-time/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: INCLUDED. Not an optional extra here — Constitution Principle VII
requires unprivileged unit coverage of serialization logic, and every user
story in `spec.md` carries an "Independent Test" plus acceptance scenarios.
Research R6's un-gated module is what makes most of them runnable in the
default CI lane.

**Organization**: by user story, in spec priority order.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on incomplete work)
- **[Story]**: US1–US4 from `spec.md`
- Every task names its file path

## Two sequencing notes

**The measurement sits at the head of US4, not Phase 1.** `plan.md`'s
closing section says to sequence "the measurement" first, which read
literally is impossible: the pass's cost cannot be measured before the pass
exists. The resolution is that nothing before US4 needs a number — US1–US3
run under the existing 256 MB per-file cap with no cumulative budget, which
`spec.md` itself calls correct-but-unbounded in US4's priority rationale. So
T034 measures the baseline, T035 measures the pass, and only then does
T036 set the default constant. No task before T034 contains a byte figure.

**Phase 2 is unusually large because the type change breaks compilation
everywhere.** Removing `ReadSetEntry::sha256` and renaming
`survived_trace_window` stops the workspace building until every consumer is
updated. That cascade is a single indivisible step, and it is also what
enumerates the work — so it is foundational rather than split across
stories.

---

## Phase 1: Setup

**Purpose**: the one new file, registered so later tasks have somewhere to land.

- [ ] T001 Create `waybill-cli/src/trace/close_time.rs` with a module doc comment stating that it is deliberately NOT cfg-gated (research R6: it contains no eBPF, so gating it would hide the feature from macOS `cargo check` and the default CI lane) and that only the call site in `scan.rs` is gated
- [ ] T002 Register `pub mod close_time;` in `waybill-cli/src/trace/mod.rs` in alphabetical position (between `aggregator` at line 13 and `compiler_pipeline` at line 14)

No Cargo.toml change: `tempfile = "3"` is already both a dependency and a
dev-dependency of `waybill-cli` (`waybill-cli/Cargo.toml:101,214`), so
FR-020's zero-new-dependency rule needs no action.

**Checkpoint**: `cargo +stable clippy --workspace --all-targets` still clean.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the wire shapes, the aggregator's in-flight maps, and the
stat-only pass. After this phase the 64-zero hash and the hardcoded
`survived_trace_window: true` are both **gone by construction** — FR-006 is
satisfied by type removal, not by a runtime check.

**⚠️ CRITICAL**: T003–T012 are one compilation cascade. The tree does not
build between T005 and T011; do not treat an intermediate failure as a
regression.

- [ ] T003 [P] Add `CompilerHashCoverage` and `BudgetSource` (data-model E3) to `waybill-common/src/attestation/integrity.rs` — four `u64` counts, `truncated: bool`, `budget_source: BudgetSource`, with `#[serde(rename_all = "snake_case")]` on the two-variant enum
- [ ] T004 Add `compiler_hash_coverage: Option<CompilerHashCoverage>` as the LAST field of `TraceIntegrity` in `waybill-common/src/attestation/integrity.rs` with `#[serde(default, skip_serializing_if = "Option::is_none")]`, following the m1070 `unresolved_relative_opens` precedent two lines above (data-model E4)
- [ ] T005 Replace `ReadSetEntry` and `WriteSetEntry` in `waybill-common/src/attestation/compiler_pipeline.rs:81-101` with the data-model E1/E2 shapes: drop `sha256` from both, drop `survived_trace_window`, add `renamed_from: Vec<PathBuf>` (write only, `skip_serializing_if = "Vec::is_empty"`), add `existed_at_close: bool` and `sha256_at_close: Option<ContentHash>` to both, and rewrite the stale doc comments that claim `sha256` is `None` for deleted intermediates
- [ ] T006 Bump both attestor URI constants to `compiler-invocation/v0.2` (FR-017) — `waybill-common/src/attestation/compiler_pipeline.rs:216` and `waybill-common/src/attestation/witness.rs:57` — and their two asserting tests at `compiler_pipeline.rs:399` and `witness.rs:568`. Research R8: the constant is duplicated; bump both, do not refactor
- [ ] T007 Change `PartialInvocation.read_set` to `HashMap<PathBuf, ReadKind>` and `write_set` to `HashMap<PathBuf, PendingWrite>` in `waybill-cli/src/trace/compiler_pipeline.rs:114-116`, adding `struct PendingWrite { renamed_from: Vec<PathBuf> }` (data-model E5) so neither map has a field a kernel hash could occupy
- [ ] T008 Update `record_file_op`'s three match arms at `waybill-cli/src/trace/compiler_pipeline.rs:235-250` for the new maps, dropping the `content_hash_from_bytes` call for these two sets (the kernel's zero hash no longer reaches them) and leaving the `Rename` arm's carry behaviour as-is for now — accumulation is T024
- [ ] T009 Update `finalize_invocation` at `waybill-cli/src/trace/compiler_pipeline.rs:508-532` to build the new entry shapes with `existed_at_close: false` and `sha256_at_close: None` as the pre-pass defaults, preserving the existing lex sort on path (R8 determinism)
- [ ] T010 Remove the 64-zero sentinel from `record_stdin_input` in `waybill-cli/src/trace/compiler_pipeline.rs` (FR-007) — the entry becomes path `<stdin>`, `kind: StdinInput { bytes_read }`, `existed_at_close: false`, no hash
- [ ] T011 Update the C130 payload builder at `waybill-cli/src/generate/compiler_pipeline_annotation.rs:129` to emit `"sha256_at_close"` instead of `"sha256"`, omitted when absent (FR-019, research R9). Emission wiring stays #1142's scope — this only keeps the dormant builder compiling and honestly labelled
- [ ] T012 Fix the in-tree test constructors the type change breaks: `write_set_entry_omits_none_sha256` and the `zero_sha256()` helper at `waybill-common/src/attestation/compiler_pipeline.rs:225,340-360`, and the fixture builder at `waybill-cli/src/generate/compiler_pipeline_annotation.rs:291-302`. Delete `zero_sha256()` outright — FR-006 means no test should be able to construct the value
- [ ] T013 Implement the distinct-path collection and fan-out core of `apply_close_time_facts` in `waybill-cli/src/trace/close_time.rs` (data-model E6 steps 1 and 3): union of write-set paths then read-set paths, each lex-sorted, a path in both visited once in its write-set position, `StdinInput` entries excluded from the walk entirely (contract C-1072-2a), and the per-path result fanned out to every entry naming that path across all invocations
- [ ] T014 Add the stat half of the pass in `waybill-cli/src/trace/close_time.rs` (E6 step 2, no hashing yet): `symlink_metadata` → absent sets `existed_at_close: false` and increments `paths_absent`; present-but-not-a-regular-file sets `existed_at_close: true` and increments `paths_skipped`; present regular files set `existed_at_close: true`. Return a `CompilerHashCoverage` with `paths_hashed: 0`
- [ ] T015 [P] Add the tempdir test scaffold in `waybill-cli/src/trace/close_time.rs` — a helper building a synthetic `CompilerPipelineData` from `(path, kind)` pairs under `tempfile::tempdir()`, inside `#[cfg(test)] #[cfg_attr(test, allow(clippy::unwrap_used))] mod tests` per the crate convention
- [ ] T016 [P] Unit-test the count partition invariant `paths_hashed + paths_absent + paths_skipped == paths_total` (FR-013) in `waybill-cli/src/trace/close_time.rs` across the stat-only cases: missing file, directory at a write path, dangling symlink, regular file
- [ ] T017 [P] Unit-test that the pass never adds, removes or reorders an entry and never alters `path`, `kind` or `renamed_from` (FR-010, contract C-1072-7, Principle II) in `waybill-cli/src/trace/close_time.rs` — snapshot the observation tier before and after and assert equality
- [ ] T018 [P] Unit-test that a `StdinInput` entry is not stat'd (C-1072-2a) in `waybill-cli/src/trace/close_time.rs`: create a real file literally named `<stdin>` in the tempdir, set it as the CWD, and assert the entry still reports `existed_at_close: false` and contributes to none of the four counts
- [ ] T019 Wire the pass into `waybill-cli/src/cli/scan.rs` between `compiler_agg.finalize()` (line 766) and `builder::build_attestation(...)` (line 789), keeping the existing `#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]` gate on the call site, and change `let trace` at `waybill-cli/src/cli/scan.rs:652` to `let mut trace` so the coverage record can be assigned (research R5)

**Checkpoint**: tree builds; `./scripts/pre-pr.sh` clean; no all-zero hash
is constructible anywhere in the workspace; `existed_at_close` is a real
`symlink_metadata` result rather than a constant.

---

## Phase 3: User Story 1 — A deleted intermediate is reported as deleted (Priority: P1) 🎯 MVP

**Goal**: rustc temporaries report absent; surviving products report present
with a true content hash. This is #1144's stated defect.

**Independent Test**: trace `two_binaries_diverge` on a real kernel and
confirm `deps/rmeta*/full.rmeta` reports `existed_at_close: false` with no
hash while the final `.rlib`/binary reports `true` with a hash matching an
independent `sha256sum`.

- [ ] T020 [US1] Add the hashing half of the pass in `waybill-cli/src/trace/close_time.rs`: for a present regular file call `crate::trace::hasher::sha256_file_hex(path, DEFAULT_SIZE_CAP_BYTES)`, set `sha256_at_close` on `Ok`, and on `Err` increment `paths_skipped` with a WARN naming the path and error — never `.unwrap()` (Boundary 4). No cumulative budget yet; US4 adds it
- [ ] T021 [P] [US1] Unit-test the over-cap path in `waybill-cli/src/trace/close_time.rs`: pass a deliberately tiny per-file cap and assert the file is counted in `paths_skipped` with `existed_at_close: true` and no hash — research R2's correction, since `sha256_file_hex` returns `Err(InvalidData)` rather than hashing a prefix
- [ ] T022 [P] [US1] Unit-test an unreadable file in `waybill-cli/src/trace/close_time.rs` via `set_permissions(0o000)`, asserting `existed_at_close: true`, no hash, counted in `paths_skipped`, and that the pass completes rather than aborting. Skip the assertion when running as root, where the mode is not enforced
- [ ] T023 [US1] Add the kernel integration test for US1's three acceptance scenarios in `waybill-cli/tests/compiler_pipeline_close_time.rs`, gated `#![cfg(all(target_os = "linux", feature = "ebpf-tracing"))]` per the m210 convention: at least one write-set entry reports `existed_at_close: false` (SC-001's positive control — a zero here is a FAILURE, indistinguishable from the pass never running), zero entries carry an all-zero hash (SC-002), and ten sampled `sha256_at_close` values match an independent `sha256sum` (SC-003)

**Checkpoint**: US1 verifiable on a real kernel. #1144's headline defect is
fixed and the fix is proven to have run.

---

## Phase 4: User Story 2 — A read-set entry never claims a hash it does not have (Priority: P1)

**Goal**: no read-set entry carries a `sha256` field; any hash it carries is
explicitly close-time.

**Independent Test**: deserialize an attestation from a real trace and
confirm no read-set entry has a `sha256` field, any `sha256_at_close`
matches the file's current content, and a read path deleted during the build
reports `existed_at_close: false` with no hash.

Most of US2 is delivered by the Phase 2 type change — `sha256` cannot be
emitted because the field no longer exists. These tasks prove it and cover
the `<stdin>` case.

- [ ] T024 [P] [US2] Unit-test in `waybill-common/src/attestation/compiler_pipeline.rs` that a serialized `ReadSetEntry` contains no `"sha256"` key (substring assertion, so `sha256_at_close` does not satisfy it — assert on `"\"sha256\":"`), and that `sha256_at_close` is omitted rather than null when absent
- [ ] T025 [P] [US2] Unit-test in `waybill-cli/src/trace/compiler_pipeline.rs` that `record_stdin_input` produces an entry with `existed_at_close: false`, no `sha256_at_close`, and no 64-zero value anywhere in the serialized invocation (FR-007, US2 scenario 2)
- [ ] T026 [P] [US2] Unit-test a read path deleted before close in `waybill-cli/src/trace/close_time.rs`: the entry reports `existed_at_close: false` and is still present in the read set rather than omitted (US2 scenario 3, Principle VIII)
- [ ] T027 [US2] Extend `waybill-cli/tests/compiler_pipeline_close_time.rs` with US2's kernel assertions: no read-set entry carries `sha256`, and every `sha256_at_close` present in a read set matches an independent `sha256sum`
- [ ] T028 [US2] Add the shared-path assertion for SC-010 in `waybill-cli/tests/compiler_pipeline_close_time.rs`: every pair of entries naming one path agrees on both close-time fields, and `paths_total` is strictly less than the total entry count on this fixture because `deps/liblibsafe-<hash>.rlib` is read under three distinct rustc invocations (`measurements/findings.md` Q3)

**Checkpoint**: both P1 stories complete. The predicate can no longer state
a hash it does not have, in either set.

---

## Phase 5: User Story 3 — A renamed artifact is one entry with its history (Priority: P2)

**Goal**: an artifact rustc wrote under a temporary name and renamed into
place is one entry at the final path, carrying the intermediate names.

**Independent Test**: trace a build with a rename and confirm exactly one
entry for the artifact, keyed by the final path, whose `renamed_from` lists
the intermediate, which also appears in the raw `file_access` events.

- [ ] T029 [US3] Change the `ObservedKind::Rename` arm at `waybill-cli/src/trace/compiler_pipeline.rs:243-250` from carry-and-replace to accumulation (data-model E5): take the source's `PendingWrite` if one exists, else `PendingWrite::default()`, push the observed source onto `renamed_from`, and insert at the destination path
- [ ] T030 [P] [US3] Unit-test the three rename shapes in `waybill-cli/src/trace/compiler_pipeline.rs`: single rename yields one entry with `renamed_from: [tmp]` and no entry at `tmp` (FR-009); a two-step chain lists both intermediates oldest-first (US3 scenario 2); a write with no rename serializes without the key at all (US3 scenario 3)
- [ ] T031 [P] [US3] Unit-test the two rename edge cases from `spec.md` in `waybill-cli/src/trace/compiler_pipeline.rs`: a rename whose source was never observed as a write still yields a destination entry recording that source, and a rename onto an existing write-set entry keeps the final path while accumulating both histories in observed order
- [ ] T032 [P] [US3] Add the FR-009a regression test in `waybill-cli/src/trace/compiler_pipeline.rs` asserting `ObservedKind` has exactly three variants and that no code path synthesises a `Rename` from a link. Research R4: `linkat` is traced nowhere, so this invariant currently holds by construction — the test's job is to fail loudly if a future link probe routes through this arm
- [ ] T033 [US3] Extend `waybill-cli/tests/compiler_pipeline_close_time.rs` with SC-004: every entry for an artifact rustc renamed into place has a non-empty `renamed_from`, and no entry's `path` appears in another entry's `renamed_from`

**Checkpoint**: rename-into-place — rustc's normal pattern for libraries,
per `measurements/findings.md` Q2 — is recorded losslessly.

---

## Phase 6: User Story 4 — An operator can see how much was hashed (Priority: P2)

**Goal**: coverage and truncation are disclosed, and the budget is bounded
and operator-overridable.

**Independent Test**: trace with the budget below the build's hashable
bytes; confirm `truncated: true` with `paths_hashed < paths_total`, a WARN
naming the exhaustion, and exit zero with the attestation written.

**The two measurement tasks gate the default constant.** No byte figure may
appear in code before T035.

- [ ] T034 [US4] Measure the baseline in Colima and record it under `specs/1072-compiler-set-close-time/measurements/cost.md`: total `waybill trace run` wall-clock for `two_binaries_diverge` at the US3 checkpoint, plus the distinct-path count and total hashable bytes the pass would face. This is SC-008's denominator and must be captured before the pass is budgeted
- [ ] T035 [US4] Measure the pass itself in Colima and append to `measurements/cost.md`: per-run wall-clock of `apply_close_time_facts` against that same trace, as a ratio of total trace wall-clock, over at least three runs with the fixture's `target/` cleaned between them. State the kernel, arch and fixture, as `measurements/findings.md` does
- [ ] T036 [US4] Define the default budget constant in `waybill-cli/src/cli/scan.rs` with a doc comment citing `measurements/cost.md` for the figure (contract C-1072-9), and add the `compiler_hash_budget_bytes: Option<u64>` flag to `ScanArgs` with `#[arg(long, env = "WAYBILL_COMPILER_HASH_BUDGET_BYTES", value_name = "BYTES")]` — `None` → default + `BudgetSource::Default`, `Some(_)` → `BudgetSource::Operator` (C-1072-8)
- [ ] T037 [US4] Add the cumulative-bytes budget to `apply_close_time_facts` in `waybill-cli/src/trace/close_time.rs` (FR-011a, FR-012): a path whose size exceeds the remaining budget is skipped, counted in `paths_skipped`, sets `truncated: true`, and the pass CONTINUES to the next path, so one oversized artifact early in sort order does not blind the pass to everything after it. A shared path is charged once, at its first position
- [ ] T038 [US4] Attach the coverage record to the attestation in `waybill-cli/src/cli/scan.rs`: assign `trace.trace_integrity.compiler_hash_coverage = Some(coverage)` after the pass, and leave it `None` when the trace captured zero compiler invocations so pre-m1072 byte-identity holds (FR-014)
- [ ] T039 [US4] Emit the exhaustion WARN in `waybill-cli/src/trace/close_time.rs` naming hashed/skipped/absent/total and the budget, exactly once per trace, and confirm the exit status stays zero (FR-015, C-1072-10)
- [ ] T040 [P] [US4] Unit-test truncation determinism in `waybill-cli/src/trace/close_time.rs`: with `Budget { total_bytes: 0 }` every existing path lands in `paths_skipped` with `truncated: true`; and with a budget that cuts mid-way, two runs over the same synthetic input hash the identical path set (FR-012, SC-006)
- [ ] T041 [P] [US4] Unit-test `budget_source` reporting in `waybill-cli/src/trace/close_time.rs` — `Default` when the flag is absent, `Operator` when supplied — and that `TraceIntegrity` with `compiler_hash_coverage: None` serializes without the key (SC-007, FR-014)
- [ ] T042 [US4] Extend `waybill-cli/tests/compiler_pipeline_close_time.rs` with US4's kernel scenarios: `--compiler-hash-budget-bytes 0` yields `truncated: true`, `paths_hashed: 0`, `budget_source: "operator"`, a satisfied count partition, exit zero and a written attestation (SC-006)
- [ ] T043 [US4] Add the determinism test for SC-005 in `waybill-cli/tests/compiler_pipeline_close_time.rs`: two consecutive traces of the same build produce byte-identical pre-signature canonical payloads including the coverage record, with the fixture's `target/` cleaned between runs so the second build is not a vacuous no-op

**Checkpoint**: all four stories complete. Partial hash coverage can no
longer be mistaken for a clean result.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T044 [P] Verify SC-007 end to end against `waybill-common/src/attestation/integrity.rs`: an attestation from a trace with zero compiler invocations has a `trace_integrity` byte-identical to one produced before this feature, by diffing against an attestation generated at the pre-m1072 commit `3a385042`
- [ ] T045 [P] Update `docs/architecture/attestations.md` for the `compiler-invocation/v0.2` attestor and the two-tier read/write set shape, pointing at `contracts/read-write-set-shape.md` as the normative source
- [ ] T046 [P] Add a pointer at the top of `specs/210-compiler-pipeline-trace/contracts/attestor-predicate.md` noting that C-1 and C-2 are superseded for these two entities by `specs/1072-compiler-set-close-time/contracts/read-write-set-shape.md`, without editing the historical clauses
- [ ] T047 Run the full `quickstart.md` verification block in Colima — the four `jq` checks, the determinism diff and the truncation run — and record the output in `measurements/cost.md` alongside the timings
- [ ] T048 Run `./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo "EXIT=$?"` and assert on the POSITIVE signals: the `>>> all pre-PR checks passed.` line and every per-target `test result: ok. N passed; 0 failed`. The absence of FAILED is not evidence — a build that never compiled also contains no failures
- [ ] T049 Run the `ebpf-tracing` feature-on lane in a Linux container (`cargo run -p xtask -- ebpf` then `cargo +stable test --workspace --no-fail-fast --features ebpf-tracing`) and confirm the gated integration tests actually executed rather than compiling as empty modules (SC-009)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies.
- **Foundational (Phase 2)**: depends on Setup. **Blocks every story.** T003–T012 are one compilation cascade and must land together.
- **US1 (Phase 3)**: depends on Phase 2. Delivers the MVP.
- **US2 (Phase 4)**: depends on Phase 2. Largely *verifies* Phase 2's type change; independent of US1 except that both extend the same integration-test file.
- **US3 (Phase 5)**: depends on Phase 2 only. Touches `compiler_pipeline.rs`'s rename arm, which no other story modifies — the most independent story of the four.
- **US4 (Phase 6)**: depends on US1 (it budgets the hashing US1 adds).
- **Polish (Phase 7)**: depends on all four.

### Story Dependencies

- US1 → none beyond Foundational.
- US2 → none beyond Foundational. Shares `waybill-cli/tests/compiler_pipeline_close_time.rs` with US1, so T027/T028 follow T023 to avoid a merge conflict in one file.
- US3 → none beyond Foundational. Fully parallel with US1 and US2.
- US4 → **US1** (T037 budgets the hash call T020 adds).

### Within Each Story

Implementation precedes its unit tests here rather than the reverse. The
breaking type change makes a test-first ordering impossible to compile for
T003–T012, and `spec.md`'s acceptance scenarios are the test specification
those tests transcribe. The kernel integration test for each story comes
last in its phase, because it can only run in Colima.

### Parallel Opportunities

- T015–T018 (four independent unit tests, same file but disjoint functions) after T014.
- T021, T022 after T020.
- T024, T025, T026 — three different files, fully parallel.
- T030, T031, T032 after T029.
- T040, T041 after T037.
- T044, T045, T046 — docs and verification, fully parallel.
- **US1, US2 and US3 can be worked in parallel by three people** once Phase 2 lands; US4 waits on US1.

---

## Parallel Example: after the Phase 2 checkpoint

```bash
# Three independent tracks, no shared files except the integration test:
#  track A (US1): T020 → T021, T022 → T023
#  track B (US2): T024, T025, T026 in parallel → T027, T028
#  track C (US3): T029 → T030, T031, T032 → T033
```

---

## Implementation Strategy

**MVP = Phase 1 + Phase 2 + Phase 3 (US1).** That is 23 tasks and it fixes
#1144's stated defect: no 64-zero hashes, and `existed_at_close` derived
from the filesystem instead of hardcoded. It is independently shippable —
the pass is unbudgeted at that point, which `spec.md` calls
correct-but-unbounded.

**Incremental delivery**: US2 is cheap after Phase 2 (mostly assertions on a
change already made). US3 is independent and can land in any order. US4 is
last because it is the only story that needs a measured number, and
measuring it needs the pass US1 delivers.

**What needs Colima**: T023, T027, T028, T033, T034, T035, T042, T043,
T044, T047, T049. Everything else runs on the dev host, which is research
R6's whole purpose — macOS `cargo check` is blind to the gated call site,
so a design that put the pass behind the gate would have made 30 of these
49 tasks unverifiable locally.

**Experimental posture**: the tracing subsystem is feature-gated and
pre-stable (`spec.md`, "Maturity, and what happens if this is wrong"). If
the two-tier shape, the byte budget or the once-per-path rule proves
ineffective, append the finding to `measurements/findings.md` rather than
reverting silently.
