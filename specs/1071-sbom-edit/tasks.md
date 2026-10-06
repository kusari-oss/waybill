---
description: "Tasks for milestone 1071: edit an emitted SBOM (filter, redact, derivation record, signature chain)"
---

# Tasks: Edit an emitted SBOM

**Input**: `specs/1071-sbom-edit/` (spec.md, plan.md, research.md, data-model.md, contracts/cli.md, quickstart.md)

**Tests**: Required. The success criteria are test-defined: SC-001…SC-008, the tamper matrix, the redaction search, the parity agreement. Write each test before its code and watch it fail.

**Organization**: by user story. US4 (re-identification) is the next milestone, so it has no tasks here; FR-014 and FR-015 only reserve room for it in the model.

## Format: `[ID] [P?] [Story] Description`

`W` = `waybill-cli/src`, `T` = `waybill-cli/tests`. Every new test module carries `#[cfg(test)] #[cfg_attr(test, allow(clippy::unwrap_used))]`.

---

## Phase 1: Setup

- [ ] T001 Promote `hmac = "0.12"` to a direct dependency in `waybill-cli/Cargo.toml`, matching the version already in `Cargo.lock` through sigstore. Afterwards, `git diff Cargo.lock` must add no package (research R5). Create the `W/edit/` module (`mod.rs`, `select.rs`, `cdx.rs`, `spdx23.rs`, `spdx3.rs`, `redact.rs`, `derivation.rs`) and register `mod edit;` wherever `W/main.rs` declares its modules.
- [ ] T002 [P] Build the test fixtures in `T/fixtures/sbom_edit/`. Generate the CycloneDX, SPDX 2.3 and SPDX 3 outputs of one scan of a small fixture project that contains:
  - a dev-only and a test-only dependency;
  - a component in the middle of the dependency graph that has dependencies (for bridging);
  - internal file paths;
  - a URL on `*.corp.acme.example`;
  - a package named `@acme/internal-*`;
  - a vulnerability entry on a dev dependency.

  Record the generating command in a `README.md` there, so the fixtures can be regenerated.

---

## Phase 2: Foundational (blocks all stories)

- [ ] T003 In `W/edit/mod.rs`, define per data-model.md:
  - `EditOp { action, selector, params }`;
  - `Action::{DropComponents, DropAnnotations, Redact}`, with `ReIdentify` only as a commented reservation (FR-015);
  - `RedactClass::{Paths, Hosts, Names}` and `RedactMode::{Remove, Pseudonymise}`;
  - `EditReport { per-op matched/changed }`.

  All are `serde` Serialize + Deserialize (FR-014, the policy-file vocabulary).
- [ ] T004 In `W/edit/select.rs`:
  - `Selector` parsing from the `;`-separated `key=v1,v2` syntax (contracts/cli.md);
  - glob matching via `globset`, and `re:` regex via `regex`;
  - AND across terms, OR within a term;
  - an empty selector is an error.

  Unit tests cover parse errors, each key, AND/OR, `re:` names, and the empty selector.
- [ ] T005 In `W/edit/mod.rs`, define the `SbomAdapter` trait, with the methods in data-model.md "Per-format adapter". Add `detect(doc: &Value) -> Result<Format>` per research R10:
  - `@graph` is SPDX 3;
  - `spdxVersion` + `packages` is SPDX 2.3;
  - `bomFormat == "CycloneDX"` with `specVersion == "1.6"` is CycloneDX;
  - anything else is an `Unsupported` error.

  Mirror the detection in `W/binding/verify.rs::walk_image_components`. Unit-test each format and each refusal.
- [ ] T006 In `W/edit/mod.rs`, write the pipeline `run(input_bytes, ops, opts) -> Result<(output_bytes, EditReport)>`:
  1. detect the format and build its adapter;
  2. apply the operations in order;
  3. run the post-conditions (T007);
  4. attach the derivation record (US3: until T030, a no-op hook);
  5. serialise with sorted keys and 2-space indentation;
  6. sign (US3).

  Refuse any operation whose selector matches the document's root or subject (`metadata.component`, the SPDX `DESCRIBES` target, the SPDX 3 `rootElement`), with an error naming it. Nothing is written on any error (FR-016).
