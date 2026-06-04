# Feature Specification: Pluggable fingerprint corpus v2 (multi-indicator records + signed fetch + authenticated sources)

**Feature Branch**: `110-pluggable-corpus-v2`
**Created**: 2026-06-03
**Status**: Draft
**Input**: User description: "Replace milestone-108's library-only fingerprint corpus with a multi-indicator schema and a pluggable fetch protocol so the matcher can identify statically-linked C/C++ libraries at PURL-grade (library + version + ecosystem), with the corpus itself shipped from one or more configurable sources rather than baked into the binary."

## Overview / Context

The milestone-108 fingerprint corpus ships **7 libraries × 10 well-known public-API symbols** and emits `pkg:generic/<library>` — **library only, no version**. That ceiling prevents PURL-grade identification: an Nginx binary statically linked against OpenSSL 3.1.4 surfaces today as `pkg:generic/openssl`, which is non-actionable for downstream OSV / NVD / GHSA lookups.

This milestone delivers the **mechanism** to do better: a multi-indicator corpus record schema (symbols + version strings + Build-IDs + ABI markers + ecosystem aliases), a pluggable fetch protocol that allows mikebom to consume corpora from arbitrary configured sources, signature verification of fetched archives, and a confidence-fusion matcher that surfaces how trustworthy each identification is.

The **contents** of any specific corpus (which libraries, which versions, who curates them) are NOT specified here. The milestone-108 public corpus continues to ship as the OSS-default source. Any operator — or any vendor — can author additional corpora conforming to this milestone's schema and configure mikebom to fetch from them; this is a generic capability, not a hand-off to a specific provider. Authenticated sources are supported so corpora that aren't appropriate to distribute publicly (proprietary, customer-curated, embargoed-vulnerability-bound) can still flow through the same matcher.

This milestone scopes the FIRST shippable slice of a longer corpus-v2 design. Low-confidence emissions + operator triage layer, lazy-fetch sharding, source-tree copyright-header indicators, and Tier-2 build-from-source ingestion are deferred to follow-on milestones.

## Clarifications

### Session 2026-06-03

- Q: Confidence floor — what does the matcher do with matches below "medium"? → A: Suppress emission entirely; below-medium matches do NOT produce fingerprint-derived components. The binary surfaces at the pre-milestone-108 file-SHA-256 baseline for those cases. Aligns with the milestone's deferral of the low-confidence operator-triage layer to a follow-on milestone.
- Q: Cache freshness — when does mikebom re-fetch a corpus archive? → A: 24-hour TTL on the local cache directory, keyed on the pinned content SHA (matches milestone-108 semantics). Re-fetch on next scan after 24h; otherwise reuse. `mikebom fingerprints fetch --force` bypasses for ad-hoc refresh.
- Q: v1 backward-compat semantics — how does the v2 matcher confidence-tag milestone-108 records? → A: Map v1 matches to a fixed numeric `confidence: 0.70` (matching the design doc §7 "threshold-met exported symbols" baseline). v1 records emit as today; the only delta to the OSS-default SBOM is the addition of a `mikebom:confidence: "0.70"` annotation on existing fingerprint-derived components — the component list itself is unchanged. (Originally proposed as a bucket-name annotation `"medium"`; revised on 2026-06-03 during implementation to emit the numeric value, which is lossless + matches the CDX-native `evidence.identity[0].confidence` carrier.) SC-002 byte-identity is therefore re-anchored: the post-milestone-110 baseline INCLUDES the new numeric annotation, and the regression check compares against that new baseline, not the pre-milestone-110 one.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Versioned PURL emitted for a statically-linked library on a real binary (Priority: P1)

A security analyst points mikebom at a statically-linked Linux binary (e.g., a stripped Nginx that embeds OpenSSL). When a corpus source containing a record for the embedded library version is configured, mikebom emits a binary-tier component whose PURL is the actual upstream library and version present — `pkg:github/openssl/openssl@openssl-3.1.4` or `pkg:deb/debian/libssl3@3.1.4-1` — instead of the milestone-108 `pkg:generic/openssl` floor.

