---

description: "Task list for milestone 132: closeout of milestone-131 SC misses"
---

# Tasks: Close milestone-131 SC misses with grounded targets

**Input**: Design documents from `/specs/132-sc-closeout/`
**Prerequisites**: plan.md (loaded), spec.md (loaded), research.md (loaded), data-model.md (loaded), contracts/sbom-format-mapping-row.md (loaded), quickstart.md (loaded)

**Tests**: This milestone explicitly requests integration tests for each user story per
`plan.md §Scale/Scope` and `quickstart.md §Step 3`. SC verification is itself the
integration-test surface; per-story integration tests are gating for SC-001..SC-004.

**Organization**: Tasks are grouped by user story to enable independent implementation
and testing. US1 and US4 are both P1 and can run in parallel (different files, no
overlap). US2 (P2) and US3 (P3) follow in priority order. The Phase 1 audit-image-pin
task is BLOCKING for any SC-claim PR.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3, US4)
- Include exact file paths in descriptions

## Path Conventions

Single-project Rust workspace at repository root:
- Production code: `mikebom-cli/src/`
- Integration tests: `mikebom-cli/tests/`
- Test fixtures: `mikebom-cli/tests/fixtures/`
- Spec/plan artifacts: `specs/132-sc-closeout/` + `specs/131-quality-metadata-backfill/`
- Documentation: `docs/reference/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Resolve the BLOCKING `<DIGEST>` placeholder per spec §Assumptions Q3
clarification, and capture the baseline numbers any SC-claim PR will measure against.
Both tasks are mechanical and require no code changes.

- [ ] T001 Run `aws sso login` then `aws ecr describe-images --region us-east-1 --repository-name remediation-planner --image-ids imageTag=latest --query 'imageDetails[0].imageDigest' --output text` to resolve the pinned digest; back-substitute the resulting `sha256:...` value into the `<DIGEST>` placeholders in three locations: `specs/132-sc-closeout/spec.md §Assumptions`, `specs/132-sc-closeout/spec.md §Dependencies`, `specs/132-sc-closeout/research.md §Audit Baseline`.
- [ ] T002 Execute the full `specs/132-sc-closeout/quickstart.md §Step 1` + `§Step 2` re-measurement protocol against the pinned digest from T001 to produce `/tmp/mb-rp-132-baseline.cdx.json`, `/tmp/syft-rp-132-baseline.cdx.json`, and `/tmp/mb-rp-132-baseline.scorecard.json`; record the pre-milestone-132 measured values (mismatch count, license stars, supplier stars, weighted score delta) inline in a new commit message body for traceability — these become the "before" half of every SC delta in subsequent PRs.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: None at the code level — milestone 132 is a metadata-emission closeout, and
every user story extends an existing milestone-001 / -012 / -130 / -131 surface. There
are no shared types, no shared services, and no shared infrastructure that all stories
must build on. Phase 2 is intentionally empty so the four user stories can begin in
parallel after Phase 1 completes.

**Checkpoint**: T001 + T002 complete → all four user stories may proceed in parallel.

---

## Phase 3: User Story 1 — Supplier name backfill (Priority: P1) 🎯 MVP

**Goal**: Populate CDX `components[].supplier.name` (+ SPDX 2.3 `Package.originator` +
SPDX 3 `software:supplier`) for every emitted component whose PURL ecosystem matches
the canonical-supplier table. Lifts Supplier Attribution from 2/5 to ≥3/5 on the audit
image.

**Independent Test**: Run `mikebom sbom scan` against the pinned audit image; pipe to
`jq '[.components[] | select(.supplier.name != null)] | length'`; expect the count to
exceed `0.60 × .components | length` (per the `coverageStarsPct` ≥3★ band in
`research.md §SC-003 Threshold`). The `sbom-comparison` harness's
`suppliers.starsA` field MUST be ≥3 against the same pinned digest.

### Tests for User Story 1

- [ ] T003 [P] [US1] Create new integration test file `mikebom-cli/tests/sc_closeout_supplier_attribution.rs` with three test cases: (1) a synthetic fixture image containing one `pkg:cargo`, one `pkg:nuget`, one `pkg:apk` component asserts each emitted component's CDX `supplier.name` matches the FR-005 table; (2) a fixture pre-populating `entry.maintainer = "Custom Maintainer"` asserts `supplier.name == "Custom Maintainer"` (FR-006 wins-over rule); (3) a `pkg:bitbake` fixture asserts `supplier.name == null` (FR-001 edge case — ecosystem not in table). Guard `#[cfg(test)] #[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md convention.

