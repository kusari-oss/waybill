# Feature Specification: SLSA build provenance for waybill release artifacts

**Feature Branch**: `668-slsa-provenance`
**Created**: 2026-08-28
**Status**: Draft
**Input**: User description: "add SLSA support, using the most up to date mechanism in the Github built in provenance attestation generator"

## Clarifications

### Session 2026-08-28

- Q: Is this feature strictly release-pipeline-only, or does it also add SLSA-Provenance emission to the waybill CLI's runtime output? → A: Release-artifact only (Option A). CLI runtime emission (waybill trace, waybill scan → SLSA-Provenance sibling predicates) is tracked as a separate future feature at [issue #725](https://github.com/kusari-oss/waybill/issues/725).
- Q: Does waybill's public messaging (release notes, README, docs) formally claim a SLSA Build level? → A: No level claim (Option B). Ship provenance; let downstream consumers rank per their own audit framework. Avoids ongoing conformance-certification burden.
- Q: Does the release workflow verify its own emitted attestations as a final self-test step before completion? → A: Yes, self-verify (Option A). Release workflow runs `gh attestation verify` against each emitted attestation post-emission; any failure fails the release. Catches misconfigured subject digests / workflow-identity mismatches before consumers discover them.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Downstream consumer verifies a release binary came from waybill's official build pipeline (Priority: P1) 🎯 MVP

A security-conscious downstream consumer — a distro packager, a compliance
auditor, or an operator ingesting waybill into an air-gapped environment —
downloads a waybill binary tarball from a GitHub release and wants
cryptographic evidence that (a) the binary was produced by waybill's
official GitHub Actions build pipeline, (b) the pipeline ran against a
specific commit SHA on the `main` branch of `kusari-oss/waybill`, and
(c) no post-build tampering has replaced the binary bytes.

The consumer runs a single verification command against the downloaded
tarball and gets a machine-readable YES/NO answer plus the source
commit + workflow-run identity that produced it. If the answer is YES,
they can proceed to install; if NO, they surface the mismatch to their
security team.

**Why this priority**: SLSA build provenance is the industry-standard
answer to the "did this artifact come from where the vendor says it
did?" question. Downstream consumers are already asking this of every
signed release artifact in 2026 (via `gh attestation verify`, `cosign
verify-attestation`, Kyverno / Ratify admission policies, Sigstore
policy-controller). Without SLSA provenance, waybill releases fail the
first row of any downstream consumer's SLSA-Build-L2/L3 checklist —
even though the cosign-signed binary (already shipped as of m222) is
strong signing evidence, it doesn't carry the build-pipeline identity
in a machine-verifiable predicate. This is the MVP: everything else in
this feature is either scope expansion (US2, US3) or ergonomic
improvement (US4).

**Independent Test**: after one release cycle post-merge, download a
released tarball. Run `gh attestation verify <tarball>
--repo kusari-oss/waybill`. Verification succeeds, and the output
names the source workflow file, the source commit SHA, and the
workflow-run URL. Verification against a tampered tarball (any single
byte flipped) fails with a hash-mismatch error.

**Acceptance Scenarios**:

1. **Given** a waybill release with SLSA provenance emitted, **When** a
   downstream consumer runs `gh attestation verify <tarball>
   --repo kusari-oss/waybill`, **Then** verification succeeds and the
   output identifies (a) the source workflow file path in
   `.github/workflows/`, (b) the source commit SHA the workflow ran
   against, and (c) the GitHub Actions run URL.
2. **Given** a waybill release with SLSA provenance emitted, **When** a
   downstream consumer flips one byte in the downloaded tarball and
   re-runs verification, **Then** verification fails with a diagnostic
   citing hash mismatch between the tarball's SHA-256 and the digest
   recorded in the provenance predicate's `subject[]` array.
3. **Given** a waybill release with SLSA provenance emitted, **When** a
   downstream consumer inspects the emitted attestation as JSON,
   **Then** the `predicateType` field equals the current version of
   the SLSA Provenance URI (`https://slsa.dev/provenance/v1` or the
   most recent version the GitHub attest-build-provenance action
   emits at the time of release).

