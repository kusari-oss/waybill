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

- [ ] T001 Verify branch `213-kernel-noise-filter` is checked out and up-to-date with `main` post-m212 merge — run `git status && git log -1 --oneline main..HEAD` to confirm the plan-phase commit is HEAD.
- [ ] T002 Verify m212 baseline: `cargo test -p mikebom --lib counters::` passes cleanly (SC-006 depends on the m212 counters module being green pre-m213 changes).
- [ ] T003 Verify Colima container-harness prerequisite: `docker build -f Dockerfile.ebpf-test -t mikebom-ebpf-test .` succeeds — the m213 harness extension in T029 relies on this image being buildable.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The shared entities every user story depends on — `FilterCategoryTag` (E1) and the increment helper. Without these, US1's classifier can't compile.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [ ] T004 [P] Create `FilterCategoryTag` `#[repr(u8)]` enum with 4 variants (`System=0`, `UserCache=1`, `Ephemeral=2`, `CargoFingerprint=3`) + `ALL: [FilterCategoryTag; 4]` const + `name(self) -> &'static str` + `TryFrom<u8>` impl in `mikebom-common/src/events.rs`. Include unit tests for round-trip (u8 → variant → u8) and name-stability (variant → &str MUST return exact wire strings per contracts/filter-category-tag.md).
- [ ] T005 [P] Add `increment_filter_category_hit(cat: u8)` `#[inline(always)]` helper in `mikebom-ebpf/src/helpers.rs`. Uses `FILTER_CATEGORY_HITS.get_ptr_mut(cat as u32)` + `saturating_add(1)`. Mirrors m212's `increment_drop_counter` pattern verbatim.
- [ ] T006 Add `FILTER_CATEGORY_HITS: PerCpuArray<u64>` (4 slots) `#[map]` declaration in `mikebom-ebpf/src/maps.rs`. Placement: adjacent to m212's `FILE_EVENT_DROPS` declaration. Comment cites data-model.md E2.

**Checkpoint**: `cargo test -p mikebom-common --lib events::filter_category_tag` passes; `cargo xtask ebpf` (if available in dev env) compiles `mikebom-ebpf` with the new map + helper. Foundation ready.

---

## Phase 3: User Story 1 - Cargo builds no longer lose real compiler events to fingerprint spam (Priority: P1) 🎯 MVP

**Goal**: Kernel-side classifier drops System/UserCache/Ephemeral/CargoFingerprint events BEFORE `FILE_EVENTS.reserve()`. On the SC-001 fixture, rustc + linker file events start appearing in the attestation.

**Independent Test**: Container harness (extended in T014) asserts `[.predicate.file_access.operations[] | select(.comm == "rustc")] | length >= 1` on the `two_binaries_diverge` fixture (baseline: 0).

### Tests for User Story 1 ⚠️

> Write these tests FIRST; ensure they FAIL before implementation lands.

- [ ] T007 [P] [US1] Add kernel-side pattern-shape unit tests in `mikebom-ebpf/src/programs/file_ops.rs::tests` (host-side, no eBPF load — pure Rust helpers): assert `path_matches_prefix` correctly matches `/etc/hostname` → System, `~/.cache/foo` → UserCache, `/tmp/bar` → Ephemeral, `target/debug/build/x/fingerprint/y` → CargoFingerprint, and NON-matching paths (`/home/user/src/main.rs`, `target/release/mikebom`) → None. Test also asserts truncated paths (256-byte buffer full, no trailing NUL) return None per FR-016.

### Implementation for User Story 1

