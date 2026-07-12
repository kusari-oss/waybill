# Feature Specification: OCI Referrers API SBOM discovery (fetch instead of re-scan)

**Feature Branch**: `186-oci-referrers-sbom`
**Created**: 2026-07-11
**Status**: Draft
**Input**: User description: "#442 — OCI Distribution Spec v1.1 added a `/v2/<repo>/referrers/<digest>` endpoint that advertises companion artifacts attached to an image (SBOMs, attestations, signatures). When an SBOM has already been generated upstream and published alongside the image, re-scanning the image bytes is redundant work. mikebom should optionally discover + fetch a pre-existing SBOM from the Referrers API instead of scanning. Filter to SBOM media types (`application/spdx+json`, `application/vnd.cyclonedx+json`, `application/vnd.cyclonedx+xml`); if a matching referrer is found AND the operator opted in, fetch + emit it (with provenance markers identifying it as referrer-sourced). Otherwise fall through to the existing scan path. Extends the m031+m032+m034+m036+m182 OCI pull infrastructure. Provenance: `mikebom:sbom-source = \"referrer\"` annotation + the referring artifact's descriptor digest recorded for auditability."

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Fetch a referrer SBOM when available; fall back to scan otherwise (Priority: P1)

An operator scanning a well-provisioned production image (e.g., a Docker Hub or GHCR image that publishes an SBOM at build time via `docker buildx --sbom=true`) wants mikebom to prefer the upstream SBOM if it exists, and fall back to scanning the image bytes otherwise. This saves substantial CPU and network cost on large images while preserving the guarantee that mikebom ALWAYS emits an SBOM.

**Why this priority**: Modern container CI pipelines routinely publish SBOMs alongside images (SLSA compliance, Kubernetes admission-controller integration, supply-chain security requirements). Every re-scan of an image whose SBOM is already published is wasted work. The `either` mode is the operator-friendly default for teams migrating from "always scan" to "trust the upstream when available."

**Independent Test**: Scan an OCI image whose registry has a Referrers API entry for `application/vnd.cyclonedx+json` attached to the image's manifest digest. Invoke `mikebom sbom scan --image <ref> --sbom-source either --format cyclonedx-json --output out.cdx.json`. Verify (a) mikebom fetches the referrer artifact instead of running the scanner, (b) the emitted `out.cdx.json` matches the referrer's bytes verbatim, (c) mikebom logs an INFO-level line identifying the referrer descriptor digest as the source, (d) if the same command is run against an image WITHOUT a matching referrer, mikebom falls through to the scan path and emits a scanner-derived SBOM.

**Acceptance Scenarios**:

1. **Given** a registry image whose `/v2/<repo>/referrers/<digest>` endpoint returns exactly one descriptor with media type `application/vnd.cyclonedx+json`, **When** the operator runs mikebom with `--sbom-source either --format cyclonedx-json --output out.cdx.json`, **Then** the emitted `out.cdx.json` MUST be byte-identical to the referrer artifact's content, AND mikebom MUST log the referrer descriptor digest at INFO level.
2. **Given** the same image and command, **When** mikebom emits, **Then** the mikebom scan metadata MUST include `mikebom:sbom-source = "referrer"` AND `mikebom:sbom-source-descriptor-digest = "<sha256:...>"` recorded as scan-run properties (not mutations of the fetched SBOM content).
3. **Given** an image WITHOUT any Referrers API response OR a Referrers response containing zero SBOM-shaped media types, **When** the operator runs the same command, **Then** mikebom MUST fall through to the existing scan path AND emit a scanner-derived SBOM. The scanner-derived SBOM's scan metadata MUST include `mikebom:sbom-source = "scan"` (or absence, backward-compatible with pre-m186).
4. **Given** the registry returns HTTP 404 on `/v2/<repo>/referrers/<digest>` (indicating no Referrers API support at all), **When** mikebom is run with `--sbom-source either`, **Then** mikebom MUST fall through to the scan path silently (the 404 is expected; no warning to the operator).

---

### User Story 2 — Strict-mode referrer requirement (Priority: P1)

A compliance-driven operator wants to ENFORCE that mikebom emits only upstream-published SBOMs — no scanner-derived SBOMs allowed under any circumstance. This is used in supply-chain-audit workflows where the upstream SBOM has been signed by a trusted party and the auditor MUST NOT accept any scanner-derived alternative.

