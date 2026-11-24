# Tasks: Relabel a component tier in an edited SBOM (`sbom edit --retier`)

**Input**: `specs/1651-edit-retier/` (spec.md, plan.md, research.md, data-model.md, contracts/cli.md, quickstart.md)

**Tests**: requested by the spec (SC-001 to SC-006). Fixture cases are written red before the code that makes them pass.

**Paths**:
- `ED` = `waybill-cli/src/edit/`
- `CE` = `waybill-cli/src/cli/edit.rs`
- `LP` = `waybill-cli/src/generate/lifecycle_phases.rs`
- `PX` = `waybill-cli/src/parity/extractors/`
- `IT` = `waybill-cli/tests/sbom_edit_retier.rs` (new)
- `FX` = `waybill-cli/tests/fixtures/sbom_edit_retier/` (new)
- `MS` = `specs/1651-edit-retier/measurements/`

Research decisions are R1–R9. Cases are contracts/cli.md's R1–R6, T1, T2, C1 and V1, plus the refusals.

## Phase 1: Setup

- [X] T001 Create `FX/project/`, a synthetic two-part npm tree: *As built: confirmed in `MS/fixture.md`; the completeness reason is `transitive-edges-unresolvable: npm`, which C1 uses.*
  - `locked/`: `package.json` plus `package-lock.json`, declaring `waybill-fixture-locked-dep`, which scans as `source`;
  - `declared/`: `package.json` only, declaring `waybill-fixture-declared-dep` and a dev dependency `waybill-fixture-dev-dep`, which scan as `design`, one of them `scope: excluded`.

  Add `FX/README.md` saying the tests scan it at run time; nothing in it is a golden. Confirm with one `waybill --offline sbom scan` that the scan carries both `source` and `design`, and record the tiers in `MS/fixture.md`. npm without a lockfile gives `design`: the committed npm-express golden has 27 `required` and 16 `excluded` `design` components (analyze U5).
- [X] T002 Create `IT` with the harness. Reuse `tests/sbom_edit_support/mod.rs` (`edit`, `edit_to`, `components`, `edges`) by `#[path]` module, as the other `sbom_edit_*.rs` tests do. Add: *As built: `IT` includes `tests/common` too, since the support module needs it; tiers are compared on CycloneDX, and cross-format agreement on C5 and C243 comes from `parity-check` in every multi-format case.*
  - `scan(project, extra_flags) -> [PathBuf; 3]`: an offline three-format scan into a tempdir, with `HOME` isolated;
  - `tier_of(doc, name)` for all three formats, and `marker_of(doc, name)` for C243;
  - `derivation_ops(doc)`;
  - `parity(dir)`, which runs `waybill sbom parity-check --scan-dir` and requires no gap where a format carries a value;
  - `doc_type(doc)`, the CycloneDX `lifecycles`, the SPDX 2.3 comment phases and the SPDX 3 `software_sbomType`.

## Phase 2: Foundational

- [X] T003 In `LP`, add `aggregate_phases_from_tiers` and `aggregate_spdx3_sbom_types_from_tiers`, over `impl IntoIterator<Item = &str>` plus `Option<SbomType>`, and make `aggregate_phases` and `aggregate_spdx3_sbom_types` call them (R5). Unit test: for a mixed tier set, the old and new functions agree. Emission must be unchanged. T020's full gate runs every golden test, so no subset is named here (analyze U4).
- [X] T004 In `ED/mod.rs`:
  - add `EditOp::Retier { from: String, to: SbomType }` (`generate::lifecycle_phases::SbomType`), with category `retier` (analyze I1);
  - add `OpReport { from: Option<String>, to: Option<String> }`, serialised only when present (R3);
  - add `waybill:retiered-from` to `PROTECTED_ANNOTATIONS` (R4).

  Unit test: an existing `OpReport` serialises byte-identically to before.

## Phase 3: User Story 1 — exercise a consumer with the same graph under another tier (P1) 🎯 MVP

**Goal**: FR-001, FR-003, FR-008.
**Independent test**: R1, R2, R3 and the parity check.