- [ ] T007 In `W/edit/mod.rs`, add the post-condition checks:
  - (a) no identifier of a dropped component appears anywhere in the serialised output (string search);
  - (b) every reference resolves to an element that exists in the output: CycloneDX `dependsOn`/`ref`/`assemblies` and `affects[].ref` against `bom-ref`s; SPDX 2.3 relationship ends against `SPDXID`s (and `DocumentRef-*:` prefixes); SPDX 3 `from`/`to`/`subject` against `spdxId`s or `import` entries. This is structural only. Schema and `spdx3-validate` conformance are test-time (T008, T015, T025), since waybill ships no runtime validator (Principle I; analysis H1);
  - (c) redaction leaks: wired in T024.

  On failure, return an error with the first offending path.

**Checkpoint**: the selector parses and matches, the formats are detected, and a no-op edit of each fixture reproduces it **byte-identically** (research R2). Add that as a test: `T/sbom_edit_noop.rs`.

---

## Phase 3: User Story 1 — Filter (P1) 🎯 MVP

**Goal**: drop components by selector, cleanly, with bridging and downgraded completeness, and drop annotations by namespace. All three formats agree.
**Independent Test**: the spec's US1 test, against T002's fixtures.

- [ ] T008 [P] [US1] Tests first, in `T/sbom_edit_filter.rs`, for each of the three fixture formats:
  - drop `scope=development,test`; the output validates (conformance) and contains no identifier of a dropped component (SC-001);
  - the vulnerability on the dev dependency is gone;
  - the bridged component's dependents now depend on its dependencies (SC-007: everything reachable before and not dropped is still reachable);
  - the affected dependency lists no longer claim `complete`;
  - `--drop-annotations waybill:` leaves no `waybill:` annotation except the protected set, `waybill:generation-context` (C21) and `waybill:derivation`. Assert C21 is still present with its original value (constitution Operating Modes, requirement 1; analysis C1);
  - the SC-002 diff: apart from the targeted content, the consequential changes and the derivation record, input and output are identical.
- [ ] T009 [P] [US1] Write the cross-format test in `T/sbom_edit_parity.rs`. Apply the same operations to the three fixture outputs, then run every `waybill::parity::extractors::EXTRACTORS` row over the three results. They must agree with the same directionality rules the parity check uses (FR-013). Reuse the comparison logic behind `waybill sbom parity-check`; read `W/cli/parity_cmd.rs` for the entry point.
- [ ] T010 [US1] In `W/edit/cdx.rs`, write the CycloneDX adapter: `components()` and `ComponentView`, reading the research R3 fields (`purl`, `waybill:lifecycle-scope` then `scope`, `waybill:sbom-tier`, `type` + `waybill:component-role`, `name`), with nested `components[]` walked recursively.
- [ ] T011 [US1] In `W/edit/cdx.rs`, write `drop(ids)` per research R4:
  - remove the components (nested too) and their `dependencies[]` entries;
  - remove their `dependsOn` members, bridged per R4 with its scope rule;
  - trim `compositions[]` `assemblies` and `dependencies`;
  - trim `vulnerabilities[].affects`, removing a vulnerability whose `affects` empties.

  Then `downgrade_completeness(changed)` moves each changed `bom-ref` out of `aggregate: complete` into an `incomplete` composition. `remove_annotations(ns)` handles `properties[]` at document and component level, sparing the protected set. Define `PROTECTED_ANNOTATIONS = ["waybill:generation-context", "waybill:derivation"]` once in `W/edit/mod.rs`, and consult it in all three adapters (analysis C1).
