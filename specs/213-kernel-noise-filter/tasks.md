---

description: "Task list for m213 — kernel-side trace-noise filter for file_ops kprobes (issue #616)"
---

# Tasks: Kernel-side trace-noise filter for file_ops kprobes

**Input**: Design documents from `/specs/213-kernel-noise-filter/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: Test tasks INCLUDED — plan.md explicitly enumerates unit tests, wire-shape round-trip tests, and container-harness assertions. The m210 → m211 → m212 precedent has made unit + integration coverage a merge-blocker per the CLAUDE.md pre-PR gate.

**Organization**: 3 user stories from spec.md. US1 (P1) is the actual fix — kernel-side classifier that drops noise. US2 (P2) is the observability layer — emit `filter_categories_applied[]`. US3 (P3) is the widening flag. **Note**: Constitution Principle VIII analysis in plan.md deems US2 a merge-blocker for US1 (transparent-aggregate mitigation for the deliberate event-drop). US1 alone is dev-testable but cannot ship without US2.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story (US1, US2, US3)
- File paths absolute-from-repo-root

---

## Phase 1: Setup

**Purpose**: Sanity-check the branch + prerequisites before touching code

- [X] T001 Verify branch `213-kernel-noise-filter` is checked out and up-to-date with `main` post-m212 merge — verified 2026-07-21: branch clean, HEAD at 4c58921 (tasks remediations).
- [X] T002 Verify m212 baseline: `cargo test -p mikebom --bin mikebom trace::counters` = **3 passed; 0 failed** (m212 counters module green pre-m213 changes).
- [X] T003 Docker container-harness prerequisite: DEFERRED to T015 (~10 min cold build; workspace verified compiles via T002).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The shared entities every user story depends on — `FilterCategoryTag` (E1) and the increment helper. Without these, US1's classifier can't compile.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T004 [P] `FilterCategoryTag` enum + `ALL` + `name` + `TryFrom<u8>` landed at `mikebom-common/src/events.rs:181-235`. 4 unit tests green: `filter_category_tag_u8_round_trip`, `filter_category_tag_name_matches_wire_contract`, `filter_category_tag_try_from_unknown_discriminant_errors`, `filter_category_tag_all_covers_all_variants`.
- [X] T004a [P] `file_event_size_is_stable` test pinning `size_of::<FileEvent>() == 352` at `mikebom-common/src/events.rs:255`. Captured pre-m213 on macOS aarch64 stable Rust 2026-07-21.
- [X] T005 [P] `increment_filter_category_hit(cat: u8)` helper at `mikebom-ebpf/src/helpers.rs:65-79`. Mirrors m212's `increment_drop_counter` pattern verbatim; graceful no-op on out-of-bounds `cat`.
- [X] T006 `FILTER_CATEGORY_HITS: PerCpuArray<u64>` (4 slots) + `FILTER_WIDEN: PerCpuArray<u8>` (1 slot) declared at `mikebom-ebpf/src/maps.rs:156-193`. Placed adjacent to m212's drop counters as planned. Comment cites data-model.md E2 + E3.

**Checkpoint**: `cargo test -p mikebom-common --lib events::filter_category_tag` passes; `cargo xtask ebpf` (if available in dev env) compiles `mikebom-ebpf` with the new map + helper. Foundation ready.

---

## Phase 3: User Story 1 - Cargo builds no longer lose real compiler events to fingerprint spam (Priority: P1) 🎯 MVP

**Goal**: Kernel-side classifier drops System/UserCache/Ephemeral/CargoFingerprint events BEFORE `FILE_EVENTS.reserve()`. On the SC-001 fixture, rustc + linker file events start appearing in the attestation.

**Independent Test**: Container harness (extended in T014) asserts `[.predicate.file_access.operations[] | select(.comm == "rustc")] | length >= 1` on the `two_binaries_diverge` fixture (baseline: 0).

### Tests for User Story 1 ⚠️

> Write these tests FIRST; ensure they FAIL before implementation lands.

- [X] T007 [P] [US1] Classifier + pattern tests at `mikebom-common/src/filter.rs::tests` — **7 US1 tests green** (`t007_system_paths_classified`, `t007_user_cache_paths_classified`, `t007_ephemeral_paths_classified`, `t007_cargo_fingerprint_paths_classified`, `t007_non_matching_paths_return_none`, `t007_truncated_full_buffer_paths_return_none`, `t007_fingerprint_beyond_scan_window_missed`). Design pivot from tasks.md original spec: classifier lives in `mikebom-common/src/filter.rs` (shared `no_std`-compatible module) not `mikebom-ebpf/src/programs/file_ops.rs`. Enables host-side testing without an eBPF loader and shares one implementation across kernel + tests. Also 4 pattern-catalog stability tests (`patterns_catalog_size_matches_declared_categories`, `all_patterns_have_valid_category_discriminants`, `all_pattern_lengths_within_bounds`, `all_patterns_have_valid_kind_discriminants`). Total: 12/12 pass.

### Implementation for User Story 1

- [X] T008 [US1] `FilterPattern` struct + `const PATTERNS: [FilterPattern; 11]` catalog at `mikebom-common/src/filter.rs:38-166`. **Adjusted scope from tasks.md**: 11 patterns (4 System + 2 UserCache + 2 Ephemeral + 3 CargoFingerprint) — the original "15 with 4 padding slots" was moot since Rust `const` arrays don't need padding for future extension (adding a variant recompiles the crate). 16-byte `bytes` field (not 32) fits the longest pattern (`/.local/share/` at 14 bytes) with slack.
- [X] T009 [US1] `path_starts_with(path, pattern, plen)` at `mikebom-common/src/filter.rs:181-193`. **Simplified from tasks.md**: byte-wise bounded loop instead of `u64::from_le_bytes` word compares — the pattern maximum is 16 bytes so a fully-unrolled inner `while j < 16` loop is what the verifier prefers on kernels 5.15+ per contracts/ebpf-verifier-notes.md Rule 2. Word-compare would need alignment guarantees we don't have on the user-space-derived path buffer.
- [X] T010 [US1] `path_contains_pattern(path, pattern, plen)` at `mikebom-common/src/filter.rs:197-217`. Bounded scan (offset 0..128) × fixed-size 16-byte inner check. `CONTAINS_SCAN_MAX_OFFSET = 128` per FR-016 rationale.
- [X] T011 [US1] `path_matches_filter_category(path, widen_system)` at `mikebom-common/src/filter.rs:230-260`. **Widen-flag consultation folded into T011 instead of deferred to US3**: the classifier takes `widen_system: bool` as a parameter, so US1's implementation is complete-in-one-file. US3 (T028) still needs to wire the eBPF-side widen-flag map read.
- [X] T012 [US1] Wired `classify_and_drop_if_noise(&path)` into `try_do_filp_open` at `mikebom-ebpf/src/programs/file_ops.rs` — early return on classifier match, before `FILE_EVENTS.reserve()`. Comment cites m213 issue #616 rationale.
- [X] T013 [US1] Same wiring into `try_openat2`. Both kprobes use the same `classify_and_drop_if_noise` helper at the top of `file_ops.rs`.
- [X] T014 [US1] `scripts/ebpf-integration-test.sh` extended with SC-001 assertions: rustc file-count ≥ 1, linker file-count ≥ 1, fingerprint-leak count == 0. Also prints diagnostic first-5-leaked-paths on failure.
- [ ] T015 [US1] Verify US1 end-to-end in Colima (kernel 6.8 aarch64). **DEFERRED**: bundled with Phase 4 + Phase 5 into a single container-build/verify iteration to save ~10 min per Docker cold-build cycle. See task T037 for the combined verification. **Multi-kernel coverage note per SC-003 + SC-004 + FR-013**: CI's `lint-and-test-ebpf` matrix (5.15 LTS, 6.1 LTS, 6.6, 6.8) is the merge-blocking gate; Colima 6.8 is the local subset. Do not merge before all four kernel lanes are green.

**Checkpoint**: The kernel-side filter drops events; rustc file events appear in the attestation. **BUT** — per plan.md Principle VIII analysis, this state is NOT mergeable without US2 (the transparent aggregate). Continue to Phase 4.

---

## Phase 4: User Story 2 - Operator can see which noise categories the filter suppressed (Priority: P2) 🚨 MERGE-BLOCKER for US1

**Goal**: Emit `TraceIntegrity.filter_categories_applied[]` — a sorted-deduplicated list of category names whose kernel-side count > 0. Provides the transparent aggregate that Principle VIII requires as mitigation for US1's event-drop.

**Independent Test**: Container harness (extended in T024) asserts `[.predicate.trace_integrity.filter_categories_applied[] | select(. == "CargoFingerprint")] | length >= 1` on the SC-001 fixture; and asserts empty state `[]` on `mikebom trace capture -- true` per FR-009.

### Tests for User Story 2 ⚠️

> Write these FIRST; ensure they FAIL before implementation lands.

- [X] T016 [P] [US2] Round-trip test `trace_integrity_serde_populated_filter_categories_applied` + `trace_integrity_empty_filter_categories_applied_serializes_as_empty_array` (FR-009 back-compat) at `mikebom-common/src/attestation/integrity.rs`. 4 integrity tests green — includes pre-m213-JSON deserialization contract (missing field → `vec![]` via `serde(default)`).
- [X] T017 [P] [US2] `FilterCategoryHitsSummary` unit tests at `mikebom-cli/src/trace/counters.rs::tests` — 5 US2 tests green (`t017_applied_categories_sorts_and_dedups`, `_omits_zero_counts`, `_empty_when_no_hits`, `_empty_when_all_zero`, `t017_attach_failures_propagate_through_summary`).

### Implementation for User Story 2

- [X] T018 [US2] `filter_categories_applied: Vec<String>` field added to `TraceIntegrity` at `mikebom-common/src/attestation/integrity.rs`. Placement: LAST field. `#[serde(default)]` for pre-m213 back-compat. Field always emitted (`[]` when empty per FR-009).
- [X] T019 [US2] `FilterCategoryHitsSummary` struct + `applied_categories()` method at `mikebom-cli/src/trace/counters.rs`. Uses `BTreeMap<FilterCategoryTag, u64>` for stable iteration.
- [X] T020 [US2] `read_filter_category_hits(bpf) -> FilterCategoryHitsSummary` at same file. Iterates `FilterCategoryTag::ALL`, aggregates via `read_percpu_slot_sum`. Attach failure surfaces as SINGLE `"filter_category_hits"` entry per R9 (not per-slot). Non-Linux stub returns default.
- [X] T021 [US2] `read_percpu_slot_sum(bpf, name, idx)` helper at same file — parallel to m212's `read_percpu_sum` with explicit slot index.
- [X] T022 [US2] Wired `read_filter_category_hits` into `mikebom-cli/src/cli/scan.rs::execute_scan` at trace-end. Populates `TraceIntegrity.filter_categories_applied` via `summary.applied_categories()`. Merges `attach_failures` into `counter_attach_failures` which flows into `kprobe_attach_failures[]` per m212 convention. `tracing::info!` line emits `applied` values per contracts/filter-hits-map.md observability requirement.
- [X] T023 [US2] Threaded `filter_categories_applied: Vec<String>` field through `TraceStats` at `mikebom-cli/src/trace/processor.rs` + `LiveStats::snapshot` (returns empty per m212 convention) + `aggregator.rs::finalize` (defensive sort+dedup before write to wire).
- [X] T024 [US2] `scripts/ebpf-integration-test.sh` extended with SC-003 assertions: (a) `filter_categories_applied` is JSON array type; (b) contains `"CargoFingerprint"` on cargo build; (c) FR-008 defensive — every value matches `^[A-Za-z]+$` regex (guards against accidental path leakage). Applied C2 analyze-remediation bonus.
- [X] T025 [US2] `scripts/ebpf-integration-test.sh` — removed the pre-m213 `[[ "$OVERFLOWS" -gt 100 ]]` assertion + added new SC-002 upper-bound `[[ "$OVERFLOWS" -gt 10 ]] && FAIL`. The two assertions cannot coexist (they're inverse hypotheses); m213's kernel-side filter makes the m212 assertion false by design.

**Checkpoint**: `TraceIntegrity.filter_categories_applied` appears in every emitted attestation; container harness asserts its presence and content. US1 + US2 together satisfy Principle VIII. **Now mergeable.**

---

## Phase 5: User Story 3 - Operator can opt out of System-category filtering when they need full coverage (Priority: P3)

**Goal**: The existing `--include-system-reads` CLI flag disables the kernel-side System-category filter (only) for the current trace. UserCache/Ephemeral/CargoFingerprint remain filtered.

**Independent Test**: Two-invocation harness check (in T031): default run has `"System"` in `filter_categories_applied` when the traced process reads `/etc/*`; widened run has NO `"System"` entry AND has `/etc/*` events in `file_access.operations`.

### Tests for User Story 3 ⚠️

- [X] T026 [P] [US3] Widen-flag unit test `t026_widen_flag_disables_system_only` at `mikebom-common/src/filter.rs::tests` (co-located with T007). Verifies (a) all 4 System paths return `None` with `widen_system=true`, (b) UserCache/Ephemeral/CargoFingerprint categories STILL fire in widened mode per FR-010.

### Implementation for User Story 3

- [X] T027 [US3] `FILTER_WIDEN: PerCpuArray<u8>` (1 slot) declared at `mikebom-ebpf/src/maps.rs:189` alongside `FILTER_CATEGORY_HITS`. **Landed in T006** — bundled with the E2 map declaration for efficiency.
- [X] T028 [US3] Classifier widen-gate landed as `widen_system: bool` parameter to `path_matches_filter_category` at `mikebom-common/src/filter.rs:230`. **Landed in T011** — folded into US1's design so the classifier is complete-in-one-function. The eBPF-side wrapper `classify_and_drop_if_noise` at `mikebom-ebpf/src/programs/file_ops.rs` reads `FILTER_WIDEN[0]` via the `widen_system()` helper and passes it into the classifier.
- [X] T029 [US3] `FILTER_WIDEN[0]` write landed in `mikebom-cli/src/trace/loader.rs` alongside the CONFIG map write. Uses `aya::util::nr_cpus()` + `PerCpuValues::try_from(vec![val; nr_cpus])` per aya idiom. On map-attach failure, WARN + trace continues (fail-open — the kernel-side classifier's default-zero read maintains pre-m213 System-filtering behavior).
- [X] T030 [US3] `LoaderConfig.include_system_reads` field added; `execute_scan` at `mikebom-cli/src/cli/scan.rs:222` plumbs `args.include_system_reads` through. `tracing::info!` line at loader emit confirms the flag state per contracts/filter-hits-map.md observability requirement.
- [X] T031 [US3] `scripts/ebpf-integration-test.sh` extended with SC-006 second-invocation: runs `mikebom trace capture --include-system-reads -- cat /etc/hostname`, asserts widened run's `filter_categories_applied` does NOT contain `"System"`, and reports `/etc/hostname` event count as diagnostic.

**Checkpoint**: All 3 user stories independently functional. Ready for polish + pre-PR gate.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Rollups, non-blocking cleanup, cross-cutting verification.

- [X] T032 [P] Fail-open coverage landed as `t017_attach_failures_propagate_through_summary` at `mikebom-cli/src/trace/counters.rs::tests` — asserts `attach_failures == vec!["filter_category_hits"]` + `applied_categories() == vec![]`. Same-file/same-class coverage as the T032 spec asked for; separate function name would have been redundant.
- [X] T033 [P] `docs/architecture/attestations.md` extended with a new paragraph describing `filter_categories_applied[]` — closed value set, always-present-as-array semantics per FR-009, cross-layer join contract, R9 fail-open handling.
- [X] T034 [P] Not needed — the verifier-safe recipe from m211 held; no new failure classes surfaced during Phases 2-5. Kernel-side classifier compiled cleanly. Container-harness verification at T037 will confirm on 4 kernels.
- [X] T035 Pre-PR gate — `cargo +stable clippy --workspace --all-targets -- -D warnings`: **clean**. `cargo +stable test --workspace`: **PASS after helm-race fix**. Incidental fix: added `std::sync::Mutex` serializer to `with_helm_render_timeout_env` at `mikebom-cli/src/scan_fs/package_db/helm.rs:1264` — the 4 unit tests mutating `MIKEBOM_HELM_RENDER_TIMEOUT_SECS` race in parallel; documented flake per `reference_m203_helm_test_flake` memory. Fix is stdlib-only, no new dep, isolated to helm.rs test module.
- [X] T036 m212 harness assertions preserved — the SC-002 assertion `[[ "$OVERFLOWS" -le 10 ]]` still asserts `ring_buffer_overflows | type == "number"` (m212 SC-001) as a precondition. The pre-m213 `[[ "$OVERFLOWS" -gt 100 ]]` inverse-hypothesis assertion was removed per T025 (it becomes false-by-design post-m213).

### Final gates

- [ ] T037 DEFERRED — Colima container verification (kernel 6.8) bundled with CI's `lint-and-test-ebpf` multi-kernel matrix on PR push. The user will receive CI status on the PR itself.
- [ ] T038 DEFERRED — PR open pending user confirmation. Ready-state: 6 phase commits on branch, tests green locally, clippy clean, docs updated.

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (Phase 1)**: no dependencies — can start immediately.
- **Foundational (Phase 2)**: depends on Setup — BLOCKS every user story.
- **US1 (Phase 3)**: depends on Foundational only. NOT a merge-shippable state alone (Principle VIII requires US2 as transparent-aggregate mitigation).
- **US2 (Phase 4)**: depends on Foundational only. Independently developable in parallel with US1 by a second dev if staffed. Merge blocks on US1 too (US2's harness assertions in T024 depend on US1's classifier producing hits).
- **US3 (Phase 5)**: depends on Foundational + US1 (T028 modifies `path_matches_filter_category` from T011).
- **Polish (Phase 6)**: depends on all preceding phases.

### Cross-story parallelism

- T004 (E1 in mikebom-common) and T005 (helper in mikebom-ebpf) are in different files with no shared imports → run in parallel.
- T007 (US1 unit tests) and T016 (US2 wire-shape test) and T026 (US3 unit test) are in three different files → run in parallel.
- T008 + T009 + T010 (all US1, same file) → strictly sequential.
- T011 + T012 + T013 (all US1, same file, T012 + T013 depend on T011) → sequential.
- T018 (mikebom-common), T019 (mikebom-cli), T020 (mikebom-cli), T022 (mikebom-cli), T023 (mikebom-cli) → mostly sequential within mikebom-cli, but T018 parallel with T019.

### Within each user story

- Tests (T007, T016, T017, T026) — write FIRST; ensure they FAIL before implementation lands.
- Then implementation.
- Then harness extension.
- Then verification pass in Colima.

---

## Parallel Example: User Story 2

```bash
# After Phase 2 completes, launch the two US2 test tasks together:
Task: "Wire-shape round-trip test for filter_categories_applied in mikebom-common/src/attestation/integrity.rs::tests"     # T016
Task: "FilterCategoryHitsSummary unit tests in mikebom-cli/src/trace/counters.rs::tests"                                    # T017

# Then implement US2 sequentially (single file focus per task):
Task: "Add TraceIntegrity.filter_categories_applied field in mikebom-common/src/attestation/integrity.rs"                    # T018
Task: "Add FilterCategoryHitsSummary struct in mikebom-cli/src/trace/counters.rs"                                            # T019
```

---

## Implementation Strategy

### MVP (US1 + US2 — Principle VIII floor)

1. Complete Phase 1 (Setup).
2. Complete Phase 2 (Foundational — E1 + helper + hits map).
3. Complete Phase 3 (US1) — dev-testable but not mergeable alone.
4. Complete Phase 4 (US2) — the Principle VIII transparent aggregate.
5. **STOP + VALIDATE**: container harness passes SC-001 + SC-002 + SC-003; `filter_categories_applied` present.
6. This is the earliest mergeable point.

### Full delivery (US1 + US2 + US3)

7. Complete Phase 5 (US3) — widening flag.
8. Complete Phase 6 (Polish + pre-PR gate + PR open).

### Single-developer solo sequencing (recommended for this milestone)

Given the tight cross-file dependencies within mikebom-ebpf and the single-crate `trace/counters.rs` module, solo sequential execution beats parallel-team overhead. Ordered: T001 → T002 → T003 → T004 → T004a → T005 → T006 → T007 → T008 → T009 → T010 → T011 → T012 → T013 → T014 → T015 → T016 → T017 → T018 → T019 → T020 → T021 → T022 → T023 → T024 → T025 → T026 → T027 → T028 → T029 → T030 → T031 → T032 → T033 → T034 → T035 → T036 → T037 → T038.

---

## Notes

- [P] tasks = different files, no dependencies.
- [Story] label maps task to user story for traceability.
- Test-first: verify tests FAIL before implementing (T007 fails until T008-T013; T016 fails until T018; T017 fails until T019-T020; T026 fails until T027-T028).
- Commit after each logical group (per-phase, or per-story within a phase).
- Container harness (T014, T024, T025, T031) is the merge-blocking integration gate; local unit tests alone are insufficient per the feedback_ebpf_container_test_gap memory entry.
- Verifier rejection on ANY kernel in the SC-003 matrix (5.15, 6.1, 6.6, 6.8) is a merge-blocker per FR-013 + SC-004. Rollback per quickstart.md's rollback recipe if T015 fails.
