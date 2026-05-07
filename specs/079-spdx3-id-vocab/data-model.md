# Data Model — milestone 079 SPDX 3 externalIdentifierType conformance

The milestone introduces one new internal type (`SpdxIdType` enum), one new pure function (`map_scheme_to_vocab`), and one new emitted field (`comment` on `Core/ExternalIdentifier`). No changes to mikebom's internal `SchemeName` / `Identifier` / `IdentifierKind` / `IdentifierValue` types — those flow unchanged from milestones 073/074/076.

## Internal Rust types (NEW — `mikebom-cli/src/generate/spdx/v3_id_type_map.rs`)

### `SpdxIdType` enum

```rust
/// The 11 controlled-vocabulary values for SPDX 3's
/// `Core/externalIdentifierType` (per the 2026-05-07 schema audit
/// against `mikebom-cli/tests/fixtures/schemas/spdx-3.0.1.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpdxIdType {
    Other,
    Cve,
    Swhid,
    SecurityOther,
    Cpe23,
    PackageUrl,
    Gitoid,
    Cpe22,
    UrlScheme,
    Email,
    Swid,
}

impl SpdxIdType {
    /// Returns the literal vocab string the SPDX 3 emission writes
    /// into `externalIdentifierType`. Matches the schema enum verbatim.
    pub fn as_str(self) -> &'static str { /* "other" / "cve" / ... */ }
}
```

**Invariant**: `as_str()` returns one of the 11 literal strings the SPDX 3 SHACL constraint accepts. Validator-conformance is encoded in the type system per Constitution Principle IV.

**Lifetime**: `Copy`-cheap; created per-identifier at emission time; no allocation.

### `MappingResult` struct

```rust
/// Output of the per-identifier mapping decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingResult {
    /// SPDX 3 controlled-vocabulary value to emit.
    pub vocab_type: SpdxIdType,
    /// `Some(comment_text)` when information would otherwise be lost
    /// (the original mikebom scheme name doesn't equal the vocab value).
    /// `None` when no info-preservation comment is needed (the scheme
    /// IS a vocab value, e.g., `--component-id <PURL>=cve:...`, or the
    /// gitoid detection captures the full semantic).
    pub comment: Option<String>,
}
```

### `map_scheme_to_vocab` function

```rust
/// Pure function: (mikebom scheme, identifier value) → SPDX 3
/// vocab value + optional `comment` field text.
///
/// Determinism contract (FR-005): same inputs → byte-identical
/// outputs across re-runs. No I/O, no clock, no PRNG.
pub fn map_scheme_to_vocab(scheme: &SchemeName, value: &str) -> MappingResult { ... }
```

**Lookup logic** (per research §1):
1. If `scheme.as_str()` is one of the 11 vocab strings → `MappingResult { vocab_type: <that variant>, comment: None }`.
2. If `scheme.as_str() == "git"` AND `value` matches `^[0-9a-f]{40}$` → `MappingResult { vocab_type: Gitoid, comment: None }`.
3. Otherwise (every built-in non-vocab scheme + every user-defined non-vocab scheme + `git:` with non-SHA value) → `MappingResult { vocab_type: Other, comment: Some(format!("original-scheme: {}", scheme.as_str())) }`.

### `is_git_sha` helper

```rust
/// Compiled-once regex `^[0-9a-f]{40}$`.
fn is_git_sha(value: &str) -> bool { ... }
```

Uses `std::sync::OnceLock<Regex>` per the existing project pattern. No `lazy_static!` (not in workspace deps).

## Wire-format entities (SPDX 3 graph elements that change)

### `Core/ExternalIdentifier` element (MODIFIED)

```json
// BEFORE (today's emission, conformance-broken when scheme is non-vocab):
{
  "type": "ExternalIdentifier",
  "externalIdentifierType": "image",                          // ← SHACL violation
  "identifier": "registry.example.com/img:tag"
}

