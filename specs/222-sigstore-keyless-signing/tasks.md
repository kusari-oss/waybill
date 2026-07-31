# Tasks: Sigstore keyless SBOM signing (completes m221 US2b)

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/222-sigstore-keyless-signing/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md
**Tests**: INCLUDED — FR-010 mandates an integration test; FR-016 requires log-field assertions.

**Organization**: All work belongs to a single P1 user story (Sigstore keyless signing). Foundational + Setup are minimal because m221 already scaffolded the surface; the bulk of the work is filling the `sign_keyless()` function body.

## Format: `[ID] [P?] [Story?] Description with file path`

- **[P]**: Can run in parallel with other [P] tasks in the same phase
- **[Story]**: US1 for user-story tasks; unlabeled for Setup, Foundational, Polish
- Full file paths absolute or workspace-root-relative

---

## Phase 1: Setup

**Purpose**: Confirm Phase 0 R1 audit result and vendor Sigstore CTFE public keys (chosen path after audit FAILED on `sigstore-trust-root-rustls-tls`).

- [X] T001 **AUDIT COMPLETED 2026-07-30 — FAILED**. Ran `cargo tree -p waybill --target x86_64-unknown-linux-gnu -e normal` with `sigstore-trust-root-rustls-tls` toggled on: 3 hits (`aws-lc-rs v1.17.1`, `aws-lc-sys v0.42.0`, `cmake v0.1.58`). Root cause: `tough` at both `0.19` and `0.22` has unconditional `[dependencies.aws-lc-rs]`. Reverted feature toggle. Adopted R1-alt (vendored CTFE keys + `SigningContext::new()`). Result recorded in `research.md` §R1 as `<!-- verified: 2026-07-30 -->`.
- [ ] T002 **PIVOTED FROM FEATURE TOGGLE**. Vendor Sigstore CTFE public keys as `&'static [u8]` DER SPKI and expose a `ctfe_keyring(rekor_url)` helper.
  1. Run `cosign initialize` locally (fetches + TUF-verifies Sigstore's production trust root at `~/.sigstore/root/`).
  2. Extract the CTFE pub keys under `~/.sigstore/root/targets/` (look for `ctfe*.pub` — production has 1 active CTFE key; sigstage has 1 as well).
  3. For each: `openssl pkey -pubin -inform PEM -outform DER -in <ctfe.pub> -out ctfe_prod.der` (and staging equivalent via `cosign initialize --mirror https://tuf-repo-cdn.sigstage.dev --root https://tuf-repo-cdn.sigstage.dev/root.json`).
  4. Commit both to `waybill-cli/vendor/sigstore/ctfe_prod.der` + `waybill-cli/vendor/sigstore/ctfe_stage.der`. Confirm each is <2 KiB.
  5. Create `waybill-cli/src/attestation/sigstore_trust_root.rs` exposing: `pub const SIGSTORE_{PROD,STAGE}_CTFE_KEY_DER: &[u8] = include_bytes!(...)` + `pub fn ctfe_keyring(rekor_url: &str) -> Result<Keyring, SigningError>` (dispatch: `contains("sigstage.dev") => STAGE`, else `PROD`). Wire into `waybill-cli/src/attestation/mod.rs`.
  6. Add unit test: `sigstore_trust_root::ctfe_keyring("https://rekor.sigstore.dev").is_ok()` — verifies DER parses via `sigstore::crypto::Keyring::new`.
  7. **NO** changes to `waybill-cli/Cargo.toml` feature list — the current `["cosign-rustls-tls", "fulcio-rustls-tls", "rekor-rustls-tls", "bundle"]` set is sufficient.
- [ ] T002a [P] Document key sourcing + rotation policy at `docs/sigstore-trust-keys.md`. Include: cosign version used at vendoring time, pinned Sigstore trust-root `root.json` SHA-256, the exact 4-line vendoring recipe (steps 1–3 of T002), rotation cadence expectation (~1x/year), and the "how to regenerate" section. Cross-link from `docs/cisa-2026-coverage.md` row 2 (SBOM Author Signature).
- [ ] T003 [P] Manually verify Sigstore staging endpoints reachable from the dev environment: `curl -sSf -o /dev/null -w '%{http_code}' https://fulcio.sigstage.dev/api/v2/trustBundle` (expect 200); `curl -sSf -o /dev/null -w '%{http_code}' https://rekor.sigstage.dev/api/v1/log` (expect 200). Blocks the T015 integration test's runtime — if either endpoint is unreachable, defer test execution to CI where staging is guaranteed-provisioned.

**Checkpoint**: CTFE keys vendored at `waybill-cli/vendor/sigstore/`; `ctfe_keyring()` helper compiles; `sigstore_trust_root.rs` unit test green; staging reachable.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Extend the m221-shipped `SigningMode` enum with the `Keyless` variant + wire the `--sign` CLI flag surface. Also verify sigstore-rs 0.11's public API entry points are what R2 documented (defensive re-verify).

**⚠️ CRITICAL**: US1 cannot start until this phase completes.

- [X] T004 Extend `SigningMode` enum in `waybill-cli/src/sbom/signer.rs` with a `Keyless { fulcio_url: String, rekor_url: String, rekor_timeout: std::time::Duration }` variant per data-model.md §SigningMode. Update `SigningMode::is_enabled()` to return true for the new variant. Add inline unit test asserting `SigningMode::Keyless{...}.is_enabled() == true`. `load_key` extended with exhaustive match arm returning `NotImplemented` for the Keyless variant (T019/T021 replace with real sign path). Two new unit tests green: `signing_mode_keyless_is_enabled_m222` + `load_key_rejects_keyless_mode_m222`.
- [X] T005 Add `--sign` (bool) CLI flag to `ScanArgs` in `waybill-cli/src/cli/scan_cmd.rs`. Configured clap `conflicts_with = "sign_key"` per m221 FR-007 (mutual exclusion with `--sign-key`). Threaded three env-var-honoring fields: `--fulcio-url` (env `WAYBILL_FULCIO_URL`, default `https://fulcio.sigstore.dev`), `--rekor-url` (env `WAYBILL_REKOR_URL`, default `https://rekor.sigstore.dev`), `--rekor-timeout-secs` (env `WAYBILL_REKOR_TIMEOUT_SECS`, default 30). `ScanArgs::test_helper_default` updated with matching defaults. Four new clap-parse tests green: `sign_flag_defaults_off_m222`, `sign_flag_accepts_bare_toggle_m222`, `sign_and_sign_key_are_mutually_exclusive_m222`, `sign_endpoint_overrides_via_flags_m222`.
- [X] T006 Modified `waybill-cli/src/cli/scan_cmd.rs` `SigningMode` construction (at ~line 3831) to add the `--sign` branch first (mutually exclusive with `--sign-key` via clap `conflicts_with`): `if args.sign { SigningMode::Keyless { fulcio_url, rekor_url, rekor_timeout } } else { match args.sign_key {...} }`. FR-008a validator at ~line 2568 extended: `if args.sign_key.is_some() || args.sign` now triggers the stdout-rejection guard for both paths; error message dynamically names the offending flag (`--sign` vs `--sign-key`).
- [X] T007 Compile-verified sigstore-rs 0.11 API surface — see `signing_context_new_chain_compiles` test at `waybill-cli/src/attestation/sigstore_trust_root.rs` (added during T002, kept as a permanent regression guard against upstream API drift instead of deleted). Constructs full `FulcioClient::new(Url, TokenProvider::Oauth(OauthTokenProvider::default())) → RekorConfiguration::default() → SigningContext::new(fulcio, rekor, ctfe_keyring)` chain; passes. Confirmed 2026-07-30 against `sigstore = 0.11.0` at the `kusari-sandbox/sigstore-rs v0.11.0-waybill-1` fork tag.

**Checkpoint**: `SigningMode::Keyless{}` compiles; `--sign` flag reaches the sign-dispatch layer; sigstore-rs 0.11 API surface matches R2 docs.

---

## Phase 3: User Story 1 — Sigstore keyless SBOM signing (Priority: P1) 🎯 MVP

**Goal**: Complete the m006-scaffolded `sign_keyless()` at `waybill-cli/src/attestation/signer.rs:170` so that `waybill sbom scan --sign` produces a Sigstore Bundle in CDX `metadata.signature` + `.sig.bundle.json` sidecar for SPDX; verifies with `cosign verify-blob --bundle` against production Sigstore trust root.

**Independent Test**: In a GitHub Actions job with `permissions: id-token: write`, run `waybill sbom scan --path <target> --format cyclonedx-json --output signed.cdx.json --sign` — assert exit code 0, `jq .metadata.signature signed.cdx.json` returns a Sigstore Bundle, `cosign verify-blob --bundle signed.cdx.json --certificate-identity <job-oidc-subject> --certificate-oidc-issuer https://token.actions.githubusercontent.com` returns exit 0, and byte-mutation of the CDX payload flips verify to non-zero.

### Tests for User Story 1

- [ ] T008 [P] [US1] Add integration test skeleton `us2b_keyless_signing_failure_cleans_up_output` to `waybill-cli/tests/cisa_2026_signing.rs` — mirrors m221 US2a's `us2a_signing_failure_cleans_up_output_file` pattern but sets `WAYBILL_FULCIO_URL=https://fulcio.invalid.example` to force Fulcio to fail, asserts non-zero exit + no `--output` file left behind. Runs unconditionally (no `WAYBILL_TEST_KEYLESS` gate — pure failure-mode test doesn't need a real OIDC token).
- [ ] T009 [P] [US1] Add integration test skeleton `us2b_keyless_no_oidc_token_fails_close` to `waybill-cli/tests/cisa_2026_signing.rs` — unsets both `ACTIONS_ID_TOKEN_REQUEST_*` and `SIGSTORE_ID_TOKEN`, runs `waybill sbom scan --sign` as subprocess, asserts non-zero exit + stderr contains the Q1-mandated diagnostic wording (`"no OIDC token available"` + `"SIGSTORE_ID_TOKEN"` + `"id-token: write"`). Runs unconditionally.
- [ ] T010 [US1] Un-`#[ignore]` the m221-scaffolded `us2b_keyless_bundle_sign_and_verify` test in `waybill-cli/tests/cisa_2026_signing.rs:296`. Add `WAYBILL_TEST_KEYLESS` env-var gate at test entry: if unset, `eprintln!("INFO: us2b_keyless_bundle_sign_and_verify skipped (WAYBILL_TEST_KEYLESS unset)"); return;`. Implement the happy-path assertion: (a) subprocess `waybill sbom scan --sign --output <tmp>/signed.cdx.json` with `WAYBILL_FULCIO_URL=https://fulcio.sigstage.dev` + `WAYBILL_REKOR_URL=https://rekor.sigstage.dev` env; (b) assert exit 0; (c) parse the output, extract `metadata.signature` field; (d) verify Bundle shape via `sigstore::bundle::verify::blocking::Verifier::production()?.verify(&mut Cursor::new(payload_bytes), bundle, &policy::Identity::new(expected_subject, expected_issuer), true)`.
- [ ] T011 [P] [US1] Add unit tests to `waybill-cli/src/attestation/signer.rs` `#[cfg(test)] mod tests`: (a) `resolve_identity_token_interactive_returns_fail_close_diagnostic` — construct `OidcProvider::Interactive`, assert the returned `Err(OidcTokenError)` detail contains all three Q1 substrings (`"no OIDC token available"`, `"SIGSTORE_ID_TOKEN"`, `"id-token: write"`); (b) `identity_token_from_env_var_reads_sigstore_id_token` — use `temp_env::with_var` to set `SIGSTORE_ID_TOKEN=<minimal-valid-jwt>`, assert `Ok(IdentityToken)`. Guard `.unwrap()` per Constitution IV convention (`#[cfg_attr(test, allow(clippy::unwrap_used))]` at mod level).
- [ ] T012 [P] [US1] Add unit test `github_actions_oidc_helper_constructs_correct_url` — using `temp_env::with_vars` to stub `ACTIONS_ID_TOKEN_REQUEST_URL=https://example.test/oidc?run_id=123` + `ACTIONS_ID_TOKEN_REQUEST_TOKEN=fake`, assert that the URL passed to reqwest is `https://example.test/oidc?run_id=123&audience=sigstore` (query-param concatenation correctness — `&` separator, not `?`). Uses a mock reqwest server (`mockito` or `httpmock` — check workspace; add via a `#[cfg(test)]`-only dev-dep if needed; `mockito` is preferred, verify via `grep 'mockito\|httpmock' waybill-cli/Cargo.toml`).