- [ ] T008 [US1] Add `PathPattern` `#[repr(C)]` struct (32-byte `bytes`, `len: u8`, `category: u8`, 6-byte pad) in `mikebom-ebpf/src/programs/file_ops.rs`. Place adjacent to (immediately above) `try_openat2`. Define `const PATTERNS: [PathPattern; 15]` covering: `/etc/`, `/proc/`, `/sys/`, `/dev/` (System×4); `/.cache/`, `/.local/share/` (UserCache×2 — matched via any-directory-component, not prefix); `/tmp/`, `/var/tmp/` (Ephemeral×2); `/fingerprint/`, `/deps/`, `/incremental/` (CargoFingerprint×3 — matched via any-directory-component beneath `/target/`); reserve 4 unused slots for future expansion. Comment cites contracts/ebpf-verifier-notes.md Rule 1.
- [ ] T009 [US1] Add `#[inline(always)] fn path_matches_prefix(pattern: &PathPattern, path: &[u8; 256]) -> bool` in same file, using the m211 word-wide u64 compare pattern (4 iterations of `u64::from_le_bytes` + xor + jne). Comment cites contracts/ebpf-verifier-notes.md Rule 2 + Rule 3.
- [ ] T010 [US1] Add `#[inline(always)] fn path_contains_component(path: &[u8; 256], component: &[u8; 32], clen: u8) -> bool` for UserCache/CargoFingerprint's "any directory component" matching. Bounded scan across the 256-byte path buffer looking for `/component/` substring. Verifier-safe: fixed-size, no dynamic indexing bounds.
- [ ] T011 [US1] Add `fn path_matches_filter_category(path: &[u8; 256]) -> Option<FilterCategoryTag>` in same file. Iterates System patterns → UserCache patterns → Ephemeral patterns → CargoFingerprint patterns via unrolled match; returns `Some(cat)` on first hit, `None` if no match. Widen-flag consultation is DEFERRED to US3 (T017); this task hard-codes "System always active". Emit `#[inline(always)]` per Rule 3.
- [ ] T012 [US1] Wire `path_matches_filter_category` into `try_do_filp_open` in `mikebom-ebpf/src/programs/file_ops.rs`: after path is read into local `[u8; 256]` (line ~234) and BEFORE `FILE_EVENTS.reserve()`, call the classifier; if `Some(cat)`, call `increment_filter_category_hit(cat as u8)` and `return Ok(0)` (early exit — no `FILE_EVENTS.reserve()` call, no `FILE_EVENT_DROPS` increment).
- [ ] T013 [US1] Wire same short-circuit into `try_openat2` (line ~154). Identical pattern to T012 — classifier call before `FILE_EVENTS.reserve()`.
- [ ] T014 [US1] Extend `scripts/ebpf-integration-test.sh` with SC-001 jq assertion: `RUSTC_FILE_COUNT=$(jq '[.predicate.file_access.operations[] | select(.comm == "rustc")] | length' "$OUTPUT")` and `[[ "$RUSTC_FILE_COUNT" -ge 1 ]] || (echo "FAIL: 0 rustc file events (m213 SC-001 target)" && exit 1)`. Add a parallel `LINKER_FILE_COUNT` assertion for `ld` / `ld.lld` / `mold` comm names.
- [ ] T015 [US1] Verify US1 end-to-end in Colima: `docker build -f Dockerfile.ebpf-test -t mikebom-ebpf-test . && docker run --rm --privileged -v /sys/kernel/debug:/sys/kernel/debug mikebom-ebpf-test /mikebom/scripts/ebpf-integration-test.sh`. Expected: rustc file events appear; harness passes SC-001. If the verifier rejects the classifier, revert T008–T011 and re-apply per contracts/ebpf-verifier-notes.md Rules 1–5.

**Checkpoint**: The kernel-side filter drops events; rustc file events appear in the attestation. **BUT** — per plan.md Principle VIII analysis, this state is NOT mergeable without US2 (the transparent aggregate). Continue to Phase 4.

---

## Phase 4: User Story 2 - Operator can see which noise categories the filter suppressed (Priority: P2) 🚨 MERGE-BLOCKER for US1

**Goal**: Emit `TraceIntegrity.filter_categories_applied[]` — a sorted-deduplicated list of category names whose kernel-side count > 0. Provides the transparent aggregate that Principle VIII requires as mitigation for US1's event-drop.

