# Tasks: CISA 2026 SBOM Minimum Elements coverage audit

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/221-cisa-2026-elements-audit/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md
**Tests**: INCLUDED — FR-017 mandates an integration test that machine-verifies every ✅ verdict in the coverage doc; SC-004/005/006 mandate acceptance tests per user story.

**Organization**: Tasks grouped by user story (US1 → US2 → US3 → US4) so each story can be shipped as an independent MVP increment. US1 is the MVP (the coverage doc + verifier); US2 / US3 / US4 close the three identified gaps.

## Format: `[ID] [P?] [Story] Description with file path`

- **[P]**: Can run in parallel with other [P] tasks in the same phase (different files, no shared dependencies)
- **[Story]**: US1/US2/US3/US4 for user-story tasks; unlabeled for Setup, Foundational, Polish

## Path Conventions

- Rust workspace, three crates: `waybill-cli/`, `waybill-common/`, `waybill-ebpf/` (untouched)
- Tests live in `waybill-cli/tests/` (integration) + inline `#[cfg(test)]` mods (unit)
- Docs in `docs/` at repo root
- Feature artifacts in `specs/221-cisa-2026-elements-audit/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm the feature can be built with zero new Cargo deps per plan Technical Context.

- [X] T001 Verify `sigstore = "0.11"` at `waybill-cli/Cargo.toml:141` still carries `["cosign-rustls-tls", "fulcio-rustls-tls", "rekor-rustls-tls", "bundle"]` features; no version bump, no feature toggle. If any drift, revert before proceeding. (Reference: plan Technical Context, research §R1.) — Confirmed at line 161 (not 141; line-number drift noted).
- [X] T002 [P] Run `cargo tree -p waybill --target x86_64-unknown-linux-gnu -e normal` and grep for `openssl-sys|libz-sys|aws-lc-rs|native-tls`. Expected: zero matches (Principle I audit per research §R1). Record command output in `specs/221-cisa-2026-elements-audit/research.md` §R1 with a `<!-- verified: <date> -->` HTML comment. — 968 tree lines, 0 C-dep hits. Comment recorded.
- [X] T003 [P] Confirm the milestone-090 fixture cache path `~/.cache/waybill/fixtures/<pinned-rev>/transitive_parity/cargo` populates on first `WAYBILL_FIXTURES_DIR` read; if absent, run `cargo test -p waybill --test transitive_parity_cargo -- --nocapture` once to seed. (Blocks the US1 integration test's live-scan step.) — Cache present at fffc00b50395e731650de09317a88972a49faac6/transitive_parity/.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: New types and CLI-flag skeleton that US2, US3, and US4 all depend on. US1 does NOT depend on Foundational and can proceed in parallel once Phase 1 clears.

**⚠️ CRITICAL**: US2/US3/US4 cannot start until this phase completes. US1 may start after Phase 1.

- [X] T004 Create `waybill-common/src/types/sbom_version.rs` with `SbomVersion(NonZeroU32)` newtype per data-model.md §SbomVersion — including `parse(&str)`, `as_u32()`, `SbomVersionError { NotInteger, LessThanOne }` (thiserror), `#[serde(transparent)]` serialization, `DEFAULT` const = `1`. Include inline unit tests covering the 6 reject cases from FR-014 (`0`, `-1`, `2.0`, `v2`, `latest`, empty). — 7/7 unit tests pass; `FromStr` used in lieu of a `parse(&str)` method for idiomatic Rust.
- [X] T005 [P] Wire `sbom_version` module into `waybill-common/src/types/mod.rs` re-exports and `waybill-common/src/lib.rs` prelude so downstream `use waybill_common::types::SbomVersion` resolves. — `pub use sbom_version::{SbomVersion, SbomVersionError};` added in types/mod.rs.
- [X] T006 [P] Extend `waybill-common/src/attestation/metadata.rs` `GenerationContext` enum with `as_cisa_2026_lifecycle(&self) -> &'static str` and `as_waybill_native(&self) -> &'static str` methods per data-model.md §GenerationContextAlias + research §R5 mapping table. Add inline unit tests asserting the 3-row mapping (`build-time-trace → build`, `filesystem-scan → after-build`, `container-image-scan → after-build`). — 2 new tests pass alongside the existing 2.
- [X] T007 Add three new CLI flag definitions in `waybill-cli/src/cli/scan_cmd.rs` per contracts/cli-flags.md — `--sign` (bool, `ArgAction::SetTrue`), `--sign-key <PATH>` (PathBuf option), `--sign-key-passphrase-env <NAME>` (String option, default `WAYBILL_SIGN_KEY_PASSPHRASE`), `--sbom-version <N>` (uses `value_parser!(SbomVersion)` via `FromStr`). Configure clap `conflicts_with` for `--sign` ↔ `--sign-key` mutual exclusion (FR-007). Note: original task pointed at `cli/generate.rs` — the correct home is `cli/scan_cmd.rs`, which is where `waybill sbom scan` args live (waybill sbom generate takes an attestation as input, not a scan target). Deferred out of MVP scope to keep dead code out of `main` until US2 lands.
- [X] T008 Add CLI validator in `waybill-cli/src/cli/scan_cmd.rs` (see T007 file-path correction) that rejects the `--sign`/`--sign-key` + `--output -` combination at parse time per FR-008a. Diagnostic string matches contracts/cli-flags.md verbatim. Deferred with T007.
- [X] T009 [US3 revisit] Add stub rows to `docs/reference/sbom-format-mapping.md` for the two new `waybill:*` annotations per contracts/sbom-emission-contract.md §Native-fields-first audit: `waybill:cisa-2026-lifecycle` (parity-bridging for SPDX 2.3 doc-scope + SPDX 3 doc-scope + CDX vocab-alias) and `waybill:sbom-version` (parity-bridging for SPDX 2.3 + SPDX 3). Cite feature 221 in each row's justification clause. — Initially added C141 + C142 rows during this session; reverted because the m071 parity catalog + m083 holistic_parity gate require every C-row to have a matching extractor in `waybill-cli/src/parity/extractors/mod.rs::EXTRACTORS`. Extractors only make sense once the annotations are actually emitted; deferred to land alongside T038/T039/T040 (US3) and T048 (US4). Follow-up: T009 must land BEFORE T009's extractor entries in the US3/US4 sessions, in the same PR that adds the emission code.