// AFTER (post-fix, conformant):
{
  "type": "ExternalIdentifier",
  "externalIdentifierType": "other",                          // ← in vocab
  "identifier": "registry.example.com/img:tag",
  "comment": "original-scheme: image"                         // ← NEW: info preservation
}
```

**Field changes**:
- `externalIdentifierType` value remapped (was: arbitrary mikebom scheme; now: one of 11 vocab values per `map_scheme_to_vocab`).
- `comment` field added when the mapping returns `Some(comment)`. Optional per the SPDX 3 schema; not emitted when mapping returns `None`.

**Special case (gitoid)**:
```json
// auto-detected git SHA
{
  "type": "ExternalIdentifier",
  "externalIdentifierType": "gitoid",                         // ← per-research §2 detection
  "identifier": "0123456789abcdef0123456789abcdef01234567"
  // No `comment` — gitoid carries the semantic faithfully
}
```

**Special case (vocab-named user scheme)**:
```json
// --component-id <PURL>=cve:CVE-2024-1234
{
  "type": "ExternalIdentifier",
  "externalIdentifierType": "cve",
  "identifier": "CVE-2024-1234"
  // No `comment` — operator named the vocab value directly
}
```

### Document-level `externalIdentifier[]` (MODIFIED — call site at `v3_document.rs:309`)

The document-level identifiers (`SpdxDocument.externalIdentifier`) flow through the same `map_scheme_to_vocab` helper. No structural change beyond the per-element shape above.

### Per-package `externalIdentifier[]` (MODIFIED — call site at `v3_packages.rs:170`)

The per-`software_Package` identifiers (`software_Package.externalIdentifier`) flow through the same helper. No structural change beyond the per-element shape above.

### Sort-key extension on the `externalIdentifier[]` array (MODIFIED — `v3_external_ids.rs`)

```rust
// BEFORE:
ext_ids.sort_by_key(|e| (e.external_identifier_type.clone(), e.identifier.clone()));

// AFTER:
ext_ids.sort_by_key(|e| (
    e.external_identifier_type.clone(),
    e.identifier.clone(),
    e.comment.clone().unwrap_or_default(),
));
```

Per research §4: preserves determinism + dedup correctness when multiple identifiers map to the same vocab value but carry different original-scheme provenance.

## Validation rules

- **VR-079-001**: Every emitted SPDX 3 `externalIdentifier[]` element MUST have `externalIdentifierType` ∈ `{other, cve, swhid, securityOther, cpe23, packageUrl, gitoid, cpe22, urlScheme, email, swid}`. Verified by integration test.
- **VR-079-002**: When the mapping returns `Some(comment)`, the `comment` field MUST be present in the emitted JSON-LD object with the exact string `format!("original-scheme: {}", scheme.as_str())`.
- **VR-079-003**: When the mapping returns `None` for comment, the emitted JSON-LD object MUST NOT contain a `comment` field (no empty string, no `null`, no field at all).
- **VR-079-004**: The `Core/ExternalIdentifier` SHACL constraint set MUST pass for every emitted SBOM under `spdx3-validate==0.0.5`. Verified by integration test (extends milestone 078's `every_existing_golden_passes_validator` automatically).
- **VR-079-005**: The mapping is deterministic — same `(scheme, value)` input → byte-identical `MappingResult` output across re-runs.
- **VR-079-006**: The sort key `(externalIdentifierType, identifier, comment)` produces a total order on `externalIdentifier[]` arrays. Two array entries with identical sort keys are duplicates and dedup to one entry.
- **VR-079-007**: CDX 1.6 + SPDX 2.3 emission code paths are NOT touched by this milestone. Verified by `cdx_regression` + `spdx_regression` test targets continuing to pass without `MIKEBOM_UPDATE_*_GOLDENS` env vars.

## Backward compatibility

- **No new `Cargo.toml` deps**: `regex` is already in the dependency closure (used elsewhere in the project for milestone 073's scheme-name validation regex).
- **No MSRV change**: stable Rust toolchain per workspace.
- **No nightly required**: pure user-space transform.
- **CDX 1.6 + SPDX 2.3 byte-identity goldens stay byte-identical** (FR-006 + VR-079-007).
- **SPDX 3 byte-identity goldens** that don't exercise auto-detected, build-tier, or user-defined identifiers stay byte-identical (FR-007). The 9 milestone-078 source-tier ecosystem goldens (`apk`/`cargo`/`deb`/`gem`/`golang`/`maven`/`npm`/`pip`/`rpm`) fall into this category — verified by inspection of those fixtures (manifest-only, no `Identifier` injection points).
- **Downstream consumers of mikebom's pre-fix SPDX 3 output** that hard-coded `externalIdentifierType: "image"` need to update to either (a) read the vocab value `other` + parse the `comment` field's `original-scheme: ` prefix, or (b) filter by mikebom-aware logic. This is the expected operator-visible change of the milestone (analogous to milestone 078's `createdBy` slot move). Operators who consume by spec-defined paths (filter `externalIdentifierType` against the SPDX 3 controlled vocabulary) will work post-fix without changes — they just won't see the `image:` etc. values they've never been allowed to see per the SPDX 3 spec.