**Independent Test**: Container harness (extended in T024) asserts `[.predicate.trace_integrity.filter_categories_applied[] | select(. == "CargoFingerprint")] | length >= 1` on the SC-001 fixture; and asserts empty state `[]` on `mikebom trace capture -- true` per FR-009.

### Tests for User Story 2 ⚠️

> Write these FIRST; ensure they FAIL before implementation lands.

- [ ] T016 [P] [US2] Add wire-shape round-trip test `trace_integrity_serde_populated_filter_categories_applied` in `mikebom-common/src/attestation/integrity.rs::tests`. Populate `TraceIntegrity` with `filter_categories_applied: vec!["CargoFingerprint".into(), "Ephemeral".into(), "System".into()]` alongside a non-zero `ring_buffer_overflows`. Assert `serde_json::to_value(&original) == serde_json::to_value(&round_tripped)` per m212 R4 pattern. Also assert empty state serializes as `[]` (never `null`, never absent) per FR-009.
- [ ] T017 [P] [US2] Add `FilterCategoryHitsSummary` unit tests in `mikebom-cli/src/trace/counters.rs::tests`: (a) `applied_categories()` returns sorted-deduplicated names for a populated `per_category`; (b) empty `per_category` returns `vec![]`; (c) `attach_failures` propagate through — a `filter_category_hits` attach failure emits an empty applied list AND appends to a caller-owned failures vec (per R9 semantics).

### Implementation for User Story 2

