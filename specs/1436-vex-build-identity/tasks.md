# Tasks: Nix closure VEX that scanners honour, without overclaiming

**Input**: Design documents from `/specs/1436-vex-build-identity/`
**Prerequisites**: plan.md, spec.md, research.md (R1–R11), data-model.md, contracts/backport-vex.md, quickstart.md

**Tests**: requested (FR-011, SC-001–SC-005). Tests come before implementation in each story.

**Rewriting existing tests:**
- The existing m1035/m1068 tests pin the two-statement shape. Each one rewritten is **mutation-checked**: it must fail when the old shape is restored.
- Revert a mutation with `cp` from a copy taken first, never `git checkout` on a file.

**Fixture rule:** every non-boot package is a `waybill-fixture-*` name. Use only synthetic CVE ids in fixtures (`CVE-2099-0001` style), so no committed fixture names a real vulnerable coordinate.

## Path conventions

- `NC` = `waybill-cli/src/scan_fs/package_db/nix/closure`
- `OV` = `waybill-cli/src/generate/openvex/mod.rs`
- `CV` = `waybill-cli/src/generate/cyclonedx/vex.rs`
- `T` = `waybill-cli/tests`

---

## Phase 1: Setup

- [X] T001 Measure R4's variant question, from waybill itself. Do not re-derive patch-to-CVE matching in a script; that would be modelling the reader.
  - **When:** after T004, which gives waybill per-variant patch sets.
  - **How:** add one INFO line in `NC/emit.rs`, `nix-closure: backport identity`, with three counts:
    - (a) components with ≥1 resolved CVE;
    - (b) how many of those merge >1 patching derivation;
    - (c) (component, CVE) pairs that not every variant resolves, which is the C220 count.
  - **Local inputs:** only the five nixpkgs wrapper targets (curl, openssh, httpie, gh, ripgrep) over nixpkgs `a799d3e3`, as wrapper flakes in a temp dir outside the repo. They evaluate nixpkgs only.
  - **Moat:** **never evaluate moat, or any other third-party flake, locally.** Take moat's counts, and the other corpus closure targets', from the CI corpus run's log (T025).
  - **Record:** `specs/1436-vex-build-identity/measurements/variants-results.tsv`, plus a note in `measurements/README.md`.
  - **Gate:** if (c) is a large share of (a)'s pairs, stop and report before US1.
- [X] T002 [P] Re-measure SC-001's baseline. For the five nixpkgs wrapper targets (moat's baseline comes from its committed golden, because it is never evaluated locally), scan with current main (`waybill sbom scan --nix-closure`), then run `measurements/fixed-qualified-probe.sh` with `GOLDENS` pointed at those documents (each as `<target>/cdx.json`). Record `none` (patched findings reported) and the synthetic-VEX result in `measurements/baseline-2026-11-12.tsv`. Use the grype and database pins from `quickstart.md`. This is the number SC-001 is judged against.

## Phase 2: Foundational (blocks every story)

- [X] T003 [P] Write unit tests in `NC/patches.rs` (`#[cfg(test)]` with `#[cfg_attr(test, allow(clippy::unwrap_used))]`):
  - two derivations of one identity with the same patches: the merged record keeps both derivation keys, and each one's resolved-CVE set;
  - two derivations where only one resolves CVE X: X is in the union but not in the every-variant set;
  - a single derivation: one key.
- [X] T004 Implement research R11 in `NC/patches.rs`.
  - `ComponentPatches` keeps `variants: BTreeMap<String /*drv key*/, BTreeSet<String /*CVE ids*/>>` beside the existing unioned `patches`.
  - `merge_by_identity` fills it, and keeps unioning `patches` exactly as today, so `pedigree` output is unchanged.
  - Add `fn every_variant_resolves(&self) -> BTreeSet<&str>` and `fn drv_hashes(&self) -> Vec<String>`. The hashes are the store-hash part of each `.drv` key, sorted. Use the existing store-path basename helper in `NC/derivation.rs`.
- [X] T005 [P] Write unit tests in `NC/emit.rs` for the qualifier:
  - a component with ≥1 resolving patch gets `?nix_drv=<h1>[,<h2>]` (sorted) on its PURL;
  - a component with patches but no `resolves` keeps its PURL byte-identical (clarification 2);
  - two scans of the same closure give the same PURL (US3-1).
