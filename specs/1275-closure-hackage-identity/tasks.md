# Tasks: closure members that are known Hackage packages

**Input**: Design documents from `/specs/1275-closure-hackage-identity/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/README.md](./contracts/README.md),
[quickstart.md](./quickstart.md)

**Tests**: REQUIRED, not optional. FR-014 and SC-008 demand a test per
requirement that fails when the behaviour is removed, mutation-tested. FR-004's
forbidden name-only match gets its own negative test, because an over-matching
implementation mints a confidently wrong identity — worse than the opaque one it
replaces. Tests are written before the implementation they cover, and watched to
fail first; milestone 1274 recorded writing its integration tests afterwards as a
departure, and this milestone does not repeat it.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel — different files, no dependency on an incomplete task
- **[Story]**: `[US1]`, `[US2]`, `[US3]`. Setup, Foundational, Edges and Polish
  phases carry no story label: Phase 6's edges serve US1's components but no US1
  acceptance scenario mentions them, and plan.md's Phase 2 note makes them
  separately shippable, so they are their own phase rather than a story's tail.

## Paths

Single Rust workspace. The pass lands in
`waybill-cli/src/scan_fs/package_db/nix/haskell_packages/closure_identity.rs`;
the call site is `waybill-cli/src/cli/scan_cmd.rs`; the suite is
`waybill-cli/tests/nix_closure_hackage_identity.rs`.

---

## Phase 1: Setup

**Purpose**: establish the baseline this milestone is measured against, and the
mutation harness SC-008 needs. No production code.

- [ ] T001 Re-run the committed probe and record the baseline in `specs/1275-closure-hackage-identity/measurements/baseline-001fa50.md`: `cargo build -p waybill --bin waybill`, then produce `cdx.json` and `closure.json` per `quickstart.md` and run `python3 -I specs/1275-closure-hackage-identity/measurements/size-closure-members.py`. Record the five figures the quickstart states (276 generic, 60 exact, 26 name-only, 190 absent, 14 no-version) as observed, and note any that differ — a drift means the fixture or revision moved and SC-001/SC-003's populations must be restated before anything is built on them (SC-009, C21).
- [ ] T002 [P] Copy milestone 1274's mutation harness to `specs/1275-closure-hackage-identity/measurements/mutate.py` and adapt its target paths to this milestone's files. It is the instrument for SC-008, and a mutation that SURVIVES is a finding about the test, not a formality — m1274's M8 survived because the mutation was wrong, and its replacement was first caught by a neighbouring suite.
- [ ] T003 [P] Create `specs/1275-closure-hackage-identity/measurements/findings.md` with an empty numbered list. Every probe lie, surprise and departure in the phases below is appended here as it happens, not reconstructed at the end (the `happy-lib` / `ghc-binary` pattern error in research R5 is the model).

**Checkpoint**: the numbers the spec's success criteria quote are observed on this branch, not inherited from the spec's prose.

---

## Phase 2: Foundational (blocking prerequisites)

**Purpose**: three things must be settled before any reclassification can be
written — where the package set comes from when `enrich` declines to run, which
wire carrier FR-007's reason uses, and the pure decision function itself.

**⚠️ CRITICAL**: no user story work begins until T004–T010 are complete. T005 and
T006 can each invalidate a design claim in plan.md, and discovering that after
US1 is written means rewriting US1.

- [ ] T004 Extract the package-set load path out of `enrich` into a reusable loader in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs`, returning the revision, the parsed `PackageSet` and the boot-library `nulled` set, or an `UnresolvedReason`. The sequence to lift is `scan_root.join("flake.lock")` → `parse_flake_lock` → `pinned_nixpkgs` → `location` → `cached_or_fetch(HACKAGE_PACKAGES_PATH, HACKAGE_PACKAGES_KEY)` → `package_set::parse` → the `configuration-common.nix` rebinds (`mod.rs:795`–`888`). `enrich` must call the new loader and behave identically — it keeps its own `is_enrichable` and `flake.lock` gates, which the new pass must NOT inherit (research R2: a repository whose only Haskell content comes from the closure is exactly the scan #1282 is about, and `enrich` returns `None` on it).
- [ ] T005 Measure what a second parse of the package set costs, and record it in `specs/1275-closure-hackage-identity/measurements/findings.md`. The file is **16,633,916 bytes** at the measured revision (`~/.cache/waybill/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb/hackage-packages.nix`), and the new pass needs the same `PackageSet` `enrich` just built but has no way to receive it: `EnrichmentSummary` does not carry it, and adding it would break that struct's `PartialEq`. Time `package_set::parse` over that file, min/median/max of N≥3 — one run is not a measurement. plan.md claims "no measurable change"; if a second parse is material, the fix is a per-scan memo keyed by revision (**not** a process-global one: the test suites run many scans in one process and would serve a stale set), and this task is where that is decided on the number rather than on preference.
- [ ] T006 Settle which wire carrier FR-007's reason uses, and record the decision in `specs/1275-closure-hackage-identity/measurements/findings.md` plus a plan.md correction. **plan.md's Principle V row is wrong on this point and it was checked, not assumed**: C151 `waybill:unresolved-reason` is emitted "iff and only if the same component carries `waybill:sbom-tier: \"design\"`" (`docs/reference/sbom-format-mapping.md:203`), the reasoning #1268 relies on at `waybill-cli/src/generate/cpe.rs:63`, and closure components are built tier — `sbom_tier: Some("build")` at `nix/closure/emit.rs`. C170 `waybill:haskell-version-unresolved-reason` is a closed enum about why a declared dependency has **no version**, and these components all have one; what is unresolved here is identity. So the likely outcome is a **new** C-row. If it is, T006 also adds the row to `docs/reference/sbom-format-mapping.md` with its Principle V audit and the matching entry in `waybill-cli/src/parity/extractors/mod.rs` — a doc row without an extractor fails `parity::extractors::tests::every_catalog_row_has_an_extractor` and `holistic_parity`, so the two land together or neither does.
- [ ] T007 Create `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/closure_identity.rs` holding the `HackageIdentity` enum exactly as `data-model.md` §2 specifies — six variants, exhaustive, no `_ =>` arm. Add `pub(crate) mod closure_identity;` to that directory's `mod.rs`. Enum only in this task; the decision function is T008.
- [ ] T008 Implement the two pure decision helpers in `closure_identity.rs`, with unit tests in the same file written first: (a) the exact-pair test — a single `PackageSet::get(name)` whose `entry.version == component.version`, the default attribute only per research R4, which costs 1 of 60 (`hashable 1.4.7.0` → `hashable_1_4_7_0`) and buys no new ambiguity; (b) the GHC signal — `RawDerivation::paths_in("nativeBuildInputs")` containing a store path matching `-ghc(-binary)?-\d`. The pattern is load-bearing and `-ghc-\d` is **wrong**: it reports `happy-lib 2.1.7` as not-GHC-built because that package bootstraps with `ghc-binary-9.0.2` (research R5). Guard `#[cfg(test)]` with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per the crate's deny.
- [ ] T009 Write the FR-006 agreement probe at `specs/1275-closure-hackage-identity/measurements/signal-agreement.py` and run it against the real closure, recording the result in `findings.md`. It reports, per bucket, how many rows the exact-pair signal and the GHC signal agree on. Expected 60 of 60 with the GHC signal rejecting all 26 collisions independently (research R5). **A single disagreeing row is a finding to explain before the pass is trusted** — that is how R5's pattern error was caught, and asserting the agreement instead of measuring it is what FR-006 forbids (C7).
- [ ] T010 Build the test fixtures at `waybill-cli/tests/fixtures/nix_closure_hackage/`: a seeded `hackage-packages.nix` and a stub closure JSON covering all five outcomes — one exact pair whose derivation references a GHC, one name collision at a different version, one exact pair whose derivation references no GHC (the `SignalsDisagree` row, measured 0 of 60 and therefore only reachable from a fixture), one name absent from the set, and one member with no version. **All names are `waybill-fixture-*`**; real coordinates in fixtures trip the advisory scan (#285, #640), so the collision case is a synthetic pair rather than `bash`. Seed the set through `WAYBILL_NIXPKGS_CACHE` pointed at a `tempfile::tempdir()` the way `waybill-cli/tests/nix_haskell_resolution_m926.rs:116` does, and drive nix through the stub that `waybill-cli/tests/nix_config_closure.rs:47` establishes. **No test reads the host's `~/.cache/waybill/nixpkgs/`** (Principle VII).

**Checkpoint**: the decision function is tested in isolation, the loader serves a closure-only scan, the reason's carrier exists on the wire with an extractor, and the fixture can express every outcome including the one real data never produces.

---

## Phase 3: User Story 1 — a Haskell package in the closure is identified as one (P1) 🎯 MVP

**Goal**: every closure member the pinned set identifies exactly carries
`pkg:hackage/<name>@<version>` with that revision's license and hash.

**Independent Test**: scan the fixture with `--nix-closure` and assert the exact-pair
members carry `pkg:hackage` PURLs with licenses present at the resolved
components' rate. Measured today: 0 of 60 (SC-001), 0 of 60 licensed against 47
of 65 (SC-002).

### Tests for User Story 1 (write first, watch fail)

- [ ] T011 [P] [US1] Create `waybill-cli/tests/nix_closure_hackage_identity.rs` with the C2 test: a fixture closure member whose default attribute's version equals its own, and whose derivation references a GHC, is emitted as `pkg:hackage/<name>@<version>`. Assert on the positive signal — the PURL's presence — never on the absence of `pkg:generic`.
- [ ] T012 [P] [US1] Add the C3 license test to `waybill-cli/tests/nix_closure_hackage_identity.rs`: the reclassified component carries the revision's declared license, including a non-canonicalisable declaration preserved as a `LicenseRef-` rather than dropped, by the same rules `DeclaredLicense` already applies (FR-002).
- [ ] T013 [P] [US1] Add the C4 hash test to `waybill-cli/tests/nix_closure_hackage_identity.rs`: the component carries `source_hash` as hex, or **no hash at all** when the revision records one that does not decode to 32 bytes — never a malformed value (FR-003, Principle IX).
- [ ] T014 [P] [US1] Add the US1 scenario-2 test to `waybill-cli/tests/nix_closure_hackage_identity.rs`: the document holds exactly **one** component for the reclassified identity, with no `pkg:generic` twin beside the `pkg:hackage` one.
- [ ] T015 [US1] Run T011–T014 and record in `findings.md` that each fails, and with what message. A test never watched to fail is not yet evidence of anything (`feedback_a_passing_test_may_exercise_nothing`), and a control that cannot fail is not a control.

### Implementation for User Story 1

- [ ] T016 [US1] Implement the reclassification pass in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/closure_identity.rs`: over `&mut [ResolvedComponent]`, for each closure-appended component, compute the `HackageIdentity` from T008's helpers and, on `Confirmed` only, rewrite `purl` to `pkg:hackage/<name>@<version>` and set `licenses` and `hashes`. `name`, `version` and the `waybill:closure-role` annotation are left untouched (data-model §3, FR-011). Return a summary carrying the per-variant counts and the rename pairs.
- [ ] T017 [US1] Build the rename map **inside** T016's reclassification and apply it through the existing `apply_renames` (`haskell_packages/mod.rs:562`) in the same step, not as a following pass. #980 measured the alternative: CycloneDX keeps an edge pointing at the old PURL, SPDX drops the relationship outright, and **neither format errors** — so an ordering mistake here produces a document that validates and is wrong (research R8, data-model §4, C15).
- [ ] T018 [US1] Wire the call site in `waybill-cli/src/cli/scan_cmd.rs`, **after** `merge_closure_components` (`scan_cmd.rs:4729`) and before emission. Not earlier: milestone 1035 measured that moving the merge before the Haskell enrichment downgraded **41** components from `pkg:hackage/QuickCheck@2.15.0.1` to `pkg:generic/...` — enabling the closure made PURLs worse — and the comment recording it sits at `scan_cmd.rs:4710`–`4720`. Pass the `ClassifiedClosure` (still in scope as `nix_closure`) for the GHC signal, and the loader's `PackageSet` from T004.
- [ ] T019 [US1] Verify T011–T014 now pass, then run mutation M1 (invert the exact-pair comparison to `!=`) and M2 (drop the license assignment) through `measurements/mutate.py` and confirm both are CAUGHT. Record in `findings.md`. Run these now rather than deferring them — m1274 deferred and had to record the departure.

**Checkpoint**: US1 is independently demonstrable — the fixture's exact-pair members carry Hackage identity, license and hash, and the mutants prove the tests have teeth. **Do not ship US1 alone**: without Phase 4, the same code path mints `pkg:hackage/bash@5.3p9`.

---

## Phase 4: User Story 2 — a name collision does not mint a wrong identity, and says so (P2)

**Goal**: 26 of 276 measured components have a Hackage name and nothing to do
with Haskell. None of them is ever given a Hackage coordinate, and each says it
was considered.

**Independent Test**: assert no component in the emitted document carries a
`pkg:hackage` PURL whose version is absent from the pinned set for that name,
and that each near-miss is annotated with why it was left generic.

**This story lands in the same PR as US1.** It is what makes US1 publishable
rather than dangerous: `pkg:hackage/bash@5.3p9` would answer a vulnerability
feed for GNU bash with a small unrelated Haskell library's coordinate.

### Tests for User Story 2 (write first, watch fail)

- [ ] T020 [P] [US2] Add the C5 negative test to `waybill-cli/tests/nix_closure_hackage_identity.rs`: a fixture member whose name is in the set at a **different** version keeps `pkg:generic`, and **no** component in the whole document carries a `pkg:hackage` PURL whose version is absent from the set for that name. This is the single most important test in the milestone (FR-004, SC-003, quickstart "the three things to hold on to" #2).
- [ ] T021 [P] [US2] Add the C6 two-signal test to `waybill-cli/tests/nix_closure_hackage_identity.rs`: the `SignalsDisagree` fixture row — an exact pair whose derivation references no GHC — is **not** reclassified. This exercises the variant real data has never produced (0 of 60), which is why T010 built a fixture for it.
- [ ] T022 [P] [US2] Add the C9 test to `waybill-cli/tests/nix_closure_hackage_identity.rs`: no two components in the document share one identity. Research R7 says this is satisfied by construction — `merge_closure_components` keys on `(name, version)` and reclassification changes neither — but "satisfied by construction" is a claim about code that can change, so FR-008 gets a test rather than an argument.
- [ ] T023 [US2] Add the US2 scenario-2 test to `waybill-cli/tests/nix_closure_hackage_identity.rs`: the collision member carries an annotation recording that a same-named Hackage package exists and was deliberately not used, so the decision is auditable rather than silent. Uses T006's carrier.
- [ ] T024 [US2] Run T020–T023 and record that each fails, and how, in `findings.md`.

### Implementation for User Story 2

- [ ] T025 [US2] Make the GHC signal a hard gate in `closure_identity.rs`: `Confirmed` requires **both** signals, and an exact pair without the GHC signal yields `SignalsDisagree`, not `Confirmed`. A name-or-version-only path must not exist in the code at all (FR-005, C6).
- [ ] T026 [US2] Emit the collision annotation in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/closure_identity.rs` for `VersionDisagrees` and `SignalsDisagree` through T006's carrier, naming the set version that did not match.
- [ ] T027 [US2] Verify T020–T023 pass, then run mutation M3 (drop the GHC-signal conjunct, leaving the name-and-version match alone) and M4 (relax the version comparison to a name match) and confirm both are CAUGHT by T020 specifically. Record in `findings.md`. If M4 is caught only by a neighbouring suite, say so — m1274's M8b was, and the reason was a control that could not fail.

**Checkpoint**: US1 + US2 together are shippable. The reclassification is now safe by measurement rather than by intention.

---

## Phase 5: User Story 3 — an unresolved closure member says why (P3)

**Goal**: every remaining `pkg:generic` closure member distinguishes "waybill
looked and this is not a Haskell package" from "waybill did not look". Measured
today: 0 of 276 carry any reason.

**Independent Test**: assert every `pkg:generic` closure member carries a reason,
and that the five outcomes are distinguishable from each other on the wire.

### Tests for User Story 3 (write first, watch fail)

- [ ] T028 [P] [US3] Add the C8 coverage test to `waybill-cli/tests/nix_closure_hackage_identity.rs`: **every** `pkg:generic` closure member in the emitted document carries a reason. Assert the count equals the generic-member count, not merely that some reasons exist — a check that did not run looks identical to a check that passed.
- [ ] T029 [P] [US3] Add the five-outcome test to `waybill-cli/tests/nix_closure_hackage_identity.rs`: `NotInPackageSet`, `VersionDisagrees`, `SignalsDisagree`, `NoVersionRecorded` and `PackageSetUnavailable` each produce a distinct wire value. `NoVersionRecorded` must be distinguishable from a failed lookup — the lookup was *impossible*, which is a different fact and calls for different action (US3 scenario 2, FR-007).
- [ ] T030 [P] [US3] Add the C10 degradation test: with the package set unavailable (point `WAYBILL_NIXPKGS_CACHE` at an empty dir and run offline, as `nix_haskell_resolution_m926.rs:167` does), every member stays `pkg:generic`, the **scan still succeeds**, and the document records that the pass did not run (FR-009, Principle XI). Assert on the recorded value's presence — not on the absence of Hackage PURLs, which a crashed pass would also produce.
- [ ] T031 [US3] Run T028–T030 and record that each fails, and how, in `findings.md`.

### Implementation for User Story 3

- [ ] T032 [US3] Emit the reason for every non-`Confirmed` variant in `closure_identity.rs` through T006's carrier. The enum is exhaustive, so this is a total match with no fallback arm — a `_ => None` is how a later variant silently loses its reason (#1218, #892, Principle IV).
- [ ] T033 [US3] Handle `PackageSetUnavailable` in the `scan_cmd.rs` call site: when T004's loader returns an `UnresolvedReason`, every member lands in that variant and the document records that the pass did not run. Nothing fails, nothing panics.
- [ ] T034 [US3] Verify T028–T030 pass, then run mutation M5 (collapse `NoVersionRecorded` into `NotInPackageSet`) and M6 (make the unavailable path emit nothing rather than a recorded reason) and confirm both are CAUGHT. Record in `findings.md`.

**Checkpoint**: all three stories are independently functional. 59 members matchable, 217 that say why they are not.

---

## Phase 6: Declared dependency edges (FR-012, separable)

**Purpose**: give reclassified components the `dependsOn` edges the pinned set
**declares** for them. Separable from US1–US3 per plan.md, and shippable on its
own. No story label: these serve US1's components, but no US1 acceptance scenario
mentions them.

**The mechanism already exists.** `PackageSetEntry::runtime_relations` (m985) is
`libraryHaskellDepends + executableHaskellDepends`, merged and deduplicated —
already parsed, never consulted for closure members. The 65 resolved components
get their 245 out-edges from this same field through the conversion at
`scan_cmd.rs:4598`–`4660`.

### Tests for the edges (write first, watch fail)

- [ ] T035 [P] Add the C11 test to `waybill-cli/tests/nix_closure_hackage_identity.rs`: a reclassified member carries out-edges from the set's declared `runtime_relations`, and **every edge target is a component present in the document**. Measured: 278 declared, 265 resolving by name (SC-005a; today 0 out-edges from any closure-appended component).
- [ ] T036 [P] Add the C12 test to `waybill-cli/tests/nix_closure_hackage_identity.rs`: **no** `dependsOn` edge in the document is derived from a store reference. Milestone 1066's decision stands — store references drive a component's *role* and nothing else, and the two facts travel in two carriers so neither misrepresents the other (FR-012a, SC-005b).
- [ ] T037 [P] Add the C13 test to `waybill-cli/tests/nix_closure_hackage_identity.rs`: a member that was **not** reclassified gains no edges. Edges follow the declaration; a member the set does not identify has no declaration to follow, and inventing one from its store references is what C12 forbids (FR-012b).
- [ ] T038 [P] Add the C14 test to `waybill-cli/tests/nix_closure_hackage_identity.rs`: where the set declares a dependency the closure does not contain, the target is emitted as a **versionless compiler-supplied component**, not dangled and not dropped. Measured 13 of 278, all three distinct targets being GHC boot libraries (`directory` ×11, `ghc-heap`, `pretty`) — the population of #1340 and #966 (FR-012c).
- [ ] T039 [P] Add the C15 test to `waybill-cli/tests/nix_closure_hackage_identity.rs`, asserted **directly**: no relationship in the document refers to a PURL this pass replaced. Do not infer it from an edge count — #980's failure mode is silent in all three formats, so a count can match while the graph is orphaned.
- [ ] T040 Run T035–T039 and record that each fails, and how, in `findings.md`.

### Implementation for the edges

- [ ] T041 Resolve edge targets **by name** in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/closure_identity.rs` against the document, never by the set's version. This is load-bearing and measured: 165 of 278 targets are present at a version the set does not declare, because the closure carries what GHC actually shipped, so binding to the set's version would point **59%** of edges at components that do not exist (research R6, data-model §5).
- [ ] T042 Emit a dangling target as a versionless compiler-supplied component, reusing `haskell_packages/boot_libraries.rs`'s identification (`is_boot_library`, `union_nulled`) rather than a new list, so the boot set stays derived from the revision's own configuration. **The versionless component needs an accepted reason annotation or it fails the `versionless_explained` invariant** (`waybill-cli/tests/corpus_harness_195/invariants/checks/versionless_explained.rs:79`): C170's `compiler-supplied` is exactly that value and already exists, which is the carrier to use here — settle it against the invariant's accepted set, not from memory.
- [ ] T043 Dedupe the new edges against those already present, as the existing conversion does. CycloneDX hides duplicates (`dependsOn` is a set per component) and SPDX does not — the measured cost of omitting this guard was 225 duplicate rows and a CDX-vs-SPDX edge-count gap that grew from 1 to 232 (`scan_cmd.rs:4600`–`4618`). Give each edge the provenance that says where it came from (Principle X).
- [ ] T044 Verify T035–T039 pass, then run mutation M7 (bind edge targets to the set's version instead of resolving by name) and M8 (apply the rename map after edge construction rather than in the same step) and confirm both are CAUGHT. M8 is the #980 mutant and the one most likely to survive, because the document it produces validates. Record in `findings.md`.
- [ ] T045 Run the cross-format root-edge parity check over the fixture emissions written by `waybill-cli/tests/nix_closure_hackage_identity.rs` (`reference_cross_format_root_edge_parity`): compare root out-edge counts across CDX, SPDX 2.3 and SPDX 3 on the fixture. It is the cheapest way to catch a fabricated or orphaned edge and found two defects in m866.

**Checkpoint**: reclassified components carry a smaller, honest graph. It will be far smaller than sbomnix's 8,230 store-reference edges, on purpose and as stated in the spec's Assumptions.

---

## Phase 7: Polish & cross-cutting

- [ ] T046 [P] Assert C1 — `waybill sbom scan --help` is byte-identical to the parent commit. No new flag, no changed default (FR-010).
- [ ] T047 [P] Assert C17 — `cargo test -p waybill --test nix_config_closure` (`waybill-cli/tests/nix_config_closure.rs`) stays **14/14**, and the closure's derivation count, membership and role classification are unchanged. This feature changes how a member is named and described, never which members exist (FR-011, SC-006).
- [ ] T048 [P] Assert C18/SC-007 — every other ecosystem's output is byte-identical, recording the comparison in `specs/1275-closure-hackage-identity/measurements/findings.md`. Diff a normalized, sorted emission on a non-Nix fixture against the parent commit, masking content-addressed IDs and timestamps (`feedback_verify_golden_churn_normalized`).
- [ ] T049 [P] Assert C19/FR-015 against `waybill-cli/tests/nix_haskell_resolution_m926.rs` and `waybill-cli/tests/haskell_nixpkgs_license_us3.rs` — the 65 already-resolved `pkg:hackage` components are unchanged, and `cargo test -p waybill --test nix_haskell_resolution_m926` and `--test haskell_nixpkgs_license_us3` both stay green. They are produced by a pass this feature does not touch, and that is a claim to check.
- [ ] T050 Run the full mutation set M1–M8 in one pass through `measurements/mutate.py` and record every verdict in `measurements/mutations.md`: which mutant, which test caught it, and **every survivor with its explanation**. A survivor is a finding about the test or about the mutation — m1274's M8 survived because the mutation itself was invalid, and replacing it with a valid one was the fix (SC-008, C20).
- [ ] T051 Re-run every committed probe — `size-closure-members.py`, `signal-agreement.py` — and confirm each reproduces its recorded finding (SC-009, C21).
- [ ] T052 [P] Update `docs/ecosystems.md`'s nix section and `docs/reference/sbom-format-mapping.md` for any row T006 or T042 added or extended, keeping the extractor in `waybill-cli/src/parity/extractors/mod.rs` in step. A doc row without an extractor fails the catalog gate.
- [ ] T053 Correct plan.md's Constitution Check Principle V row with T006's finding, so the artifact that outlives the milestone does not keep asserting that C151 was reusable here.
- [ ] T054 Run the pre-PR gate on the default feature set, with **no** `+toolchain` argument (#1189): `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Enumerate the per-target `test result: ok. N passed; 0 failed` lines and the `>>> all pre-PR checks passed.` line. Assert on that positive signal — the absence of the word FAILED is not evidence, since a build that never compiled also contains no failures. Never run `cargo fmt`.

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (Setup)**: no dependencies.
- **Phase 2 (Foundational)**: depends on Phase 1. **Blocks every story.** T005 and T006 can each change the design, which is why they precede US1 rather than accompanying it.
- **Phase 3 (US1)**: depends on Phase 2 complete.
- **Phase 4 (US2)**: depends on Phase 3 — it gates the same code path US1 writes. Ships with US1 in one PR.
- **Phase 5 (US3)**: depends on Phase 2; independent of Phases 3–4 in principle, since reasons attach to members US1 leaves alone. In practice it reads T007's enum, so it follows US1's pass existing.
- **Phase 6 (Edges)**: depends on Phase 3 — edges attach to reclassified components. Independent of Phases 4–5 and separately shippable.
- **Phase 7 (Polish)**: depends on every phase shipped.

### Within Phase 2

T004 and T005 are sequential (T005 measures what T004's loader costs when called twice). T006 is independent of both. T007 precedes T008. T009 is independent. T010 depends on nothing but informs every test phase.

### Within each story

Tests first, watched to fail (T015, T024, T031, T040 are the watch-it-fail gates), then implementation, then the mutants.

### Parallel opportunities

- T002, T003 together.
- T006, T009, T010 alongside T004/T005 — different files.
- T011–T014 together; T020–T023 together; T028–T030 together; T035–T039 together.
- T046–T049 and T052 together.

---

## Implementation Strategy

### MVP

Phase 1 → Phase 2 → Phase 3 → **Phase 4**, as one PR. US1 alone is not an MVP:
the same code path that gives `aeson 2.2.4.1` its identity gives GNU
`bash 5.3p9` the coordinate of an unrelated Haskell library. 26 of 276 measured
components are in that population, so collision safety is not a hardening pass
to follow — it is part of the first shippable increment.

### Incremental delivery after the MVP

1. Phase 5 (US3) — transparency for the 217 that stay generic.
2. Phase 6 (Edges) — the declared graph.

Each adds value without changing what the previous one emitted.

---

## Notes

- `[P]` means different files and no dependency on an incomplete task.
- Commit after each phase. `Refs #1282`, never `Closes` — and no qualifier like "part 1" beside an issue number, which is the signal that the keyword is wrong rather than a way to make it right (`feedback_partial_fix_closes_issue`).
- Commits end with `Assisted-by: AGENT_NAME:MODEL_VERSION`. No `Co-Authored-By:`, no session URL in any public artifact.
- Never generate corpus goldens locally; refresh goes through the `public-corpus.yml` workflow dispatch. The 109 corpus tests are `#[ignore]`d locally, so a green local gate says nothing about them (`feedback_green_local_gate_excludes_corpus_goldens`).
- Every number quoted in this file traces to `research.md` or a measured byte count, and the two that do not yet — T005's parse cost and T006's carrier decision — are tasks precisely because they are unmeasured.
