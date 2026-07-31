# Implementation Plan: Sigstore keyless SBOM signing (completes m221 US2b)

**Branch**: `222-sigstore-keyless-signing` | **Date**: 2026-07-30 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/222-sigstore-keyless-signing/spec.md`

## Summary

Complete the m221-scaffolded `sign_keyless()` at
`waybill-cli/src/attestation/signer.rs:170` (currently returns
`Err(SigningError::KeylessNotImplemented)`) so that
`waybill sbom scan --sign` produces Sigstore-keyless-signed SBOMs:
Sigstore Bundle in CDX `metadata.signature` + `<output>.sig.bundle.json`
sidecar for SPDX 2.3 / SPDX 3.

**Critical Phase 0 finding**: sigstore-rs 0.11 already exposes the
full flow via `bundle::sign::SigningContext`. We call
`ctx.blocking_signer(oidc_token).sign(&mut sbom_bytes)?` and get back
a `SigningArtifact` whose `.to_bundle()` serializes to the exact
protobuf-JSON shape at content-type
`application/vnd.dev.sigstore.bundle+json;version=0.3` — no
hand-rolled Fulcio + Rekor + Bundle protobuf assembly. The m221
research §R6 estimate of ~150 LOC of manual integration is stale;
actual v1 diff will be closer to 100 LOC concentrated in
OIDC-provider dispatch + the ambient GitHub Actions token fetch.

OIDC-token acquisition covers 2 provider variants in v1 per Q1
clarification: **ambient** (GitHub Actions
`ACTIONS_ID_TOKEN_REQUEST_URL` + `ACTIONS_ID_TOKEN_REQUEST_TOKEN`,
audience=`sigstore`) and **explicit** (`SIGSTORE_ID_TOKEN` env var).
Interactive browser flow returns a fail-close diagnostic pointing at
`SIGSTORE_ID_TOKEN` + `cosign` — deferred to v2.

Integration test lives at
`waybill-cli/tests/cisa_2026_signing.rs::us2b_keyless_bundle_sign_and_verify`
(scaffold already in place, `#[ignore]`d pending
`WAYBILL_TEST_KEYLESS=1`). Dedicated CI job
`lint-and-test-keyless-sbom` runs against Sigstore staging
(`fulcio.sigstage.dev` + `rekor.sigstage.dev`) — real network, no
mocks per Q2 clarification.

Success observability per Q3: INFO log 3 structured fields on
sign — `rekor_log_index`, `fulcio_cert_subject`, `oidc_provider` —
via `tracing::info!`. No new SBOM fields (avoids the m071
parity-extractor gate).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited
from milestones 001–221; no nightly required for user-space work).
No MSRV bump.
**Primary Dependencies**: `sigstore = "0.11"` (workspace, already
present at `waybill-cli/Cargo.toml:161`). Existing feature set is
sufficient — `cosign-rustls-tls`, `fulcio-rustls-tls`,
`rekor-rustls-tls`, `bundle` — **no `sigstore-trust-root*` variant**
per Phase 0 R1 (audit revealed `tough` still unconditionally pulls
`aws-lc-rs` at both `0.19` and `0.22`, violating Principle I).
CTFE public keys instead vendored as `&'static [u8]` DER SPKI at
`waybill-cli/vendor/sigstore/ctfe_{prod,stage}.der`, consumed by
`sigstore::crypto::Keyring::new()` + `SigningContext::new()` — both
reachable via the current base feature set. `reqwest = "0.12"`
(workspace, `blocking` + `json` + `rustls-tls`) for the GitHub
Actions ambient OIDC endpoint fetch. `tracing` (workspace) for
FR-016 success observability. `tokio` (workspace, already
pervasive). **Zero new Cargo dependencies at any lockfile layer.**
**Storage**: N/A — all signing state is in-process for the
duration of a single scan. OIDC tokens are single-use (fetched
just-in-time per FR-008); Fulcio certs live 10 minutes and are
Bundle-embedded, not cached; Rekor entries are Bundle-embedded.
**Testing**: `cargo test --workspace` per Constitution Principle
VII (no privilege escalation required for the sign path — runs on
unprivileged CI runners). US2b's integration test is
env-var-gated (`WAYBILL_TEST_KEYLESS=1`) so the general test suite
stays hermetic + fast. Dedicated CI job `lint-and-test-keyless-sbom`
in `.github/workflows/ci.yml` runs it against Sigstore staging.
**Target Platform**: Linux + macOS + Windows (matches m221; sigstore-rs
supports all three under rustls-tls). This feature is user-space
only; `waybill-ebpf` is untouched.
**Project Type**: Rust CLI (three-crate workspace per Principle VI).
**Performance Goals**:
- End-to-end sign time (SC-004): p95 < 30s in CI (GitHub Actions
  ambient path: ~2s OIDC fetch + ~2s Fulcio round-trip + ~3s Rekor
  round-trip including inclusion-proof + ~1s Bundle assembly + scan
  time itself).