- [ ] T012 [P] [US1] In `W/edit/spdx23.rs`, write the SPDX 2.3 adapter:
  - **`components()`** reads `packages[]` (PURL from `externalRefs`; scope from the `waybill:lifecycle-scope` annotation envelope, else the `*_DEPENDENCY_OF` types).
  - **`drop`** removes `packages[]`, `files[]`, every relationship naming a dropped `SPDXID`, annotations on them, and `hasExtractedLicensingInfos` entries no longer referenced. Bridging covers `DEPENDS_ON` and the reversed `*_DEPENDENCY_OF` types, kept in their direction.
  - **Completeness:** the `waybill:graph-completeness` document annotation is set to incomplete for the affected ecosystems.
  - **`remove_annotations`** uses the `waybill-annotation/v1` envelope; reuse the envelope parsing in `W/parity/extractors/common.rs`.
- [ ] T013 [P] [US1] In `W/edit/spdx3.rs`, write the SPDX 3 adapter:
  - **`components()`** reads `software_Package` elements (PURL from `software_packageUrl`; scope from `LifecycleScopedRelationship` edges into the package, else the annotation).
  - **`drop`** removes the elements and trims relationships' `to`, removing a relationship whose `to` empties. Bridging covers grouped `dependsOn` (milestone 1069 shape) and keeps that grouping. It also removes `Annotation`, `LicenseExpression` and VEX/vulnerability elements left without a subject.
  - **Completeness:** set `completeness: incomplete` on changed `dependsOn` relationships, and downgrade the `waybill:graph-completeness` annotation (same as T012).
  - **`remove_annotations`** removes `Annotation` elements by envelope.
- [ ] T014 [US1] Write the `waybill sbom edit` command in `W/cli/edit.rs`, wired as `SbomSubcommand::Edit(EditArgs)` in `W/cli/sbom_cmd.rs`. Flags per contracts/cli.md: `<INPUT>`, `-o/--output`, repeatable `--drop` and `--drop-annotations`. Signing flags come in T031; `--redact` in T022.
  - Each flag parses into an `EditOp`, in the order given (clap keeps the order of repeated flags per flag; to preserve cross-flag order, read `ArgMatches` indices).
  - The per-operation report goes to stderr.
- [ ] T015 [US1] Make T008, T009 and the T007 checkpoint test pass. Then add the SC-001 corpus test: in `T/sbom_edit_filter.rs`, edit a committed public-corpus golden (`T/fixtures/public_corpus/<target>/{cdx,spdx-2.3,spdx-3}.json`), dropping `scope=development,test`, and require conformance plus no dangling reference in all three. Pick a target whose golden has dev or test components (`jq` to find one), and note which.

**Checkpoint**: US1 is shippable. Filtering works in all three formats, which stay in agreement.

---

## Phase 4: User Story 2 — Redact (P1)

**Goal**: remove or pseudonymise paths, hosts and names everywhere, fail closed on a leak, and record only categories and counts.
**Independent Test**: the spec's US2 test (SC-003, SC-008).

- [ ] T016 [P] [US2] Tests first, in `T/sbom_edit_redact.rs`, for each fixture format:
  - `--redact paths`, `--redact 'hosts:pseudonymise=*.corp.acme.example'` and `--redact 'names:pseudonymise=@acme/*' --redact-key-file k`: searching the output for every original value finds 0 matches (SC-003). The values come from the fixture README;
  - every rewritten PURL still parses as a PURL;
  - the redacted components keep distinct identities;
  - pseudonymising with the same key in two documents gives identical tokens, a different key gives none of them, and no token equals an original (SC-008);
  - `names:remove` gives `redacted-<n>` ordinals, distinct per component;
  - pseudonymising without `--redact-key-file` is an error with nothing written.
