# Tasks: Compiler-Pipeline eBPF Tracing (m210)

**Branch**: `210-compiler-pipeline-trace`
**Feature**: [spec.md](./spec.md) | [plan.md](./plan.md)

## Task Format

Each task follows: `- [ ] T### [P?] [Story?] Description with file path`. `[P]` = parallelizable (no dependency on incomplete sibling task; different file). `[US#]` = maps to a user story (US1 = source-attribution; US2 = reproducibility; US3 = downstream reachability).

## Phase 1: Setup

- [X] T001 Create scaffold file `mikebom-common/src/attestation/compiler_pipeline.rs` with top-of-file doc-comment naming milestone 210 + citing data-model E1..E6 + E14
- [ ] T002 Create scaffold file `mikebom-ebpf/src/programs/compiler_exec.rs` with top-of-file doc-comment naming milestone 210 + citing research R1 (sched_process_exec tracepoint choice) — **DEFERRED to Linux session; requires nightly + bpf-linker**
- [ ] T003 Create scaffold file `mikebom-cli/src/trace/compiler_pipeline.rs` with top-of-file doc-comment naming milestone 210 + citing plan.md's user-space aggregator role — **DEFERRED to Linux session** (compiles on macOS but has no testable behavior without the eBPF side)
- [X] T004 Register new modules — DONE for `mikebom-common/src/attestation/mod.rs` (`pub mod compiler_pipeline;` added). mikebom-ebpf + mikebom-cli/src/trace registrations DEFERRED to Linux session (files not yet created)
- [X] T005 Create fixture-project directory scaffold at `mikebom-cli/tests/fixtures/compiler_pipeline/` — DONE; `two_binaries_diverge/` fully populated (see T015); `secrets_touch/` + `stdin_input/` directories NOT created (see T016 + T017 deferral notes). Comprehensive `REGEN.md` documents which fixtures land this session vs the Linux follow-up

## Phase 2: Foundational (blocking prerequisites)

### Types (mikebom-common)

- [X] T006 [P] Define `CompilerFamily` enum in `mikebom-common/src/attestation/compiler_pipeline.rs` per data-model E3 — 11 variants (`Rustc`, `Gcc`, `Clang`, `Gpp`, `Go`, `Ld`, `Mold`, `Cc1`, `Cpp`, `As`, `Unknown`) with `#[serde(rename_all = "snake_case")]`, `Copy`, `PartialEq`, `Eq`, `Debug`, `Serialize`, `Deserialize` derives
- [X] T007 [P] Define `ReadKind` enum in `mikebom-common/src/attestation/compiler_pipeline.rs` per data-model E4 — 2 variants (`File`, `StdinInput { bytes_read: u64 }`) with snake_case serde rename
- [X] T008 Define `ReadSetEntry` + `WriteSetEntry` structs in `mikebom-common/src/attestation/compiler_pipeline.rs` per data-model E4 + E5 — `ReadSetEntry { path: PathBuf, sha256: ContentHash, kind: ReadKind }`, `WriteSetEntry { path: PathBuf, sha256: Option<ContentHash>, survived_trace_window: bool }`; both `Serialize + Deserialize + PartialEq + Debug + Clone`
- [X] T009 Define `CompilerInvocation` struct in `mikebom-common/src/attestation/compiler_pipeline.rs` per data-model E2 — all fields per E2 spec (`invocation_id: u64`, `compiler: CompilerFamily`, `pid: u32`, `ppid: u32`, `parent_invocation_id: Option<u64>`, `cgroup_id: u64`, `start_timestamp: Timestamp`, `end_timestamp: Option<Timestamp>`, `argv_full_path: Option<PathBuf>`, `argv: Vec<String>`, `cwd: Option<PathBuf>`, `exit_code: Option<i32>`, `read_set: Vec<ReadSetEntry>`, `write_set: Vec<WriteSetEntry>`, `events_dropped: u64`)
- [X] T010 Define `CompletenessState` + `PartialReason` + `InvocationDagEdge` + `FilterCategory` types in `mikebom-common/src/attestation/compiler_pipeline.rs` per data-model E6 — `CompletenessState` is a serde-tagged enum with 3 variants, `PartialReason` has one variant `AttachLate`, `InvocationDagEdge { parent_invocation_id, child_invocation_id }`, `FilterCategory` is a snake-case enum with 4 variants
- [X] T011 Define `CompilerPipelineData` struct in `mikebom-common/src/attestation/compiler_pipeline.rs` per data-model E6 — all fields per E6 spec + unit tests verifying serde roundtrip (empty pipeline data, one-invocation pipeline data, degraded-completeness pipeline data)
- [X] T012 Extend `BuildTracePredicate` in `mikebom-common/src/attestation/statement.rs` per data-model E7 — add `pub compiler_pipeline: Option<CompilerPipelineData>` field with `#[serde(skip_serializing_if = "Option::is_none")]` attribute + milestone-210 doc-comment citing R6 backward-compat rationale
- [X] T013 Update EVERY `BuildTracePredicate` construction site to explicitly set `compiler_pipeline: None` — locate via `grep -rn "BuildTracePredicate {" mikebom-common/ mikebom-cli/ mikebom-ebpf/` (expect ~5 test-only sites + 1 production site in `trace_cmd.rs`). Preserves pre-m210 golden byte-identity per m208 defensive-default pattern