- `--sign` unset path throughput unchanged (SC-005 + SC-007 gate).
**Constraints**:
- Byte-identical golden output when `--sign` unset (FR-015 — no
  regression on the default path).
- Signing failure MUST fail-close per m221 FR-009a (Principle III):
  non-zero exit + unlink partial output + no silent unsigned
  fallback.
- OIDC token fetch deferred to as late in the pipeline as possible
  per FR-008 (right before Fulcio submission, NOT at CLI parse
  time — protects token validity window against long scans).
- Rekor transparency-log inclusion is a hard requirement per FR-007
  (WAYBILL_REKOR_TIMEOUT_SECS default 30s; timeout → RekorError →
  fail-close).
- Interactive browser flow explicitly deferred to v2 per Q1
  clarification — v1 returns fail-close diagnostic for the
  `Interactive` provider variant.
**Scale/Scope**: 1 user story, 16 functional requirements (15
original + FR-016 from Q3), 8 success criteria. Estimated diff:
~150 LOC production + ~200 LOC tests + 1 new CI workflow entry +
minor Cargo.toml feature toggle.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Applies? | Verdict | Notes |
|-----------|----------|---------|-------|
| I. Pure Rust, Zero C | ✅ | PASS | Phase 0 R1 audit (2026-07-30) confirmed `sigstore-trust-root-rustls-tls` is not viable (pulls `aws-lc-rs` transitively via `tough`). Adopted vendored-CTFE + `SigningContext::new()` path — zero net-new C-native transitives. Verified by the existing `no_c_dependencies_in_tree` regression test which continues to pass post-implementation. |
| II. eBPF-Only Observation | ➖ | N/A | Signing is a post-scan metadata operation; discovery is untouched. |
| III. Fail Closed | ✅ | PASS | FR-007 (Rekor mandatory), FR-009 (interactive → fail-close), and m221 FR-009a (unlink partial output on any signing error) all enforced. No silent unsigned fallback ever. |
| IV. Type-Driven Correctness | ✅ | PASS | Reuses existing m006 `SigningIdentity::Keyless{}` variant + `SigningError::{OidcTokenError,FulcioError,RekorError,CryptoError}` enum; sigstore-rs's `IdentityToken` newtype prevents raw-JWT-string boundary crossings; no `.unwrap()` in production paths. |
| V. Specification Compliance | ✅ | PASS | CISA 2026 § SBOM Author Signature (per constitution v2.1.0 amendment after m221 merged): completing US2b moves row 2 of `docs/cisa-2026-coverage.md` from "opt-in `--sign-key` only, `--sign` pending US2b" to "either path satisfies." No new `waybill:*` annotations per Q3 (INFO log only) — Principle V bullet 5 (native-fields-first) trivially satisfied. |
| VI. Three-Crate Architecture | ✅ | PASS | All new code lands in `waybill-cli`. No new crates. |
| VII. Test Isolation | ✅ | PASS | Sign path runs without root/CAP_BPF. Integration test env-var-gated so unprivileged `cargo test --workspace` stays green; CI runs the network-dependent test in an isolated job that doesn't block unrelated PRs. |
| VIII. Completeness | ➖ | N/A | Metadata-only feature. |
| IX. Accuracy | ➖ | N/A | Metadata-only feature. |
| X. Transparency | ✅ | PASS | FR-016 (INFO log rekor_log_index + fulcio_cert_subject + oidc_provider on successful sign) is exactly the transparency signal Principle X requires; per-variant `SigningError::*` diagnostics carry actionable failure classes. |
| XI. Enrichment | ➖ | N/A | Metadata-only feature. |
| XII. External Data Source Enrichment | ⚠️ | PASS w/ note | Sigstore Fulcio + Rekor + OIDC providers are external services queried during `--sign`. Constraint 3 ("external source unavailability MUST NOT prevent SBOM generation") is intentionally overridden per FR-007/FR-009a + operator's explicit opt-in — inherited from m221 signing plumbing. |

**Result**: PASS. Phase 0 R1 (2026-07-30) completed: the
`sigstore-trust-root-rustls-tls` feature-toggle path was rejected on
empirical `cargo tree` evidence; the vendored-CTFE + `SigningContext::new()`
alternative preserves Principle I with zero net-new C-native
transitives and adds ~30 min/year of Sigstore key-rotation cost.
Zero gate violations.

## Project Structure

### Documentation (this feature)

