---

description: "Task list for 926-nixpkgs-haskell-versions (#947)"
---

# Tasks: Resolve Haskell dependency versions through the pinned nixpkgs

**Input**: Design documents from `/specs/926-nixpkgs-haskell-versions/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Test tasks ARE included. The spec's success criteria are assertions
about emitted documents (SC-002 reason coverage, SC-003 never-invent, SC-004
determinism), and the project's pre-PR gate is mandatory — these are not
optional extras here.

**Organization**: Grouped by user story. Note the sequencing constraint in
"Implementation Strategy" — US1 and US2 form the MVP together, not US1 alone.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3 from spec.md
- Exact file paths included in every task

## Path Conventions

Single Rust workspace. Feature code lives under
`waybill-cli/src/scan_fs/package_db/nix/haskell_packages/`, extending the m925
`nix/` reader rather than creating a sibling module (plan.md Structure
Decision).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Module skeleton and flag surface so later phases have somewhere to land.

- [x] T001 Create the module skeleton `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/` with `mod.rs`, `fetch.rs`, `cache.rs`, `package_set.rs`, `nix_base32.rs`, `boot_libraries.rs`, and declare the submodule in `waybill-cli/src/scan_fs/package_db/nix/mod.rs`
- [x] T002 [P] Add the FR-015a opt-out flag (disables this feature only, leaving other network enrichment active) to the scan args in `waybill-cli/src/cli/scan_cmd.rs`, with doc text distinguishing it from `--offline`
- [x] T003 [P] Create the hermetic fixture tree `waybill-cli/tests/fixtures/nix_haskell/` with a repo that has `flake.lock` + `.cabal` ranges and no freeze file; use synthetic package names (`waybill-fixture-*`) for anything that is not a real Haskell package name required by the scenario
- [x] T043 [P] Add the FR-019 retrieval time-bound override to the scan args in `waybill-cli/src/cli/scan_cmd.rs`, default 30 s, documented as a budget at 30× the measured warm fetch rather than a measured figure (research R7) — the plan claims this capability, so it needs a task or the claim needs striking
- [ ] T045 Point the per-revision cache at a per-test temporary directory in `waybill-cli/tests/nix_haskell_resolution_m926.rs` so T035/T036 assert against isolated state rather than the developer's real `$HOME`; if the override is an environment variable, route it through `crate::testing::EnvGuard::acquire()` to inherit the serialization that resolved the podman and m205 env-var races

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Retrieval, caching, gating, and the boot-library exclusion. No user story can be implemented correctly without the boot union — see Implementation Strategy.

**⚠️ CRITICAL**: T010–T012 must complete before US1, or US1 will resolve boot libraries against the default package set and emit versions the build does not use (FR-014c, Principle IX).

- [x] T004 Fix the R3 over-match in `specs/926-nixpkgs-haskell-versions/measurements/probe_nixpkgs_haskell.py`: extract nulled names by attrset nesting depth instead of the line-anchored regex, so `editedCabalFile` (depth 2, inside a derivation override) is excluded; re-run and update the counts in `specs/926-nixpkgs-haskell-versions/measurements/README.md`
- [ ] T005 Define `ResolutionOutcome` (`Resolved { version, source_hash, revision }` / `Unresolved { reason }`) and the closed reason enum (`compiler-supplied`, `absent-from-package-set`, `source-unreachable`, `no-exact-revision`, `offline`) in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs`, with no representable partial state (data-model C2, Principle IV)
- [ ] T006 Plumb `PinnedNixpkgs { revision, location, pin_state }` out of the existing m925 lockfile reader in `waybill-cli/src/scan_fs/package_db/nix/lockfile.rs` into `haskell_packages/mod.rs`, reusing `OriginalPinState` rather than re-reading `flake.lock` (research R6)
- [ ] T007 Implement the C1 trigger in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs`: run only when the lock resolves to `OriginalPinState::Exact` on a nixpkgs-shaped input AND the Haskell reader produced ≥1 declared dependency; honour `--offline` and the T002 flag. Classify the nixpkgs-shaped input from `flake.lock` alone with no network access — root input named `nixpkgs`, or a locked entry whose repository component is `nixpkgs` — never by probing an input to see whether it qualifies, and record which rule matched (FR-015b, SC-009)
- [ ] T008 Implement the per-revision cache at `~/.cache/waybill/nixpkgs/<rev>/` in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/cache.rs`, mirroring the m090/m108/m195 pinned-SHA layout with no TTL and no invalidation (research R5)
- [ ] T009 Implement bounded retrieval in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/fetch.rs`: build the URL from the lock entry's own location (FR-016, never assume upstream), apply the 30 s default bound (FR-019), and map unreachable / refused / unauthorized / timed-out to one degraded outcome without prompting (FR-017, FR-018)
- [x] T010 Implement the nulled-set reader in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/boot_libraries.rs`: collect every `name = null;` binding, then treat a name as a boot library only when it is ALSO a package in the package set (research R3). Do **not** scope by attrset nesting depth — measurement rejected that rule, because real boot libraries also live at deeper nesting (`directory-ospath-streaming`, a real package at v0.3, is nulled at depth in GHC 9.4.x). Over-inclusion withholds a version; under-inclusion invents one, so the rule must fail toward over-inclusion
- [ ] T011 Implement candidate-compiler determination and the boot union in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/boot_libraries.rs`: scan the project flake for explicit `haskell.packages.ghc<NN>` paths, fall back to every GHC series present at the revision, and take the **union** of their nulled sets so a package nulled in any candidate is treated as boot (FR-014/FR-014a, Principle III)
- [ ] T012 [P] Unit-test T010/T011 in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/boot_libraries.rs`: assert a nulled name that IS a package is a boot library, assert a nulled name that is NOT a package (`editedCabalFile`) is excluded, assert a nulled package at deeper attrset nesting is still a boot library (the `directory-ospath-streaming` case that rejected the depth rule), and assert the union rule marks a package nulled in only one candidate as boot
- [ ] T044 Emit the document-scope degradation record on any FR-017 degraded outcome in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs`, reusing the existing document-scope degradation annotation channel rather than introducing a new one (contract C7). If that channel cannot carry it, add the catalog row and three extractors under T031 instead of emitting an unregistered key — an unregistered `waybill:` key fails `every_catalog_row_has_an_extractor`, and an out-of-enum value can pass a `debug_assert!` in release (the #946 `nix-flake-lock` evidence-kind case)

**Checkpoint**: Retrieval, caching, gating and the boot exclusion are in place.

---

## Phase 3: User Story 1 - A Nix-built Haskell project gets real versions (Priority: P1) 🎯 MVP (with US2)

**Goal**: Dependencies the pinned revision carries gain an exact version and a source hash, moving out of design tier.

**Independent Test**: Scan the T003 fixture; assert a dependency declared as a range carries the exact version the revision pins, plus a native SHA-256, and is no longer design tier.

### Implementation for User Story 1

- [ ] T013 [P] [US1] Implement the Nix-base32 decoder in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/nix_base32.rs` (alphabet `0123456789abcdfghijklmnpqrsvwxyz`, reversed bit order, 52 chars → 32 bytes), returning `None` rather than a malformed digest when the input does not decode to exactly 32 bytes
- [ ] T014 [P] [US1] Unit-test the decoder in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/nix_base32.rs` with the research R2 vector: `1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly` → `9e26f12230d38ae56dcf94f8c139799dc3b7376f3434d35ce74847a0a24fd5ff`, plus a wrong-length input returning `None`
- [ ] T015 [US1] Implement the package-set parser in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/package_set.rs`: read `pname`/`version`/`sha256` into `name → PackageSetEntry`, handle **unquoted** attribute names (`th-compat = callPackage`), apply last-wins for names defined more than once, and convert each hash to hex at parse time via T013
- [ ] T016 [US1] Resolve each declared Haskell dependency against the package set minus the T011 boot union in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs`, producing one `ResolutionOutcome` per dependency
- [ ] T017 [US1] Consume the outcomes in `waybill-cli/src/scan_fs/package_db/haskell.rs`: attach the version to the existing component and promote it out of design tier, adding no new components (FR-001a, Principle XII constraint 1)
- [ ] T018 [P] [US1] Emit the source hash as a native `components[].hashes[]` entry with `alg: SHA-256` in `waybill-cli/src/generate/cyclonedx/builder.rs`
- [ ] T019 [P] [US1] Emit the source hash as a native `packages[].checksums[]` entry with `algorithm: SHA256` in the SPDX 2.3 package builder under `waybill-cli/src/generate/spdx/packages.rs`
- [ ] T020 [P] [US1] Emit the source hash in the SPDX 3 native content-identifier shape under `waybill-cli/src/generate/spdx/v3_document.rs`
- [ ] T021 [US1] Integration test in `waybill-cli/tests/nix_haskell_resolution_m926.rs`: a ranged dependency resolves to an exact version with a native SHA-256 in all three formats, and is not design tier, and resolution occurs with **no flags passed**, so a regression flipping the feature to opt-in fails (FR-015)
- [ ] T022 [US1] Integration test in `waybill-cli/tests/nix_haskell_resolution_m926.rs`: a repository with no `flake.lock` produces output byte-identical to a pre-feature scan (SC-007)
- [ ] T042 [US1] Integration test in `waybill-cli/tests/nix_haskell_resolution_m926.rs` for SC-001: assert the declared dependency set partitions **exactly** into resolved ∪ unresolved-with-reason — no dependency in neither, none in both — and that every member of the resolved partition carries both a version and a native SHA-256. Assert on the partition, not on a sampled dependency, so a regression resolving only the first entry fails

**Checkpoint**: Versions and native hashes land — but do not ship without US2.

---

## Phase 4: User Story 2 - A boot library is reported honestly, never invented (Priority: P1) 🎯 MVP (with US1)

**Goal**: Every dependency without a version says why, and nothing is ever invented.

**Independent Test**: Scan a fixture declaring an ordinary Hackage dependency and a boot library; assert the first carries a version and the second carries none plus reason `compiler-supplied`.

### Implementation for User Story 2

- [ ] T023 [US2] Emit the closed-set reason for every unresolved dependency in `waybill-cli/src/scan_fs/package_db/haskell.rs` via the `extra_annotations` channel, one value per `ResolutionOutcome::Unresolved` (FR-006)
- [ ] T024 [US2] Emit the candidate-compiler disclosure when more than one candidate was considered, naming whether they came from the flake scan or the all-series fallback, in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs` (FR-014b)
- [ ] T025 [US2] Integration test in `waybill-cli/tests/nix_haskell_resolution_m926.rs`: a boot library is versionless, carries reason `compiler-supplied`, and carries no source hash and no versioned identifier
- [ ] T026 [US2] Integration test in `waybill-cli/tests/nix_haskell_resolution_m926.rs` for SC-003: sweep every emitted Haskell component version and assert each traces to either the pinned package set or a project-local lockfile — the test fails if any version exists in neither
- [ ] T027 [US2] Integration test in `waybill-cli/tests/nix_haskell_resolution_m926.rs` for SC-002: assert the count of versionless Haskell dependencies carrying no reason is zero
- [ ] T028 [US2] Mutation-check T025/T026 by temporarily removing the boot union from T016 and confirming both tests fail; record the result in the PR body rather than only asserting the tests pass
- [ ] T041 [US2] Integration test in `waybill-cli/tests/nix_haskell_resolution_m926.rs` for FR-014c: using a fixture whose flake names several compilers, assert (a) the emitted component count equals the declared dependency count — no per-compiler variants — and (b) a package nulled in a candidate set carries **no** version *even though that same package is present in the default package set with a version*. Clause (b) is the specific Principle IX failure FR-014c exists to prevent, and it is currently the only requirement with no asserting task

