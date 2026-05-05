# Data Model — milestone 073 source identifiers

The milestone introduces 4 new types in `mikebom-cli/src/binding/identifiers/` plus extends one existing struct in `mikebom-cli/src/generate/mod.rs`. Constitution Principle IV: every domain value is a newtype or enum; production code uses `anyhow::Result` / `IdentifierError`; test code uses `#[cfg_attr(test, allow(clippy::unwrap_used))]`.

## Entities

### `Identifier` (new)

The canonical type. One `Identifier` per (scheme, value) pair attached to an SBOM document.

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Identifier {
    pub scheme: SchemeName,
    pub value: IdentifierValue,
    #[serde(skip)]
    pub kind: IdentifierKind,
    /// Optional human-readable origin info — populated by auto-detection
    /// (`"auto-detected from git remote `origin`"`) or empty for manual flags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_label: Option<String>,
}

impl Identifier {
    /// Parse `<scheme>:<value>` from a CLI flag value.
    /// Returns IdentifierError on scheme-prefix parse failure.
    /// Built-in scheme value-validation runs and may downgrade `kind` to UserDefined
    /// with a tracing::warn! per research.md §1's soft-fail rule.
    pub fn parse(raw: &str) -> Result<Self, IdentifierError> { /* ... */ }

    pub fn is_builtin(&self) -> bool { matches!(self.kind, IdentifierKind::Builtin(_)) }
}
```

### `SchemeName` (new)

Newtype around the scheme prefix. Construction validates against the FR-004 regex.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SchemeName(String);

impl SchemeName {
    /// Construct from a string. Validates against `^[a-z][a-z0-9_-]*$` (FR-004).
    pub fn new(s: impl Into<String>) -> Result<Self, IdentifierError> {
        let s = s.into();
        // Validate: starts with lowercase letter; subsequent chars are
        // lowercase ASCII alphanumeric, underscore, or hyphen. Rejects
        // empty strings, leading digits, uppercase, etc.
        // ...
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str { &self.0 }
}
```

### `IdentifierValue` (new)

Newtype around the post-`:` value. Opaque post-parse — built-in scheme validators inspect it but the type itself doesn't enforce structure.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct IdentifierValue(String);

impl IdentifierValue {
    /// Construct from anything string-like. Empty values are rejected.
    pub fn new(s: impl Into<String>) -> Result<Self, IdentifierError> {
        let s = s.into();
        if s.is_empty() { return Err(IdentifierError::EmptyValue); }
        Ok(Self(s))
    }
    pub fn as_str(&self) -> &str { &self.0 }
}
```

### `IdentifierKind` (new)

Two-variant enum classifying whether the scheme is recognized by mikebom.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierKind {
    /// One of the 4 built-in schemes; value passed validation.
    Builtin(BuiltinScheme),
    /// Either a non-built-in scheme (operator-defined) OR a built-in
    /// scheme whose value failed validation (research.md §1's soft-fail
    /// path — the identifier emits as opaque under `mikebom:source-identifiers`).
    UserDefined,
}
```

### `BuiltinScheme` (new)

Closed registry of recognized built-in schemes. Each variant has an associated CDX `externalReferences[].type` value (per research.md §2).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinScheme {
    Repo,
    Git,
    Image,
    Attestation,
}

impl BuiltinScheme {
    pub fn from_scheme_name(name: &SchemeName) -> Option<Self> {
        match name.as_str() {
            "repo" => Some(Self::Repo),
            "git" => Some(Self::Git),
            "image" => Some(Self::Image),
            "attestation" => Some(Self::Attestation),
            _ => None,
        }
    }

    /// CDX 1.6 `externalReferences[].type` value for this scheme (research.md §2).
    pub fn cdx_external_reference_type(self) -> &'static str {
        match self {
            Self::Repo | Self::Git => "vcs",
            Self::Image => "distribution",
            Self::Attestation => "attestation",
        }
    }

    /// SPDX 2.3 `Package.externalRefs[].referenceCategory` — uniformly
    /// `PERSISTENT-ID` for all built-in schemes per FR-005.
    pub fn spdx23_reference_category(self) -> &'static str { "PERSISTENT-ID" }
}
```

### `IdentifierError` (new)

```rust
#[derive(Debug, thiserror::Error)]
pub enum IdentifierError {
    #[error("identifier missing `:` separator: {0:?}")]
    MissingSeparator(String),

    #[error("identifier scheme is empty")]
    EmptyScheme,

    #[error("identifier value is empty")]
    EmptyValue,

    #[error("scheme {0:?} fails regex `^[a-z][a-z0-9_-]*$` (FR-004)")]
    InvalidSchemeName(String),

