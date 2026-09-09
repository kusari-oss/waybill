# Feature Specification: Extensible signing identity + unattended keyless tests

**Feature Branch**: `779-signing-identity-shapes`
**Created**: 2026-09-08
**Status**: Draft
**Input**: User description: "Make the signing identity a first-class extensible shape so keyless signing tests can run unattended in CI"

## Context

waybill can sign SBOMs keylessly through Sigstore. Three related defects
keep that capability from being exercised without a human in the loop:

1. **Only one identity shape is accepted.** The signing substrate requires
   the identity token to carry an `email` claim. A token that identifies a
   *workload* rather than a *person* — which is what a CI runner's ambient
   OIDC token is — is rejected while the credential is still being parsed,
   before any Sigstore call happens. The operator sees
   `Malformed JWT: claims JSON malformed`, which describes neither the
   cause nor the fix.

2. **Therefore the keyless tests cannot run unattended.** All five
   identity-gated keyless tests are `#[ignore]`d and gated on a
   human exporting a short-lived token by hand. They have never been
   observed passing as a suite; the behaviour they assert was proven by
   manual reproduction instead.

3. **The documented CI workaround has never been demonstrated.** Operator
   docs point CI users at a helper action that exports a token. That advice
   was written from the type signature outward, not from an executed run.
   This is the same class of defect as the `cosign login --identity-token`
   recipe that shipped in four documents and four runtime diagnostics
   before anyone noticed the subcommand does not exist (#810 / #811).

Separately, the identity waybill records about a signature is a bare
string. An email address and a workflow reference are semantically
different things — they verify differently and they mean different things
about who is accountable — and today nothing in the recorded output
distinguishes them.

## Clarifications

### Session 2026-09-08

- Q: How often should the unattended keyless suite run — every pull
  request, or on a schedule? → A: On a recurring schedule (nightly),
  not per-PR. Rationale: the step from zero observed passing runs to a
  recurring observed run carries nearly all the value, and putting an
  external service with no availability guarantee in every PR's merge
  path introduces a flake source for a surface that changes rarely.
  Narrowing to per-PR runs on signing-path changes is a later
  optimisation, deliberately deferred until a healthy scheduled run
  exists to compare against.

- Q: Where is the signer identity recorded, given that the signed
  document and sidecar structure are out of scope? → A: In the log
  fields, plus waybill prints a complete, ready-to-run verification
  command on successful sign. No new artifact and no format change; the
  command is derived from the issued certificate so it cannot drift from
  what actually verifies.

- Q: What happens when the scheduled keyless run fails? → A: Auto-open a
  deduped issue carrying the outage-vs-defect classification, and close
  it when a later run succeeds — the same mechanism `ebpf-canary.yml`
  already uses (exact-title dedupe, comment on the existing issue rather
  than opening a second, close on recovery).

- Q: Which verifier should the emitted verification command target,
  given docs use cosign and the test harness uses sigstore-python? → A:
  Whichever matches the environment that was signed against — cosign for
  production, the staging-capable verifier for staging. A command that
  is not runnable in the case it was emitted for does not satisfy
  SC-004, and staging is the case automation exercises nightly.

- Q (from `/speckit.analyze`): SC-004 demanded demonstration in
  production, while FR-011 and Out of Scope forbid automated production
  signing — nothing planned could satisfy it. → A: keep both
  environments, but demonstrate production once by hand against an
  artifact a real release already signed, rather than by automation.
  FR-011 constrains unattended runs and is not engaged; no new
  production transparency-log entry is created.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Sign with a CI workload identity (Priority: P1)

A release pipeline signs the SBOM it just produced using the identity the
CI platform already grants the job. No secret is configured, no token is
pasted, no human is present.

**Why this priority**: Everything else in this feature is downstream of
this. Without it there is no unattended signing and therefore no
unattended test of unattended signing. It is also the capability operators
most reasonably expect from "keyless".

**Scope note (from Phase 0)**: this story is two pieces of work, not one.
The signing substrate must accept the claim, *and* waybill must implement
the ambient token exchange — which has never been written (research.md
R8). Patching the substrate alone would leave CI still dependent on a
helper action.

**Independent Test**: Run a signing job in an environment that issues a
workload identity token and no email claim; confirm a signed document and
its signature artifact are produced and verify against the recorded
identity.

**Acceptance Scenarios**:

1. **Given** a valid identity token carrying a workload subject and no
   email claim, **When** an SBOM is signed keylessly, **Then** signing
   succeeds and produces the same artifacts an email-identity sign
   produces.
2. **Given** a valid identity token carrying an email claim, **When** an
   SBOM is signed keylessly, **Then** behaviour is unchanged from before
   this feature.
3. **Given** a token carrying neither a usable email nor a usable workload
   subject, **When** signing is attempted, **Then** it fails before any
   network call with a diagnostic that names which claims were looked for
   and what the token actually carried.

---

### User Story 2 - Keyless conformance runs unattended (Priority: P2)

The keyless signing tests execute on their own schedule against Sigstore's
staging environment and report pass or fail without anyone fetching a
credential.

**Why this priority**: This is the requested outcome, and it is what turns
"we believe keyless works" into "we observe keyless working". It depends
on User Story 1 in practice but is separately testable and separately
valuable.

**Independent Test**: Trigger the automated run with no human-supplied
credential in the environment and confirm the keyless tests report a real
pass, not a skip.

**Acceptance Scenarios**:

1. **Given** an automated run in an environment that can obtain its own
   identity, **When** the keyless suite executes, **Then** every keyless
   test reports pass or fail — none report skipped.
2. **Given** an environment with no obtainable identity, **When** the
   keyless suite executes, **Then** the tests report as skipped and never
   as passed.
3. **Given** the external Sigstore staging service is unavailable,
   **When** the suite executes, **Then** the failure is reported in a form
   that distinguishes an outage from a waybill defect.
4. **Given** an automated run, **When** verification is performed,
   **Then** the expected identity and issuer are derived from the running
   environment rather than from a value hardcoded in the test.
5. **Given** a scheduled run fails, **When** the next scheduled runs also
   fail the same way, **Then** exactly one open record exists for it, and
   **When** a later run succeeds, **Then** that record is closed without
   anyone doing it by hand.

---

### User Story 3 - The recorded identity says what kind of identity it is (Priority: P2)

An auditor reading a signing record, or a consumer reading waybill's
output, can tell whether a signature was made by a person or by a
workload, and can construct the correct verification command from what was
recorded.

**Why this priority**: Explicitly requested. It is also the difference
between a verification recipe an operator can follow and one they have to
reverse-engineer from a certificate — a gap that has already cost a wasted
credential round-trip when the token's issuer and the certificate's issuer
turned out to differ under federation.

**Independent Test**: Sign under each available identity shape and confirm
the recorded output names the shape and carries both subject and issuer.

**Acceptance Scenarios**:

1. **Given** a completed keyless sign, **When** the signing record is
   read, **Then** it names the identity shape, the subject value, and the
   issuer.
2. **Given** an identity form the current vocabulary does not recognise,
   **When** signing completes, **Then** the raw subject and issuer are
   still recorded and signing does not fail.
3. **Given** a future release adds a new identity shape, **When** an
   existing consumer that does not know that shape reads the record,
   **Then** it continues to work without modification.

---

### User Story 4 - Published recipes are recipes that have been run (Priority: P3)

Every keyless instruction in operator-facing documentation has been
executed successfully at least once, and says which identity shape it
produces.

**Why this priority**: Corrective and low-risk, but the failure mode it
prevents has now occurred twice. Documentation that cannot work is worse
than absent documentation because it consumes the reader's trust before it
consumes their time.

**Independent Test**: For each published recipe, execute it verbatim and
record the outcome.

**Acceptance Scenarios**:

1. **Given** a keyless recipe in operator docs, **When** it is executed
   verbatim, **Then** it succeeds.
2. **Given** a diagnostic that currently tells operators a form of
   identity is unsupported, **When** that form becomes supported,
   **Then** the diagnostic no longer says otherwise.

### Edge Cases

- Token carries an `email` claim whose value is an empty string.
- Token carries both an email claim and a workload subject — the selection
  must be deterministic and the record must say which was used.
- The identity recorded in the issued certificate is a form the
  classification vocabulary does not recognise (for example a workload
  identity expressed as a URI from an unfamiliar platform).
- The token's stated issuer differs from the issuer recorded in the
  certificate, because the OIDC flow federated to an upstream provider.
  Verification requires the certificate's issuer; using the token's fails
  with an error that reads like a broken signature and is not one.
- The identity token expires between acquisition and signing.
- The transparency log is unreachable, rate-limited, or slow partway
  through an automated suite.
- An automated run is attempted in an environment that cannot obtain any
  identity at all.
- The identity form is unrecognised, so the emitted verification command
  is built from verbatim values rather than a known shape — it must still
  be emitted, and must still be correct.
- Signing targets a self-hosted or otherwise non-standard Sigstore
  deployment, so neither the production nor the staging verifier recipe
  applies as printed.

## Requirements *(mandatory)*

### Functional Requirements

**Accepting identities**

- **FR-001**: The system MUST accept an identity token that carries a
  workload subject and no email claim, and complete a keyless signature
  with it.
- **FR-002**: The system MUST continue to accept email-carrying identity
  tokens with no observable change to the artifacts produced.
- **FR-003**: When an identity token carries no claim the system can use
  as a signer identity, the system MUST fail before contacting any signing
  service, and the diagnostic MUST name the claims it looked for.
- **FR-004**: When an identity token carries more than one usable claim,
  the system MUST select one deterministically, keyed on the token's
  issuer per the Sigstore ecosystem mapping, and MUST record which claim
  was selected. An unrecognised issuer MUST fall back to the subject
  claim rather than failing.

**Recording identities**

- **FR-005**: Every successful keyless signature MUST record a signer
  identity consisting of at minimum a shape classification, a subject
  value, and the issuer that vouched for it.
- **FR-006**: The shape classification MUST be extensible: adding a shape
  in a later release MUST NOT invalidate previously recorded values and
  MUST NOT break consumers written against the earlier vocabulary.
- **FR-007**: An identity form the current vocabulary does not recognise
  MUST still be recorded verbatim and MUST NOT cause signing to fail.
- **FR-008**: The existing operator-visible `fulcio_cert_subject` log
  field MUST keep its name and its meaning; the shape classification is
  additive alongside it.
- **FR-009**: The recorded issuer MUST be the issuer that the certificate
  attests, not the issuer the token claims, because these differ under
  federation and only the former verifies.
- **FR-009a**: On a successful keyless sign the system MUST emit a
  complete, ready-to-run verification command for the artifact just
  signed. Every value in it MUST be derived from the issued certificate
  rather than from the token or from operator input, so that the command
  cannot disagree with what actually verifies. Where the identity form is
  one the vocabulary does not recognise (FR-007), the command MUST still
  be emitted using the verbatim recorded values.
- **FR-009b**: The emitted command MUST name a verifier that can actually
  verify against the Sigstore environment the artifact was signed against
  — the production-oriented verifier for production signs, a
  staging-capable verifier for staging signs. Emitting a command whose
  named tool cannot verify the environment just used does not satisfy
  FR-009a.
- **FR-009c**: When signing targets a Sigstore deployment that is neither
  the standard production nor the standard staging environment, the
  system MUST still emit an identity record, and MUST make clear that the
  verification command is a template requiring the operator to supply
  their deployment's trust configuration rather than a command that will
  run as printed.

**Running the tests unattended**

- **FR-010**: The keyless conformance tests MUST be executable with no
  human-supplied credential.
- **FR-011**: Unattended signing runs MUST target Sigstore's staging
  environment and MUST NOT write to the production transparency log.
- **FR-012**: The tests MUST derive the identity and issuer they verify
  against from the environment in which they run, not from a value written
  into the test source.
- **FR-013**: A keyless test that cannot obtain an identity MUST report as
  skipped and MUST NOT report as passed.
- **FR-014**: A failure caused by the external signing or transparency
  service being unavailable MUST be distinguishable in the run output from
  a failure caused by waybill.
- **FR-015**: The unattended keyless suite MUST run on a recurring
  schedule rather than on every pull request, and MUST additionally be
  triggerable on demand. Pull requests MUST NOT be gated on the
  availability of an external signing service.
- **FR-015a**: A failing scheduled run MUST raise a persistent, assignable
  record that states the FR-014 classification, so a reader can tell an
  outage from a defect without opening the run output. Consecutive
  failures of the same kind MUST NOT create additional records, and the
  record MUST be closed automatically once a later run succeeds.

**Documentation and diagnostics**

- **FR-016**: Every keyless recipe published in operator-facing
  documentation MUST have been executed successfully at least once, and
  MUST state which identity shape it produces.
- **FR-017**: Runtime diagnostics that currently state a form of identity
  is unsupported MUST be updated when that form becomes supported.

### Key Entities

- **Signer Identity**: What a signature attests about who made it. Carries
  a shape, a subject value, and the issuer that vouched for it. Replaces a
  bare subject string that could not distinguish a person from a workload.
- **Identity Shape**: The classification of a signer identity — at minimum
  a personal identity and a workload identity, with room for further
  shapes to be added later without breaking existing consumers.
- **Identity Token**: The short-lived credential presented to obtain a
  signing certificate. Carries claims from which the signer identity is
  derived; its stated issuer may differ from the certificate's.
- **Keyless Test Gate**: The preconditions an automated keyless test
  checks before running, and the reason it reports when it declines.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A keyless signature completes using only credentials an
  automated job can obtain by itself — zero human steps.
- **SC-002**: All five currently-ignored keyless tests are observed
  passing together in a single automated run. Today the observed count is
  zero.
- **SC-003**: An email-identity keyless sign produces the same document
  and signature-artifact structure as before this feature, differing only
  in the values that are inherently non-deterministic.
- **SC-004**: The verification command waybill emits on a successful sign
  succeeds when run verbatim, on the first attempt, with no manual
  certificate inspection and no edits by the operator. Demonstrated in
  both environments, by different means: **staging** on every scheduled
  run, and **production** once, human-initiated, against an artifact a
  real release already signed. The production demonstration does not
  conflict with FR-011 — that requirement constrains *unattended* runs,
  and no automation is added that writes to the production transparency
  log. Today the equivalent attempt failed and cost a credential
  round-trip.
- **SC-005**: A consumer written against today's identity shapes continues
  to work unmodified when a new shape is added.
- **SC-006**: 100% of keyless recipes in operator documentation have a
  recorded successful execution.
- **SC-007**: When the external signing service is unavailable, the run
  output identifies it as an external outage without a human reading logs
  to work that out.
- **SC-008**: No pull request can fail because an external signing or
  transparency service was unavailable.
- **SC-009**: A failure of the scheduled run reaches a human as a tracked,
  assignable record within one scheduled interval, and repeated identical
  failures produce exactly one such record rather than one per run.

## Assumptions

- The Sigstore Rust substrate waybill depends on is a fork under our own
  control, so the claim-acceptance change can be made there. A matching
  contribution upstream is desirable and would reduce long-term
  maintenance, but it is not a prerequisite for this feature and opening
  it requires separate explicit approval.
- Which claim identifies the signer is determined by the token's issuer,
  following the Sigstore ecosystem's established issuer-to-claim mapping,
  not by a fixed preference between claims. **Corrected during Phase 0
  research** — the spec originally assumed a personal claim was always
  preferred; see research.md R2. The issuer-keyed rule is what makes the
  recorded identity match the certificate by construction, which FR-009
  and SC-004 depend on.
- Automated signing targets staging only. Production keyless signing stays
  operator-initiated, because entries written to the production
  transparency log are permanent and public.
- The signing service derives the certificate's recorded identity from the
  token's claims rather than from what the client requests. **Phase 0
  research supports this** (research.md R1: the reference client sends a
  non-email value inside an email attribute and Fulcio still issues a
  correct certificate), but it remains inference from a reference
  implementation. A live probe MUST pass before User Story 1 is accepted.
- No new runtime dependencies are expected. The change is to which claims
  are accepted and how the resulting identity is modelled, not to how
  signing is performed.
- The wire format of signed documents and signature sidecars is unchanged.

## Out of Scope

- Interactive browser-based identity flows.
- Signing into the production transparency log from automation.
- Changing the structure of signed CycloneDX documents or the detached
  signature sidecar.
- Verification of signatures produced by other tools.