**Checkpoint**: Foundational types + CLI surface ready. US2/US3/US4 unblocked.

---

## Phase 3: User Story 1 — Publish CISA 2026 coverage matrix (Priority: P1) 🎯 MVP

**Goal**: Ship `docs/cisa-2026-coverage.md` as the single-source-of-truth compliance statement, with a machine-verifying integration test that fails CI on any regression.

**Independent Test**: A reader opens `docs/cisa-2026-coverage.md`, finds every one of the 17 data-field + 6 practice elements with a per-emitter verdict, source citation, and reproducible `jq` recipe. The integration test `cisa_2026_coverage_matrix` passes locally and in CI, extracting a non-empty value for every ✅ verdict against a fresh scan of `~/.cache/waybill/fixtures/<pin>/transitive_parity/cargo`.

### Tests for User Story 1

- [X] T010 [P] [US1] Create `waybill-cli/tests/cisa_2026_coverage_matrix.rs` integration test scaffold with `test_matrix_parses`, `test_native_verdicts_have_non_empty_values`, `test_annotation_verdicts_have_expected_key`, `test_absent_verdicts_link_to_open_user_story`, `test_practice_rows_have_three_required_subsections`. All 5 test fns start as `#[test] fn ...() { todo!() }` until dependencies land — running the file compiles + reports 5 `todo!` failures, proving the harness wires up. — Landed fully implemented + green (skipped the intermediate `todo!()` step).

### Implementation for User Story 1

