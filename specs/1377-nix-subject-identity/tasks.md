# Tasks: the subject of a Nix scan is the package it built, identified by the CPE nixpkgs declares

**Input**: `specs/1377-nix-subject-identity/` (plan.md, spec.md, research.md, data-model.md, contracts/wire.md, quickstart.md)
**Tests**: included and written first. The spec defines testable acceptance scenarios, Principle VII requires serialization tests, and the project works TDD. Every new test is shown failing before its implementation task.
**Revision**: 2026-11-09, after `/speckit-analyze` (C1 option a, U1 → FR-013, U2, I1, U3, G1–G3, D1).

Paths are relative to the repository root. In-module unit tests use
`#[cfg(test)] #[cfg_attr(test, allow(clippy::unwrap_used))] mod tests`. Closure
integration tests use the stub-`nix` pattern of `waybill-cli/tests/closure_licenses.rs`
and never run a real evaluation.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

- [X] T001 Make the spike probes path-independent: read `TARGETS_DIR`, `OUT_DIR`, `WAYBILL`, `GRYPE`, `SBOMNIX` from the environment, failing loudly when one is unset, instead of the hard-coded `target/nix-vuln-spike/` paths. Files: `specs/1377-nix-subject-identity/measurements/gen-sboms.sh`, `scan.sh`, `analyze.py`, `adjudicate.py`, `precision.py` (research R9).
- [X] T002 Add an independent reach probe, `specs/1377-nix-subject-identity/measurements/meta-eval-cost/reach.py`, for SC-009. It takes a target's members from `nix derivation show -r` (pname and version from `env`/`structuredAttrs`, `.drv` basename) and the rows `probe.py`'s guarded expression returns for their names. It joins them by drv, then by pname + version, without using waybill, and prints `wanted`, `received`, `by-drv`, `by-name-version` and the unreached count.
- [X] T003 File the issue *"`ResolvedComponent.cpes` is `Vec<String>`; Principle IV requires a `Cpe` newtype"*, citing 108 references in 49 files, this milestone's single conversion point, and plan.md's Constitution Check. Use `gh issue create` on `kusari-oss/waybill`, with no @-mentions and ending with the AI-drafting line. Record the number in `specs/1377-nix-subject-identity/plan.md` (analysis C1, option a).
- [X] T004 Record a control baseline on the base commit `49a24a63`, before any code change. Run the probes per `specs/1377-nix-subject-identity/quickstart.md` over all six targets and save the per-target, per-scanner finding sets, the subjects and the C184 JSON as a summary in `specs/1377-nix-subject-identity/measurements/baseline-49a24a63/`. Raw scanner JSON stays outside the repo.

---

## Phase 2: Foundational (blocks every story)

- [X] T005 Extend the stub-`nix` fixture support used by `waybill-cli/tests/closure_licenses.rs`:
  - a test can supply meta-evaluation rows carrying `version` and `cpe` (absent, null and present);
  - a test can supply a derivation-show closure whose `graph.root` is a chosen derivation.
  Put the shared helper where the closure tests already share fixture code, and add fixtures under `waybill-cli/tests/fixtures/` with `waybill-fixture-*` names only.

**Checkpoint**: an existing closure test still passes against the extended stub (no behaviour change).

---

## Phase 3: User Story 1 — the built package is the subject (P1) 🎯 MVP

**Goal**: when a closure resolves, `graph.root`'s component is the subject in all three formats (FR-001 to FR-004, FR-012).
**Independent test**: SC-001 on the openssh wrapper flake; SC-008's subject identity on the override-free moat probe scan. SC-002 also needs US2, because openssh has no curated CPE.

### Tests (write first, see them fail)

- [X] T006 [P] [US1] Unit tests in `waybill-cli/src/generate/root_selector.rs`:
  - `NixClosure` with exactly one evaluated-package-tagged component returns `MainModule(idx)` with `NixEvaluatedPackage`, and `losers` holds every other main-module;
  - an operator override still wins;
  - no tagged component falls through to today's ladder, byte-identical;
  - the heuristic serialises as `"nix-evaluated-package"` with confidence 0.99;
  - the tag key is listed by `is_internal_emission_key`.
