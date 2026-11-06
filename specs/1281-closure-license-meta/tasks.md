# Tasks: licenses for nix closure components

**Feature**: `1281-closure-license-meta` | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)
**Branch**: `1281-closure-license-meta`

Tests are generated here because the spec asks for them explicitly: SC-004
("asserted by a test"), SC-008 ("disabling the acquisition makes a test fail"),
and each user story carries an **Independent Test** clause.

**Stack**: Rust stable pinned 1.98.1, zero new Cargo dependencies, host `nix`
as an existing subprocess requirement of this tier.

---

## Phase 1: Setup

- [ ] T001 Read `waybill-cli/src/scan_fs/package_db/nix/eval/invoke.rs` lines 242–420 and record, in a comment at the head of the new module, which helpers are reused verbatim (`is_safe_attribute_name`, `is_valid_revision`, `resolving_argv`, `argv_is_safe`, `run_bounded`) per research R8 and FR-012
- [ ] T002 [P] Create `waybill-cli/src/scan_fs/package_db/nix/eval/licenses.rs` as an empty module with its doc comment stating the FR-002 store-path-join rule and the R5 reason for it
- [ ] T003 [P] Register the new module in `waybill-cli/src/scan_fs/package_db/nix/eval/mod.rs`
- [ ] T004 [P] Add the committed closure fixture for the license join under `waybill-cli/tests/fixtures/nix_closure_licenses/`, using synthetic package names (`waybill-fixture-*`) per the project's fixture-naming rule

**Checkpoint**: the module exists, compiles, and is wired.

---

## Phase 2: Foundational (blocking)

These block every user story: the row type, the expression, and the guards.
Nothing in Phase 3+ can be tested without them.

- [ ] T005 Define `LicenseRow { drv, out, attr, root, licenses }` in `waybill-cli/src/scan_fs/package_db/nix/eval/licenses.rs` per `contracts/license-lookup.md`, with `out` named `out` and a comment recording the R3 `outPath` string-coercion trap
- [ ] T006 Define `LicenseCoverage { wanted, received, source, degraded }` in `waybill-cli/src/scan_fs/package_db/nix/closure/summary.rs` per data-model.md
- [ ] T007 Write `licenses_expr(revision, system, roots, names) -> String` in `licenses.rs`, modelled on `versions_expr` but returning rows keyed by `baseNameOf drvPath`, with the three R2 guards in order: `builtins.hasAttr`, then `builtins.isAttrs p && p ? drvPath`, then `builtins.tryEval (builtins.deepSeq v v)`
- [ ] T008 [P] Unit-test in `licenses.rs` that `licenses_expr` interpolates the system literally and the revision only after `is_valid_revision`, mirroring the existing `expression_is_pure_pinned_and_literal_in_the_system` test
- [ ] T009 [P] Unit-test in `licenses.rs` that the generated argv contains neither `--impure` nor `--accept-flake-config` and passes `argv_is_safe` (FR-011, FR-012)
- [ ] T010 Add `evaluate_licenses` to the `Evaluator` trait in `waybill-cli/src/scan_fs/package_db/nix/eval/mod.rs` with the signature in `contracts/license-lookup.md`
- [ ] T011 Implement `evaluate_licenses` for `NixEvaluator` in `invoke.rs`, reusing the `evaluate_versions` scaffolding verbatim — name filter with refused-count warning, revision validation, `resolving_argv`, `argv_is_safe`, `run_bounded` — and adding no second set of controls (FR-012)
- [ ] T012 Implement `evaluate_licenses` for the test double alongside the existing one in `eval/mod.rs`, returning fixture rows so Phase 3+ is testable without a host `nix`
- [ ] T013 Parse the evaluation's JSON into `Vec<LicenseRow>` in `licenses.rs`, dropping malformed rows with a `tracing::warn!` rather than failing the pass (FR-013)
- [ ] T014 [P] Unit-test that a row whose attribute throws, a `null` alias, and a missing attribute each yield no row and do not abort the pass (FR-013, R2) in `licenses.rs`

**Checkpoint**: the evaluation can be invoked and its rows parsed, against the
double. No component is licensed yet.

---

## Phase 3: User Story 1 — a consumer can see what a closure component is licensed under (P1) 🎯 MVP

**Goal**: closure components carry the licenses the pinned revision declares.