- [ ] T017 [US2] In `W/edit/redact.rs`, write `pseudonym(key, class, value)`: `"redacted-"` + lowercase base32 (`data-encoding`) of the first 10 bytes of HMAC-SHA256(key, class ‖ 0x00 ‖ value) (research R5). Unit tests: determinism, key sensitivity, class sensitivity, and that the token is a valid PURL name segment.
- [ ] T018 [US2] In `W/edit/redact.rs`, write the value collection. Each adapter's `redaction_targets(class, pattern)` returns the concrete strings to replace, from research R5's fields:
  - **paths:** evidence and occurrence locations, `waybill:source-files`, file-tier names, SPDX `files[].fileName` and `packageFileName`;
  - **hosts:** URL-valued fields whose host matches;
  - **names:** component `name` and the PURL name segment matching the pattern.

  Implement the per-adapter part in `W/edit/{cdx,spdx23,spdx3}.rs`.
- [ ] T019 [US2] In `W/edit/redact.rs`, write `rewrite_strings(doc, map, class)`. It walks every string in the document and replaces each collected value under the class boundary rules (research R5):
  - paths: as a whole value, or as a prefix of a longer path;
  - hosts: within a URL authority;
  - names: as a whole name field, or as a PURL name segment.

  PURL strings are parsed, have their name segment replaced, and are re-encoded with `waybill_common::types::purl::Purl`. `bom-ref`s and SPDX ids that embed a redacted name are rewritten consistently with every reference to them.
- [ ] T020 [US2] In `W/edit/redact.rs`, write the remove mode: delete optional fields; set required fields to the marker; names become `redacted-<n>` ordinals in document order, so identities stay distinct.
- [ ] T021 [US2] Write the operations' derivation categories, `redact-paths`, `redact-hosts` and `redact-names`, with their matched/changed counts, in `EditReport` (`W/edit/mod.rs`). Counts are in the format-independent units in data-model.md (distinct values matched and replaced; components selected and removed; annotation entries per component and field), so the three formats' records agree (analysis M1).
- [ ] T022 [US2] Add `--redact <class>[:<mode>][=<pattern>]` and `--redact-key-file <path>` to `W/cli/edit.rs` per contracts/cli.md. The default modes are `remove` for paths and `pseudonymise` for hosts and names. Read the key file without logging its contents. A missing key with pseudonymisation is an error.
- [ ] T023 [US2] Docs: in `docs/user-guide/sbom-edit.md`, a "What redaction does not hide" section (FR-017). Content hashes, version strings, licence data and dependency-graph shape can still identify a redacted component. Redaction is not anonymisation. Pseudonyms are linkable by anyone holding the key. The derivation record's original hash lets anyone holding a candidate original confirm it was the source (analysis L1).
- [ ] T024 [US2] Add the redaction leak post-condition (T007 part c) in `W/edit/mod.rs`. Search the serialised output for every collected original value, and fail without writing if any remains. Add a test that forces a leak by stubbing a field the collector misses (a `#[cfg(test)]` injection), and confirms the command refuses to write.

**Checkpoint**: US2 is shippable. A redacted document contains none of the redacted values, and the command refuses to write rather than leak.

---

## Phase 5: User Story 3 — Derivation record and signature chain (P1)

**Goal**: every edited document states its derivation natively and in `waybill:derivation`, embeds the original's signature, can be re-signed, and is verifiable back to its originals.
**Independent Test**: the spec's US3 test (SC-004, SC-005).

- [ ] T025 [P] [US3] Tests first, in `T/sbom_edit_chain.rs`:
  - **per format:** the native link is present (data-model.md table); `waybill:derivation` is present, with the original's SHA-256, the categories applied, and none of the removed or redacted values (SC-005, by search); the output validates (SPDX 3 with `spdx3-validate` when available, following the existing `WAYBILL_REQUIRE_SPDX3_VALIDATOR` convention).
  - **The tamper matrix (SC-004)**, with a static key generated per test like `T/cisa_2026_signing.rs::ephemeral_keypair`. Sign the original, edit and sign. `verify-chain --original --key` passes. It fails when:
    - (a) an original byte is changed;
    - (b) a derivative byte is changed;
    - (c) a different original is supplied;
    - (d) the derivative's signature is removed;
    - (e) the embedded original signature is altered.
  - **A two-step chain** (edit an edited document) verifies with two `--original`s.
  - **An unsigned original** is recorded as `kind: none`, and verification reports `Unsigned`, not `Verified`.
  - **Signature material containing a redacted value** (analysis H2): sign the original with a static key whose JSF or DSSE material is crafted to contain a redacted host (e.g. in the public-key `kid` or a DSSE payload field), and redact that host. The record has `embedded: false`, `material_sha256` and `reason: contains-redacted-values`. The edit still succeeds, with no leak. `verify-chain` needs `--original-signature` and verifies it against `material_sha256`.