### eBPF maps (mikebom-ebpf)

- [ ] T014 Extend `mikebom-ebpf/src/maps.rs` per data-model E8 — add `COMPILER_INVOCATIONS: HashMap<u32, u64>` (max 4096 entries) + `COMPILER_EXEC_EVENTS: RingBuf` (256 KB). Both with `#[map]` attribute + milestone-210 doc-comments referencing research R3 + R7 — **DEFERRED to Linux session**

### Fixture projects

- [X] T015 [P] Vendor the SC-001 fixture at `mikebom-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge/` — Cargo workspace with two library crates (`libsafe`, `libvuln`) + two binary crates (`safe-only` depends on libsafe only; `vuln-included` depends on libsafe + libvuln). Each library ≤ 10 LOC; each binary ≤ 20 LOC (small enough for a sub-1s compile + trace)
- [ ] T016 [P] Vendor the FR-016a fixture at `mikebom-cli/tests/fixtures/compiler_pipeline/secrets_touch/` — **DEFERRED to Linux session** (needs shell + gcc; testable only with `sudo mikebom trace run`)
- [ ] T017 [P] Vendor the FR-018 fixture at `mikebom-cli/tests/fixtures/compiler_pipeline/stdin_input/` — **DEFERRED to Linux session** (needs gcc `-x c -` invocation; testable only with `sudo mikebom trace run`)

**Checkpoint**: Foundational done. Run `cargo +stable clippy --workspace --all-targets -- -D warnings` (default features — no ebpf-tracing). Zero errors. Run `cargo +stable clippy --workspace --all-targets --features ebpf-tracing -- -D warnings`. Zero errors. Run `cargo +stable test --workspace`. All existing tests pass (BuildTracePredicate additive field verified to not break any test).

## Phase 3: User Story 1 — Attribute vulnerabilities to source files (P1, MVP)

**Story goal**: Operator can run `sudo mikebom trace run -- cargo build --release` on the SC-001 fixture and get a SBOM where the `safe-only` binary's `mikebom:source-read-set` annotation contains ONLY `libsafe`'s files and the `vuln-included` binary's annotation contains BOTH `libsafe` + `libvuln`'s files.

**Independent test**: `mikebom-cli/tests/compiler_pipeline_two_binaries.rs` — runs `mikebom trace run` against the SC-001 fixture, parses the emitted SBOM, and asserts (a) both binary components exist, (b) `safe-only`'s read_set does NOT contain any `libvuln` path, (c) `vuln-included`'s read_set contains at least one `libvuln` path, (d) both binaries' read_sets contain at least one `libsafe` path.

