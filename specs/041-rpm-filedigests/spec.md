# Feature Specification: Rpm FILEDIGESTS Cross-Reference

**Feature Branch**: `041-rpm-filedigests`
**Created**: 2026-04-29
**Status**: Draft
**Input**: User description: "Rpm FILEDIGESTS cross-reference for additionalContext (per-file upstream-provided digest)"

## User Scenarios & Testing *(mandatory)*

Milestone 040 Q1 deferred the rpm FILEDIGESTS cross-reference to a
follow-on. This milestone closes that. After it ships, every
populated rpm `evidence.occurrences[]` entry's `additionalContext`
JSON-string carries both `sha256` (mikebom-computed from on-disk
bytes at scan time) AND `rpm_filedigest` (the upstream-provided
per-file digest from the rpm package's HeaderBlob FILEDIGESTS
tag). This brings rpm to full cross-ref symmetry with deb (which
carries `md5`) and apk (which carries `sha1`).

### User Story 1 - Per-file digest cross-ref for rpm components (Priority: P1)

An SBOM consumer correlating a rpm component's per-file evidence
against an upstream-published checksum (rpm-sigcheck-style
verification, vulnerability-scan tooling that already trusts the
rpm-distro digests, or compliance audits that require an
upstream-provenance claim) wants the same cross-reference checksum
deb and apk components carry. Today rpm components ship with
mikebom-computed SHA-256 only — the upstream digest from the
package's FILEDIGESTS tag is parsed but discarded.

**Why this priority**: small, contained symmetry-completion
milestone. Closes a documented Q1 deferral from milestone 040.
Mirrors milestone 040 US2 (apk SHA-1 cross-ref) almost exactly in
shape — single new field, single new emitter clause, single new
ecosystem-specific accessor on the rpm header.

**Independent Test**: Run `mikebom sbom scan --image fedora:40
--output fedora.cdx.json` and inspect a populated rpm
occurrence's `additionalContext`: it now contains both `sha256`
(mikebom-computed) AND `rpm_filedigest` (algorithm-prefixed:
`sha256:<hex>` for modern rpm packages, `md5:<hex>` for older
ones), where today only `sha256` appears.

**Acceptance Scenarios**:

1. **Given** a modern rpm-based image (`fedora:40`,
   `almalinux:9`, etc.), **When** the user runs an SBOM scan,
   **Then** every populated rpm occurrence's `additionalContext`
   carries both `sha256` AND `rpm_filedigest`. The
   `rpm_filedigest` value uses the algorithm-prefix form
   (e.g. `sha256:<64-hex>`).
2. **Given** an older rpm-based image whose FILEDIGESTS use a
   non-default algorithm (e.g. an old CentOS-7-era package using
   MD5-encoded digests), **When** the user runs a scan, **Then**
   the `rpm_filedigest` carries the algorithm prefix that
   matches the package's actual choice (e.g. `md5:<32-hex>`).
3. **Given** a malformed or empty FILEDIGESTS tag (rare; would
   indicate a damaged rpmdb), **When** the user runs a scan,
   **Then** the affected occurrences carry `sha256` only —
   `rpm_filedigest` is omitted from `additionalContext` for
   those entries; no error.
4. **Given** a deb-based image scan or an apk-based image scan,
   **When** the user runs it after this milestone ships,
   **Then** the deb / apk output is byte-identical to milestone
   040's output — only the rpm path's `additionalContext`
   carries the new `rpm_filedigest` key.

---

### Edge Cases

- **FILEDIGESTALGO tag missing or zero**: per the rpm spec, when
  FILEDIGESTALGO is absent or set to `0`, the algorithm defaults
  to MD5. mikebom honors this default rather than failing.
- **FILEDIGESTS array length mismatched against BASENAMES**:
  rare; would indicate a damaged HeaderBlob. mikebom emits
  `rpm_filedigest` only for indices where both arrays have a
  value; out-of-range entries get no cross-ref (sha256-only).
- **Unknown / future FILEDIGESTALGO codes**: the IANA hash-
  algorithm registry has codes for SHA-3 family etc. that
  aren't typically used by rpm yet. mikebom silently omits
  `rpm_filedigest` for unknown codes rather than emitting a
  partial / mis-prefixed string. A debug log notes the unknown
  code so operators can surface it if needed.