---

### User Story 2 - Provenance covers every release artifact, not just the Linux tarball (Priority: P2)

Waybill ships four platform-specific binary tarballs (linux-x86_64,
linux-aarch64, macos-aarch64, windows-x86_64), one multi-arch OCI
container image at `ghcr.io/kusari-oss/waybill`, and one source-code
SBOM sidecar per release. A downstream consumer who only trusts the
OCI image (not the tarball) or who ingests the source SBOM into a
regulatory workflow needs the same "did this come from where the
vendor says it did?" answer for their artifact of choice — not just
the tarball.

**Why this priority**: SLSA Build L2 requires provenance for every
artifact the build pipeline produces, not a subset. Skipping the OCI
image or the source SBOM leaves a gap that admission-control policies
(Kyverno, Ratify, Sigstore policy-controller) will detect as
"missing attestation" and block deployment. Bundling all release
artifact types under one provenance emission wave is cheaper than
retrofitting later — the GitHub action is the same one call per
subject.

**Independent Test**: after one release cycle post-merge, run
`gh attestation verify` against (a) each of the 4 tarballs by
platform, (b) the OCI image via
`gh attestation verify oci://ghcr.io/kusari-oss/waybill:<version>`,
and (c) the source SBOM sidecar. All 6 verifications succeed.

**Acceptance Scenarios**:

1. **Given** a waybill release with SLSA provenance emitted, **When** a
   downstream consumer downloads each of the 4 platform tarballs and
   runs `gh attestation verify` against each, **Then** all 4
   verifications succeed with matching source commit SHA.
2. **Given** a waybill release with SLSA provenance emitted, **When** a
   downstream consumer pulls the multi-arch OCI image and runs
   `gh attestation verify oci://ghcr.io/kusari-oss/waybill:<version>
   --repo kusari-oss/waybill`, **Then** verification succeeds and the
   subject digest matches the pulled image's content-addressed digest.
3. **Given** a waybill release with SLSA provenance emitted, **When** a
   downstream consumer downloads the source SBOM sidecar and runs
   `gh attestation verify` against it, **Then** verification succeeds
   with the same source commit SHA as the tarball/image provenance.

---

### User Story 3 - Nightly release builds also carry provenance (Priority: P3)

Nightly builds run daily via cron and produce the same 4 tarballs + 1
OCI image the stable release produces (per milestone 229). A
downstream consumer running a rolling-update pipeline against
`ghcr.io/kusari-oss/waybill:nightly` should get the same provenance
guarantee as a consumer running against a stable tag.

**Why this priority**: nightlies are convenience artifacts for
integration testing. Downstream verifiers who pin `:nightly` in a
CI job aren't running production, but the "same shape" principle
(nightly = stable minus versioning) means adding provenance to
nightly is cheap and prevents the "why does verification work on
stable but fail on nightly?" support burden. Not blocking for MVP —
a downstream consumer running `:nightly` in production is doing
something waybill doesn't warrant regardless of provenance.

**Independent Test**: after one nightly cycle post-merge, run
`gh attestation verify` against the nightly tarball + OCI image.
Verification succeeds with the correct source commit SHA + workflow
identifier for the nightly build.

**Acceptance Scenarios**:

1. **Given** a waybill nightly build with SLSA provenance emitted,
   **When** a downstream consumer pulls
   `ghcr.io/kusari-oss/waybill:nightly` and runs
   `gh attestation verify`, **Then** verification succeeds and the
   emitted predicate names the nightly workflow (not the stable
   release workflow) as the build source.

---

### User Story 4 - Waybill's release documentation tells operators how to verify (Priority: P3)

A first-time downstream consumer visits waybill's release-verification
documentation and finds a copy-paste one-liner for verifying any
release artifact type they might download. They can complete
verification in under 5 minutes without piecing together SLSA
tutorials from external sources.

**Why this priority**: SLSA provenance is only useful if downstream
consumers know how to verify it. First-time consumers today can't
copy-paste a verification recipe from waybill docs because no such
docs exist. This is docs-only work; not blocking for MVP but
essential for the value chain to close.

