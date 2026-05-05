---
description: "Task list for milestone 072 — cross-tier SBOM binding (verify image's foo == source's foo; constrain VEX propagation by binding strength)"
---

# Tasks: Cross-tier SBOM binding

**Input**: Design documents from `/specs/072-cross-tier-sbom-binding/`
**Prerequisites**: spec.md ✅ (with 3 clarifications), plan.md ✅, research.md ✅, data-model.md ✅, contracts/{binding-hash-v1,source-document-binding-annotation,openvex-instance-identifiers}.md ✅, quickstart.md ✅

## Format: `[ID] [P?] [Story?] Description`

## Phase 1: Setup

- [X] T001 Confirm working tree clean and on branch `072-cross-tier-sbom-binding`. Confirm `cargo +stable test --workspace` passes baseline before any edits (so any new failure is attributable to this milestone).
- [X] T002 Create the new module skeleton at `mikebom-cli/src/binding/{mod.rs,hash.rs,source_inputs.rs,verify.rs,annotation.rs}` with `pub mod ...` declarations in `mod.rs` and `pub mod binding;` in `mikebom-cli/src/lib.rs` (so integration tests can import it). Each file is empty stubs at this stage. Verify `cargo +stable check -p mikebom` builds clean.

## Phase 2: Foundational (blocking prerequisites for all user stories)

### Data types (data-model.md)

- [X] T003 [P] Implement the 5 newtype/enum data types in `mikebom-cli/src/binding/mod.rs`: `BindingHashInputs`, `BindingHash` (newtype with `from_hex` validator), `BindingStrength` enum, `SourceDocumentId`, `SourceDocumentBinding`. Use `serde::Serialize` + `serde::Deserialize` derives where data-model.md specifies. Add `#[derive(clap::ValueEnum)]` on a new `VexPropagationMode { Permissive, Caveated, Strict }` enum (default `Caveated`) per data-model.md. Add inline unit tests covering: (a) `BindingHashInputs::populated_count()` with 0/1/2/3 sides; (b) `BindingHash::from_hex` rejects non-hex / wrong-length input; (c) `BindingStrength` derives correctly from `populated_count()` per VR-001.
- [X] T004 [US-foundational] Extend `OpenVexProduct` at `mikebom-cli/src/generate/openvex/statements.rs:71` with `pub identifiers: BTreeMap<String, String>` field, `#[serde(skip_serializing_if = "BTreeMap::is_empty", default)]`. Confirm the existing emit-side code paths (any callers of `OpenVexProduct::new` or struct-literal constructions) are updated to either pass `BTreeMap::new()` (back-compat) or populate identifiers. Add a unit test asserting empty-map produces wire-identical pre-072 shape and populated-map produces post-072 shape per `contracts/openvex-instance-identifiers.md` C-1.

### Binding hash algorithm (contracts/binding-hash-v1.md)

- [X] T005 [US-foundational] Implement `compute_binding_hash(inputs: &BindingHashInputs) -> BindingHash` in `mikebom-cli/src/binding/hash.rs`. Algorithm per contracts/binding-hash-v1.md C-2 + C-3: build a `serde_json::Value` object with keys `algo`, `lockfile`, `manifest`, `vcs` (lex-sorted), values are JSON `null` when input side is `None`, otherwise the string. Pass through `parity::extractors::common::canonicalize_for_compare(value, false)` from milestone 071 for canonical serialization. SHA-256 the UTF-8 bytes via `sha2::Sha256`. Hex-encode lowercase via `data_encoding::HEXLOWER`. Return `BindingHash::from_hex(...)`. Add unit tests for: (a) all-three-sides populated produces stable hex; (b) only-manifest-populated produces different stable hex; (c) two equivalent inputs (same content, different on-disk bytes — N/A here, just verify determinism across two calls); (d) the algo field is always `"v1"`; (e) **pinned-vector cross-version determinism (per analyze C2 + SC-007)** — assert at least 3 specific (vcs, lockfile, manifest) → known-good lowercase-hex pairs computed at first commit. The pinned values become the algo-v1 contract; future changes to canonicalization or input-encoding break this test, surfacing the version-drift risk before consumers see it. A future algo-v2 work creates new v2-pinned vectors in parallel, leaving v1 pinned values intact for the deprecation window.