**Checkpoint**: MVP complete — versions where they are known, honest silence where they are not.

---

## Phase 5: User Story 3 - The operator can tell where a version came from (Priority: P2)

**Goal**: A nixpkgs-resolved version is distinguishable from a lockfile-resolved one.

**Independent Test**: Scan two fixtures — one with a `cabal.project.freeze`, one resolved through nixpkgs — and assert their components carry different machine-readable provenance.

### Implementation for User Story 3

- [ ] T029 [US3] Emit the provenance annotation (version is nixpkgs-resolved, plus the revision it came from) for every resolved dependency in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs` (FR-007, C4)
- [ ] T030 [US3] Implement FR-013 precedence in `waybill-cli/src/scan_fs/package_db/haskell.rs`: a version from a project-local freeze/lock file wins over a nixpkgs-resolved one, and a disagreement between the two is recorded rather than silently resolved
- [ ] T031 [US3] Add catalog rows for the provenance, unresolved-reason and candidate-disclosure annotations to `docs/reference/sbom-format-mapping.md`, each with the Principle V audit stating why no native carrier exists — and explicitly noting that the source hash is **not** among them because it is emitted natively (research R2)
- [ ] T032 [US3] Add the three extractors per new row to `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` and register them in `waybill-cli/src/parity/extractors/mod.rs`, including the import lists, or `every_catalog_row_has_an_extractor` fails
- [ ] T033 [US3] Integration test in `waybill-cli/tests/nix_haskell_resolution_m926.rs`: nixpkgs-resolved and freeze-resolved components carry distinguishable provenance, and a disagreement between the two sources is recorded

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T034 Integration tests for the degradation paths in `waybill-cli/tests/nix_haskell_resolution_m926.rs`: unreachable host, refused/unauthorized host, `--offline`, and a lock pinning a moving reference — each produces component and relationship counts identical to a pre-feature scan, a document-scope degradation record, no credential prompt, and completion within the bound (SC-006, SC-008)
- [ ] T035 [P] Integration test in `waybill-cli/tests/nix_haskell_resolution_m926.rs`: two scans of the same repository at the same revision are byte-identical (SC-004), and the second performs no retrieval (SC-005)
- [ ] T036 [P] Integration test in `waybill-cli/tests/nix_haskell_resolution_m926.rs`: a repository with a flake but no Haskell dependencies performs no retrieval at all (SC-009)
- [ ] T037 [P] Add the CHANGELOG entry under `## [Unreleased]` in `CHANGELOG.md`, stating the measured before/after for the target and naming the native-hash finding
- [ ] T038 [P] Document the feature and its opt-out flag in the operator docs under `docs/`, including the private/internal-mirror behaviour from FR-016–FR-019
- [ ] T039 Run the mandatory pre-PR gate — `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` — and enumerate the per-target results rather than citing the exit code
- [ ] T040 Assess corpus golden impact: no existing corpus target is a Nix-built Haskell repository, so the expectation is zero churn; verify that claim and, if any golden moves, regenerate in CI per rule zero and attribute every change before installing

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies.
- **Phase 2 (Foundational)**: depends on Phase 1. **Blocks everything.**
- **Phase 3 (US1)**: depends on Phase 2 — specifically T011's boot union.
- **Phase 4 (US2)**: depends on Phase 2 and on T016 producing outcomes. Ships with US1.
- **Phase 5 (US3)**: depends on US1 (there must be a resolved version to attribute).
- **Phase 6 (Polish)**: depends on all prior phases.

