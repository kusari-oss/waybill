# Feature Specification: Strip userinfo credentials from auto-detected git URLs

**Feature Branch**: `075-strip-id-credentials`
**Created**: 2026-05-06
**Status**: Draft
**Input**: User description: "When mikebom auto-detects a `repo:` or `git:` identifier from a git remote URL, the URL MUST have userinfo (the `USER:TOKEN@` portion of `https://USER:TOKEN@github.com/...`) stripped before being embedded in the emitted SBOM. Manual flags emit verbatim. Provide an opt-out flag for operators who legitimately want credentials preserved."

## Overview

Milestones 073 and 074 ship identifier auto-detection that calls `git remote get-url` and embeds the result verbatim in published SBOMs. If an operator has configured a git remote with userinfo credentials — e.g., `https://USER:TOKEN@github.com/acme/foo.git`, common in CI runners using GitHub App tokens or deploy tokens for private-repo access — those credentials currently end up visible in:

- CDX `metadata.component.externalReferences[type:vcs].url`
- SPDX 2.3 `Package.externalRefs[PERSISTENT-ID].referenceLocator` and `creationInfo.creators[]` redundant text line
- SPDX 3 `Element.externalIdentifier[].identifier`

SBOMs are typically published artifacts (release attachments, OCI registry referrers, attached to images, archived in compliance pipelines, ingested by third-party scanners). A token visible in any one of those carriers is a supply-chain leak path: the operator never intended to publish the token; the publication happened as a side-effect of running mikebom.

This milestone closes the leak path. Auto-detected URL values get sanitized (userinfo stripped) before being embedded in any emitted identifier. Manual operator-supplied values stay verbatim — the operator typed them explicitly and the tool respects that. An opt-out flag exists for operators on private/internal networks where the credentials are non-sensitive and they specifically want the original URL preserved.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Source-tier auto-detect strips credentials by default (Priority: P1)

A developer or CI runner has configured a git remote URL with embedded credentials (e.g., `https://x-access-token:ghs_AAA...@github.com/acme/private-repo.git`, the standard GitHub App token form). They run `mikebom sbom scan --path .` with no manual identifier flags. The emitted source SBOM contains a `repo:` identifier with the URL sanitized to `https://github.com/acme/private-repo.git` — the token is gone.

**Why this priority**: This is the most common leak vector. Source-tier scans run on every CI build of every repo that uses mikebom. Without this protection, every published source SBOM is a potential token-disclosure incident. P1 because it's the largest blast-radius surface in mikebom's public footprint.

**Independent Test**: Initialize a git repo with `git remote add origin https://USER:TOKEN@github.com/foo/bar.git`, run `mikebom sbom scan --path . --output out.cdx.json`, and verify the emitted SBOM contains `repo:https://github.com/foo/bar.git` (no userinfo). Equivalently, `jq` over the emitted SBOM finds zero occurrences of the literal token string.

**Acceptance Scenarios**:

1. **Given** a source repository with `origin` set to `https://USER:TOKEN@github.com/foo/bar.git`, **When** the operator runs `mikebom sbom scan --path .` with no identifier flags, **Then** the emitted SBOM's `repo:` identifier has value `https://github.com/foo/bar.git` (userinfo stripped). The literal token string MUST NOT appear anywhere in the emitted document.
2. **Given** the same repository, **When** the operator inspects the emitted SBOM, **Then** the `source_label` for the auto-detected identifier reflects the sanitization (e.g., `"auto-detected from git remote `origin` (credentials stripped)"`).
3. **Given** the same repository, **When** the operator runs the scan, **Then** an info-level log line records the sanitization: which identifier was affected and that userinfo was stripped (without revealing the redacted credential value — log `<userinfo redacted>` or similar).
4. **Given** a source repository with an SSH-form remote `git@github.com:foo/bar.git` (no userinfo by construction), **When** the operator runs the scan, **Then** the emitted `repo:` identifier matches the original URL byte-for-byte. SSH-form URLs are unaffected.

---

### User Story 2 - Build-tier auto-detect strips credentials by default (Priority: P1)

