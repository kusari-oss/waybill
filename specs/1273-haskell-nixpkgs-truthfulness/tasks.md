# Tasks: Haskell/nixpkgs resolution correctness

**Input**: Design documents from `/specs/1273-haskell-nixpkgs-truthfulness/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/README.md, quickstart.md

**Tests**: REQUIRED, not optional. SC-006 demands every requirement be covered
by a test that fails when the behaviour is removed, **mutation-tested rather
than assumed** — on this milestone's own evidence two of the four defects are
cases where the existing tests passed throughout. Mutation steps are therefore
**per requirement**, not per story.

**Organization**: grouped by user story. The stories are *not* fully
independent and the plan says so rather than letting it be discovered —
see Dependencies below.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on incomplete work)
- **[Story]**: US1–US4 from spec.md
- Exact file paths in every description

## Path Conventions

Single Rust workspace. Sources under `waybill-cli/src/`, integration tests
under `waybill-cli/tests/`, fixtures under `waybill-cli/tests/fixtures/`.

---

## Phase 1: Setup

**Purpose**: the module split the plan chose, and the fixture tree.

- [X] T001 Create `waybill-cli/src/scan_fs/package_db/haskell/hpack.rs` as an empty module with its module doc comment, and declare it from `waybill-cli/src/scan_fs/package_db/haskell.rs` with `mod hpack;` — the parser lands in a sibling module because `haskell.rs` is already 3302 lines, following the `nix/haskell_packages/` precedent
- [X] T002 [P] Create the fixture tree `waybill-cli/tests/fixtures/haskell_hpack/` with a `README.md` recording that every package name is synthetic (`waybill-fixture-*`) per the project's fixture rule, and that real coordinates appear only in `specs/1273-*/measurements/` probes

---

## Phase 2: Foundational (Blocking Prerequisites)

**⚠️ CRITICAL**: T003 and T004 block all four stories. T003 is the baseline
every FR-016 fixture correction is justified against; T004 settles a question
whose two answers need different code.

- [X] T003 Capture the pre-change baseline: run all five probes in `specs/1273-haskell-nixpkgs-truthfulness/measurements/` and save their output to `specs/1273-haskell-nixpkgs-truthfulness/measurements/baseline-0.14.0.txt`, committed. FR-016 requires every fixture correction be justified "against a measurement rather than against this spec", and a baseline captured after the change has already lost the comparison
- [X] T004 Settle the `!include` question (FR-001b) with a unit test in `waybill-cli/src/scan_fs/package_db/haskell/hpack.rs`: feed `serde_yaml` a document containing `dependencies: !include "common.yaml"` and assert which happens — a deserialization error, or a tagged value. `data-model.md` §1.1 labels this as **inference, not measured**; detection is a pre-parse text scan if it errors and a value read if it does not, and the two need different code. Record the answer in `data-model.md` §1.1, replacing the inference note
- [X] T005 [P] Add `specs/1273-haskell-nixpkgs-truthfulness/measurements/run-all.sh` invoking all five probes with a non-zero exit if any fails, so SC-008 ("every probe re-runs and reproduces its recorded finding") is one command rather than five

---

## Phase 3: User Story 1 — a project the reader cannot read says so (P1)

**Goal**: an hpack-only project emits its declared dependencies, and a
manifest that cannot be read is announced instead of silently yielding a
complete-looking empty document.

**Independent test**: scan a project containing only `package.yaml` and
assert the emitted components match its declared dependencies (C2), and that
the document does not claim completeness over an unread manifest (C8).

**Why P1**: the only one of the four that produces a *confidently wrong*
document rather than a degraded one.

### Tests first (each must fail before its implementation task)

**Fixtures land first (C1).** A test missing its fixture fails for the wrong
reason, and a vacuous failure is indistinguishable from a behavioural one.
Each test below must be observed failing on its **assertion**, not on a
missing path.

- [X] T006 [P] [US1] Create every fixture T007–T018 needs under `waybill-cli/tests/fixtures/haskell_hpack/`, synthetic names only (`waybill-fixture-*`): `manifest_only/`, `inheritance/`, `repeated_name/`, `declared_range/`, `when_flag_off/`, `cabal_flag_off/`, `when_literal_false/`, `manifest_and_generated_cabal/`, `manifest_and_handwritten_cabal/`, `malformed/`, `defaults/`, `include/`
- [X] T007 [P] [US1] Write `waybill-cli/tests/haskell_hpack_us1.rs` with a failing test `m1273_us1_hpack_only_emits_declared_dependencies` (FR-001, C2, SC-001): fixture `package.yaml` declaring 3 dependencies and no `*.cabal`, asserting 3 `pkg:hackage/` components. Measured today: 0
- [X] T008 [P] [US1] Add failing `m1273_us1_toplevel_dependencies_reach_every_stanza_kind` (FR-001a, C3) to `waybill-cli/tests/haskell_hpack_us1.rs`: top-level `dependencies` must reach `library`, `executables`, `tests` **and** `benchmarks`, with test/benchmark-only names carrying `Development` and a name in both carrying `Runtime` — the measured inheritance rule (Probe A1) plus the existing `merge_scope` precedence
- [X] T009 [P] [US1] Add failing `m1273_us1_repeated_name_keeps_last_constraint_only` (FR-001a, C4) to `waybill-cli/tests/haskell_hpack_us1.rs`, including the **negative** half: `text >= 1.0` plus `text < 3` yields `< 3` and the document MUST NOT carry an intersected range, which is a constraint the project does not declare (Probe A3)
- [X] T010 [P] [US1] Add failing `m1273_us1_range_recorded_as_declared_not_as_hpack_renders` (FR-001a, C5) to `waybill-cli/tests/haskell_hpack_us1.rs`: a manifest declaring `base >= 4 && < 5` emits that range, not hpack's `==4.*` normalisation (Probe A4)
- [X] T011 [P] [US1] Add failing `m1273_us1_when_block_agrees_with_cabal_conditional` (FR-001a, C6) to `waybill-cli/tests/haskell_hpack_us1.rs`: **two fixtures, one graph** — a dependency declared by a default-off `when: flag(x)` block in `package.yaml`, and the same dependency declared by a `buildable: False` stanza in a `*.cabal` — asserting identical `lifecycle-scope` AND identical `waybill:optional-derivation`. This is the agreement contract; assert both documents in one test so a disagreement names both
- [X] T012 [P] [US1] Add failing `m1273_us1_literal_false_when_contributes_nothing` (FR-001a, C6) to `waybill-cli/tests/haskell_hpack_us1.rs`: hpack prunes a literal `false` (Probe A5), so its dependencies are absent entirely rather than optional
- [X] T013 [P] [US1] Add failing `m1273_us1_manifest_wins_over_generated_cabal` (FR-002, C7) to `waybill-cli/tests/haskell_hpack_us1.rs`: a `package.yaml` beside an hpack-generated `*.cabal` emits each component **once**, from the manifest, and the document records which file was read
- [X] T014 [P] [US1] Add failing `m1273_us1_handwritten_cabal_is_not_suppressed` (FR-002, C7) to `waybill-cli/tests/haskell_hpack_us1.rs`: a `*.cabal` **without** the hpack generation header beside a `package.yaml` is NOT suppressed — it is not hpack output, so the manifest is not its source
- [X] T015 [P] [US1] Add failing `m1273_us1_unparseable_manifest_is_announced` (FR-003, C8) to `waybill-cli/tests/haskell_hpack_us1.rs`: a malformed `package.yaml` produces a document naming the file and the reason, asserted **on the document** and not on captured logs (SC-009)
- [X] T016 [P] [US1] Add failing `m1273_us1_unread_manifest_defeats_completeness` (FR-005, C8, SC-002) to `waybill-cli/tests/haskell_hpack_us1.rs`: `waybill:graph-completeness` is not `complete` when a manifest was dispatched and not read. Measured today: `complete` over 0 components
- [X] T017 [P] [US1] Add failing `m1273_us1_unmodelled_construct_is_announced` (C9, FR-001b, SC-010) to `waybill-cli/tests/haskell_hpack_us1.rs`: a `defaults:` manifest and an `!include` manifest each yield a document naming the construct and the file
- [X] T018 [P] [US1] Add failing `m1273_us1_defaults_performs_no_network_request` (C10, FR-001c, SC-010) to `waybill-cli/tests/haskell_hpack_us1.rs`: the `defaults:` scan runs under `--offline` **and** is asserted on the document. Both halves are needed — a pass that silently skipped itself offline would satisfy the `--offline` run alone
### Implementation

- [X] T019 [US1] Implement the `serde_yaml` wire types in `waybill-cli/src/scan_fs/package_db/haskell/hpack.rs` per `data-model.md` §1.1 — `HpackManifest`, `HpackFlag`, `HpackComponent`, `HpackWhen`, and the `OneOrMany<T>` helper, since `source_dirs`, `dependencies` and `when` each accept a scalar or a sequence in real manifests
- [X] T020 [US1] Implement `UnreadableReason` and `UnmodelledConstruct` in `waybill-cli/src/scan_fs/package_db/haskell/hpack.rs` per `data-model.md` §1.3 as enums (Principle IV), serialized only at the emission boundary. A `when:` whose condition cannot be evaluated is deliberately NOT a variant — Probe A5 measured that hpack does not evaluate those either, so it is a conditional stanza and a complete answer, not a gap
- [X] T021 [US1] Implement `lower_to_cabal_manifest` in `waybill-cli/src/scan_fs/package_db/haskell/hpack.rs`: the ordered merge (top-level list, then per-component, **last-wins per name replacing the constraint**), one `CabalStanza` per component with the kind that gives it the right `stanza_lifecycle_scope`, and each `when:` block lowered into the representation `stanza_default_buildable` already evaluates. Do NOT add a second conditional evaluator — Probe A5 is why
- [X] T022 [US1] Add the `origin: ManifestOrigin` field to `CabalManifest` in `waybill-cli/src/scan_fs/package_db/haskell.rs` per `data-model.md` §2.1, so FR-002's choice is recordable and a suppressed `*.cabal` is distinguishable from one that was read
- [X] T023 [US1] Implement `HpackReadOutcome` per `data-model.md` §1.2 and fold it into the **existing** `HaskellParseSummary` in `waybill-cli/src/scan_fs/package_db/haskell.rs` rather than a new side channel — that struct already carries exactly this kind of reader-outcome fact ("whether a `.cabal` file was read at all", milestone 895 FR-012a/b) and already reaches the document through `generate::mod::haskell_parse_summary`
- [X] T024 [US1] Wire the lowering into `finalize` in `waybill-cli/src/scan_fs/package_db/haskell.rs`: parse each `package_yaml_paths` entry, lower it, and merge the resulting manifests into `cabal_manifests` before the emit loop, so every downstream pass applies unchanged
- [X] T025 [US1] Implement FR-002 suppression in `finalize`: drop a `*.cabal` whose `hpack_generated` is true when a `package.yaml` sits in the same directory, recording it in `HpackReadOutcome.suppressed_cabal`. Key on `hpack_generated`, which already exists and is already tested by `parse_cabal_manifest_detects_hpack_header`
- [X] T026 [US1] Add the `ManifestUnread { ecosystem, count }` variant to `ReasonCode` in `waybill-cli/src/generate/graph_completeness/reason_codes.rs` with its `as_str` rendering, per `data-model.md` §2.5. This adds a reason code and **not** a catalog row — the variant renders through the existing `waybill:graph-completeness` value, so no parity extractor is needed (C24)
- [X] T027 [US1] Thread the unread-manifest fact into `compute_graph_completeness` in `waybill-cli/src/generate/graph_completeness/mod.rs` and update its three production call sites — `generate/orphan_reason.rs:601`, `generate/spdx/document.rs:699`, `generate/spdx/v3_document.rs:677` — plus the in-module unit-test constructors. The fact must be *carried* from the reader: `trivially_complete()` is correct for a dependency-free project and wrong for an unread manifest, and the two are indistinguishable at that point in the code
- [X] T028 [US1] Wire `ManifestUnread` into `degraded_ecosystems` in `waybill-cli/src/generate/cyclonedx/compositions.rs`. **That function's match has a `_ => {}` arm**, so a new variant is silently dropped and CDX's native `compositions[]` would say `complete` while `waybill:graph-completeness` did not — which is exactly the disagreement the function's own doc comment exists to prevent. An enum allowlist that falls out of date is a known failure in this repo
- [X] T029 [US1] Rewrite the warning text in `emit_hpack_warnings` (`waybill-cli/src/scan_fs/package_db/haskell.rs:2439`) per FR-004a and C21: it currently advises "run 'hpack' to regenerate before scanning if package.yaml has been edited", which under FR-002 cannot change the document. Leaving it would instruct an operator to perform a step that cannot affect the output, which is worse than silence because it looks like a remedy
- [X] T030 [US1] Verify FR-004/C22 holds unchanged: `emit_hpack_warnings` iterates `cabal_manifests`, so the disagreement warning still fires for a manifest **with** a generated sibling. Add `m1273_us1_disagreement_warning_still_fires` to `waybill-cli/tests/haskell_hpack_us1.rs` — FR-001's gap and this warning are separate code paths and fixing one does not fix the other

**Checkpoint**: an hpack-only project emits its dependencies and no document
claims completeness over an unread manifest. US1 is independently shippable.

---

## Phase 4: User Story 2 — an optional mark is withdrawn only on evidence (P2)

**Goal**: a mark derived from a `buildable: False` stanza survives unless a
component outside the marked set genuinely reaches it.

**Independent test**: scan a default-off-flag project twice, with and without
a `flake.lock`, and assert the classification is identical unless a non-seed
closure path reaches the component.

**Why P2**: it silently undoes a shipped fix (#955 / PR #1030) whenever a
`flake.lock` is present, which for a Nix-based Haskell project is always.

### Tests first

**Cache isolation is mandatory (D1).** These tests MUST NOT read the host's
`~/.cache/waybill/nixpkgs/`. The pattern already exists: `seed_cache()` at
`waybill-cli/tests/nix_haskell_resolution_m926.rs:116` writes a **synthetic**
`hackage-packages.nix` (its `PACKAGES` const, all `waybill-fixture-*` names)
into a `tempfile::tempdir()` and points `WAYBILL_NIXPKGS_CACHE` at it — and
for the negative case deliberately at `__no_cache__`, because "an unset cache
would fall back to the real `$HOME`". Reuse it. A test that reads the real
cache passes on a developer's machine and fails on CI, and Principle VII
forbids it.

That file's header states the other half of the contract: no test there
touches the network, and none is `#[ignore]`d, because "an ignored test is one
nobody runs, and this project has already been bitten by a suite that was
green because it was not looking (#949)".

- [X] T031 [P] [US2] Create the fixtures T032–T037 need under `waybill-cli/tests/fixtures/haskell_hpack/` (synthetic names), each with a `flake.lock` pinning the **test's own** `REV` constant — not a revision from the developer's cache. Extend the synthetic package set so the marked names and a three-level fan-out below them exist as `waybill-fixture-*` attributes with `libraryHaskellDepends` relations
- [X] T032 [P] [US2] Write `waybill-cli/tests/haskell_optional_closure_us2.rs` with failing `m1273_us2_mark_survives_when_only_the_seed_reaches_it` (FR-006, C11, SC-003), seeding the cache per the note above: the `buildable: False` fixture keeps `lifecycle-scope=optional` for both flag-gated dependencies. Measured today: both are restored to runtime
- [X] T033 [P] [US2] Add failing `m1273_us2_mark_is_withdrawn_on_genuine_non_seed_reach` (FR-006, C11) to `waybill-cli/tests/haskell_optional_closure_us2.rs`: a dependency declared by a disabled stanza AND genuinely reachable below a dependency that IS built loses its mark — the positive control, without which the fix could be "never withdraw anything"
- [X] T034 [P] [US2] Add failing `m1273_us2_withdrawal_names_its_justification` (C12, FR-007) to `waybill-cli/tests/haskell_optional_closure_us2.rs`: the withdrawal records the non-seed parent whose edge reached the component, in the document
- [X] T035 [P] [US2] Add failing `m1273_us2_ran_and_withdrew_nothing_differs_from_did_not_run` (C13, FR-008) to `waybill-cli/tests/haskell_optional_closure_us2.rs`: assert all **three** states are distinguishable in the document. Use the `__no_cache__` seed for the "did not run" case, which is what makes that state reachable without touching the host cache. These are one state today, which is the condition under which a feature that cannot fire looks like a feature with nothing to do
- [X] T036 [P] [US2] Add failing `m1273_us2_declined_withdrawal_keeps_scope_and_derivation` (C14, FR-009) to `waybill-cli/tests/haskell_optional_closure_us2.rs`: a declined withdrawal keeps `lifecycle-scope=optional` AND `waybill:optional-derivation=cabal-flag-disabled-stanza` — restoring one without the other publishes a scope with no account of where it came from
- [X] T037 [P] [US2] Add failing `m1273_us2_fanout_below_a_marked_component_is_not_published_as_runtime` (FR-006) to `waybill-cli/tests/haskell_optional_closure_us2.rs`: the synthetic three-level graph where the marked component has a child reachable only through it. Probe D measured 8 such components on the public library; the error fans out, so a test that only checks the directly-marked pair would pass with the defect intact
### Implementation

- [X] T038 [US2] Add `OptionalReconciliation` to `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs` per `data-model.md` §2.4 — `reviewed`, `withdrawn: BTreeMap<String, String>`, `declined` — and hang it off `EnrichmentSummary` as `Option<OptionalReconciliation>`, where `None` means the reconciliation did not run. `BTreeMap` not `HashMap`: the value reaches the document and a golden has to pin the string
- [X] T039 [US2] Rewrite `reconcile_optional_with_closure` in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs:623` per `data-model.md` §3: collect the marked names, re-seed with `declared \ marked`, walk, and withdraw only for a component the second closure reaches. Delete the `flat_map(|e| [e.from, e.to])` membership test — a marked component is a seed, so it appears as `e.from` of its own out-edges and membership is true by construction
- [X] T040 [US2] Extend `closure::walk` usage (not its signature) in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/closure.rs` so the second walk reuses the same `RelationLookup`. One extra BFS over an already-parsed in-memory package set: no new I/O, no fetch. Keep `reached_from` populated so T034's justification has a source
- [X] T041 [US2] Replace the `restored > 0` log at `waybill-cli/src/cli/scan_cmd.rs:4668` with one that reports all three states, and pass `OptionalReconciliation` into the document-scope record so FR-018 is satisfied in the artifact and FR-020 in the log. Milestone 973 exists because milestone 926 computed exactly this kind of record, logged it, and dropped it
- [X] T042 [US2] Assert FR-015/C25 explicitly: confirm the re-seeded closure removes the marked names and **changes nothing else about the seed**. Milestone 1272 measured that folding an extra section into a runtime seed silently reassigns a component from Optional to Development; run the Haskell dev-scope tests (`stanza_lifecycle_scope`'s `TestSuite`/`Benchmark` → `Development`) unchanged and record the result

**Checkpoint**: the mark survives unless something outside the marked set
reaches the component, and the document says which happened.

---

## Phase 5: User Story 3 — a resolved component carries its declared license (P3)

**Goal**: components resolved from the cached package set carry the license
that set declares, canonicalised where possible and preserved where not.

**Independent test**: scan a project resolved against a pinned nixpkgs and
assert components resolved from the cached set carry a license.

**Why P3**: a pure gain with a known source — the data is already on disk in
a file this pass opens. Fully independent of US1/US2/US4.

### Tests first

**Cache isolation is mandatory (D1)**, exactly as for US2: seed a `tempfile::tempdir()`
via the `seed_cache()` pattern at
`waybill-cli/tests/nix_haskell_resolution_m926.rs:116` and point
`WAYBILL_NIXPKGS_CACHE` at it. Its `PACKAGES` const currently carries **no
`license =` lines at all**, so US3's first job is to add them — which is also
what makes these tests exercise the new field rather than the host's copy of
nixpkgs.

- [ ] T043 [P] [US3] Extend the synthetic `PACKAGES` const used by `seed_cache` with `license =` lines covering all three measured families (Probe B) — `lib.licenses.bsd3`, `lib.meta.getLicenseFromSpdxId "MIT"`, and the bare strings `"(BSD-2-Clause OR Apache-2.0)"`, `"GPL"`, `"publicDomain"` and `"unknown"` — on `waybill-fixture-*` attributes. Use the exact forms the probe found, not invented ones
- [ ] T044 [P] [US3] Write `waybill-cli/tests/haskell_nixpkgs_license_us3.rs` with failing `m1273_us3_resolved_component_carries_declared_license` (FR-010, C15, SC-004), seeding the cache per the note above: components resolved from the seeded set carry a license. Measured today: 0 of 66
- [ ] T045 [P] [US3] Add failing unit tests to `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/package_set.rs` covering the same three families as inline `&str` literals — a unit test over a literal needs no cache at all, which is why the parser's own coverage belongs here rather than in an integration test
- [ ] T046 [P] [US3] Add failing `m1273_us3_uncanonicalisable_license_is_preserved_and_flagged` (C16, FR-011) to `waybill-cli/tests/haskell_nixpkgs_license_us3.rs`: `"GPL"` appears as declared with the uncanonicalised fact recorded, not presented as a canonical expression. Measured population: 586 unversioned strings plus `publicDomain` (313)
- [ ] T047 [P] [US3] Add failing `m1273_us3_unknown_yields_no_license` (C17, FR-011a) to `waybill-cli/tests/haskell_nixpkgs_license_us3.rs`: a package whose declaration is `"unknown"` (360 of 994) emits an **absent** license field, never the string `unknown` in `licenses[]`. Publishing it would be worse than absence — a consumer filtering for components with no license information would not find it
- [ ] T048 [P] [US3] Add failing `m1273_us3_unresolved_component_gets_no_license` (C18, FR-012) to `waybill-cli/tests/haskell_nixpkgs_license_us3.rs`: a component the resolution did not resolve carries no license from this pass
- [ ] T049 [P] [US3] Add failing `m1273_us3_license_is_emitted_in_all_three_formats` (FR-010) to `waybill-cli/tests/haskell_nixpkgs_license_us3.rs`: the **native** fields — CDX `component.licenses[]`, SPDX 2.3 `PackageLicenseDeclared`, SPDX 3 `simplelicensing_LicenseExpression` — per the Principle V audit. No `waybill:*` key carries the license itself

### Implementation

- [ ] T050 [US3] Add `DeclaredLicense` to `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/package_set.rs` per `data-model.md` §2.3 — `Canonical(SpdxExpression)` and `Uncanonical(String)` — so FR-011's "uncanonicalised" is a type state rather than a convention
- [ ] T051 [US3] Add `license: Option<DeclaredLicense>` to `PackageSetEntry` (FR-010) and a `license_re` beside the existing `version_re` / `sha256_re` in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/package_set.rs`. A line-oriented read is sound because Probe B **asserted** zero multi-line forms, zero list forms and zero `with lib.licenses;` blocks. That assertion stays in `measurements/license-shapes.sh`, which already makes it against the real file — it must NOT become a unit test reading `~/.cache/waybill/nixpkgs/` (D1)
- [ ] T052 [US3] Implement the three-family classifier in `package_set.rs`: a 15-entry `lib.licenses.<attr>` → SPDX-id table (the whole measured vocabulary at the pinned revision), pass-through for the `getLicenseFromSpdxId` family, and `SpdxExpression::try_canonical` for bare strings. `publicDomain` has no clean SPDX id and is `Uncanonical`; `"unknown"` is `None` per FR-011a
- [ ] T053 [US3] Attach the license at the point the nixpkgs pass assigns a version in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs`, so FR-012 holds structurally: a component the pass did not resolve is never reached by the attaching code
- [ ] T054 [US3] Confirm the per-revision cache's on-disk format is unchanged — this parses one more field out of a file milestone 926 already downloads. Assert no change to `cache.rs` or `fetch.rs` in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/`

**Checkpoint**: resolved components carry licenses in all three formats, via
native fields.

---

## Phase 6: User Story 4 — unresolved-reason agrees with the document (P4)

**Goal**: no component is both version-resolved and marked unresolved.

**Independent test**: scan the resolved fixture and assert no component
carries both a resolved version and `waybill:unresolved-reason`.

**⚠️ Depends on US2.** The 2 contradicting components are the same 2 US2 is
about, so part of US4 resolves as a consequence. T057 **must** run after
Phase 4.

### Tests first

- [ ] T055 [P] [US4] Write `waybill-cli/tests/haskell_unresolved_reason_us4.rs` with failing `m1273_us4_resolved_and_unresolved_are_mutually_exclusive` (C19, SC-005): no component carries both a resolved version and `waybill:unresolved-reason`. Measured today: 2
- [ ] T056 [P] [US4] Add failing `m1273_us4_unresolved_component_distinguishes_compiler_supplied` (C20, FR-014) to `waybill-cli/tests/haskell_unresolved_reason_us4.rs`: a compiler-supplied dependency's reason says so rather than "could not be determined". `pretty` at the measured revision is the live case — nixpkgs binds it to `null` in `configuration-ghc-9.10.x.nix`, so reporting it as undeterminable tells an operator to go and find something that does not exist

### Implementation

- [ ] T057 [US4] **After Phase 4 lands**, re-measure US4's remaining population with `specs/1273-haskell-nixpkgs-truthfulness/measurements/reproduce-1273.sh` and record it in `measurements/findings.md` as Probe F. The spec's own assumption requires this be measured after US2 rather than predicted now; design T057 against the measured remainder
- [ ] T058 [US4] Strip `waybill:unresolved-reason` where the nixpkgs pass assigns a version, in the same mutation that writes it, in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs`. The reason is **stale, not wrong** — the reader stamps it correctly when there is no version — so the fix belongs where the fact changes. Do NOT add an emission-time sweep: it would have to decide what to do about every other reader's C151, which FR-015 forbids
- [ ] T059 [US4] Confirm the 62 components carrying no reason are left alone and document why in the test: the pass resolved them, so FR-013 says they must not carry one. C151's "universal across design-tier components" specification is satisfied by the components that remain unresolved, and FR-014 is about those
- [ ] T060 [US4] Verify the C151 reason strings in `waybill-cli/src/scan_fs/package_db/haskell.rs:2217` are **unchanged** — FR-004a changes the hpack warning, not these. #956 chose them per toolchain and that reasoning is untouched

**Checkpoint**: the document no longer contradicts itself.

---

## Phase 7: Polish & Cross-Cutting

### Mutation testing — per requirement, not per story (SC-006)

Each task below removes one behaviour and confirms a **named** test fails.
"The suite is green" is not evidence here: two of the four defects are cases
where the existing tests passed throughout, and a passing test may exercise
nothing.

- [X] T061 Mutate FR-001/FR-002/FR-003/FR-004/FR-004a (US1 reading, suppression and the two warnings) one at a time and record which named test fails for each, in `specs/1273-haskell-nixpkgs-truthfulness/measurements/mutations.md` (SC-006). FR-004 is included because the disagreement warning is a separate code path from FR-001's gap — `emit_hpack_warnings` iterates `cabal_manifests`, so a change that fixed the gap and broke the warning would leave both tests green unless the warning is mutated on its own
- [X] T062 [P] Mutate FR-001a's three measured rules separately — inheritance reach, last-wins merge, `when:` lowering — and record the failing test per rule. The merge mutant that matters most is "intersect instead of replace", because it is the plausible wrong implementation
- [X] T063 [P] Mutate FR-005 by restoring `trivially_complete()` for an empty set and confirm `m1273_us1_unread_manifest_defeats_completeness` fails. Separately mutate T028's `degraded_ecosystems` wiring back to the `_ => {}` arm and confirm a test catches the CDX/annotation disagreement — if none does, that is a missing test, not a passing mutant
- [X] T064 [P] Mutate FR-006 back to the `flat_map(|e| [e.from, e.to])` form and confirm both `m1273_us2_mark_survives_when_only_the_seed_reaches_it` **and** `m1273_us2_fanout_below_a_marked_component_is_not_published_as_runtime` fail. Then mutate to the `e.to`-only and parent-local forms `research.md` R3 rejected and confirm each is still caught — a fix that passes only the shallow test is the one to catch here
- [X] T065 [P] Mutate FR-007/FR-008/FR-009 one at a time and record the failing test per requirement
- [ ] T066 [P] Mutate FR-010/FR-011/FR-011a/FR-012 one at a time. The FR-011a mutant — emitting `"unknown"` as a license — is the one the Constitution gate caught in design, so it must be caught by a test too
- [ ] T067 [P] Mutate FR-013/FR-014 one at a time and record the failing test per requirement
- [ ] T068 [P] Mutate the announcement and transparency requirements one at a time, recording the failing test per requirement: **FR-001b** (drop the unmodelled-construct announcement), **FR-001c** (make `defaults:` fetch, which must fail under `--offline`), **FR-015** (fold `devDependencies`-equivalent names back into the runtime seed, the shape milestone 1272 measured as silently reassigning Optional → Development), **FR-018** and **FR-020** (move one signal to log-only, and to document-only). FR-018's mutant matters most: a log-only signal leaves the artifact exactly as dishonest as the defect being fixed, which is the milestone's whole premise
- [ ] T069 Record the deliberate mutation **exemptions** in `measurements/mutations.md`, with the reason for each, so the claim "a mutation step per requirement" is either true or visibly qualified: **FR-016** (a process requirement about how a fixture correction is justified — it has no runtime behaviour to remove), **FR-019** (enforced by the pre-existing `every_catalog_row_has_an_extractor` and `holistic_parity` tests, which already fail on the mutation this would perform), and **FR-021**/**FR-022** (absence requirements — the mutation is "add a flag" or "read `PATH`", which T080 asserts positively instead). An unstated exemption is indistinguishable from an oversight
- [X] T070 Mutate FR-017 itself (C26): for each diagnostic this milestone adds, confirm a committed fixture triggers it. A diagnostic no input can reach is the defect class US1 and US2 are both instances of, and `emit_hpack_warnings` is a measured case of a warning wired for one branch only

### Parity and non-regression

- [ ] T071 Run the parity suite and confirm **no new catalog row** was added (C24, FR-019). Every signal lands in `waybill:graph-completeness`, `waybill:unresolved-reason` (C151) or the nixpkgs document-scope records (C173/C175), all of which already have extractors. If a row did become necessary, its extractor ships in this same change or `parity::extractors::tests::every_catalog_row_has_an_extractor` and `holistic_parity` fail by design
- [ ] T072 [P] Verify SC-007: the development-axis and optional-axis output of every other ecosystem is byte-identical. Run the npm, pip, cargo, maven and gradle optional/dev suites and record the result — FR-015's blast-radius check, and R7 names milestone 1272's measured near-miss as the live hazard
- [ ] T073 [P] Confirm `--offline` is still honoured end to end (SC-010): no construct in the modelled subset reaches the network, and `defaults:` is announced rather than resolved
- [ ] T074 Correct, do not pin, any committed fixture asserting the old shape (FR-016), justifying each correction against T003's baseline rather than against the spec

### Corpus goldens

- [ ] T075 Refresh the Haskell/Nix corpus goldens through the `public-corpus.yml` **workflow dispatch**. Never generate them locally; `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` is never set locally. These are `#[ignore]`d locally and the workflow never runs on a PR, so this churn surfaces as a red nightly unless driven deliberately
- [ ] T076 Assert the measured expectations from Probe E against the refreshed goldens: `haskell-aeson` **unchanged** (C27 — the control: it has no `flake.lock` and no `package.yaml` at its pin, so no code path here can reach it, and a change means the blast radius is wrong); `haskell-security-advisories`, `haskell-language-server` and `nix-closure-moat` changed. Read the diffs rather than counting files — a directory upload can contribute bytes for a target that never wrote
- [ ] T077 Confirm `nix-closure-moat`'s refreshed golden now reads its root `package.yaml`. It is a live witness to #1278 already under version control, whose pre-change golden records 321 hackage components derived without reading that file — the clearest single demonstration that US1 landed

### Documentation and closeout

- [ ] T078 [P] Update `docs/ecosystems.md` to record that `package.yaml` is read and is the source of truth over an hpack-generated `*.cabal`, and that `!include` / `defaults:` are announced rather than resolved
- [ ] T079 [P] Add a Probe F section to `specs/1273-haskell-nixpkgs-truthfulness/measurements/findings.md` holding T057's post-US2 measurement and T061–T070's mutation results
- [ ] T080 Re-run every probe via `measurements/run-all.sh` and confirm each reproduces its recorded finding (SC-008). The recorded findings are now *post-change*, so update each probe's expected-output note to say which side of the fix it describes
- [ ] T081 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. No `+toolchain` argument (#1189). Assert on the positive signal and enumerate every `test result: ok. N passed; 0 failed` line — the absence of `FAILED` is not evidence, because a build that never compiled also contains no failures
- [ ] T082 Assert the CLI surface is unchanged (C1, FR-021, FR-022): diff `waybill sbom scan --help` against the pre-change binary and confirm it is byte-identical — no flag, no subcommand, no environment variable, no positional argument. Then confirm no `std::process::Command` and no `reqwest` call was added on any path this milestone touches. FR-022 is the stronger half: two scans of one tree on two machines must agree, and the project has already shipped fixtures whose Gradle tier was silently decided by a system `gradle` on CI's runner image
- [ ] T083 Assert C23 as its own check, across every signal this milestone adds: each of C8–C14 and C16–C17 is observable by reading the emitted document **alone**, with no access to the scan log (FR-018, SC-009), **and** is additionally logged (FR-020). Verify the document half by parsing the emitted JSON with the log discarded — asserting on captured logs is what would leave the artifact exactly as dishonest as the defect being fixed

---

## Dependencies

```
Phase 1 (T001–T002)
   ↓
Phase 2 (T003–T005)  ← BLOCKS EVERYTHING; T003 is the baseline, T004 decides code shape
   ↓
   ├─────────────────┬─────────────────┐
   ↓                 ↓                 ↓
US1 (T006–T030)   US2 (T031–T042)   US3 (T043–T054)
   │                 │                 │
   │                 ↓                 │
   │              US4 (T055–T060)      │
   │              T057 needs Phase 4   │
   └─────────────────┴─────────────────┘
                     ↓
         Phase 7 (T061–T083)
```

**Story dependencies, and why they are not all independent:**

- **US1 → US2 (soft).** Probe A5 measured that an hpack `when: flag(x)` block
  with a default-off flag lowers to the same construct a `buildable: False`
  stanza produces, so it acquires an optional mark and then goes through US2's
  closure. T011's two-fixture agreement test is where they meet. Either story
  can land first; the agreement test needs both.
- **US2 → US4 (hard).** The 2 components that are both resolved and
  unresolved-reasoned are the same 2 US2 is about. **T057 must run after
  Phase 4**, per the spec's own assumption that US4's remaining population be
  measured after US2 rather than predicted.
- **US3 ⟂ everything.** The only genuinely independent story: one more field
  out of a file the pass already opens.

**Within-story ordering**: tests before implementation in every phase. Each
test task must be observed failing before its implementation task begins —
otherwise a vacuously-satisfiable test is indistinguishable from a passing one.

## Parallel Execution

**Phase 2**: T005 is independent of T003/T004.

**Across stories after Phase 2**: US1, US2 and US3 can proceed concurrently.
They touch disjoint files — `haskell.rs` + `haskell/hpack.rs` +
`graph_completeness/` for US1, `nix/haskell_packages/mod.rs` +
`closure.rs` for US2, `nix/haskell_packages/package_set.rs` for US3 — with
one collision to sequence: **T053 (US3) and T058 (US4) both edit the
version-assignment site in `nix/haskell_packages/mod.rs`**, and T039 (US2)
edits the same file. Land US2 first, then US3, then US4, or expect a merge.

**Within US1**: T006 lands first (the fixtures), then T007–T018 are all
`[P]` — one new test file, separate functions, no shared state.

**Within US2**: T031 lands first (the fixtures and the seeded package set),
then T032–T037 are all `[P]`.

**Within US3**: T043 lands first (the `license =` lines in the synthetic
package set), then T044–T049 are all `[P]`.

**Phase 7**: T062–T068 are `[P]` (independent mutants); T072, T073, T078,
T079, T082 and T083 are `[P]`. T069–T071, T074–T077, T080 and T081 are
sequential — each depends on the state the previous leaves.

## Implementation Strategy

**MVP = US1 alone.** It is the only one of the four producing a *confidently
wrong* document rather than a degraded one: the other three publish a real
answer with something missing, this one publishes nothing and claims the
nothing is complete. It is also independently shippable — Phases 1–3 plus the
US1 mutation tasks make a coherent PR.

**Suggested delivery, one PR per story**, which is how milestones 1271 and
1272 were landed and what keeps a bisect useful:

| PR | content | why this order |
|---|---|---|
| 1 | Phases 1–2 + US1 + T061–T063, T070 | the MVP; the defect class's worst instance |
| 2 | US2 + T064, T065 | lands before US3/US4 because all three edit `nix/haskell_packages/mod.rs` and US2's change is the largest |
| 3 | US3 + T066 | fully independent; a pure gain |
| 4 | US4 + T057, T067 | last, because T057 cannot be measured until PR 2 is on main |
| 5 | T068–T069, T071–T083 closeout | the corpus refresh needs every reader change on main first |

**Two things to carry into execution:**

1. **Observe each test fail before implementing against it.** T060's
   predecessor in milestone 1272 was vacuously satisfiable as first written,
   and the only reason it was caught is that someone checked what the test
   would do if the feature were absent.
2. **A green suite is not evidence on this milestone.** SC-006 says so
   explicitly, with receipts: two of these four defects shipped and persisted
   under a fully passing test suite.
