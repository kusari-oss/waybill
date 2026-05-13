# Feature Specification: CPE candidate emission for binary-identified components

**Feature Branch**: `097-cpe-candidates`
**Created**: 2026-05-12
**Status**: Draft
**Input**: User description: "milestone 097 — emit CPE candidates alongside the binary-extracted `pkg:generic/<lib>@<version>` components from milestone 096, so downstream vulnerability scanners can match without their own PURL-to-CPE mapping table"

## Background

Milestone 096 enriched mikebom's binary scanner to extract embedded version strings (11 libraries) and ELF symbol fingerprints (3 libraries) — both surface as `pkg:generic/<library>@<version>` components in the emitted SBOM. The PURL is sufficient identity for any tool that can map `pkg:generic/openssl@3.0.13` to its actual NVD records. **In practice, most CVE-driven scanners can't**.

The dominant vulnerability matchers (Trivy, Grype, OWASP Dependency-Track, MSR-style enterprise scanners) drive their advisory lookup off **CPE 2.3 strings** — the NVD-defined platform-applicability identifier. A component with no CPE attached is effectively invisible to these scanners for advisories that key off the NVD CVE feed. For binaries with package-db-sourced components (dpkg/rpm/apk), the OS package metadata already carries enough information for downstream tools to construct a CPE. But for the *binary-extracted* `pkg:generic/openssl@3.0.13` from milestone 096 — the stripped third-party blob's static-link content — there is no package-db row, no maintainer-supplied CPE, and the operator's downstream scanner gets nothing.

The fix: mikebom emits the CPE candidate **at SBOM-generation time**, using an in-source `(library → vendor, product)` mapping table indexed by the lowercase library slug that the milestone-096 scanners already produce. CDX 1.6 has a native `component.cpe` field; SPDX 2.3 has `externalRefs[].referenceType = "cpe23Type"`; SPDX 3 has `Software:cpe`. All three are first-class spec fields — no `mikebom:*` annotation needed, satisfying Constitution V (standards-native precedence).

**Scope framing**: this milestone is deliberately narrow — *the v1 starter set is the same 11 libraries the embedded-version-string scanner already covers*. Operators who need broader coverage can extend the table in-source; new libraries land in follow-up milestones as the scanner backlog evolves. Composite-evidence components (where both version-string + symbol-fingerprint matched per milestone-096 Q1 clarification) inherit the version-string PURL's CPE candidate.

**What this is NOT**: this is not CPE-name *matching* (looking up an unknown PURL against an external CPE database). This is candidate *generation* — converting an already-identified library's `pkg:generic/...` PURL into the CPE 2.3 string an NVD-driven scanner expects. The mapping is small (a handful of `vendor:product` pairs per library), maintainable in mikebom's source tree, and deterministic.

Out of scope: external CPE-database lookups (NVD APIs, deps.dev), CPE-to-CPE alias resolution, package-db-sourced components (RPM/DEB/APK readers already produce CPEs via their distro metadata where applicable), version normalization beyond what the spec already requires.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator runs Trivy against a mikebom SBOM and CVEs land (Priority: P1)

An operator scans a stripped binary with mikebom; the resulting CDX SBOM contains `pkg:generic/openssl@3.0.13` from the milestone-096 embedded-version-string scanner. They feed the SBOM to Trivy (or Grype, or Dependency-Track) for vulnerability matching. Today: zero CVE hits — the scanner has no CPE to drive its NVD lookup against. With this milestone: the same component carries `cpe:2.3:a:openssl:openssl:3.0.13:*:*:*:*:*:*:*` as a first-class CDX `component.cpe` field, and Trivy/Grype/DT match the OpenSSL 3.0.13 CVEs directly.

**Why this priority**: this is the headline user-visible payoff of milestone 096. Without CPE emission, the static-link identification work from milestone 096 doesn't translate into downstream CVE visibility for the dominant scanner ecosystem. Operators see PURLs in the SBOM but no advisories on their dashboards.

**Independent Test**: emit an SBOM containing a synthetic `pkg:generic/openssl@3.0.13` component (or run the milestone-096 openssl-static control test); inspect the CDX JSON; confirm `components[?(@.purl=='pkg:generic/openssl@3.0.13')].cpe == 'cpe:2.3:a:openssl:openssl:3.0.13:*:*:*:*:*:*:*'`. Feed the SBOM to a current Trivy/Grype binary and confirm the OpenSSL 3.0.13 CVE list appears in the scanner output.