### eBPF kernel-side (mikebom-ebpf)

- [ ] T018 [US1] Implement `sched_process_exec` tracepoint in `mikebom-ebpf/src/programs/compiler_exec.rs` per research R1 — `#[tracepoint]` attribute; read the exec'd process's `comm` field via `bpf_get_current_comm()`; two-stage match logic per R2 (kernel-side prefix compare against a `const &[u8; 16]` array of the whitelist basenames — cargo/rustc/gcc/clang/g++/clang++/go/ld/mold/cc1/cpp/as, all padded to 16 bytes); on match, `bpf_ringbuf_reserve` a `CompilerExecEvent` on the `COMPILER_EXEC_EVENTS` map and populate pid/ppid/cgroup_id/start_ts_ns/comm/argv0_hint (best-effort via `bpf_probe_read_user`)
- [ ] T019 [US1] Implement `sched_process_fork` tracepoint in `mikebom-ebpf/src/programs/compiler_exec.rs` per research R3 — look up parent's PID in `COMPILER_INVOCATIONS`; if present, insert child PID with the same invocation_id (propagates the descendant-tracking through the entire process subtree)
- [ ] T020 [US1] Extend the existing `vfs_open` / `vfs_read` / `vfs_write` kprobes in `mikebom-ebpf/src/programs/file_ops.rs` per plan.md — add a prelude check at each kprobe entry: `if COMPILER_INVOCATIONS.get(pid_of_current()).is_none() { return 0; }`. File-op events from non-compiler-descendant PIDs no longer land in the ring buffer (zero user-space cost per R3)
- [ ] T021 [US1] Implement in-kernel prefix-match denylist in `mikebom-ebpf/src/programs/file_ops.rs` per research R5 — inside the compiler-descendant branch, compare the path against a `const &[&[u8]]` array of R5's kernel-side prefixes (`/etc/`, `/proc/`, `/sys/`, `/dev/`, `/tmp/`, `/var/tmp/`, `/root/.cache/`, `/root/.local/share/`). On match, do NOT emit to the ring buffer

### User-space aggregator (mikebom-cli)

- [ ] T022 [US1] Implement `CompilerPipelineAggregator` struct in `mikebom-cli/src/trace/compiler_pipeline.rs` — state: `invocations: BTreeMap<u64, CompilerInvocation>`, `pid_to_invocation_id: HashMap<u32, u64>`, `next_invocation_id: u64`, `filter_config: FilterConfig`, `dropped_event_count: u64`. Methods: `handle_exec_event(&mut self, event: CompilerExecEvent)`, `handle_file_open(&mut self, event: FileOpEvent)`, `handle_file_close(&mut self, event: FileOpEvent)`, `finalize(self) -> CompilerPipelineData` (assembles DAG + sorts everything deterministically per R8)
- [ ] T023 [US1] Implement userspace secrets-denylist + heuristic filter in `mikebom-cli/src/trace/compiler_pipeline.rs` per research R5 — glob-match paths against R5's userspace patterns (`/var/run/secrets/*`, `/run/secrets/*`, `/run/keys/*`, `~/.ssh/*`, `~/.aws/*`, `~/.gnupg/*`, `~/.docker/config.json`, `~/.netrc`, `~/.kube/config`) + basename heuristic (`.pem`, `.key`, `.crt`, `_rsa`, `_ed25519`). Expand `~` via the `dirs::home_dir()` (already in workspace via `mikebom-cli/src/scan_fs/oci_pull.rs`). On match, increment `secrets_read_filtered` counter + DROP the entry (unless `include_system_reads == true`)
- [ ] T024 [US1] Implement DAG assembly in `CompilerPipelineAggregator::finalize` — walk `invocations` to produce `dag_edges` (parent→child linkages); sort invocations by (start_timestamp_ns, pid); for each invocation, sort read_set + write_set by path lex; sort dag_edges by (parent_invocation_id, child_invocation_id) per R8
- [ ] T025 [US1] Extend `mikebom-cli/src/trace/loader.rs` — load + attach the new `sched_process_exec` + `sched_process_fork` tracepoints alongside existing programs; attach the extended file_ops kprobes (which already exist but now have the compiler-descendant prelude); wire the `COMPILER_EXEC_EVENTS` ring buffer into the processor
- [ ] T026 [US1] Extend `mikebom-cli/src/trace/processor.rs` — dispatch new `COMPILER_EXEC` event type to `CompilerPipelineAggregator::handle_exec_event`; dispatch the file-op events with a `compiler_invocation_id` field (present when the descendant filter fired) to `handle_file_open` / `handle_file_close` on the aggregator