### Per-ecosystem source-input extractors (research.md §1)

- [X] T006 [P] [US-foundational] Implement `extract_source_inputs_for_component(c: &PackageDbEntry, scan_root: &Path) -> BindingHashInputs` in `mikebom-cli/src/binding/source_inputs.rs`. Dispatch on the component's PURL ecosystem. For each of golang/cargo/npm/pip/gem/maven, populate the input triple per research.md §1. Reuse: Go BuildInfo `vcs.revision` from `scan_fs/package_db/go_binary.rs::GoVcsInfo`; existing per-ecosystem main-module discovery from milestones 053-070 already located the lockfile + manifest paths in the `PackageDbEntry`. SHA-256 the file bytes (as on disk, no canonicalization per contracts/binding-hash-v1.md C-1). For VCS where `git rev-parse HEAD` is the source, shell out via `Command::new("git")` (same pattern as milestone 053's `git describe`). Tolerate absent inputs gracefully — leave the corresponding side as `None`. Add unit tests for at least 3 ecosystems (cargo / npm / golang) using inline fixtures.

### Annotation serialize/deserialize (contracts/source-document-binding-annotation.md)

- [X] T007 [P] [US-foundational] Implement `serialize_to_cdx_property(b: &SourceDocumentBinding) -> serde_json::Value` and `serialize_to_envelope_value(b: &SourceDocumentBinding) -> serde_json::Value` (the value-side of `MikebomAnnotationCommentV1`) in `mikebom-cli/src/binding/annotation.rs`. Plus matching deserialize functions. The CDX side encodes the binding as a JSON-string (per contracts/source-document-binding-annotation.md C-3 CDX 1.6 example); the SPDX-envelope side encodes as a JSON object. Round-trip unit tests: serialize → deserialize → equality.

### Parity catalog row registration (constraint from plan.md)

- [X] T008 [US-foundational] Add a new `ParityExtractor` row at `mikebom-cli/src/parity/extractors/mod.rs` for `mikebom:source-document-binding` (suggested row_id `C46` — pick the next free C-section row after the milestone-071 audit). Directionality `SymmetricEqual`. Per-format extractors at `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` reuse the existing macros (`cdx_anno!`, `spdx23_anno!`, `spdx3_anno!`). Confirm the catalog ordering invariant `extractors_table_is_sorted_by_row_id` still passes by inserting at the alphabetical position. Add a row to `docs/reference/sbom-format-mapping.md`'s parity-catalog table.

## Phase 3: User Story 1 — Source ↔ image binding emission + verification (P1) 🎯 MVP

### CDX emission (contracts/source-document-binding-annotation.md C-2 + C-3)

- [X] T009 [US1] Wire binding emission into `mikebom-cli/src/generate/cyclonedx/builder.rs` for per-component `properties[]`: when a component has a populated `SourceDocumentBinding` (from the new `--bind-to-source` flow per US4 OR from the consumer-side emission flow), push a `properties[]` entry with `name == "mikebom:source-document-binding"` and `value = serialize_to_cdx_property(...)` (the JSON-encoded string). Emit only on `mikebom:sbom-tier: build` or `deployed` components — source-tier components don't carry the binding (per plan.md constraint, source-tier goldens stay byte-identical to alpha.14).
- [X] T010 [US1] Wire document-level cross-document reference into `mikebom-cli/src/generate/cyclonedx/metadata.rs` `metadata.component.externalReferences[]` per contracts/source-document-binding-annotation.md C-2 CDX section. Type: `"bom"`, URL = source-doc IRI (or empty string when only sha256 known), comment = `"source-tier SBOM that produced this build/deployment"`, hashes = `[{ alg: "SHA-256", content: <sha256-of-source-sbom-canonical-bytes> }]`. Only emit when the scan was invoked with `--bind-to-source` AND the source was loaded successfully.

### SPDX 2.3 emission

