# Tasks: A split document says which resolve it is

**Input**: Design documents from `/specs/912-split-doc-self-describing/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/resolve-identity.md, quickstart.md
**Issue**: [#914](https://github.com/kusari-oss/waybill/issues/914)

**Tests**: included. Every success criterion is about the shape of an emitted
document, which is only checkable by asserting on one. The defect is also
silent — two files that differ in no observable way — so a test that does not
fail against the pre-change binary proves nothing.

**Organization**: by user story. US2 depends on US1.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: different files, no dependency on an incomplete task.
- The three emitters are genuinely parallel. The readers are genuinely
  parallel. Everything touching `split.rs` is not.

## Path Conventions

Repository root. `waybill-cli/src/scan_fs/package_db/` (namespace recording),
`waybill-cli/src/generate/` (derivation and emission),
`waybill-cli/src/parity/extractors/` (catalogue),
`docs/reference/sbom-format-mapping.md`.

---

## Phase 1: Setup

- [X] T001 **Ask before building.** Confirm with the #914 requester that a consumer reads a split document without its manifest. `split-manifest.json` already maps every file to its resolve via `root_purl`, so if no such reader exists this feature is tidiness and the reader-layer work in Phase 2 is not worth it. Record the answer in `specs/912-split-doc-self-describing/measurements/value.md`. One message; the alternative is building Phase 2 on an unexamined premise.
- [X] T002 Capture the pre-change baseline to `specs/912-split-doc-self-describing/measurements/baseline.md`: split `waybill-cli/tests/fixtures/pants_discovered_resolves` by resolve and show that both documents name the repository AND carry a byte-identical `waybill:resolve-ownership`. The second half is the part that makes this a defect rather than a cosmetic gap — a reader sees two resolve names and nothing identifying the file.
- [X] T003 [P] Record in `specs/912-split-doc-self-describing/measurements/baseline.md` that `pants-example-jvm` has zero `lockfile-resolve` anchors, so a JVM Pants repository is already in the failing case regardless of whether its resolves are declared (research R3). The spec's "declared works, discovered does not" framing is true within Python and false across the product.

**Checkpoint**: the defect is observable, and its true breadth is written down.

---

## Phase 2: Foundational

**Purpose**: two things that do not exist yet and that every later task needs.

### The Pants language namespace

- [ ] T004 Add a language-namespace type — a closed set, `python` | `jvm`, not a string — beside the membership accessor in `waybill-cli/src/scan_fs/package_db/pants_resolve.rs`, with read and write helpers mirroring the membership ones.
- [ ] T005 [P] Record the namespace in `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs` at both emission sites, alongside `waybill:pants-resolve`.
- [ ] T006 [P] Record it in `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs`.
- [ ] T007 [P] Record it in `waybill-cli/src/scan_fs/package_db/pip/uv_lock.rs` — uv as a Pants resolver backend is `python`. This is the reader that was easy to miss in m911 and is easy to miss again.
- [ ] T008 Add a test in `waybill-cli/tests/pants_resolve_membership.rs` asserting every component carrying membership also carries a namespace. A reader that sets one without the other produces an identity that cannot satisfy FR-001a, and nothing else would catch it.

### Per-document doc-scope values

- [ ] T009 Add a per-document override slot for doc-scope values in `waybill-cli/src/generate/mod.rs`, set by `ScanArtifacts::narrow`. Today `narrow` copies every doc-scope field from the parent verbatim (`mod.rs:386-420`), which is why the ownership statement is identical across split documents — correct per FR-007, and also why there is nowhere to put a per-document value.
- [ ] T010 Assert in `waybill-cli/tests/pants_split_resolve.rs` that the repository-wide ownership value is unchanged and still identical across a repository's documents (FR-007, SC-006). The obvious wrong fix for this whole feature is to narrow that value instead of adding a new one; this is the test that rejects it.

**Checkpoint**: a namespace exists to be unambiguous about, and a place exists to put a per-document statement.

---

## Phase 3: User Story 1 — A document identifies itself (P1)

**Goal**: a per-resolve split document states which resolve it represents, readable from its own bytes.

**Independent test**: rename a document from a convention-only repository and read it in isolation. The resolve is recoverable.

### Fixtures first — both are load-bearing

- [ ] T011 [P] [US1] Add a JVM Pants fixture at `waybill-cli/tests/fixtures/pants_jvm_resolves/` with two coursier resolves. **Not optional** (research R3): anchoring is Pex-only, so every JVM document is in the failing case, and Python-only fixtures would let a Python-only implementation look complete.
- [ ] T012 [P] [US1] Add a namespace-collision fixture at `waybill-cli/tests/fixtures/pants_namespace_collision/` declaring the same resolve name under `[python.resolves]` and `[jvm.resolves]`. This is the only thing that exercises FR-001a, and it is also the reproducer for #919 — expect ONE document today where there should be two.

### Tests

- [ ] T013 [US1] Write the failing test for C-1/SC-001 in `waybill-cli/tests/pants_split_identity.rs`: each document from `pants_discovered_resolves` states its own resolve. Confirm it fails against the pre-change binary for its own reason.
- [ ] T014 [P] [US1] Write the filename-independence test (SC-003) in `waybill-cli/tests/pants_split_identity_filename.rs`: copy a document to an unrelated name and assert the answer is unchanged.
- [ ] T015 [P] [US1] Write the namespace test (SC-002a/C-2) against the T012 fixture: two resolves sharing a name are distinguishable. Note in the test that until #919 lands they arrive merged into one document, so this asserts the identity's shape, not the split's correctness.
- [ ] T016 [P] [US1] Write the absence test (SC-008/C-1) in `waybill-cli/tests/pants_split_identity_absence.rs`: an unsplit document, a `--split=workspace` document and a `--split=directory` document carry no identity — absent, not present-and-empty.
- [ ] T017 [P] [US1] Write the nothing-invented test (SC-007/FR-006/C-5) in `waybill-cli/tests/pants_split_identity_no_anchor.rs`: per-document component counts are unchanged from the T002 baseline, and no anchor component appears for a discovered resolve. **This is the guard on the constraint the whole feature is downstream of.** The easy wrong implementation synthesises the anchor m868 refused — which would make the identity trivial to derive and would pass every other test in this list.
- [ ] T018 [P] [US1] Write the cross-format test in `waybill-cli/tests/pants_split_identity_formats.rs`: the identity decodes to the same value in all three formats. Compare decoded values, not bytes — CycloneDX carries a property value as a string and SPDX carries structure (the m911 precedent).

### Implementation

- [ ] T019 [US1] Derive the identity in `waybill-cli/src/generate/split.rs::resolve_projections` from the resolve's own namespace-qualified name, **not** from the grouping key. The grouping key is the bare name and is what #919 is about; deriving from it would bake the defect into the identity.
- [ ] T020 [US1] Handle the merged-document case (C-6): a document that represents two resolves states both. Stating either alone would be false. Self-correcting — once #919 lands the state cannot arise.
- [ ] T021 [US1] Attach the identity to the per-document doc-scope slot from T009 in `waybill-cli/src/generate/split.rs`.
- [ ] T022 [P] [US1] Emit it in CycloneDX at `waybill-cli/src/generate/cyclonedx/metadata.rs` (doc-scope properties, beside the C161 emission around `:764`).
- [ ] T023 [P] [US1] Emit it in SPDX 2.3 at `waybill-cli/src/generate/spdx/annotations.rs` (beside `:764`).
- [ ] T024 [P] [US1] Emit it in SPDX 3 at `waybill-cli/src/generate/spdx/v3_annotations.rs` (beside `:763`).
- [ ] T025 [US1] Register the new row — next free id is **C163** — in `waybill-cli/src/parity/extractors/mod.rs` with its three extractors in `cdx.rs`, `spdx2.rs`, `spdx3.rs`. A row without matching extractors fails `every_catalog_row_has_an_extractor`.
- [ ] T026 [US1] Write the C163 row in `docs/reference/sbom-format-mapping.md`. **The KEEP-NO-NATIVE audit must say that a native carrier EXISTS and is deliberately unused** — `metadata.component` / `documentDescribes` / `rootElement` already carry this semantic and work for declared Python resolves; they fail elsewhere only because they point at a component and FR-006 declines to invent one. The decisive reason for an annotation is parity: SPDX 3's `Bundle.context` is the one structured native option and CycloneDX has no equivalent. Claiming no construct exists would be false and would sit in the catalogue unchallenged (research R1).

**Checkpoint**: a document says what it is. US2 is unblocked.

---

## Phase 4: User Story 2 — Every document answers the same way (P2)

**Goal**: one reading procedure, no branch on whether the resolve was declared.

**Independent test**: read the identity from a declared-resolve document and a discovered-resolve document with one expression. Both answer.

**Depends on US1.**

- [ ] T027 [US2] Ensure the identity is attached to **every** per-resolve document in `waybill-cli/src/generate/split.rs`, including declared ones whose root already names the resolve (FR-004). This is the requirement most likely to be dropped as redundant; it is what stops a consumer having to establish provenance in order to know where to read identity.
- [ ] T028 [US2] Add the agreement test (FR-005/SC-005) in `waybill-cli/tests/pants_split_identity.rs`: for a declared-Python document, the identity and `metadata.component` name the same resolve. Two fields stating one fact drift, and this session already produced two instances of that class — the C143 mis-parse risk and the C161 double-encoding.
- [ ] T029 [US2] Add the one-procedure test (SC-004) in `waybill-cli/tests/pants_split_identity.rs`: one expression reads the identity from a declared document, a discovered document and a JVM document, with no branch on provenance.

**Checkpoint**: the feature is usable without knowing how a repository declares its resolves.

---

## Phase 5: Polish

- [ ] T030 Teeth-check T013–T018 against the pre-change binary and record in `specs/912-split-doc-self-describing/measurements/us1-verification.md` which fail and which are guards. A test passing on both sides is a guard, not proof — label it rather than counting it.
- [ ] T031 Run the mandatory pre-PR gate: `./scripts/pre-pr.sh`. Enumerate the per-target results rather than citing the exit code.
- [ ] T032 Assess corpus golden impact on `waybill-cli/tests/fixtures/public_corpus/`. Expect **none**: the corpus has no per-resolve split output, so a document-scope identity that only appears in split documents cannot reach it. If a golden does move, that is a signal the identity is leaking into unsplit documents — which FR-008 forbids.
- [ ] T033 [P] Add the C163 row and the new `--split=resolve` behaviour to `docs/reference/split-modes.md`, including that an identity is absent from non-resolve splits.
- [ ] T034 [P] Write the CHANGELOG entry. Additive — nothing that parses today stops parsing — so it does not need the warning m911's entry led with.
- [ ] T035 Comment on #914 and #919: what landed, and that the identity is defined so it stays correct once #919 is fixed.

---

## Dependencies

```
Phase 1 (T001-T003)   the value question, and the baseline
        │             T001 can end the feature. It is first for that reason.