The same protection applies to `mikebom trace run` (milestone 074's auto-detection path). A developer running `mikebom trace run -- ./build.sh` in a git checkout configured with credentialed origin URL gets a build-tier SBOM with both `repo:` and `git:` identifiers sanitized.

**Why this priority**: Build-tier scans run as part of CI build-and-publish flows. The same leak vector applies — the build SBOM gets attached to artifacts, included in attestations, and signed by Sigstore. A token in a signed attestation is *worse* than one in a plain SBOM because it's now cryptographically bound to the supply-chain proof itself; revoking the token doesn't undo the publication. P1 for the same reasoning as US1 plus the attestation amplification.

**Independent Test**: In a git checkout with `origin` set to a credentialed URL, run `mikebom trace run -- /usr/bin/true`. Verify the emitted build-tier SBOM contains `repo:` and `git:` identifier slots whose URLs have userinfo stripped, and the literal token string appears nowhere in the document.

**Acceptance Scenarios**:

1. **Given** a build cwd with `origin` set to `https://x-access-token:TOKEN@github.com/foo/bar.git`, **When** the operator runs `mikebom trace run`, **Then** the emitted build-tier SBOM's `repo:` identifier has value `https://github.com/foo/bar.git` and the `git:` identifier has value `https://github.com/foo/bar.git#<commit-sha>` — both sanitized; the original token string appears nowhere in the emitted document.
2. **Given** the same setup, **When** the operator inspects log output, **Then** info-level log lines record both sanitizations (one per affected identifier).

---

### User Story 3 - Manual identifier flags emit verbatim (Priority: P1)

Operators who explicitly supply identifier values via `--repo`, `--git-ref`, or `--id repo=...` flags get those values emitted verbatim in the SBOM. No sanitization. If the operator typed credentials, mikebom respects that.

**Why this priority**: Operators have legitimate reasons to type a credentialed URL — internal/airgapped deployments where the credential is shared infrastructure, or test scenarios where the URL is known-fake. Sanitizing manual input would surprise operators and break workflows that explicitly want the original URL. P1 because the rule is symmetric with the auto-detect strip rule: auto = strip, manual = verbatim, and getting that boundary right is what makes the feature trustworthy.

**Independent Test**: Run `mikebom sbom scan --repo https://USER:TOKEN@github.com/foo/bar.git --path /tmp/non-git-dir --output out.cdx.json`. Verify the emitted SBOM contains `repo:https://USER:TOKEN@github.com/foo/bar.git` exactly as typed.

**Acceptance Scenarios**:

1. **Given** an operator runs `mikebom sbom scan --repo https://USER:TOKEN@github.com/foo.git --path .`, **When** the SBOM is emitted, **Then** the `repo:` identifier value is `https://USER:TOKEN@github.com/foo.git` byte-for-byte. No sanitization, no warning.
2. **Given** an operator runs `mikebom trace run --repo https://USER:TOKEN@github.com/foo.git -- ./build.sh`, **Then** the manual flag wins over auto-detection (per milestone 074's FR-004) and the emitted `repo:` value is the operator's verbatim input.
3. **Given** auto-detection emits a sanitized `repo:https://github.com/foo.git` and the operator additionally passes `--id custom_repo=https://USER:TOKEN@internal.acme.corp/foo.git`, **Then** the user-defined `custom_repo:` identifier rides through verbatim (manual = verbatim rule applies to user-defined schemes too).

---

### User Story 4 - Opt-out flag preserves original URL (Priority: P2)

Operators on private/internal-network setups where the credentials are deliberately non-sensitive (e.g., a public read-only deploy token, an internal-network-only HTTPS token that has no value outside the corporate VPN) can opt out of sanitization globally for their scan via a CLI flag.

**Why this priority**: Real but narrow use case. The default behavior (strip) handles 95% of operators correctly. The opt-out is for edge cases — large organizations with internal SBOM pipelines where the credentials are infrastructure-level, not secrets. P2 because the absence of the flag is acceptable for the milestone's MVP (operators with that need can pass manual `--repo` to override per US3); the flag is a convenience.

**Independent Test**: With a credentialed `origin` URL, run `mikebom sbom scan --path . --keep-credentials-in-identifiers`. Verify the emitted SBOM's `repo:` identifier preserves the userinfo verbatim.

**Acceptance Scenarios**:

1. **Given** a credentialed `origin` URL, **When** the operator runs `mikebom sbom scan --path . --keep-credentials-in-identifiers`, **Then** the emitted SBOM's `repo:` identifier preserves the original URL with userinfo intact.
2. **Given** the same flag is passed to `mikebom trace run`, **Then** both auto-detected `repo:` and `git:` identifiers preserve userinfo in the emitted build-tier SBOM.
3. **Given** the flag is set, **When** the operator inspects log output, **Then** an info-level log line records that sanitization was suppressed by the flag (so the audit trail still reflects the operator's choice).

---

### Edge Cases

- **URL with empty userinfo**: `https://@github.com/foo.git` (a malformed-but-syntactically-valid URL with empty userinfo). The empty userinfo is still userinfo per RFC 3986 — strip it, leaving `https://github.com/foo.git`.
- **URL with userinfo but no password**: `https://username@github.com/foo.git` (legitimate auth pattern for some setups). Strip it — the username may be a personal access token in some configurations (`oauth2:TOKEN` pattern is the explicit token form, but bare `TOKEN@` is also possible). Treating any userinfo as sensitive is the safe-by-default rule.
- **`git://` and other non-HTTPS schemes**: Apply the same userinfo-strip rule. RFC 3986 defines userinfo for any URI scheme that uses the `authority` component, including `git://` (rare but legitimate).
- **Malformed URLs that don't parse cleanly**: A `git remote get-url` output that doesn't parse as RFC 3986 falls back to milestone 073's existing soft-fail-to-`UserDefined` rule (FR-010 in 073). Sanitization is best-effort: parse failure → emit verbatim and fall through to the existing soft-fail. Don't fail the scan over a parse error.
- **SSH-form URLs**: `git@github.com:foo/bar.git` is the SCP-like syntax. RFC 3986 doesn't define this form; it has no userinfo by construction (the `git@` is just a fixed username for the SSH protocol, not a credential). Pass through unchanged.
- **URL with both userinfo AND port**: `https://USER:TOKEN@github.com:443/foo.git`. Strip only the userinfo; preserve the port and everything else. Result: `https://github.com:443/foo.git`.
- **URL with userinfo AND fragment** (relevant for `git:<url>#<sha>`): `https://USER:TOKEN@github.com/foo.git`. After stripping userinfo and combining with the SHA: `git:https://github.com/foo.git#<sha>`. The sanitization happens before the SHA is appended.
- **Stripping makes the URL identical to a manually-supplied verbatim value**: An operator runs `mikebom sbom scan --repo https://github.com/foo.git --path .` AND auto-detection finds `https://USER:TOKEN@github.com/foo.git` — after sanitization both reduce to `https://github.com/foo.git`. Per milestone 073's FR-006 dedup-by-(scheme, value) rule, these collapse to a single entry attributed to the manual flag.
- **Multiple credentialed remotes**: A repo has `origin` and `upstream` both with different credentials. Only one wins per the milestone-073 fallback algorithm; the chosen one gets sanitized. The losing remote's URL never reaches the SBOM regardless. No new behavior.
- **Operator passes both `--keep-credentials-in-identifiers` AND `--id ...` with a credentialed user-defined value**: Both signals point the same direction — keep credentials. No conflict.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When auto-detecting a `repo:` identifier (source-tier per milestone 073, build-tier per milestone 074), mikebom MUST strip userinfo from the discovered URL before constructing the identifier value. URL parsing follows RFC 3986; userinfo is the `<user>[:<password>]@` segment that precedes the host.
- **FR-002**: When auto-detecting a `git:<repo-url>#<sha>` identifier (build-tier per milestone 074), the sanitization rule from FR-001 MUST be applied to the `<repo-url>` portion before the `#<sha>` is appended. The result is a `git:` value with the SHA intact and userinfo absent from the URL.
- **FR-003**: Sanitization MUST apply uniformly to HTTPS, HTTP, `git://`, and any other URI scheme that uses the RFC 3986 `authority` component. SSH-form URLs (`git@host:path`) carry no userinfo by construction and pass through unchanged.
- **FR-004**: Manual identifier flags (`--repo`, `--git-ref`, `--image-id`, `--attestation`, `--id <scheme>=<value>`) MUST emit values verbatim. No sanitization is applied to operator-supplied input.
- **FR-005**: A new opt-out flag `--keep-credentials-in-identifiers` MUST be available on `mikebom sbom scan` and `mikebom trace run`. When set, the FR-001/FR-002 sanitization is suppressed and auto-detected URLs emit verbatim. The flag is a boolean, default false.
- **FR-006**: When sanitization fires (i.e., FR-001 or FR-002 actually removed userinfo from a URL), mikebom MUST emit an info-level log line per affected identifier recording: which scheme (`repo` or `git`), that userinfo was stripped, and a `<userinfo redacted>` placeholder where the secret would otherwise appear in the log. The literal credential value MUST NOT appear in the log.
- **FR-007**: When sanitization is suppressed by `--keep-credentials-in-identifiers`, mikebom MUST emit an info-level log line acknowledging the suppression so the audit trail reflects the operator's choice.
- **FR-008**: The `source_label` field on auto-detected identifiers whose URL was sanitized MUST be augmented to indicate the sanitization (e.g., the source-tier label becomes `"auto-detected from git remote `origin` (credentials stripped)"`; build-tier labels follow the same `(credentials stripped)` suffix). When sanitization does NOT fire (the URL had no userinfo to strip), the label is unchanged from milestones 073/074.
- **FR-009**: When the discovered URL fails to parse as RFC 3986, sanitization MUST fall through to milestone 073's existing soft-fail-to-`UserDefined` rule (FR-010 in 073). The scan MUST NOT fail over a URL parse error. The unparseable value emits as user-defined under the `mikebom:identifiers` namespace, unchanged from existing behavior.
- **FR-010**: Sanitization MUST be deterministic. Given a fixed input URL, the sanitized output is byte-identical across runs. No randomness, no timestamping, no environment-dependent transformation.
- **FR-011**: The sanitization step MUST execute exactly once per `(scheme, value)` pair at identifier-construction time. It MUST NOT re-run during emission per format. The post-sanitization value is what flows into all per-format carriers (CDX `externalReferences`, SPDX 2.3 `externalRefs`, SPDX 3 `externalIdentifier`) without further transformation.
- **FR-012**: Cross-format byte-identity goldens for fixtures whose `git remote` configuration had no userinfo MUST stay byte-identical to alpha.16 (no sanitization fires; no behavior change). Per milestone 074's T001 audit, no existing fixture has a credentialed remote, so the golden regen is empirically a no-op for this milestone. Test fixtures that exercise the credentialed-URL path live in the new integration test file and produce no goldens.

### Key Entities

- **SanitizedUrl**: A URL value that has had its RFC 3986 userinfo component removed (if present) prior to embedding in an identifier. Composed of: the original URL string, the sanitized URL string, and a boolean `sanitized` flag indicating whether userinfo was actually present and removed. Used internally by `discover_repo_url` and the build-tier wrapper to drive log-line emission and `source_label` augmentation.
- **CredentialOptOut**: The boolean state flowing from the `--keep-credentials-in-identifiers` flag through both `mikebom sbom scan` and `mikebom trace run`. Threaded into the identifier construction path so the sanitization step can be conditionally suppressed. Default false.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For an operator with `https://USER:TOKEN@github.com/foo.git` configured as origin, the emitted source-tier SBOM (with no manual flags, no opt-out flag) contains zero occurrences of the literal token string anywhere in the document. Verified by integration test: build a tempdir git fixture with a known-fake token, run `mikebom sbom scan`, search the emitted JSON for the token string, assert zero matches.
- **SC-002**: The same zero-occurrence guarantee holds for build-tier SBOMs emitted by `mikebom trace run` against the same fixture — both the `repo:` and `git:` identifier slots sanitized, and zero occurrences in the document.
- **SC-003**: When the operator passes `--repo https://USER:TOKEN@github.com/foo.git` explicitly, the emitted SBOM contains the literal token string in exactly one place: the `repo:` identifier value. Manual = verbatim, no sanitization. Verified by integration test on both `mikebom sbom scan` and `mikebom trace run`.
- **SC-004**: When the operator passes `--keep-credentials-in-identifiers`, auto-detected URLs emit verbatim. The emitted SBOM contains the literal token string in the auto-detected identifier slots. Verified by integration test against the same fixtures.
- **SC-005**: Sanitization adds less than 1ms to identifier construction for a typical git checkout (one URL parse + one userinfo-clear per identifier). Verified by manual smoke timing during quickstart validation; no dedicated benchmark fixture.
- **SC-006**: The `source_label` field on sanitized identifiers contains the substring `credentials stripped`. Verified by unit and integration tests asserting on the `source_label` value.
- **SC-007**: SSH-form URLs (`git@host:path`) emit byte-identical to alpha.16 in both auto-detected and manual paths. Verified by integration test using an SSH-form remote.
- **SC-008**: 100% of existing milestone-073 and milestone-074 byte-identity goldens stay byte-identical after milestone 075 ships, since the existing fixture set has no credentialed remotes (audit per milestone 074 task T001 confirmed). Verified by the existing parity-check golden suite continuing to pass unchanged.

## Assumptions

- The implementation reuses Rust's `url` crate (already in the workspace dep graph if available, or addable as a small dependency) for RFC 3986 parsing. Rolling our own URL parser is out of scope. If `url` isn't already a workspace dep, this milestone adds it — a one-line `Cargo.toml` change with broad ecosystem use.
- Sanitization scope is **userinfo only**. Other URL components (scheme, host, port, path, query, fragment) pass through unchanged. Specifically: query-string credentials (e.g., `https://github.com/foo.git?token=ABC`) are NOT sanitized by this milestone — they are an extremely rare pattern and addressing them would expand scope significantly. Operators with query-string credentials should pass `--repo` manually with sanitized URL, or accept the verbatim emission.
- The opt-out flag `--keep-credentials-in-identifiers` lives on `mikebom sbom scan` and `mikebom trace run`. It does NOT affect manual flag values (which are already verbatim per FR-004). Setting the flag has no observable effect when the operator provides only manual identifier flags.
- This milestone fixes a known information-disclosure path that pre-dated 074 (introduced in 073 when source-tier auto-detect first shipped). 074 inherited it without making it worse. The fix applies symmetrically across both auto-detect call sites and inherits 074's per-tier `source_label` distinction (FR-008 augmentation preserves the build-tier vs source-tier prefix).
- Manual flag verbatim emission preserves the existing milestone 073/074 contract that operators who explicitly type credentials are responsible for their choice. This milestone does not introduce any "smart" detection on manual input (e.g., warning the operator that their manual `--repo` value contains userinfo). That is reserved for a future hardening milestone if an operator-survey-like signal indicates demand.
- Goldens for the existing fixture set are unaffected per milestone 074's T001 audit (no fixture has a credentialed remote). The new integration test file for this milestone uses tempdir-based fixtures and produces no goldens.
- The released alpha.16 binaries are NOT re-published as a hotfix. Operators concerned about the leak path can mitigate today by passing `--repo <sanitized-url>` manually. This milestone ships in alpha.17.
- The SBOM consumer ecosystem treats userinfo-bearing URLs as semantically equivalent to userinfo-stripped URLs for correlation purposes — i.e., a downstream tool resolving `repo:https://github.com/foo.git` should match an SBOM that originally had `repo:https://USER:TOKEN@github.com/foo.git`. Sanitization preserves the cross-tier correlation contract from milestones 072-074 because the sanitized form is the canonical one.