**Why this priority**: Same class of use case as `cosign verify` requiring valid signatures — the operator wants a fail-closed guarantee. If no referrer SBOM exists, the scan MUST fail rather than fall through to scanner-derived output.

**Independent Test**: Same setup as US1 acceptance scenario 3 (image WITHOUT a matching referrer). Run mikebom with `--sbom-source referrer --format cyclonedx-json --output out.cdx.json`. Verify mikebom exits non-zero with an actionable error naming both the image reference AND the exact "no matching SBOM referrer found" cause. Verify NO output file is written.

**Acceptance Scenarios**:

1. **Given** a registry image with a valid CDX SBOM referrer, **When** the operator runs `mikebom sbom scan --image <ref> --sbom-source referrer`, **Then** mikebom MUST fetch + emit the referrer verbatim (same as US1 acceptance 1).
2. **Given** a registry image WITHOUT any SBOM referrer (Referrers API returns empty list OR only non-SBOM media types like signatures/attestations), **When** the operator runs `--sbom-source referrer`, **Then** mikebom MUST exit non-zero AND print an actionable error naming the image ref + the reason ("no matching SBOM referrer found for `<image>` on registry `<registry>`"). NO output file is written.
3. **Given** a registry that returns HTTP 404 on the Referrers endpoint (no v1.1 support), **When** mikebom is run with `--sbom-source referrer`, **Then** mikebom MUST exit non-zero AND print an actionable error naming the registry + the reason ("registry `<name>` does not support the OCI Referrers API (HTTP 404); use `--sbom-source scan` or `--sbom-source either` to scan the image bytes instead").

---

### User Story 3 — Scan-only opt-out (Priority: P1)