### Implementation for User Story 1

- [ ] T013 [US1] Add `GitHubOidcResponse` struct to `waybill-cli/src/attestation/signer.rs` per data-model.md — `#[derive(Debug, serde::Deserialize)] struct GitHubOidcResponse { value: String }`. Private (non-pub) module-level type.
- [ ] T014 [US1] Add `KeylessSignSuccess` struct to `waybill-cli/src/attestation/signer.rs` per data-model.md — public struct with fields `bundle: sigstore::bundle::Bundle`, `rekor_log_index: u64`, `fulcio_cert_subject: String`, `oidc_provider: &'static str`. Add doc-comment cross-references to FR-016.
- [ ] T015 [US1] Implement `identity_token_from_env_var()` helper in `waybill-cli/src/attestation/signer.rs` per contracts/oidc-provider-dispatch.md §Provider: Explicit. Reads `SIGSTORE_ID_TOKEN`, parses via `IdentityToken::try_from(str)`, checks `in_validity_period()` before returning. Three failure modes → three distinct `SigningError::OidcTokenError` detail strings matching the contract's table.
- [ ] T016 [US1] Implement `identity_token_from_github_actions()` helper in `waybill-cli/src/attestation/signer.rs` per contracts/oidc-provider-dispatch.md §Provider: GitHubActions. Uses `reqwest::blocking::Client::new()`, appends `&audience=sigstore` to the ambient URL, bearer-auths with the ambient token, deserializes `GitHubOidcResponse`. 30s total timeout via `.timeout(Duration::from_secs(30))`. Six failure modes → six distinct `SigningError::OidcTokenError` detail strings.
- [ ] T017 [US1] Implement `resolve_identity_token(&OidcProvider) -> Result<IdentityToken, SigningError>` dispatcher in `waybill-cli/src/attestation/signer.rs` per contracts/oidc-provider-dispatch.md. Match on the three enum variants; `GitHubActions` → T016 helper; `Explicit` → T015 helper; `Interactive` → the fail-close diagnostic per Q1 clarification (exact wording in the contract).
- [ ] T018 [US1] Implement `classify_sign_error(SigstoreError) -> SigningError` mapping in `waybill-cli/src/attestation/signer.rs` per contracts/keyless-signing-flow.md §Error variant mapping. 6-arm match: FulcioClientError → FulcioError, RekorError → RekorError, PublicKeyOrCertificateError → CryptoError, IdentityTokenError → OidcTokenError, plus catch-all → CryptoError with detail preserving the original string.
- [ ] T018a [US1] **5-minute research spike before T019**: grep sigstore-rs 0.11's `~/.cargo/registry/src/index.crates.io-*/sigstore-0.11.0/src/rekor/**/*.rs` + `bundle/sign.rs` for `timeout` / `Duration` / `RekorConfiguration` to discover if a built-in Rekor timeout knob exists. If yes, T019's timeout-wrapper approach is replaced with `RekorConfiguration::with_timeout(rekor_timeout)` (or equivalent) passed to `SigningContext::new()`; the `mpsc::recv_timeout` wrapper is skipped (~20 LOC saved). If no knob exists, T019 falls through to the wrapper as-spec'd. Record the finding as an inline comment in `research.md` §R4: `<!-- resolved: <date>: sigstore-rs 0.11 (does OR does not) expose Rekor timeout — using (knob OR wrapper) approach -->`.
- [ ] T019 [US1] Replace the m006-scaffolded `sign_keyless()` body at `waybill-cli/src/attestation/signer.rs:170` (currently `Err(SigningError::KeylessNotImplemented)`). Signature stays the same. New body implements Steps 1–7 from contracts/keyless-signing-flow.md: resolve OIDC token → construct `SigningContext` (`::production()` if T018a found no timeout knob; `::new()` with `RekorConfiguration::with_timeout(rekor_timeout)` if T018a found one) → `blocking_signer(token)?` → sign `session.sign(&Cursor::new(bytes))` (wrap in `std::thread::spawn + mpsc::recv_timeout(rekor_timeout)` ONLY if T018a found no knob) → extract `rekor_log_index` from `bundle.verification_material.tlog_entries.first().log_index` → extract `fulcio_cert_subject` from the leaf cert's SAN → emit `tracing::info!` with all three FR-016 fields → return `Ok(KeylessSignSuccess{...})`. Delete the `SigningError::KeylessNotImplemented` variant (no longer reachable) OR leave for potential future re-scaffolding — check with reviewer preference; safer default is to leave it for now.
- [ ] T020 [US1] Add public wrapper `sign_keyless_sbom(canonical_bytes: &[u8], fulcio_url: &str, rekor_url: &str, rekor_timeout: Duration) -> Result<KeylessSignSuccess, SigningError>` in `waybill-cli/src/attestation/signer.rs`. This is the entry point the SBOM signer's Keyless dispatch arm calls; it constructs the `SigningIdentity::Keyless{...}` internally and delegates to the T019 `sign_keyless()` for the actual work. Keeps the SBOM-side signer decoupled from the attestation-side `SigningIdentity` type.
- [ ] T021 [US1] Wire the `Keyless` arm into `sign_cdx_document_in_place()` at `waybill-cli/src/sbom/signer.rs` per contracts/keyless-signing-flow.md §CDX-embedded Bundle canonical-bytes contract. Currently a two-arm `match mode { Unsigned => Ok(()), StaticKey{...} => ... }` (per m221 US2a). Add a third arm: `Keyless { fulcio_url, rekor_url, rekor_timeout } => { let canonical = canonical_json_bytes(&doc_without_signature_slot)?; let success = sign_keyless_sbom(&canonical, fulcio_url, rekor_url, rekor_timeout)?; insert serde_json::to_value(success.bundle) at metadata.signature; Ok(()) }`. **CRITICAL**: sign the CDX bytes WITHOUT `metadata.signature` populated (unlike m221 US2a's JSF empty-value trick — Bundle envelope doesn't support empty-value substitution). Verifiers reproduce the signed bytes by parsing the doc, removing `metadata.signature`, re-canonicalizing. This departs from the m221 static-key flow deliberately per the contract's Alternatives-rejected section.
- [ ] T022 [US1] Wire the `Keyless` arm into `sign_spdx_bytes_to_dsse()` at `waybill-cli/src/sbom/signer.rs`. Currently two-arm match. Add third arm: `Keyless { fulcio_url, rekor_url, rekor_timeout } => { let success = sign_keyless_sbom(bytes, fulcio_url, rekor_url, rekor_timeout)?; Ok(Some(BundleSidecar::from(success.bundle))) }`. Note the return-type shift: static-key returns `Option<SignedEnvelope>` (DSSE); keyless should return the `Bundle` shape for sidecar-write. Extend the return type to `Result<Option<Sidecar>, SbomSigningError>` where `enum Sidecar { Dsse(SignedEnvelope), SigstoreBundle(sigstore::bundle::Bundle) }` — the CLI-side write logic already routes SPDX sidecars per `sidecar_extension()` helper in m221 `scan_cmd.rs`; extend that helper to emit `.sig.bundle.json` for the Bundle variant vs `.sig.json` for DSSE. Match m221 US2a's `<output>.sig.bundle.json` vs `<output>.sig.json` naming convention exactly per FR-004.
- [ ] T023 [US1] Update the CLI-side sidecar-writer at `waybill-cli/src/cli/scan_cmd.rs` (`sidecar_extension` + the write loop around line ~3777 per m221 US2a work) to branch on the new `Sidecar` enum: DSSE → `.sig.json` (matches existing behavior), SigstoreBundle → `.sig.bundle.json`. Extend the fail-close cleanup tracker (`written_files: Vec<PathBuf>`) to include both sidecar shapes.
- [ ] T024 [US1] Implement the T010 happy-path test body. Point at Sigstore staging (`WAYBILL_FULCIO_URL=https://fulcio.sigstage.dev` + `WAYBILL_REKOR_URL=https://rekor.sigstage.dev` set by the test setup). Verify via `sigstore::bundle::verify::blocking::Verifier::production()?.verify(...)` per contracts/keyless-signing-flow.md wire-format guarantees. Assert: (a) Bundle mediaType matches `application/vnd.dev.sigstore.bundle+json;version=0.3`; (b) `verificationMaterial.tlogEntries[].logIndex` is a positive integer; (c) `verificationMaterial.x509CertificateChain.certificates[]` has ≥2 entries per FR-014 amendment (leaf + at least one intermediate); (d) `messageSignature.messageDigest.algorithm == "SHA2_256"`; (e) full round-trip: parse the emitted CDX, extract `metadata.signature`, remove that field, re-canonicalize via `canonical_json_bytes`, and confirm `Verifier::verify()` returns Ok against those bytes (validates the CDX canonical-bytes contract end-to-end).
- [ ] T025 [US1] Implement the T008 test body (Fulcio-unreachable → fail-close): subprocess `waybill sbom scan --sign --output <tmp>/signed.cdx.json` with `WAYBILL_FULCIO_URL=https://fulcio.invalid.example.test` + `ACTIONS_ID_TOKEN_REQUEST_*` env stubbed OR `SIGSTORE_ID_TOKEN=<fake>` env set. Assert non-zero exit, absence of `<tmp>/signed.cdx.json`, stderr contains `"FulcioError"` OR `"OidcTokenError"` (either is acceptable since token acquisition might fail before Fulcio depending on env stubbing).
- [ ] T026 [US1] Implement the T009 test body (no OIDC token → fail-close). Clear both provider-detection env vars via `temp_env::with_vars_unset(["ACTIONS_ID_TOKEN_REQUEST_URL", "ACTIONS_ID_TOKEN_REQUEST_TOKEN", "SIGSTORE_ID_TOKEN"], || {...})`; subprocess with `--sign --output <tmp>/x.cdx.json`; assert non-zero exit + stderr contains the Q1 diagnostic (`"no OIDC token available"`).
- [ ] T027 [US1] Add integration test `us2b_keyless_signature_covers_document_mutation` mirroring m221 US2a's `us2a_signature_covers_document_mutation_flips_verify` pattern (guarded by `WAYBILL_TEST_KEYLESS=1`): sign against staging → mutate one byte of the emitted CDX payload → assert `sigstore::bundle::verify` returns Err.
- [ ] T028 [US1] Add integration test `us2b_keyless_fr016_info_log_fields` (guarded by `WAYBILL_TEST_KEYLESS=1`): subprocess with `env RUST_LOG=info WAYBILL_LOG=info` (both env vars set — belt-and-suspenders since waybill's tracing subscriber may honor either); capture combined stdout+stderr via `Command::output()`; sign against staging; parse waybill's stderr; assert the presence of the three FR-016 fields (`rekor_log_index=<positive integer>`, `fulcio_cert_subject=<non-empty string>`, `oidc_provider=github-actions-ambient` or `oidc_provider=explicit-env`). Verifies FR-016 + SC-008. **Precondition** (verify at test authoring time): run `waybill sbom scan` locally with `RUST_LOG=info` and confirm at least one INFO-level line reaches stderr; if it doesn't, waybill's tracing subscriber default is misconfigured and T019 must explicitly set up an `EnvFilter::from_default_env().with_default("info")` subscriber in the sign path.
- [ ] T029 [US1] Add integration test `us2b_keyless_stdout_output_is_rejected_at_parse` — subprocess `waybill sbom scan --sign --output -` — assert exit 2 (clap parse-time reject) + stderr contains `"--sign requires --output <file>"`. Reuses m221 US2a's FR-008a validator; extending its clap-conflict list to include `--sign` was T006's responsibility.

**Checkpoint**: US1 shippable. Every FR-001 through FR-016 has code + test coverage. Coverage doc row 2 update deferred to Polish (T032).

---

## Phase 4: CI + docs (Polish + Cross-Cutting)

**Purpose**: Wire the CI job (FR-012), update the coverage doc (FR-013), and close the audit trail.

- [ ] T030 Add new CI job `lint-and-test-keyless-sbom` to `.github/workflows/ci.yml` per contracts/keyless-signing-flow.md + Phase 0 R5 shape. Mirror the existing `lint-and-test-ebpf` job's structure. Env: `WAYBILL_TEST_KEYLESS=1`, `WAYBILL_FULCIO_URL=https://fulcio.sigstage.dev`, `WAYBILL_REKOR_URL=https://rekor.sigstage.dev`. Permissions: `id-token: write, contents: read`. Runs `cargo +stable test --workspace --test cisa_2026_signing`. Job runs on `pull_request` + `push` to `main`.
- [ ] T031 [P] Update `docs/cisa-2026-coverage.md` row 2 (SBOM Author Signature) per FR-013 — cite both `--sign` (Sigstore keyless) AND `--sign-key <PATH>` (static PEM) as satisfying paths across all three emitters. Remove the "pending US2b" language. Add reference to `contracts/keyless-signing-flow.md` in the Notes column. Update the `last-verified` YAML front-matter to the current date.
- [ ] T032 [P] Update `docs/cisa-2026-coverage.md` Appendix A row 2 recipes — add Sigstore Bundle verification recipe: `cosign verify-blob --bundle signed.cdx.json --certificate-identity <expected> --certificate-oidc-issuer <expected>`.
- [ ] T033 [P] Update `waybill-cli/tests/cisa_2026_coverage_matrix.rs` row-2 assertion — the matrix parser reads row 2's verdict; ensure the "opt-in `--sign` or `--sign-key`" wording still passes the annotation-verdicts test (should — both `--sign` and `--sign-key` are ⚠️ opt-in signals the test accepts).
- [ ] T034 [P] Add memory entry: create `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/reference_us2b_completion.md` (or append to `reference_cisa_2026_coverage.md`) noting that US2b landed via feature 222, that `sigstore-trust-root-rustls-tls` was audited-and-rejected on Principle I grounds (aws-lc-rs via tough) with vendored-CTFE + `SigningContext::new()` as the ship path, the Sigstore key rotation cadence (~1x/year), and any operational gotchas (Rekor timeout defaults, staging endpoint URLs).
- [ ] T035 [P] Update `README.md` "Standards & compliance" paragraph to note that both static-key AND Sigstore keyless signing paths are now supported (row 2 fully closed).

---

## Phase 5: Pre-PR gate

- [ ] T036 Run the full pre-PR gate per Constitution §Pre-PR Verification: `./scripts/pre-pr.sh` (chains `cargo +stable clippy --workspace --all-targets -- -D warnings` + `cargo +stable test --workspace --no-fail-fast`). Confirm zero clippy errors + zero warnings AND every test suite reports `ok. N passed; 0 failed`. Report per-target counts per memory `feedback_prepr_gate_full_output`.
- [ ] T037 Verify no unintended goldens changed: `git status waybill-cli/tests/fixtures/` MUST show zero modified files (US2b is opt-in per FR-015; default-path byte-identity preserved). Any golden churn indicates a leaked side-effect on the unsigned emit path — investigate before proceeding.
- [ ] T038 Locally walk quickstart.md end-to-end using `SIGSTORE_ID_TOKEN` fetched via `cosign login`: (a) run `waybill sbom scan --sign --path . --output /tmp/signed.cdx.json` against a small tree with `RUST_LOG=info` env; (b) verify with `cosign verify-blob --bundle /tmp/signed.cdx.json --certificate-identity <you>@<your-issuer> --certificate-oidc-issuer <issuer>`; (c) confirm the three FR-016 INFO fields appear in stderr with correct values (grep `rekor_log_index=` `fulcio_cert_subject=` `oidc_provider=` — all three should hit).

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (Phase 1)**: T001 is COMPLETE (audit FAILED 2026-07-30 — plan pivoted to R1-alt inline). T002 vendors CTFE keys + writes the `sigstore_trust_root.rs` module; T002a documents the vendoring recipe (parallel with T002 tail). T003 is parallel with T001/T002.
- **Foundational (Phase 2)**: T004–T007 sequential (each touches interlocking files). Depends on Phase 1 complete.
- **US1 (Phase 3)**: Depends on Phase 2. Within Phase 3:
  - Tests (T008–T012) can start in parallel with implementation.
  - T013+T014 (types) block T015–T023 (function bodies + wiring).
  - T015–T018 (helper implementations) block T019 (sign_keyless body — the big one).
  - T019 blocks T020 (public wrapper) and T024–T028 (integration tests need the wrapper working).
  - T021+T022+T023 (wiring into sbom/signer.rs + cli/scan_cmd.rs) block T024 (integration test needs the CLI path wired).
- **CI + docs (Phase 4)**: Can start once US1 code compiles (before all tests pass). T031–T035 all parallel.
- **Pre-PR gate (Phase 5)**: Depends on everything else.

### Story dependencies (visualized)

```text
Phase 1 (Setup) ──> Phase 2 (Foundational) ──> Phase 3 (US1: sign_keyless implementation)
                                                    │
                                                    └──> Phase 4 (CI + docs) ──> Phase 5 (Pre-PR gate)
```

### Parallel opportunities within phases

- **Phase 1**: T003 parallel with T001/T002.
- **Phase 2**: All sequential (interlocking Cargo/enum/CLI changes).
- **Phase 3**:
  - T008 + T009 + T011 + T012 parallel (different test fns / different unit-test modules).
  - T013 + T014 parallel (different structs in same file — merge conflict risk if edited concurrently, so serialize in practice).
  - T015 + T016 + T017 + T018 parallel (different helper functions).
  - T021 + T022 + T023 partly parallel (T021/T022 same file, T023 different file).
  - T024–T028 all parallel (different test fns in the same file — serialize the file edits but the actual work can happen in parallel).
- **Phase 4**: T030 sequential; T031 + T032 + T033 + T034 + T035 all parallel (different files).
- **Phase 5**: T036 + T037 sequential; T038 manual.

---

## Implementation Strategy

### MVP scope

The entire US1 phase IS the MVP — there's no "smaller ship" option because keyless signing is a single indivisible unit (CLI flag → OIDC → Fulcio → sign → Rekor → Bundle emit). Splitting would ship broken behavior.

Fastest-first ordering within US1:

1. **Foundational compile** (T004–T007): CLI flag + `SigningMode::Keyless{}` variant compiles cleanly. Nothing works yet.
2. **Fail-close paths** (T008 + T009 + T011): tests + Interactive-branch fail-close land first. These run without network access; useful smoke test that the CLI plumbing is wired.
3. **Types + helpers** (T013–T018): non-signing scaffolding — helpers compile + unit tests pass.
4. **Sign body + wiring** (T019–T023): the actual sigstore-rs integration + CDX/SPDX arm wiring. First point at which `waybill sbom scan --sign` can produce a signed output locally.
5. **Integration tests** (T010 + T024–T029): full happy-path + mutation-detection + FR-016 log-fields tests. Land + green in CI (staging) before merging.

### Incremental delivery

- **Slice 1** (Phase 1 + Phase 2 + T004–T007): ships nothing user-visible; sets up the compile-clean baseline. Not for merge — internal spike.
- **Slice 2** (add T013–T020, T008–T009, T011): ships fail-close + explicit-token happy path (no staging required for local verify). MVP-adjacent — CI staging tests still need Phase 3 completion.
- **Slice 3** (full Phase 3 + Phase 4 T030): ships full `--sign` including ambient GitHub Actions path + staging CI test. **This is the mergeable checkpoint.**
- **Slice 4** (Phase 4 T031–T035 + Phase 5): documentation catch-up + pre-PR gate. Final PR-ready state.

### If timeboxed

- **Cannot slip**: T019 (sign_keyless body) — this IS the feature.
- Can defer: T028 (FR-016 log-field assertion test) if `tracing`-in-tests infrastructure is missing — recover via manual T038 walkthrough.
- Can defer: T034 memory entry + T035 README bump if time-constrained; not merge-blocking.

---

## Task count summary

| Phase | Count | Story | Notes |
|-------|-------|-------|-------|
| 1 Setup | 3 | — | Audit + feature toggle + staging reachability |
| 2 Foundational | 4 | — | Enum extension + CLI flag + API sanity |
| 3 US1 | 23 | P1 (MVP) | Types, helpers, main body, wiring, tests (T018a research spike added post-analyze) |
| 4 CI + docs | 6 | — | CI job + coverage matrix + README + memory |
| 5 Pre-PR gate | 3 | — | Clippy + tests + walkthrough |
| **Total** | **39** | | |

---

## Format validation

- ✅ All tasks start with `- [ ]` checkbox.
- ✅ All tasks carry a Task ID (T001–T038).
- ✅ User-story-phase tasks carry `[US1]` label; Setup / Foundational / Polish tasks do not.
- ✅ All tasks name at least one file path (or config path where applicable).
- ✅ Parallel-safe tasks marked `[P]`.
- ✅ No leftover placeholder text from the template.
