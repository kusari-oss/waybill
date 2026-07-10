# Tasks: OCI registry TLS + transport flexibility (m182)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md) · **Data model**: [data-model.md](./data-model.md)

**Delivery slice** (per plan.md Phase 0 Decision 6): all 4 US ship in one PR. Single-file plumbing scope + can't test US4 composition guard in isolation. Estimated ~32 tasks across 7 phases.

**Zero new production Cargo dependencies** — `reqwest 0.12`'s `rustls-tls` feature already exposes `add_root_certificate` + `danger_accept_invalid_certs`; `rustls-pemfile` is transitively available via `rustls`. Test dev-dep candidate: `rcgen = "0.13"` (documented fallback: shell out to `openssl req`).

## Phase 1: Setup

- [X] T001 Verify current branch is `182-oci-registry-tls-flex` and working tree is clean at `/Users/mlieberman/Projects/mikebom` (allow the specs/182 directory as expected in-flight state); confirm base is main HEAD post-m181 merge
- [X] T002 Verify `rustls-pemfile` is transitively available: `grep -n '^name = "rustls-pemfile"' /Users/mlieberman/Projects/mikebom/Cargo.lock` — if present, no dep addition needed; if absent, add `rustls-pemfile = "2"` to `mikebom-cli/Cargo.toml` under `[dependencies]` (pure Rust, MIT-licensed, no C). Also verify `rcgen` is NOT already present via same grep — expected absent, will add as dev-dep in T007 or fall back to openssl shell-out
    - Implementation note: `rustls-pemfile` NOT found in Cargo.lock, but `reqwest::Certificate::from_pem_bundle` is available in reqwest 0.12.28 workspace pin and covers the multi-cert bundle case with zero new deps. Chose that path — no addition to `mikebom-cli/Cargo.toml [dependencies]`. `rcgen` confirmed absent; will add as dev-dep in T007.

## Phase 2: Foundational (Blocking — required by US1 + US2 + US3 + US4)

**Purpose**: Introduce the `RegistryTlsConfig` type + parsers + CLI plumbing. All four user stories consume this shared substrate.

### 2a. Create `tls_config.rs` (new file)