**Independent test**: scan a fixture flake with `--nix-closure` against the
stub evaluator and assert closure components carry `licenses[]` entries.

- [ ] T015 [US1] Build the store-path join in `waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs`: index `LicenseRow`s by `drv` basename and match against the closure's derivation keys, never by name (FR-002)
- [ ] T016 [US1] Collect the name set offered to the evaluation from the closure's distinct `pname` values, and record the count as `LicenseCoverage.wanted`
- [ ] T017 [US1] Replace the hardcoded `licenses: vec![]` at `waybill-cli/src/scan_fs/package_db/nix/closure/emit.rs:180` with the joined licenses, leaving `concluded_licenses` empty — a nixpkgs `meta.license` is declared, not concluded (data-model.md)
- [ ] T018 [US1] Wire the lookup into the closure pass **after** `closure_identity::reclassify`, matching the milestone-1280 ordering, in `waybill-cli/src/cli/scan_cmd.rs`
- [ ] T019 [US1] Carry every declared license, in nixpkgs' stated order, deduplicated (FR-005)
- [ ] T020 [P] [US1] Integration test in `waybill-cli/tests/closure_licenses.rs`: a member declared MIT emits SPDX `MIT` (US1 scenario 1)
- [ ] T021 [P] [US1] Integration test: a member with several declared licenses emits all of them, not the first (US1 scenario 2, FR-005)
- [ ] T022 [P] [US1] Integration test: two scans of one closure emit byte-identical license sets (US1 scenario 3, FR-010, SC-004)
- [ ] T023 [P] [US1] Integration test: the join is by store path — a fixture with two derivations sharing a `pname` at different versions licenses each from its own store path, and swapping to a name join fails the test (FR-002, R5)
- [ ] T024 [US1] Reconcile with licenses an earlier reader already attached: do not overwrite, duplicate or contradict (FR-004), with a test over a member milestone 1273 already licenses
- [ ] T025 [US1] Handle the ambiguous case where one store path matches rows from more than one root — rows carried identical licenses in every measured case (R4), so assert that and degrade the component to no license rather than guessing if it ever does not

**Checkpoint**: US1 is independently deliverable. Components carry licenses.

---

## Phase 4: User Story 2 — a license stated in nixpkgs' own vocabulary is preserved, not faked (P2)

**Goal**: an attribute with no SPDX equivalent is preserved and marked, never
rewritten into a plausible-looking identifier.

**Independent test**: a member declared `publicDomain` carries the declaration
in a form marked uncanonicalised, and no component carries an SPDX identifier
the mapping table does not hold.

- [ ] T026 [US2] Reuse milestone 1273's `DeclaredLicense` two-state type at `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/package_set.rs:36` rather than defining a second one; move or re-export it if the module boundary requires
- [ ] T027 [US2] Use nixpkgs' own `spdxId` from the license attrset directly where it is present, rather than re-deriving it from the attribute name (FR-014) — the expression already returns it per research R1
- [ ] T028 [US2] Where `spdxId` is absent, preserve the attribute as stated and mark it uncanonicalised; never synthesise an SPDX identifier (FR-003)
- [ ] T029 [P] [US2] Integration test: a member declared `publicDomain` is preserved and is not presented as a canonical SPDX expression (US2 scenario 1)
- [ ] T030 [P] [US2] Integration test: a member declared `mit` carries SPDX `MIT` (US2 scenario 2)
- [ ] T031 [P] [US2] Test that no emitted component carries an SPDX identifier absent from the milestone-1273 mapping table (SC-003)
- [ ] T032 [US2] Handle the composite form `with lib.licenses; [ asl20 mit ]` as several entries rather than one string to split (spec Edge Cases)

**Checkpoint**: US2 independently testable on top of US1.

---

## Phase 5: User Story 3 — the scan still succeeds when licenses cannot be obtained (P3)

**Goal**: every acquisition failure degrades rather than failing the scan, and
says so in the document.

**Independent test**: scan with a failing evaluation; the document is still
emitted, carries its components, and records why.

