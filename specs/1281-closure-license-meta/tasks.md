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

- [X] T001 Read `waybill-cli/src/scan_fs/package_db/nix/eval/invoke.rs` lines 242–420 and record, in a comment at the head of the new module, which helpers are reused verbatim (`is_safe_attribute_name`, `is_valid_revision`, `resolving_argv`, `argv_is_safe`, `run_bounded`) per research R8 and FR-012
- [X] T002 [P] Create `waybill-cli/src/scan_fs/package_db/nix/eval/licenses.rs` as an empty module with its doc comment stating the FR-002 store-path-join rule and the R5 reason for it
- [X] T003 [P] Register the new module in `waybill-cli/src/scan_fs/package_db/nix/eval/mod.rs`
- [X] T004 [P] Add the committed closure fixture for the license join under `waybill-cli/tests/fixtures/nix_closure_licenses/`, using synthetic package names (`waybill-fixture-*`) per the project's fixture-naming rule

**Checkpoint**: the module exists, compiles, and is wired.

---

## Phase 2: Foundational (blocking)

These block every user story: the row type, the expression, and the guards.
Nothing in Phase 3+ can be tested without them.

- [X] T005 Define `LicenseRow { drv, out, attr, root, licenses }` in `waybill-cli/src/scan_fs/package_db/nix/eval/licenses.rs` per `contracts/license-lookup.md`, with `out` named `out` and a comment recording the R3 `outPath` string-coercion trap
- [X] T006 Define `LicenseCoverage { wanted, received, source, degraded }` in `waybill-cli/src/scan_fs/package_db/nix/closure/summary.rs` per data-model.md
- [X] T007 Write `licenses_expr(revision, system, roots, names) -> String` in `licenses.rs`, modelled on `versions_expr` but returning rows keyed by `baseNameOf drvPath`, with the three R2 guards in order: `builtins.hasAttr`, then `builtins.isAttrs p && p ? drvPath`, then `builtins.tryEval (builtins.deepSeq v v)`
- [X] T008 [P] Unit-test in `licenses.rs` that `licenses_expr` interpolates the system literally and the revision only after `is_valid_revision`, mirroring the existing `expression_is_pure_pinned_and_literal_in_the_system` test
- [X] T009 [P] Unit-test in `licenses.rs` that the generated argv contains neither `--impure` nor `--accept-flake-config` and passes `argv_is_safe` (FR-011, FR-012)
- [X] T010 Add `evaluate_licenses` to the `Evaluator` trait in `waybill-cli/src/scan_fs/package_db/nix/eval/mod.rs` with the signature in `contracts/license-lookup.md`
- [X] T011 Implement `evaluate_licenses` for `NixEvaluator` in `invoke.rs`, reusing the `evaluate_versions` scaffolding verbatim — name filter with refused-count warning, revision validation, `resolving_argv`, `argv_is_safe`, `run_bounded` — and adding no second set of controls (FR-012). The import-from-derivation refusal is **already verified once per tier run** at `eval/mod.rs:86`; this path inherits it and MUST NOT re-issue its own
- [X] T012 Implement `evaluate_licenses` for the test double alongside the existing one in `eval/mod.rs`, returning fixture rows so Phase 3+ is testable without a host `nix`
- [X] T013 Parse the evaluation's JSON into `Vec<LicenseRow>` in `licenses.rs`, dropping malformed rows with a `tracing::warn!` rather than failing the pass (FR-013)
- [X] T014 [P] Unit-test that a row whose attribute throws, a `null` alias, and a missing attribute each yield no row and do not abort the pass (FR-013, R2) in `licenses.rs`

**Checkpoint**: the evaluation can be invoked and its rows parsed, against the
double. No component is licensed yet.

---

## Phase 3: User Story 1 — a consumer can see what a closure component is licensed under (P1) 🎯 MVP

**Goal**: closure components carry the licenses the pinned revision declares.

**Independent test**: scan a fixture flake with `--nix-closure` against the
stub evaluator and assert closure components carry `licenses[]` entries.

- [X] T015 [US1] Build the store-path join in `waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs`: index `LicenseRow`s by `drv` basename and match against the closure's derivation keys, never by name (FR-002)
- [X] T016 [US1] Collect the name set offered to the evaluation from the closure's distinct `pname` values, and record the count as `LicenseCoverage.wanted`
- [X] T017 [US1] Replace the hardcoded `licenses: vec![]` at `waybill-cli/src/scan_fs/package_db/nix/closure/emit.rs:180` with the joined licenses, leaving `concluded_licenses` empty — a nixpkgs `meta.license` is declared, not concluded (data-model.md)
- [X] T018 [US1] Wire the lookup into the closure pass **after** `closure_identity::reclassify`, matching the milestone-1280 ordering, in `waybill-cli/src/cli/scan_cmd.rs`
- [X] T019 [US1] Carry every declared license, in nixpkgs' stated order, deduplicated (FR-005)
- [X] T020 [P] [US1] Integration test in `waybill-cli/tests/closure_licenses.rs`: a member declared MIT emits SPDX `MIT` (US1 scenario 1)
- [X] T021 [P] [US1] Integration test: a member with several declared licenses emits all of them, not the first (US1 scenario 2, FR-005)
- [X] T022 [P] [US1] Integration test: two scans of one closure emit byte-identical license sets (US1 scenario 3, FR-010, SC-004)
- [X] T023 [P] [US1] Integration test: the join is by store path — a fixture with two derivations sharing a `pname` at different versions licenses each from its own store path, and swapping to a name join fails the test (FR-002, R5)
- [X] T024 [US1] Reconcile with licenses an earlier reader already attached: do not overwrite, duplicate or contradict (FR-004), with a test over a member milestone 1273 already licenses
- [X] T025 [US1] Implement FR-015 in `closure/mod.rs`: where one store path matches rows from more than one root, require the licenses to agree, and emit none for that component if they do not. Rows carried identical licenses in every measured case (R4), so assert agreement rather than assuming it