- [X] T005 [US1] Write R1, R2, R3 and R7 in `IT`, each editing all three formats of one scan and checking parity. They are red on this commit, since the flag does not parse. *As built: R1, R2, R3 and R7 were written together with the code rather than red first; T017's mutations show each one fails when its rule is removed.*
- [X] T006 [US1] In `CE`, add `--retier <FROM=TO>`, repeatable and sequence-stamped (`parse_retier`). Validation (R7) is written in Phase 6; here, parse the form only.
- [X] T007 [US1] Add `fn set_tier(&mut self, id: &str, to: &str) -> Option<String>` to `SbomAdapter` in `ED/mod.rs`. It returns the previous tier, and writes C243 with that tier when the component has no C243 yet. Implement it in `ED/cdx.rs` (the property), `ED/spdx23.rs` (the package annotation envelope) and `ED/spdx3.rs` (the `Annotation` element). Unit tests per adapter: *As built: the per-adapter unit test is one test over all three formats (`s1651_set_tier_keeps_the_first_marker_in_every_format`).*
  - only the tier and the marker change;
  - an existing marker keeps its value.
- [X] T008 [US1] In `ED/mod.rs::run`, apply `EditOp::Retier` with a run-local `relabelled` set (R2), filling `matched`, `changed`, `from` and `to`. R1, R2, R3 and R7 pass, with parity clean.

## Phase 4: User Story 2 — the relabel cannot be mistaken for a scan (P1)

**Goal**: FR-004, FR-005; C243 as a catalog row.
**Independent test**: R4, R5, R6, V1.

- [X] T009 [US2] Write R4, R5, R6 and V1 in `IT`. V1 covers `verify-chain`, with a key-signed and an unsigned original, as `sbom_edit_chain.rs` does.
- [X] T010 [P] [US2] Add C243 `waybill:retiered-from`:
  - extractors in `PX/cdx.rs`, `PX/spdx2.rs` and `PX/spdx3.rs`, with the same macros as C218;
  - a `ParityExtractor` entry in `PX/mod.rs`, `SymmetricEqual`, with its unit test;
  - the catalog row in `docs/reference/sbom-format-mapping.md`, with the standards-native audit (R4: KEEP-NO-NATIVE).

  The gate's catalog/extractor check must pass.
- [X] T011 [US2] Extend `waybill-cli/tests/sbom_edit_parity.rs` with a `--retier design=source` case on `IT`'s scan, asserting the C194 `operations` projection, including `from` and `to`, agrees across the three formats. R4, R5, R6 and V1 pass.

## Phase 5: User Story 3 — the document stays self-consistent (P2)

**Goal**: FR-006, FR-007.
**Independent test**: T1, T2, C1.

- [X] T012 [US3] Write T1, T2, C1 and D1 in `IT`. T2 scans the fixture with `--sbom-type build`. C1 uses a scan whose completeness reason names an ecosystem: the declared-only part should give one; if not, use a hand-made document and note it.
- [X] T013 [US3] In `ED/mod.rs`, after the operation loop, call a new adapter method, `recompute_document_type(original_tiers, new_tiers)`, implemented in all three adapters: *As built: implemented as `document_type_is` and `set_document_type` on each adapter; the SPDX 3 comment lives on `SpdxDocument`.*
  - if the document's current type equals the aggregate of `original_tiers` (`LP`'s `*_from_tiers`), replace it with that of `new_tiers` in CycloneDX `metadata.lifecycles`, the SPDX 2.3 `creationInfo.comment` phases clause, and SPDX 3 `software_sbomType` and its comment;
  - otherwise keep it, and set `EditReport.document_type_kept = true`, which the CLI prints.

  `original_tiers` are the input's, read before any operation runs; `new_tiers` are the output's, after every operation (analyze U1). Run it only when a `Retier` changed at least one component. Never call `downgrade_graph_completeness` for a retier (R6). T1, T2, C1 and D1 pass.

## Phase 6: User Story 4 — misuse is refused clearly (P2)

- [X] T014 [US4] Write the refusal cases in `IT`, from contracts/cli.md's table. Each asserts a non-zero exit, the message naming the problem, and that no output file exists.
- [X] T015 [US4] In `CE`, implement R7: *As built: `TO` is parsed by `SbomType::parse_str`, but the refusal message is `--retier`'s own and names the six values; `--sbom-type`'s message would name the wrong flag.*
  - `parse_retier` refuses a missing `=`, an empty side and `FROM == TO`, and parses `TO` with `SbomType::parse`, which refuses anything outside the CISA six with `--sbom-type`'s message;
  - a post-parse check refuses one `FROM` with two targets.

  The same mapping given twice is accepted. Unit tests in `CE`, including that `image=build` is accepted (analyze U3). The refusal cases pass.