- **Empty per-file digest string**: rpm sometimes ships empty
  FILEDIGESTS entries for non-regular files (devices, fifos,
  symlinks). mikebom omits `rpm_filedigest` for those entries.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When mikebom scans an rpm-based image, each
  populated `evidence.occurrences[]` entry MUST carry an
  `rpm_filedigest` key in its `additionalContext` JSON-string
  alongside the existing `sha256` key, taking the form
  `<algorithm>:<lowercase-hex>` where `<algorithm>` is one of:
  `md5`, `sha1`, `sha256`, `sha384`, `sha512` (the algorithms
  rpm currently uses; matches the IANA hash-algorithm registry
  values rpm honors).

- **FR-002**: The algorithm encoded in `rpm_filedigest` MUST
  match the package's actual FILEDIGESTALGO value (or default
  to MD5 when FILEDIGESTALGO is absent / zero, per the rpm
  spec).

- **FR-003**: Occurrences whose underlying FILEDIGESTS entry is
  empty (non-regular files in the package) or missing
  (mismatched array lengths) MUST appear in the SBOM with the
  existing `sha256` cross-ref but with `rpm_filedigest` OMITTED
  from `additionalContext`. The scan MUST NOT fail.

- **FR-004**: Deb-based image scans and apk-based image scans
  MUST be byte-identical to milestone 040's output. Only the
  rpm-side `additionalContext` adds the new key.

- **FR-005**: No new top-level Cargo dependencies. Reuses
  `sha2`, the existing rpm header reader, and stdlib.

- **FR-006**: The byte-identity goldens MUST regen with zero
  diff (the 27-fixture suite uses `--no-deep-hash`, which
  emits no per-file occurrences and is therefore insulated
  from this change by design).

### Key Entities

- **Rpm FILEDIGESTS tag (1035)**: a string-array tag in the
  rpm HeaderBlob, parallel to BASENAMES. Each entry is a
  hex-encoded digest of the corresponding file's content as
  computed by the rpm packager at build time.
- **Rpm FILEDIGESTALGO tag (5011)**: an int32 tag carrying
  the IANA hash-algorithm code that FILEDIGESTS uses. Common
  values: `1`=MD5, `2`=SHA-1, `8`=SHA-256, `9`=SHA-384,
  `10`=SHA-512. Absent / `0` → MD5 (legacy default).
- **`FileOccurrence.rpm_file_digest`**: new optional
  `Option<String>` field on the existing
  `mikebom_common::resolution::FileOccurrence` carrying the
  algorithm-prefixed form (e.g. `"sha256:abc..."`) for rpm
  occurrences. None for non-rpm occurrences (deb, apk).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A scan of `fedora:40` produces an SBOM where every
  populated rpm occurrence's `additionalContext` carries both
  `sha256` and `rpm_filedigest` — measurable as `100%` of
  populated rpm occurrences having both keys (today: 0%).

- **SC-002**: For at least one rpm package in `fedora:40`, the
  `rpm_filedigest` value MUST match the upstream-published rpm
  for that package — verifiable by hand-checking against
  `rpm -ql --dump <pkg>` (which emits per-file SHA-256 digests).

- **SC-003**: A scan of `alpine:3.19` (apk) and
  `debian:bookworm-slim` (deb) produces an SBOM byte-identical
  to milestone 040's output — measurable as the existing
  byte-identity goldens regenning with zero diff AND the
  apk/deb portions of the live-image SBOMs comparing equal
  via canonical-JSON diff.

- **SC-004**: All 3 CI lanes green.

## Assumptions

- The rpm packages in `fedora:40` use SHA-256 FILEDIGESTS
  (the modern default since dnf 4 / rpm 4.14, ~2018). Older
  RHEL / CentOS rpm versions use SHA-1 or MD5; mikebom
  handles all three uniformly via the FILEDIGESTALGO tag.
- "rpm-based image" includes the same set as milestone 040:
  `fedora:*`, `almalinux:*`, `rockylinux:*`,
  `centos:stream*`, `redhat/*`. No additional rpm-variant
  detection is needed.
- mikebom's existing `RpmHeader::string_array` /
  `int32_array` accessors handle the FILEDIGESTS / FILEDIGESTALGO
  tag-types correctly (FILEDIGESTS is a STRING_ARRAY = type 8;
  FILEDIGESTALGO is INT32 = type 4).

## Out of scope

- Modifying the on-disk Merkle root computation. The
  per-component Merkle root remains a function of
  occurrence locations + observed SHA-256s only. The new
  cross-ref is purely advisory metadata.
- Schema-level `hashes` array on `FileOccurrence` (would
  unify cross-ref carriers across deb / apk / rpm; defer
  until external demand surfaces).
- Container layer attribution (separate architectural
  milestone; pre-existing deferred item).
- Maven sidecar Debian / Alpine variants (separate concern).
