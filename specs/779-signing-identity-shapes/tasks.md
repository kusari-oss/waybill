---
description: "Task list for 779-signing-identity-shapes"
---

# Tasks: Extensible signing identity + unattended keyless tests

**Input**: Design documents from `/specs/779-signing-identity-shapes/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/identity-resolution.md, quickstart.md

**Tests**: INCLUDED. This feature is substantially about tests — User
Story 2 is the test suite itself, and SC-002 is an observation about test
runs. Test tasks are not optional here.

**Organization**: Grouped by user story. Phase order follows plan.md's
delivery order, which puts US3 before US2 even though both are P2: US3 is
pure local work needing no credential, and US2's assertions consume what
US3 produces.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no incomplete dependencies)
- **[Story]**: US1 / US2 / US3 / US4
- Exact file paths included

---

## Phase 1: Setup

**Purpose**: Establish the baselines that later "it still works" and "it
now works" claims will be measured against.

- [X] T001 Run `./scripts/pre-pr.sh` on the untouched branch and record the exact suite/test counts in `specs/779-signing-identity-shapes/baseline.md`. This session has produced two false-green gate runs that were caught only because the numbers did not match a prior run; a recorded reference makes that check mechanical rather than remembered.
- [ ] T002 [P] Capture the current keyless behaviour as evidence in `specs/779-signing-identity-shapes/baseline.md`: sign a small tree against staging with an email-carrying `SIGSTORE_ID_TOKEN`, and record the emitted document path, sidecar path, `fulcio_cert_subject`, the certificate's SAN, and the certificate's OIDC-issuer extension. **Requires a human-fetched staging token** (`sigstore --staging get-identity-token`) — this is the last task in the feature that does.

---

## Phase 2: Foundational — substrate patch (BLOCKING)

**Purpose**: Make the signing substrate accept a workload identity. Every
user story is downstream of this.

**⚠️ CRITICAL**: No user story work begins until T011 passes.

- [X] T003 Clone `kusari-sandbox/sigstore-rs` at tag `v0.11.0-waybill-1` into a working directory outside this repo, and confirm the working tree matches the vendored checkout at `~/.cargo/git/checkouts/sigstore-rs-fe366a804a4aa93b/f3ac508` before editing.
- [X] T004 In the fork's `src/oauth/token.rs`, change `Claims.email` from `String` to `Option<String>` and add `sub: Option<String>`, `iss: String`, and an optional `federated_claims` struct carrying `connector_id`. Keep the existing `aud` / `exp` / `nbf` fields and their serde attributes byte-identical.
- [X] T005 In the fork's `src/oauth/token.rs`, add issuer-keyed identity resolution per `contracts/identity-resolution.md` §1 — the four-entry issuer map plus a `sub` fallback — exposed as a public accessor on `IdentityToken`. On absence, return an error naming the claim looked for, the issuer that selected it, and the claims actually present.
- [X] T006 In the fork's `src/bundle/sign.rs`, change `materials()` (lines 84-101) to build the CSR subject from T005's resolved identity instead of `token.unverified_claims().email.as_ref()`. Keep the `EMAIL_ADDRESS` OID regardless of shape, matching sigstore-python byte-for-byte — per research.md R1 the attribute is ignored by Fulcio, and diverging from the reference client buys nothing.
- [X] T007 [P] Add unit tests in the fork's `src/oauth/token.rs` `#[cfg(test)] mod tests` covering: a Google-issued token resolving via `email`; a GitHub-Actions-issued token resolving via `sub`; an unknown issuer falling back to `sub`; and a token with neither claim producing the named-claims error. These must fail against `v0.11.0-waybill-1` and pass after T004-T006.
- [X] T008 Tag the fork `v0.11.0-waybill-2` and push it. **Outward-facing — confirm before pushing.** Record the resulting commit SHA in `specs/779-signing-identity-shapes/baseline.md`.
- [X] T009 Bump the `sigstore` git tag in `waybill-cli/Cargo.toml:161` to `v0.11.0-waybill-2`, refresh `Cargo.lock`, and confirm `cargo +stable build --workspace --all-targets` succeeds. Use `--all-targets`: a plain `cargo build` skips test-only call sites and has silently hidden breakage in this workspace before.
- [ ] T010 **Live probe — no regression.** Sign against staging with an email-carrying token and confirm the document, sidecar, cert SAN and cert issuer match the T002 baseline exactly. A difference here means T004-T006 changed behaviour for the path that already worked.
- [ ] T011 **Live probe — the load-bearing claim (GATE).** Sign against staging with a token carrying no `email` claim and confirm Fulcio issues a certificate whose SAN holds the `sub` value. This closes research.md R1, which is currently inference from sigstore-python rather than observation. **If this fails, stop and re-plan** — the fork patch needs per-shape subject encoding and User Story 1 must be re-estimated. Record the outcome either way.