Phase 2 (T004-T010)   namespace + per-document plumbing   ◀── BLOCKING
        │
Phase 3 US1 (T011-T026)  ◀── MVP
        │
Phase 4 US2 (T027-T029)  ◀── needs US1
        │
Phase 5 (T030-T035)
```

**Story independence**: US2 is not independent of US1 and the graph says so —
there is no identity to read uniformly until there is an identity. US1 alone
is shippable and is the whole of SC-001 through SC-003.

## Parallel opportunities

- **T005, T006, T007** — three readers, three files.
- **T011, T012** — two fixtures.
- **T014 through T018** — five test files.
- **T022, T023, T024** — three emitters.
- **T033, T034** — docs and CHANGELOG.

Not parallel: anything in `split.rs` (T019–T021, T027), and T025 must follow
the emitters or the extractors have nothing to extract.

## Implementation strategy

**MVP = Phases 1–3.** That is the feature: a document that says what it is.
US2 makes it usable without provenance knowledge, which matters to a consumer
but does not change what any single document contains.

**T001 is first because it can end the feature.** The manifest already answers
"which resolve is this file" for anyone who has the manifest. The entire case
rests on a reader that does not, and nobody has confirmed one exists. Phase 2
is the largest work in the feature and it would all be spent on that premise.

## Verification standard

1. **A test that passes against the pre-change binary is a guard, not a proof.**
   T030 requires labelling which is which.
2. **The JVM fixture is not optional.** Anchoring is Pex-only, so Python-only
   fixtures would let a Python-only implementation pass while an entire
   ecosystem stays broken.
3. **Corpus goldens should not move.** If they do, the identity is leaking
   into documents that have no resolve to identify.