**Independent Test**: a first-time consumer follows the release-
verification recipe in the docs, verifies one release artifact of
each type, and gets a green result for all types within 5 minutes
without external tutorials.

**Acceptance Scenarios**:

1. **Given** the release-verification documentation is published,
   **When** a first-time consumer copy-pastes the recipe for the
   tarball verification, **Then** they reach a green verification
   result within the same session (single terminal, no context
   switching, no external tutorials).
2. **Given** the release-verification documentation is published,
   **When** a first-time consumer copy-pastes the recipe for the OCI
   image verification, **Then** they reach a green verification
   result within the same session.

---

### Edge Cases

- **Release re-run**: what happens if a release workflow is manually
  re-run for the same tag (network flake, transient GHCR outage)?
  The provenance MUST regenerate — each workflow run gets its own
  attestation. Downstream consumers verifying the tarball see the
  most-recent provenance's workflow-run URL. This matches the
  actions/attest-build-provenance action's default behavior; no
  extra work.
- **Partial-release failure**: what happens if provenance emission
  fails partway through a release (e.g., 3 tarballs get provenance,
  the 4th job fails)? The release itself MUST fail, backing out the
  partial provenance. No mixed-state releases where some artifacts
  have provenance and others don't. Enforced by making the provenance
  emission a required step in each build job, not a separate optional
  job at the end.
- **SLSA predicate URI version bump upstream**: what happens if
  slsa.dev bumps the Provenance predicate schema from v1 to v2 mid-
  release-cycle? Waybill inherits whatever version the pinned
  `actions/attest-build-provenance` action emits at that pin — no
  in-band re-emission. Version-bump PRs against waybill's workflows
  are the migration path.
- **Non-release CI runs**: the `Lint + test` CI lanes and any
  scheduled canaries do NOT emit provenance — they don't produce
  distributable artifacts. Only the release + nightly workflows do.
- **Historical releases**: releases produced BEFORE this feature ships
  do NOT get retroactive provenance. Downstream consumers verifying
  old releases will see "no provenance found" from
  `gh attestation verify`. This is expected behavior; documented in
  the release notes.
- **Attestation storage cost**: GitHub's attestation store is free
  and unlimited today for public repos. No storage-cost concern.
- **Detached artifacts consumed outside GitHub**: consumers who
  mirror waybill binaries into their own artifact registry (Artifactory,
  Nexus, etc.) can still verify — the attestation is bundled as a
  Sigstore bundle transferable alongside the artifact bytes. The recipe
  in US4's docs covers this path.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST emit a SLSA build provenance attestation
  for each of the 4 platform-specific binary tarballs produced by the
  stable release workflow (linux-x86_64, linux-aarch64, macos-x86_64,
  macos-aarch64). The attestation MUST bind the tarball's SHA-256
  digest as the subject and MUST identify the workflow file, commit
  SHA, and workflow-run URL as the build source.
- **FR-002**: The system MUST emit a SLSA build provenance attestation
  for the multi-arch OCI container image published to
  `ghcr.io/kusari-oss/waybill`. The attestation MUST bind the image
  manifest digest as the subject.
- **FR-003**: The system MUST emit a SLSA build provenance attestation
  for the source-code SBOM sidecar published per release (per m229
  release-flow implementation).
- **FR-004**: The provenance MUST use the current SLSA Provenance
  predicate schema as emitted by the pinned version of the GitHub
  `actions/attest-build-provenance` action — at the time of feature
  authoring (2026-08), this action targets `https://slsa.dev/provenance/v1`.
  The pin bumps as upstream ships new versions; waybill inherits.
- **FR-005**: Verification via `gh attestation verify <artifact>
  --repo kusari-oss/waybill` MUST succeed for every artifact type
  covered by FR-001 through FR-003 immediately after the release
  workflow completes.
- **FR-006**: Verification via `gh attestation verify` MUST fail with
  a diagnostic error message if the artifact bytes have been tampered
  with (any single byte differs from the released version).
