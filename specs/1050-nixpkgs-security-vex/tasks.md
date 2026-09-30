# Tasks: nixpkgs security declarations as VEX

**Feature**: `1050-nixpkgs-security-vex` | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Ordered so the riskiest measured assumption is exercised before anything
depends on it. Phase 2 reproduces research R1's 71% from Rust; if it cannot,
that is worth knowing before five phases are built on the number.

---

## Phase 1: Setup

- [X] T001 Create the module skeleton at `waybill-cli/src/scan_fs/package_db/nix/declarations/mod.rs` and register it in `waybill-cli/src/scan_fs/package_db/nix/mod.rs` — an unregistered module compiles to nothing, which milestone 1035 learned the slow way
- [X] T002 [P] Add the `DeclarationSource` enum (`TopLevel`, `Haskell`, `Python3`, `Perl`) with its wire forms in `waybill-cli/src/scan_fs/package_db/nix/declarations/mod.rs`, ordered as research R1 measured — the order is the resolution order, not alphabetical
- [X] T003 [P] Add the `AttributeResolution` enum (`Confirmed`, `PathMismatch`, `NoAttribute`) in `waybill-cli/src/scan_fs/package_db/nix/declarations/mod.rs`, keeping `PathMismatch` and `NoAttribute` distinct even though both report as unchecked — they say different things about why, and a later milestone widening the probed sets moves members between them

---

## Phase 2: Foundational — reaching the declarations

**Blocks every user story.** Nothing below can be tested until a declaration
can be read at all.

- [X] T004 Implement the single-expression Nix builder in `waybill-cli/src/scan_fs/package_db/nix/declarations/evaluate.rs` — one evaluation listing every distinct `pname`, never one invocation per member (376 process spawns is not an option)
- [X] T005 Include the `or null` guard on every attribute lookup in `waybill-cli/src/scan_fs/package_db/nix/declarations/evaluate.rs` — a missing attribute is not a `throw` and escapes `tryEval`; without it the run dies on the first nested-set name (measured: `ChasingBottoms`)
- [X] T006 Include `deepSeq` **inside** the `tryEval` in `waybill-cli/src/scan_fs/package_db/nix/declarations/evaluate.rs` — `tryEval` returns a lazy value, so without it the throw escapes at serialisation time and the first unfree package kills the run
- [X] T007 Evaluate against the project's own pinned nixpkgs via the flake's inputs in `waybill-cli/src/scan_fs/package_db/nix/declarations/evaluate.rs`, never a registry or channel nixpkgs — a declaration from another revision describes a different package set
- [X] T008 Route the argv through the existing `argv_is_safe` guard at `waybill-cli/src/scan_fs/package_db/nix/eval/invoke.rs` and the bounded-subprocess helper, so this path inherits milestone 1034's protections rather than restating them
- [X] T009 Implement candidate resolution across the ordered package sets in `waybill-cli/src/scan_fs/package_db/nix/declarations/resolve.rs`, first **confirmed** hit wins — a hit that fails the path check MUST NOT stop the search, or a name collision in one set masks a real match in the next
- [X] T010 Implement the output-path verification in `waybill-cli/src/scan_fs/package_db/nix/declarations/resolve.rs`, normalising **both** sides through `closure::derivation::store_basename` — the closure JSON omits the `/nix/store/` prefix that `outPath` carries, and comparing raw strings yields 0% coverage while reading as "the mechanism does not work"
- [X] T011 [P] Write a unit test in `resolve.rs` proving the path check rejects a same-named attribute that builds something else, with a CONTROL assertion that the fixture's two names really do collide — otherwise the test passes on any input
- [X] T012 [P] Write a unit test in `resolve.rs` proving a raw comparison (prefix intact on one side) finds nothing, pinning the R4 trap from both directions the way milestone 1035 pinned its basename trap
- [X] T013 Implement `Declaration` parsing in `waybill-cli/src/scan_fs/package_db/nix/declarations/parse.rs` — CVE extraction that does **not** consume the text (FR-003), since 72% of entries embed the identifier inside a description worth keeping
- [X] T014 [P] Write unit tests in `parse.rs` for: an entry naming several CVEs producing several identifiers, an entry with a CVE inside prose keeping both, and an empty list producing nothing
- [X] T015 Add the env-var-gated coverage cross-check in `waybill-cli/src/scan_fs/package_db/nix/declarations/mod.rs`, mirroring the milestone-1035 pattern, reporting confirmed / unchecked / path-mismatch counts against a real closure
- [X] T016 **Gate**: run the T015 cross-check in `waybill-cli/src/scan_fs/package_db/nix/declarations/mod.rs` against moat and reproduce research R1 — ~273 of 380 confirmed (71%), ~72 unreachable, ~35 rejected, with haskell contributing more than top-level. **Pass band**: confirmed 65–77% AND path-mismatch ≥ 5% AND haskell > top-level. Outside it, the plan rests on a number the implementation does not produce — stop and reconcile before any later phase. A mismatch below 5% is as suspicious as bad coverage: it means the verification is not rejecting anything and may not be running
- [ ] T017 Build the integration fixture at `waybill-cli/tests/fixtures/nix_declarations/` — a flake that pins a package carrying `knownVulnerabilities` **and** permits it, since no public project reaches this path (research R5). Package names MUST be synthetic (`waybill-fixture-*`); real coordinates in a fixture trip advisory scanning, which has bitten this repository twice
- [X] T018 Add `NixpkgsSecuritySummary` in `waybill-cli/src/scan_fs/package_db/nix/declarations/mod.rs` with the fields data-model.md lists, as a **sibling** to `NixClosureSummary` rather than an extension — the two degrade independently and the spec requires that be visible