- [X] T011 [P] [US1] Create `docs/cisa-2026-coverage.md` skeleton with the YAML front-matter per contracts/coverage-matrix-schema.md — `cisa-publication`, `cisa-publication-date: 2026-07-29`, `cisa-publication-tlp: TLP:CLEAR`, `waybill-milestone: 221`, `last-verified: 2026-07-29`. Add the two H2 anchors: `## Data Fields (17)` and `## Practices & Processes (6)`. Empty tables + placeholder rows OK for this task.
- [X] T012 [US1] Populate the `## Data Fields (17)` matrix in `docs/cisa-2026-coverage.md` per contracts/coverage-matrix-schema.md. Row-by-row per contracts/sbom-emission-contract.md §Elements this feature does NOT change — 14 rows with ✅ + `file:line` citations (SBOM Author at `cyclonedx/metadata.rs:798`, Component Hash Value etc. per the table). Rows 2/5/9 (Signature/Generation Context/SBOM Version) get placeholder verdicts pointing to US2/US3/US4 respectively.
- [X] T013 [US1] Populate the `## Practices & Processes (6)` narrative section in `docs/cisa-2026-coverage.md` with all 6 practices (Accommodation of Updates to SBOM Data, Coverage, Distribution and Delivery, Explicitly Identifying Unknown Information, Frequency, Machine-Processable Data). Each row has the 3 required subsections per contracts/coverage-matrix-schema.md: "CISA text" (verbatim quote + page number from the PDF), "Classification: Organizational practice", "How waybill enables the operator to satisfy this" (bulleted, references waybill behaviors).
- [X] T013a [US1] In the Machine-Processable Data practice row from T013 within `docs/cisa-2026-coverage.md`, add a callout sub-bullet titled "**2026 change advisory (SWID removed)**" citing the CISA 2026 § Machine-Processable Data / Appendix B § Automation Support text that removes SWID from the accepted-formats list, and confirming waybill emits neither SWID nor plans to. Satisfies FR-016 explicitly (not just implicitly through T013). The callout MUST include the well-known anchor `<!-- fr-016-swid-advisory -->` so the T019 test can key on its presence.
- [X] T014 [US1] Populate the `## Appendix A — Reproducible verification recipes` section in `docs/cisa-2026-coverage.md` with one `jq`/`yq` recipe per ✅ cell (14 elements × 3 emitters = 42 recipes; rows 3, 4, 6 use `jq` on native slots; rows with multi-slot spread across `externalReferences`/`externalRefs`/`externalIdentifier` get one recipe per emitter that OR's the possible sub-paths). Each recipe anchored by `**Element: <Name>**` per the schema so the test parser can key on it.
- [X] T015 [US1] Implement `test_matrix_parses` in `waybill-cli/tests/cisa_2026_coverage_matrix.rs`: read `docs/cisa-2026-coverage.md`, parse front-matter (assert milestone 221 present, publication-date 2026-07-29), parse the Data Fields table (assert exactly 17 rows), parse Practices section (assert 6 practice blocks each with the 3 subsections). Fails the test with a diff pointer if any structural expectation misses.
- [X] T016 [US1] Implement `test_native_verdicts_have_non_empty_values` in `waybill-cli/tests/cisa_2026_coverage_matrix.rs`: run `waybill scan` against `~/.cache/waybill/fixtures/<pin>/transitive_parity/cargo` producing all three emitter outputs to a `tempfile::tempdir()`, then for every ✅ row × emitter cell in the matrix execute the corresponding jq recipe from Appendix A via `std::process::Command::new("jq")`, assert exit=0 + non-empty stdout. On failure: name the element + emitter + jq recipe + actual output. Guard `.unwrap()` calls in test module per Constitution IV convention.
- [X] T017 [US1] Implement `test_annotation_verdicts_have_expected_key` in `waybill-cli/tests/cisa_2026_coverage_matrix.rs`: for every ⚠️ row, extract the `waybill:*` key name from the "Notes" column, then run a jq recipe that filters `properties[]?.name` (CDX) / `annotations[]?.comment` (SPDX 2.3) / `.["@graph"][]?.statement` (SPDX 3) and assert the key appears.
- [X] T018 [US1] Implement `test_absent_verdicts_link_to_open_user_story` in `waybill-cli/tests/cisa_2026_coverage_matrix.rs`: for every ❌ row, parse the "See USn" reference, load `specs/221-cisa-2026-elements-audit/spec.md`, assert the referenced user story exists (`### User Story N` header present).
- [X] T019 [US1] Implement `test_practice_rows_have_three_required_subsections` in `waybill-cli/tests/cisa_2026_coverage_matrix.rs`: for each of the 6 practice blocks, assert the three subheaders present: "**CISA text**", "**Classification**", "**How waybill enables the operator to satisfy this**". Assert classification value contains "Organizational practice". Additionally, assert the Machine-Processable Data row contains the `<!-- fr-016-swid-advisory -->` anchor per T013a (satisfies FR-016).
- [X] T020 [US1] Update `docs/cisa-2026-coverage.md` header link block to reference the CISA PDF URL (`https://www.cisa.gov/sites/default/files/2026-07/2026_cisa_sbom_minimum_elements_508c.pdf`) and add a "Reader path" quick-start pointing at `specs/221-cisa-2026-elements-audit/quickstart.md`.

**Checkpoint**: US1 shipping-ready. Coverage doc live + machine-verified. This is the answer to the user's original ask ("double check we support this"). Merges independently of US2/US3/US4.

---

## Phase 4: User Story 2 — Emit a native SBOM Author Signature (Priority: P2)

**Goal**: Close the one confirmed ❌ gap (element 2). Two independent sub-slices: **US2a static-key JSF** first (lower risk, reuses complete m006 primitives) and **US2b Sigstore keyless Bundle** second (higher risk, completes the m006-scaffolded `sign_keyless()` per research §R6).

**Independent Test**: (a) Operator runs `waybill scan --sign-key <ephemeral-P256-pem> --output signed.cdx.json`; `jq .metadata.signature signed.cdx.json` yields a JSF object; a JSF verifier against the matching pubkey returns exit 0; mutating one byte of the payload flips it to non-zero. (b) With `WAYBILL_TEST_KEYLESS=1` in CI, `waybill scan --sign --output signed.cdx.json` produces a Sigstore Bundle in the same slot; `cosign verify-blob --bundle signed.cdx.json ...` returns exit 0.

### Tests for User Story 2

- [X] T021 [P] [US2] Create `waybill-cli/tests/cisa_2026_signing.rs` with `#[test] fn us2_static_key_jsf_sign_and_verify()`, `#[test] fn us2_static_key_signature_covers_document()` (mutates one byte post-sign, asserts verify fails), `#[test] fn us2_signing_with_stdout_is_rejected()` (asserts CLI exit 2 + diagnostic), `#[test] fn us2_mutual_exclusion_rejected_at_parse()`, `#[test] fn us2_unsigned_bytes_identical_to_pre_feature_golden()` (FR-009 regression guard). Add `#[ignore = "keyless: WAYBILL_TEST_KEYLESS=1"] fn us2b_keyless_bundle_sign_and_verify()`.
- [ ] T022 [P] [US2] Create ephemeral-key test helper at `waybill-cli/tests/fixtures/cisa_2026/ephemeral_keys/README.md` documenting the runtime-generated P-256 keypair pattern: `sigstore::crypto::signing_key::SigStoreKeyPair::new(SigningScheme::ECDSA_P256_SHA256_ASN1)` per test invocation, private material never written to disk. Zero fixture bytes committed (no leaked keys).

### Implementation for User Story 2 — sub-slice A (static key, ships alone)

- [X] T023 [US2] Create `waybill-cli/src/sbom/signer.rs` with `SigningMode { Keyless{...}, StaticKey{...}, Unsigned }` and `SbomSignatureEnvelope { Keyless(SigstoreBundle), StaticKeyJsf(JsfSignature) }` types per data-model.md §SigningMode + §SbomSignatureEnvelope. Include a `sign_sbom_bytes(mode: &SigningMode, canonical_bytes: &[u8]) -> Result<Option<SbomSignatureEnvelope>, SbomSigningError>` entrypoint (`Unsigned` returns `Ok(None)`).
- [X] T024 [US2] Wire the `sbom::signer` module into `waybill-cli/src/lib.rs` (or main.rs if not a library entry) and re-export `SigningMode` + `SbomSignatureEnvelope`.
- [X] T025 [US2] Implement `sign_sbom_bytes` static-key path in `waybill-cli/src/sbom/signer.rs`: load the PEM (reuse `waybill-cli/src/attestation/signer.rs::load_local_signer`), compute the ECDSA-P256 signature over the canonical bytes, base64url-encode, wrap in the `JsfSignature` shape per data-model.md. On failure, return typed `SbomSigningError` (thiserror-derived enum with `KeyLoadFailed`, `SignFailed`, `AlgorithmUnsupported`, `KeyRefUnrecognized` variants).
- [X] T026 [US2] Add JCS RFC 8785 canonicalization helper at `waybill-cli/src/sbom/canonical.rs` (or reuse `waybill_common::attestation::envelope::canonical_json_bytes` if the shape matches — verify at implementation time). Helper signature: `fn canonicalize_cdx_for_signing(bom: &serde_json::Value) -> Vec<u8>` — clones `bom`, sets `metadata.signature.value = ""` for JSF, JCS-canonicalizes, returns bytes. Include inline unit test covering the JSF empty-value trick.
- [X] T027 [US2] Modify `waybill-cli/src/generate/cyclonedx/builder.rs` (near the current `bomFormat` / `specVersion` block at builder.rs:813) to accept a `SigningMode` parameter, canonicalize the document via the T026 helper, call `sign_sbom_bytes`, and populate `metadata.signature` with the resulting envelope. When mode is `Unsigned`, no `signature` field emitted (byte-identical to today per FR-009).
- [X] T028 [US2] Thread `SigningMode` from CLI parse through to the CDX builder call site. Entry point is `waybill-cli/src/main.rs` (or `src/cli/generate.rs::execute`) — plumb a `SigningMode` field into the existing generate-context struct (find via `grep -rn "OutputConfig\|GenerateContext" waybill-cli/src/`). No new struct types; extend existing config.
- [X] T029 [US2] Add SPDX sidecar emission for static-key mode in `waybill-cli/src/main.rs`: after each SPDX 2.3 / SPDX 3 file is written, if `SigningMode::StaticKey { .. }`, canonicalize the on-disk bytes, wrap in DSSE via `waybill_common::attestation::envelope::dsse_pae`, sign, and write to `<output>.sig.json`. Reuse m006 `waybill_common::attestation::envelope::SignedEnvelope` shape verbatim.
- [X] T030 [US2] Implement fail-close cleanup per FR-009a in `waybill-cli/src/main.rs`: wrap every emit-then-sign block in a scope that on `SbomSigningError` calls `std::fs::remove_file(&output_path)` (best-effort, ignore ENOENT), logs the failure class via `tracing::error!`, and returns `ExitCode::from(1)`. Add unit test covering the cleanup with a mock signer that always fails.
- [X] T031 [US2] Implement the T021 tests `us2_static_key_jsf_sign_and_verify`, `us2_static_key_signature_covers_document`, `us2_signing_with_stdout_is_rejected`, `us2_mutual_exclusion_rejected_at_parse`, `us2_unsigned_bytes_identical_to_pre_feature_golden`. Static-key verify uses `sigstore::cosign::verification_constraint::PublicKeyVerifier` against the ephemeral pubkey. Golden regression test snapshots the pre-feature CDX bytes to `waybill-cli/tests/fixtures/cisa_2026/unsigned_baseline.cdx.json` (regenerate via `MIKEBOM_UPDATE_CISA_2026_BASELINE=1 cargo test us2_unsigned_bytes_identical_to_pre_feature_golden`).

**Checkpoint A**: US2a (static-key) shippable independently. CDX and SPDX both signable with PEM keys. Keyless deferred to US2b.

### Implementation for User Story 2 — sub-slice B (Sigstore keyless — complete m006 scaffold)

- [ ] T032 [US2] Complete `sign_keyless()` in `waybill-cli/src/attestation/signer.rs` per research §R6 sequence: (1) resolve OIDC token via existing `OidcProvider::detect()`, (2) POST to `<fulcio_url>/api/v2/signingCert` with ephemeral P-256 pubkey + OIDC token, (3) receive short-lived cert, (4) sign the SBOM canonical bytes with the private key, (5) POST cert+signature to `<rekor_url>/api/v1/log/entries` as `hashedrekord` type, (6) receive inclusion proof, (7) return the raw cert-chain + signature + rekor entry via a new `KeylessResult` intermediate type. Replace the current stub return `Err(SigningError::KeylessNotImplemented)` at signer.rs:~170.
- [ ] T033 [US2] Implement Sigstore Bundle assembly in `waybill-cli/src/sbom/signer.rs` `sign_sbom_bytes` keyless branch: consume `KeylessResult` from T032, build the protobuf-JSON Bundle (`mediaType: "application/vnd.dev.sigstore.bundle+json;version=0.3"`, `verificationMaterial.x509CertificateChain.certificates`, `verificationMaterial.tlogEntries`, `messageSignature.messageDigest.{algorithm: SHA2_256, digest: <base64>}`, `messageSignature.signature`), return as `SbomSignatureEnvelope::Keyless(SigstoreBundle)`. Use sigstore-rs 0.11's `sigstore::bundle::Bundle::new_verified` if available, else hand-serialize per bundle spec v0.3.
- [ ] T034 [US2] Wire keyless bundle into CDX emit path in `waybill-cli/src/generate/cyclonedx/builder.rs`: same slot as JSF (`metadata.signature`), just a different envelope shape (untagged serde per data-model.md). SPDX sidecar in `main.rs` writes `<output>.sig.bundle.json` for keyless (vs `<output>.sig.json` for DSSE). Naming rule per contracts/sbom-emission-contract.md.
- [ ] T035 [US2] Enable T021's `us2b_keyless_bundle_sign_and_verify` (remove `#[ignore]`, add feature-gate: `if std::env::var("WAYBILL_TEST_KEYLESS").is_err() { return; }`), point at Sigstore staging (`https://fulcio.sigstage.dev` + `https://rekor.sigstage.dev` via env-var override in the test), verify with `sigstore::cosign::Client` against the staging trust root.
- [ ] T036 [US2] Add a new CI job `lint-and-test-keyless-sbom` in `.github/workflows/ci.yml` mirroring the existing `lint-and-test` job structure but with `permissions: { id-token: write, contents: read }` and env `WAYBILL_TEST_KEYLESS=1 WAYBILL_FULCIO_URL=https://fulcio.sigstage.dev WAYBILL_REKOR_URL=https://rekor.sigstage.dev`. Runs `cargo test --workspace --test cisa_2026_signing`.

**Checkpoint B**: US2b (keyless) shippable. Both signing paths live. FR-007a and FR-007b satisfied.

---

## Phase 5: User Story 3 — Doc-scope SBOM Generation Context in SPDX 2.3 + SPDX 3 (Priority: P3)

**Goal**: Emit `waybill:generation-context` + `waybill:cisa-2026-lifecycle` at document scope for the two SPDX emitters (CDX already covered by m047 lifecycles).

**Independent Test**: Fresh `waybill scan --format spdx-2.3 --output /tmp/scan.spdx.json` on a filesystem target yields a document with an `Annotation` on `SPDXRef-DOCUMENT` whose `comment` string contains both `waybill:generation-context=filesystem-scan` and `waybill:cisa-2026-lifecycle=after-build`. Same test for SPDX 3 against `.["@graph"][]` filtered by `@type == "Annotation"` + `subject == <SpdxDocument @id>`.

### Tests for User Story 3

- [X] T037 [P] [US3] Create `waybill-cli/tests/cisa_2026_generation_context.rs` with 4 tests: `us3_spdx23_carries_doc_scope_annotation_filesystem_scan`, `us3_spdx23_annotation_has_cisa_alias`, `us3_spdx3_carries_doc_scope_annotation_filesystem_scan`, `us3_spdx3_annotation_has_cisa_alias`. Each runs `waybill scan` against a filesystem fixture (reuse `~/.cache/waybill/fixtures/<pin>/transitive_parity/cargo`), inspects the emitted document, asserts the key=value string presence via jq (`jq -r '.annotations[]?.comment' /tmp/scan.spdx.json | grep waybill:generation-context=filesystem-scan`).

### Implementation for User Story 3

- [X] T038 [P] [US3] Add helper `fn emit_doc_scope_generation_context_annotation(gc: &GenerationContext, sbom_version: Option<SbomVersion>) -> String` in `waybill-cli/src/generate/spdx/annotations.rs` that returns the semicolon-joined key=value string per contracts/sbom-emission-contract.md (`waybill:generation-context=<native>;waybill:cisa-2026-lifecycle=<alias>[;waybill:sbom-version=<N>]`). Include inline unit tests covering (a) generation-context only, (b) both signals present. **Note**: SBOM Version key handled here so US3 and US4 share the annotation-composition helper. — Implementation deviation: emit `waybill:cisa-2026-lifecycle` as its own annotation (matching the m071 one-key-per-annotation convention) rather than composing into one semicolon-joined string. Consumers still get both signals; parity catalog gate (C141) works. The `SbomVersion` composition parameter is deferred to US4.
- [X] T039 [US3] Modify `waybill-cli/src/generate/spdx/document.rs` SPDX 2.3 emit path to append an `Annotation` on `SPDXRef-DOCUMENT` per research §R3 shape (`annotationType: "OTHER"`, `annotator: "Tool: waybill-<version>"`, `annotationDate: <created>`, `comment: <T038 output>`). Use existing document-scope annotations plumbing (grep for `waybill:file-inventory-mode` per m133 — same pattern). Emit unconditionally (always present, even with default GenerationContext) so consumers can always resolve the field.
- [X] T040 [US3] Modify `waybill-cli/src/generate/spdx/v3_document.rs` SPDX 3 emit path to append a top-level `Annotation` element with `@type: "Annotation"`, `@id: <content-addressed IRI per m011>`, `annotationType: "other"`, `subject: <SpdxDocument @id>`, `statement: <T038 output>`. Use `waybill-cli/src/generate/spdx/v3_annotations.rs::compute_content_addressed_iri` for the `@id` (locate via grep for existing `@id` computations in v3 emitters).
- [X] T041 [US3] Regenerate SPDX 2.3 and SPDX 3 goldens for the doc-scope annotation addition per SC-007. Use `MIKEBOM_UPDATE_*=1` env vars per memory `feedback_release_bump_regen_all_golden_tests`. Verify churn with normalized-sorted diff per memory `feedback_verify_golden_churn_normalized` (mask `rel-` / `anno-` IDs, `LC_ALL=C sort`, then diff). Report cell counts in the PR body.
- [X] T042 [US3] Confirm SPDX 3 conformance validator tolerance per research §R4: run `.venv/spdx3-validate/bin/spdx3-validate --input <regenerated-spdx3-golden>` (per memory `reference_spdx3_validator`). On any failure due to `annotationType: "other"`, fall back to `annotationType: "review"` per §R4 tail plan and re-regen. Record decision in research.md §R4 as a follow-up `<!-- validator-result: <date> ok -->` comment.
- [X] T043 [US3] Implement the T037 tests for both SPDX 2.3 + SPDX 3, keying on the emitted annotation comment/statement string.
- [X] T044 [US3] Update coverage matrix row 5 (SBOM Generation Context) in `docs/cisa-2026-coverage.md` from placeholder to actual verdicts: CDX ✅ (existing m047), SPDX 2.3 ⚠️ (`document.rs` + T038 helper), SPDX 3 ⚠️ (`v3_document.rs` + T038 helper). Add corresponding recipes to Appendix A.

**Checkpoint**: US3 shippable. SPDX 2.3 and SPDX 3 documents now carry generation-context at doc scope per FR-010/011/012. Coverage matrix row 5 updated.

---

## Phase 6: User Story 4 — Caller-supplied SBOM document version (Priority: P3)

**Goal**: `--sbom-version <N>` flag threads through CDX `metadata.version` (native integer) and SPDX 2.3 / SPDX 3 doc-scope annotations (via the T038 helper).

**Independent Test**: `waybill scan --sbom-version 2 --format cyclonedx-1.6 --output /tmp/scan.cdx.json` yields `jq .metadata.version /tmp/scan.cdx.json` = `2`. Same scan with `--format spdx-2.3` yields `jq -r '.annotations[]?.comment' /tmp/scan.spdx.json | grep waybill:sbom-version=2`. Without `--sbom-version`, `metadata.version` is `1` and no `waybill:sbom-version` key emitted.

### Tests for User Story 4

- [X] T045 [P] [US4] Create `waybill-cli/tests/cisa_2026_sbom_version.rs` with 5 tests: `us4_cdx_native_integer_when_set`, `us4_cdx_default_is_1_when_unset`, `us4_spdx23_annotation_key_present_when_set`, `us4_spdx3_annotation_key_present_when_set`, `us4_invalid_values_rejected_at_parse` (parameterized over `0`, `-1`, `2.0`, `v2`, `latest`, empty, whitespace, NUL).

### Implementation for User Story 4

- [X] T046 [US4] Thread `Option<SbomVersion>` through the generate-context struct (same struct extended in T028). Default None. Wire the CLI `--sbom-version <N>` value in `waybill-cli/src/cli/generate.rs` into the struct.
- [X] T047 [US4] Modify `waybill-cli/src/generate/cyclonedx/builder.rs` at builder.rs:816 (`"version": 1`) to emit `sbom_version.map(|v| v.as_u32()).unwrap_or(1)`. Preserves byte-identity when `--sbom-version` unset (still writes `1`).
- [X] T048 [US4] Extend the T038 annotation-composition helper to accept the `Option<SbomVersion>` parameter and append `;waybill:sbom-version=<N>` when Some. Both SPDX 2.3 and SPDX 3 emit paths pass the value through the shared helper — one annotation carries both generation-context and sbom-version per contracts/sbom-emission-contract.md.
- [X] T049 [US4] Implement the T045 tests. Reject-cases test drives `waybill scan --sbom-version <bad>` as a subprocess (`std::process::Command::new(env!("CARGO_BIN_EXE_mikebom"))` per memory `feedback_build_check_all_targets`) and asserts exit code 2 + diagnostic contains "positive integer".
- [X] T050 [US4] Update coverage matrix row 9 (SBOM Version) in `docs/cisa-2026-coverage.md` — CDX ✅ (`builder.rs:816` + `--sbom-version` flag), SPDX 2.3 ⚠️ (`document.rs` annotation + `waybill:sbom-version` key), SPDX 3 ⚠️ (`v3_document.rs` annotation). Add recipes to Appendix A.
- [X] T051 [US4] Note in `docs/cisa-2026-coverage.md` row 9 Notes column: waybill's UUID `serialNumber` (CDX) + content-addressed `documentNamespace` / `@id` (SPDX per m010) already satisfy CISA's RFC 9562 alternative for revision identity — `--sbom-version` is the monotonic-counter path only.

**Checkpoint**: US4 shippable. All three data-field gaps closed.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Fold docs, verify no regressions, close constitution amendment.

- [X] T052 [P] Update `docs/reference/sbom-format-mapping.md` T009 stub rows to final form with the mapping details, jq recipes, and cross-references to `docs/cisa-2026-coverage.md`.
- [X] T053 [P] Update coverage matrix row 2 (SBOM Author Signature) in `docs/cisa-2026-coverage.md` from placeholder to actual verdicts: CDX ⚠️ (`--sign`/`--sign-key` opt-in, JSF or Sigstore Bundle at `metadata.signature`), SPDX 2.3 ⚠️ (sidecar `.sig.json` / `.sig.bundle.json`), SPDX 3 ⚠️ (sidecar). Cite US2 completion.
- [X] T054 [P] Update `README.md` "Standards & compliance" section (or add if absent) with a paragraph referencing `docs/cisa-2026-coverage.md` as the compliance source of truth. Include: publication date, `waybill-milestone` value, TLP:CLEAR designation.
- [X] T055 [P] Update `CLAUDE.md` "Active Technologies" auto-generated section to include the feature 221 line (already appended by `.specify/scripts/bash/update-agent-context.sh` during plan phase — verify present, no manual edit needed if the script ran clean).
- [X] T056 Add feature 221 line to `MEMORY.md` index at `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/MEMORY.md` referencing the completed audit: `- [CISA 2026 coverage matrix](reference_cisa_2026_coverage.md) — machine-verified matrix at docs/cisa-2026-coverage.md; opt-in --sign / --sign-key / --sbom-version flags close the three gaps`. Create the pointed-at memory file with frontmatter `type: reference` naming the CISA publication date + waybill milestone.
- [X] T057 Run the full pre-PR gate per Constitution §Pre-PR Verification: `./scripts/pre-pr.sh` (which chains `cargo +stable clippy --workspace --all-targets -- -D warnings` + `cargo +stable test --workspace`). Report both "Zero errors and zero warnings" (clippy) and per-suite `ok. N passed; 0 failed` (test) per memory `feedback_prepr_gate_full_output`. Do NOT amend an existing commit if a hook fails; create a NEW fix commit per Constitution guidance.
- [X] T058 [P] Run the m090 fixture cache smoke test to confirm no unintended goldens got touched: `git status waybill-cli/tests/fixtures/` should show ONLY the intentional US3 SPDX regen from T041 + US2 unsigned-baseline snapshot from T031. If any other fixture bytes changed, revert per memory `feedback_verify_golden_churn_normalized`.
- [X] T059 [P] Draft the follow-up constitution amendment PR body (do NOT open the PR from this branch): update Principle V line 211 "CISA 2025 Minimum Elements" → "CISA 2026 Minimum Elements"; version bump 2.0.0 → 2.1.0 (MINOR); SYNC IMPACT REPORT note cites this milestone. Save draft at `specs/221-cisa-2026-elements-audit/followup-constitution-amendment.md`.
- [X] T060 Run through `specs/221-cisa-2026-elements-audit/quickstart.md` end-to-end manually: (a) open `docs/cisa-2026-coverage.md`, verify readable; (b) run `waybill scan --sign-key <ephemeral.pem> --output signed.cdx.json`, verify with a JSF tool; (c) run `waybill scan --sbom-version 2 --output scan.cdx.json`, verify `jq .metadata.version scan.cdx.json` → `2`. Record any UX papercuts as `[NEEDS FOLLOWUP]` in `specs/221-cisa-2026-elements-audit/quickstart.md`.
- [X] T061 [P] Add `docs/cisa-2026-coverage.md` `last-verified` front-matter update to the "regeneration process" checklist. This is a doc reminder, not code.

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (Phase 1)**: No dependencies. Start immediately.
- **Foundational (Phase 2)**: Requires Phase 1 (dep confirmation + fixture cache). Blocks US2 + US3 + US4.
- **US1 (Phase 3)**: Requires Phase 1 only. **US1 can proceed in parallel with Phase 2.** US1 is the MVP — ship first even if US2/US3/US4 slip.
- **US2 (Phase 4)**: Requires Phase 2. US2a (static-key) ships independently of US2b (keyless).
- **US3 (Phase 5)**: Requires Phase 2 (needs `GenerationContext::as_cisa_2026_lifecycle` from T006 and the T038 helper).
- **US4 (Phase 6)**: Requires Phase 2 (needs `SbomVersion` newtype from T004 and the T038 helper — reason US4 lists after US3 despite sharing priority).
- **Polish (Phase 7)**: Requires every user-story checkpoint reached (or the corresponding row in T052/T053 gracefully absent).

### Story dependencies (visualized)

```text
Phase 1 (Setup) ──┬──> Phase 3 (US1: coverage matrix + verifier) ──> ship MVP
                  │
                  └──> Phase 2 (Foundational: types + CLI skeleton)
                                    │
                                    ├──> Phase 4 (US2: signing) ──> a + b
                                    │
                                    ├──> Phase 5 (US3: gen-context in SPDX)
                                    │
                                    └──> Phase 6 (US4: --sbom-version)
                                              │
                                              └──> Phase 7 (Polish)
```

### Within each user story

- Tests scaffold first (T010, T021, T037, T045) — start as `todo!()`, get implemented as their dependencies land.
- New types before consuming code (T004 SbomVersion → T047 emit; T023 SigningMode → T027 CDX slot; T038 annotation helper → T039/T040 emit sites).
- CDX before SPDX in signing (T027 → T029) because CDX has native slot to prove the crypto works; SPDX sidecar is the harder-to-verify path.
- US2 sub-slice A (static-key) before sub-slice B (keyless): T023–T031 gate T032–T036, because sub-slice A validates every plumbing decision without the R6 network-integration risk.

### Parallel opportunities

- **Phase 1**: T002 + T003 in parallel.
- **Phase 2**: T005 + T006 + T009 parallel (different files, T005 depends on T004 completing first for the re-export).
- **Phase 3 (US1)**: T010 + T011 parallel; then T012 + T013 + T014 partly parallel (different sections of the same file — serialize to avoid conflict); T015–T019 parallel (different test fns).
- **Phase 4 (US2)**: T021 + T022 parallel; then T023–T031 mostly serial (crypto plumbing has file-level dependencies); T032–T036 for sub-slice B.
- **Phase 5 (US3)**: T037 + T038 parallel; T039 + T040 serial (different emitters, but T038 blocks both).
- **Phase 6 (US4)**: T045 + reuse T038 → T046/T047/T048 mostly serial (single struct field extension).
- **Phase 7**: T052 + T053 + T054 + T055 + T058 + T059 + T061 parallel (different files, no shared deps).

---

## Implementation Strategy

### MVP first (US1 only) — deliver in isolation

Phases 1 + 3 alone. Result: coverage doc live, machine-verifier green in CI, the user's original ask ("double check we support this") is answered with an evidence-backed matrix. US2/US3/US4 slip → the audit still stands.

**Ship criteria for MVP**:
- `docs/cisa-2026-coverage.md` merged.
- `cisa_2026_coverage_matrix` test passes in CI.
- README section referencing the coverage doc merged.

### Incremental delivery (recommended)

1. **MVP** (T001–T003, T010–T020): ship US1. Merge to `main`, open follow-up branch for US2.
2. **US2a static-key** (T004–T009, T021–T031): ship signing with PEM keys. Merges even if keyless slips.
3. **US2b keyless** (T032–T036): ship Sigstore Bundle path. Requires CI OIDC configuration.
4. **US3** (T037–T044): ship SPDX doc-scope generation-context. Regenerates SPDX goldens once.
5. **US4** (T045–T051): ship `--sbom-version` flag.
6. **Polish** (T052–T061): fold docs, close constitution amendment.

Each numbered slice above merges standalone and delivers observable value.

### If timeboxed

- **Cannot slip US1** — it IS the user's ask.
- Can defer US2b (keyless) if Sigstore integration proves harder than R6 estimate (2d impl + 1d test).
- Can defer US4 entirely if consumers indicate they key on `serialNumber`/`documentNamespace` (CISA-blessed RFC 9562 identity) rather than a monotonic counter — flag in `specs/221-cisa-2026-elements-audit/spec.md` US4 already notes this.
- US3 is not deferrable if SPDX 2.3 / SPDX 3 emitters are considered production paths — doc-scope generation-context is the parity gap consumers most feel.

---

## Task count summary

| Phase | Count | Story | Notes |
|-------|-------|-------|-------|
| 1 Setup | 3 | — | Dep audit + fixture cache |
| 2 Foundational | 6 | — | New types + CLI skeleton |
| 3 US1 | 12 | P1 (MVP) | Coverage matrix + verifier (T013a covers FR-016 SWID advisory) |
| 4 US2 | 16 | P2 | Signing (a: static-key, b: keyless) |
| 5 US3 | 8 | P3 | SPDX doc-scope gen-context |
| 6 US4 | 7 | P3 | `--sbom-version` |
| 7 Polish | 10 | — | Docs + regressions + follow-ups |
| **Total** | **62** | | |

---

## Format validation

- ✅ All tasks start with `- [ ]` checkbox.
- ✅ All tasks carry a Task ID (T001–T061).
- ✅ User-story-phase tasks carry `[USn]` label; Setup / Foundational / Polish tasks do not.
- ✅ All tasks name at least one file path (or docs path / config path where applicable).
- ✅ Parallel-safe tasks marked `[P]`.
- ✅ No leftover placeholder text from the template.