**Checkpoint**: the substrate accepts workload identities, proven by observation.

---

## Phase 3: User Story 1 — Sign with a CI workload identity (P1) 🎯 MVP

**Goal**: waybill obtains the CI platform's ambient credential itself and
signs with it. No secret, no pasted token, no helper action.

**Independent Test**: run a signing job in an environment issuing a
workload token and no email claim; confirm a signed document and sidecar
are produced and verify against the recorded identity.

**Scope note**: two pieces of work, not one — the substrate patch (Phase
2) *and* the ambient exchange below, which has never been written
(research.md R8).

### Tests for User Story 1

- [X] T012 [P] [US1] Unit test in `waybill-cli/src/attestation/signer.rs` `#[cfg(test)] mod tests`: a token with neither a usable email nor a usable subject fails before any network call, and the error names the claim looked for, the issuer, and the claims present. Guard the module with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per house convention.
- [X] T013 [P] [US1] Unit test in `waybill-cli/src/attestation/signer.rs` `#[cfg(test)] mod tests`: `OidcProvider::detect()` returns `GitHubActions` when both `ACTIONS_ID_TOKEN_REQUEST_URL` and `ACTIONS_ID_TOKEN_REQUEST_TOKEN` are set. Route env mutation through `crate::testing::EnvGuard::acquire()` — unguarded env writes have caused two resolved flakes in this workspace.
- [X] T014 [US1] Identity-gated integration test `m779_us1_workload_identity_signs` in `waybill-cli/tests/cisa_2026_signing.rs`: with a workload token, a keyless CycloneDX sign produces both the document and the `.sig.bundle.json` sidecar. `#[ignore]`d with the existing m809 gate wording so an unmet gate reports `ignored`, never `ok`.

### Implementation for User Story 1