### CLI flag + integration

- [ ] T027 [US1] Add `--include-system-reads` flag to `ScanArgs` in `mikebom-cli/src/cli/scan_cmd.rs` — boolean; default false; doc-comment cites FR-016 + FR-016a; wired to `FilterConfig::include_system_reads`. NO flag is added to `TraceRunArgs` beyond this scan-cmd flag reuse (matches the existing pattern where scan_cmd + trace share the same arg struct)
- [ ] T028 [US1] Wire the finalized `CompilerPipelineData` into the emitted `BuildTracePredicate` — locate the trace-end code path (`mikebom-cli/src/cli/scan.rs` or `run.rs`); on trace completion, call `aggregator.finalize()` + set `predicate.compiler_pipeline = Some(data)`. When `--features ebpf-tracing` is off OR the host is non-Linux, keep `compiler_pipeline = None` per R10

### Emitter augmentation (mikebom-cli/src/generate/)

- [ ] T029 [US1] Implement write-set-to-SBOM-component mapping helper `map_component_to_source_read_set(component, compiler_pipeline_data) -> Option<SourceReadSetPayload>` in `mikebom-cli/src/generate/mod.rs` per Clarifications Q1 — for each component with a file path (via m133 evidence OR file-anchoring `hashes[]`), find every invocation whose `write_set` contains the component's file path; take the union of those invocations' `read_set` PLUS the read_sets of ALL ancestor invocations (traverse `parent_invocation_id` chain). Sort the resulting read_set by path lex per R8
- [ ] T030 [US1] Extend `mikebom-cli/src/generate/cyclonedx/metadata.rs` — when `artifacts.attestation.predicate.compiler_pipeline.is_some()`, for each component: call the T029 mapping helper; if it returns `Some(payload)`, emit `mikebom:source-read-set` annotation on the component's `properties[]` using the `MikebomAnnotationCommentV1` envelope + emit `mikebom:read-set-source = "traced"` alongside. If it returns `None` (no matching write-set), skip C130 + emit `mikebom:read-set-source = "unknown"` per contracts/annotations.md A-2
- [ ] T031 [US1] Extend `mikebom-cli/src/generate/spdx/document.rs` — same as T030 but for SPDX 2.3 (`packages[].annotations[]` carrier)
- [ ] T032 [US1] Extend `mikebom-cli/src/generate/spdx/v3_document.rs` — same as T030 but for SPDX 3 (`Annotation` element carrier)
- [ ] T033 [US1] Register catalog rows C130 + C131 in `mikebom-cli/src/parity/extractors/mod.rs` EXTRACTORS array (numerically sorted, appended after existing C129) — add 2 new extractor entries with `Directionality::SymmetricEqual` + per-format extraction helpers matching the m204/m208 pattern
- [ ] T034 [US1] Update `docs/reference/sbom-format-mapping.md` — append 2 new sections (C130 + C131) with wire-shape examples + Rationale citing why no standards-native carrier exists per contracts/annotations.md A-7

### Integration test