**Why this priority**: This is the entire reason corpus v2 exists. If versioned identification doesn't work on the target use case (statically-linked C/C++ binaries from real-world distros), nothing else in this milestone matters. Every other story is in service of making this one usable.

**Independent Test**: Run `mikebom sbom scan --path nginx-binary --fingerprints-corpus` against a fixture binary built from a known OpenSSL version, with a corpus source configured that carries the appropriate v2 record. The emitted CDX SBOM contains a component whose `purl` field carries the correct major + minor version (e.g., `pkg:github/openssl/openssl@openssl-3.1.4`) and whose annotations include `mikebom:identification-confidence: "high"` plus the matched indicator types (e.g., `mikebom:indicators-matched: "exported_symbols+version_string"`).

**Acceptance Scenarios**:

1. **Given** a stripped binary statically linked against OpenSSL 3.1.4 + a configured corpus source containing the corresponding v2 record, **When** the operator runs `mikebom sbom scan --fingerprints-corpus`, **Then** the emitted SBOM contains a component with PURL matching `pkg:github/openssl/openssl@openssl-3.1.4` (or a canonical ecosystem PURL with that version) and confidence annotation = "high".
2. **Given** the same binary but with only the milestone-108 public corpus configured (the OSS default — no extra sources), **When** the operator runs the same scan, **Then** the emitted SBOM contains a component with PURL `pkg:generic/openssl` (current milestone-108 behavior — no regression).
3. **Given** a corpus record that supplies multiple equivalent PURLs for the same library version, **When** the matcher emits the component, **Then** the most-specific PURL is used as the primary identifier and the equivalents surface as a `mikebom:purl-aliases` annotation.

---

### User Story 2 - Pluggable corpus sources with signed-fetch + optional auth + public fallback (Priority: P1)

An operator configures one or more corpus sources via mikebom's standard configuration surface (config file or environment variables). At scan time, mikebom fetches the configured archives, verifies each one's cryptographic signature, caches them locally, and feeds the union of their records into the matcher. Sources can be public (no auth) or authenticated (operator-supplied credential). When configured sources are unreachable, the scan continues using the milestone-108 public default — never a hard failure.

**Why this priority**: Without a working fetch + verify + cache + multi-source story, the corpus contents (story 1) cannot reach operators in any production-usable form. The "pluggable" + "auth-optional" properties are also what keeps this an OSS capability rather than a vendor-tied feature: any operator or any vendor can stand up a corpus source against the spec without changes to mikebom. Co-P1 with story 1 because they are mutually load-bearing.

**Independent Test**: Configure two corpus sources (one public, one fixture-private-with-auth). Run `mikebom fingerprints fetch` → both archives are fetched, signatures verified, contents merged into the cache. Re-run with the private source's credentials removed → fetch falls back to the public source with a single info-level log line and the scan still completes. Bad credentials on the private source → fetch of THAT source fails with a clear actionable error; the public source still loads and the scan succeeds.

**Acceptance Scenarios**:

1. **Given** an operator configures a corpus source URL + (where required) an auth credential supplied via environment variable or config file, **When** mikebom fetches the corpus during scan startup, **Then** the archive is downloaded, the cosign signature on the archive is verified, the contents are unpacked into the local cache directory, and the operation completes with zero warnings.
2. **Given** a configured source whose credential is missing or invalid, **When** the fetch is attempted, **Then** the operation logs a clear non-cryptic warning identifying the source URL + the credential source the operator should check, the matcher continues with whatever other sources DID load successfully, and the scan completes.
3. **Given** ALL configured sources fail (network outage, signature mismatch, malformed archive), **When** the scan runs, **Then** the matcher operates with no fingerprint corpus loaded (binary-tier components emit at the pre-milestone-108 file-SHA-256 baseline) and the scan still exits with success status — the operator's CI is not broken by transient corpus-source outages.
4. **Given** a valid corpus archive in the local cache and a subsequent scan within the 24-hour TTL, **When** the operator runs a scan, **Then** no network fetch occurs; the cached corpus is used directly.
5. **Given** an operator configures NO additional corpus sources, **When** they run a scan with `--fingerprints-corpus`, **Then** mikebom uses ONLY the milestone-108 public corpus (the OSS default) and behavior is identical to milestone 108.