**Acceptance Scenarios**:

1. **Given** a CDX SBOM containing `pkg:generic/openssl@3.0.13` from the milestone-096 embedded-version-string scanner, **When** mikebom emits the SBOM, **Then** the same component carries `cpe = cpe:2.3:a:openssl:openssl:3.0.13:*:*:*:*:*:*:*` (CDX 1.6 native `component.cpe` field).
2. **Given** the same component, **When** mikebom emits the SBOM as SPDX 2.3, **Then** the Package carries `externalRefs[]` containing `{ referenceCategory: "SECURITY", referenceType: "cpe23Type", referenceLocator: "cpe:2.3:a:openssl:openssl:3.0.13:*:*:*:*:*:*:*" }`.
3. **Given** the same component, **When** mikebom emits the SBOM as SPDX 3, **Then** the `Software/Package` element carries `cpe = ["cpe:2.3:a:openssl:openssl:3.0.13:*:*:*:*:*:*:*"]` (or whatever SPDX 3's canonical `cpe` field name is per the milestone-079 vocab inventory).
4. **Given** a binary triggering BOTH version-string AND symbol-fingerprint matches for OpenSSL on the same scan (composite-evidence per milestone-096 Q1), **When** mikebom emits the SBOM, **Then** the merged component still carries exactly ONE CPE candidate — the version-string-derived one. No double-emission, no second CPE for the symbol-fingerprint half of the evidence trail.

---

### User Story 2 — Maintainer extends the CPE vendor/product table when adding a new library (Priority: P2)

A maintainer adds `libpng` to a future milestone's version-string scanner. They open the CPE mapping table in `mikebom-cli/src/generate/cpe_candidates.rs` (or equivalent), see a clean `const CPE_MAPPINGS: &[(library_slug, vendor, product)]` table mirroring the version-string `CuratedLibrary::slug()` enum, and add `("libpng", "libpng", "libpng")` in one line. Tests verify the emission shape for the new library.

**Why this priority**: P2 because the v1 starter set covers the libraries the scanner can already identify; the maintainer-friendly extension story is what keeps this from becoming a write-once frozen artifact. Future milestones extend the version-string scanner OR the symbol-fingerprint scanner; the CPE table needs to track both surfaces.

**Independent Test**: add a row to the CPE mapping table for a synthetic test library, build, run a unit test that calls the candidate-builder with a `pkg:generic/test-lib@1.2.3` PURL, confirm the produced CPE 2.3 string matches `cpe:2.3:a:test-lib:test-lib:1.2.3:*:*:*:*:*:*:*` (or whatever the table's vendor/product values were).

**Acceptance Scenarios**:

1. **Given** the CPE mapping table file, **When** a maintainer reads it, **Then** the table is a single in-source `const` with `(library_slug, vendor, product)` triples — no scattered per-library constants, no JSON config file, no DB. Sorted alphabetically by `library_slug` for diff-friendliness.
2. **Given** a maintainer adds one row to the table, **When** they recompile + run the unit-test suite, **Then** the new library produces a valid CPE 2.3 string on a synthetic `pkg:generic/<slug>@<version>` PURL — no further wiring required.
3. **Given** the table is missing a row for some library, **When** mikebom emits an SBOM containing that library's `pkg:generic/...` component, **Then** the component emits WITHOUT a CPE (silent skip), no SBOM-validation error, no crash. The operator sees the PURL but no CPE; this is an explicit "we don't know" rather than a guess.

---

### User Story 3 — Operator scans a symbol-fingerprint-only binary and the SBOM withholds the CPE (Priority: P2)

An operator scans a binary that exports OpenSSL's symbol set but doesn't carry the embedded version string. The milestone-096 symbol-fingerprint scanner emits `pkg:generic/openssl` (no `@<version>` segment) with `mikebom:evidence-kind = symbol-fingerprint` and `confidence = heuristic`. Mikebom does NOT emit a CPE for this component — a wildcard-version CPE (`cpe:2.3:a:openssl:openssl:*:*:*:*:*:*:*:*`) would match *every* OpenSSL CVE ever filed, drowning downstream scanners in false positives that the operator can neither verify nor dismiss.

**Why this priority**: P2 because it's a transparency-and-safety policy decision: emitting wildcard CPEs for symbol-only matches would degrade the operator's signal-to-noise ratio more than helping. Better to surface the PURL without CPE and let the operator make a manual call on whether to investigate further.

**Independent Test**: build a test binary with OpenSSL symbols but no version string (or use the milestone-096 `openssl-symbols-only` fixture); scan; inspect the emitted SBOM; confirm the `pkg:generic/openssl` component has NO `cpe` field on its CDX entry, and NO `cpe23Type` `externalRefs` entry on its SPDX 2.3 Package.

**Acceptance Scenarios**:

1. **Given** a component with PURL shape `pkg:generic/<lib>` (no `@<version>` segment), **When** mikebom emits the SBOM, **Then** the component carries no `cpe` field (CDX), no `cpe23Type` external ref (SPDX 2.3), no `cpe` array entry (SPDX 3).
2. **Given** the same operator inspects the SBOM, **When** they look for the OpenSSL component, **Then** they see the `pkg:generic/openssl` PURL, the `mikebom:evidence-kind = symbol-fingerprint` property, and the `mikebom:fingerprint-symbols-matched = 9/10` annotation — but no CPE. The transparency is: "we identified the library family, not the version; CVE matching requires a version pin".

---

### Edge Cases

- **Library with no NVD-tracked CPE** (e.g., a niche library that NVD has never advisory'd): the mapping table simply omits that row. Component emits with PURL but no CPE. Same code path as US2 acceptance scenario 3.
- **Version-letter suffix** (OpenSSL `1.1.1w`): CPE 2.3 accepts the version string verbatim — `cpe:2.3:a:openssl:openssl:1.1.1w:*:*:*:*:*:*:*` is valid. No special-case translation needed.
- **Version with NVD-internal modification format** (e.g., NVD sometimes splits `1.1.1w` into `1.1.1:w` for update qualifier): out of scope for v1. Mikebom emits the raw version triplet; downstream scanners apply their own version-matching heuristics. Document the limitation in research.
- **OpenJDK version with build suffix** (`21.0.1+12`): CPE 2.3 doesn't natively encode `+`; some NVD records use `update_12`, others put it in the `update` field. v1 emits the raw `21.0.1` (drops build suffix for CPE; full version stays on the PURL). Conservative: less false-negative risk than emitting a CPE that doesn't match any NVD record.
- **BoringSSL fingerprint** (no NVD-tracked CPE — BoringSSL has CVE history but Google doesn't get standalone CPEs; vulnerabilities flow through `openssl:openssl`): omit BoringSSL from the v1 table. Document the gap.
- **PCRE vs PCRE2**: two separate NVD records — `pcre:pcre` for PCRE 8.x, `pcre:pcre2` for PCRE 10.x. The version-string scanner already disambiguates; the CPE table picks the matching vendor/product pair for each.
- **LibreSSL conflict** (`libressl:libressl` exists, but most CVEs are filed against `openbsd:libressl`): primary vendor is `openbsd:libressl` (the more-cited NVD shape — LibreSSL is an OpenBSD project and most advisory records use that namespace). Secondary candidate `libressl:libressl` is emitted via the existing `mikebom:cpe-candidates` property so fuzzy NVD matchers (Trivy, Grype) can take the union.
- **LLVM** (compiler infrastructure, multiple sub-projects): NVD typically uses `llvm:llvm` for the umbrella project. Sub-component CVEs (`llvm:clang`, `llvm:lld`) are out of scope for v1 — the version-string scanner doesn't distinguish them anyway.
- **SQLite encoding quirks**: NVD uses `sqlite:sqlite`. SQLITE_SOURCE_ID hashes (the milestone-096 fallback when version triplet isn't extractable) don't map to NVD records — omit CPE when only the source-id is captured.
- **Composite evidence inheritance**: the milestone-096 Q1 merge produces one component with two evidence-identity entries. The CPE inherits from the version-string-side PURL (which has `@<version>`), not the symbol-fingerprint side (which doesn't). Verified by US1 acceptance scenario 4.
- **Malformed CPE syntax fail-soft**: if the candidate-builder produces a syntactically-invalid CPE 2.3 string (e.g., a version containing whitespace or unescaped reserved character — should be impossible given the existing version-string scanner's anchors, but defensive), the component emits without a CPE rather than with a corrupt one.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: For each `pkg:generic/<library>@<version>` component in the emitted SBOM where `<library>` matches a row in the v1 CPE-mapping table, mikebom MUST emit `cpe:2.3:a:<vendor>:<product>:<version>:*:*:*:*:*:*:*` on the corresponding CDX `component.cpe` field, SPDX 2.3 `externalRefs[].referenceLocator` (with `referenceType = cpe23Type`, `referenceCategory = SECURITY`), and SPDX 3 `Software:cpe` (or the SPDX 3 IRI-vocab equivalent per milestone-079's IRI registry). v1 mapping table MUST cover **10 libraries** — OpenSSL, zlib, SQLite, curl, PCRE, PCRE2, GnuTLS, LibreSSL, LLVM, OpenJDK. The 11th library identified by the existing embedded-version-string scanner (BoringSSL) is omitted because it has no NVD-tracked CPE namespace; documented in Edge Cases. SQLite and OpenJDK are both in the table — only their version-handling edge cases (SQLite source-id-only suppression; OpenJDK build-suffix stripping) are covered separately per Edge Cases.
- **FR-002**: The CPE-mapping table MUST live as a single in-source `const` array of `(library_slug, vendor, product)` triples, sorted alphabetically by `library_slug` for diff-friendliness. Adding a library MUST require touching only that one table — no scattered per-library wiring elsewhere.
- **FR-003**: Components with no mapping-table entry (e.g., a maintainer adds version-string support for a library before adding its CPE row) MUST emit silently — the PURL appears, no CPE field, no error, no warning at INFO level. The "missing" state is explicit-by-omission; an operator who needs the CPE files a PR adding the row.
- **FR-004**: Components without a captured version (`pkg:generic/<library>` shape — the symbol-fingerprint-only case from milestone-096 FR-004 per Clarification Q1 inheritance + the SQLite source-id-only edge case) MUST NOT emit a CPE. Wildcard-version CPEs are explicitly out of scope per Edge Case rationale.
- **FR-005**: When a component carries BOTH milestone-096 evidence techniques (composite-evidence per Q1), the CPE is derived from the version-string PURL only — the version-pinned form. The symbol-fingerprint evidence does NOT add a second CPE. Composite components emit exactly ONE `cpe` field value.
- **FR-006**: Generated CPE 2.3 strings MUST be syntactically valid per the [CPE 2.3 specification](https://csrc.nist.gov/projects/security-content-automation-protocol/specifications/cpe). Vendor/product slugs are lowercase ASCII; version is the verbatim captured triplet (with OpenSSL letter-suffix preserved; OpenJDK build-suffix stripped per Edge Cases). Pre-emission validation MUST reject any malformed candidate (e.g., one containing whitespace or unescaped reserved characters) and emit no CPE in that case rather than a corrupt one — fail-soft to "no CPE", never fail-loud.
- **FR-007**: No new Cargo dependencies. CPE 2.3 emission is a pure-Rust string-formatting operation — no parser crate, no schema validator beyond a hand-rolled regex check. Constitution Principle I (zero new transitive crates per milestone unless justified) compliance.
- **FR-008**: Production code changes confined to `mikebom-cli/src/generate/` (the existing `cpe.rs` module which already emits CPE candidates for package-db-sourced components is the natural home — extend it). No changes to `scan_fs/binary/`, no changes to `mikebom-common/`, no changes to the parity-catalog.
- **FR-009**: Goldens may regenerate for the 11 v1 libraries if any existing fixture contains a binary that triggers a version-string match for those libraries. Per milestone-096 SC-007, the ≤1-spurious-match bound across the 9 existing ecosystem fixtures means the golden diff is bounded — likely zero or one component gaining a new `cpe` field.
- **FR-010**: The parity-catalog row(s) currently covering `component.cpe` (CDX) / `cpe23Type` external refs (SPDX 2.3) / `Software:cpe` (SPDX 3) MUST already exist OR be added in this milestone IF the milestone introduces a new format-parity surface. Audit at planning time per Constitution V's standards-native-precedence rule; this is a documented native field across all 3 formats so no `mikebom:*` annotation is needed.

### Key Entities

- **CPE mapping entry**: a `(library_slug, vendor, product)` triple. `library_slug` is the lowercase string emitted by `version_strings::CuratedLibrary::slug()` (or `symbol_fingerprint::SymbolFingerprintMatch::library`). `vendor` and `product` are the NVD-canonical CPE 2.3 component-name strings (lowercase, no spaces, hyphens preserved). One entry per supported library.
- **CPE candidate**: a CPE 2.3 string of the form `cpe:2.3:a:<vendor>:<product>:<version>:*:*:*:*:*:*:*`, produced at SBOM-generation time from a mapping entry plus a component's version field. Attached to the component via the format-native field (CDX `cpe`, SPDX 2.3 external ref, SPDX 3 `Software:cpe`).
- **Mapping audit log entry**: an in-source `//` comment next to each table row documenting the NVD-citation rationale (e.g., `// libpng vuln history filed under libpng:libpng per NVD CVE-2024-XXXX`). Doesn't appear in emitted SBOMs; lives in the source tree for maintainer transparency.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An operator scans a fixture containing a `pkg:generic/openssl@3.0.13` component; the emitted CDX SBOM has `components[?(@.purl=='pkg:generic/openssl@3.0.13')].cpe == 'cpe:2.3:a:openssl:openssl:3.0.13:*:*:*:*:*:*:*'`. Verified end-to-end via Trivy invocation: feed the SBOM to `trivy sbom <file>`, confirm ≥1 OpenSSL 3.0.13 CVE appears in the output. (Toolchain-graceful-skip if `trivy` unavailable.)
- **SC-002**: All 11 v1-supported library slugs (minus the documented exclusions) emit valid CPE 2.3 strings on a representative synthetic-PURL test set. Validation: each emitted string parses via a CPE 2.3 syntax check (URI-formatted CPE WFN canonical form) without error.
- **SC-003**: Symbol-fingerprint-only components emit NO CPE — verified by integration test that scans a fixture that produces a `pkg:generic/openssl` (no version) component and asserts the absence of any CPE-related field across CDX, SPDX 2.3, and SPDX 3.
- **SC-004**: Composite-evidence components (milestone-096 Q1) emit exactly ONE CPE — verified by integration test scanning a fixture that triggers both version-string + symbol-fingerprint matches for OpenSSL; the resulting merged component has exactly one CDX `cpe` field, one `cpe23Type` external ref in SPDX 2.3, one `cpe` array entry in SPDX 3.
- **SC-005**: `./scripts/pre-pr.sh` clean — zero clippy warnings, every test target reports `0 failed`. `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1` opt-in also passes (CPE emission preserves SPDX 3 conformance — the validator accepts `Software:cpe` arrays).
- **SC-006**: Golden regen scope bounded — at most 3 goldens regenerate (the 3 formats × 1 fixture if any existing fixture contains a binary-extracted CPE-eligible component). Per milestone-096 SC-007 the spurious-match bound was ≤1 across all 9 ecosystem fixtures, so this milestone inherits that constraint.
- **SC-007**: Zero new Cargo dependencies (FR-007).

## Assumptions

- The 11 v1 libraries cover the bulk of statically-linked CVE-relevant cases that the milestone-096 scanner can identify. New library coverage lands in follow-up milestones (097.x or 098.x) as the version-string + symbol-fingerprint scanners expand.
- NVD vendor/product names for the v1 libraries are stable enough that a once-set in-source table doesn't need monthly re-curation. Vendor renames in NVD do happen (rarely); when one is observed, a PR updates the table.
- CPE 2.3 is the dominant format target. CPE 2.2 ("URI binding") is legacy; modern Trivy / Grype / Dependency-Track all consume 2.3. Skipping 2.2 emission keeps the table simple.
- CDX 1.6's `component.cpe`, SPDX 2.3's `externalRefs[].referenceType = cpe23Type`, and SPDX 3's `Software:cpe` are stable native fields per the format specs (verified at planning time against the existing milestone-011 / -078 / -079 vocabulary inventory). No format-spec workarounds required.
- Existing package-db-sourced components (RPM/DEB/APK readers) either already emit CPEs via their distro metadata or are out of scope for this milestone — milestone 097 narrowly targets the binary-extracted `pkg:generic/...@...` cases, not the broader CPE-everywhere ambition.
- Existing `mikebom-cli/src/generate/cpe.rs` is the natural home for the new code (extend, not replace). At planning time the implementer audits its current shape to confirm — if it's RPM-specific, the new code lives in a sibling module under the same parent directory.

## Dependencies

- **Milestone 096** (binary-id enrichment) — provides the `pkg:generic/<lib>@<version>` components that this milestone enriches with CPE candidates. Direct dependency on milestone-096 PURL shape + library slug naming.
- **Milestone 011** (SPDX 3 full support) — provides the SPDX 3 emission infrastructure; the new CPE field flows through the existing SPDX 3 builder.
- **Milestone 079** (SPDX 3 ID vocab) — confirms the SPDX 3 `Software:cpe` field's exact IRI / canonical-name; this milestone defers to that registry for the field-name choice.
- **Milestone 052/047** (lifecycle scope + standards-native precedence) — the existing pattern for emitting standards-native fields (no `mikebom:*` annotation) the new code follows.

## Out of Scope

- **External CPE database lookups** (NVD APIs, deps.dev CPE queries, ClearlyDefined). Network-fetch fallback; high-cost; deferred to a future milestone if signal emerges.
- **CPE-to-CPE alias resolution** (e.g., `openssl:openssl` ↔ `openssl:openssl-foundation` ↔ `redhat:openssl`). The emitted CPE is the most-cited NVD vendor/product pair; downstream scanners do their own alias resolution.
- **CPE 2.2 emission** (legacy URI binding). Modern scanners consume 2.3; the URI-form is documented as legacy in the CPE spec.
- **Symbol-fingerprint-only CPE emission** (wildcard-version CPEs). Per FR-004 + Edge Cases — they generate too many false-positive CVE matches to be useful.
- **CPE emission for package-db-sourced components** (RPM/DEB/APK readers). Those readers emit format-native PURLs (`pkg:rpm/...`, `pkg:deb/...`) and downstream tools have well-established PURL→CPE mapping for distro packages. Out of scope for this milestone; could be a separate milestone if signal emerges.
- **Vendor-table runtime configuration** (CLI flag to add CPE rows from a YAML file at scan-time). Source-code edits suffice for v1; a runtime config path adds attack surface and dependency churn for marginal gain.
- **Per-component CPE multiplicity at the top-level field**. CDX 1.6 `component.cpe` is single-valued by spec; mikebom emits ONE primary candidate there. The existing `mikebom:cpe-candidates` property (already populated by the deb/apk/rpm readers when their synthesizer produces multiple plausible vendor:product pairs) carries the secondary candidates for libraries where two vendors are both NVD-cited (curl: `haxx:curl` primary, `curl:curl` secondary; libressl: `openbsd:libressl` primary, `libressl:libressl` secondary). What's out of scope is *broader alias-explosion* — e.g., emitting `redhat:openssl` + `oracle:openssl` + every distro's namespace alongside the upstream `openssl:openssl`. Two well-cited NVD candidates per library is the ceiling for v1; alias resolution beyond that belongs in the downstream scanner, not the SBOM.
- **OpenJDK build-suffix encoding** (`+12` in CPE update field). The CPE spec accommodates it, but NVD records are inconsistent — some use `update_12`, some `*`, some omit. v1 strips the suffix to match the most-common NVD shape; revisit if false-negatives emerge.
- **CPE 2.3 "Update" field for security patches** (e.g., OpenSSL `1.0.2t` as `cpe:2.3:a:openssl:openssl:1.0.2:t:*:...`). Some NVD records split the letter into the update field; the milestone-096 version-string scanner emits the suffix as part of the version. v1 stays with the verbatim version; revisit if false-negatives emerge.
- **CPE deprecation tracking** (some CPEs are deprecated in favor of newer ones per NVD's CPE dictionary). Beyond scope; the v1 table picks current canonical names per NVD's current state.
- **SBOM-validate-time CPE-vs-PURL consistency check** (ensuring the emitted CPE matches the PURL's library identity). The mapping table is the source of truth; the components-emitting code unconditionally trusts it.