- [X] T049 [P] [US1] Integration test in `waybill-cli/tests/closure_licenses.rs`: a licensed closure component carries its license in the **native** construct of all three formats — CycloneDX `licenses[]`, SPDX 2.3 and SPDX 3 — and in no `waybill:` property (FR-009, Constitution Principle V)

**Checkpoint**: US1 is independently deliverable. Components carry licenses.

---

## Phase 4: User Story 2 — a license stated in nixpkgs' own vocabulary is preserved, not faked (P2)

**Goal**: an attribute with no SPDX equivalent is preserved and marked, never
rewritten into a plausible-looking identifier.

**Independent test**: a member declared `publicDomain` carries the declaration
in a form marked uncanonicalised, and no component carries an SPDX identifier
the mapping table does not hold.

- [X] T026 [US2] Reuse milestone 1273's `DeclaredLicense` two-state type at `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/package_set.rs:36` rather than defining a second one. **Re-export it; do not move it** — relocating a shipped type to serve a new caller churns m1273's code for no behavioural gain
- [X] T027 [US2] Use nixpkgs' own `spdxId` from the license attrset directly where it is present, rather than re-deriving it from the attribute name (FR-014) — the expression already returns it per research R1
- [X] T028 [US2] Where `spdxId` is absent, preserve the attribute as stated and mark it uncanonicalised; never synthesise an SPDX identifier (FR-003)
- [X] T029 [P] [US2] Integration test: a member declared `publicDomain` is preserved and is not presented as a canonical SPDX expression (US2 scenario 1)
- [X] T030 [P] [US2] Integration test: a member declared `mit` carries SPDX `MIT` (US2 scenario 2)
- [X] T031 [P] [US2] Test that every emitted SPDX identifier is traceable to nixpkgs' own `spdxId` or the milestone-1273 mapping table — an identifier nixpkgs states that the table does not hold MUST pass, since FR-014 makes nixpkgs the authority (SC-003)
- [X] T032 [US2] Handle the composite form `with lib.licenses; [ asl20 mit ]` as several entries rather than one string to split (spec Edge Cases)

**Checkpoint**: US2 independently testable on top of US1.

---

## Phase 5: User Story 3 — the scan still succeeds when licenses cannot be obtained (P3)

**Goal**: every acquisition failure degrades rather than failing the scan, and
says so in the document.

**Independent test**: scan with a failing evaluation; the document is still
emitted, carries its components, and records why.

- [X] T033 [US3] Return `DegradationReason` from every failure path in `licenses.rs` and leave the document emittable (FR-006)
- [X] T034 [US3] Record the reason in the document rather than only in logs, through the existing closure degradation channel in `waybill-cli/src/scan_fs/package_db/nix/closure/summary.rs` (FR-006, Constitution XII.3)
- [X] T035 [US3] Emit the `wanted`/`received` coverage record at document scope (FR-008), so a closure whose members declare no licenses is distinguishable from an acquisition that silently returned nothing
- [X] T036 [P] [US3] Integration test: a failing evaluation still emits a document with its full component set and a recorded reason (US3 scenario 1, SC-005)
- [X] T037 [P] [US3] Integration test: a scan without `--nix-closure` is byte-identical to today (US3 scenario 2, FR-007)
- [X] T038 [P] [US3] Integration test: an evaluation that succeeds and returns zero rows reports zero coverage explicitly, not silence (spec Edge Cases, FR-008)
- [X] T039 [US3] Confirm a closure member nixpkgs does not describe at all — a bootstrap or fetch derivation — is distinguishable from one never looked up (spec Edge Cases)

- [X] T050 [US3] Cover the system-configuration closure shape (milestone 1066), where the nixpkgs input may not be reachable as it is for a package flake: confirm the lookup degrades per FR-006 rather than failing, and record the outcome in `specs/1281-closure-license-meta/measurements/` (spec Edge Cases)

**Checkpoint**: all three stories complete and independently testable.

---

## Phase 6: Polish & cross-cutting