- [X] T006 Implement the qualifier in `NC/emit.rs`, at the `ANN_CLOSURE_PATCHES` attachment around `:636`.
  - When `record.distinct_cves()` is non-empty, rebuild the component's PURL with qualifier `nix_drv` = `record.drv_hashes().join(",")` through `waybill_common::types::purl::Purl`.
  - Do it **before** `graph::project`, so the closure edges are built from the qualified PURL (research R6).
  - Carry the every-variant CVE set to the VEX generator **in memory**, not as an emitted annotation. A new `waybill:` annotation would need a catalog row and extractors, and nothing needs it in the documents.
  - If no in-memory path exists without broad plumbing, stop and record the choice in `research.md` as R12 before emitting anything.
  - *Done (2026-11-12):* the in-memory path is an internal annotation filtered by `is_internal_emission_key`, so no R12. The qualifier is also re-applied by the scan immediately before `graph::relationships`, because three identity passes rebuild PURLs after emission; see research R6's implementation note.
- [X] T007 Check the merge with manifest components in `waybill-cli/src/cli/scan_cmd.rs` (`merge_closure_components`). A closure member whose **unqualified** PURL equals a manifest component's must still merge, and the qualified PURL is kept (research R6).
  - Write the test first: a fixture where the manifest tier also emits the patched component.
  - Any rename goes through `apply_renames`.
- [X] T008 [P] Write an orphan-endpoint assertion helper in `T/common/` (or reuse one if it exists; grep for "dangling" first): every relationship endpoint is a component in each of the three formats. The suites below use it.

**Checkpoint:** with only Phase 2 in, the closure goldens differ only by `?nix_drv=` on CVE-resolving components, and the gate passes. VEX is still the old shape.

---

## Phase 3: User Story 1 — the scanner stops reporting patched CVEs (P1) 🎯 MVP

**Goal:** the OpenVEX sidecar carries one `fixed` statement per (qualified component, CVE resolved by every variant). There is no `affected` half and no root dependency.

**Independent test:** grype with the emitted sidecar suppresses every patched finding on the fixtures and goldens, and nothing else.

### Tests for User Story 1

- [X] T009 [P] [US1] Create the fixture `T/fixtures/nix_closure_backport_vex/`, modelled on the closure-patch fixtures that `T/closure_vex_subject.rs` uses (`patched()` helper, `:48`). It contains:
  - `waybill-fixture-zip` with two variants that both apply a patch named for `CVE-2099-0001`;
  - `waybill-fixture-ssh` with one variant resolving `CVE-2099-0002` and `CVE-2099-0003`;
  - `waybill-fixture-split` with two variants where only one resolves `CVE-2099-0004`;
  - `waybill-fixture-plain` with a non-CVE patch;
  - an unpatched `waybill-fixture-zip` built under another name/version for the leak case;
  - a wrapper-flake project with no main-module.
