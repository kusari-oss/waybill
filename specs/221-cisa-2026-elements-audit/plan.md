# Implementation Plan: CISA 2026 SBOM Minimum Elements coverage audit

**Branch**: `221-cisa-2026-elements-audit` | **Date**: 2026-07-29 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/221-cisa-2026-elements-audit/spec.md`

## Summary

Publish a reproducible per-emitter coverage matrix (CDX 1.6 / SPDX
2.3 / SPDX 3.0.1) against the 17 data-field + 6 practice/process
elements of CISA's 2026-07-29 SBOM Minimum Elements publication,
then close the three confirmed gaps: (1) native SBOM Author
Signature via opt-in `--sign` (Sigstore keyless — Sigstore Bundle
in CDX `signature`; sidecar `.sig.bundle.json` for SPDX 2.3/3) and
`--sign-key` (static key material — JSF in CDX `signature`; DSSE
sidecar for SPDX); (2) document-scope SBOM Generation Context in
SPDX 2.3 and SPDX 3 (CDX already has native `metadata.lifecycles[]`
via m047), with CISA-vocabulary alias `waybill:cisa-2026-lifecycle`;
(3) caller-supplied SBOM Version via `--sbom-version <N>` (integer,
CDX schema-compatible). All work is opt-in; existing goldens stay
byte-identical when the new flags are unset.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from
milestones 001–220; no nightly required for user-space work). eBPF
target unaffected.
**Primary Dependencies**: Existing only — `sigstore = "0.11"` at
`waybill-cli/Cargo.toml:141` with `cosign-rustls-tls`,
`fulcio-rustls-tls`, `rekor-rustls-tls`, `bundle` features already
enabled (m006 sbomit-suite ↦ m089 bump); `serde` / `serde_json`
(JSON emission across three emitters); `tracing` (INFO/WARN logs);
`anyhow` / `thiserror` (error propagation); `clap` (new `--sign`,
`--sign-key`, `--sbom-version` flags via `Args`-derive). **Zero new
Cargo dependencies.**
**Storage**: N/A — all signing state is in-process for the duration
of a single scan. Sigstore fetches Fulcio cert + Rekor entry per
sign; no persistent cache (matches m006 attestation-envelope
posture).
**Testing**: `cargo test --workspace` per Principle VII. New
integration tests in `waybill-cli/tests/` cover: US1 coverage-doc
existence + jq-recipe roundtrip; US2 static-key JSF sign+verify
loop with ephemeral ECDSA keys; US2 keyless golden-path (feature-
gated to CI environments that provide OIDC — GitHub Actions
`id-token: write`); US3 SPDX 2.3/3 generation-context annotation
presence + CISA-vocab alias; US4 `--sbom-version` integer accept /
reject cases. Byte-identity goldens under `waybill-cli/tests/
fixtures/` for FR-009 no-regression coverage.
**Target Platform**: Linux + macOS + Windows (sigstore-rs 0.11
supports all three; already validated per milestones 100 / 101).
This feature is user-space only; `waybill-ebpf` is untouched.
**Project Type**: Rust CLI (three-crate workspace per Principle VI).
**Performance Goals**: Signing overhead:
- Static-key (JSF, ECDSA P-256): p95 < 100 ms per SBOM.
- Keyless (Sigstore Bundle: OIDC fetch + Fulcio cert issue + Rekor
  submit): p95 < 5 s in interactive mode, p95 < 2 s in GitHub
  Actions with the ambient `id-token` (avoids the browser round-
  trip).
Unsigned emission path throughput unchanged (FR-009 byte-identity
requirement precludes any hot-path perturbation).
**Constraints**:
- Byte-identical golden output when signing flags unset (FR-009 —
  golden regen forbidden for the no-op path).
- Signing failure MUST fail-close per FR-009a (Principle III).
- Combining `--sign`/`--sign-key` with `--output -` MUST reject at
  CLI parse (FR-008a).
- `--sign` and `--sign-key` MUST be mutually exclusive (FR-007).
**Scale/Scope**: 4 user stories, 21 functional requirements (17
original + 4 amended via clarifications), 7 success criteria, 51
coverage matrix cells (17 data-field elements × 3 emitters) + 6
practice-mapping cells. Estimated diff: ~600 LOC production + 400
LOC tests + 1 new doc (`docs/cisa-2026-coverage.md` ~350 lines).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Applies? | Verdict | Notes |
|-----------|----------|---------|-------|
| I. Pure Rust, Zero C | ✅ | PASS | sigstore 0.11 + `rustls-tls` features already vetted C-clean via m089; no new deps. Re-verify with `cargo tree` in research §R1. |
| II. eBPF-Only Observation | ➖ | N/A | Feature emits metadata; does not affect discovery. |
| III. Fail Closed | ✅ | PASS | FR-009a mandates non-zero exit + partial-output cleanup on signing failure. Matches Principle III. |
| IV. Type-Driven Correctness | ⚠️ | PASS w/ commitment | New types required: `SbomVersion(NonZeroU32)`, `SigningMode` enum (mirrors existing `SigningIdentity`), `SbomSignatureEnvelope` newtype. No raw `String` across function boundaries per Principle IV. Plan §Phase 1 data-model enforces this. |
| V. Specification Compliance | ⚠️ | PASS w/ audit | **Constitution Principle V currently references "CISA 2025 Minimum Elements"** — this feature updates the target to CISA 2026 (published 2026-07-29). Constitution amendment recommended as follow-up MINOR bump (2.0.0 → 2.1.0). This feature's implementation is the amendment's motivating case, but the constitution edit is out-of-scope for this branch. **Native-fields-first audit** (per Principle V bullet 5): the proposed `waybill:cisa-2026-lifecycle` annotation is parity-bridging: CDX has native `metadata.lifecycles[]` (m047); SPDX 2.3 has no doc-level lifecycle field; SPDX 3 has `LifecycleScopeType` on relationships only, not documents. The annotation MUST be documented in `docs/reference/sbom-format-mapping.md` per Principle V; Phase 1 contracts include the entry. |
| VI. Three-Crate Architecture | ✅ | PASS | All new code lands in `waybill-cli`. No new crates. |
| VII. Test Isolation | ✅ | PASS | Static-key signing tests use ephemeral keys, run without root. Keyless integration test is CI-only (requires OIDC token); unit tests use mock Fulcio/Rekor per sigstore-rs test infra. |
| VIII. Completeness | ➖ | N/A | Metadata-only feature. |
| IX. Accuracy | ➖ | N/A | Metadata-only feature. |
| X. Transparency | ✅ | PASS | Signature failures logged at ERROR with failure class named (FR-009a). Signed vs. unsigned state discoverable by SBOM consumer inspecting document. |
| XI. Enrichment | ➖ | N/A | Metadata-only feature. |
| XII. External Data Source Enrichment | ⚠️ | PASS w/ note | Sigstore Fulcio / Rekor are external services queried during `--sign`. Constraint 3 ("external source unavailability MUST NOT prevent SBOM generation") is intentionally overridden in this feature per FR-009a and per operator's explicit request — signing was opt-in. Note in research §R2. |

**Result**: PASS. Two commitments (Principle IV newtypes, Principle
V doc entry) are enforced by Phase 1 artifacts. One follow-up
(constitution amendment 2025 → 2026 reference) is documented as
out-of-scope; no gate violation.

## Project Structure

### Documentation (this feature)

```text
specs/221-cisa-2026-elements-audit/
├── plan.md                                    # This file
├── spec.md                                    # /speckit.specify + /speckit.clarify output
├── research.md                                # Phase 0 (this command)
├── data-model.md                              # Phase 1 (this command)
├── quickstart.md                              # Phase 1 (this command)
├── contracts/                                 # Phase 1 (this command)
│   ├── cli-flags.md                           # --sign, --sign-key, --sbom-version surfaces
│   ├── coverage-matrix-schema.md              # Structure of docs/cisa-2026-coverage.md
│   └── sbom-emission-contract.md              # Which slot each element populates
├── checklists/
│   └── requirements.md                        # /speckit.specify output
└── tasks.md                                   # /speckit.tasks output (NOT created by this command)
```

### Source Code (repository root)

```text
waybill-cli/
├── Cargo.toml                                 # No changes (sigstore 0.11 already present)
├── src/
│   ├── cli/
│   │   └── generate.rs                        # +--sign, --sign-key, --sbom-version, --output '-' validator
│   ├── attestation/
│   │   ├── envelope.rs                        # Extend DSSE wrap for arbitrary SBOM bytes
│   │   ├── signer.rs                          # Complete sign_keyless (currently scaffolded)
│   │   └── mod.rs                             # New: sbom_signer.rs entry point
│   ├── sbom/
│   │   └── signer.rs                          # NEW: SBOM-level signing (wraps m006 primitives)
│   ├── generate/
│   │   ├── cyclonedx/
│   │   │   ├── builder.rs                     # +metadata.version from --sbom-version;
│   │   │   │                                  # +signature slot post-emit
│   │   │   └── metadata.rs                    # (no change; already emits lifecycles)
│   │   └── spdx/
│   │       ├── document.rs                    # +document-scope Annotation for
│   │       │                                  # generation-context (both 2.3 and 3)
│   │       ├── annotations.rs                 # +waybill:cisa-2026-lifecycle alias
│   │       ├── v3_document.rs                 # +CreationInfo Annotation
│   │       └── v3_annotations.rs              # +waybill:cisa-2026-lifecycle in v3 path
│   └── main.rs                                # +sidecar write for SPDX signatures
├── tests/
│   ├── cisa_2026_coverage_matrix.rs           # NEW: US1 integration test (validates
│   │                                          # every ✅ verdict in the coverage doc)
│   ├── cisa_2026_signing.rs                   # NEW: US2 static-key sign+verify loop
│   ├── cisa_2026_generation_context.rs        # NEW: US3 doc-scope annotation coverage
│   ├── cisa_2026_sbom_version.rs              # NEW: US4 --sbom-version integer cases
│   └── fixtures/
│       └── cisa_2026/                         # NEW: golden SBOMs for each user story
│           ├── unsigned_baseline.cdx.json     # (regenerated from live scan)
│           ├── signed_static_key.cdx.json     # (regenerated with test key)
│           └── ephemeral_keys/README.md       # How to regen the test keypair
docs/
├── cisa-2026-coverage.md                       # NEW: US1 deliverable (matrix + practices)
└── reference/
    └── sbom-format-mapping.md                  # +row for waybill:cisa-2026-lifecycle
                                                # (Principle V bullet 5 audit trail)