## Phase 7: Measurement and mutations

- [X] T016 SC-001, with `MS/retier_consumer_model.py`. For npm-express, hpack-ogmios-no-generated-cabal and haskell-language-server: *As built: the filtered `hpack-ogmios-no-generated-cabal` golden has dangling `compositions[]` and `sbom edit` rightly refuses it, so ogmios was measured on a full scan of the same pinned tree. `MS/sc001.md`.*
  - copy the committed CycloneDX golden into the session scratchpad;
  - run the built `waybill sbom edit --retier design=source`;
  - apply the modelled consumer rule (R9).

  `required` lost must be 0, 0 and 0, against 27, 95 and 325 unedited. Also record the components and edges, which must be identical to the input apart from tier, C243, C194 and the type (SC-002), and that C194's counts equal the number of components carrying C243 (SC-003). Write it all to `MS/sc001.md`.
- [X] T017 Mutations (SC-006), with `MS/mutate.py` copied from spec 1650's runner: *As built: 7 mutations, all killed. `MS/mutations.md`.*
  - Ma: the run-local set removed (sequential relabelling), killed by R2;
  - Mb: the marker not written, killed by R1 and the unit test;
  - Mc: `changed` miscounted, killed by R1 and R3;
  - Md: the type never recomputed, killed by T1;
  - Me: the type always recomputed, killed by T2;
  - Mf: `downgrade_graph_completeness` called for a retier, killed by C1;
  - Mg: `TO` validation removed, killed by the refusal cases.

  Every mutation must be killed. Restore with `cp`, rebuild, and record in `MS/mutations.md`.

## Phase 8: Polish

- [X] T018 [P] Docs:
  - `docs/user-guide/sbom-edit.md`: `--retier`, its testing purpose, the at-most-once rule, the marker, the document-type rule (including the indistinguishable-assertion case), and that completeness is untouched (FR-007's note);
  - `docs/user-guide/cli-reference.md`: the flag;
  - `docs/reference/sbom-format-mapping.md`, C194's row: the `retier` category with its `from`/`to`;
  - `docs/reference/sbom-types.md`: a pointer to the edit.
- [X] T019 [P] `CHANGELOG.md`: an Added entry citing #1683 and kusaridev/iac#3552, with T016's numbers.
- [X] T020 Run the full gate on the final commit and record it in `MS/gate-log.md`: *As built: one run, pass; plus a corpus verification dispatch at the final commit, since the corpus goldens are ignored locally.*
  - the per-target totals, `GATE_RESULT` and the expected line;
  - that `git diff f0c6706 -- Cargo.lock '*Cargo.toml'` is empty;
  - that no corpus golden changed (`git diff --stat f0c6706 -- waybill-cli/tests/fixtures/public_corpus` is empty).
- [X] T021 Close-out: check every task's claim against the tree and `MS/`, note as-built differences inline, and tick each task.

## Dependencies

- **Setup:** T001 → T002 → T005, T009, T012 and T014.
- **Foundational:** T003 → T013. T004 → T007 and T008.
- **US1:** T005 → T006 → T007 → T008.
- **US2:** T009 after T008. T010 is independent of T009 (other files). T011 after T008 and T010.
- **US3:** T012 → T013, after T003 and T008.
- **US4:** T014 → T015, after T006.
- **Measurement:** T016 after T013 and T015. T017 after T015.
- **Polish:** T018 and T019 after T016. T020 and T021 last.

## Parallel opportunities

- T003 with T004 (different files).
- T010 with T009 and T011's preparation.
- T014 with T012.
- T018 with T019.

## Implementation strategy

- **MVP:** US1 and US2 together. A relabel without its marker and record is exactly the indistinguishable test artefact the spec exists to prevent.
- **US3 and US4** keep the document coherent and the flag safe. All four ship in one PR.
- **No corpus goldens change:** `sbom scan`'s output is untouched, which T003 and T020 verify.