- **FR-007**: Nightly release builds (per m229 nightly.yml) MUST emit
  provenance for the same artifact types stable releases do (4
  tarballs + 1 OCI image). Source SBOM sidecars, if produced by
  nightlies, MUST also carry provenance.
- **FR-008**: If provenance emission fails for any single artifact
  during a release, the whole release job MUST fail. No release MAY
  be published with a subset of artifacts carrying provenance —
  either all covered artifact types have provenance or the release
  is retried from scratch.
- **FR-009**: The system MUST publish a release-verification
  documentation page at a discoverable location under `docs/`
  containing copy-paste-ready `gh attestation verify` recipes for
  each artifact type covered by FR-001 through FR-003.
- **FR-010**: The verification recipe documentation MUST also
  describe how to verify artifacts pulled into third-party artifact
  registries (i.e., after the artifact has been mirrored out of
  GitHub Releases / GHCR) via the Sigstore bundle transferable
  alongside.
- **FR-011**: The waybill CLI itself MUST NOT change as part of this
  feature. Provenance is a release-pipeline concern; the compiled
  binary neither generates nor consumes SLSA Provenance predicates
  in this scope. (Separate future feature could add a
  `waybill verify slsa` subcommand for offline verification, but
  that's out of scope here.)
- **FR-012**: The waybill Rust workspace's `Cargo.toml` /
  `Cargo.lock` MUST NOT change as part of this feature. The feature
  is purely GitHub Actions workflow YAML + Markdown docs.
- **FR-013**: The release workflow MUST use SHA-pinned action
  references for the SLSA-emitting action (per waybill's existing
  action-SHA-pin convention, see memory
  `feedback_sha_pin_before_dependabot`). Version-tag references
  are forbidden.
- **FR-014**: Public-facing release notes, README, and documentation
  MUST NOT claim a specific SLSA Build level (L1/L2/L3) for waybill
  releases. Where SLSA is referenced, the language MUST be
  descriptive ("waybill emits SLSA build provenance attestations")
  rather than certifying ("waybill is SLSA Build L3"). Rationale:
  formal level claims require ongoing conformance verification
  (runner-isolation audits, provenance-non-forgeability spot-checks)
  that waybill does not commit to; downstream consumers rank
  releases per their own audit framework using the emitted
  attestation as evidence.
- **FR-015**: The release workflow MUST self-verify every emitted
  provenance attestation post-emission by invoking `gh attestation
  verify` (or the equivalent GitHub-native verification command at
  the time of implementation) against each artifact covered by
  FR-001 through FR-003. Self-verification MUST happen inside the
  same workflow run that emitted the attestation. Any single
  verification failure MUST fail the release job (matching the
  all-or-nothing semantics of FR-008), preventing publication of a
  release whose attestations don't verify. Rationale: catches
  misconfigured subject digests, malformed artifact paths, and
  workflow-identity mismatches before downstream consumers discover
  them.

### Key Entities *(include if feature involves data)*

- **SLSA Provenance predicate**: JSON document following the schema at
  `https://slsa.dev/provenance/v1` (or the current version at
  emission time). Structured payload includes `buildDefinition`
  (external parameters + resolved dependencies + build type URI) and
  `runDetails` (builder identity, metadata, byproducts). Emitted by
  the GitHub `actions/attest-build-provenance` action; waybill does
  not construct the predicate body directly.
- **Attestation subject**: a `{name: <artifact-filename>, digest:
  {sha256: <hex>}}` tuple identifying exactly which artifact bytes
  the predicate applies to. Each SLSA attestation MUST reference at
  least one subject; multiple subjects per attestation are permitted
  when a single build produces multiple correlated artifacts (e.g.,
  a tarball + its detached signature).
- **Sigstore bundle**: the transferable envelope wrapping the SLSA
  predicate + its Sigstore keyless signature. Stored by GitHub in
  the repo's attestation store; transferable to third-party
  registries via bundle file for offline verification.

## Assumptions

- The most up-to-date GitHub-built-in provenance-generation
  mechanism as of 2026-08 is the `actions/attest-build-provenance`
  action (currently at v3.x). If a newer mechanism ships before
  implementation begins, the plan phase re-checks and uses the
  latest. Alternatives NOT preferred: the earlier
  `slsa-github-generator` reusable workflows (deprecated for most
  use cases in favor of the built-in action), and
  `sigstore/gh-action-sigstore-python` (Python-native, wrong
  ecosystem fit for a Rust project).
- Downstream consumers use `gh attestation verify` as the primary
  verification tool. Secondary tools (`cosign verify-attestation`,
  policy engines) work against the same Sigstore bundle format; the
  MVP recipe uses `gh` because it's the shortest path for a first-
  time consumer.
- The current cosign-keyless signature (m222) coexists with the new
  SLSA provenance. Downstream consumers who already trust the cosign
  signature keep that path; new consumers trusting SLSA provenance
  use the new path. Both signature paths run on the same release
  workflow, both succeed together or the release fails together.
- Source SBOM sidecar identity is stable across the m229 release
  flow — the file's SHA-256 digest is the SLSA subject.
- All provenance emission uses the GitHub-hosted-runner build track
  (Actions). GitHub's own documentation describes the OIDC-bound
  ephemeral runner as meeting SLSA Build L3 requirements ("isolated
  build environment" + "provenance non-forgeability"), but waybill
  does NOT publicly claim a formal SLSA Build level per FR-014 —
  downstream consumers rank releases per their own audit framework
  using the emitted attestation as evidence.
- Waybill's release cadence remains: stable = manual `git tag +
  push`, nightly = cron. No workflow-trigger changes; provenance
  emission is additive to the existing workflow shape.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of release artifacts (4 tarballs + 1 OCI image + 1
  source SBOM sidecar = 6 subjects total) carry a verifiable SLSA
  provenance attestation within 60 seconds of the release workflow's
  final "success" status appearing on GitHub Actions.
- **SC-002**: 100% of `gh attestation verify <artifact>
  --repo kusari-oss/waybill` invocations against untampered release
  artifacts complete with a success exit code + human-readable
  identity output within 30 seconds of invocation, on a fresh
  installation of the `gh` CLI (no pre-existing local cache).
- **SC-003**: 100% of `gh attestation verify` invocations against a
  tampered (any-byte-flipped) release artifact fail with a distinct,
  non-generic error message identifying the SHA-256 mismatch.
- **SC-004**: A first-time downstream consumer following the
  release-verification recipe in the docs (US4) completes verification
  of at least one artifact type in under 5 minutes measured from
  landing on the docs page to the green verification result. Measured
  by having one non-author engineer time-box the recipe walk-through.
- **SC-005**: The release workflow's total wall-clock time increases
  by no more than 90 seconds vs the pre-feature baseline (as measured
  on the first N=3 post-merge releases). Budget breakdown:
  `actions/attest-build-provenance` emission ~30 seconds worst-case
  serialized (6 subjects × ~5 seconds), plus FR-015 self-verify via
  `gh attestation verify` ~30 seconds worst-case serialized (6
  subjects × ~5 seconds each with warm Sigstore Rekor cache in the
  workflow context), plus ~30 seconds slack. Emission and
  verification for the 4 platform tarballs happen inside the
  per-platform matrix jobs and therefore parallelize; the OCI-image
  and source-SBOM subjects add ~10-15 seconds each in a single job.
- **SC-006**: Zero waybill CLI behavior changes end-to-end pre-vs-
  post-feature. `cargo +stable test --workspace` produces byte-
  identical test counts + statuses. `cargo +stable clippy` produces
  byte-identical output. The compiled `waybill` binary emits byte-
  identical SBOMs on identical input.
- **SC-007**: Zero net-new Cargo dependencies at any layer.
  `git diff <base>..HEAD -- Cargo.toml Cargo.lock waybill-cli/Cargo.toml`
  produces zero lines.
- **SC-008**: The nightly release workflow's provenance emission
  succeeds on 100% of the first 7 nightly runs post-merge (one full
  week), matching the pre-existing nightly success rate.
