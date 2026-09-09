# Implementation Plan: Extensible signing identity + unattended keyless tests

**Branch**: `779-signing-identity-shapes` | **Date**: 2026-09-08 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/779-signing-identity-shapes/spec.md`

## Summary

Make waybill able to sign keylessly with a CI workload identity, record
what kind of identity signed, hand the operator a verification command
that runs as printed, and let the keyless conformance suite run nightly
without a human fetching a credential.

Phase 0 changed the shape of this work in two ways. It **retired** the
largest sizing risk — Fulcio ignores the CSR subject, so the substrate
patch is small (R1). It **added** scope that was not visible from the spec
— waybill has never implemented the GitHub Actions ambient token exchange
at all, so patching the substrate alone would leave CI still dependent on
a helper action (R8). And it **corrected** one spec assumption: the claim
that identifies a signer is chosen by issuer, not by preferring a personal
claim (R2).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from
milestones 001–778; no nightly). Plus GitHub Actions YAML.
**Primary Dependencies**: Existing only on the shipped-binary side —
`sigstore` 0.11 (kusari-sandbox fork, requiring a new tag),
`x509-parser` 0.16 (SAN + issuer-extension reading, already a direct dep),
`reqwest` (ambient token exchange, already a direct dep),
`serde`/`serde_json`, `tracing`, `anyhow`/`thiserror`. **Zero new Cargo
dependencies.** The fork tag moves; the dependency set does not.
**Storage**: N/A — the identity is emitted to logs and stdout; no caches,
no persistence. The signed document and sidecar are unchanged.
**Testing**: `cargo +stable test --workspace`; the keyless suite in
`waybill-cli/tests/cisa_2026_signing.rs` stays identity-gated and
`#[ignore]`d locally, and runs unattended on the scheduled lane.
External verifiers: cosign (production) and the sigstore CLI (staging),
both shell-outs in the same posture as `spdx3-validate` (m078) and
`trivy`/`syft` (m083).
**Target Platform**: Linux CI runners and developer macOS/Linux hosts.
**Project Type**: CLI (single Rust workspace, three crates).
**Performance Goals**: N/A — signing is one network round-trip per run and
is not on any hot path.
**Constraints**: Automated signing MUST target Sigstore staging only
(FR-011); production transparency-log entries are permanent and public.
Pull requests MUST NOT depend on an external signing service (FR-015).
**Scale/Scope**: One new enum plus two supporting types; three edits in
the fork; one new token-acquisition path; one scheduled workflow; five
existing tests moved from never-observed to routinely-observed.

## Constitution Check

*GATE: evaluated before Phase 0 and re-evaluated after Phase 1 design.
Both passes reached the same verdicts.*