- [X] T007 [P] [US1] Integration test `waybill-cli/tests/closure_subject.rs`, stub `nix`:
  - the subject is the root derivation's package in CycloneDX `metadata.component`, SPDX 2.3 `documentDescribes` and SPDX 3 `rootElement`;
  - a root with no CPE gives no `cpe` on CycloneDX `metadata.component` and no synthesised `waybill:` CPE in either SPDX format (FR-012);
  - a degraded closure leaves today's subject;
  - a root whose `(name, version)` equals another reader's component (the moat shape) produces one component, and that component is the subject, so the tag survived `merge_closure_components`;
  - for a NixOS-configuration attribute, the subject is the system's top-level derivation;
  - the internal tag appears in no output.

### Implementation

- [X] T008 [US1] In `waybill-cli/src/generate/root_selector.rs`:
  - add `ScanTargetKind::NixClosure` and `RootSelectionHeuristic::NixEvaluatedPackage` (`name()` → `"nix-evaluated-package"`, `confidence()` → 0.99);
  - add the internal evaluated-package tag key and register it in `is_internal_emission_key`;
  - add the rung in `select_root`, after the operator override and before the main-module rungs (research R3).
- [X] T009 [US1] In `waybill-cli/src/scan_fs/package_db/nix/closure/emit.rs`, always emit the `ClassifiedClosure.graph.root` derivation as a component and set the evaluated-package tag on it. Do not add duplicate-avoidance here: `merge_closure_components` in `waybill-cli/src/cli/scan_cmd.rs` already merges on `(name, version)` and folds annotations with `or_insert`, which carries the tag to an existing component (research R2).
- [X] T010 [US1] In `waybill-cli/src/cli/scan_cmd.rs`:
  - set `scan_target_kind = ScanTargetKind::NixClosure` once a closure resolves. The local is computed at ~3598, before resolution at ~4503, so it has to be updated after resolution;
  - make sure every later consumer sees the new value: the ~4989 seeds call, `classify_orphans_pre_emit` and `ScanArtifacts`;
  - make sure the early ~3865 selection's FR-007 warning does not report losers for a selection the final rung overrides.
- [X] T011 [US1] In `waybill-cli/src/generate/cyclonedx/metadata.rs` (~1275–1301), under `NixEvaluatedPackage` emit no `cpe` on `metadata.component` when the subject's `cpes` is empty, rather than the `waybill:` placeholder (FR-012, research R7). Confirm that `waybill-cli/src/generate/cyclonedx/builder.rs:841` and `:1313` still treat the subject as a component under `NixClosure`.
- [X] T012 [P] [US1] In `docs/reference/sbom-format-mapping.md`, add `nix-evaluated-package` (0.99) to row C69's list of values.

**Checkpoint**: T006 and T007 pass. The openssh subject is `openssh 10.3p1` in all three formats (SC-001). moat's subject is still `pkg:hackage/moat@0.1`.

