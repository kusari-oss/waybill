# Tasks: Same-named resolves in different Pants namespaces

**Input**: Design documents from `/specs/922-fix-resolve-namespace-merge/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/resolve-namespace.md, quickstart.md
**Issue**: [#919](https://github.com/kusari-oss/waybill/issues/919)

**Tests**: included. Every success criterion is about emitted output or the
set of emitted files, which is only checkable by producing them. The defect is
also silent — one plausible-looking document instead of two — so a test that
does not fail against the pre-change binary proves nothing.

**Organization**: by user story. US1 and US2 are independent of each other
once Phase 2 lands; US3 depends on US1.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: different files, no dependency on an incomplete task.
- The three readers are genuinely parallel. Everything touching `split.rs` is
  not, and neither are the three emission-verification tasks — they are
  conditional on what T018 finds.

## Path Conventions

Repository root. `waybill-cli/src/scan_fs/package_db/` (recording),
`waybill-cli/src/generate/split.rs` (grouping, filenames, manifest),
`waybill-cli/src/generate/{cyclonedx,spdx}/` (emission),
`waybill-cli/src/parity/extractors/` (catalogue).

---

## Phase 1: Setup

- [X] T001 Capture the pre-change baseline into `specs/922-fix-resolve-namespace-merge/measurements/baseline.md`: split every Pants fixture by resolve and record, per fixture, the emitted filenames, per-document component PURLs, and the manifest's `subproject_id`/`root_purl`. **This is the FR-004 / SC-005 evidence and must be captured before any edit** — byte-identity for non-colliding repositories is the constraint most easily broken and least visibly.
- [X] T002 Preserve the pre-change release binary for the T030 teeth-check, and record in `specs/922-fix-resolve-namespace-merge/measurements/baseline.md` that `pants_namespace_collision` currently emits ONE `default.*` document containing both a Maven jar and a PyPI wheel.

**Checkpoint**: the defect and the byte-identity baseline are both on record.

---

## Phase 2: Foundational — the namespace, per component

**Purpose**: the data US1's grouping needs and US2 emits. Nothing downstream can be correct before this exists.

- [X] T003 Add the scalar per-component namespace accessor to `waybill-cli/src/scan_fs/package_db/pants_resolve.rs`: read, write, and a **plural guard** that returns "cannot answer" and warns rather than picking one. Mirror m911's `read_single` shape deliberately. Research R1 measured the namespace as scalar, but that is a property of which PURL types today's readers emit, not an enforced invariant — so the impossible case is detected, not assumed away. Unit tests for both arms.
- [X] T004 [P] Record the namespace in `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs` (both membership-write sites) — `python`.
- [X] T005 [P] Record it in `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs` — `jvm`.
- [X] T006 [P] Record it in `waybill-cli/src/scan_fs/package_db/pip/uv_lock.rs` — `python`. uv as a Pants Python backend is the site that is easy to miss; m911 nearly dropped it and m912 called it out for the same reason.
- [X] T007 Add the dedup merge policy for the new key in `waybill-cli/src/resolve/deduplicator.rs`, as an **explicit allowlist entry** beside m911's membership union — not a shape-based rule. Two components merging with different namespaces is the impossible case from T003 and must surface, not silently resolve.
- [X] T008 Add a test in `waybill-cli/tests/pants_resolve_membership.rs` asserting, across every Pants fixture **and every Pants corpus target** (SC-010 names both, and the corpus is where the three moving goldens live): a component carrying membership carries exactly one namespace, and a component carrying no membership carries **none** — absent, not empty (FR-007). Absence must mean exactly one thing, which is also what makes a document produced before this feature readable: the field is missing rather than blank. This is what stops a reader being added later that writes membership without a namespace.
- [X] T009 **The additive guarantee (FR-006a / SC-007 / contract C-4).** In `waybill-cli/tests/pants_resolve_membership.rs`, assert that `waybill:pants-resolve` keeps its v0.9.0 key and its array-of-bare-names value, byte-for-byte, on a non-colliding fixture. This is the promise the whole Option-A choice rests on — the namespace was put in a NEW annotation specifically so this field would not change a second time in one release cycle — and nothing else in this list checks it. T016's byte-identity covers *split output*, which is a different artifact.

**Checkpoint**: every component that belongs to a Pants resolve knows which namespace it is in.

---

## Phase 3: User Story 1 — two resolves that share a name produce two documents (P1)

**Goal**: `--split=resolve` groups on the qualified resolve, so a Python `default` and a JVM `default` get their own documents.

**Independent test**: split `pants_namespace_collision`. Two documents, disjoint components.

- [X] T010 [US1] Add the qualified-resolve type in `waybill-cli/src/generate/split.rs` — namespace plus name, **not constructible from a bare string**. Principle IV is doing real work here: the defect is that a `String` key silently accepted a bare name, and a type that cannot be built from one is what stops the next person reintroducing it.
- [X] T011 [US1] Write the failing test in `waybill-cli/tests/pants_namespace_split.rs`: `pants_namespace_collision` yields two documents whose component sets are disjoint. Confirm it fails against the pre-change binary for its own reason (one document, both namespaces).
- [X] T012 [US1] Change the grouping in `resolve_projections` (`waybill-cli/src/generate/split.rs`, the `by_resolve` map) to key on the qualified resolve.
- [X] T013 [US1] Qualify the filename slug **only where a collision exists**, in `filename_for` / the resolve projection's root construction in `waybill-cli/src/generate/split.rs`. **Do not reach for the existing sha8 collision fallback** — research R4 measured it inert here: every resolve projection's synthetic root has the same empty `source_dir`, so both colliding resolves hash identically and collide again.
- [X] T014 [US1] Qualify the manifest's `subproject_id` and `root_purl` on the same collision-only condition, in `waybill-cli/src/generate/split.rs`.
- [X] T015 [P] [US1] Test that on collision the two documents have distinct filenames and distinct manifest entries, in `waybill-cli/tests/pants_namespace_split.rs`.
- [X] T016 [US1] **The byte-identity test (SC-005 / FR-004)** in `waybill-cli/tests/pants_namespace_split.rs`: for every non-colliding Pants fixture, split output — filenames included — is byte-identical to the T001 baseline. This is the guard on the constraint most easily broken: a fix that qualifies unconditionally passes every other test in this list and silently renames every existing consumer's files.
- [X] T017 [P] [US1] In `waybill-cli/tests/pants_split_identity.rs`, test that each resulting document's C163 identity states exactly **one** resolve. m912's C-6 test asserts the merged document states two; that expectation inverts here, and the plural case stops arising.

**Checkpoint**: the defect is fixed. US3 becomes reachable.

---

## Phase 4: User Story 2 — a consumer partitioning membership can tell them apart (P2)

**Goal**: the namespace reaches the emitted document, on every component with membership.

**Independent test**: partition one unsplit document of the collision fixture by resolve. Two partitions, not one.

**Independent of US1** — it consumes Phase 2, not US1.

> **Correction made while writing these tasks.** Per-component annotations
> ride a **generic passthrough** — `waybill-cli/src/generate/cyclonedx/builder.rs:1596`
> iterates `component.extra_annotations` wholesale, and SPDX does the
> equivalent. No emitter names `waybill:pants-resolve` anywhere, yet it appears
> in all three formats. So **emission is free**: once Phase 2's readers write
> the key, the value is already in every document. The first draft of this
> phase had three "add emission" tasks that would have been no-ops.
>
> What remains is real: the catalogue row and its extractors, which are NOT
> automatic, and the tests.

- [X] T018 [US2] Verify the generic passthrough carries the namespace into all three formats with no emitter changes — scan a Pants fixture and confirm the annotation is present per component in CDX, SPDX 2.3 and SPDX 3. Record the result in `specs/922-fix-resolve-namespace-merge/measurements/verification.md`. **If it is not automatic, this task becomes the three emission tasks it replaced**, and that is the one finding that would change this phase's size.
- [X] T019 [US2] Confirm the SPDX 2.3 per-package annotation envelope carries it, in `waybill-cli/src/generate/spdx/annotations.rs` — adding emission only if T018 shows the passthrough does not cover it.
- [X] T020 [US2] Confirm the same for SPDX 3 in `waybill-cli/src/generate/spdx/v3_annotations.rs`.
- [X] T021 [US2] Register row **C164** in `waybill-cli/src/parity/extractors/mod.rs` with its three extractors in `cdx.rs`, `spdx2.rs`, `spdx3.rs`. C163 is the highest live row (research R5). A row without matching extractors fails `every_catalog_row_has_an_extractor`.
- [X] T022 [US2] Write the C164 row in `docs/reference/sbom-format-mapping.md`. **Justified by ABSENCE (FR-006e), and it must not reuse C163's language.** No format models a build tool's configuration namespace: `component.group` and the PURL namespace segment are the package's own identity and overloading either corrupts it; `compositions[]` describes aggregate completeness; `sourceInfo` is undefined free text. m912's audit found a carrier and declined it — this one finds nothing, and a catalogue where every row claims absence is a catalogue nobody checks.
- [X] T023 [P] [US2] Test that the namespace decodes to the same value in all three formats, in `waybill-cli/tests/pants_namespace_emission.rs`. Compare decoded values, not bytes.
- [X] T024 [P] [US2] In `waybill-cli/tests/pants_namespace_emission.rs`, test that an unsplit document of the collision fixture partitions into two groups by (namespace, membership). Today both components read `["default"]` with nothing beside them.
- [X] T025 [P] [US2] **Test C-2 against the measured case**, in `waybill-cli/tests/pants_namespace_emission.rs`: on `pants-example-python`, whose `python-default` resolve contains `pkg:generic/*` members (research R2), every member including the generic ones reads `python`. If a generic component reads anything else — or nothing — the namespace is being inferred from ecosystem rather than recorded.

**Checkpoint**: the document can answer the question without the split.

---

## Phase 5: User Story 3 — a repository whose only resolves collide still splits (P3)

**Goal**: the group count is taken after regrouping, so the simplest reproduction stops being swallowed.

**Independent test**: a fixture with exactly the colliding pair. Two documents, no fallback warning.

**Depends on US1.**

- [X] T026 [US3] Add `waybill-cli/tests/fixtures/pants_namespace_collision_only/` — the same two colliding resolves with **no** third resolve. The existing collision fixture deliberately carries a third (`lint`) so the split runs at all; this one is the case that currently produces no split whatsoever.
- [X] T027 [US3] Write the failing test in `waybill-cli/tests/pants_namespace_split.rs`: that fixture yields two documents and does **not** emit the not-partitionable warning. Today: `detected=1`, fallback fires, single unsplit SBOM.
- [X] T028 [US3] Ensure the group count at `waybill-cli/src/generate/split.rs` (the `groups.len() <= 1` check) is taken **after** regrouping. Research R3 is explicit: a fix that regroups at emission while counting earlier leaves this case silently unsplit, behind a warning that reads like correct behaviour.
- [X] T029 [P] [US3] In `waybill-cli/tests/pants_namespace_split.rs`, test that a genuinely single-resolve repository (`pants_pex`) **still** falls back with the warning. Only the miscount is being fixed; the degenerate case is still degenerate (FR-009).

**Checkpoint**: the simplest real-world reproduction is covered.

---

## Phase 6: Polish

- [X] T030 Teeth-check T011, T015, T017, T023–T025, T027 against the preserved pre-change binary and record in `specs/922-fix-resolve-namespace-merge/measurements/verification.md` which fail and which are guards. A test passing on both sides is a guard — label it rather than counting it.
- [~] T031 **Descoped to [#925](https://github.com/kusari-oss/waybill/issues/925).** No public polyglot Pants repository was confirmed to exist, and a synthetic corpus target would defeat the corpus's purpose — its value is that inputs are real and pinned. FR-010's actual requirement (a regression gate exercising a namespace collision) IS met, by the two fixtures and four tests. Original text: add a polyglot Pants corpus target to `waybill-cli/tests/corpus_harness_195/manifest.rs`. The defect shipped because nothing in the corpus has two namespaces; `pants-example-jvm` and `pants-example-python` between them cover both namespaces but never in one repository.
- [X] T032 Run the mandatory pre-PR gate: `./scripts/pre-pr.sh`. Enumerate the per-target results rather than citing the exit code.
- [ ] T033 Refresh the corpus goldens via the lane (`regen_goldens=true`), **reading the diff first**. Expect movement on exactly three targets — `pants-example-django`, `-jvm`, `-python` — and **none** on `-golang` or `-javascript`, which carry no membership (research R5). Movement on those two means the annotation is being emitted where there is no resolve, which FR-006d's absence rule forbids. Never regenerate locally.
- [X] T034 [P] Document the behaviour in `docs/reference/split-modes.md` — that same-named resolves across namespaces now split, and that filenames qualify only on collision — and write the CHANGELOG entry. Additive on the wire, but the split's **output filenames change** for a colliding repository, which a consumer scripting against them must know.
- [X] T035 Remove the now-vestigial m912 document-scope namespace index. Carrying the qualified identity on the group made `NamespaceIndex`, `index_record_all`, `index_insert` and the `pants_resolve_namespaces` field on `ScanArtifacts` / `ScanResult` dead weight — nothing reads them. Clippy does not flag them (pub fields on pub types), which is exactly why they will rot quietly. ~54 references across 11 files; deliberately NOT folded into the feature commits, because a large mechanical deletion buried in a behavioural change is unreviewable.
- [ ] T036 Comment on #919 with what landed, and on #924 noting that C161 remains Python-only and is now the last namespace-blind resolve field.

---

## Dependencies

```
Phase 1 (T001-T002)   baseline + defect on record
        │             T001 before ANY edit — it is the byte-identity evidence
Phase 2 (T003-T009)   the namespace, per component      ◀── BLOCKING
        ├──────────────────────────┐
Phase 3 US1 (T010-T017)      Phase 4 US2 (T018-T025)
  qualified grouping           emission + catalogue
        │                             (independent of US1)
Phase 5 US3 (T026-T029)   ◀── needs US1
        │
Phase 6 (T030-T035)
```

**Story independence**: US1 and US2 both consume Phase 2 and neither needs the
other — the grouping needs the namespace in process, not on the wire. US3 is
not independent of US1 and the graph says so: there is no regrouping to count
until there is regrouping.

## Parallel opportunities

- **T004, T005, T006** — three readers, three files.
- **T015, T017** — two test files.
- **T023, T024, T025** — three tests, one new file.
- **Phase 3 and Phase 4 as whole phases**, once Phase 2 lands.

Not parallel: anything in `split.rs` (T010, T012, T013, T014, T028); T021,
which must follow the emitters or the extractors have nothing to extract; and
T019/T020, whose work is conditional on what T018 finds about the generic
passthrough.

## Implementation strategy

**MVP = Phases 1–3.** That is the defect fixed: two documents instead of one
wrong document. US2 makes the answer available without the split; US3 covers
the simplest reproduction.

**T001 before anything else.** FR-004 says a non-colliding repository must be
byte-identical, and that is unprovable after the fact. The baseline is the
only evidence, and T016 is the test that spends it.

**T016 and T033 are the two that catch the plausible wrong fix.** Qualifying
filenames unconditionally passes every functional test and silently renames
every existing consumer's output; emitting the namespace where there is no
resolve passes every emission test and moves two goldens that should not move.

## Verification standard

1. **A test that passes against the pre-change binary is a guard, not a proof.** T030 requires labelling which is which.
2. **Golden movement is an assertion, not an observation.** Three targets move, two must not. Both halves are the check.
3. **Byte-identity is measured against a captured baseline**, never asserted from memory.