- [ ] T033 [US3] Return `DegradationReason` from every failure path in `licenses.rs` and leave the document emittable (FR-006)
- [ ] T034 [US3] Record the reason in the document rather than only in logs, through the existing closure degradation channel in `waybill-cli/src/scan_fs/package_db/nix/closure/summary.rs` (FR-006, Constitution XII.3)
- [ ] T035 [US3] Emit the `wanted`/`received` coverage record at document scope (FR-008), so a closure whose members declare no licenses is distinguishable from an acquisition that silently returned nothing
- [ ] T036 [P] [US3] Integration test: a failing evaluation still emits a document with its full component set and a recorded reason (US3 scenario 1, SC-005)
- [ ] T037 [P] [US3] Integration test: a scan without `--nix-closure` is byte-identical to today (US3 scenario 2, FR-007)
- [ ] T038 [P] [US3] Integration test: an evaluation that succeeds and returns zero rows reports zero coverage explicitly, not silence (spec Edge Cases, FR-008)
- [ ] T039 [US3] Confirm a closure member nixpkgs does not describe at all — a bootstrap or fetch derivation — is distinguishable from one never looked up (spec Edge Cases)

**Checkpoint**: all three stories complete and independently testable.

---

## Phase 6: Polish & cross-cutting

- [ ] T040 Add the C-row for the coverage annotation to `docs/reference/sbom-format-mapping.md` **and** its matching entry in `waybill-cli/src/parity/extractors/mod.rs` EXTRACTORS in the same commit — a doc row without an extractor fails `every_catalog_row_has_an_extractor` and `holistic_parity`
- [ ] T041 [P] Write the parity extractor for the new row in `waybill-cli/src/parity/extractors/`, reading the emitted value rather than re-deriving it
- [ ] T042 **SC-008 mutation check**: disable the acquisition, confirm a test fails, re-enable, and record which test it was in the PR body. A license source no test is sensitive to is not evidence that it is read
- [ ] T043 [P] Verify SC-007: the C182 role census and component count are unchanged — this feature adds licenses, not components
- [ ] T044 [P] Verify SC-002: no component loses a license it carries today, by diffing the licensed set before and after
- [ ] T045 Measure SC-001 and SC-009 on the pinned target and record the figures per system. The 70%/85% figures in research R4 join by parsed name+version, **not** the store path FR-002 mandates — re-measure the real join yield and state it, rather than quoting R4
- [ ] T046 Run the pre-PR gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` — no `+toolchain` (#1189), and assert on the positive signal
- [ ] T047 Refresh corpus goldens via `public-corpus.yml` workflow dispatch, never locally, and verify SC-006: every target that does not scan a closure is byte-identical
- [ ] T048 Review the refreshed goldens with a normalised, sorted diff — mask content-addressed ids including `rel-`/`anno-` and `LC_ALL=C sort` before diffing, or SPDX-3 array reordering reads as a semantic hit

---

## Dependencies

```
Phase 1 (T001–T004)
      ↓
Phase 2 (T005–T014)  ← blocks everything
      ↓
Phase 3 US1 (T015–T025)  ← MVP
      ↓
Phase 4 US2 (T026–T032)   depends on US1's join and emission
      ↓
Phase 5 US3 (T033–T039)   depends on the acquisition existing
      ↓
Phase 6 (T040–T048)
```

US2 and US3 both build on US1's join; US1 alone is a shippable increment.
T040 must be one commit with T041. T047 precedes T048.

## Parallel opportunities

- **Phase 1**: T002, T003, T004 together.
- **Phase 2**: T008, T009 together once T007 lands; T014 with them.
- **Phase 3**: T020–T023 together once T015–T019 land.
- **Phase 4**: T029, T030, T031 together once T026–T028 land.
- **Phase 5**: T036, T037, T038 together once T033–T035 land.
- **Phase 6**: T041, T043, T044 together.

## Implementation strategy

**MVP = Phase 1 + Phase 2 + Phase 3 (US1)** — closure components carry
licenses joined by store path. That alone closes the measured gap; US2 governs
how honestly the values are stated and US3 governs what happens when the
acquisition cannot run.

Two cautions carried from the plan into execution:

1. **Coverage cannot reach 100%** (R5). A `drvPath` encodes the whole
   transitive input closure, so the same package built in a different context
   is a different store path. T035's coverage record is what makes an
   incomplete join legible; no success criterion should be written as if
   completeness were attainable.
2. **An empty license set and a lookup that never ran look identical.** T038
   and T042 exist because of that — assert on a positive signal, never on the
   absence of an error.