```text
specs/222-sigstore-keyless-signing/
├── plan.md                                    # This file
├── spec.md                                    # /speckit.specify + /speckit.clarify output
├── research.md                                # Phase 0 (this command)
├── data-model.md                              # Phase 1 (this command)
├── quickstart.md                              # Phase 1 (this command)
├── contracts/                                 # Phase 1 (this command)
│   ├── oidc-provider-dispatch.md              # Ambient / Explicit token acquisition contract
│   └── keyless-signing-flow.md                # OIDC → Fulcio → sign → Rekor → Bundle sequence
├── checklists/
│   └── requirements.md                        # /speckit.specify output (all 15/15 PASS)
└── tasks.md                                   # /speckit.tasks output (NOT created by this command)
```

### Source Code (repository root)

```text
waybill-cli/
├── Cargo.toml                                 # +sigstore-trust-root-rustls-tls feature
├── src/
│   ├── attestation/
│   │   └── signer.rs                          # Complete sign_keyless() (~100 LOC + tests)
│   │                                          # New helper: fetch_github_actions_oidc_token()
│   │                                          # New helper: resolve_identity_token() dispatcher
│   ├── sbom/
│   │   └── signer.rs                          # Wire keyless path into
│   │                                          # sign_cdx_document_in_place +
│   │                                          # sign_spdx_bytes_to_dsse (currently only
│   │                                          # StaticKey branch active)
│   └── cli/
│       └── scan_cmd.rs                        # Extend SigningMode Unsigned/StaticKey enum
│                                              # with Keyless{ fulcio_url, rekor_url }
│                                              # variant; wire from --sign flag
├── tests/
│   └── cisa_2026_signing.rs                   # Un-#[ignore] us2b_keyless_bundle_sign_and_verify;
│                                              # env-var-gated skip when
│                                              # WAYBILL_TEST_KEYLESS unset
docs/
└── cisa-2026-coverage.md                       # Row 2 (SBOM Author Signature) update per FR-013

.github/workflows/
└── ci.yml                                     # +lint-and-test-keyless-sbom job with
                                                # permissions: id-token: write,
                                                # env: WAYBILL_TEST_KEYLESS=1 +
                                                # sigstage endpoint overrides
```

**Structure Decision**: Fill the existing m006 scaffold verbatim.
The `SigningIdentity::Keyless{}` enum variant, `SigningError`
variants, and CLI dispatch layer are all already wired to accept
the keyless path — this feature swaps the `Err(KeylessNotImplemented)`
placeholder for the sigstore-rs `SigningContext::blocking_signer(token).sign(&bytes)`
call chain plus the OIDC-provider dispatcher.

The `--sign` CLI flag was designed but never emitted per m221's US2a
scope — extend `SigningMode` in `waybill-cli/src/sbom/signer.rs` to
add a `Keyless{...}` variant alongside the existing `Unsigned` and
`StaticKey{...}` variants. The `sign_cdx_document_in_place` +
`sign_spdx_bytes_to_dsse` entrypoints already `match mode {}` — add
the third arm.

Existing test scaffold (`us2b_keyless_bundle_sign_and_verify` at
`waybill-cli/tests/cisa_2026_signing.rs:296`) becomes the acceptance
test verbatim after un-`#[ignore]`-ing + adding a
`WAYBILL_TEST_KEYLESS` env-var check at test entry.

The dedicated `lint-and-test-keyless-sbom` CI job pattern-matches
with the existing `lint-and-test-ebpf` job (which similarly runs a
feature-gated integration test in isolation). Copy that job's shape
verbatim with different env vars.

## Complexity Tracking

> Populated only if Constitution Check has violations that must be justified.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| _none_ | — | — |

## Phase Progression

- [x] Phase 0: research.md generated (5 research items resolved)
- [x] Phase 1: data-model.md, contracts/, quickstart.md generated + agent context updated
- [x] Constitution re-check post-design: still PASS (R1 pivot plan
  documented in research if `sigstore-trust-root-rustls-tls`
  brings back C-native transitives)

## Follow-ups (out-of-scope for this branch)

- **Interactive browser OIDC flow** (deferred per Q1 clarification):
  ship in v2 if operator demand emerges. sigstore-rs 0.11 already
  exposes `oauth::openidflow::OpenIDAuthorize` + `RedirectListener`
  for the browser flow (see `sigstore-0.11.0/examples/bundle/main.rs`);
  v2 would wire it into the existing `OidcProvider::Interactive`
  variant. Scope estimate: ~150 LOC + browser-launching UX +
  TTY-detection guard + mocked-browser test infrastructure. Not
  needed for the CI-native default use case.
- **Bundled `waybill sbom verify --bundle <path>` subcommand**:
  would let operators verify without a separate cosign install.
  Requires wiring sigstore-rs's `bundle::verify` module. Nice-to-have;
  verify tooling in the ecosystem (cosign, Kyverno, admission
  controllers) is already mature.
- **Row 17 CDX asymmetry closure**: `waybill:component-version-unknown`
  per-component annotation to distinguish "unknown" from "withheld"
  on the CDX side. Separate small feature; not this branch's scope.