An operator running mikebom in a regulated environment where upstream SBOMs are NOT considered authoritative (e.g., the operator's own build system produces a distinct SBOM that supersedes any embedded one) wants mikebom to ALWAYS scan and NEVER consult the Referrers API. This preserves the pre-m186 default behavior byte-identically.

**Why this priority**: Backward compatibility. Every existing mikebom invocation (from before m186 lands) implicitly assumed "always scan." Ensuring a `--sbom-source scan` mode preserves this exact semantic guarantees zero regression risk for existing pipelines.

**Independent Test**: Scan an OCI image whose registry DOES have a matching CDX SBOM referrer. Invoke mikebom with `--sbom-source scan --format cyclonedx-json --output out.cdx.json`. Verify (a) mikebom does NOT call the Referrers API endpoint (no network activity beyond the standard image pull), (b) the emitted SBOM is scanner-derived and byte-identical to the pre-m186 output for the same image.

**Acceptance Scenarios**:

1. **Given** an image WITH a valid SBOM referrer available on the registry, **When** the operator runs `--sbom-source scan`, **Then** mikebom MUST NOT invoke the Referrers API endpoint AND MUST scan the image bytes to produce the SBOM. Byte-identical to the pre-m186 scan behavior for the same image.
2. **Given** the default `--sbom-source` (not explicitly specified), **When** the operator runs mikebom, **Then** the behavior MUST be identical to `--sbom-source scan`. Default is `scan` for backward compatibility.

---

### Edge Cases

- **Multiple SBOM referrers** for the same image digest (e.g., both a CDX 1.6 and an SPDX 2.3 referrer are attached): mikebom picks the one whose media type matches the operator's requested `--format` FIRST. If no format match, pick the first CDX-shaped referrer, then the first SPDX-shaped referrer, then any other SBOM-shaped media type. If the operator requested multiple `--format` values, prefer the referrer whose media type matches the FIRST requested format. If still no match, fall through to scan (in `either` mode) OR error (in `referrer` mode).
- **Referrer format mismatch** — operator requested `--format cyclonedx-json` but the only available referrer is `application/spdx+json`. Under `either` mode: fall through to scan (mikebom's scanner CAN emit any format; the referrer can't be transcoded). Under `referrer` mode: emit the SPDX SBOM as-is + emit a WARN log naming the format mismatch AND the operator's requested format ("no `cyclonedx-json` referrer found; emitting SPDX 2.3 referrer as-is at `<output>`"). Do NOT rename the output file's extension — respect the operator's `--output` path verbatim.
- **Multi-arch image index** — the operator requested a specific platform (e.g., `--image-platform linux/amd64`). The Referrers API is queried against the RESOLVED single-platform manifest digest, NOT the index digest. Same platform-resolution semantics as the existing m035 image-platform flag.
- **Referrer artifact is a signed attestation envelope** (in-toto DSSE / Cosign) wrapping the SBOM: MVP treats referrers as OPAQUE BYTES. If the media type reports `application/spdx+json` or `application/vnd.cyclonedx+json`, mikebom emits verbatim. If the media type reports an attestation envelope (`application/vnd.in-toto+json`, `application/vnd.dev.cosign.simplesigning.v1+json`), mikebom SKIPS that referrer (does not extract the payload). Attestation-payload extraction is deferred to a follow-up milestone.
- **Referrer artifact fails download** (registry 5xx, hash mismatch, TLS failure): mikebom logs the failure at WARN level. Under `either` mode, falls through to scan. Under `referrer` mode, exits non-zero with the underlying error message.
- **Registry auth failure on Referrers endpoint** (401/403): under `either` mode, log the auth failure at INFO (not WARN — auth failures on Referrers are expected for private-image scans without credentials for the referrer namespace) and fall through to scan. Under `referrer` mode, exit non-zero with an actionable error naming both the image AND the auth error.
- **Registry rate-limits the Referrers endpoint** (429 Too Many Requests): m186 treats 429 as a fetch failure — fall-through under `either` (INFO log with the 429 status), non-zero exit under `referrer` (stderr message includes the 429 status). `Retry-After` semantics + exponential backoff are DEFERRED to a follow-up milestone (see §Deferred to Future Milestones below). Rationale: m186 keeps the fetch path minimal + spec-compliant; adaptive retry is well-scoped for a dedicated follow-up so its behavior can be tuned without churn on the m186 contract.
- **Referrer descriptor content-length exceeds a configurable cap** (default 100 MiB — SBOMs shouldn't be larger; larger indicates a malicious or misconfigured artifact): mikebom skips the referrer with a WARN log and falls through per the media-mismatch rule.
- **`--sbom-source` used against a `--image <local-tarball-path>` input** (not an OCI reference): mikebom exits non-zero with an actionable error — the flag only applies to registry-pull scans, not to local tarball scans (which have no registry to query). The flag is silently ignored for `--path` scans.
- **The image reference is a plain tag** (not pinned to a digest): mikebom resolves the tag to its current digest via the standard image manifest fetch, THEN queries `/v2/<repo>/referrers/<digest>`. The Referrers query is ALWAYS against a digest, never a tag (per OCI Distribution Spec v1.1 §Referrers).
- **Provenance markers**: `mikebom:sbom-source = "referrer"` and `mikebom:sbom-source-descriptor-digest = "sha256:..."` MUST NOT mutate the emitted SBOM content (which is the referrer's bytes verbatim). Both markers ARE recorded in mikebom's own scan-run metadata (log line at INFO + optionally attached to a scan-summary annotation on the operator's terminal output). The design keeps the referrer bytes byte-identical to what the upstream signer signed.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: mikebom MUST add a new CLI flag `--sbom-source` accepting one of three values: `scan` (default; existing behavior), `referrer` (strict-mode: require referrer, fail if absent), `either` (prefer referrer, fall back to scan). Placement: on the `sbom scan` subcommand next to the existing `--image-src`, `--image-platform`, and `--registry-credentials-dir` flags.
- **FR-002**: mikebom MUST query the OCI Distribution Spec v1.1 Referrers API at `/v2/<repo>/referrers/<manifest-digest>` when the flag is `referrer` OR `either`. The endpoint is queried AFTER the image manifest has been resolved to a single-platform digest (per m035 platform resolution).
- **FR-003**: The Referrers API response MUST be parsed as an OCI image index (per Distribution Spec v1.1). mikebom MUST filter the returned descriptors to those whose media type matches one of: `application/spdx+json`, `application/vnd.cyclonedx+json`, `application/vnd.cyclonedx+xml`. Additional media types MAY be added in follow-up milestones without breaking m186's contract.
- **FR-004**: When multiple matching descriptors are present, mikebom MUST prefer the one whose media type matches the operator's requested `--format` value (first-match semantics if multiple `--format` values are requested). If no format match, mikebom MUST fall back to first-CDX-then-SPDX-then-any preference order.
- **FR-005**: When a matching descriptor is selected, mikebom MUST fetch the referring artifact via the standard OCI blob fetch (`/v2/<repo>/blobs/<digest>`) using the existing m034 credential-resolution infrastructure. The fetch MUST verify the descriptor's declared SHA-256 digest before emission.
- **FR-006**: The fetched artifact MUST be emitted as the operator's `--output` file BYTE-IDENTICALLY — no re-parsing, re-encoding, or transformation. Preserves the upstream signer's signed bytes verbatim.
- **FR-007**: mikebom MUST record `mikebom:sbom-source = "referrer"` and `mikebom:sbom-source-descriptor-digest = "sha256:..."` as scan-run metadata. These markers appear in mikebom's own INFO-level log stream + in an optional scan-summary JSON that operators can consume via a follow-up flag. These markers MUST NOT mutate the emitted SBOM file's contents.
- **FR-008**: Under `--sbom-source either`, mikebom MUST fall through to the existing scan path when: (a) the Referrers endpoint returns HTTP 404 (no v1.1 support), (b) the endpoint returns an empty descriptor list, (c) no descriptor matches an SBOM media type, (d) the descriptor fetch fails at TLS/HTTP/hash-verify layer, (e) the descriptor content exceeds the size cap.
- **FR-009**: Under `--sbom-source referrer`, mikebom MUST exit non-zero on each of the fall-through conditions listed in FR-008. The error message MUST name the image reference AND the specific failure reason ("no matching SBOM referrer found", "registry does not support Referrers API", "auth failure", "size cap exceeded", etc.).
- **FR-010**: Under `--sbom-source scan` (default), mikebom MUST NOT invoke the Referrers endpoint. Zero network activity beyond the existing image-pull scan path. Preserves byte-identity with pre-m186 scans of the same image (SC-004 gate).
- **FR-011**: The `--sbom-source` flag MUST be REJECTED (non-zero exit + actionable error) when used against a `--image <local-path>` (non-registry) input OR against a `--path` (filesystem) scan. Documented for operators via the CLI help text.
- **FR-012**: mikebom MUST use the existing m034/m036 registry-credentials + layer-cache infrastructure for Referrers endpoint queries. No new credential-resolution paths; no new cache paths. FR-013 preserves the m182 TLS/transport configuration invariant.
- **FR-013**: The m182 `--insecure-registry`, `--registry-ca-cert`, and `--insecure-tls-skip-verify` flags MUST apply to Referrers API calls identically to how they apply to manifest/blob fetches. The Referrers endpoint is a normal HTTP GET against the same registry host.
- **FR-014**: mikebom MUST expose a configurable size cap on referrer descriptor content, defaulting to 100 MiB. Operators MAY override via an environment variable (`MIKEBOM_REFERRER_MAX_BYTES`) for edge cases involving unusually large SBOMs. Descriptors exceeding the cap are skipped with a WARN log.
- **FR-015**: For scans that do NOT exercise the new signal (all invocations without `--sbom-source referrer|either`), the emitted SBOM output MUST be byte-identical to pre-m186. Regression guard on the default-scan path.
- **FR-016**: mikebom MUST NOT introduce any new Cargo dependency. The Referrers API response is an OCI image index shape that the existing `oci-spec` crate already parses.

### Key Entities

- **`SbomSourceMode` enum**: CLI-parsed value with variants `Scan` (default), `Referrer`, `Either`. Consumed by the OCI pull dispatcher to select the referrer-vs-scan path.
- **OCI image index of descriptors**: parsed from the Referrers API response body. Existing `oci-spec::image::ImageIndex` type covers this — no new parser.
- **SBOM media types**: enumerable constant set (`application/spdx+json`, `application/vnd.cyclonedx+json`, `application/vnd.cyclonedx+xml`). Follow-up milestones may extend the set.
- **Provenance markers**: two `mikebom:sbom-source-*` strings recorded in mikebom's scan-run metadata (NOT in the emitted SBOM's content).
- **Size cap**: `MIKEBOM_REFERRER_MAX_BYTES` environment variable, default 100 MiB. Same pattern as `MIKEBOM_OCI_CACHE_SIZE` (m036).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For a scan of an image with a matching CDX SBOM referrer, invoking `mikebom sbom scan --image <ref> --sbom-source either --format cyclonedx-json --output out.cdx.json` MUST produce an `out.cdx.json` file whose content is byte-identical to the referrer artifact's bytes. Emission time MUST be faster than the pre-m186 scan-based invocation on the same image (typical: <2 seconds vs typical scan of ≥5 seconds for a 50 MB image).
- **SC-002**: For a scan of an image WITHOUT a matching SBOM referrer under `--sbom-source either`, mikebom MUST fall through to scan and emit a scanner-derived SBOM. Total scan time MUST NOT exceed pre-m186 scan time for the same image by more than 10% (the Referrers query adds at most one HTTP round-trip).
- **SC-003**: For a scan under `--sbom-source referrer` against an image WITHOUT a matching referrer, mikebom MUST exit non-zero within 5 seconds AND produce a stderr message naming the specific failure reason (image ref + "no matching referrer found" OR "registry does not support Referrers API").
- **SC-004**: For every scan under `--sbom-source scan` (or default), the emitted SBOM MUST be byte-identical to pre-m186 output for the same image + same `--format` + same other flags. Zero drift on existing golden fixtures per FR-015.
- **SC-005**: mikebom MUST log the referrer descriptor digest AND media type at INFO level when a referrer is emitted (US1 + US2 flows). Operators consuming mikebom logs for audit purposes MUST be able to identify referrer-sourced emissions from log content alone (no need to inspect the emitted SBOM).
- **SC-006**: The `MIKEBOM_REFERRER_MAX_BYTES` cap MUST be enforceable via an integration test that stages a referrer descriptor claiming a `size` exceeding the cap. mikebom MUST skip the referrer and either (a) fall through to scan under `either` mode, or (b) exit non-zero with a size-cap-exceeded error under `referrer` mode.
- **SC-007**: Under the m182 `--insecure-registry` + `--registry-ca-cert` + `--insecure-tls-skip-verify` flags, Referrers API calls MUST honor the same transport configuration as manifest/blob calls. Verified via integration tests reusing the m182 wiremock infrastructure.
- **SC-008**: Zero new production Cargo dependencies added to `mikebom-cli/Cargo.toml` — `cargo tree -p mikebom | wc -l` MUST be identical pre- vs post-m186.
- **SC-009**: The `--sbom-source` flag's help text MUST clearly state the three modes and note that the flag is registry-only (not applicable to `--image <local-path>` or `--path` scans).

## Assumptions

- The OCI Distribution Spec v1.1 Referrers API endpoint (`/v2/<repo>/referrers/<digest>`) is served by most modern registries (Docker Hub, GHCR, gcr.io, Quay, Harbor 2.9+, distribution/distribution 2.8+, etc.). Registries lacking v1.1 support return HTTP 404 on the endpoint; mikebom detects this and falls through per FR-008.
- SBOM media type identifiers per SPDX 2.3 §D and CycloneDX Attestation Spec: `application/spdx+json` for SPDX 2.3 JSON, `application/vnd.cyclonedx+json` for CycloneDX JSON, `application/vnd.cyclonedx+xml` for CycloneDX XML. These are the industry-standard media types; other media types (SPDX 3, SPDX 2.2, tag-value, `application/vnd.oci.image.manifest.v1+json` for wrapped attestation, etc.) can be added in follow-up milestones.
- Referrer artifacts are treated as opaque bytes for emission. mikebom does NOT re-parse or re-encode; it fetches, verifies the descriptor's `sha256`, and writes to the operator's `--output` path verbatim. This preserves any upstream signer's byte-identity guarantees (Cosign signatures, in-toto attestations wrapping the SBOM, etc.).
- Signed-referrer verification (Cosign, Sigstore, in-toto envelope extraction) is OUT OF m186 scope. mikebom emits verbatim bytes; downstream tools may verify or unwrap those bytes if needed. Deferred to a follow-up milestone with its own signed-verification story.
- Format transcoding (e.g., referrer is SPDX 2.3 but operator requested CDX 1.6) is OUT OF m186 scope. Under `either` mode, mikebom falls through to scan when the format doesn't match; under `referrer` mode, mikebom emits the mismatched-format referrer as-is + a WARN log identifying the mismatch. Transcoding is a follow-up milestone.
- Default `--sbom-source` value is `scan` (preserves pre-m186 behavior byte-identically). This is a conservative default — operators must opt into the referrer path.
- The 100 MiB size cap (`MIKEBOM_REFERRER_MAX_BYTES` default) is conservative; typical SBOMs are 100 KiB to 5 MiB. A 100 MiB threshold catches malicious or misconfigured artifacts (referrer descriptor claiming an enormous SBOM to DoS the scanner) while allowing every realistic SBOM to pass.
- The m182 TLS/transport flags (`--insecure-registry` / `--registry-ca-cert` / `--insecure-tls-skip-verify`) automatically apply to Referrers calls because they configure the underlying reqwest client — no per-endpoint plumbing needed.
- No new Cargo deps needed — the existing `oci-spec` crate parses the Referrers response as `ImageIndex`; `reqwest` handles the HTTP GET; `sha2` handles the digest verification.
- Golden regeneration (`MIKEBOM_UPDATE_*_GOLDENS=1`) will show zero drift on ALL existing goldens per SC-004 — no existing fixture uses `--sbom-source`.

## Constitution Alignment

**Principle I (Pure Rust, Zero C)**: FR-016 + SC-008 verified. No new Cargo deps; the Referrers endpoint is a standard HTTP GET reusing existing reqwest + oci-spec infrastructure.

**Principle III (Fail Closed)**: FR-009 + SC-003 enforce the fail-closed guarantee for `--sbom-source referrer` mode. No silent fallbacks; every failure mode surfaces an actionable error.

**Principle IV (Type-Driven Correctness)**: The `SbomSourceMode` enum is compile-time typed; no stringly-typed dispatch. The Referrers response deserializes into `oci-spec::image::ImageIndex` — no manual JSON walking.

**Principle V (Specification Compliance + Native-first)**: The Referrers API is a first-class OCI Distribution Spec v1.1 endpoint — native to the OCI standard mikebom already implements. No `mikebom:*` invention in the wire protocol; only two `mikebom:sbom-source-*` markers in mikebom's OWN scan-run metadata (not in the emitted SBOM content).

**Principle IX (Accuracy)**: Emitting the upstream signer's bytes byte-identically preserves accuracy from the signed source. mikebom does NOT synthesize or transform, avoiding accuracy loss.

**Principle X (Transparency)**: FR-007 + SC-005 ensure operators can identify referrer-sourced emissions from mikebom's log content alone. The provenance markers are recorded in mikebom's OWN scan-run metadata for downstream audit consumption.

## Deferred to Future Milestones

- **Signed-referrer verification** (Cosign, Sigstore, in-toto DSSE envelope extraction): the m186 delivery treats referrers as opaque bytes. Signed-verification is a distinct milestone with its own crypto stack + trust-anchor management. Attestation-envelope unwrapping (extract the SBOM payload from an in-toto envelope) is deferred alongside.
- **Format transcoding** (referrer is SPDX 2.3 but operator wants CDX 1.6): m186 falls through to scan under `either` mode; emits mismatched-format under `referrer` mode. Transcoding is complex (schema-level rewriting, license normalization pipeline, etc.) and better addressed as its own milestone.
- **Additional SBOM media types** beyond the initial three (SPDX 3, SPDX tag-value, KDL SBOM, `application/vnd.cyclonedx+protobuf`): follow-up delivery. m186's design leaves the media-type-detection helper open to extension.
- **Multiple-referrer emission** (emit ALL matching referrers instead of just the first): m186 emits at most one. Multi-emission would require a per-referrer output-path scheme + new CLI ergonomics.
- **Referrer descriptor filtering by artifactType** (OCI Distribution v1.1 also supports filtering by `artifactType`, an app-level tag): m186 filters by media type only. Adding artifactType filtering is a small follow-up if operators need it.
- **Explicit Referrers-API-only opt-out** (`--sbom-source scan` covers this): if operators want a more granular flag like `--disable-referrers-api`, that's a follow-up ergonomic tweak.
- **Referrers-endpoint rate-limit handling** (`Retry-After` + exponential backoff on HTTP 429): m186 treats 429 as a fetch failure. Adaptive retry is a follow-up milestone with dedicated tests + configuration surface.