### User Story Dependencies

- **US1 → US2**: not independent in the shipping sense. US1 alone would resolve boot libraries against the default package set and emit versions the build does not use. The boot union (T011) is foundational precisely so this cannot happen, but the *reporting* of why a dependency is unresolved is US2's contribution.
- **US3** is genuinely independent of US2 and depends only on US1.

### Within Each User Story

Types → parsing → resolution → emission → tests.

### Parallel Opportunities

- T002, T003 in Setup.
- T012 alongside the tail of Foundational.
- T013/T014 (decoder) alongside T015 (parser) — different files.
- T018, T019, T020 — three format emitters, three files.
- T035, T036, T037, T038 in Polish.

---

## Parallel Example: User Story 1

```
# after T011 lands, these three run together:
T013  nix_base32.rs        decoder
T015  package_set.rs       parser
T003  fixtures/nix_haskell/ fixture tree (if not already done in Setup)

# then, after T017:
T018  cyclonedx/builder.rs        native hashes[]
T019  spdx/packages.rs            native checksums[]
T020  spdx/v3_document.rs         native content identifier
```

---

## Implementation Strategy

### MVP is US1 + US2 together, not US1 alone

This departs from the usual "MVP = User Story 1" and the reason is
constitutional rather than stylistic. Every one of the measurement target's
19 declared dependencies appears in the **default** nixpkgs package set, but
that set is not what the project builds against (#947). Shipping US1 without
the boot exclusion and its reporting would attach a `base` or `text` version
the build never uses — an invented-looking value that reads as authoritative.
FR-014c forbids it and Principle IX forbids it.

The boot union is therefore in Phase 2, not Phase 4, so the exclusion exists
before any version is emitted. US2's phase delivers the *explanation* — reason
codes and candidate disclosure — which is separable and independently testable.

### Incremental Delivery

1. Phases 1–2 — retrieval, cache, gating, boot exclusion. Nothing user-visible.
2. Phases 3–4 — MVP. Versions with native hashes, honest silence with reasons.
3. Phase 5 — provenance, so consumers can weigh a nixpkgs version against a lockfile version.
4. Phase 6 — degradation, determinism, docs, gate.

### Deferred by design

The transitive closure is **#962**, not a task here. The package set carries
each derivation's dependency list, so it is feasible from the same artifact —
but the component-count multiplier is unmeasured, and Principle XII constraint
1 forbids introducing components an external source discovered. #962 leads
with measuring that multiplier.

---

## Notes

- **Zero new Cargo dependencies.** The Nix-base32 decoder is stdlib arithmetic;
  no crate implements this alphabet and bit order.
- **The source hash is native, not an annotation.** Verified in research R2
  against the real Hackage tarball. Do not add a catalog row for it; that would
  violate Principle V. This is the opposite of m925's `narHash` (C165) — same
  ecosystem, opposite outcome, because a NAR hash is over a directory
  serialization and this is over file bytes.
- **The probe is the executable statement of the boot-library rule**, which is
  why T004 is first in Foundational. Fixing it there is what rejected the
  attrset-depth rule: the depth fix dropped `directory-ospath-streaming`, a
  real package, and measurement caught it before any Rust was written (R3).
- **Fixtures must be hermetic.** The retrieval boundary is injected; no test
  touches the network.
- **Fixture package names** follow the project rule — synthetic
  `waybill-fixture-*` names except where a real Haskell package name is
  required by the scenario under test.