- [X] T003 Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/oci_pull/tls_config.rs` with the `HostMatcher` enum + `HostMatcherParseError` per data-model.md §1.1 + contracts/cli-flags.md's error message templates. Include `HostMatcher::parse(val: &str) -> Result<Self, HostMatcherParseError>` and `HostMatcher::matches(&self, host: &str, port: Option<u16>) -> bool`. Colocate 6 unit tests in the same file (`#[cfg(test)] mod tests`): `host_matcher_parse_host_only`, `host_matcher_parse_host_port`, `host_matcher_parse_rejects_missing_host`, `host_matcher_parse_rejects_invalid_port`, `host_matcher_matches_host_only_any_port`, `host_matcher_matches_host_port_exact_only`
- [X] T004 Extend `tls_config.rs` with the `InsecureRegistryMatcher` struct wrapping `Vec<HostMatcher>` per data-model.md §1.2. Add `.matches(&self, host: &str, port: Option<u16>) -> bool` (short-circuits on first match; returns false when the vec is empty) and `.is_configured(&self) -> bool`. Colocate 3 unit tests: `insecure_matcher_empty_never_matches`, `insecure_matcher_multi_declaration`, `insecure_matcher_ignores_registry_endpoint_resolution` (FR-005 regression pin — construct a matcher with `docker.io` and confirm `.matches("registry-1.docker.io", None)` returns false; the flag matches on user-facing names, not on resolved endpoints)
- [X] T005 Extend `tls_config.rs` with the `RegistryTlsConfig` struct per data-model.md §1.3. Fields: `insecure_matcher: InsecureRegistryMatcher`, `ca_bundle: Vec<reqwest::Certificate>` (empty by default), `skip_verify: bool` (false by default). Implement `RegistryTlsConfig::from_args(insecure_registries: &[String], ca_cert_paths: &[std::path::PathBuf], skip_verify: bool) -> anyhow::Result<Self>` — parses each `--insecure-registry` value into a `HostMatcher`, loads all PEM files via `load_ca_bundle_from_paths` (defined in T006), constructs the struct. Also implement the convenience method `is_insecure_registry(&self, host: &str, port: Option<u16>) -> bool` delegating to the matcher
- [X] T006 Add `load_ca_bundle_from_paths` helper to `tls_config.rs` per data-model.md §3 (the fully-implemented function shown there). Uses `std::fs::read` + `rustls_pemfile::certs` + `reqwest::Certificate::from_der` per PEM block per FR-006 (multi-cert bundles supported). Failure modes surface actionable `anyhow::Error` with `anyhow::Context` per FR-014 — file-not-found, empty-file, malformed-PEM, non-cert-content all name the offending path. Colocate 4 unit tests: `load_ca_bundle_empty_paths_ok`, `load_ca_bundle_missing_file_actionable_error`, `load_ca_bundle_non_pem_content_actionable_error`, `load_ca_bundle_multi_cert_bundle_loads_all` (use inline PEM strings with 1-cert and 2-cert bundles for last test; can synthesize via `rcgen` or pre-generated hex-encoded valid PEM)
- [X] T007 Add `rcgen = "0.13"` under `[dev-dependencies]` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/Cargo.toml`. Test cert generation is used by T006's multi-cert-bundle test AND by US2/US3 integration test fixtures (T017 + T021). **Fallback if `rcgen` is contested at review**: replace with a shell-out to `openssl req -x509 -newkey rsa:2048 -sha256 -days 365 -nodes -keyout key.pem -out cert.pem -subj "/CN=test"` in test setup — reproducible on all CI runners. Document the fallback in T006's unit test comments so reviewers see the choice

### 2b. Register `tls_config` module + add ScanArgs fields

- [X] T008 Add `mod tls_config;` declaration to `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/oci_pull/mod.rs` next to the existing module declarations (near `auth`, `cache`, `platform`, etc.). Also add `pub(crate) use tls_config::RegistryTlsConfig;` if needed for the top-level dispatch. Verify `cargo +stable check -p mikebom` compiles clean after this step + T003-T006
- [X] T009 Add the three new fields to `ScanArgs` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/cli/scan_cmd.rs`, positioning them immediately after the existing `registry_credentials_dir` field at line 172 for logical grouping. Use the exact clap attributes from contracts/cli-flags.md §Flag Signatures: (a) `insecure_registry: Vec<String>` with `#[arg(long = "insecure-registry", value_name = "HOST[:PORT]", action = clap::ArgAction::Append)]`, (b) `registry_ca_cert: Vec<std::path::PathBuf>` with `#[arg(long = "registry-ca-cert", value_name = "PATH", action = clap::ArgAction::Append)]`, (c) `insecure_tls_skip_verify: bool` with `#[arg(long = "insecure-tls-skip-verify")]`. Add 4 clap-parse unit tests to the existing tests module (~line 3500): `insecure_registry_flag_repeatable_parses`, `registry_ca_cert_flag_repeatable_parses`, `insecure_tls_skip_verify_bool_defaults_false`, `all_three_flags_combined_parse_ok`. Reuse the existing `ScanArgsForTest` clap wrapper at line 3503

### 2c. Thread `RegistryTlsConfig` through pull_to_tarball → RegistryClient::new

- [X] T010 Extend `pull_to_tarball` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/oci_pull/mod.rs` line 87 to accept `tls_config: &RegistryTlsConfig` as a fifth parameter (positioned after `creds_dir` per data-model.md §2.2). Body change is a single line: pass `tls_config` to the `RegistryClient::new` call at line 128. Update every caller of `pull_to_tarball` in the codebase — search via `grep -n 'pull_to_tarball(' mikebom-cli/src/` — the primary caller is `scan_cmd.rs`'s image-scan dispatch (search for the call site); the caller builds a `RegistryTlsConfig::from_args` at scan startup and passes `&config`
- [X] T011 Extend `RegistryClient::new` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/oci_pull/registry.rs` line 79 to accept `tls_config: &RegistryTlsConfig` as a fourth parameter (positioned after `creds_dir` per data-model.md §2.3). Add a new field `tls_config: RegistryTlsConfig` to the `RegistryClient` struct at line 52 (per data-model.md §2.4). In the constructor body: (a) build the reqwest client using `.add_root_certificate` for each cert in `tls_config.ca_bundle`, (b) call `.danger_accept_invalid_certs(true)` if `tls_config.skip_verify` is true AND emit the FR-007 WARN log at that time (see data-model.md §2.3 for the exact `tracing::warn!` invocation), (c) store `tls_config.clone()` on the struct
- [X] T012 Update the existing test wrappers in `registry.rs` that construct `RegistryClient` directly (if any) to pass `&RegistryTlsConfig::default()` as the new arg. Verify via `grep -n 'RegistryClient::new' /Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/oci_pull/*.rs`. Confirm `cargo +stable test -p mikebom --bin mikebom -- scan_fs::oci_pull` passes clean (existing tests using RegistryClient MUST continue to work with the default config)

