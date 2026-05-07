# Feature Specification: SPDX 3 externalIdentifierType controlled-vocabulary conformance

**Feature Branch**: `079-spdx3-id-vocab`
**Created**: 2026-05-07
**Status**: Draft
**Input**: GitHub issue #154 — "SPDX 3: externalIdentifierType controlled-vocabulary violation for built-in identifier schemes"

## Overview

Milestone 078 closed the JPEWdev `spdx3-validate` gate against mikebom's 9 source-tier SPDX 3 golden fixtures. Those fixtures don't exercise auto-detected or build-tier identifiers, so the gate was clean for them. Issue #154 documented the next gap: when image-tier, source-tier-with-git-detection, or build-tier flows reach the SPDX 3 emitter, mikebom's identifier schemes (the five built-ins `image`, `repo`, `git`, `attestation`, `subject` — sourced from milestone 074 auto-detect and milestone 076 build-tier internals — plus user-defined scheme names attached via milestone 073's `--component-id <PURL>=<SCHEME>:<VALUE>` flag) pass through into the `externalIdentifierType` field verbatim — but SPDX 3 defines `Core/ExternalIdentifierType` as a SHACL-enumerated controlled vocabulary with exactly 11 allowed values: `[other, cve, swhid, securityOther, cpe23, packageUrl, gitoid, cpe22, urlScheme, email, swid]`. Every mikebom-emitted scheme is outside that set, so the validator flags every such SBOM as non-conformant.

This milestone closes that gap. mikebom-emitted SPDX 3 SBOMs across all identifier sources (auto-detected + build-tier + user-defined) MUST pass `spdx3-validate` with zero `externalIdentifierType` violations. The fix maps each mikebom scheme to a conformant SPDX 3 vocabulary value at emission time, and preserves the original scheme name as supplementary metadata so operator intent isn't lost.

CDX 1.6 + SPDX 2.3 emission paths are unaffected — their identifier vocabularies are different and not in scope.

## Clarifications

### Session 2026-05-07

- Q: Which native SPDX 3 field carries the original mikebom scheme name when mapping to `externalIdentifierType: "other"`? → A: The `comment` field on the `Core/ExternalIdentifier` element. Free-text string carrying the original scheme as a structured value (e.g., `"original-scheme: image"`). Chosen because `comment` is universally available on SPDX 3 Core elements (no class-constraint navigation needed), it's spec-conformant native metadata (Principle V's first preference), and downstream tools that don't recognize specialized fields still surface `comment` text.
- Q: How aggressively should mikebom inspect identifier values to pick a more-specific SPDX 3 vocab type? → A: `gitoid`-only detection. mikebom inspects `git:` scheme values; if they match the regex `^[0-9a-f]{40}$` (a 40-char hex SHA-1), the SPDX 3 emission uses `externalIdentifierType: "gitoid"` instead of `"other"`. All other schemes + all other `git:` value shapes (e.g., `git+https://` URLs) map to `"other"` with the original scheme preserved in `comment` per Q1. CVE / CPE / SWHID detection is intentionally out of scope — those values rarely appear under mikebom's built-in schemes in practice, and adding detection for them creates regex surface for marginal value.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Auto-detected scheme conformance for image / source / build tiers (Priority: P1)

An operator scanning a container image with RepoTags (milestone 074), or a source tree inside a git repository (milestone 074), or running build-tier trace with attestation subjects (milestone 076), gets an SPDX 3 SBOM that passes the JPEWdev validator zero-error. Today every such scan produces a SHACL violation per identifier; post-fix every such scan validates cleanly.

**Why this priority**: This covers the dominant operator paths that produce identifiers — image scanning, git-repo source scanning, and build-tier traces. These are the exact cases where milestones 074/076 were added to identify and bind cross-tier artifacts; they must conform to SPDX 3 to fulfill the milestone-078 promise of "100% SPDX 3 conformant." P1 because the absence of this fix means operators using mikebom's headline cross-tier-binding feature still hit conformance violations.