waybill-common/
├── src/
│   └── attestation/
│       └── envelope.rs                        # +constructor for signing arbitrary SBOM bytes
                                               # (extends beyond attestation-payloads)
```

**Structure Decision**: Reuse the milestone-006 `attestation/` module
for the cryptographic primitives (DSSE PAE, canonical JSON, sigstore
signer/verifier plumbing). Introduce a new sibling `sbom/signer.rs`
that adapts those primitives to sign SBOM document bytes (not
attestation payloads) and to emit into the CDX in-document
`signature` slot vs. an SPDX sidecar file. This keeps the crypto
tested-once and lets the SBOM signer stay a thin adapter.

Extend `generate/cyclonedx/builder.rs` and `generate/spdx/*` for
the FR-010–012 (generation-context alias) and FR-013–014
(`--sbom-version`) additions, sitting alongside existing
`metadata.lifecycles[]` emission. No cross-cutting refactor.

The `docs/cisa-2026-coverage.md` file is the US1 deliverable; its
schema is fixed by `contracts/coverage-matrix-schema.md` so the
US1 integration test can machine-verify every ✅ verdict against a
live scan.

## Complexity Tracking

> Populated only if Constitution Check has violations that must be justified.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| _none_ | — | — |

## Phase Progression

- [x] Phase 0: research.md generated
- [x] Phase 1: data-model.md, contracts/, quickstart.md generated + agent context updated
- [x] Constitution re-check post-design: still PASS (all commitments discharged in Phase 1 artifacts)

## Follow-ups (out-of-scope for this branch)

- **Constitution amendment**: Principle V references "CISA 2025 Minimum Elements" — bump to "CISA 2026 Minimum Elements" after this milestone lands. Recommended semver bump: 2.0.0 → 2.1.0 (MINOR — expanded normative content: the CISA 2026 target elements are a strict superset of 2025).
- **Static-key adapters beyond PEM**: `--sign-key` initially ships PEM-only per FR-007. Cloud-KMS URI (AWS/GCP/Azure KMS) and PKCS#11 (hardware tokens, HSMs) references are deferred to a follow-up milestone. The `KeyRef` enum in `waybill-cli/src/sbom/signer.rs` is structured to accept additional variants without a breaking-change bump.