**Checkpoint**: a declaration can be read, verified, and counted. Nothing is emitted yet.

---

## Phase 3: User Story 1 — what the package set itself declares (P1)

**Goal**: CVE-bearing declarations reach the consumer as VEX, attributed to nixpkgs.

**Independent Test**: scan the T017 fixture; assert a VEX statement names the CVE with the build as product and the component as subcomponent.

- [ ] T019 [US1] Extend `EvidenceGrade` with `NixpkgsDeclared` (wire `nixpkgs-declared`) in `waybill-cli/src/scan_fs/package_db/nix/closure/patches.rs` — the second variant the enum was deliberately built single-variant to accept
- [ ] T020 [US1] Encode the provenance ordering (`NixpkgsDeclared` outranks `FilenameDerived`) in `waybill-cli/src/scan_fs/package_db/nix/closure/patches.rs`, as an ordering over *sources* and not a confidence score — no number, because none was measured
- [ ] T021 [P] [US1] Write the failing test for SC-002 in `waybill-cli/src/generate/openvex/mod.rs`: a declaration-derived statement is **positively attributable to nixpkgs** *and* distinguishable from a patch-derived one, both without reading the text. Assert the attribution directly — asserting only the contrast with patch-derived leaves the first half of the criterion untested, and a statement attributed to nothing would pass it
- [ ] T022 [US1] Emit declaration-derived statements in `waybill-cli/src/generate/openvex/mod.rs` with the build as `product` and the declared component as `subcomponent` (FR-006a), reusing the `subcomponents` field milestone 1035 added
- [ ] T023 [US1] Emit one statement per identifier when a declaration names several (FR-004) in `waybill-cli/src/generate/openvex/mod.rs`, with a test asserting three identifiers yield three statements rather than one concatenated subject
- [ ] T024 [US1] Carry the grade in `impact_statement` in `waybill-cli/src/generate/openvex/mod.rs`, as milestone 1035 does, because OpenVEX has no grade slot and a bare status hides what it rests on
- [ ] T025 [US1] Thread `NixpkgsSecuritySummary` into `ScanArtifacts` in `waybill-cli/src/generate/mod.rs` and populate it in `waybill-cli/src/cli/scan_cmd.rs`, after the closure pass
- [ ] T026 [P] [US1] Write the integration test in `waybill-cli/tests/nixpkgs_declarations.rs` scanning the T017 fixture, with a CONTROL assertion that the fixture produced a confirmed declaration at all — otherwise every assertion below it passes over an empty set
- [ ] T027 [US1] Assert in `waybill-cli/tests/nixpkgs_declarations.rs` that no emitted document in any of the three formats gains a `vulnerabilities[]` array (FR-017, SC-008). The golden check at the end of Phase 8 cannot catch this: goldens are generated with the flag off, and this requirement is about the flag-on path

**Checkpoint**: SC-001, SC-002, SC-002a satisfiable.

---

## Phase 4: User Story 2 — two sources, one CVE (P1)

**Goal**: a declaration and a patch naming one CVE on one component produce a coherent answer.

**Independent Test**: construct a scan where both speak; assert exactly one VEX statement for that CVE and the patch still present in pedigree.