    /// Soft-fail bubbled up from a built-in scheme's value validator.
    /// Caller logs `tracing::warn!` and downgrades to `IdentifierKind::UserDefined`.
    #[error("built-in scheme `{scheme}` value validation failed: {reason}")]
    BuiltinValidation { scheme: String, reason: String },
}
```

### `ScanArtifacts.source_identifiers: Vec<Identifier>` (existing struct — extended)

Located at `mikebom-cli/src/generate/mod.rs`. Add a new field:

```rust
pub struct ScanArtifacts<'a> {
    // ... existing fields ...
    /// Milestone 073: identifiers attached at scan invocation
    /// (auto-detected + manual `--with-source` flags). Auto-detected
    /// entries appear first in the Vec; manual entries follow in
    /// supply order. Already deduplicated by (scheme, value) pre-emit.
    pub source_identifiers: Vec<Identifier>,
}
```

Existing `ScanArtifacts` constructions get an additional `source_identifiers: vec![]` field (back-compat default). Construction with auto-detection lives in `cli/scan_cmd.rs::execute` near the top of the scan flow.

## Relationships

```text
Identifier
   │
   ├── 1 ── parse-validates ──> SchemeName (regex per FR-004)
   ├── 1 ── parse-validates ──> IdentifierValue (non-empty)
   ├── 1 ── classified-by ────> IdentifierKind {Builtin(BuiltinScheme), UserDefined}
   └── 0..1 ── source_label (set when auto-detected)

BuiltinScheme
   │
   └── maps-to:
       ├── CDX externalReferences[].type
       ├── SPDX 2.3 Package.externalRefs[].referenceCategory (always "PERSISTENT-ID")
       └── SPDX 3 Element.externalIdentifier[].type (matches scheme name)

ScanArtifacts.source_identifiers (Vec<Identifier>)
   │
   └── consumed by:
       ├── cyclonedx/metadata.rs                  → externalReferences[]
       ├── spdx/document.rs (creationInfo)         → creators[] redundant text
       ├── spdx/packages.rs (main-module)          → externalRefs[PERSISTENT-ID]
       ├── spdx/v3_document.rs (SpdxDocument)      → externalIdentifier[]
       └── spdx/{annotations, v3_annotations}.rs   → mikebom:source-identifiers (user-defined only)
```

## State / lifecycle

Identifiers are immutable post-emit. Once a SBOM carrying identifiers is written, the identifier set never updates in-place — a re-scan with new flags produces a new SBOM document with potentially different identifiers.

Auto-detection runs ONCE per scan, at the top of `cli/scan_cmd.rs::execute`. The result is cached in the `Vec<Identifier>` and threaded through to all emitters. No re-detection per format.

## Validation rules

- **VR-001**: `SchemeName::new` MUST reject any input that fails the FR-004 regex `^[a-z][a-z0-9_-]*$`. Empty schemes are rejected. The CLI parser surface (clap) produces `IdentifierError::InvalidSchemeName` which is converted to a `clap::Error` for parse-time rejection.
- **VR-002**: `IdentifierValue::new` MUST reject empty values. Whitespace-only values pass (built-in validators may reject them downstream).
- **VR-003**: `Identifier::parse` MUST split on the FIRST `:` only. Values containing additional `:` characters (URLs with ports, in-toto IRIs, the `image:` `@sha256:...` segment) are preserved verbatim.
- **VR-004**: Auto-detection MUST produce at most ONE `repo:` identifier per scan (the chosen remote per the 3-step fallback). Multiple auto-detected `repo:` identifiers from a single scan would violate the determinism contract (FR-009).
- **VR-005**: Built-in scheme value-validation failures MUST emit a `tracing::warn!` and downgrade the identifier's `kind` to `UserDefined` (research.md §1 soft-fail). The validation does NOT fail the scan.
- **VR-006**: Identifier deduplication MUST be by `(scheme, value)` exact match. Auto-detected entries that match a manual entry on `(scheme, value)` are deduplicated to the manual entry (the manual entry wins; the auto-detected source_label is dropped).
- **VR-007**: The `mikebom:source-identifiers` annotation MUST emit only when the user-defined entry set is non-empty. Empty user-defined set → no annotation, preserving cross-format byte-identity for non-user-defined-namespace scans.
- **VR-008**: The `Vec<Identifier>` in `ScanArtifacts` MUST preserve order: auto-detected first, then manual flags in supply order. The order is part of the FR-009 deterministic-emission contract.

## Backward compatibility

- The new flag `--with-source` is opt-in. Operators not passing it see no behavior change.
- Auto-detection runs by default but is `tracing::info!` only when no detection is possible — never fails the scan.
- New `source_identifiers: Vec<Identifier>` field on `ScanArtifacts` is additive; existing call sites that use the struct-update syntax (`..default()`) continue to compile.
- Source-tier byte-identity goldens for non-git fixtures stay alpha.15-identical (no detection fires).
- Source-tier byte-identity goldens for git-tracked fixtures get one additive identifier slot per format — that's the expected FR-012 regen.