- [X] T010 [P] [US1] Write `T/nix_closure_backport_vex.rs`:
  - `one_fixed_statement_per_resolved_pair`: zip gets 1 statement (both variants resolve it) and ssh gets 2. Each product `@id` is the qualified PURL, with no subcomponents and status `fixed`.
  - `no_affected_statement_for_a_backport` (FR-003).
  - `a_cve_only_some_variants_resolve_gets_no_statement`: split has none.
  - `statements_need_no_document_subject`: a wrapper flake, no main-module, statements are emitted.
  - `the_grade_note_is_kept` (FR-004).
  - `a_declaration_withholds_the_fixed_claim` (R10): seed a nixpkgs declaration for (ssh, CVE-2099-0002) the way `T/nixpkgs_declarations.rs` does.
  - `the_unpatched_build_is_never_named`: the unpatched zip keeps an unqualified PURL, and no statement names it (SC-002's leak case).
  - `projected_documents_keep_statements_with_components`: under `--split` and `--tier`, each statement stays in the document holding its component, and C193 counts the claims a projection drops.
  - `no_orphan_endpoint` (T008).
  - *Done (2026-11-12), with one change:* `--split` is asserted at unit level (`generate/cyclonedx/vex.rs`, `a_split_document_carries_the_backport_claims_of_what_it_holds`), because the stub harness writes per-format `--output`s and `--split` refuses them. A backport claim is derived from its component, so each split document and its sidecar carry exactly the claims of the components it holds, and C193 never counts one (as under milestone 1035). `--tier` does not drop closure members (it runs before the closure tier), so the integration test asserts the claims stay; a root override likewise.
- [X] T011 [US1] Rewrite the OpenVEX unit tests that pin the pair in `OV`'s test module:
  - `a_backport_produces_both_statements_with_different_subjects` (`:1063`);
  - `the_affected_statement_does_not_say_resolved` (`:1164`);
  - `a_lone_not_affected_cannot_be_emitted` (`:1195`);
  - `each_affected_statement_says_what_to_do_within_its_evidence` (`:1390`);
  - `a_declaration_withholds_the_patch_derived_not_affected` (`:992`).

  Each asserts the new rule (or is removed with a one-line note in the commit message saying which FR made it obsolete). Each kept one is mutation-checked against the old `backport_statements`.

### Implementation for User Story 1

- [X] T012 [US1] Rewrite `backport_statements` in `OV` (`:292`):
  - Iterate components carrying the T006 every-variant set.
  - For each (component, CVE) in it, emit one `OpenVexStatement`: product `@id` and `identifiers.purl` = the component's (qualified) PURL, no subcomponents, status `Fixed`, and the m1035 grade note in the impact statement.
  - Remove the `affected` half and `backport_action` (`:111`) if nothing else uses it.
  - Remove the `root_component_purl` dependency from this path. Other callers keep it.
- [X] T013 [US1] Carry the declaration withholding (R10) in `OV`. Change `is_withheld_not_affected` (`:163`), or add a sibling, so a backport `Fixed` statement is withheld when `withheld` contains (product PURL with qualifiers stripped, CVE). Keep the declaration statements unchanged (FR-008).
- [X] T014 [US1] Run T010, T011 and the existing VEX suites:
  - `T/closure_vex_subject.rs`, `T/openvex_sidecar_path.rs`, `T/vex_per_instance.rs`, `T/nixpkgs_declarations.rs`, `T/binding_drift.rs`, `T/sbom_edit_filter.rs`.
  - Fix each failure that pins the old shape the same way as T011 (rewrite plus mutation check), and list them in the commit message.
  - `T/closure_vex_subject.rs` tests the #1436 part 1 subject fix. Its assertions about the `not_affected` subject become assertions that statements exist without a subject.

**Checkpoint:** grype, run on a T009 scan with the emitted sidecar, suppresses every fixture pair. Run this by hand with the quickstart pins, and record it in the commit message.

---

## Phase 4: User Story 2 — the document never says a patched component is exploitable (P1)

**Goal:** one CycloneDX `vulnerabilities[]` entry per backport pair, on the component's bom-ref, in state `resolved`.

**Independent test:** on the fixture and on every regenerated closure golden, 0 `exploitable` entries name a component whose own `resolves` lists that CVE.

### Tests for User Story 2

- [X] T015 [P] [US2] Add to `T/nix_closure_backport_vex.rs`:
  - `cdx_has_one_resolved_entry_per_pair_on_the_component`: `affects[0].ref` is the qualified bom-ref, the state is `resolved`, and there is no `waybill:vex-subcomponent` property.
  - `cdx_never_marks_a_patched_component_exploitable` (SC-003).
  - `sidecar_and_cdx_agree` (FR-006): the same (PURL, CVE) set in both.
  - `spdx_formats_carry_the_same_qualified_purl`: SPDX 2.3 `externalRefs` purl and SPDX 3 `software_packageUrl`.
- [X] T016 [US2] Rewrite `a_backport_is_two_entries_with_distinct_subjects` in `CV` (`:292`) to the single-entry rule, mutation-checked.

### Implementation for User Story 2

- [X] T017 [US2] In `CV`, a backport statement's entry needs nothing special. Its product is the component, so `affects[].ref` resolves to the component's bom-ref through the existing product-to-ref path. `Fixed` already maps to `resolved` (`:28`).
  - Check that no backport entry carries `waybill:vex-subcomponent`.
  - Fix any path that still assumes a root product for backports.
  - Keep `resolved_with_pedigree` unemitted (research R5).

**Checkpoint:** T015 and T016 pass, and US1's tests still pass.

---

## Phase 5: User Story 3 — a consumer can tell what the qualifier means (P2)

**Goal:** the qualifier and C220 are documented and recorded in all three formats.

### Tests for User Story 3

- [X] T018 [P] [US3] Add to `T/nix_closure_backport_vex.rs`:
  - `c220_counts_claims_not_every_variant_resolves`: `waybill-fixture-split` gives `waybill:vex-claims-unscoped` = `"1"` in all three formats.
  - `no_c220_when_nothing_is_unscoped`: on a fixture without split, the row is absent.
- [X] T019 [P] [US3] Write a C220 parity test in `waybill-cli/src/parity/extractors/mod.rs`, following C193's tests.

### Implementation for User Story 3

- [X] T020 [US3] Emit C220 at document scope, using the same plumbing as C193 (`vex-claims-omitted`; grep for it in `waybill-cli/src/generate/`):
  - CDX `metadata.properties[]`, SPDX 2.3 document annotation, SPDX 3 document `Annotation`;
  - only when greater than zero.
- [X] T021 [US3] Add C220 extractors in `waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs`, and the C220 row in `docs/reference/sbom-format-mapping.md` from `contracts/backport-vex.md`. Both land together.
- [X] T022 [US3] Update catalog row F1 in `docs/reference/sbom-format-mapping.md`:
  - a closure backport is now one `fixed` / `resolved` entry on the component's qualified PURL;
  - the version-level claim lives in `pedigree.patches[].resolves`;
  - `waybill:vex-subcomponent` no longer appears for backports;
  - `resolved_with_pedigree` is still never emitted, and why (R5).
- [X] T023 [P] [US3] Rewrite the "Where the VEX appears" section of `docs/reference/nix-evaluation.md`: the `nix_drv` qualifier (what it identifies, why scanners need it, only on CVE-resolving components), the single-statement shape, C220, and the R10 declaration rule.

**Checkpoint:** every test in `T/nix_closure_backport_vex.rs` passes.

---

## Phase 6: Scanner gate and corpus (SC-001–SC-005)

- [X] T024 Run `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` (no `+toolchain`). Confirm per-target `test result: ok` lines and 0 failure blocks. Check `git diff --exit-code main -- Cargo.lock` (FR-012).
- [X] T025 Push the branch. Dispatch `gh workflow run public-corpus.yml --ref 1436-vex-build-identity -f branch=1436-vex-build-identity -f regen_goldens=true`. Review the artifact against the committed goldens with a normalized, sorted comparison, and record it in `measurements/golden-review.txt`:
  - only closure targets with CVE-resolving backports change;
  - the changed PURLs are exactly those components plus `?nix_drv=`;
  - `vulnerabilities[]` pairs become single `resolved` entries;
  - every other golden is byte-identical (SC-005);
  - no relationship endpoint is orphaned.
- [X] T026 Commit the goldens. Dispatch a non-regen run and confirm it passes.
- [X] T027 SC-001, SC-002 and SC-004 on the regenerated goldens. Modify `measurements/fixed-qualified-probe.sh` (or add a sibling) to take the **emitted** `waybill.openvex.json` instead of synthesizing one.
  - The sidecar comes from the regen artifact if present. Otherwise regenerate locally from the same build with `--nix-closure` on the committed corpus pins.
  - Assert: every patched finding is suppressed, 0 others are lost, and without VEX the findings equal the pre-change goldens' (grype and osv-scanner).
  - Record in `measurements/sc-results.tsv`.
- [X] T028 The same for the five nixpkgs wrapper targets, using T002's baseline. moat is covered by T027 through its regenerated golden, and is never evaluated locally. Record in `measurements/sc-results.tsv`. Re-check PyPI on httpie for FR-007, and if httpie has a CVE-resolving `pkg:pypi` component, its qualifier must lose no osv-scanner finding.

## Phase 7: Polish

- [X] T029 [P] Add a CHANGELOG entry under Unreleased (Changed). Name it as a VEX shape change for consumers: the pair becomes a single `fixed` / `resolved` claim, and patched components' PURLs gain `?nix_drv=`. Quote the SC-001/SC-002 numbers from T027/T028. Note that #1435 can leave draft.
- [ ] T030 Close-out: verify every task's claim against code or a recorded artifact before ticking it. Re-run the gate if anything changed since T024.

---

## Dependencies and execution order

- **T001 runs right after T004** and gates US1. Its result can send R4 back for a decision.
- **T002** is independent, and needed only by T028.
- **Phase 2:** T003→T004, T005→T006→T007, and T008 alongside. All of it blocks every story.
- **US1 (T009–T014)** comes before US2: US2 checks CDX entries derived from the statements US1 produces.
- **US3 (T018–T023)** needs T004's every-variant set (for C220) and can run alongside US2.
  - T020 and T021 touch emitters and extractors shared with nothing in US2.
  - T022 and T023 are docs.
- **Phase 6** comes after all stories. T025→T026→T027, and T028 after T002.

### Parallel example (after Phase 2)

```text
Agent A: T009 → T010 → T011 → T012 → T013 → T014 → T015 → T016 → T017
Agent B: T018 → T019 → T020 → T021 → T022 → T023
```

## Implementation strategy

- **MVP = Phases 1–3.** The identity plus the OpenVEX shape is what makes grype suppress patched findings.
- **US2** removes the CycloneDX contradiction (#1421).
- **US3** documents and records it.
- **Phase 6** proves SC-001–SC-005 on real closures.
- Every phase ends with the gate green and non-closure goldens unchanged.