- [ ] T028 [P] [US2] Write the failing test for SC-005 in `waybill-cli/tests/nixpkgs_declarations.rs`: one `affected`, no `not_affected`, **and** the patch still in `pedigree.patches[]`. Assert both halves — asserting only the suppression passes equally well if the pedigree entry was dropped too
- [ ] T029 [US2] Implement the reconciliation in `waybill-cli/src/generate/openvex/mod.rs`: when a declaration and a patch name one CVE on one component, emit the declaration's `affected` and withhold the patch-derived `not_affected` (FR-012)
- [ ] T030 [US2] Match on (component, CVE) exactly in `waybill-cli/src/generate/openvex/mod.rs` — statements about different components or different CVEs MUST NOT reconcile against each other (FR-014), with a test for the different-component case
- [ ] T031 [US2] Record the withheld count on `NixpkgsSecuritySummary` in `waybill-cli/src/scan_fs/package_db/nix/declarations/mod.rs` (FR-013a), so "no patch statement produced" stays distinguishable from "a patch statement withheld"
- [ ] T032 [US2] Mutation-test the reconciliation in `waybill-cli/src/generate/openvex/mod.rs`: disable the withholding and confirm T028 fails, then restore. Assert the mutation applied **before** running the tests — a mutation run that silently did not apply reports a pass and proves nothing

**Checkpoint**: SC-005, SC-005a satisfiable.

---

## Phase 5: User Story 3 — declarations naming no CVE (P2)

**Goal**: the 28% that name nothing still reach the consumer.

**Independent Test**: scan a fixture whose declaration is prose; assert the text appears verbatim on the component.

- [ ] T033 [P] [US3] Write the failing test in `waybill-cli/tests/nixpkgs_declarations.rs`: a prose declaration reaches the component annotation with its text intact, not paraphrased or reduced to a flag
- [ ] T034 [US3] Emit the per-component annotation in `waybill-cli/src/scan_fs/package_db/nix/declarations/mod.rs`, carrying the declaration text verbatim (FR-010a)
- [ ] T035 [US3] Emit the no-CVE declaration count at document scope (FR-011) in `waybill-cli/src/scan_fs/package_db/nix/declarations/mod.rs` — the same reason milestone 1035 emits it for patches: partial coverage must not read as absence
- [ ] T036 [US3] Assert a component carrying both CVE-bearing and prose declarations emits both, neither displacing the other, in `waybill-cli/tests/nixpkgs_declarations.rs`

**Checkpoint**: SC-003 satisfiable.

---

## Phase 6: User Story 4 — the build accepted an exception (P2)

**Goal**: the acceptance is visible, and claims only what is supportable.

**Independent Test**: scan the T017 fixture; assert the record appears and does not name the operator's intent.

- [ ] T037 [US4] Emit the document-scope acceptance record in `waybill-cli/src/scan_fs/package_db/nix/declarations/mod.rs`, derived from a confirmed member carrying a declaration (FR-015)
- [ ] T038 [US4] Word the record in `waybill-cli/src/scan_fs/package_db/nix/declarations/mod.rs` to claim only that the build accepted a package nixpkgs marks insecure, never that the operator named it (FR-016) — `NIXPKGS_ALLOW_INSECURE=1` is indistinguishable from a targeted permission, with a test asserting the wording
- [ ] T039 [P] [US4] Assert absence of the record does not read as rejection (FR-016a) in `waybill-cli/tests/nixpkgs_declarations.rs` — a build with no insecure packages and a build never asked look identical from outside

---

## Phase 7: User Story 5 — degrading without hiding it (P3)

- [ ] T040 [US5] Degrade using milestone 1034's `DegradationReason` vocabulary in `waybill-cli/src/scan_fs/package_db/nix/declarations/mod.rs`, adding no new variant unless a measured failure fits none of the existing eight
- [ ] T041 [US5] Make the declaration pass degrade **independently** of the closure query in `waybill-cli/src/cli/scan_cmd.rs` — a closure that resolved still emits its components when this pass fails (FR-019)
- [ ] T042 [P] [US5] Assert a scan with no usable `nix` completes, emits no declaration-derived output, and records the reason, in `waybill-cli/tests/nixpkgs_declarations.rs`
- [ ] T043 [US5] Assert the flag-off path emits none of the new annotations and starts no additional evaluation, in `waybill-cli/tests/nixpkgs_declarations.rs`
- [ ] T044 [US5] Assert no scan issues a request to any external advisory database (FR-018, SC-009) in `waybill-cli/tests/nixpkgs_declarations.rs`, proven the way milestone 1035 proved its offline refusal — by exercising the path, not by reading the source. Inspection cannot prove a negative about code that has not run

**Checkpoint**: SC-007, SC-007a satisfiable.

---

## Phase 8: Polish & cross-cutting