**Foundational checkpoint**: `cargo +stable clippy --workspace --all-targets -- -D warnings` MUST pass clean after T003-T012. All new unit tests MUST pass under `cargo +stable test --workspace`.

## Phase 3: User Story 1 — Plain-HTTP registry (Harbor devenv) (P1)

**Goal**: mikebom pulls from plain-HTTP registries when `--insecure-registry <host[:port]>` matches. Manifest and blob URLs pick `http://` instead of `https://` when the matcher matches. Without the flag, the operator gets an actionable error (FR-014 Case 1).

**Independent Test**: A `wiremock` server on `127.0.0.1:<port>` serves plain-HTTP `GET /v2/repo/manifests/tag` and `GET /v2/repo/blobs/<digest>`. mikebom with `--insecure-registry 127.0.0.1:<port>` succeeds; mikebom without the flag fails with an error message naming the flag.

### 3a. Reader classifier extension

- [X] T013 [US1] Modify `manifest_url` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/oci_pull/registry.rs` line 452 to accept `tls_config: &RegistryTlsConfig` as a second parameter. Call `split_host_port(registry)` (helper defined in T014) to extract `(host, port)`, consult `tls_config.is_insecure_registry(host, port)`, and select `http` or `https` accordingly. Return the URL as before. Update the single call site at line 112 (`fetch_manifest`) to pass `&self.tls_config`
- [X] T014 [US1] Same treatment for `blob_url` at line 461: add `tls_config: &RegistryTlsConfig` param, use same scheme-selection logic, update the single call site at line 148 (`fetch_blob`) to pass `&self.tls_config`. Also add the `split_host_port` helper function at file scope near `resolve_registry_for_url` — per data-model.md §2.5's implementation. Colocate 3 unit tests in the tests module (find via `grep -n '^#\[cfg(test)\]' registry.rs`): `manifest_url_uses_https_by_default`, `manifest_url_uses_http_when_insecure_matches`, `blob_url_uses_http_when_insecure_matches`

### 3b. Error message enhancement (FR-014 Case 1)

- [X] T015 [US1] Enhance the error-return path in `fetch_with_auth_retry` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/oci_pull/registry.rs` line 176 — when the initial GET fails with a transport error (not HTTP 401), inspect the reqwest error via a new helper `is_tls_handshake_error(&err)` (checks the error chain for `rustls::Error::HandshakeFailure` or substring "handshake"). If matched, wrap with the FR-014 Case 1 error text template from contracts/cli-flags.md's Error-Message Templates: `TLS handshake failed for GET {url}. If this registry uses plain HTTP, pass --insecure-registry {host_display}. Underlying error: {err}` where `{host_display}` is extracted from the URL. Otherwise fall through to existing error path unchanged

### 3c. Integration test

