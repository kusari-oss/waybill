# Tasks: Per-resolve SBOMs for Pants monorepos

**Input**: Design documents from `/specs/911-per-resolve-sboms/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/resolve-membership.md, quickstart.md
**Issue**: [#902](https://github.com/kusari-oss/waybill/issues/902) items 1, 3, 4

**Tests**: Test tasks are included. The spec's success criteria are almost
entirely about output shape and determinism, which is only checkable by
asserting on emitted documents — and the defect being fixed is *silent*, so a
test that does not fail against the pre-change binary proves nothing here.

**Organization**: by user story. US3 depends on US1; US2 is independent of
both in code.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: different files, no dependency on an incomplete task.
- Reader changes across the three readers are genuinely parallel; everything
  touching `split.rs` or `deduplicator.rs` is not.

## Path Conventions

Repository root. Primary files: `waybill-cli/src/scan_fs/package_db/{pants,pants_jvm,pip}/`,
`waybill-cli/src/resolve/deduplicator.rs`, `waybill-cli/src/generate/split.rs`,
`waybill-cli/src/parity/extractors/`, `docs/reference/sbom-format-mapping.md`.

---

## Phase 1: Setup — establish the baseline

**Purpose**: capture the pre-change behaviour so every later assertion has
something to fail against. The defect is invisible in a single run.

- [X] T001 Create `specs/911-per-resolve-sboms/measurements/` and extend the m910 fixture at `waybill-cli/tests/fixtures/pants_resolve_edges/` with a package pinned by BOTH resolves at the SAME version. The existing fixture has same-name-different-version, which is a different case; this feature turns on the same-version one.
- [X] T002 Record the pre-change output to `measurements/baseline.md`: scan the extended fixture, show the shared package naming exactly one resolve, and show which one by running the scan twice with component read order perturbed. Order-dependence is the property that makes this defect invisible in any single run.
- [X] T003 [P] **Not reproducible in this workspace** — the repository is the issue author's; recorded as such in `measurements/baseline.md` and handed back with a pre-release build rather than faked. Confirm the reported monorepo figures still hold on current `main` (20 of 24 resolves surviving, 4 empty) and record in `measurements/baseline.md`. #910 and #901 landed after those figures were taken; neither touches membership or dedup, so they are *expected* unchanged — confirming that cheaply beats assuming it. If they have moved, the spec's problem statement needs revisiting before any code changes.

**Checkpoint**: the defect is observable on demand, not just described.

---

## Phase 2: Foundational — the four internal read-back sites

**Purpose**: waybill reads this annotation back via `.as_str()`, which returns
`None` once membership is an array — silently falling through to a default.

**Corrected during implementation**: of the four sites the analysis listed, only
**two are production code**, and they are the two that matter — both in
`scan_fs/mod.rs`, both carrying #910's edge-scoping fix, which would have
reverted with nothing failing. The other two (`pants/mod.rs:690`,
`pants_jvm/lockfile.rs:739`) turned out to be inside `#[cfg(test)]`; they still
need migrating, but they are test helpers, not blast radius.

The phase stands. The risk was correctly identified even though its size was
overstated by half.