| Principle | Verdict | Notes |
|---|---|---|
| I. Pure Rust, Zero C | PASS | No C anywhere. The sigstore CLI (Python) is invoked only as an external *verifier* in tests and named in printed guidance — never linked, never a build input. Same posture the constitution already tolerates for `spdx3-validate`. |
| II. eBPF-Only Observation | N/A | No dependency discovery involved. |
| III. Fail Closed | **PASS, reinforced** | FR-003 fails before any network call when no usable claim exists, and the diagnostic names what it looked for. `SigningIdentity` construction rejects an empty subject or issuer rather than recording a blank. |
| IV. Type-Driven Correctness | **PASS — this feature *is* a Principle IV correction** | `fulcio_cert_subject: String` is exactly the "raw `String` passed across function boundaries for a domain value" the principle forbids. Replacing it with an enum that carries its issuer is the principle applied, not merely respected. No `.unwrap()` in production; test modules take the `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard per house convention. |
| V. Specification Compliance | PASS with required doc work | Touches CISA 2026 row 2 (SBOM Author Signature). `docs/cisa-2026-coverage.md` currently states GHA ambient tokens cannot work; FR-017 requires that be corrected once they can. Tracked as a task, not left to drift. |
| VI. Three-Crate Architecture | PASS | New types live in `waybill-cli`. No new crate, no movement between crates. |
| VII. Test Isolation | PASS | Keyless tests stay gated and continue to report `ignored` — never `ok` — when the gate is unmet (FR-013). Unprivileged `cargo test --workspace` is unaffected. |
| VIII. Completeness / IX. Accuracy | N/A | No component discovery or resolution changes. |
| X. Transparency | **PASS, advances it** | The feature exists to stop waybill knowing something (which issuer verifies) that it does not tell the operator. No SBOM-embedded metadata is added, so the "prefer spec-native mechanisms" clause is not engaged. |
| XI / XII. Enrichment | N/A | No external enrichment. |

**Strict Boundaries**: none engaged. No lockfile discovery, no MITM
(the ambient exchange is an ordinary authenticated HTTPS call to the
runner's own OIDC endpoint, not interception), no C, no production
`.unwrap()`, no file-tier change.

**No violations. Complexity Tracking section omitted as empty.**

## Project Structure

### Documentation (this feature)

```text
specs/779-signing-identity-shapes/
├── plan.md              # This file
├── spec.md
├── research.md          # Phase 0 — R1..R8
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/
│   └── identity-resolution.md
├── checklists/
│   └── requirements.md
└── tasks.md             # /speckit.tasks — not created here
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   └── attestation/
│       └── signer.rs                 # SigningIdentity, VerificationCommand,
│                                     # SigstoreEnvironment; ambient token
│                                     # exchange; extended cert extraction;
│                                     # extended FR-016 log record
└── tests/
    └── cisa_2026_signing.rs          # keyless suite: env-derived expected
                                      # identity, no hardcoded address

docs/
├── cisa-2026-coverage.md             # row 2: ambient tokens now work
└── verifying-releases.md             # per-shape verification guidance

.github/workflows/
├── keyless-conformance.yml           # NEW — scheduled + workflow_dispatch,
│                                     # deduped issue on failure, close on
│                                     # recovery (mirrors ebpf-canary.yml)
└── ebpf-canary.yml                   # reference implementation, unchanged
```

Plus, in `kusari-sandbox/sigstore-rs` (separate repository, new tag):

```text
src/oauth/token.rs      # optional email, add sub + federated_claims,
                        # issuer-keyed identity resolution
src/bundle/sign.rs      # materials(): use resolved identity for the
                        # (Fulcio-ignored) CSR subject attribute
```

**Structure Decision**: single-crate change inside the existing workspace.
Everything waybill-side lands in `waybill-cli/src/attestation/signer.rs`,
which already owns `KeylessSignSuccess` and the certificate extraction.
The type is deliberately **not** exported from `waybill-cli/src/lib.rs`:
see research.md R5 — exporting it solely to give `#[non_exhaustive]`
enforcement teeth would be scope creep for no user-visible gain.

## Delivery order

The stories are independently testable but have one real dependency:
US2 cannot demonstrate anything until US1 works.

1. **Fork patch + new tag** — three edits, then bump the pin. Unblocks
   everything else. Verified by a live staging probe with an
   email-carrying token (proving no regression) before anything else.
2. **US1** — ambient token exchange, plus the live probe with a token
   carrying no `email` that closes research.md R1. This is the point at
   which the feature's central claim is either true or is not.
3. **US3** — `SigningIdentity`, the extended log record, the emitted
   verification command. Pure local work; no credential needed for the
   unit tests.
4. **US2** — the scheduled workflow, env-derived expected identity in the
   tests, deduped failure issue.
5. **US4** — docs and diagnostics, last, so every published recipe is
   written against behaviour that already exists and has been run.

## Risks

| Risk | Handling |
|---|---|
| R1's inference is wrong and Fulcio does read the CSR subject | Live probe is step 2, before any dependent work. If it fails, the fork patch grows a per-shape subject encoding and US1 re-estimates. Nothing built on top of it yet. |
| Sigstore staging is unavailable during development | It is already the environment the m809 work used successfully. Failures are classified, not silently retried. |
| The fork drifts further from upstream | The patch is three small edits shaped to be upstreamable. Opening the upstream PR needs separate explicit approval and is not on the critical path. |
| The nightly is ignored once green | FR-015a's deduped, self-closing issue is the mitigation, reusing a mechanism already proven in this repo. |