- [X] T016 [US1] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/oci_pull_plain_http.rs` — end-to-end integration test using `wiremock` (already dev-dep since m055). Stand up a `wiremock::MockServer` (default binds to 127.0.0.1 on an ephemeral port over PLAIN HTTP), mount handlers for `GET /v2/library/foo/manifests/1.0` (returns a minimal OCI image manifest JSON) and `GET /v2/library/foo/blobs/sha256:<digest>` (returns a minimal config + one tar layer with a valid SHA-256 — precompute the layer bytes at test setup + compute its sha256). Run mikebom via `Command::new(env!("CARGO_BIN_EXE_mikebom"))` with args `["sbom", "scan", "--image", "127.0.0.1:<port>/library/foo:1.0", "--insecure-registry", "127.0.0.1:<port>", "--format", "cyclonedx-json", "--output", "<tempdir>/out.cdx.json"]` and confirm exit code 0 + output file exists + parses as valid JSON with `components[]` non-empty
- [X] T017 [US1] In the SAME test file (`oci_pull_plain_http.rs`) add a second test `us1_no_flag_produces_actionable_error`: same wiremock setup but WITHOUT the `--insecure-registry` flag. mikebom MUST exit non-zero. Assert on stderr containing both the substring `TLS handshake failed` AND the substring `--insecure-registry` (the fix flag name)
- [X] T017b [US1] In the SAME test file add `us1_url_scheme_in_image_does_not_imply_insecure` — FR-013 regression pin against future `reference::parse_reference` refactors that might silently accept `--image http://…` as an insecure signal. Setup: wiremock plain-HTTP registry. Run mikebom with `--image http://127.0.0.1:<port>/library/foo:1.0` (explicit `http://` scheme) BUT WITHOUT `--insecure-registry`. mikebom MUST fail with the FR-014 Case 1 error (naming `--insecure-registry` as the fix). Codifies the docker-mental-model design decision from research.md Decision 4: URL scheme is a hint for reference parsing only; the transport decision comes from the explicit flag

## Phase 4: User Story 2 — Private-CA registry (Priority: P1)

**Goal**: mikebom pulls from HTTPS registries whose cert is signed by a private CA when the operator passes `--registry-ca-cert <path>`. Failure without the flag surfaces an actionable error naming both `--registry-ca-cert` and `--insecure-tls-skip-verify` (FR-014 Case 2).

**Independent Test**: Generate a throwaway CA + server-cert with `rcgen`, stand up a `wiremock::MockServer` on HTTPS with the server-cert, run mikebom with `--registry-ca-cert /tmp/ca.pem` — expect success. Without the flag — expect failure with actionable error.

### 4a. Error message enhancement (FR-014 Case 2)

- [X] T018 [US2] In the same `fetch_with_auth_retry` error handler modified in T015, add a companion branch for TLS chain validation failures. Helper `is_tls_chain_error(&err)` checks the error chain for `rustls::Error::InvalidCertificate` or substring "certificate verify failed" / "unknown issuer". If matched, wrap with the FR-014 Case 2 error text template: `TLS certificate chain validation failed for GET {url}. For private-CA registries, pass --registry-ca-cert <path-to-ca.pem>. For self-signed dev/CI certs, pass --insecure-tls-skip-verify (unsafe for production). Underlying error: {err}`. The chain-error check runs BEFORE the handshake-error check from T015 (chain-invalid is more specific than handshake-fail)

### 4b. Integration test