---

### User Story 3 - No regression for milestone-108 consumers (Priority: P1)

An open-source contributor or existing milestone-108 user — no corpus-source configuration, no auth credentials, default install — scans a binary today and gets `pkg:generic/openssl` from the milestone-108 corpus. After this milestone ships, the same workflow continues to produce `pkg:generic/openssl` with the same indicator set. No behavior change, no missing dependency, no broken CI on the consumer's side.

**Why this priority**: A regression that silently breaks the milestone-108 contract erodes trust in mikebom's open-source posture and the Constitution principle of supply-chain transparency. Treating this as co-P1 with stories 1+2 enforces the discipline that the new capability is co-validated with the existing one, not "ship pluggable + audit the default path later."

**Independent Test**: A CI lane runs `mikebom sbom scan` against the milestone-108 reference fixture (OpenSSL.so + the existing 6 library binaries) with no extra corpus sources configured and no auth credentials. Output is byte-identical to the pre-milestone-110 baseline; if not, that lane fails CI and blocks the merge.

**Acceptance Scenarios**:

1. **Given** mikebom installed from a public source with no extra corpus configuration and no credentials, **When** the operator runs `mikebom sbom scan --fingerprints-corpus`, **Then** the scan uses the milestone-108 public corpus (downloaded from the existing sibling repo), emits components with the same `pkg:generic/<name>` PURLs as before, and exits successfully.
2. **Given** the public milestone-108 reference test fixture from milestone 108's e2e suite, **When** the same test runs after this milestone, **Then** the emitted SBOM's component list is identical to the milestone-108 baseline modulo the permitted addition of `mikebom:identification-confidence: "medium"` annotations on existing fingerprint-derived components (canonicalization + SHA-256 comparison against the re-anchored golden).
3. **Given** a milestone-108 v1-shape record fetched from the public corpus, **When** the v2 matcher attempts to load it, **Then** the v1 record loads successfully via a compatibility path (no migration step required of the public-corpus maintainers).

---

### User Story 4 - Multi-indicator confidence fusion (Priority: P2)

When mikebom matches MULTIPLE indicator types within the same corpus record (e.g., exported symbols + embedded version string + ELF Build-ID all agree on OpenSSL 3.1.4), the emitted component's confidence annotation reflects the agreement. When only ONE indicator type matches, confidence is lower. When two different records both match a single binary at non-trivial confidence (the BoringSSL-vs-OpenSSL collision case), both components emit with cross-references rather than silent collapse.