- [X] T004 Add a shared accessor for reading resolve membership — one function that returns the resolve names from the annotation regardless of encoding — in `waybill-cli/src/scan_fs/package_db/pants/mod.rs` (or a location all three readers and `scan_fs/mod.rs` can reach). Every read-back goes through it so a future encoding change has one site, not four.
- [X] T005 Migrate `waybill-cli/src/scan_fs/mod.rs:674` (the scope-qualified index build, #910) to the accessor. A component whose membership is an array must still index under each of its resolves.
- [X] T006 Migrate `waybill-cli/src/scan_fs/mod.rs:1079` (the per-entry scope lookup, #910) to the accessor. **Scope reduced by the T002 baseline**: edges are emitted per entry at `mod.rs:1081`, before `deduplicate` at `mod.rs:1253`, and an entry comes from one lockfile — so at this point a requirer belongs to exactly ONE resolve and FR-011b already holds. This task handles the array encoding without breaking #910's per-entry scoping; it does NOT implement multi-resolve resolution, which the architecture already produces.
- [X] T006b Guard the ordering fact T006 now depends on: assert in `waybill-cli/tests/pants_resolve_membership.rs` that a component in two resolves reaches BOTH versions of a differently-pinned dependency (FR-011b/SC-007a). The behaviour is emergent rather than intended — nothing states it today, so nothing protects it.
- [ ] T006a Confirm #910's existing fixture is unaffected: its components each belong to exactly ONE resolve, so FR-011b changes nothing there. If `waybill-cli/tests/pants_resolve_scoped_edges.rs` starts failing, the multi-resolve path has leaked into the single-resolve case.
- [X] T007 [P] Migrate `waybill-cli/src/scan_fs/package_db/pants/mod.rs:688` to the accessor.
- [X] T008 [P] **Was a test assertion, not a read-back site** — the line is inside `mod tests`, so it belongs with the writer change, not the reader migration. Handled by the decoding helper in `waybill-cli/tests/pants_coursier_jvm_reader.rs`.
- [X] T009 Add a regression test asserting #910's resolve-scoped edges still hold once membership is an array, in `waybill-cli/tests/pants_resolve_scoped_edges.rs`. Without this, T005/T006 regressing is silent — the edges simply fall back to the flat index and look plausible.

**Checkpoint**: every internal reader survives the encoding change, proven by a test that fails if it does not.

---

## Phase 3: User Story 1 — Every resolve that contains a package says so (P1)

**Goal**: membership is plural, unions at dedup, and does not depend on read order.

**Independent test**: two resolves pinning one package at one version — both name it, in both of two scans with perturbed order.

### Tests first

- [X] T010 [US1] Write the failing test for C-1/C-2 in `waybill-cli/tests/pants_resolve_membership.rs`: the shared package's membership is `["app","tools"]`. Confirm it fails against the pre-change binary for its own reason, not a compile error.
- [X] T011 [P] [US1] Write the determinism test (FR-003/SC-003) in `waybill-cli/tests/pants_resolve_membership_determinism.rs`: scan twice with component read order perturbed, assert byte-identical membership.
- [X] T012 [P] [US1] Write the single-resolve test (FR-006a/SC-004) in `waybill-cli/tests/pants_resolve_membership.rs`: a one-resolve component emits `["app"]`, not `"app"`. This one must fail pre-change too — the old binary emits the scalar.
- [X] T013 [P] [US1] Write the cross-format test (FR-006/SC-005) in `waybill-cli/tests/pants_resolve_membership_parity.rs`: same package, all three formats, same array and same order.

### Implementation

- [X] T014 [US1] Emit membership as a lex-sorted JSON array in `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs:399` and `:537`.
- [X] T015 [P] [US1] Same in `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs:395`.
- [X] T016 [P] [US1] Same in `waybill-cli/src/scan_fs/package_db/pip/uv_lock.rs:227` — the uv reader stamps this when uv is a Pants resolver backend (`uv_lock.rs:188`). Easy to miss; a reader left on the scalar reintroduces exactly the cross-reader inconsistency #901 fixed.
- [X] T017 [US1] Add the per-key union merge policy at the annotation merge in `waybill-cli/src/resolve/deduplicator.rs` (`or_insert`, currently line 214 — cite the call, not the line, which drifts). An explicit allowlist of plural keys — NOT a blanket change from `or_insert` to union, and NOT a rule inferred from the value being an array. `waybill:source-files` and `waybill:file-paths` are already arrays and already unioned by m148's dedicated pass; a generic rule would double-handle them, and unioning `waybill:sbom-tier` would produce a value true of neither side.
- [X] T018 [US1] Ensure the union output is lex-sorted and duplicate-free at the merge site in `waybill-cli/src/resolve/deduplicator.rs`, so FR-003 holds regardless of which component won the merge.
- [X] T019 [US1] Update the C143 row grammar in `docs/reference/sbom-format-mapping.md:183` — value is now a lex-sorted JSON array. The row's KEEP-NO-NATIVE audit is unchanged: no CDX/SPDX native carrier has appeared for "which build-tool resolve owns this".
- [X] T020 [US1] Verify the C143 extractors in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` still compare correctly with an array value; adjust if the comparison assumed a scalar.

### Verification

- [X] T021 [US1] Teeth-check T010–T013 against the pre-change binary and record which fail and why in `measurements/us1-verification.md`. A test that passes on both sides is a guard, not a defect test — label them as such rather than counting them as proof.
- [X] T022 [US1] Re-run the T003 monorepo measurement and record the after figures in `specs/911-per-resolve-sboms/measurements/us1-verification.md`: every resolve containing packages represented, none empty that is not genuinely empty (SC-002, SC-009).

**Checkpoint**: partitioning has something correct to partition on. US3 is unblocked.

---

## Phase 4: User Story 2 — A document says whether its resolves were declared (P2)

**Goal**: a consumer can tell declared from discovered from the document alone.

**Independent test**: one declaring fixture, one convention-only fixture; the two documents differ on that point.

**Independent of US1 in code** — it extends a different annotation. Ordered second because it is worth less alone.

- [ ] T023 [US2] Add a convention-only fixture at `waybill-cli/tests/fixtures/pants_discovered_resolves/` — lockfiles matching `3rdparty/python/*.lock`, no `[python.resolves]` in `pants.toml`.
- [ ] T024 [US2] Write the failing test for C-4/SC-006 in `waybill-cli/tests/pants_resolve_ownership.rs`: the document names which resolves were declared and which discovered, and the two lists together account for every resolve named on any component.
- [ ] T025 [US2] Replace the C161 wire form at `waybill-cli/src/scan_fs/package_db/pants/mod.rs:385-393` with a JSON object naming the resolves per category: `{"declared":[…],"discovered":[…],"weak_classification":N}`. This replaces the semicolon-delimited `key=value` grammar rather than nesting inside it — a flat scalar cannot carry a list without a second delimiter level, and JSON is what every other plural value here uses. An existing reader of the count form breaks loudly, which is the intended failure mode.
- [ ] T026 [US2] Update the C161 row in `docs/reference/sbom-format-mapping.md` to match, and the extractors at `waybill-cli/src/parity/extractors/mod.rs:642` if the grammar changed. A catalogue row and its extractors must move together or `every_catalog_row_has_an_extractor` fails.
- [ ] T027 [US2] Assert FR-009 still holds in `waybill-cli/tests/pants_resolve_ownership.rs`: a discovered resolve is named but gains **no** anchor component. Naming is information, not an ownership claim the repository never made.

**Checkpoint**: a consumer can decide whether to split without inspecting the repository.

---

## Phase 5: User Story 3 — waybill can do the partitioning itself (P3)

**Goal**: `--split=resolve` produces one document per resolve.

**Depends on US1**: the filter has nothing correct to filter on until membership is complete. Splitting on today's membership would produce a confidently wrong partition — four empty documents and shared packages in one arbitrary place.

- [ ] T028 [US3] Add `SplitMode::Resolve` to the enum in `waybill-cli/src/generate/split.rs:48` and to the `--split` value surface at `waybill-cli/src/cli/scan_cmd.rs:768`.
- [ ] T029 [US3] Implement membership-filter projection in `waybill-cli/src/generate/split.rs` — components whose membership contains resolve R, plus relationships whose endpoints are both in that set. **Not** `project_for_root`, which BFSes from a seed (`split.rs:311`): a discovered resolve has no seed, so a walk cannot reach it. Research R1 is the argument; if this drifts back to a walk, SC-006a is the check that catches it.
- [ ] T030 [US3] Resolve the sub-document root question (C-5a): a resolve projection contains no `component-role = "main-module"`, so m127's root-selector would fall through to its synthetic-placeholder branch and name every sub-SBOM unhelpfully — the failure m215 hit and documented at `split.rs:355-380`, where 23 of 25 sub-SBOMs named the repository instead of themselves. The split must name its own root: the anchor where one exists, synthesised where it does not.
- [ ] T031 [US3] Ensure a synthesised root exists ONLY inside split output, asserted in `waybill-cli/tests/pants_split_resolve.rs`. Emitting one into the unsplit document would anchor discovered resolves by the back door and contradict FR-009.
- [ ] T031a [US3] Confirm edge separation (FR-011c) needs no new mechanism in `waybill-cli/src/generate/split.rs`: an edge belongs to resolve R when both endpoints name R, which the membership filter from T029 already produces. If the implementation reaches for an edge-level resolve tag, that is a sign the filter is wrong rather than that a tag is needed.
- [ ] T032 [US3] Preserve full membership per document (FR-011a) in `waybill-cli/src/generate/split.rs`: the `app` document records `["app","tools"]` for a shared package, not `["app"]`. Narrowing recreates the under-reporting this feature fixes, moved to document scope and unrecoverable without re-scanning.
- [ ] T033 [US3] Implement the FR-012 fallback: no resolves, or none containing packages → stated outcome plus the single-document fallback, matching the existing `roots.len() <= 1` behaviour at `split.rs:816`. Never an empty directory and exit zero.
- [ ] T033a [US3] Add a fixture at `waybill-cli/tests/fixtures/pants_multi_resolve_member/` where one component belongs to two resolves AND its declared dependency is pinned at different versions in each — the FR-011b case. The m910 fixture does not cover it: its components each belong to exactly one resolve.
- [ ] T033b [US3] Test SC-007a in `waybill-cli/tests/pants_split_resolve.rs`: both edges present in the unsplit document, exactly one in each per-resolve document.
- [ ] T034 [US3] Write the split tests in `waybill-cli/tests/pants_split_resolve.rs` covering SC-007 (one document per resolve, shared package in each with full membership), SC-006a (a convention-only repository partitions with no anchors anywhere), C-5a (each document's root names its own resolve) and C-5b (the unpartitionable case).
- [ ] T035 [US3] Assert C-6 against the existing split tests in `waybill-cli/tests/`: `--split=workspace` and `--split=directory` output is byte-identical to before. A new mode must not perturb the two that exist.

**Checkpoint**: the partitioning rule lives in one place instead of once per consumer.

---

## Phase 6: Polish & cross-cutting

- [ ] T036 Run the mandatory pre-PR gate: `./scripts/pre-pr.sh`. Enumerate the per-target results rather than citing the exit code.
- [ ] T037 Refresh the public-corpus goldens **once**, via `public-corpus.yml` with `regen_goldens`. 72 components carry C143 (django 34, jvm 27, python 11) and all churn, because the encoding changes for single-resolve components too. Goldens are CI-generated — never locally. This is the third consecutive feature to churn Pants goldens; refreshing per story would triple the cost.
- [ ] T038 Verify the golden churn with a masked, sorted diff — mask content-addressed SPDX 3 IDs, document IRIs, serial numbers and bom-refs. Expect THREE kinds of change and nothing else: the membership encoding, the C161 grammar, and — new with FR-011b — additional edges wherever a corpus component belongs to more than one resolve. Unlike the first two, the edge change is semantic, so it must be explained component by component rather than waved through as cascade.
- [ ] T039 [P] Write the CHANGELOG entry for FR-006b. This must be explicit that an existing reader of `waybill:pants-resolve` will **mis-parse rather than fail** — it receives an array where it expected a string and carries on. For a downstream security tool partitioning on this value, a silent mis-parse is the worse failure, which is why it is announced rather than left to be discovered.
- [ ] T040 [P] Update `docs/` wherever per-resolve SBOMs are described, including how a consumer chooses between splitting and falling back.
- [ ] T041 Comment on #902 with before/after figures and offer the pre-release build for validation against the large monorepo. The two numbers that matter: distinct resolve names surviving, and whether any resolve is empty that is not genuinely empty.

---

## Dependencies

```
Phase 1 (T001-T003)   baseline — the defect is invisible without it
        │
Phase 2 (T004-T009)   the four internal read-backs  ◀── BLOCKING
        │                two of them carry #910's fix
        │
Phase 3 US1 (T010-T022)  ◀── MVP. Membership plural, unioned, deterministic.
        │
        ├─────────────── Phase 4 US2 (T023-T027) — independent in code
        │
Phase 5 US3 (T028-T035)  ◀── needs US1; filtering on broken membership
        │                     yields a confidently wrong partition
Phase 6 (T036-T041)      golden refresh ONCE, at the end
```

**Story independence**: US2 is genuinely independent and could ship alone.
US3 is not independent of US1 and the plan says so rather than pretending
otherwise. US1 is the MVP and is independently valuable — a consumer doing its
own walk needs only correct membership.

## Parallel opportunities

Real, and confined to the reader layer:

- **T007, T008** — different readers' read-back sites.
- **T015, T016** — `pants_jvm` and `uv_lock` emission, different files.
- **T011, T012, T013** — three test files.
- **T039, T040** — CHANGELOG and docs.
- **Phase 4 entire** can run alongside Phase 3 — different annotation, different files.

Not parallel: anything touching `split.rs` (T028–T033) or `deduplicator.rs`,
and T014 must land before T017 has anything plural to union.

## Implementation strategy

**MVP = Phases 1, 2, 3.** Roughly 22 tasks, delivering correct membership.
That alone unblocks the consumer that raised #902 — they can walk it
themselves and said so. US2 and US3 are reach and convenience respectively.

Phase 2 is the part most likely to be skipped and least safe to skip. Four
internal sites read this annotation with `.as_str()`; an array makes all four
return `None` and fall through to a default, and two of them carry #910's
edge-scoping fix. Nothing would fail — the edges would quietly revert to the
flat index and look plausible. T009 exists specifically so that does not
happen silently.

## Verification standard

Two rules carried from this project's practice:

1. **A test that passes against the pre-change binary is a guard, not a proof.**
   T021 requires labelling which is which rather than counting all of them as
   evidence.
2. **Corpus goldens are CI-generated.** Never regenerate locally, and refresh
   once at the end rather than per story.