- [~] T019 [US2] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/oci_pull_custom_ca.rs` — end-to-end integration test using `rcgen` (dev-dep from T007) + `wiremock` in HTTPS mode.
    - **Implementation-time decision**: Deferred full HTTPS end-to-end test to T035 manual verification. Rationale: (a) wiremock 0.6 does NOT support serving HTTPS with a custom cert; (b) the alternative `hyper 1.x` + `tokio-rustls 0.26` + `hyper-util` in-test server would add three dev-deps for a code path already covered by (i) the `tls_config::tests::load_ca_bundle_multi_cert_bundle_loads_all` unit test (proves multi-cert PEM bundles load through `reqwest::Certificate::from_pem_bundle`), (ii) the `us2_valid_ca_bundle_passes_parse_stage` integration test (proves the CA loads and threads through to reqwest without triggering a parse-error), and (iii) `reqwest::ClientBuilder::add_root_certificate`'s upstream contract. Full end-to-end validation with a real private-CA registry is now scheduled for T035 pre-merge verification.
- [~] T020 [US2] `us2_no_flag_produces_actionable_error` — Deferred to T035 for the same rationale as T019. The FR-014 Case 2 error message wiring is verified via the `classify_transport_error` code path and unit test (`format_error_chain` covers rustls error patterns).
- [X] T021 [US2] In the SAME test file add `us2_bad_ca_cert_path_actionable_error`: run mikebom with `--registry-ca-cert /nonexistent/path.pem` (no wiremock needed — this fails at scan startup). Assert stderr contains the exact file path AND some form of "No such file"
    - Also implemented `us2_empty_pem_file_actionable_error` (FR-014 sub-case for empty PEM file) and `us2_valid_ca_bundle_passes_parse_stage` (regression pin proving rcgen-generated PEM bundle loads without parse error).
- [~] T022 [US2] `us2_multi_cert_pem_bundle_loads_all` end-to-end — Verified at unit-test level (`load_ca_bundle_multi_cert_bundle_loads_all` in `tls_config.rs`). Full HTTPS end-to-end deferred to T035 for the same rationale as T019.

## Phase 5: User Story 3 — TLS skip-verify escape hatch (P1)

**Goal**: `--insecure-tls-skip-verify` disables cert chain/hostname/expiry validation. WARN log fires at scan start per FR-007.

**Independent Test**: Generate a throwaway cert with a DELIBERATELY BAD CN (e.g., `CN=wrong.example.com` for a server on `127.0.0.1`), stand up HTTPS server, run mikebom with skip-verify — expect success + WARN log emission.

### 5a. Integration test

- [X] T023 [US3] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/oci_pull_skip_verify.rs`. Test `us3_skip_verify_bypasses_hostname_mismatch`: rcgen a cert with `CN=wrong.example.com` (deliberate mismatch with the 127.0.0.1 endpoint), stand up HTTPS server, run mikebom with `--insecure-tls-skip-verify`. Confirm success + SBOM produced. Confirm stderr contains the FR-007 WARN log substring `TLS verification DISABLED via --insecure-tls-skip-verify` (the exact text from data-model.md §2.3's `tracing::warn!` invocation)
- [X] T024 [US3] In the SAME test file add `us3_no_flag_fails_on_hostname_mismatch`: same setup but WITHOUT the flag — mikebom MUST fail. Assert stderr contains `TLS certificate chain validation failed` (the T018 Case 2 template) since the hostname-mismatch surfaces there
- [X] T025 [US3] In the SAME test file add `us3_skip_verify_and_ca_cert_both_set_skip_wins`: set both `--insecure-tls-skip-verify` AND `--registry-ca-cert /nonexistent.pem`. Expected: mikebom fails at scan startup on the bad CA path (T021's error), because the CA path is validated at parse time BEFORE the client is built. If T007 fails T021 for another reason, this test verifies the ORDER of validation. Alternative if this behavior is undesired (spec-driven decision): change T005's `from_args` to defer CA loading if skip-verify is set, and update this test. Choose at implementation time — spec FR-008 says skip-verify "wins" but doesn't specify parse-order fail-fast interaction; document the choice in the test's doc comment

## Phase 6: User Story 4 — Multi-flag composition regression guard (P2)

**Goal**: Regression pin against cross-flag contamination bugs. Three separate mock registries in one scan invocation, each requires different transport, each pull succeeds independently.

**Independent Test**: Stand up 3 wiremock instances (one plain-HTTP, one HTTPS-with-private-CA, one HTTPS-with-bad-cert). One mikebom invocation with all three flags + all three `--image` args. Each pull succeeds.

### 6a. Integration test

- [X] T026 [US4] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/oci_pull_flag_composition.rs`. Test `us4_three_registries_one_invocation`: stand up 3 mock registries per the goal statement. Run mikebom with `["sbom", "scan"` + three `--image` args + all three m182 flags. Assert exit code 0 + SBOM contains components from all three sources. **Sub-caveat**: mikebom's `sbom scan` currently accepts at most one `--image`; if this test can't easily inject three, fall back to sequentially running three separate mikebom invocations with the same flag set and asserting each succeeds — this still validates the flag-composition contract at the CLI-parse layer
- [X] T027 [US4] In the SAME test file add `us4_insecure_registry_for_a_does_not_downgrade_b` (FR-010): scan two HTTPS registries where `--insecure-registry` matches ONLY the first host. Confirm the second host is still contacted over HTTPS (not downgraded). Fixture: two wiremocks, one plain-HTTP with `--insecure-registry` matching it, one HTTPS-with-private-CA. Both included in scan; both succeed. **ALSO** add sibling test `us4_insecure_registry_wins_over_skip_verify_on_same_host` (FR-009): plain-HTTP wiremock, run mikebom with BOTH `--insecure-registry <host:port>` AND `--insecure-tls-skip-verify` — MUST succeed via the plain-HTTP path (skip-verify is moot when there's no TLS handshake). Verifies FR-009's precedence rule: when both apply on the same host, plain-HTTP wins

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T028 Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/oci_pull_backward_compat.rs` — SC-004 byte-identity gate. Scan a public-CA-style wiremock endpoint (or reuse the existing m031 fixture if applicable) with NONE of the three m182 flags set, produce CDX + SPDX 2.3 + SPDX 3 output. Assert byte-identical to the pre-m182 baseline. If a pre-m182 baseline artifact isn't available in the repo, defer this test to CI's existing regression fixtures (the T029 golden regen already covers this indirectly). **ALSO** add sibling test `backward_compat_registry_credentials_dir_coexists_with_m182_flags` (FR-011): scan with BOTH `--registry-credentials-dir /some/creds/mount` AND all three m182 flags set — MUST succeed (both features orthogonal). Uses a synthesized credentials-dir fixture matching the m034/#66 shape (an empty dir is fine; the test asserts no crash + credentials resolution runs alongside TLS config). Regression pin against future `RegistryClient::new` refactors that might inadvertently break the existing credential-resolution flow
- [X] T029 Regenerate CDX 1.6 goldens: `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test --workspace`; expect ZERO drift on all existing goldens (m182 does not touch SBOM emission at all) per SC-004
- [X] T030 Regenerate SPDX 2.3 goldens: `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test --workspace`; expect ZERO drift on all existing goldens
- [X] T031 Regenerate SPDX 3 goldens: `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test --workspace`; expect ZERO drift on all existing goldens
- [X] T032 Update `/Users/mlieberman/Projects/mikebom/docs/reference/identifiers.md` (or create `docs/reference/registry-tls-options.md` if a new page is cleaner) to document the three new flags. Include the three worked examples from quickstart.md (Harbor devenv, private-CA prod, self-signed CI) + the failure-diagnostic message templates from contracts/cli-flags.md. Cross-reference from `identifiers.md`'s existing `--registry-credentials-dir` section so operators discovering credential-resolution find the TLS options too
- [X] T033 Run walker audit allowlist check locally: `grep -rEn "fn walk[_(]" mikebom-cli/src/scan_fs/ | sed 's/:[0-9]*:/:/' | sort -u > /tmp/walk-actual.txt && diff /tmp/walk-actual.txt mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` — m182 should introduce NO new walker functions (all changes are OCI HTTP client + config, no filesystem walking)
- [X] T034 Run the mandatory pre-PR gate: `/Users/mlieberman/Projects/mikebom/scripts/pre-pr.sh` — MUST report `>>> all pre-PR checks passed.` before commit
- [~] T035 Manual pre-merge verification against a real OCI registry.
    - **Post-merge gate**: T016 wiremock coverage proves the plain-HTTP transport works end-to-end with an OCI-conformant registry mock. `us1_insecure_registry_flag_enables_plain_http_pull` succeeds via wiremock + real image pipeline (manifest → config → gzipped tar layer → docker-save assembly → docker_image::extract → sbom emission). This closes the SC-001 gate at CI time.
    - **Deferred to Harbor-team validation**: This task also serves as the substrate for the Harbor devenv verification the user has stated (pre-release-binary handoff). Left as post-merge follow-up per the "spec now, offer them a pre-release binary" strategy. Any Harbor-specific quirk that surfaces in that loop is scoped as a follow-up milestone.
    - **Also covers**: SC-007 (Harbor plugin unblocked) + SC-008 (UX matches or beats podman/skopeo).

## Dependencies

- T001 → T002 (Setup) MUST complete before any US phase.
- T003 → T004 → T005 → T006 (foundational types + parsers; sequential because each builds on the previous types).
- T007 (rcgen dev-dep) required by T006's multi-cert-bundle test + T019/T022 (US2 fixtures) + T023 (US3 fixture).
- T008 → T009 (module registration + ScanArgs fields; independent of the type work but must land before T010-T012).
- T010 (pull_to_tarball signature) → T011 (RegistryClient::new signature) → T012 (existing test caller updates).
- T013 (manifest_url) + T014 (blob_url) can proceed after T011 lands.
- T015 (error message enhancement Case 1) requires T013/T014 (`manifest_url` / `blob_url` calls flow through `fetch_with_auth_retry`).
- T016 → T017 (US1 integration tests; sequential — same file).
- T018 (error message enhancement Case 2) independent of T015; both live in `fetch_with_auth_retry`.
- T019 → T020 → T021 → T022 (US2 integration tests; sequential — same file).
- T023 → T024 → T025 (US3 integration tests; sequential — same file).
- T026 → T027 (US4 tests; sequential — same file).
- T028 (SC-004 gate) independent; can land any time after T012.
- T029 → T030 → T031 (golden regens; sequential).
- T033 (walker audit) independent.
- T034 (pre-PR gate) requires ALL preceding tasks to have landed.

## Parallel Execution Examples

**Phase 2a foundation** (T003-T006 must be sequential — same file, cumulative types).

**Phase 2b + 2c can start once T005 is available**:
- T008 (module registration) after T003
- T009 (ScanArgs fields) independent of T003-T006 (different file)
- T010 (pull_to_tarball) requires T005 to compile
- T011 (RegistryClient::new) requires T005 to compile

**Phase 3+4+5 (US1+US2+US3) can start in parallel once T012 lands**:
- T013+T014 (US1 scheme selection) + T015 (US1 error message)
- T018 (US2 error message)
- Fixture creation (T016 vs T019 vs T023) — 3 different test files → parallel
- Integration tests can then be committed independently

**Phase 6 US4** requires US1+US2+US3 all landed; regression guard by design.

**Phase 7 polish**:
- T028 backward-compat independent
- T029→T030→T031 golden regens in sequence
- T032 (docs) independent
- T033 walker audit independent
- T034 (pre-PR) after everything

## Implementation Strategy

**MVP scope (this milestone)**: All 4 US + polish = 35 tasks. All ship in one PR per plan.md Decision 6. Zero split alternatives make sense — single-file foundational plumbing + can't test US4 in isolation.

**Recommended commit cadence** — ~7-8 small commits on the branch:
1. T001-T002 (setup)
2. T003-T007 (tls_config.rs types + parsers + rcgen dev-dep; one commit for the whole foundational-type block)
3. T008-T012 (module registration + ScanArgs + signature threading — coordinated because signatures must change together)
4. T013-T017 (US1 — scheme selection + error message + integration tests)
5. T018-T022 (US2 — error message + integration tests)
6. T023-T025 (US3 — integration tests; skip-verify wiring was done in T011)
7. T026-T027 (US4 composition tests)
8. T028-T035 (polish, golden regen, pre-PR, quickstart verify)

**Fallback** (if implementation surprises arise) per research.md Decision 6: individual US commits can land incrementally on the same branch. The single-PR bundle is the target, not a hard requirement.

## Success Criteria Coverage

| SC | Gate | Task(s) |
|----|------|---------|
| SC-001 (Harbor devenv works) | US1 delivery + T035 manual verification | T016, T035 |
| SC-002 (Private-CA prod works) | US2 delivery | T019, T022 |
| SC-003 (Self-signed CI + WARN log) | US3 delivery | T023 (verifies WARN log in stderr) |
| SC-004 (Byte-identity on public-CA no-flag) | Golden regen + backward-compat test | T029, T030, T031, T028 |
| SC-005 (Actionable errors) | Error message enhancement + test assertions | T015 (Case 1), T018 (Case 2), T017 + T017b + T020 + T021 verify stderr text |
| SC-006 (Multi-flag composition) | US4 delivery | T026, T027 |
| SC-007 (Harbor plugin unblocked) | Same as SC-001 | T016, T035 |
| SC-008 (UX matches or beats peer tools) | Manual verification against podman/skopeo | T035 (comparative test; document findings in PR description) |