- [ ] T035 [US1] Write SC-001 integration test `mikebom-cli/tests/compiler_pipeline_two_binaries.rs` — spawn `mikebom trace run -- cargo build --release` against the T015 fixture; parse the emitted SBOM; assert (a) `safe-only`'s source-read-set does NOT contain `libvuln`, (b) `vuln-included`'s source-read-set contains both `libsafe` + `libvuln`, (c) both contain `libsafe`. Gated behind `#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]` per m020 test-isolation convention; skips with a diagnostic when run without `CAP_BPF`. **Plus a FR-012 cross-compilation subtest (C2 analyze remediation)**: `cross_compilation_attribution_preserved` — same fixture built with `cargo build --release --target x86_64-unknown-linux-musl` (assumes musl target installed OR skips cleanly with diagnostic if `rustup target list --installed` doesn't include it); asserts attribution semantics are identical to the host-arch case (per FR-012 preservation)
- [ ] T035a [US1] Write SC-002 self-build coverage test `mikebom-cli/tests/compiler_pipeline_self_build.rs` (C1 analyze remediation) — spawn `sudo mikebom trace run -- cargo build -p mikebom --release --features ebpf-tracing` (or equivalent hermetic invocation); parse the emitted SBOM; assert every emitted binary component has a non-empty `mikebom:source-read-set` containing at minimum the top-level `main.rs` / `lib.rs` files of the crate that produced it. Gated behind `#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]` + `#[ignore]`-gated (mikebom self-build is heavy — runs opt-in via `--ignored` or in a dedicated CI perf lane). Documents SC-002 as the reachability-assertion regression guard the perf test does NOT provide

**Checkpoint**: US1 done. `sudo mikebom trace run` on the two-binaries fixture emits SBOMs with correct source-read-set attribution. Run `./scripts/pre-pr.sh` with default features (no ebpf-tracing) — clean. Run `MIKEBOM_PREPR_EBPF=1 ./scripts/pre-pr.sh` on a Linux host — clean.

## Phase 4: User Story 2 — Reproducibility beyond byte-identity (P2)

**Story goal**: Two consecutive traces produce byte-identical source-read-set annotations. Modifying one source file's content changes only that file's sha256 in the read-set of the binaries that consume it.

**Independent test**: `mikebom-cli/tests/compiler_pipeline_reproducibility.rs` — runs the SC-001 fixture twice; asserts the source-read-set annotations are byte-equal. Then modifies a source file + rebuilds; asserts the delta appears in exactly the expected binary's read-set.

- [ ] T036 [US2] Write SC-004 byte-identity test in `mikebom-cli/tests/compiler_pipeline_reproducibility.rs` — spawn `mikebom trace run` twice on the same fixture; extract each run's `mikebom:source-read-set` annotations; assert byte-equal via `pretty_assertions::assert_eq!` for readable diffs on failure. Gated behind `#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]`
- [ ] T037 [US2] Write SC-005 exclusion test in `mikebom-cli/tests/compiler_pipeline_reproducibility.rs` — remove `libvuln`'s source file from the fixture (via `tempfile::TempDir` clone + delete); rebuild + retrace; assert `libvuln` files are NOT in any binary's source-read-set
- [ ] T038 [US2] Add deterministic-ordering unit test in `mikebom-common/src/attestation/compiler_pipeline.rs::tests` per R8 — construct a `CompilerPipelineData` with intentionally out-of-order invocations + read/write sets; serialize; assert the emitted JSON has entries in the R8-mandated order (invocations by (start_ts, pid), read_set + write_set by path lex, dag_edges by (parent, child), filter_categories_applied lex)

**Checkpoint**: US2 done. Byte-identity + exclusion invariants locked at test-level.

## Phase 5: User Story 3 — Downstream reachability tooling (P3)

**Story goal**: Downstream tools consuming a mikebom SBOM can filter an advisory list by whether the advisory's known-affected files intersect any binary's source-read-set. Also: witness-format consumers see the `compiler-invocation/v0.1` attestor entry.

**Independent test**: `mikebom-cli/tests/compiler_pipeline_witness_attestor.rs` — verify the emitted witness attestation-collection contains a `https://mikebom.dev/attestation/compiler-invocation/v0.1` inner entry with the correct shape per contracts/attestor-predicate.md.

- [ ] T039 [US3] Extend `mikebom-common/src/attestation/witness.rs` — implement the `compiler-invocation/v0.1` attestor entry per data-model E14 + contracts/attestor-predicate.md; wire it into the `build_witness_collection` builder function. Add a helper constant `pub const COMPILER_INVOCATION_PREDICATE_TYPE: &str = "https://mikebom.dev/attestation/compiler-invocation/v0.1";`
- [ ] T040 [US3] Write integration test `mikebom-cli/tests/compiler_pipeline_witness_attestor.rs` — emit a witness attestation from a trace; parse it; assert the presence of a `compiler-invocation/v0.1` entry with correct URI + shape per C-2 wire spec. Gated behind eBPF cfg
- [ ] T041 [US3] Write fixture consumer script at `mikebom-cli/tests/fixtures/compiler_pipeline/downstream_consumer/reachability_filter.py` — reads a mikebom SBOM, extracts each binary's source-read-set, matches against a synthetic advisory list, prints "REACHABLE" or "NOT REACHABLE" per binary. Documented in the fixture's README as the SC-006 proof-of-concept

**Checkpoint**: US3 done. Ecosystem-side integration lands.

## Phase 6: Polish & Cross-Cutting

### Additional transparency annotations

- [ ] T042 [P] Emit `mikebom:compiler-pipeline-completeness` doc-scope annotation (C132) per FR-008 — extend the 3 emitter files (`cyclonedx/metadata.rs`, `spdx/document.rs`, `spdx/v3_document.rs`) to include the annotation at document scope when `compiler_pipeline.is_some()`. Values per data-model E11 (`"complete"` / `"degraded"` / `"partial"`)
- [ ] T043 [P] Emit `mikebom:secrets-read-filtered` doc-scope annotation (C133) per FR-016a — same 3 emitter files; emitted only when `secrets_read_filtered > 0`
- [ ] T044 [P] Emit `mikebom:trace-attach-late` per-component annotation (C134) per FR-017 — same 3 emitter files; emitted per-component when the invocation was attach-late
- [ ] T045 [P] Register catalog rows C132..C134 in `mikebom-cli/src/parity/extractors/mod.rs` EXTRACTORS array (numerically sorted, appended after C130 + C131) — same shape as T033
- [ ] T046 [P] Update `docs/reference/sbom-format-mapping.md` — append C132..C134 sections per contracts/annotations.md A-7

### FR-018 stdin-input handling

- [ ] T047 Implement stdin-input marker per FR-018 — extend `CompilerPipelineAggregator::handle_file_open` to detect stdin (`path == "-"` or `path == "/dev/stdin"`); insert a `ReadSetEntry { path: PathBuf::from("<stdin>"), sha256: <sentinel-zero-hash>, kind: ReadKind::StdinInput { bytes_read } }` where `bytes_read` is tracked via subsequent `read` syscall counters. Integration test at `mikebom-cli/tests/compiler_pipeline_stdin.rs` using the T017 fixture

### FR-008 overflow test

- [ ] T048 Write SC-007 overflow test at `mikebom-cli/tests/compiler_pipeline_overflow.rs` — synthetic heavy build (spawn `make -j$(nproc*4)` on a large C project fixture OR inject synthetic drops via a test-only aggregator method); assert `mikebom:compiler-pipeline-completeness = "degraded"` with `dropped > 0`. Gated behind eBPF cfg + `#[ignore]`-gated (needs a heavy build target)

### Perf regression test

- [ ] T049 Write perf regression test at `mikebom-cli/tests/compiler_pipeline_perf.rs` per FR-007 + SC-003 — `#[ignore]`-gated (matches m094 / m208 convention). Baseline: mikebom self-build with `--features ebpf-tracing` but the trace attached to nothing (measure raw build time). Traced: mikebom self-build with active trace. Assert traced_wall_clock <= 1.15 * baseline_wall_clock. Baseline captured once, stored at `mikebom-cli/tests/fixtures/compiler_pipeline/perf_baseline.json`

### Feature-flag verification

- [ ] T050 Verify default-features (no ebpf-tracing) build still passes clean — run `cargo +stable build -p mikebom` + `cargo +stable clippy --workspace --all-targets -- -D warnings` (WITHOUT `--features ebpf-tracing`). Zero errors, zero warnings. The `compiler_pipeline` field on `BuildTracePredicate` deserializes as `None` on non-Linux + default-features hosts per R10
- [ ] T051 Verify ebpf-tracing feature build passes clean on Linux — `cargo +stable clippy --workspace --all-targets --features ebpf-tracing -- -D warnings`. Zero errors, zero warnings. `cargo +stable test --workspace --features ebpf-tracing` — every suite `ok. N passed; 0 failed`

### Documentation

- [ ] T052 [P] Update `docs/architecture/attestations.md` — add a section "Compiler-Invocation Predicate (m210)" documenting the `https://mikebom.dev/attestation/compiler-invocation/v0.1` URI + shape per contracts/attestor-predicate.md
- [ ] T053 [P] Add architectural note to `docs/architecture/scanning.md` OR `docs/design-notes.md` — reference the new compiler-pipeline data flow (eBPF program → COMPILER_INVOCATIONS map → user-space aggregator → BuildTracePredicate.compiler_pipeline → per-component annotation emission)
- [ ] T054 [P] Add contributor guide entry in `docs/architecture/attestations.md` (new subsection): "Adding a new compiler to the whitelist" — step-by-step recipe (edit `COMPILER_WHITELIST` const in `mikebom-ebpf/src/programs/compiler_exec.rs`, extend `CompilerFamily` enum, add fixture)

### Final gates

- [ ] T055 Verify byte-identity of pre-m210 attestation goldens — run the full workspace test suite with `--features ebpf-tracing` off; assert no attestation-golden fixture regenerated during the test (the additive `Option<CompilerPipelineData>` field + `skip_serializing_if` guarantees this)
- [ ] T056 Run pre-PR gate: `./scripts/pre-pr.sh` (default features) + `MIKEBOM_PREPR_EBPF=1 ./scripts/pre-pr.sh` (ebpf-tracing on) — both must exit 0 with zero warnings + all tests passing
- [ ] T057 Open PR against main — title `impl(210): compiler-pipeline eBPF tracing for source-to-binary attribution`; body cites pre-PR gate output + notes the CANONICAL Principle II fulfillment + links to spec/plan/data-model/contracts; closes no external issue (this is a new feature not tracked by a GitHub issue — self-scoped)

## Dependencies

**Phase → Phase**: 1 → 2 → 3 → (4, 5 parallel-safe) → 6.

**Within Phase 2**:
- T006, T007 `[P]` (different enums in same file, no cross-refs).
- T008 requires T006 + T007 (uses both).
- T009 requires T008 (uses ReadSetEntry + WriteSetEntry).
- T010 has no dependencies (fresh types).
- T011 requires T009 + T010 (uses CompilerInvocation + InvocationDagEdge + CompletenessState).
- T012 requires T011 (imports CompilerPipelineData).
- T013 requires T012 (updates every `BuildTracePredicate { ... }` construction to include the new field).
- T014 has no dependencies.
- T015, T016, T017 all `[P]`.

**Within Phase 3 (US1)**:
- T018 requires T014 (references COMPILER_INVOCATIONS + COMPILER_EXEC_EVENTS).
- T019 requires T014 + T018 (extends the same file; sched_process_fork writes to the same map).
- T020, T021 require T018 (kernel-side descendant filter presupposes the map is populated).
- T022 requires T009 + T011 (uses CompilerInvocation + CompilerPipelineData).
- T023 requires T022 (extends the aggregator).
- T024 requires T022.
- T025 requires T018 + T019 + T020 + T021 (loads all the new programs).
- T026 requires T022 + T025.
- T027 requires T023.
- T028 requires T024 + T027.
- T029 requires T011.
- T030, T031, T032 all `[P]` after T029 (different files).
- T033 requires T030 + T031 + T032.
- T034 `[P]` with T033 (different file).
- T035 requires everything above.

**Within Phase 4 (US2)**:
- T036, T037 both require Phase 3 done.
- T038 requires T011.
- T036 + T037 + T038 `[P]` relative to each other.

**Within Phase 5 (US3)**:
- T039 requires T011.
- T040 requires T039 + Phase 3 emitters.
- T041 has no code dependencies (Python fixture).

**Within Phase 6**:
- T042, T043, T044 all `[P]` (different annotation emissions but same 3 files → sequential within a file, parallel across files).
- T045 requires T042 + T043 + T044.
- T046 `[P]` with T045.
- T047 requires T022.
- T048 requires T042.
- T049 requires everything above.
- T050 has no dependencies (independent verification).
- T051 requires everything above.
- T052, T053, T054 `[P]`.
- T055 requires everything above.
- T056 requires T055.
- T057 requires T056.

## Parallel Execution Examples

**Phase 2 type definitions** (same file, independent types):
```text
T006 [P] CompilerFamily enum
T007 [P] ReadKind enum
```

**Phase 2 fixture vendoring** (3 different directories):
```text
T015 [P] SC-001 two-binaries-diverge
T016 [P] FR-016a secrets-touch
T017 [P] FR-018 stdin-input
```

**Phase 3 emitter augmentation** (3 different files):
```text
T030 [US1] cyclonedx/metadata.rs (C130 + C131)
T031 [US1] spdx/document.rs (C130 + C131)
T032 [US1] spdx/v3_document.rs (C130 + C131)
```

**Phase 6 polish docs** (3 different files):
```text
T052 [P] attestations.md
T053 [P] scanning.md / design-notes.md
T054 [P] contributor guide
```

## Implementation Strategy

- **MVP scope**: Phase 1 + Phase 2 + Phase 3 (US1) = 35 tasks. Delivers the source-to-binary attribution end-to-end for the two-binaries fixture. US2 (reproducibility locks) + US3 (witness attestor + downstream fixture) build on top without touching MVP surface.
- **Incremental delivery**: after MVP, US2 (T036–T038) adds 3 regression tests. US3 (T039–T041) adds the witness attestor + fixture. Both can ship in the same PR or follow-ups.
- **Sequencing recommendation**: One PR for the whole milestone. Total ~2000 LOC counting tests + fixtures. If the PR is too large to review, natural split boundaries: (A) Setup + Foundational (T001–T017); (B) US1 (T018–T035); (C) US2 + US3 (T036–T041); (D) Polish (T042–T057). Split A pre-emerges types without touching trace behavior; A→B is where new observation happens.
- **Rollback plan if any regression surfaces**: the `compiler_pipeline: Option<CompilerPipelineData>` field with `skip_serializing_if` means turning off the feature (or reverting the tracer wiring in `trace_cmd.rs`) leaves all pre-m210 attestation goldens byte-identical. Emergency revert of just the tracer entry point is a one-line fix.

## Task count

- **Setup**: 5 (T001–T005)
- **Foundational**: 12 (T006–T017)
- **US1 (P1, MVP)**: 19 (T018–T035 + T035a — one added per analyze remediation C1 for SC-002 self-build coverage; T035 amended per C2 for FR-012 cross-compilation subtest)
- **US2 (P2)**: 3 (T036–T038)
- **US3 (P3)**: 3 (T039–T041)
- **Polish**: 16 (T042–T057)

**Total**: 58 tasks