- [X] T040 Add the C-row for the coverage annotation to `docs/reference/sbom-format-mapping.md` **and** its matching entry in `waybill-cli/src/parity/extractors/mod.rs` EXTRACTORS in the same commit — a doc row without an extractor fails `every_catalog_row_has_an_extractor` and `holistic_parity`
- [X] T041 [P] Write the parity extractor for the new row in `waybill-cli/src/parity/extractors/`, reading the emitted value rather than re-deriving it
- [X] T042 **SC-008 mutation check**: disable the acquisition, confirm a test fails, re-enable, and record which test it was in the PR body. A license source no test is sensitive to is not evidence that it is read
- [X] T043 [P] Verify SC-007: the C182 role census and component count are unchanged — this feature adds licenses, not components
- [X] T044 [P] Verify SC-002: no component loses a license it carries today, by diffing the licensed set before and after
- [X] T045 Measure SC-001 and SC-009 on the pinned target and record the figures per system. The 70%/85% figures in research R4 join by parsed name+version, **not** the store path FR-002 mandates — re-measure the real join yield and state it, rather than quoting R4
- [X] T046 Run the pre-PR gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` — no `+toolchain` (#1189), and assert on the positive signal
- [ ] T047 Refresh corpus goldens via `public-corpus.yml` workflow dispatch, never locally, and verify SC-006: every target that does not scan a closure is byte-identical
- [ ] T048 Review the refreshed goldens with a normalised, sorted diff — mask content-addressed ids including `rel-`/`anno-` and `LC_ALL=C sort` before diffing, or SPDX-3 array reordering reads as a semantic hit

---

## Dependencies

```
Phase 1 (T001–T004)
      ↓
Phase 2 (T005–T014)  ← blocks everything
      ↓
Phase 3 US1 (T015–T025, T049)  ← MVP
      ↓
Phase 4 US2 (T026–T032)   depends on US1's join and emission
      ↓
Phase 5 US3 (T033–T039, T050)   depends on the acquisition existing
      ↓
Phase 6 (T040–T048)
```

US2 and US3 both build on US1's join; US1 alone is a shippable increment.
T040 must be one commit with T041. T047 precedes T048. T049 and T050 were
added after the analysis pass and carry trailing IDs rather than being
inserted — renumbering would have silently broken the range references above.

## Parallel opportunities

- **Phase 1**: T002, T003, T004 together.
- **Phase 2**: T008, T009 together once T007 lands; T014 with them.
- **Phase 3**: T020–T023 and T049 together once T015–T019 land.
- **Phase 4**: T029, T030, T031 together once T026–T028 land.
- **Phase 5**: T036, T037, T038 together once T033–T035 land; T050 after them.
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


---

## Deviations found during implementation

Four things the plan got wrong, each discovered by reading the code rather
than by a test failing. Recorded here so they are not re-derived later.

1. **`evaluate_licenses` is not a trait method** (supersedes T010–T012).
   `contracts/license-lookup.md` put it on the `Evaluator` trait, but that
   trait serves the milestone-1034 *version* tier. The closure tier calls
   `invoke::` functions directly — `closure::resolve` invokes
   `verify_ifd_refusal` itself at `closure/mod.rs:314`. So the lookup is a
   plain `eval::licenses::resolve_licenses`, called from
   `closure::attach_licenses`, inheriting the IFD verification the closure run
   already performed. Putting it on the trait would have been the second,
   parallel set of controls FR-012 forbids.

2. **The reused license type is the package-db one, not m1273's**
   (supersedes T026). There are two `DeclaredLicense` types. Milestone 1273's
   lives in `haskell_packages/package_set.rs` and is Haskell-reader-local with
   no path to `licenses[]`. Milestone 1010's
   `scan_fs/package_db/declared_license.rs` is the package-db-wide ladder —
   `Canonical` / `Preserved` / `Absent`, `into_licenses()`, already wired to
   LicenseRef minting — and is what every other reader states licenses
   through. Reusing it is what makes FR-003 hold without new emission code.

3. **No new catalog row** (supersedes T040–T041). The coverage record is an
   additive `licenses` key inside C184, exactly as milestone 1280 added
   `edges`. C184's extractor is `cdx_anno!` over the whole JSON string and is
   `SymmetricEqual`, so a new key is carried identically by all three formats
   with no extractor change. A new row would have been a second thing to keep
   in step for no new signal. The C184 prose documents the key.

4. **FR-004 was a live defect, not a precaution** (T024).
   `merge_closure_components` carried `extra_annotations` and nothing else, so
   a closure member matching a manifest component at the same `(name,
   version)` lost its licenses outright on merge. The reconciliation now fills
   only where the existing component has none.

## What is not closed

**SC-001 and SC-009 are not verified.** They are stated for aarch64-darwin,
where the 96-component gap was counted; the measurement available here is a
x86_64-linux closure, which is a different closure rather than a scaling of
the same one. The figures are in `measurements/README.md` with that caveat
stated. Closing them needs a darwin closure capture.

**T047 / T048 (corpus goldens) are not run.** Goldens refresh through the
`public-corpus.yml` workflow dispatch, never locally.