- [ ] T018 [US2] Add `filter_categories_applied: Vec<String>` field on `TraceIntegrity` in `mikebom-common/src/attestation/integrity.rs`. Placement: LAST field in the struct so pre-m213 JSON prefix is byte-identical. Apply `#[serde(default)]` for pre-m213 attestation back-compat. NO `#[serde(skip_serializing_if)]` — empty state MUST serialize as `[]` per FR-009.
- [ ] T019 [US2] Add `FilterCategoryHitsSummary` struct in `mikebom-cli/src/trace/counters.rs` per data-model.md E4. Fields: `per_category: BTreeMap<FilterCategoryTag, u64>`, `attach_failures: Vec<String>`. Methods: `applied_categories(&self) -> Vec<String>` (sorted + dedup + filter count > 0 per FR-006).
- [ ] T020 [US2] Add `read_filter_category_hits(bpf: &mut aya::Ebpf) -> FilterCategoryHitsSummary` in same file, mirroring m212's `read_ring_buffer_drops`. Iterates `FilterCategoryTag::ALL`, calls `read_percpu_slot_sum(bpf, "FILTER_CATEGORY_HITS", cat as u32)` for each slot. On attach failure, appends `"filter_category_hits"` to `attach_failures` (single entry per R9, not per-slot). Non-Linux stub returns `FilterCategoryHitsSummary::default()`.
- [ ] T021 [US2] Add `read_percpu_slot_sum(bpf, name, idx) -> anyhow::Result<u64>` helper in same file — parallel to m212's `read_percpu_sum` but takes an explicit slot index. Sums `PerCpuArray::get(&idx, 0)?` result across all online CPUs.
- [ ] T022 [US2] Wire `read_filter_category_hits` call into `mikebom-cli/src/cli/scan.rs::execute_scan` at trace-end. Placement: adjacent to (immediately after) the m212 `read_ring_buffer_drops` call. Populate `TraceIntegrity.filter_categories_applied = summary.applied_categories()`. Append `summary.attach_failures` to `TraceIntegrity.kprobe_attach_failures` (dedup + sort matches m212's merge convention).
- [ ] T023 [US2] Update `mikebom-cli/src/trace/aggregator.rs::finalize` to accept the new field via `TraceStats` — add `filter_categories_applied: Vec<String>` to `TraceStats` and copy into the built `TraceIntegrity`. Update `TraceStats::default` + `LiveStats::snapshot` (returns empty vec).
- [ ] T024 [US2] Extend `scripts/ebpf-integration-test.sh` with SC-003 jq assertions: (a) `.predicate.trace_integrity.filter_categories_applied | type == "array"` (present as a JSON array, not null or missing); (b) `.predicate.trace_integrity.filter_categories_applied | index("CargoFingerprint") != null` on the SC-001 fixture. Add a companion `mikebom trace capture -- true` invocation with a separate jq check: `filter_categories_applied == []` (FR-009).
- [ ] T025 [US2] Extend `scripts/ebpf-integration-test.sh` with SC-002 jq assertion: `[[ "$OVERFLOWS" -le 10 ]] || (echo "FAIL: ring_buffer_overflows=$OVERFLOWS > 10 (m213 SC-002 target)" && exit 1)`. Note: the pre-m213 assertion `[[ "$OVERFLOWS" -gt 100 ]]` from m212 becomes stale post-m213 and MUST be removed alongside adding the new upper-bound assertion.

**Checkpoint**: `TraceIntegrity.filter_categories_applied` appears in every emitted attestation; container harness asserts its presence and content. US1 + US2 together satisfy Principle VIII. **Now mergeable.**

---

## Phase 5: User Story 3 - Operator can opt out of System-category filtering when they need full coverage (Priority: P3)

**Goal**: The existing `--include-system-reads` CLI flag disables the kernel-side System-category filter (only) for the current trace. UserCache/Ephemeral/CargoFingerprint remain filtered.

**Independent Test**: Two-invocation harness check (in T031): default run has `"System"` in `filter_categories_applied` when the traced process reads `/etc/*`; widened run has NO `"System"` entry AND has `/etc/*` events in `file_access.operations`.

### Tests for User Story 3 ⚠️

- [ ] T026 [P] [US3] Add unit test `filter_widen_gates_system_category` in `mikebom-ebpf/src/programs/file_ops.rs::tests` (host-side): assert that `path_matches_filter_category` with `FILTER_WIDEN[0] = 0` returns `Some(System)` on `/etc/hostname`, and with `FILTER_WIDEN[0] = 1` returns `None` on the same path. The other 3 categories return `Some(cat)` in BOTH cases (widen only affects System per FR-010).

### Implementation for User Story 3

- [ ] T027 [US3] Add `FILTER_WIDEN: PerCpuArray<u8>` (1 slot) `#[map]` declaration in `mikebom-ebpf/src/maps.rs`. Placement: adjacent to `FILTER_CATEGORY_HITS` (from T006). Comment cites data-model.md E3.
- [ ] T028 [US3] Update `path_matches_filter_category` in `mikebom-ebpf/src/programs/file_ops.rs` (from T011): after a System-pattern match hits BUT before returning `Some(FilterCategoryTag::System)`, read `FILTER_WIDEN.get(&0, 0)`; if `Some(&1)`, `continue` past the System-match block instead of returning (i.e., check the other three categories, or fall through to `None`). Non-System patterns are UNCHANGED — widen only affects System per FR-010.
- [ ] T029 [US3] Write `FILTER_WIDEN[0]` from `mikebom-cli/src/trace/loader.rs` after program load: `if config.include_system_reads { widen_map.set(&0, &1u8, 0)?; } else { widen_map.set(&0, &0u8, 0)?; }`. Placement: adjacent to the FILE_EVENT_DROPS attach code from m212. On map-attach failure, append `"filter_widen"` to `kprobe_attach_failures` per R9.
- [ ] T030 [US3] Extend `execute_scan` in `mikebom-cli/src/cli/scan.rs` to plumb `args.include_system_reads` into the loader config (already threaded per grep at `scan.rs:104` and `scan.rs:249` — verify the value reaches `loader.rs::load` without loss). Add a `tracing::info!` on trace-start noting `include_system_reads` state per contracts/filter-hits-map.md observability requirement.
- [ ] T031 [US3] Extend `scripts/ebpf-integration-test.sh` with SC-006 assertions: run `mikebom trace capture -- cat /etc/hostname` twice (default + `--include-system-reads`), assert (a) default's `filter_categories_applied` contains `"System"` AND `file_access.operations` has NO `/etc/hostname` entry; (b) widened run's `filter_categories_applied` does NOT contain `"System"` AND `file_access.operations` DOES have `/etc/hostname`.

**Checkpoint**: All 3 user stories independently functional. Ready for polish + pre-PR gate.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Rollups, non-blocking cleanup, cross-cutting verification.

- [ ] T032 [P] Add fail-open unit test `filter_category_hits_attach_failure_surfaces_in_kprobe_failures` in `mikebom-cli/src/trace/counters.rs::tests`. Simulate map-attach failure (returns error from `bpf.map_mut`); assert `FilterCategoryHitsSummary.attach_failures == vec!["filter_category_hits"]` AND `applied_categories() == vec![]`. This covers R9 end-to-end at the userspace boundary.
- [ ] T033 [P] Update `docs/architecture/attestations.md` to describe the new `TraceIntegrity.filter_categories_applied[]` field alongside the m212 `ring_buffer_overflows` section. Cross-link to contracts/filter-category-tag.md.
- [ ] T034 [P] Update `feedback_ebpf_container_test_gap.md` memory entry to catalog "kernel-side classifier verifier rejection" as the 5th eBPF failure class if any T015 iteration hits verifier rejection. Skip if T015 passes on first attempt.
- [ ] T035 Run pre-PR gate locally per CLAUDE.md: `./scripts/pre-pr.sh` — `cargo +stable clippy --workspace --all-targets -- -D warnings` (zero warnings) + `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`). If clippy fires on the new dead-code paths under default features (macOS / linux-x86_64 no-ebpf-tracing), apply the m212 pattern: module-level `#[cfg_attr(not(all(target_os = "linux", feature = "ebpf-tracing"))), allow(dead_code)]`.
- [ ] T036 Verify m212 harness assertions still pass end-to-end alongside the m213 additions: container harness reports both `ring_buffer_overflows ≤ 10` (m213 SC-002) AND the m212 `ring_buffer_overflows` field remains a `type == "number"` (m212 SC-001).