- [X] T011 [P] [US1] Wire binding emission through the existing `MikebomAnnotationCommentV1` envelope at `mikebom-cli/src/generate/spdx/annotations.rs` (around the existing `c.extra_annotations` handling). Push a `mikebom:source-document-binding` entry per Package whenever the component has a populated `SourceDocumentBinding`. Reuse the existing `push` helper (line 110-area).
- [X] T012 [US1] Add document-level `externalDocumentRefs[]` + a `BUILT_FROM` relationship to `mikebom-cli/src/generate/spdx/document.rs` (or wherever SPDX 2.3 document-level construction lives) per contracts/source-document-binding-annotation.md C-2 SPDX 2.3 section. ExternalDocumentId format: `DocumentRef-source-sbom`. Relationship pairs the image-tier root SPDXID with the cross-doc reference.

### SPDX 3 emission

- [X] T013 [P] [US1] Wire binding emission through the existing `Annotation.statement` envelope at `mikebom-cli/src/generate/spdx/v3_annotations.rs` (mirror T011's pattern). Same envelope shape; the `subject` field points at the Package's `spdxId`.
- [X] T014 [US1] Add document-level `import[]` (`ExternalMap`) on the `SpdxDocument` element + a `Relationship` graph element with `relationshipType: built_from` to `mikebom-cli/src/generate/spdx/v3_document.rs` (or equivalent) per contracts/source-document-binding-annotation.md C-2 SPDX 3 section.

### Consumer-side verify subroutine

- [X] T015 [US1] Implement `verify_binding(image_sbom: &Value, source_sbom: &Value) -> VerifyReport` in `mikebom-cli/src/binding/verify.rs`. Algorithm: (1) walk the image-tier SBOM's components (CDX `components[]`, SPDX `packages[]`, SPDX 3 `@graph[Package]`); (2) for each component, decode the `mikebom:source-document-binding` annotation; (3) recompute the binding hash by extracting the named source-tier component from the source SBOM and running through `extract_source_inputs_for_component` + `compute_binding_hash`; (4) compare; (5) return a per-component pass/fail report. The `VerifyReport` struct mirrors the JSON shape shown in quickstart.md Recipe 1 + Recipe 2. Follow VR-005: exit non-zero on ANY verification failure.

### CLI surface — `mikebom sbom verify-binding` subcommand

- [X] T016 [US1] Add `verify-binding` subcommand to `mikebom-cli/src/cli/sbom_cmd.rs`. Args: `--image-sbom <path>`, `--source-sbom <path>`, `--format {table,json}` (default `table`). Implementation calls `binding::verify::verify_binding` and renders the report. Exit non-zero on verification failures per FR-005 / VR-005. Matches the existing `parity-check` command's signature pattern.

### US1 tests

- [X] T017 [US1] Create `mikebom-cli/tests/binding_emission.rs`. Test the round-trip: scan a Go fixture (use the existing `cargo-workspace` or create a small synthesized one with `git init` + commit + Cargo.toml + Cargo.lock), invoke `mikebom sbom scan --image <fake-image> --bind-to-source <source-sbom-path>`, parse the resulting image-tier SBOM, assert each first-party component carries a `mikebom:source-document-binding` annotation with the expected `strength` and a non-empty `binding_hash`. Test in all 3 formats. Note: T009-T014 emission code is the production target; this test exercises it end-to-end.
- [X] T018 [US1] Create `mikebom-cli/tests/binding_verify.rs`. Two test cases: (a) clean verify — scan source + image with `--bind-to-source`, run `mikebom sbom verify-binding`, assert `verified` strength + zero failures + exit 0; (b) wrong-source verify — same image SBOM but a source SBOM from a different commit (synthesize via git `git commit --allow-empty -m "drift"` + re-scan), assert `verification-failed` reason + exit non-zero per VR-005.

## Phase 4: User Story 2 — VEX propagation respects binding strength (P1)

### OpenVEX product-identifier population

- [ ] T019 [US2] Populate `OpenVexProduct.identifiers` at every existing **mikebom-emit-side** OpenVEX statement-construction site — i.e., where mikebom *itself* constructs an OpenVEX statement from an `AdvisoryRef` on a `ResolvedComponent` (NOT the propagation path; T020 owns the propagated-statement construction). Concrete scope (per analyze F1 fix): grep `mikebom-cli/src/` for `OpenVexProduct {` literal struct constructions; the matches that live OUTSIDE `sbom/mutator.rs::propagate_vex_with_binding` (which T020 introduces) are this task's targets. At each site, populate `identifiers` map with `purl` (always — equal to the existing `@id`); `cyclonedx-bom-ref` from `ResolvedComponent.bom_ref` when the OpenVEX is paired with a CDX SBOM; `spdx-spdxid` from `ResolvedComponent.spdx_id` when paired with a SPDX SBOM. Pre-072 sites that don't have the per-format identifier in scope at construction time get an empty map (skip-serialized — back-compat preserved). T020's propagation path also populates `identifiers` but for the TARGET-SBOM's instance — the lookup happens at propagation time when both sides are known.

### Propagation engine

- [ ] T020 [US2] Implement `propagate_vex_with_binding(mode: VexPropagationMode, source_vex: &OpenVexDocument, target_sbom: &Value) -> PropagationReport` in `mikebom-cli/src/sbom/mutator.rs`. Algorithm per contracts/openvex-instance-identifiers.md C-3 + C-4 + C-5: for each statement in the source VEX, for each product in the statement, find the matching target-component instances (PURL match + optional bom-ref/spdxid match); look up each instance's `mikebom:source-document-binding`; apply mode-specific behavior. **Matching rules (per analyze F2 fix):** when the source statement carries a per-instance identifier (`cyclonedx-bom-ref` or `spdx-spdxid` in `Product.identifiers`), match is one-to-one against the target instance with the same identifier. When the source statement has no per-instance identifier (typical of pre-072 input), match is one-to-many: ALL target instances with the same PURL are candidates. Per-mode behavior on the matched set: (a) `permissive` → propagate unchanged to every matched instance (pre-072 semantic preserved); (b) `caveated` → propagate to every matched instance WITH a `mikebom:vex-binding-status: unverified` caveat on every instance whose binding strength is not `verified`; (c) `strict` → propagate to ONLY the matched instances with `verified` binding strength, write a structured refusal-rationale annotation under `mikebom:enrichment-patch[N]` for every non-`verified` instance that was refused, and exit non-zero per VR-006. Produce a `PropagationReport` summarizing what was propagated, caveated, or refused, with per-instance breakdown so callers can audit. Also populate `OpenVexProduct.identifiers` for each propagated statement using the TARGET instance's bom-ref/SPDXID (this is the propagation-path identifier population complementing T019's emit-side path).

### CLI flag wiring

- [ ] T021 [US2] Add `--vex-propagation-mode {permissive,caveated,strict}` to `EnrichArgs` at `mikebom-cli/src/cli/enrich.rs:8` per data-model.md `VexPropagationMode` derive (uses `clap::ValueEnum` per the established `ImageSource` pattern from milestone-071's review feedback). Default value: `caveated`. Wire `cli/enrich.rs::execute` to call `mutator::propagate_vex_with_binding` when `--vex-overrides <path>` is supplied. The `--vex-overrides` flag's pre-072 no-op behavior is replaced by the new propagation logic. Document the breaking-change opt-out (`--vex-propagation-mode permissive`) in the flag's help text.

### Per-instance VEX emission in target SBOM

- [ ] T022 [US2] Update the CycloneDX `vulnerabilities[]` emit site (find via `grep -rn "vulnerabilities" mikebom-cli/src/generate/cyclonedx/`) so propagated VEX statements include the per-instance `affects[].ref` correctly — each `affects` entry binds to a specific bom-ref, not to a coord-aggregate. The `mikebom:vex-binding-status` field appears as a sibling on each `affects` entry per contracts/openvex-instance-identifiers.md C-5.

### US2 tests

- [ ] T023 [US2] Create `mikebom-cli/tests/binding_drift.rs`. Synthesize a (source SBOM, target SBOM) pair where the source has a `not_affected` VEX statement on `pkg:golang/x@v1` but the target's instance is bound with `strength=weak` (NOT verified). Run `mikebom sbom enrich --vex-propagation-mode strict` and assert the propagation is REFUSED — exit non-zero, no `affects` entry written for that vuln, refusal-rationale annotation present in the output SBOM's `metadata.properties[]` (under the existing `mikebom:enrichment-patch` provenance scheme).
- [ ] T024 [US2] Create `mikebom-cli/tests/vex_per_instance.rs` — the canonical worked-example test from US2 AS#4 / SC-003. Synthesize a target SBOM with TWO instances of `pkg:golang/golang.org/x/net@v0.28.0` — instance A with `bom-ref=foo-net-instance` bound to source (`strength=verified`), instance B with `bom-ref=baselayer-net-instance` unbound (`strength=unknown`). Source-tier OpenVEX has `not_affected` on the PURL. Run `mikebom sbom enrich --vex-propagation-mode caveated`. Assert: (a) instance A receives `not_affected` cleanly; (b) instance B receives the statement WITH a `mikebom:vex-binding-status: unverified` caveat AND an `affected`-by-default rationale; (c) the per-PURL aggregate (per the C-3 aggregation rule) reports `affected` because instance B is unverified.

## Phase 5: User Story 3 — `mikebom sbom trace-binding` operator triage (P2)

- [ ] T025 [US3] Add `trace-binding` subcommand to `mikebom-cli/src/cli/sbom_cmd.rs`. Args: `--component-purl <purl>`, `--image-sbom <path>`, `--candidate-sources-dir <dir>` (or `--source-sbom <path>` for single-source mode), `--format {table,json}`. Implementation: load the image SBOM, find ALL instances of the component matching the PURL, for each load-and-test against the candidate sources, return per-instance binding state (per quickstart.md Recipe 6 output shape).
- [ ] T026 [US3] Create `mikebom-cli/tests/binding_trace.rs`. Three test cases: (a) component exists in image with one source-SBOM-bound instance → trace returns `verified` for that instance with the bound source ID; (b) component exists in image but NO candidate source SBOM contains it → trace returns `unknown` with `reason: "source-not-found-in-bind-target"`; (c) component appears via two paths (one bound + one unbound) → trace returns BOTH instances with their respective binding states. Exit code 0 (informational).

## Phase 6: User Story 4 — `--bind-to-source` first-party emission (P3)

- [X] T027 [US4] Add `--bind-to-source <path-or-iri>` flag to `ScanArgs` at `mikebom-cli/src/cli/scan_cmd.rs`. When set, mikebom loads the source SBOM at the named location BEFORE the per-component emission stage. For each image-tier component, look up the matching source-tier component (PURL match), call `extract_source_inputs_for_component` against the source SBOM's project root, compute the binding hash, attach a `SourceDocumentBinding` to the component's evidence bag. If the source SBOM cannot be loaded, exit non-zero with a structured error per FR-011 (Constitution Principle X transparency). If individual components don't match the source, attach `SourceDocumentBinding { strength: Unknown, reason: "source-not-found-in-bind-target", ... }` per FR-003.
- [ ] T028 [US4] Create `mikebom-cli/tests/bind_to_source_emission.rs`. Test cases: (a) source SBOM load succeeds + all image components match → all components carry `verified` or `weak` binding; (b) source SBOM path doesn't exist → command exits non-zero per FR-011; (c) source SBOM exists but target image has components with no source counterpart → those components carry `Unknown` strength with the documented reason. Verify via JSON parsing of the emitted SBOM.

- [X] T028b Create the **published reference fixture set** for SC-004 (per analyze C1 fix) at `docs/reference/binding-fixtures/`. Three fixture pairs covering the three strength outcomes: (1) `cargo-verified/` — a small Rust project with git checkout + Cargo.toml + Cargo.lock; expected `strength: verified` once T005's pinned-vector hex is computed and pinned here; (2) `golang-verified/` — small Go module with go.mod + go.sum + git HEAD; expected `verified`; (3) `maven-weak/` — small maven project with pom.xml + git HEAD but NO lockfile; expected `strength: weak` per the maven cap in research.md §1. Each fixture directory contains the source-tier SBOM (`source.cdx.json`), the synthesized image-tier SBOM (`image.cdx.json`), and a `EXPECTED.md` with the canonical (vcs, lockfile, manifest) input triple + the expected SHA-256 hex output per fixture. External verifiers writing their own implementation use these to validate against mikebom's emission. The pinned hex values match the T005 pinned-vector test outputs — single source of truth.

## Phase 7: Polish

- [ ] T029 Author `docs/reference/cross-tier-binding.md` per FR-010 — comprehensive reference for external verifiers. Sections: §1 the binding-hash-v1 algorithm (canonical envelope, SHA-256, hex output); §2 per-ecosystem input table (mirrors research.md §1); §3 per-format carrier shapes (CDX `properties[]`, SPDX 2.3 envelope, SPDX 3 envelope, plus standards-native `externalReferences`/`externalDocumentRefs`/`built_from`); §4 OpenVEX `Product.identifiers` extension contract; §5 the propagation modes + the C-3 aggregation rule; §6 a Python verifier reference implementation snippet (mirrors the milestone-071 conformance-harness-guide pattern); §7 stability commitment + algo-version policy. **§8 (per analyze C1 fix) — pointer to `docs/reference/binding-fixtures/` (created in T028b) as the SC-004 published reference fixture set: three pinned fixture pairs (cargo-verified, golang-verified, maven-weak) with canonical input triples + expected SHA-256 hex outputs that any external verifier implementation can use to validate compatibility with mikebom's emission.**
- [ ] T030 Update `docs/design-notes.md` with a new section "Cross-tier SBOM binding (milestone 072)" pointing to the new `cross-tier-binding.md` and explaining the operator-visible behavior: when to use `--bind-to-source`, what `verify-binding` / `trace-binding` answer, and how the `--vex-propagation-mode` default flip affects existing pipelines (with a clear migration paragraph for operators using `--vex-overrides` today).
- [ ] T031 CHANGELOG.md `[Unreleased]` entry for milestone 072. Sections: **Added** (binding emission, verify-binding + trace-binding subcommands, OpenVEX per-instance identifiers, cross-tier-binding.md guide); **Changed (BREAKING — VEX propagation default)** (the `--vex-propagation-mode` default flip from implicit-permissive to `caveated`, with the `--vex-propagation-mode permissive` opt-out call-out); **Migration** (operators currently using `mikebom sbom enrich --vex-overrides <path>` see new caveats on unverified bindings — read the migration guide in design-notes.md); **Goldens regen** (the new `mikebom:source-document-binding` row will surface on image-tier-fixture goldens — call out which fixtures changed and that source-tier goldens are unchanged).
- [ ] T032 Run `MIKEBOM_UPDATE_CDX_GOLDENS=1 / SPDX_GOLDENS=1 / SPDX3_GOLDENS=1` to regenerate the 27 byte-identity goldens. Spot-check: source-tier goldens (the 9 source-scan fixtures) MUST be byte-identical to alpha.14 — milestone 072 emits the binding annotation only on `build`/`deployed` tiers. If any source-tier golden changes, halt and investigate (likely indicates an unintended emission on the source-tier path).
- [ ] T033 Run `./scripts/pre-pr.sh` end-to-end and confirm clippy clean + every test target reports `ok. N passed; 0 failed`. Per the memory rule, show the full per-target output, NOT just a failure-grep.
- [ ] T034 Open PR via `gh pr create` with title `feat(072): cross-tier SBOM binding (verify image foo == source foo; constrain VEX propagation by binding strength)`. Body cites the SC-001..SC-008 measurement targets; the worked-example test (SC-003) explicitly validated; the breaking change called out at the top with the migration path; pointer to `docs/reference/cross-tier-binding.md` for external verifier authors.

## Dependencies

```text
T001 (Setup) → T002 (module skeleton)
                 │
                 ├─→ T003 (data types)               (Foundational, blocks every US)
                 ├─→ T004 (OpenVexProduct extend)
                 ├─→ T005 (binding hash algo)
                 ├─→ T006 (per-ecosystem extractors)
                 ├─→ T007 (annotation serde)
                 └─→ T008 (parity catalog row + docs/sbom-format-mapping)
                            │
                            ├─→ T009-T014  (US1 emission, parallel CDX/SPDX2/SPDX3 paths)
                            │      ↓
                            │      T015 (verify subroutine, depends on T009-T014 emit shapes)
                            │      ↓
                            │      T016 (verify-binding CLI)
                            │      ↓
                            │      T017, T018 (US1 tests, depend on T009-T016 wired)
                            │
                            ├─→ T019 (OpenVEX identifier emission, parallel with US1)
                            │      ↓
                            │      T020 (propagation engine), T022 (CDX vulnerabilities[] per-instance)
                            │      ↓
                            │      T021 (--vex-propagation-mode flag)
                            │      ↓
                            │      T023, T024 (US2 tests, depend on T019-T022)
                            │
                            ├─→ T025 (trace-binding CLI), depends on T015 (reuses verify subroutine)
                            │      ↓
                            │      T026 (US3 tests)
                            │
                            └─→ T027 (--bind-to-source flag), depends on T005-T007 (reuses hash + extractors + annotation)
                                   ↓
                                   T028 (US4 tests)
                                   ↓
                                   T028b (reference fixture set for SC-004, depends on T005's pinned-vectors)
                                   ↓
                                   T029, T030, T031 (Polish docs — parallel; T029 references T028b's fixtures)
                                   ↓
                                   T032 (golden regen, depends on all emission code being final)
                                   ↓
                                   T033 (pre-PR gate)
                                   ↓
                                   T034 (PR)
```

## Format validation

All 35 tasks follow the required checklist format. Setup (T001-T002), Foundational (T003-T008), US1 (T009-T018), US2 (T019-T024), US3 (T025-T026), US4 (T027-T028), Reference fixtures (T028b — sits between US4 and Polish; carries no `[US#]` because it's a published artifact for external verifiers, not user-story implementation work), Polish (T029-T034). Every US-phase task carries the `[US#]` story label; every `[P]`-marked task is genuinely parallelizable across different files or independent code paths.

## MVP scope

**US1 alone delivers the user's primary verification ask** ("can we verify image's foo == source's foo?"). It covers SC-001 (≥95% verifiable bindings), SC-005 (`--bind-to-source` populates strength labels — though that's specifically delivered by US4 and US1 alone takes consumer-supplied bindings), SC-006 (binding-unknown reasons surfaced), SC-007 (algo determinism).

**US2 is the user's second framing** ("VEX propagation must respect binding strength"). SC-002, SC-003 (the worked example), and SC-008 (back-compat opt-out flag) are all US2-delivered.

US1 + US2 together = the milestone's MVP. US3 (operator triage) and US4 (first-party automatic emission) are post-MVP polish that lands in the same milestone for atomicity but could ship independently if scope had to compress.

## Parallel execution opportunities

- **T003–T008** (Foundational) — most are independent: T003/T004/T005/T006/T007 touch different files and have no inter-dependencies; T008 modifies an existing file (the catalog) and can be done in parallel with the rest.
- **T009–T014** (US1 emission) — split into 3 format-specific paths (CDX, SPDX 2.3, SPDX 3) plus document-level constructs; all 6 tasks parallel after T003-T008 complete.
- **T029–T031** (Polish docs) — three independent doc files.
- **T017, T018, T023, T024, T026, T028** (per-US tests) — independent test files; can be authored in parallel with their respective US implementations (TDD-style if the implementer prefers).

## Independent test criteria (per user story)

- **US1**: T017 produces a triple-format SBOM with `mikebom:source-document-binding` annotations on every first-party component; T018 round-trips through `mikebom sbom verify-binding` correctly (clean → exit 0; wrong source → exit non-zero with explicit reason).
- **US2**: T023 strict-mode refusal exits non-zero with refusal annotation; T024 caveated-mode produces the worked-example output (instance A clean, instance B caveated, aggregate `affected`).
- **US3**: T026 produces correct trace output for one-bound, no-bound, and multi-instance cases.
- **US4**: T028 confirms `--bind-to-source` populates strength on matching components, fails fast on missing source SBOM, and writes `Unknown { reason: "source-not-found-in-bind-target", ... }` for unmatched components.

## Closing context

This milestone is the largest since 053/055 — 34 tasks, 4 user stories, ~10 new source files, 6 new integration tests, 2 new doc files (`cross-tier-binding.md` plus the design-notes pointer), and one breaking change in CLI default behavior. Implementers should expect a multi-day implementation arc; T029 (the published doc) is best authored alongside the code rather than left to the end so the contract clauses stay in sync with the implementation as it evolves.

The milestone closes the user's two specific worries: (1) "we cannot verify the binary running in the image matches the version that the source or build SBOM is built for" — US1 + US4; (2) "a vulnerability in the source has a VEX against it but the image actually has the vuln through some other path" — US2 + the per-instance VEX architecture documented in `contracts/openvex-instance-identifiers.md` C-3 / C-4.