**Found during US1, fixed inside it** (each one makes the US1 output wrong without the fix):
- `resolve/closure_role_scope.rs`: the evaluated package classifies `unreferenced` (nothing consumes it), so the role-scope pass marked it `scope: excluded`. That was already true for openssh 10.3p1 at `49a24a63`. The pass now skips the tagged component; unit test `m1377_the_evaluated_package_is_never_classified_not_in_the_artifact`.
- `generate/cyclonedx/builder.rs`: `target_ref` is the *first* `main-module` component, not the selected subject. Under the new rung, openssh's `dependencies[]` kept a `filesystem-scan@0.0.0` entry that resolved to nothing. `target_ref` now follows the selection, under `NixEvaluatedPackage` only. SPDX 2.3 and SPDX 3 already root at the selection.
- `cli/scan_cmd.rs`: the FR-007 warning moved into `warn_if_root_heuristic_outranked`. It runs early as before, unless a closure may resolve; then it runs after the closure, with the final kind.
- `closure/graph.rs`: `root` is the *first* unconsumed derivation in `.drv`-key order. A closure with several unconsumed derivations does not say which one was evaluated, and a fixture without `inputs.drvs` made `waybill-fixture-aeson` the "subject". The new `ProjectedGraph.evaluated` is `root` only when it is the sole candidate. The tag uses `evaluated`; edge anchoring keeps `root`. Real closures have one (all six spike targets name their package).
- `generate/cyclonedx/builder.rs::build_components`: a lone main-module was promoted to `metadata.component`, and skipped in `components[]`, on count alone. When the evaluated package outranks it, the module dropped out of the document, and closure edges named it as an absent component (same class as #1170/#1186). Only the actual subject is promoted now. `root_selector::evaluated_nix_package` is the one definition of the rung, shared by the selector and this site.
- Coverage counts the evaluated package (clarification 2026-11-09): `emit::coverage_drv_keys` feeds C184's `wanted`/`received`, and `debug closure-dump` reports it per row (`in_coverage_population`, `evaluated_package`), which `reach.py` now reads. Test: `the_evaluated_package_is_counted_in_metadata_coverage`.
- `tests/closure_edges.rs::m1280_t019` now uses `anchor-two-roots.json`. A sole root is always emitted now, so "a root no reader emits" exists only as the first of several candidates.

---

## Phase 4: User Story 2 — closure components carry the CPE nixpkgs declares (P1)

**Goal**: declared CPEs, joined by drv then by pname + version, win over the curated table, which stays the fallback, including on merged members (FR-005 to FR-007, FR-009, FR-010, FR-013).
**Independent test**: SC-003, SC-004 and SC-009 on the openssh wrapper flake. With US1, SC-002.

### Tests (write first, see them fail)

- [X] T013 [P] [US2] Unit tests in a new `waybill-cli/src/scan_fs/package_db/nix/closure/identifiers.rs`:
  - `DeclaredCpe` accepts a well-formed CPE 2.3 string and rejects a wrong field count, a bad `part`, an empty or `*` vendor, and an empty or `*` product;
  - the join prefers `drv` over `name-version`;
  - several `name-version` rows with different CPEs leave the member without one, counted as `rejected`;
  - the `IdentifierCoverage` invariants hold (`by-drv + by-name-version = received ≤ wanted`; `declared + rejected ≤ received`);
  - the closure-tier functions take and return `DeclaredCpe`, never `String`.
- [X] T014 [P] [US2] Unit tests in `waybill-cli/src/scan_fs/package_db/nix/eval/licenses.rs`:
  - `licenses_expr` emits `version` and a `cpe` guarded by its own `builtins.tryEval`;
  - a row deserialises with the new fields absent, null and present;
  - the existing license tests pass unchanged.
- [X] T015 [P] [US2] Integration test `waybill-cli/tests/closure_declared_cpe.rs`, stub `nix`:
  - a declared CPE replaces the curated one on a member the table also maps. Name a fixture member so the table matches, as `waybill-fixture-mapped`/`zlib` does in `waybill-cli/tests/closure_component_cpes.rs`;
  - a member whose drv differs but whose pname + version match gets the declared CPE (the bash shape);
  - no declared CPE leaves the curated CPE (FR-006);
  - a row with product-only `cpeParts` and no `cpe` synthesises nothing;
  - an invalid declared CPE is not emitted and is counted;
  - a declared CPE whose version field differs from the derivation's version is emitted exactly as declared;
  - no component that had a CPE loses it;
  - on merged members (FR-013), an existing curated CPE is replaced; an existing operator-supplement CPE is kept with no nixpkgs provenance; an existing empty slot is filled;
  - the C184 JSON carries `identifiers` with the expected counts;
  - the stub sees exactly one meta-evaluation call (FR-010).

### Implementation

- [X] T016 [US2] In `waybill-cli/src/scan_fs/package_db/nix/eval/licenses.rs`, add `version = p.version or null;` and the guarded `cpe` to `licenses_expr`'s row, and add `version` and `cpe` fields to the row type (research R1, `measurements/meta-eval-cost/`).
- [X] T017 [US2] Create `waybill-cli/src/scan_fs/package_db/nix/closure/identifiers.rs`:
  - `DeclaredCpe` (validated; the type at every closure-tier boundary, Principle IV);
  - the drv-then-name-version join over the meta rows;
  - `IdentifierCoverage`.
  Register the module in `waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs`.
- [X] T018 [US2] In `waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs`, restructure `attach_licenses` (~480–547) so the single existing meta evaluation feeds both the license index and the identifier join, and store the declared CPEs and coverage on `ClassifiedClosure`. On a failed evaluation, degrade exactly as licenses do: no declared CPEs, the curated fallback, `received: 0`.
- [X] T019 [US2] In `waybill-cli/src/scan_fs/package_db/nix/closure/emit.rs`:
  - set a member's `cpes = [declared]` when it has a `DeclaredCpe`. This is the single `DeclaredCpe` → `String` conversion (analysis C1, option a);
  - compute what `waybill-cli/src/generate/cpe.rs::synthesize_cpes` would have produced, and keep it as the overridden value when it differs;
  - leave members without a declared CPE untouched, so that `fill_missing_cpes` (`waybill-cli/src/cli/scan_cmd.rs` ~4907, ~6701) applies the curated table as it does today.
- [X] T020 [US2] Apply FR-013 in `waybill-cli/src/cli/scan_cmd.rs::merge_closure_components`:
  - when a closure member carrying a declared CPE merges into an existing component, replace that component's `cpes` only when they are empty or equal `synthesize_cpes(existing)`;
  - otherwise keep them, and do not fold in the closure's CPE provenance;
  - first, establish how `--supplement-cdx`-asserted CPEs are marked (`waybill-cli/src/supplement/`), so that an asserted value equal to the curated one is still kept (research R4).
- [X] T021 [US2] In `waybill-cli/src/scan_fs/package_db/nix/closure/summary.rs`, add `identifiers {wanted, received, by-drv, by-name-version, declared, rejected}` to the C184 `waybill:nix-closure` JSON (~169–210), beside `licenses`.
- [X] T022 [P] [US2] In `docs/reference/sbom-format-mapping.md`, document C184's `identifiers` object and its Principle V audit (KEEP-NO-NATIVE, research R6).

**Checkpoint**: T013 to T015 pass. On the openssh wrapper flake, all 16 distinct name+version pairs with a declared CPE (17 derivations) carry it: bash `gnu:bash:5.3:9`, gnugrep `gnu:grep` (SC-003). openssh's subject carries `openbsd:openssh:10.3p1`.

**Checkpoint result (2026-11-09, US2 build, grype DB built 2026-11-08):**
- SC-003: all 17 nixpkgs-declared CPEs in sbomnix's `--exclude-cpe-matching` openssh document (16 members plus the subject) are carried exactly. That includes bash `gnu:bash:5.3:9`, gnugrep `gnu:grep` and the subject `openbsd:openssh:10.3p1`.
- SC-002: grype finds 28 on `openssh 10.3p1`, the same CVE set as sbomnix's 28 (0 differences). Baseline: 0.
- SC-004: the emitted bash encoding (version `5.3`, update `9`) is the nixpkgs form. Re-run on the same DB, that form at 5.0 patch 11 matches CVE-2019-18276; the old curated form matches nothing.
- SC-006: no finding lost on any target or scanner. Gained: openssh +28 (all on openssh 10.3p1), httpie +1 grype (p11-kit, via its declared CPE) and +1 osv (PYSEC-2023-242 on httpie 3.2.4, newly present as the subject). Patched false positives are still 6 per target.
- SC-009: C184 `identifiers` equals `reach.py` on every field. openssh: 670 wanted, 189 received (80 by drv, 109 by name+version), 39 declared, 0 rejected. gh: 722, 200 (91 + 109), 42, 0.
- Probe correction: reach.py first matched a `.drv` only among rows with the member's own name. Production indexes rows by `.drv` globally (`pkg-config-wrapper` and `clang-wrapper` are reached through differently-named attributes), as 1281's license index does. The narrow join had also made the license control look like a 74/76 bracket. It is an exact 75. The README records both.

---

## Phase 5: User Story 3 — a reader can tell where a CPE came from (P2)

**Goal**: each closure component's CPE source is recorded natively in all three formats. The join basis and any overridden CPE go in C212. Provenance always describes the CPE actually emitted (FR-008, FR-013).
**Independent test**: SC-005, plus US3 acceptance scenario 3 on bash.

### Tests (write first, see them fail)

- [X] T023 [P] [US3] Unit tests in `waybill-cli/src/generate/cyclonedx/evidence.rs`, values per `contracts/wire.md` §3:
  - a closure component with a declared CPE gets an `evidence.identity[]` entry with `field: "cpe"`, `concludedValue`, technique `manifest-analysis`, value `nixpkgs meta.identifiers.cpe`, confidence 0.9;
  - a curated one gets technique `other`, value `waybill curated CPE table`, confidence 0.7;
  - non-closure components are unchanged.
- [X] T024 [P] [US3] Unit tests in `waybill-cli/src/generate/spdx/packages.rs` and `waybill-cli/src/generate/spdx/v3_external_ids.rs`:
  - the CPE `externalRefs` entry and the `cpe23` `ExternalIdentifier` carry exactly one of the two comment strings in `contracts/wire.md` §3;
  - non-closure CPE refs keep `comment: None`.
- [X] T025 [P] [US3] Parity test for row C212 `waybill:cpe-provenance` (`SymmetricEqual`, `order_sensitive: false`), placed beside the existing per-row parity tests under `waybill-cli/src/parity/`. Cover `{basis, overrode}` and the case where `overrode` is omitted.

### Implementation

- [X] T026 [US3] Carry each closure component's `CpeProvenance` (data-model.md) from `waybill-cli/src/scan_fs/package_db/nix/closure/emit.rs` to the emitters, as follows:
  - put `waybill:cpe-provenance` in the component's existing annotation channel;
  - give the emitters a value they can read for the native carriers;
  - make the provenance survive the m1366 identity pass;
  - drop the provenance wherever T020 kept another reader's CPE.
- [X] T027 [US3] Emit the native CPE provenance in three places:
  - the `evidence.identity[]` CPE entry, in `waybill-cli/src/generate/cyclonedx/evidence.rs`;
  - the CPE `externalRefs[].comment`, in `waybill-cli/src/generate/spdx/packages.rs`;
  - the `cpe23` `ExternalIdentifier.comment`, in `waybill-cli/src/generate/spdx/v3_external_ids.rs`.
  Define the two comment strings once, the way `SOURCE_PACKAGE_COMMENT` is defined in `waybill-cli/src/os_source_package.rs`.
- [X] T028 [US3] Emit C212 `waybill:cpe-provenance` on nixpkgs-declared CPEs only:
  - in `waybill-cli/src/generate/cyclonedx/builder.rs`, as a `properties[]` entry;
  - in `waybill-cli/src/generate/spdx/annotations.rs`, as a `waybill-annotation/v1` envelope;
  - in `waybill-cli/src/generate/spdx/v3_annotations.rs`, as `Annotation.statement`.
- [X] T029 [US3] Add the C212 parity extractors in `waybill-cli/src/parity/extractors/cdx.rs`, `spdx2.rs` and `spdx3.rs`, and register the row in `waybill-cli/src/parity/extractors/mod.rs`. In the same task, add the C212 row to `docs/reference/sbom-format-mapping.md` with its justification clause: no format has a field for an identifier's join basis, or for an identifier that was overridden. The doc-row coverage test lives in the lib target, so the row and the extractor must land together.

**Checkpoint**: T023 to T025 pass. Every CPE-bearing closure component in the openssh document records its source natively in all three formats (SC-005). bash's provenance records `overrode: "cpe:2.3:a:gnu:bash:5.3p9:…"`.

**Checkpoint result (2026-11-09, US3 build):**
- SC-005: every CPE-bearing closure component records its source natively in all three formats. openssh: 72 of 72 in CycloneDX `evidence.identity[]`, 72 of 72 SPDX 2.3 `cpe23Type` refs carry the comment, and 99 of 99 SPDX 3 `cpe23` identifiers. httpie: 263 of 263, which includes members the m1366 pass re-identified as `pkg:pypi`, so the provenance survives that pass.
- bash: `waybill:cpe-provenance` = `{"basis":"name-version","overrode":"cpe:2.3:a:gnu:bash:5.3p9:*:*:*:*:*:*:*"}`. The only CPE identifier is `gnu:bash:5.3:9`.
- openssh has 18 nixpkgs-declared CPEs where sbomnix's declared-only document has 17. The extra one is `curl 8.20.0` (`haxx:curl`), reached by the name+version join, which sbomnix evidently does not make.
- How it was built: `overrode` is recomputed after the m1366 identity pass and `fill_missing_cpes` (`scan_cmd::record_closure_cpe_sources`), so it names what the curated table gives the component's *final* PURL. A curated CPE is marked only on closure components whose CPEs equal `synthesize_cpes`, and never on operator-supplied ones. The internal key `waybill:nix-curated-cpe` is never emitted. The C212 key is emitted through the generic annotation channel.
- T025 uses `waybill sbom parity-check`. It counts distinct values (3, for 4 components), so the integration test also compares the sorted C212 values across the three formats directly.

---

## Phase 6: Polish and cross-cutting

- [X] T030 [P] Document the subject rule, declared-CPE precedence (including FR-013), the join bases and the C184 `identifiers` counts for operators in `docs/reference/nix-evaluation.md`, in the `--nix-closure` section.
- [X] T031 [P] Add an `[Unreleased]` entry to `CHANGELOG.md` covering #1398 and #1394.
- [X] T032 Update the closure tests that pin a whole document (`waybill-cli/tests/closure_*.rs`, `waybill-cli/tests/nix_config_closure.rs`) for the new subject and provenance. Change only what this milestone changes.
- [X] T033 Regenerate every golden. Verify with a normalised, sorted diff (content-addressed IDs masked, `LC_ALL=C sort`) that only `waybill-cli/tests/fixtures/public_corpus/nix-closure-moat/` and closure-test fixtures change (SC-007). Verify that the moat diff is exactly the following, and nothing else (SC-008):
  - the subject stays `nix-closure-moat` (the harness passes `--root-name`, which wins; no heuristic is recorded);
  - CPEs and their provenance on closure members;
  - C184 `identifiers`, and `licenses.wanted` if the root was not already counted.
  Any other change is investigated, not accepted. The moat-shaped subject itself (`pkg:hackage/moat@0.1`, `nix-evaluated-package`) is checked by `closure_subject.rs` and by the override-free probe scan in T035.
  Corpus goldens are `#[ignore]`d locally, so run that target's harness explicitly.

  **Done on Linux CI**, because the local moat closure differs (aarch64-darwin: 1,275 derivations; the golden's Linux closure: 1,749). The first regen run (42748371200) failed I10 on `pkg:hackage/moat@0.1`, which was the defect fixed in `8e3ffa03` (C203 `exempt-evaluated-package`). The second (42751378299, head `8e3ffa03`) passed all 17 targets. Set-based diff of the moat CDX golden: subject unchanged (`nix-closure-moat`); 14 CPEs changed (12 gained, bash re-encoded, libidn2 `libidn2_project`→`gnu`); 5 `cpe-candidates` lists dropped; 63 `evidence.identity` cpe entries and 30 C212 records added; `moat` gains C182 `unreferenced` and C203 `exempt-evaluated-package`; C184 gains `identifiers`, with `licenses.wanted` 1236→1237; C209 moves 12 components from `unaddressable` to `with_candidates`. Nothing else. `image-postgres16` also differs in the artifact, from upstream image drift plus a C209 record that predates this branch. main's nightly fails on it too (run 42574068957), so it is not included here.
- [X] T034 Run the full gate, `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, and read its positive signals: the pass line, per-target `ok. N passed; 0 failed`, and the new tests by name.
- [X] T035 Re-run the probes per `quickstart.md` against the T004 baseline. Record SC-001 to SC-009 with numbers in `specs/1377-nix-subject-identity/measurements/results.md`:
  - SC-002: at least 28 findings on openssh 10.3p1;
  - SC-004: run the emitted bash CPE through the `measurements/bash-cpe-encoding/` control, expecting CVE-2019-18276;
  - SC-006: no finding lost except by a CPE-source change, and distinct patched false positives ≤ 6;
  - SC-009: C184's unreached count equals `measurements/meta-eval-cost/reach.py`'s.

---

## Dependencies

- Setup (T001–T004) comes before Foundational (T005), which comes before the stories. T002 needs T001's environment conventions.
- Some tasks edit the same file, so they run in order:
  - US1 and US2 both edit `closure/emit.rs` (T009, then T019).
  - US1 and US2 both edit `cli/scan_cmd.rs` (T010, then T020).
  - The tests (T006/T007 and T013–T015) can be written in parallel.
- US3 depends on US2: provenance needs a source, and the merge rule decides which provenance survives. T026 reads what T019 and T020 set.
- SC-002 needs both US1 and US2. SC-001 and the subject part of SC-008 need only US1.
- Polish (T030–T035) comes after all stories, with T033 → T034 → T035 in order.

## Parallel opportunities

- US1: T006 ∥ T007 ∥ T012.
- US2: T013 ∥ T014 ∥ T015, then T022 alongside the implementation.
- US3: T023 ∥ T024 ∥ T025.
- Polish: T030 ∥ T031.

## Implementation strategy

1. **MVP = Setup + Foundational + US1** (T001–T012). The subject becomes the package (SC-001; moat unchanged). Stop and review.
2. **US2.** Declared CPEs and the merge rule. SC-002 becomes judgeable (openssh's 28 findings).
3. **US3.** Provenance (SC-005).
4. **Polish**, then the scanner-outcome re-run (T035) as the acceptance evidence. The goal is measured scanner outcome, not passing tests.