- [ ] T026 [US3] In `W/edit/derivation.rs`, write `DerivationRecord` per data-model.md: canonical sorted-key JSON, the `schema` `waybill-derivation/v1`, `ancestors` copied from the original's own record if any, `tool`, and `created`.
- [ ] T027 [US3] In `W/edit/derivation.rs`, write `original_signature(input_path, input_doc, override)`, returning `{kind, material}`:
  - the CycloneDX root `signature` object (JSF);
  - otherwise a sidecar at `<input>.sig.bundle.json` (Sigstore bundle) or `<input>.sig.json` (DSSE), or the `--original-signature` path;
  - otherwise `none`.

  Before embedding, search the material for every value this edit redacts. On a hit, return `{kind, embedded: false, material_sha256, reason: "contains-redacted-values"}` and do not embed (analysis H2). Read the sidecar naming from `W/cli/scan_cmd.rs` (around the `sign_sbom_bytes_to_sidecar` call, line ~5433) so the conventions match exactly.
- [ ] T028 [US3] In `W/edit/{cdx,spdx23,spdx3}.rs`, write `attach_derivation(record)`, adding the native link (data-model.md table) and the `waybill:derivation` annotation:
  - CycloneDX root `externalReferences` `type: bom` and a root `properties` entry;
  - SPDX 2.3 `externalDocumentRefs` `DocumentRef-original` + an `AMENDS` relationship + a document annotation in the `waybill-annotation/v1` envelope;
  - SPDX 3 an `import` ExternalMap + an `amendedBy` Relationship + an `Annotation` on the `SpdxDocument`.

  For a second edit, a fresh `DocumentRef-original-<n>` and a fresh IRI avoid colliding with the earlier link.
- [ ] T029 [P] [US3] Add catalogue row **C194** `waybill:derivation` (document scope, all three formats, `SymmetricEqual`) to `docs/reference/sbom-format-mapping.md`, following C193's wording pattern. The row states that parity compares only the format-independent projection (`schema`, `operations`, the ancestors' `operations`), because `original.sha256`, `format` and the signature describe three different original files (analysis H3).

  Write `c194_cdx` / `c194_spdx23` / `c194_spdx3` by hand in `W/parity/extractors/{cdx,spdx2,spdx3}.rs`, not with the plain `*_anno!` macros. Each reads the annotation value, parses the JSON and returns the canonicalised projection. Add the `EXTRACTORS` entry in `W/parity/extractors/mod.rs` after C193. Then `T/sbom_format_mapping_coverage.rs` passes.
- [ ] T030 [US3] Wire the derivation step into the T006 pipeline in `W/edit/mod.rs`: compute the SHA-256 of the input **bytes**, build the record from `EditReport`, call `attach_derivation`. This runs after the operations and before serialisation and signing.
- [ ] T031 [US3] Add signing to `W/cli/edit.rs`: the same flags as `sbom scan` (`--sign-key`, `--sign-key-passphrase-env`, keyless `--sign` and its companions; read their definitions in `W/cli/scan_cmd.rs` ~786–856) and `--original-signature`.
  - **CycloneDX:** `strip_existing_signature`, then `sign_cdx_document_in_place` (`W/sbom/signer.rs`). Keyless: the m778 detached bundle plus `inject_signature_reference` (`W/cli/scan_cmd.rs:6078`).
  - **SPDX:** `sign_spdx_bytes_to_dsse` / `sign_sbom_bytes_to_sidecar`.

  Move any helper used from `scan_cmd.rs` into `W/sbom/signer.rs` rather than duplicating it.