### Implementation for User Story 1

- [ ] T004 [US1] Add `const SUPPLIER_TABLE: &[(&str, &str)]` immediately above the existing `supplier_from_purl` function in `mikebom-cli/src/scan_fs/mod.rs` with the 9 entries enumerated in `data-model.md §Entity: PURL-ecosystem → Supplier-name lookup table` (cargo, nuget, maven, npm, pypi, gem, apk, deb, rpm; golang deliberately omitted to preserve the existing host-heuristic at lines 580–610).
- [ ] T005 [US1] Extend `mikebom-cli/src/scan_fs/mod.rs::supplier_from_purl` to consult `SUPPLIER_TABLE` via linear scan keyed on `purl.ecosystem()` BEFORE falling through to the existing golang host-heuristic. The precedence chain at line 572 (`entry.maintainer.clone().or_else(supplier_from_purl)`) is unchanged — FR-006's "reader-populated value wins" rule already encoded.
- [ ] T006 [US1] Run `cargo +stable test --workspace --test sc_closeout_supplier_attribution` and confirm all three test cases pass. Quote the per-target `N passed; 0 failed` line in the commit message.
- [ ] T007 [US1] Update existing CDX/SPDX byte-identity golden fixtures under `mikebom-cli/tests/fixtures/goldens/` that contain `pkg:cargo` / `pkg:nuget` / `pkg:maven` / `pkg:npm` / `pkg:pypi` / `pkg:gem` / `pkg:apk` / `pkg:deb` / `pkg:rpm` components per FR-003 — these MUST gain the new `supplier.name` field; the change is the intentional additive churn called out in SC-005. Regenerate via `MIKEBOM_REGEN_GOLDENS=1 cargo +stable test --workspace --test golden_byte_identity` and review the diff before committing.

**Checkpoint**: US1 fully functional and independently testable. Supplier Attribution
score must move to ≥3★ on the audit image (verifiable via the quickstart §Step 3.4
command).

---

## Phase 4: User Story 4 — Retrospective milestone-131 SC accounting (Priority: P1, independent)

**Goal**: Edit `specs/131-quality-metadata-backfill/spec.md` so the spec record honestly
reflects what shipped vs what was claimed at PR-merge time. Closes the structural pattern
the maintainer flagged. Independent of all code work — pure documentation edits.

**Independent Test**: `grep -c '\*\*Status (2026-06-19)\*\*' specs/131-quality-metadata-backfill/spec.md`
returns `4`; `grep -c '## Post-Milestone Outcomes (2026-06-19)' specs/131-quality-metadata-backfill/spec.md`
returns `1`. The original milestone-131 SC bullets MUST still appear unchanged (the
amendment APPENDS — does NOT replace per FR-015).

### Implementation for User Story 4