**Independent Test**: Run `mikebom sbom scan --image registry.example.com/img:tag` (or `--path <git-repo>` or `mikebom trace run`); validate the emitted SPDX 3 with `spdx3-validate -j <file>`; assert exit 0 + zero `externalIdentifierType` violations.

**Acceptance Scenarios**:

1. **Given** an image-tier scan with non-empty RepoTags, **When** the operator emits SPDX 3 and runs the validator, **Then** zero `externalIdentifierType` violations are reported.
2. **Given** a source-tier scan against a directory inside a git repository (where milestone 074 auto-detects `repo:` and `git:` identifiers), **When** the operator emits SPDX 3 and runs the validator, **Then** zero `externalIdentifierType` violations are reported.
3. **Given** a build-tier trace that records `subject:` and `attestation:` identifiers (per milestone 076), **When** the operator emits SPDX 3 and runs the validator, **Then** zero `externalIdentifierType` violations are reported.
4. **Given** any of the above post-fix SBOMs, **When** the operator inspects an `externalIdentifier` element, **Then** the original scheme name (`image` / `repo` / `git` / `subject` / `attestation`) is preserved as supplementary metadata so cross-tier correlation tooling that filters by scheme still works.

---

### User Story 2 — User-defined `--component-id` conformance (Priority: P2)

An operator using milestone 073's `--component-id <PURL>=<SCHEME>:<VALUE>` flag to attach a custom identifier scheme to a specific component (e.g., `--component-id pkg:cargo/foo@1.0=jira:PROJ-1234`) gets an SPDX 3 SBOM that conforms to the controlled vocabulary. The user-defined scheme name is preserved in the `comment` field. Note: per `mikebom-cli/src/binding/identifiers/component_id.rs:52`, the parser **rejects** the five built-in scheme names (`repo`/`git`/`image`/`attestation`/`subject`) at flag parse time, so user-defined schemes are guaranteed not to collide with the auto-detect / build-tier paths from US1.

**Why this priority**: Lower priority because user-defined schemes are an explicit operator choice — operators who use the flag understand they're attaching non-standard identifiers. P2 rather than P1 because the volume of operators hitting it is smaller (auto-detect is implicit and ubiquitous; `--component-id` requires opt-in). Still P-track because conformance shouldn't depend on whether the operator opted in.

**Independent Test**: Run `mikebom sbom scan --path <dir> --component-id <PURL>=jira:PROJ-1234`; validate the emitted SPDX 3 with `spdx3-validate`; assert zero violations + original `jira` scheme name preserved in the `comment` field.

**Acceptance Scenarios**:

1. **Given** any `--component-id <PURL>=<SCHEME>:<VALUE>` invocation where `<SCHEME>` is not in the SPDX 3 controlled vocabulary, **When** the SBOM is validated, **Then** zero violations are reported.
2. **Given** the same invocation, **When** the operator inspects the SBOM, **Then** the original `<SCHEME>` value (e.g., `jira`) is recoverable from the `comment` field's `original-scheme: ` prefix.
3. **Given** a `--component-id <PURL>=<SCHEME>:<VALUE>` invocation where `<SCHEME>` IS in the SPDX 3 controlled vocabulary (e.g., `--component-id <PURL>=cve:CVE-2024-1234`), **When** the SBOM is validated, **Then** zero violations are reported AND the emitted `externalIdentifierType` is `<SCHEME>` verbatim with no `comment` field (no info loss).

---

### User Story 3 — CI gate prevents regression (Priority: P2)

The new conformance test on the milestone-078 CI gate covers all identifier-source paths (auto-detected + build-tier + user-defined), so a future PR that introduces a new non-vocab scheme to the SPDX 3 emission path fails CI before it can merge.

**Why this priority**: Same reasoning as milestone 078's US3 — the immediate breakage is fixed by US1 + US2; the CI hardening prevents tomorrow's regressions. P2 because the gate already exists from milestone 078; this milestone extends its coverage rather than building from scratch.