### Final gates

- [ ] T037 Final: verify quickstart.md's 60-second recipe runs end-to-end from a fresh Colima container. Expected output matches the "▲ NEW" markers in quickstart.md.
- [ ] T038 Push branch + open PR against main citing spec/plan/tasks/research/data-model/contracts/quickstart. Include a body section "Test Plan" enumerating: unit-tests (T004, T007, T016, T017, T026, T032), container harness (T014, T024, T025, T031, T036), and pre-PR gate (T035).

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

Given the tight cross-file dependencies within mikebom-ebpf and the single-crate `trace/counters.rs` module, solo sequential execution beats parallel-team overhead. Ordered: T001 → T002 → T003 → T004 → T005 → T006 → T007 → T008 → T009 → T010 → T011 → T012 → T013 → T014 → T015 → T016 → T017 → T018 → T019 → T020 → T021 → T022 → T023 → T024 → T025 → T026 → T027 → T028 → T029 → T030 → T031 → T032 → T033 → T034 → T035 → T036 → T037 → T038.

---

## Notes

- [P] tasks = different files, no dependencies.
- [Story] label maps task to user story for traceability.
- Test-first: verify tests FAIL before implementing (T007 fails until T008-T013; T016 fails until T018; T017 fails until T019-T020; T026 fails until T027-T028).
- Commit after each logical group (per-phase, or per-story within a phase).
- Container harness (T014, T024, T025, T031) is the merge-blocking integration gate; local unit tests alone are insufficient per the feedback_ebpf_container_test_gap memory entry.
- Verifier rejection on ANY kernel in the SC-003 matrix (5.15, 6.1, 6.6, 6.8) is a merge-blocker per FR-013 + SC-004. Rollback per quickstart.md's rollback recipe if T015 fails.