- [ ] T008 [P] [US4] Read `specs/131-quality-metadata-backfill/spec.md` to locate the existing "Success Criteria" section. For each of SC-001, SC-002, SC-003, SC-004, append a new `**Status (2026-06-19)**:` line below the existing bullet per the exact shape in `data-model.md §Entity: Milestone-131 spec amendment §FR-015 — Each SC line gains an appended Status clause`. Preserve the original target text verbatim.
- [ ] T009 [P] [US4] After the existing Success Criteria block in `specs/131-quality-metadata-backfill/spec.md`, insert a new `## Post-Milestone Outcomes (2026-06-19)` section with the verbatim content from `data-model.md §Entity: Milestone-131 spec amendment §FR-016 — New section appended`. Cite the four specific PR numbers (#374, #375, #376, #377). The measured-value cells (e.g. "374" for VERSION_MISMATCH) come from the T002 baseline-measurement output captured in milestone 132's setup phase.
- [ ] T010 [US4] Verify both edits via the two `grep -c` commands from this section's Independent Test. Both counts must be exactly 4 and 1 respectively. SC-007 in this milestone's spec is gated on these two grep results.

**Checkpoint**: US4 complete; milestone-131 spec record now honest. Can land in any PR
landing US1, US2, or US3 — choose the LAST one so the section's "what actually shipped"
table cites all milestone-132 measured values.

---

## Phase 5: User Story 2 — VERSION_MISMATCH realistic scoping (Priority: P2)

**Goal**: Emit a companion `mikebom:assembly-version-informational-stripped` annotation
alongside the existing `mikebom:assembly-version-informational` for every PE/CLR
component whose Informational version contains a `+` build-metadata separator.
Surfaces both representations so consumers picking the stripped form match syft's
behavior. Drops VERSION_MISMATCH from 374 to <50 on the audit image.

**Independent Test**: A PE/CLR component with Informational version
`"4.8.0-7.25569.25+38896ab4..."` emits a CDX `properties[]` entry
`{name: "mikebom:assembly-version-informational-stripped", value: "4.8.0-7.25569.25"}`.
A component with Informational `"5.0.0"` (no `+`) emits NO stripped annotation
(FR-009). The `sbom-comparison` harness's `versions.mismatch` field MUST drop to <50
against the pinned digest.

### Tests for User Story 2

- [ ] T011 [P] [US2] Create new integration test file `mikebom-cli/tests/sc_closeout_version_mismatch_strip.rs` with four test cases: (1) `+sha`-bearing Informational → stripped annotation present + value correct; (2) no-`+` Informational → no stripped annotation per FR-009; (3) `+`-bearing Informational whose stripped prefix fails `is_plausible_version_string` → no stripped annotation per FR-010 (silent skip); (4) end-to-end fixture image with mixed PE/CLR assemblies asserts the audit-image-style pattern.
- [ ] T012 [P] [US2] Add two new fixture inputs to `mikebom-cli/tests/fixtures/parity_catalog/` mirroring the milestone-071 catalog convention: `informational_with_plus.dll.bin` (a synthetic PE/CLR with Informational `"4.8.0-7.25569.25+38896ab4abcdef0123456789"`) and `informational_no_plus.dll.bin` (Informational `"5.0.0"`). Expected SBOMs at `parity_catalog/informational_with_plus.expected.cdx.json` show the stripped annotation; the no-plus expected SBOM shows NO stripped annotation.

### Implementation for User Story 2

- [ ] T013 [US2] Inside the existing `extract_custom_attribute_versions` function in `mikebom-cli/src/scan_fs/package_db/nuget/pe_clr.rs` (milestone 131 PR #377 location), after the `mikebom:assembly-version-informational` annotation is populated and BEFORE the `AssemblyAccumulator` dedup pass, derive a stripped form per `data-model.md §Stripped-Informational version annotation §Derivation rule`: if the value contains `+`, split-once on `+`, run `is_plausible_version_string` on the prefix, and emit `mikebom:assembly-version-informational-stripped = <prefix>` only on sanity pass. Skip silently on no-`+` or sanity-fail.
- [ ] T014 [US2] Append the catalog C-row from `specs/132-sc-closeout/contracts/sbom-format-mapping-row.md` verbatim to the existing "C-rows" section of `docs/reference/sbom-format-mapping.md`. The row registers the new parity-bridging `mikebom:*` annotation per Constitution Principle V's mandate.
- [ ] T015 [US2] Run `cargo +stable test --workspace --test sc_closeout_version_mismatch_strip` and `cargo +stable test --workspace --test parity_catalog_cdx` and `cargo +stable test --workspace --test parity_catalog_spdx23` and `cargo +stable test --workspace --test parity_catalog_spdx3`. All four targets MUST report `N passed; 0 failed`. The catalog tests pick up the new C-row automatically and verify the documented emission shape against the new fixtures from T012.

**Checkpoint**: US2 fully functional. `versions.mismatch` on the pinned digest scorecard
must drop to <50 (verifiable via the quickstart §Step 3.2 command).

---

## Phase 6: User Story 3 — License Coverage extension (Priority: P3)

**Goal**: Lift License Coverage from 37.8 % (2★) to ≥60 % (3★) on the audit image. Per
research.md §License Path Analysis, this requires Path C (deps.dev online enrichment
for `pkg:cargo` and `pkg:nuget`) as the primary path, with Path A (extended PE/CLR
fingerprinting) as an offline-mode complement. Path B (rootfs-local cargo cache) was
rejected — see research.md for the decision matrix.

**Independent Test**: Two scan modes — offline (`--offline`) and online
(`--enrich-licenses=depsdev`). The online scan's `sbom-comparison` harness output MUST
show `licenses.effectiveRateA >= 60.0` and `licenses.starsA >= 3`. The offline scan
must still show no regression vs milestone-131 baseline (proves Path A complement is
purely additive). cargo components MUST carry `mikebom:license-source = "depsdev"` in
the online mode; nuget components not previously matched by the fingerprint table MUST
carry the same annotation.

### Tests for User Story 3

- [ ] T016 [P] [US3] Create new integration test file `mikebom-cli/tests/sc_closeout_license_coverage.rs` with two scan-mode test cases: (1) `--offline` scan of a synthetic PE/CLR fixture carrying an MS-PL-style LICENSE.txt asserts the new Path A fingerprint match emits `licenses[].license.id = "MS-PL"`; (2) `--enrich-licenses=depsdev` against a wiremock-backed deps.dev stub asserts a cargo component receives a deps.dev-sourced license expression + `mikebom:license-source = "depsdev"` annotation. The wiremock stub serves the same JSON shape documented at the deps.dev v3 cargo endpoint (per research.md §Best-practices research US3 deps.dev cargo support).
- [ ] T017 [P] [US3] Generate canonical first-64-byte byte slices for the 7 new license texts (MS-PL, LGPL-2.1-only, LGPL-3.0-only, LGPL-2.1-or-later, MIT-0, EPL-1.0, EPL-2.0) and stage them as `mikebom-cli/tests/fixtures/license_fingerprints/<spdx-id>.first64bytes.bin`. The fingerprint-table extension in T018 will `include_bytes!` these.

### Implementation for User Story 3 — Path A (offline fingerprint extension)

- [ ] T018 [US3] In `mikebom-cli/src/scan_fs/package_db/nuget/pe_clr.rs`, extend the existing milestone-131 `LICENSE_FINGERPRINT_TABLE` constant with 7 new entries per `data-model.md §License-enrichment dispatch (US3 Path A)`. Use `include_bytes!("../../../tests/fixtures/license_fingerprints/<spdx-id>.first64bytes.bin")` for each new fingerprint. Add a `#[test] fn fingerprint_spdx_ids_are_canonical()` that runs each entry's SPDX ID through `mikebom_common::types::license::SpdxExpression::try_canonical` — typos fail at unit-test time, not at production scan time.

### Implementation for User Story 3 — Path C (online deps.dev enrichment)

- [ ] T019 [US3] In `mikebom-cli/src/enrich/depsdev_source.rs` (existing milestone-012 scaffolding), add a `"cargo"` arm to the existing `match purl.ecosystem()` dispatch returning the URL `https://api.deps.dev/v3/systems/CARGO/packages/<urlencoded-name>/versions/<urlencoded-version>`. The response deserialization, retry/timeout/cache layers, and `DepsDevLicense` newtype emission already exist for nuget and are reused as-is.
- [ ] T020 [US3] Add a new `--enrich-licenses=<source>` clap flag to `mikebom sbom scan` (the new flag lives on the existing `ScanArgs` struct). Supported value `depsdev` only for this milestone; the value enum uses clap's `ValueEnum` derive. The flag REQUIRES the absence of `--offline` (clap `conflicts_with = "offline"`); when set, the integration point in `mikebom-cli/src/scan_fs/mod.rs` (post-resolution hook) iterates every `ResolvedComponent` with PURL ecosystem in `{cargo, nuget}` and calls the `depsdev_source.rs` enrichment routine. Components that already carry a non-empty `licenses[]` from upstream readers or from Path A are skipped (no overwrite). The enrichment annotation `mikebom:license-source = "depsdev"` is set on every component that gained licenses from this path.
- [ ] T021 [US3] Implement transparency annotations per Constitution Principle X / XII / research.md §Path C failure-mode handling: deps.dev 404 → `mikebom:license-source = "depsdev-not-found"`; network timeout → `mikebom:license-source = "depsdev-unavailable"`; rate-limit (429) after 3 backoff retries → `mikebom:license-source = "depsdev-rate-limited"`. The scan MUST emit successfully in all failure modes — Principle III "Fail Closed" applies to the trace path, not to the enrichment path; here, graceful degradation is the contract.

### Verification for User Story 3

- [ ] T022 [US3] Run `cargo +stable test --workspace --test sc_closeout_license_coverage`; both test cases MUST report `passed`. Quote the full per-target `N passed; 0 failed` line in the commit message.
- [ ] T023 [US3] Execute `specs/132-sc-closeout/quickstart.md §Step 1 + §Step 2 + §Step 3.3` against the pinned digest from T001; record the measured `licenses.starsA` and `licenses.effectiveRateA` from the resulting scorecard JSON in the PR description's measured-vs-target table per quickstart §Step 5. Both numbers MUST satisfy SC-003 (≥3 stars / ≥60.0 %).

**Checkpoint**: US3 fully functional in both offline and online modes. SC-003 met on
the pinned digest. License Coverage on `mikebom-cli`'s scorecard against syft MUST show
mikebom at ≥3★.

---

## Phase 7: Polish & Cross-Cutting

**Purpose**: Final verification — pre-PR gate, full SC verification, CHANGELOG entry.

- [ ] T024 Run `./scripts/pre-pr.sh` (the mandatory pre-PR gate per CLAUDE.md). Both `cargo +stable clippy --workspace --all-targets` (zero errors) AND `cargo +stable test --workspace` (every suite `N passed; 0 failed`) MUST pass. Per the standing "Pre-PR gate: full output, don't grep" feedback, paste the per-target `N passed; 0 failed` lines verbatim into the PR description; do NOT cite a failure-grep result.
- [ ] T025 Re-run `specs/132-sc-closeout/quickstart.md` end-to-end against the pinned digest. Populate the measured-vs-target table per `quickstart.md §Step 5`. Each of SC-001, SC-002, SC-003, SC-004, SC-005, SC-006, SC-007 MUST be marked MET. If any is NOT MET, the PR does not land — open a follow-up issue first, do not silently merge.
- [ ] T026 [P] Append a new `[Unreleased]` CHANGELOG entry to `/Users/mlieberman/Projects/mikebom/CHANGELOG.md` describing milestone 132's three behavioral additions: (1) US1 supplier-name backfill + the table of ecosystems; (2) US2 `mikebom:assembly-version-informational-stripped` annotation; (3) US3 `--enrich-licenses=depsdev` flag + offline-mode fingerprint-table extension. Cite the pinned digest from T001 in the SC-verification line for traceability.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: ECR reauth (T001) MUST precede T002. Both must complete before any SC-claim PR opens. Phase 2 is empty, so Phase 1 transitions directly into the user-story phases.
- **Phase 2 (Foundational)**: empty. Skip directly to user stories.
- **Phase 3 (US1)**: starts after T001 + T002. Independent of US2, US3, US4.
- **Phase 4 (US4)**: starts after T002. Independent of US1, US2, US3. Pure doc edits — can land in any of US1/US2/US3's PR.
- **Phase 5 (US2)**: starts after T001 + T002. Independent of US1, US3, US4.
- **Phase 6 (US3)**: starts after T001 + T002. Independent of US1, US2, US4.
- **Phase 7 (Polish)**: depends on US1 + US2 + US3 + US4 complete.

### User Story Dependencies

- **US1 (P1)**: depends only on Phase 1.
- **US2 (P2)**: depends only on Phase 1.
- **US3 (P3)**: depends only on Phase 1. (Note: spec.md FR-012 said US3 was BLOCKED on research, but research is complete — see research.md §License Path Analysis decision matrix. Path C is the chosen path. US3 can now proceed.)
- **US4 (P1)**: depends only on T002 (so the "what actually shipped" table can cite measured values).

All four user stories are independent and can run in parallel if multiple developers are
available. With a single developer, run in priority order: US1 → US4 → US2 → US3.

### Within Each User Story

- Tests can be written before OR after implementation in this milestone (no strict TDD requirement per spec). The recommended order is tests first (T003 / T011 / T016) so the implementation tasks (T004–T007 / T013–T015 / T018–T021) have a failing-test target to drive against.
- US1: T003 (test) → T004 (table) → T005 (lookup) → T006 (run tests) → T007 (regen goldens). T004 + T005 same file → sequential.
- US2: T011 + T012 (parallel: different files) → T013 (impl) → T014 (catalog row) → T015 (run tests).
- US3: T016 + T017 (parallel: different files) → T018 (Path A) || T019 (Path C scaffolding) → T020 (flag wiring) → T021 (transparency) → T022 (test) → T023 (SC verification).
- US4: T008 + T009 (parallel: different sections of same file — still parallel because the edits are at different line ranges) → T010 (grep verification).

### Parallel Opportunities

- After T001 + T002 complete: US1, US2, US3, US4 can be launched in parallel by 4 developers.
- Within US1: T003 (test write) is [P] vs T004/T005 (impl); could overlap.
- Within US2: T011 (test write) + T012 (fixtures) marked [P] — different files, no dependencies.
- Within US3: T016 (test write) + T017 (fixture generation) marked [P]. T018 (Path A in pe_clr.rs) and T019 (Path C in depsdev_source.rs) marked [P] — different files.
- Within US4: T008 + T009 marked [P] — same file but different section-level edits.
- T026 (CHANGELOG) marked [P] — independent of T024 + T025 sequencing.

---

## Parallel Example: US1 + US4 + US2 + US3 launched together

After T001 + T002 land in their setup commit:

```bash
# Four developers, one per user story:
Task: "T003 Create supplier-attribution integration test in mikebom-cli/tests/sc_closeout_supplier_attribution.rs"
Task: "T008 Append Status (2026-06-19) clauses to milestone-131 spec's SC-001..SC-004"
Task: "T011 Create version-mismatch-strip integration test in mikebom-cli/tests/sc_closeout_version_mismatch_strip.rs"
Task: "T016 Create license-coverage integration test in mikebom-cli/tests/sc_closeout_license_coverage.rs"
```

For a single developer, launch the within-story parallel tasks together:

```bash
# Inside US3 (after T001 + T002):
Task: "T016 Create license-coverage integration test in mikebom-cli/tests/sc_closeout_license_coverage.rs"
Task: "T017 Generate canonical first-64-byte license-text fixtures in mikebom-cli/tests/fixtures/license_fingerprints/"

# Then after both land:
Task: "T018 Extend LICENSE_FINGERPRINT_TABLE in mikebom-cli/src/scan_fs/package_db/nuget/pe_clr.rs"
Task: "T019 Add cargo arm to deps.dev dispatch in mikebom-cli/src/enrich/depsdev_source.rs"
```

---

## Implementation Strategy

### MVP First (US1 + US4)

US1 (Supplier Attribution → 3★) + US4 (milestone-131 spec amendment) is the smallest
shippable increment that closes ONE of the four milestone-131 SC misses + addresses the
structural premature-completion pattern. Suggested as the MVP because:

1. US1 is a pure-additive metadata change (zero behavior change at the trace level).
2. US4 is documentation-only.
3. Together they close SC-004 and SC-007 — two of the seven milestone-132 SCs — in a
   single small PR.
4. Surface area for review is minimal: one constant table + one source-file edit + one
   spec-file edit.

Sequence:

1. T001 → T002 (Setup; resolve `<DIGEST>`).
2. T003 → T004 → T005 → T006 → T007 (US1).
3. T008 + T009 (parallel) → T010 (US4).
4. T024 → T025 → T026 (Polish).
5. **STOP and VALIDATE**: scorecard shows Supplier Attribution at ≥3★; milestone-131
   spec carries the Status / Post-Milestone Outcomes amendments.
6. Open MVP PR; land; observe behavior on the pinned digest.

### Incremental Delivery

After the MVP PR lands, the next increment is US2 (VERSION_MISMATCH → <50), then US3
(License Coverage → ≥3★). Each lands as its own PR with its own re-measurement
evidence per quickstart §Step 5.

### Parallel Team Strategy

With 4 developers post-setup:

- Dev A: US1 (T003 → T007)
- Dev B: US4 (T008 → T010)
- Dev C: US2 (T011 → T015)
- Dev D: US3 (T016 → T023)

All four merge their PRs in priority order (US1 first, US4 with US1 or alone, then US2,
then US3). The final polish PR (T024 → T026) is by whoever lands US3 — they re-run the
full quickstart and update the CHANGELOG with all measured SC values cited.

---

## Notes

- [P] = different files, no dependencies on incomplete tasks.
- [Story] label maps every user-story-phase task back to spec.md user stories US1..US4.
- Each user story is independently completable and testable in isolation; SC-001..SC-007
  verification is end-to-end in T025.
- Pre-PR gate (T024) is MANDATORY per CLAUDE.md; "passing per-crate `cargo test` is not
  evidence of CI-readiness" per the standing constitution.
- The `<DIGEST>` placeholder resolution (T001) BLOCKS every SC-claim PR. A PR that does
  not back-substitute the digest into the three spec/research locations MUST NOT cite
  SC closure.
- Commit after each task or logical group; "Commits should be small enough to bisect
  through" per the project convention.
- Stop at any checkpoint to validate the user story independently before continuing.
- Avoid: vague tasks, same-file conflicts not marked sequential, declaring SCs MET
  without re-measuring against the pinned digest (the exact pattern milestone 132
  exists to remediate).