- [ ] T032 [US3] Write `waybill sbom verify-chain` in `W/cli/verify_chain.rs`, wired as `SbomSubcommand::VerifyChain`, producing the `ChainReport` per data-model.md:
  - **Own signature:** JSF verify, using the canonicalise-with-`value:""` + `CosignVerificationKey` path from `W/sbom/signer.rs` tests, lifted into a non-test `verify_cdx_jsf` in `W/sbom/signer.rs`; DSSE via `W/attestation/verifier.rs::verify_signature`. Keyless bundles: check the bundle's artifact digest against the document hash, and return `Delegated { cosign verify-blob … }` (the same command shape m779 prints, `W/attestation/signer.rs` ~337–390).
  - **Each link:** the supplied original's SHA-256 against `original.sha256`; the original's embedded signature against the original's bytes.
  - **Output:** `--original` repeatable, newest first; `--key` repeatable; `--json`. The exit status follows `ok`.
- [ ] T033 [US3] Make T025 pass.

**Checkpoint**: US3 is shippable. Integrity survives editing, as a verifiable chain.

---

## Phase 6: Polish & cross-cutting

- [ ] T034 SC-006 measurement (target: ratio ≤ 1.0, i.e. editing takes no longer than generating): time `waybill sbom edit` (the quickstart's operations) on the `image-postgres16` corpus goldens (CycloneDX 2.1 MB, plus its SPDX outputs). Record it against the corpus run's generation time for that target, if the corpus harness logs it, or against a local scan if the image is available. Write `specs/1071-sbom-edit/measurements/performance.txt`, giving the ratio and both raw numbers, labelled as measured. A ratio above 1.0 fails SC-006, and is reported rather than adjusted (analysis M2).
- [ ] T035 [P] Write `docs/user-guide/sbom-edit.md` (the T023 section included), from quickstart.md: the model ("generate once, derive what you distribute"); operations and selectors; redaction modes and key handling; the derivation record and native links; `verify-chain` and what "delegated" means; a pointer to the next milestone (policy files, re-identification). Add `sbom edit` and `sbom verify-chain` to `docs/user-guide/cli-reference.md`.
- [ ] T036 [P] `CHANGELOG.md` `[Unreleased]` entry under "Added": `waybill sbom edit` (filter, redact, derivation record, signing) and `waybill sbom verify-chain` (#1129).
- [ ] T037 Run `./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo EXIT=$?`. Require EXIT=0, `>>> all pre-PR checks passed.` and every `test result: ok`.
- [ ] T038 After merge, `cargo clean`.

---

## Dependencies & Execution Order

- **T001** → **T002** (∥ with T003–T005) → **Phase 2 (T003–T007)** → stories.
- **US1 (T008–T015)** first: US2 and US3 extend its adapters and CLI, and need T014's command.
- **US2 (T016–T024)** and **US3 (T025–T033)** can proceed in either order after US1. Both edit the adapters, so do them one at a time in each adapter file. The T029 catalogue row is independent.
- **Polish (T034–T038)** last; T035 and T036 can run in parallel.

## Parallel Opportunities

- **Phase 1:** T002 ∥ Phase 2.
- **US1:** the T008 and T009 tests ∥ each other; the T012 and T013 adapters ∥ each other, once T010/T011 have fixed the trait in practice.
- **US2:** the T016 tests ∥ T017.
- **US3:** the T025 tests ∥ T029 (catalogue).
- **Polish:** T035 ∥ T036.

## Implementation Strategy

**MVP = Phase 2 + US1:** a working, three-format, self-consistent filter with the post-conditions. Then US2 (redaction, which unblocks distribution), then US3 (the chain). Each checkpoint is a mergeable state if the milestone is split into several PRs. The default is one PR.