- [X] T015 [US1] Implement the ambient token exchange in `waybill-cli/src/attestation/signer.rs`: GET `ACTIONS_ID_TOKEN_REQUEST_URL` with `audience=sigstore`, bearing `ACTIONS_ID_TOKEN_REQUEST_TOKEN`, parse the `value` field, and construct an `IdentityToken` from it. Reuse the existing `reqwest` client posture; no new dependency.
- [X] T016 [US1] Replace the hard-error `OidcProvider::GitHubActions` arm at `waybill-cli/src/attestation/signer.rs:316-324` with a call to T015. Keep the `Interactive` arm's fail-close diagnostic unchanged.
- [X] T017 [US1] Update the `identity_token_from_env_var` diagnostic at `waybill-cli/src/attestation/signer.rs:274-280`, which currently tells operators GHA ambient OIDC cannot work. It can now. Leaving it is the same defect class as the `cosign login --identity-token` recipe (#810/#811): advice that contradicts shipped behaviour.
- [X] T018 [US1] In `waybill-cli/src/attestation/signer.rs`, verify `in_validity_period()` is checked on the ambient token before use, matching the explicit-env path, so an expired ambient credential fails closed rather than at Fulcio.

**Checkpoint**: waybill signs in CI with no credential configuration.

---

## Phase 4: User Story 3 — The recorded identity says what kind it is (P2)

**Goal**: the signing record names the identity shape, subject and issuer,
and waybill emits a verification command that runs as printed.

**Independent Test**: sign under each shape and confirm the record names
the shape and carries both halves; run the emitted command verbatim.

**Ordered before US2** (both P2): this is pure local work needing no
credential, and US2's assertions consume what it produces.

### Tests for User Story 3

- [X] T019 [P] [US3] Unit tests in `waybill-cli/src/attestation/signer.rs` for `SigningIdentity` classification from a certificate: an RFC822 SAN yields `Email`, a URI SAN yields `Workload`, an unfamiliar SAN form yields `Unrecognized` without failing, and an empty subject or issuer is rejected at construction.
- [X] T020 [P] [US3] Unit tests in `waybill-cli/src/attestation/signer.rs` for `SigstoreEnvironment` classification: matching production endpoints, matching staging endpoints, and a mixed production/staging pair classifying as `Custom` rather than guessing.
- [X] T021 [P] [US3] Unit tests in `waybill-cli/src/attestation/signer.rs` for verification-command rendering: production renders the cosign form, staging renders the sigstore-CLI form, `Custom` sets `is_template` and carries the trust-configuration note, and subject/issuer are single-quoted so a workload subject's `:` and `/` survive a paste.
- [X] T022 [US3] Extend the m222-FR-016 log-fields assertion in `waybill-cli/tests/cisa_2026_signing.rs:1104` to cover the two new fields. Use the existing `strip_ansi()` helper — `tracing` writes ANSI escapes between a field name and its `=`, which is why the original m222 assertion was wrong for a year while `#[ignore]`d.

### Implementation for User Story 3

- [X] T023 [P] [US3] Add the `SigningIdentity` enum to `waybill-cli/src/attestation/signer.rs` per data-model.md, with `#[non_exhaustive]`, the three variants, and the `subject_value()` / `issuer()` / `shape_name()` accessors. Note in the doc-comment that `#[non_exhaustive]` is intent-signalling while the module stays private to the binary (research.md R5) — do not imply it is doing enforcement work it is not.
- [X] T024 [P] [US3] Add `VerificationCommand` and `SigstoreEnvironment` to the same file per data-model.md.
- [X] T025 [US3] Extend `extract_fulcio_cert_subject` at `waybill-cli/src/attestation/signer.rs:364` into a classifier returning `SigningIdentity`, reading the certificate's OIDC-issuer extension alongside the SAN. Preserve first-match SAN ordering so today's behaviour is unchanged for email certificates.
- [X] T026 [US3] Add `identity` and `verification_command` to `KeylessSignSuccess` at `waybill-cli/src/attestation/signer.rs:255`. Keep `fulcio_cert_subject` — FR-008 requires the name and meaning survive, and the m809 tests grep it.
- [X] T027 [US3] Implement command rendering per `contracts/identity-resolution.md` §4, deriving every substituted value from the certificate — never from the token, which disagrees with the certificate under federation.
- [X] T028 [US3] Extend the m222-FR-016 INFO record at `waybill-cli/src/attestation/signer.rs:571` with `fulcio_cert_identity_shape` and `fulcio_cert_oidc_issuer`, and print the verification command on success. Additive only: the three existing field names keep their spelling.

**Checkpoint**: an operator can verify without inspecting a certificate.

---

## Phase 5: User Story 2 — Keyless conformance runs unattended (P2)

**Goal**: the keyless suite runs on a schedule, with no human fetching a
credential, and a failure reaches someone.

**Independent Test**: trigger the run with no human-supplied credential and
confirm the keyless tests report a real pass, not a skip.

### Tests for User Story 2

- [X] T029 [US2] Replace the hardcoded verification identity in `waybill-cli/tests/cisa_2026_signing.rs:283-284` so `WAYBILL_TEST_CERT_IDENTITY` / `WAYBILL_TEST_CERT_OIDC_ISSUER` are derived from the running environment when unset, rather than requiring a personal address. Satisfies FR-012.
- [X] T030 [US2] Extend `keyless_gate()` at `waybill-cli/tests/cisa_2026_signing.rs:187` to accept an ambient credential as satisfying the gate, not only `SIGSTORE_ID_TOKEN`. Keep the skip-reports-`ignored` behaviour exactly — a skipped keyless test must never read as coverage.
- [X] T031 [US2] Add classification to the harness so a Sigstore staging outage is distinguishable in the run output from a waybill defect (FR-014). Extend the existing diagnostic block at `waybill-cli/tests/cisa_2026_signing.rs:298-311`, which already prints the verifier's own stderr for exactly this reason.

### Implementation for User Story 2

- [X] T032 [US2] Add `.github/workflows/keyless-conformance.yml` with a `schedule` trigger and `workflow_dispatch`, `permissions: id-token: write` plus `issues: write`, running the keyless suite with `-- --ignored` against staging endpoints. Give `workflow_dispatch` an optional Fulcio-endpoint input, defaulting to staging — it is useful for debugging on its own, and it is the lever T034a needs to rehearse a failure without editing the workflow. No `pull_request` trigger — FR-015 and SC-008 forbid PRs depending on an external signing service.
- [X] T033 [US2] Add the failure-reporting step to that workflow, copying the mechanism from `.github/workflows/ebpf-canary.yml:134-197`: exact-title dedupe, comment on the existing issue rather than opening a second, and state the FR-014 classification in the body.
- [X] T034 [US2] Add the recovery step, copying `.github/workflows/ebpf-canary.yml:211+`, so the issue closes itself when a later run succeeds.
- [ ] T034a [US2] **Rehearse the failure path before trusting it.** Using T032's endpoint input, dispatch the workflow against an unreachable Fulcio and confirm the full lifecycle: (a) exactly one issue opens, and its body classifies the cause as an external outage rather than a waybill defect; (b) a second failing dispatch comments on that issue instead of opening a second one; (c) a normal dispatch closes it. Record all three observations in `specs/779-signing-identity-shapes/baseline.md`. Building a notification mechanism and never firing it would reproduce, in the reporting layer, exactly the defect this feature exists to end — five tests that everyone believed were passing had never run. Covers FR-015a and SC-009.
- [ ] T035 [US2] Trigger the workflow manually via `workflow_dispatch` and record in `specs/779-signing-identity-shapes/baseline.md` that all five previously-ignored tests ran and passed in one run. **This is SC-002.** It is an observation, not an assertion — do not mark it done from a green summary line without the per-test output.

**Checkpoint**: keyless behaviour is observed on a schedule rather than believed, and the path that reports a failure has itself been observed working.

---

## Phase 6: User Story 4 — Published recipes are recipes that have been run (P3)

**Goal**: every keyless instruction in operator docs has been executed.

**Independent Test**: execute each published recipe verbatim; record the outcome.

- [X] T036 [P] [US4] Update row 2 of `docs/cisa-2026-coverage.md`, which currently states GHA ambient tokens do not emit `email` and are therefore unsupported, and points at a helper action as the CI workaround. Both statements become false with US1. State which identity shape each path produces.
- [X] T037 [P] [US4] Update the verification recipes at `docs/cisa-2026-coverage.md:399` and `:447` to show both shapes, and keep the existing federated-issuer note — it is correct and hard-won.
- [X] T038 [P] [US4] Update `docs/verifying-releases.md` with per-shape verification guidance and the staging-vs-production verifier split from research.md R4.
- [X] T038a [P] [US4] Update `README.md:225-237`. Two claims there become false with US1: that GHA ambient tokens "are not directly supported in v1" with users directed to a helper action, and that `cosign login` is an example of an email-emitting OIDC provider. **`cosign login` logs in to a container registry and emits no OIDC token** — verified by running `cosign login --help`. It is a surviving instance of the #810/#811 defect, which corrected four documents, four runtime diagnostics and four test assertions but did not reach README. While in this block, correct the stale m778 claim that keyless `--sign` embeds the bundle in CDX `metadata.signature`; it has been a detached sidecar since m778.
- [ ] T039 [US4] Execute every **staging** keyless recipe in the updated docs verbatim and record each outcome in `specs/779-signing-identity-shapes/baseline.md`. **This is SC-006 and it is the point of the story** — a recipe that has not been run is what this story exists to eliminate.
- [ ] T039a [US4] Demonstrate the **production** verification command once, satisfying SC-004's production half. Ride the next real release rather than minting a throwaway production entry: release signing already writes to the production transparency log, so take the artifact and sidecar that release produces, run the emitted cosign command verbatim, and record the outcome in `specs/779-signing-identity-shapes/baseline.md`. This is human-initiated, so FR-011 — which constrains *unattended* runs — is not engaged. Production transparency-log entries are permanent and public; do not create one solely for this task.

**Checkpoint**: no unexecuted advice remains in operator docs.

---

## Phase 7: Polish & Cross-Cutting

- [X] T040 Add a `CHANGELOG.md` entry describing the workload-identity capability, the identity classification, and the corrected GHA guidance.
- [X] T041 Run `./scripts/pre-pr.sh` and compare the suite/test counts against the T001 baseline. Both `cargo +stable clippy --workspace --all-targets` and `cargo +stable test --workspace` must pass. Enumerate every `^---- .+ stdout ----` line before claiming green; cargo stops after the first failing binary without `--no-fail-fast`.
- [ ] T042 Walk `specs/779-signing-identity-shapes/quickstart.md` end to end and correct anything that does not behave as written.
- [ ] T043 Consider opening the upstream sigstore-rs contribution for T004-T006. **Requires separate explicit approval per project policy** — a plan approval does not authorize a PR to a third-party repository. Not on this feature's critical path.

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (Setup)**: no dependencies.
- **Phase 2 (Foundational)**: needs T002's baseline to compare against. **Blocks every user story.** T011 is a hard gate.
- **Phase 3 (US1)**: needs Phase 2.
- **Phase 4 (US3)**: needs Phase 2. Independent of US1 for its unit tests; needs US1 to exercise the workload shape end to end.
- **Phase 5 (US2)**: needs US1 (nothing to run unattended without it) and US3 (its assertions consume the identity record).
- **Phase 6 (US4)**: needs US1, since the docs describe US1's behaviour.
- **Phase 7**: needs everything intended for the PR.

### Story dependency graph

```text
Phase 2 (fork + T011 gate)
   ├─→ US1 (P1) ──┬─→ US2 (P2)
   │              └─→ US4 (P3)
   └─→ US3 (P2) ──┘
```

US3's unit tests (T019-T021, T023-T024) can be built in parallel with US1
— they need no credential and no network.

### Parallel opportunities

- T002 alongside T001.
- T007 alongside T004-T006 (test file is separate from the sources).
- T012, T013 in parallel.
- T019, T020, T021 in parallel; T023, T024 in parallel.
- T036, T037, T038 in parallel.

### Within each story

- Tests before implementation.
- Types (T023, T024) before the code consuming them (T025-T028).
- T025 before T026 — `KeylessSignSuccess` cannot carry a type that does not exist.

---

## Implementation Strategy

**MVP = Phase 1 + Phase 2 + Phase 3 (US1).** At that point waybill signs
in CI with no credential configuration, which is the capability the
feature exists to deliver. Everything after makes it legible (US3),
observed (US2), and documented (US4).

**Stop-and-re-plan point**: T011. Every task after it assumes Fulcio
ignores the CSR subject. That belief currently rests on the behaviour of a
reference client, not on an observation of the service. Do not build past
T011 on the assumption it will pass.

**Two habits worth carrying, both earned this session:**

- Compare gate output against T001's recorded counts rather than reading
  the summary line. Two runs this session reported green while having
  silently run a fraction of the suite.
- Treat SC-002 and SC-006 as observations to be recorded, not boxes to be
  ticked. Five keyless tests spent a year `#[ignore]`d while everyone
  believed they were passing, and one of them contained an assertion that
  could never have matched.