- [ ] T045 Emit the four document-scope signals — unchecked-member count (FR-001d), no-CVE declaration count (FR-011), withheld-reconciliation count (FR-013a) and the acceptance record (FR-015) — in `waybill-cli/src/generate/cyclonedx/metadata.rs`
- [ ] T046 [P] Emit the same four signals in `waybill-cli/src/generate/spdx/annotations.rs`
- [ ] T047 [P] Emit the same four signals in `waybill-cli/src/generate/spdx/v3_annotations.rs`
- [ ] T048 Verify all four reach all three formats with one scan of the T017 fixture, in `waybill-cli/tests/nixpkgs_declarations.rs`. Computing a value into `NixpkgsSecuritySummary` does **not** emit it: per-component annotations ride the `extra_annotations` pass-through, document-scope ones need explicit emission in each of the three files above, as milestone 1035 required. Without this the catalogue rows below would carry extractors pointing at fields nothing writes
- [ ] T049 Add catalogue rows C186–C189 to `docs/reference/sbom-format-mapping.md` **in the same change as** their extractors — the gate fails in both directions, as milestone 1034 found
- [ ] T050 Edit `docs/reference/sbom-format-mapping.md` by **exact string match**, never anchored regex, and diff the whole field afterwards — a DOTALL pattern anchored on one row has silently edited the next one before
- [ ] T051 [P] Write extractors for C186–C189 across all three formats in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs`, carrying the Principle V audit from research R6 into each row
- [ ] T052 Extend the extractor-correctness test in `waybill-cli/src/parity/extractors/mod.rs` to cover the new rows, with per-side non-empty CONTROL assertions — registration-only checking passes when an extractor points at a field nobody writes, which is how milestone 1035 shipped a cardinality bug
- [ ] T053 Run the parity suite in the **lib** target (`cargo test -p waybill --lib`), not `--bins` — milestone 1035 read "3 passed" from `--bins parity` while the real suite had not run at all
- [ ] T054 [P] Measure the added wall-clock cost against a real closure and record it in `specs/1050-nixpkgs-security-vex/measurements/README.md` (FR-020b). A figure above roughly a fifth of closure-scan time reopens the automatic-by-default decision rather than absorbing it (SC-006a)
- [ ] T055 [P] Document the feature in `docs/reference/nix-evaluation.md` alongside `--nix-closure`, leading with the coverage annotation and stating plainly that an empty result is the common case because a building project has already permitted what it contains
- [ ] T056 Verify SC-006: with `--nix-closure` absent, every golden under `waybill-cli/tests/fixtures/public_corpus/` is unchanged (`git status --porcelain` clean after the suite)
- [ ] T057 Regenerate corpus goldens **in CI, not locally**, only if T056 shows legitimate churn
- [ ] T058 Run the walker-audit grep with the workflow's exact command if any `fn walk`-shaped function appeared — it is not in `pre-pr.sh`, and an approximation of the command matched 52 entries against an allowlist of 12 when a shell glob ate `--include=*.rs`
- [ ] T059 Run `./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo "EXIT=$?"` — read the **script's** status, never a pipeline's — then assert `>>> all pre-PR checks passed.` and the per-target `test result: ok. N passed; 0 failed`

---

## Dependencies & Execution Order

```
Phase 1 (Setup)
   │
Phase 2 (Foundational — reach) ◄── T016 is a hard gate: reproduce 71% or stop
   │                                T017 fixture gates every test below
   ├───────────────┬───────────────┬──────────────┐
Phase 3 (US1)   Phase 5 (US3)   Phase 6 (US4)  Phase 7 (US5)
   │  MVP           independent     independent    independent
Phase 4 (US2) ◄── needs US1's statements to reconcile against
   │
Phase 8 (Polish)
```

- **US1 → US2** is the only hard story dependency: there is nothing to reconcile until declaration statements exist.
- **US3, US4, US5 are independent of US1** and of each other. US3 and US4 read the same resolution output; US5 is about its absence.
- **T016 gates Phase 3 onward.** It is the only task whose failure invalidates the plan rather than the code.

## Parallel Opportunities

- T002, T003 — different types, same new file, no dependency
- T011, T012, T014 — unit tests in modules being written
- T021, T026 (US1), T028 (US2), T033 (US3), T039 (US4), T042 (US5) — the failing tests for each story
- T051, T054, T055 — extractors, measurement and docs touch disjoint files

## Implementation Strategy

**MVP = Phase 1 + 2 + 3.** That delivers what the feature is for: nixpkgs'
own declarations reaching a consumer as attributed VEX. US2 makes it coherent
alongside milestone 1035, US3 recovers the higher-value prose half, and
US4/US5 are transparency.

**Stop at T016 if the number disagrees.** Everything after it assumes ~71%
coverage. A throwaway probe produced that figure; if the shipped code does
not, the honest move is to reconcile the two before building on either.