**Independent Test**: Modify the milestone-078 `fresh_image_tier_emission_passes` test to use non-empty RepoTags (per the issue's reproduction recipe); confirm the conformance gate runs against the broader identifier surface and continues to assert zero violations post-fix.

**Acceptance Scenarios**:

1. **Given** the existing milestone-078 conformance test infrastructure, **When** the test invocations include identifier-rich inputs (image RepoTags, git-detected source dirs, build-tier subjects), **Then** the validator is invoked against each and zero `externalIdentifierType` violations are required for the test to pass.
2. **Given** a future PR that introduces a new mikebom scheme without mapping it to the SPDX 3 vocabulary, **When** CI runs, **Then** the conformance test fails with the validator's error output visible in the build log.

---

### Edge Cases

- **Scheme value resembles a URL**: when an `image:` identifier value is `registry.example.com/img:tag` (a registry URL-shaped string but NOT an IANA URI scheme like `mailto:`/`tel:`), the conformant emission MUST NOT use `urlScheme` because the SPDX 3 spec reserves `urlScheme` for IANA URI schemes specifically. Use `other` with the original scheme preserved as supplementary metadata.
- **Scheme value IS a git commit SHA**: when a `git:` identifier value matches `^[0-9a-f]{40}$` (a SHA-1 git commit), per the 2026-05-07 clarification mikebom emits `externalIdentifierType: "gitoid"` (purpose-built for git object IDs in the SPDX 3 vocab). All other `git:` value shapes (URLs, abbreviated SHAs, branch names) map to `"other"` per FR-002.
- **Multiple identifiers per component, mixed types**: a single component may carry both an auto-detected build-tier identifier (e.g., a `subject:` identifier from milestone 076) and a user-defined `--component-id <PURL>=jira:...` identifier. Each MUST independently map to a conformant value; the SBOM MUST emit both as separate `externalIdentifier` array entries. (Note: the auto-detected built-ins `image`/`repo`/`git` are document-level, not per-component, so they don't co-occur with `--component-id` on the same component; `subject` and `attestation` are the only built-ins that apply per-component.)
- **Existing milestone-073/074/076 byte-identity goldens for CDX 1.6 + SPDX 2.3**: MUST stay byte-identical — those formats have different identifier vocabularies and are not affected by this change.
- **Existing SPDX 3 byte-identity goldens (9 source-tier ecosystem fixtures from milestone 078)**: those fixtures don't exercise auto-detected or build-tier identifiers, so they MUST stay byte-identical too — only fixtures that actually exercise the new mapping will regenerate.
- **Validator surfaces a previously-unknown vocab value**: if the SPDX 3 model adds new vocab values in a future release (e.g., `purloid`, `oci-image`), mikebom should be able to opt into the new value through a deliberate validator-version bump, not through silent emission changes.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: mikebom's SPDX 3 emission path MUST map every emitted `externalIdentifierType` value to one of the 11 SPDX 3 controlled-vocabulary values: `[other, cve, swhid, securityOther, cpe23, packageUrl, gitoid, cpe22, urlScheme, email, swid]`. No other values may appear in any emitted SPDX 3 SBOM.
- **FR-002**: For each mikebom built-in scheme that does not have a direct SPDX 3 vocabulary equivalent (`image`, `repo`, `git`, `attestation`, `subject`), mikebom MUST emit `externalIdentifierType: "other"` and preserve the original scheme name in the `comment` field of the `Core/ExternalIdentifier` element (per the 2026-05-07 clarification), formatted as a structured value (e.g., `"original-scheme: image"`), so cross-tier correlation tooling can still filter by the original scheme.
- **FR-003**: For user-defined `--component-id <PURL>=<SCHEME>:<VALUE>` invocations where `<SCHEME>` is not in the SPDX 3 controlled vocabulary, mikebom MUST emit `externalIdentifierType: "other"` and preserve the user-supplied `<SCHEME>` value in the `comment` field (same convention as FR-002) so operators don't lose their custom-scheme intent. When `<SCHEME>` IS in the SPDX 3 controlled vocabulary (e.g., `--component-id <PURL>=cve:CVE-1234`), mikebom MUST emit `externalIdentifierType: <SCHEME>` verbatim with no `comment` field (no info loss).
- **FR-004**: Per the 2026-05-07 clarification, mikebom MUST detect `git:` scheme values that match the regex `^[0-9a-f]{40}$` (a 40-char hex SHA-1 git commit) and emit them as `externalIdentifierType: "gitoid"`. All other `git:` value shapes (e.g., `git+https://` URLs, abbreviated SHAs) map to `"other"` per FR-002. No content-shape detection is performed for any other scheme — `image`, `repo`, `subject`, `attestation`, and user-defined schemes always map to `"other"` regardless of value shape.
- **FR-005**: The mapping MUST be byte-deterministic — same scheme + same value across re-runs produces the same `externalIdentifierType` + same supplementary-metadata field. The mapping logic itself MUST be a pure function of input scheme + value.
- **FR-006**: Existing milestone-073/074/076 byte-identity goldens for CDX 1.6 + SPDX 2.3 MUST stay byte-identical. The mapping is scoped strictly to the SPDX 3 emission code path.
- **FR-007**: SPDX 3 byte-identity goldens that do NOT exercise auto-detected, build-tier, or user-defined identifiers MUST stay byte-identical. Only fixtures that actually exercise the new mapping regenerate.
- **FR-008**: The milestone-078 conformance integration test MUST be extended (or new tests added in the same file) to cover the auto-detected + build-tier + user-defined identifier paths. Every covered path's emission MUST validate with zero `externalIdentifierType` violations.
- **FR-009**: When the JPEWdev validator's controlled-vocabulary set changes (a future SPDX 3 model release adds new values), mikebom's mapping MUST be updatable through a deliberate code change with a corresponding validator-version bump in `scripts/install-spdx3-validate.sh` — not through silent runtime detection.
- **FR-010**: CDX 1.6 emission of the same identifiers MUST remain unchanged — `externalReferences` field shape, type-string conventions, and reference URL formatting are not in scope for this milestone.
- **FR-011**: SPDX 2.3 emission of the same identifiers MUST remain unchanged — `externalRefs` field shape, `referenceCategory`/`referenceType` conventions are not in scope.

### Key Entities

- **mikebom scheme**: The internal identifier-type label mikebom uses across formats. Today: `image`, `repo`, `git`, `attestation`, `subject`, plus user-defined values. The internal label is preserved as the source-of-truth for cross-format mapping.
- **SPDX 3 vocabulary value**: One of `[other, cve, swhid, securityOther, cpe23, packageUrl, gitoid, cpe22, urlScheme, email, swid]`. The set the SPDX 3 SHACL constraint enforces on `Core/externalIdentifierType`.
- **Mapping decision**: For each (mikebom scheme, value) pair, a single SPDX 3 vocabulary value plus optional supplementary metadata. Pure-function of inputs; documented in research at implementation time so operators can predict the mapping.
- **Supplementary metadata slot**: The `comment` field on the `Core/ExternalIdentifier` element. Per the 2026-05-07 clarification, mikebom writes the original scheme as a structured value (e.g., `"original-scheme: image"`) into this field whenever the vocabulary mapping uses `"other"`. The format is intentionally readable by humans and parseable by tooling that recognizes the `original-scheme:` prefix.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Running `spdx3-validate` against a freshly-emitted SPDX 3 SBOM from an image-tier scan with non-empty RepoTags reports zero `externalIdentifierType` violations. Verified by integration test.
- **SC-002**: Running `spdx3-validate` against a freshly-emitted SPDX 3 SBOM from a source-tier scan inside a git repository (where milestone-074 auto-detects `repo:` and `git:` identifiers) reports zero `externalIdentifierType` violations. Verified by integration test.
- **SC-003**: Running `spdx3-validate` against a freshly-emitted SPDX 3 SBOM from a build-tier trace with `subject:` and `attestation:` identifiers reports zero `externalIdentifierType` violations. Verified by integration test.
- **SC-004**: Running `spdx3-validate` against a freshly-emitted SPDX 3 SBOM that includes user-defined `--component-id <PURL>=<SCHEME>:<VALUE>` identifiers (where `<SCHEME>` is not in the SPDX 3 vocab) reports zero `externalIdentifierType` violations. Verified by integration test.
- **SC-005**: Operators inspecting any post-fix SBOM can recover the original mikebom scheme name (e.g., `image`, `repo`, `subject`, or a user-supplied `jira`) from supplementary metadata. Verified by direct JSON-LD field assertions in tests.
- **SC-006**: Existing milestone-073/074/076 byte-identity goldens for CDX 1.6 + SPDX 2.3 stay byte-identical. Verified by `cdx_regression` and `spdx_regression` test targets continuing to pass without `MIKEBOM_UPDATE_*_GOLDENS` env vars.
- **SC-007**: Existing SPDX 3 byte-identity goldens (the 9 source-tier ecosystem fixtures from milestone 078) stay byte-identical except for any fixtures that actually exercise the new mapping path (likely 0–3 fixtures). Verified by per-fixture diff inspection during golden regen.
- **SC-008**: A PR that introduces a new mikebom scheme without mapping it fails CI with a clear validator-output error pointing at the violation. Verified by deliberate-regression smoke test (mirroring milestone 078's T014(d) pattern).

## Assumptions

- The 11-value SPDX 3 controlled vocabulary `[other, cve, swhid, securityOther, cpe23, packageUrl, gitoid, cpe22, urlScheme, email, swid]` is current as of the JPEWdev `spdx3-validate==0.0.5` pin from milestone 078. Future SPDX 3 model releases that add or remove vocabulary values will be handled through a deliberate validator-version bump (FR-009).
- The default mapping for built-in non-vocab schemes is `"other"` with the original scheme name preserved in the `Core/ExternalIdentifier` element's `comment` field, per the 2026-05-07 clarification. The exact `comment` value format is `"original-scheme: <name>"`.
- Content-shape detection (FR-004) is scoped narrowly per the 2026-05-07 clarification: `git:` values matching `^[0-9a-f]{40}$` map to `gitoid`; everything else maps to `other`. No CVE / CPE / SWHID detection — those vocabulary values are reachable only via `--component-id <PURL>=cve:...` / `--component-id <PURL>=cpe23:...` / `--component-id <PURL>=swhid:...` invocations where the operator has named the scheme deliberately.
- mikebom's CDX 1.6 + SPDX 2.3 emission paths use independent vocabularies (CDX `externalReferences[].type`, SPDX 2.3 `externalRefs[].referenceCategory`/`referenceType`) and are not modified by this milestone.
- The milestone ships as a single PR. The fix is bounded enough (mapping function + 4 new test scenarios + validator-coverage extension) to deliver in one cycle.
- The conformance test infrastructure introduced in milestone 078 (`mikebom-cli/tests/spdx3_conformance.rs` + `MIKEBOM_REQUIRE_SPDX3_VALIDATOR` env var + `scripts/install-spdx3-validate.sh`) is reused. No new validator integration is needed; the test surface is extended.
- Existing operators who programmatically consume mikebom's pre-fix SPDX 3 output by hard-coding `externalIdentifierType: "image"` (or other non-vocab values) will need to update their consumers. This is the expected operator-visible change of the milestone, analogous to milestone 078's createdBy slot move. Operators who consume by spec-defined paths (filter by `externalIdentifierType` against the SPDX 3 vocab + read the supplementary metadata slot) will work post-fix without changes.
- This milestone deliberately does NOT pursue option (3) from the issue body (petition SPDX upstream to add new vocab values). That's a parallel advocacy effort with a multi-quarter timeline; this milestone fixes the conformance gap mikebom can fix today.