**Why this priority**: Without fusion, the milestone gives operators no machine-readable signal of how trustworthy any individual identification is. With it, downstream automation (CI gates, alerting policies, deny-lists) can treat `confidence: "high"` differently from `confidence: "medium"`. P2 because the MVP could ship with a single-indicator matcher (matching milestone-108's behavior) and add fusion as a polish layer — but it should land in this milestone, not slip, because the v2 records carry per-indicator confidence baselines that don't deliver value without a consumer.

**Independent Test**: Run the matcher against a test binary that matches exactly ONE indicator of a corpus record (e.g., 8 of 10 zlib symbols, no version string) → emitted component has `mikebom:identification-confidence: "medium"`. Same matcher against a binary that matches THREE indicators of the same record (symbols + version string + Build-ID) → emitted component has confidence "high". Same matcher against a binary that matches indicators from TWO different records → both records emit as components, each with `mikebom:also-detected-via` cross-referencing the other.

**Acceptance Scenarios**:

1. **Given** a corpus v2 record with three indicator types defined and a binary that matches all three, **When** the matcher runs, **Then** the emitted component carries `mikebom:identification-confidence` = "high" and `mikebom:indicators-matched` lists all three indicator names.
2. **Given** the same record and a binary matching only the weakest indicator above its individual threshold, **When** the matcher runs, **Then** the emitted component carries confidence "medium" and `indicators-matched` lists only that one indicator.
3. **Given** two records (e.g., OpenSSL and BoringSSL) sharing overlapping exported symbols, and a binary matching exported symbols of both records, **When** the matcher runs, **Then** TWO components are emitted, each with `mikebom:also-detected-via` cross-referencing the other and a clear confidence ordering visible in their annotations.
4. **Given** a corpus record whose `purl` resolves to the scanned project's own identity (from cmake `project()`, cargo `[package].name`, npm `name`, or `--scan-as` operator override), **When** the matcher would otherwise emit a third-party-dep component for that record, **Then** the matcher suppresses the emission per the design-doc self-identification rule (no "openssl-contains-openssl" duplicate components).

---

### Edge Cases

- **Configured source unreachable**: log a clear warning naming the source URL + the network failure category, continue with whichever other sources DID load (including the milestone-108 default).
- **Signature verification fails**: reject the archive, delete partial cache state, log a warning, do NOT trust any of the rejected archive's records. Other sources are unaffected.
- **Single malformed record within an otherwise-valid archive**: skip that record (log a warning naming the record ID + the validation failure) but load the remaining valid records from the same archive.
- **Cache directory not writable** (e.g., read-only filesystem like a Docker container): emit a clear error, fall back to in-memory caching for the duration of the scan, do NOT silently fail.
- **Self-identification scan** (operator scans the source tree of a library that ALSO has a corpus record): the matcher's self-identification suppression rule fires; the manifest-tier emission (from cmake `project()` etc.) is what surfaces, not a spurious "library contains itself" duplicate.
- **Two corpus sources contain conflicting records for the same library + version**: the matcher emits both candidates via the multi-record cross-reference path (story 4) — no silent dedup that could mask a misconfigured source.
- **A configured source uses a v1-shape archive** (milestone-108 layout): the reader's compatibility path loads it transparently; operators do NOT need to upgrade their existing sources.
- **Record's `version_range` is wider than the operator's expected pin** (e.g., one record covers OpenSSL 3.1.x; binary is specifically 3.1.4): emit the most-specific PURL from the record and surface the range in `mikebom:identification-version-range` so consumers see both the specific claim and the full match range.
- **`--fingerprints-corpus` not passed by the operator** (the milestone-108 opt-in pattern): no corpus is loaded at all, no fetch occurs, binary-tier identification emits at the pre-fingerprint baseline. Opt-in behavior is unchanged from milestone 108.

## Requirements *(mandatory)*

### Functional Requirements

**Corpus schema (v2)**

- **FR-001**: A corpus v2 record MUST conform to a versioned JSON Schema that requires: a canonical `purl`, a `version_range`, at least one indicator block with a typed structure, a `provenance` block with `extracted_from` (source URL + content hash), and a `schema_version` field set to 2.
- **FR-002**: A corpus v2 record MUST support multiple indicator types in one record (exported symbols, embedded version strings, ELF Build-ID, Mach-O LC_UUID, PE PDB GUID, ABI markers, source-tree copyright-header patterns where applicable), each with its own confidence baseline.
- **FR-003**: A corpus v2 record MUST support `purl_aliases` (cross-ecosystem identifiers — upstream-source / deb / rpm / apk / vcpkg / Conan) and `cpe_candidates` so a single record can describe the same library across ecosystem identifier conventions.
- **FR-004**: The v2 schema MUST be public — published in the mikebom repository and at a stable operator-facing URL — stable enough that third-party corpus authors (any operator, any vendor) can produce conforming records without coordination with mikebom maintainers. *(NOTE: an earlier draft of this FR additionally mandated "≥10 distinct C/C++ libraries × ≥2 versions in the initial seed". That mandate has been removed in remediation per the 2026-06-03 /speckit-analyze report finding F1: corpus contents are out-of-scope per § Assumptions — the corpus author, not mikebom-cli, owns content authoring. The schema-publication requirement remains; the content requirement is dropped.)*
- **FR-005**: The v2 reader in mikebom-cli MUST be backward-compatible with the milestone-108 v1 record shape so existing v1 corpora (including the public milestone-108 corpus itself) continue to load and match without breaking change. v1 records MUST be treated as single-indicator (exported-symbols) records with a fixed `confidence_baseline: 0.70` mapping to the `medium` confidence bucket. The set of components emitted from a v1 corpus MUST be identical to the milestone-108 behavior; the ONLY permitted delta in the emitted SBOM is the addition of the `mikebom:identification-confidence: "medium"` annotation on each fingerprint-derived component.

**Pluggable fetch + auth + fallback**

- **FR-006**: Mikebom-cli MUST support fetching the corpus from one or more configurable source URLs declared via a standard configuration mechanism (config file, environment variables, or both — choice deferred to plan phase). Each source is configured independently and may include an optional auth credential reference.
- **FR-007**: Mikebom-cli MUST support an opaque auth credential per source, supplied via the standard mikebom configuration surface and forwarded to the source's fetch endpoint via a documented protocol (bearer token in HTTP `Authorization` header is the default; other schemes a plan-phase decision if the user case demands).
- **FR-008**: Mikebom-cli MUST verify the cryptographic signature on each fetched corpus archive (cosign keyless OIDC, consistent with the milestone-108 signature scheme) before trusting its contents; signature failures MUST cause the offending archive to be rejected and removed from cache.
- **FR-009**: Mikebom-cli MUST NOT bake any specific corpus's contents into the released binary; corpora are fetched at runtime and held only in the local cache directory.
- **FR-010**: When ANY configured source is unreachable (network failure, auth failure, signature mismatch, malformed archive), mikebom-cli MUST log a warning, continue with the remaining successfully-loaded sources, and complete the scan successfully.
- **FR-011**: When NO sources load (all configured sources fail AND the milestone-108 default also unreachable), mikebom-cli MUST complete scans without crashing — binary-tier components emit at the pre-milestone-108 file-SHA-256 baseline.
- **FR-012**: The milestone-108 public corpus URL + signature scheme MUST remain the OSS-default source loaded when no additional sources are configured; existing milestone-108 users see no behavioral change.
- **FR-012a**: Each configured corpus source's local cache MUST honor a 24-hour TTL keyed on the pinned content SHA (matching milestone-108 semantics). Within the TTL window, mikebom MUST reuse the cached archive without network I/O; after the TTL expires, the next scan MUST re-fetch. A `mikebom fingerprints fetch --force` invocation MUST bypass the TTL for ad-hoc refresh.

**Multi-indicator matcher**

- **FR-013**: When two or more indicator types within a single corpus record match the same binary, the matcher MUST emit ONE component with `mikebom:identification-confidence` reflecting the fused confidence per the documented fusion rule and `mikebom:indicators-matched` listing each contributing indicator type.
- **FR-014**: When indicators from two different corpus records both match the same binary at non-suppressed confidence levels, the matcher MUST emit BOTH components, each with `mikebom:also-detected-via` cross-referencing the other (no silent collapse).
- **FR-015**: The matcher MUST honor self-identification suppression: a corpus record whose `purl` resolves to the scanned project's own identity (from cmake `project()`, cargo `[package].name`, npm `name`, or an operator-supplied `--scan-as` override) MUST NOT emit as a third-party dep of itself.
- **FR-016**: The matcher MUST emit the most-specific PURL it can derive from the matched record's `purl` field, AND surface alias PURLs via the `mikebom:purl-aliases` annotation.
- **FR-017**: Each fingerprint-derived component MUST carry a `mikebom:fingerprint-confidence` annotation whose value is the **numeric** fused-confidence score (a string formatted as "X.XX", e.g., `"0.70"` / `"0.85"` / `"0.99"`) — lossless, matches the CDX-native `evidence.identity[0].confidence` numeric carrier, and lets downstream automation compute its own bucket thresholds without re-deriving from a coarser label. The annotation name is `mikebom:fingerprint-confidence` (NOT the existing `mikebom:confidence` C16 enum-string carrier whose value is fixed at `"heuristic"`); the two are distinct carriers per the principle-V audit. Numeric emission is derived from per-indicator confidence baselines via a documented and version-stable fusion rule (the "max + bump" algorithm; see `research.md` R2 + `plan.md`). The matcher MUST emit components ONLY when fused confidence is ≥ 0.70 (the equivalent of the `medium` floor); matches below 0.70 MUST NOT produce a fingerprint-derived component (the binary surfaces at the pre-milestone-108 file-SHA-256 baseline for those cases) per the 2026-06-03 Q1 clarification. The `low` confidence tier + its operator-triage surface are reserved for a follow-on milestone.

**OSS continuity + extensibility**

- **FR-018**: The public milestone-108 corpus repository and its existing fetch contract MUST continue to operate unchanged; this milestone MUST NOT modify the milestone-108 public records, fetch URLs, or signature scheme.
- **FR-019**: A CI lane MUST exist that runs the full mikebom integration test suite using ONLY the public milestone-108 corpus (no extra sources configured, no auth credentials in the environment), ensuring no regression in OSS user behavior.
- **FR-020**: The fetch protocol — URL format, signature verification, auth header convention, archive structure, cache layout — MUST be documented in the mikebom repository at a stable URL so independent operators or vendors can author conformant corpus sources.

### Key Entities

- **Corpus record (v2)**: a single (library, version-range, ABI) tuple description containing canonical PURL + alias PURLs + CPE candidates + per-indicator-type match specifications + provenance metadata. Stored as JSON; validated against `corpus-record-v2.schema.json` published in the mikebom repository.
- **Corpus archive**: the packaged set of corpus records distributed to mikebom-cli consumers. Tar-gzipped; cosign-signed; cached locally per host. May be produced by any corpus author — the milestone-108 public archive is one example, and operators may configure additional archives from other sources.
- **Corpus source**: an addressable origin (URL) that mikebom-cli fetches a corpus archive from, optionally protected by an auth credential. Multiple sources may be configured concurrently; the matcher merges their records.
- **Indicator extractor**: an existing mikebom extractor (milestones 099 / 108 / 098 / 026 / 023 / 024 / 028) that reads a specific file format (ELF / Mach-O / PE / source tree) and outputs an indicator value for matching against a corpus record. The extractors themselves are unchanged by this milestone; the schema describes how their outputs compose into records and how the matcher fuses results.
- **Fingerprint corpus cache**: the per-host on-disk cache directory (extending milestone 108's `~/.cache/mikebom/fingerprints/<sha>/` pattern) holding fetched corpus archives. Multiple cached corpora live in distinct subdirectories so operators with access to multiple sources can switch between them without re-fetch.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: When scanning the milestone's fixture binaries against the fixture corpus authored by the test suite, mikebom emits a versioned upstream PURL (not `pkg:generic/...`) with `mikebom:confidence: "high"` for every binary whose fixture record carries ≥ 2 indicators (the openssl 3.1.4 + boringssl fixtures both qualify). Measured by the `fingerprints_v2_match` + `fingerprints_v2_fusion` integration tests; the milestone's fixture surface intentionally bounds the assertion to the libraries the test corpus authors. Broader-roster verification (e.g., 10+ libraries × 2+ versions) is corpus-author responsibility and out of scope per § Assumptions.
- **SC-002**: The OSS regression CI lane (milestone-108 public corpus only, no extra sources, no auth) emits an SBOM whose component list is identical (set equality on the `components[]` array, ignoring the new `mikebom:identification-confidence: "medium"` annotation per FR-005) to the pre-milestone-110 baseline — measured by JSON canonicalization + SHA-256 comparison against a re-anchored golden that incorporates the single permitted annotation delta.
- **SC-003**: An operator configuring a new corpus source for the first time can complete the entire fetch-verify-cache-scan pipeline in under 30 seconds end-to-end on a typical broadband connection for archives ≤ 5 MB — measured by an end-to-end fixture test.
- **SC-004**: After the first scan with a given corpus source, subsequent scans within the 24-hour cache TTL perform zero network I/O against that source — verifiable by observing the network egress (or absence) on a follow-up scan within the window.
- **SC-005**: When a configured source fails (auth, network, signature, malformed archive), the operator sees an actionable warning naming the failure category (one of: missing-credential / invalid-credential / network-unreachable / signature-mismatch / archive-malformed) and the scan still completes — measured by inducing each failure mode and confirming the user-facing message + exit status.
- **SC-006**: When the matcher encounters a binary that matches indicators from multiple corpus records (the BoringSSL-vs-OpenSSL collision case), it emits both components with cross-references — measured by an end-to-end test asserting both PURLs appear in the SBOM and each carries `also-detected-via` pointing at the other.
- **SC-007**: A third-party corpus author following the documented fetch protocol + schema can stand up a conformant corpus source and have mikebom consume it without changes to mikebom-cli — measured by an integration test where the test harness hosts a fixture corpus source on a local port and mikebom is configured to consume it.
- **SC-008**: 100% of records in a conformant corpus archive carry valid provenance metadata (`provenance.extracted_from` URL parses, `provenance.extracted_from_sha256` is 64-hex-char) — verified at corpus-archive load time by the v2 loader's deserialization-strict-shape gate (FR-001 / data-model.md `#[serde(deny_unknown_fields)]` + newtype constructors). A live re-fetch + content-hash re-validation subcommand (`mikebom fingerprints verify`) is OUT of scope for this milestone and a tracked forward-pointer for a follow-on; the load-time validation suffices for milestone-110 acceptance.

## Assumptions

- The milestone-108 cosign signature scheme (keyless OIDC against a GitHub Actions identity) extends to additional corpus archives unchanged; signing key management is not a new problem this milestone has to solve.
- The matcher's per-indicator confidence baselines + fusion rule are derived from the design captured in `/Users/mlieberman/Projects/mikebom-design-notes/corpus-v2-symbols-to-purls.md` (private gist `6d2bde7965e67ffa3123d0a5d23ae034`); the public spec records the rule + baselines in the implementation but does not include the design rationale.
- Tier 1 ingestion (the corpus-author-side machinery that produces records from package sources), Tier 2 (build-from-source), and Tier 3 (manual curation) are NOT part of this milestone — they live in corpus-author tooling that the schema + fetch protocol enable but do not include. This milestone delivers the consumer side and the schema; corpus authors (whoever they are) consume the schema and produce records via their own pipelines.
- Source-tree copyright-header indicators (a weak indicator type in the broader design) are **out of scope** for this milestone. Binary-side indicators (symbols + version strings + Build-IDs) suffice for the initial PURL-grade outcome. Source-tree indicators are a follow-on.
- The "low confidence + operator triage" layer (`mikebom-overrides.yaml`, `mikebom corpus contribute`, indicator-bag annotations on below-threshold matches) is **out of scope** for this milestone. The matcher emits matches at `high` or `medium` confidence only; below-medium matches are suppressed entirely and the binary surfaces at the pre-milestone-108 file-SHA-256 baseline for those cases (see FR-017 + the 2026-06-03 clarification). Triage paths ship in a follow-on milestone.
- Lazy-fetch sharding is **out of scope**. Bundled full-archive fetch (the milestone-108 pattern) suffices at the < 100-library scale this milestone targets; sharding becomes necessary later if/when consumer-facing archives grow into the multi-hundred-MB range.
- Function-body hashing remains a research-stage indicator type and is **out of scope** at v2.
- The constitution principle of supply-chain transparency requires that the schema, fetch protocol, signature verification, matcher algorithm, and confidence fusion rule all live in the public mikebom-cli source so any consumer can audit how their data is being identified — only the CONTENTS of any specific corpus may be private (per its author's choice); the MECHANISM is OSS.
- Authentication-related configuration follows existing mikebom conventions (env-var-first, config-file alternative) and does not require introducing a new secrets-management framework. The credential is treated as opaque by mikebom — semantically it grants read access to a configured source; auth schemes more elaborate than "bearer token in HTTP header" are deferred to plan-phase research if needed.
