# Data Model — milestone 076 subject identifier + per-component identifiers

The milestone adds one new variant to the existing `BuiltinScheme` enum and one small new struct (`ComponentIdentifierFlag`) for the per-component flag's parsed shape. Otherwise composes existing milestone-073 types.

## Entities

### `BuiltinScheme::Subject` (extension to existing enum)

```rust
// In mikebom-cli/src/binding/identifiers/mod.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinScheme {
    Repo,
    Git,
    Image,
    Attestation,
    Subject,        // NEW
}

impl BuiltinScheme {
    pub fn from_scheme_name(name: &SchemeName) -> Option<Self> {
        match name.as_str() {
            "repo" => Some(Self::Repo),
            "git" => Some(Self::Git),
            "image" => Some(Self::Image),
            "attestation" => Some(Self::Attestation),
            "subject" => Some(Self::Subject),    // NEW
            _ => None,
        }
    }

    pub fn cdx_external_reference_type(self) -> &'static str {
        match self {
            Self::Repo | Self::Git => "vcs",
            Self::Image => "distribution",
            Self::Attestation | Self::Subject => "attestation",  // Subject reuses per research §1
        }
    }

    pub fn spdx23_reference_category(self) -> &'static str { "PERSISTENT-ID" }
    // Same uniform mapping as existing schemes.
}
```

### `SubjectIdentifier` (conceptual; materialized as `Identifier` instance)

A document-level `Identifier` with `scheme = SchemeName("subject")`, `value = IdentifierValue("<algo>:<hex>")`, `kind = IdentifierKind::Builtin(BuiltinScheme::Subject)` (or `UserDefined` on validation failure per research §4).

Multiple SubjectIdentifiers may attach to one SBOM (multi-output builds). Auto-detected ones carry `source_label = "auto-detected from build-tier in-toto subject `<subject-name>`"`. Manual ones from `--subject-hash` carry `source_label = "manual --subject-hash"`.

### `ComponentIdentifierFlag` (NEW, public struct)

```rust
// In mikebom-cli/src/binding/identifiers/component_id.rs (NEW module)
#[derive(Debug, Clone)]
pub struct ComponentIdentifierFlag {
    /// Exact PURL string the operator typed; matched byte-identically
    /// against `components[].purl` per research §5.
    pub selector_purl: String,
    /// User-defined scheme name. MUST NOT be a built-in (parser
    /// rejects per FR-009).
    pub scheme: SchemeName,
    /// The identifier value. No format constraint beyond the existing
    /// IdentifierValue rules (non-empty).
    pub value: IdentifierValue,
}

impl ComponentIdentifierFlag {
    /// Parse a `--component-id PURL=scheme:value` flag value.
    /// Splits on the FIRST `=` (PURL containing `=` is invalid input).
    /// Splits the RHS on the FIRST `:` (scheme:value).
    /// Rejects built-in scheme names per FR-009.
    pub fn parse(raw: &str) -> Result<Self, ComponentIdentifierFlagError>;
}
```

Lifetime: parsed at CLI parse time, stored in `ScanArgs.component_id: Vec<ComponentIdentifierFlag>` and `RunArgs.component_id: Vec<ComponentIdentifierFlag>`, threaded through `ScanArtifacts.component_identifiers` to per-format emitters.

### `ComponentIdentifierFlagError` (NEW)

```rust
#[derive(Debug, thiserror::Error)]
pub enum ComponentIdentifierFlagError {
    #[error("--component-id missing `=` separator: {0:?}")]
    MissingEquals(String),

    #[error("--component-id PURL (LHS of `=`) is empty")]
    EmptyPurl,

    #[error("--component-id RHS missing `:` separator: {0:?}")]
    MissingColon(String),

    #[error("--component-id scheme is empty")]
    EmptyScheme,

    #[error("--component-id value is empty")]
    EmptyValue,

    #[error("--component-id scheme {0:?} is reserved for document-level built-in usage")]
    BuiltinSchemeRejected(String),

    #[error("--component-id scheme {0:?} fails the FR-004 regex from milestone 073: {1}")]
    InvalidSchemeName(String, IdentifierError),
}
```

### `ScanArtifacts.component_identifiers: Vec<ComponentIdentifierFlag>` (extension)

Located at `mikebom-cli/src/generate/mod.rs`. Add a new field:

```rust
pub struct ScanArtifacts<'a> {
    // ... existing fields including identifiers (073) ...
    /// Milestone 076: per-component user-defined identifiers from
    /// `--component-id` flags. Threaded to per-format emitters which
    /// match `selector_purl` against emitted components and attach
    /// the identifier to matches.
    pub component_identifiers: Vec<ComponentIdentifierFlag>,
}
```

Existing `ScanArtifacts` constructions get an additional `component_identifiers: vec![]` field (back-compat default).

## Functions (public surface added by this milestone)

### `validate_subject` (NEW, in `validators.rs`)

```rust
pub fn validate_subject(value: &str) -> Result<(), IdentifierError>;
```

Behavior per research §4: accepts `^(sha256:[0-9a-f]{64}|sha512:[0-9a-f]{128})$` exactly; otherwise `Err(IdentifierError::BuiltinValidation { ... })` triggering soft-fail.

### `subject_identifiers_from_attestation_subjects` (NEW, in `auto_detect.rs`)

```rust
pub fn subject_identifiers_from_attestation_subjects(
    subjects: &[Subject],
) -> Vec<Identifier>;
```

Behavior:
1. Iterate `subjects` (already in lexically-sorted witness-v0.1 order).
2. For each subject, extract the sha256 digest from the digest map. If sha256 is absent, log `tracing::info!` with subject name + available algos and skip per FR-002 + 2026-05-06 clarification.
3. Construct an `Identifier` with scheme `subject`, value `sha256:<hex>`, kind `Builtin(Subject)`, source_label `"auto-detected from build-tier in-toto subject `<subject-name>`"`.
4. Append to result vec.

Never panics, never returns `Result`. Failures collapse to "skip this subject."

### Updated `auto_detect_build_tier_identifiers` flow

The existing milestone-074 helper continues to return `repo:` and `git:` identifiers. The new `subject:` identifiers come from `subject_identifiers_from_attestation_subjects` called at the build-tier emission site. Build-tier identifier vec ordering: `repo:` first, then `git:`, then `subject:` entries (in witness-v0.1 lexical order), then any manual `--subject-hash` entries (in supply order), then existing manual `--repo` / `--git-ref` etc.

## Validation rules

- **VR-076-001**: `BuiltinScheme::Subject` value MUST pass the `validate_subject` regex per research §4. Failures soft-fail to `IdentifierKind::UserDefined` per FR-005.
- **VR-076-002**: Auto-detected `subject:` identifiers MUST emit only when the source subject has a sha256 digest in its `digest` map. Subjects with only non-sha256 digests are skipped with an info-log per FR-002.
- **VR-076-003**: `ComponentIdentifierFlag::parse` MUST reject inputs whose LHS is empty, whose `=` separator is missing, whose RHS lacks `:`, whose scheme is empty, whose value is empty, or whose scheme matches a built-in name (`repo`, `git`, `image`, `attestation`, `subject`). Reject at CLI parse time with a clear, actionable error message.
- **VR-076-004**: When a `--component-id` selector matches zero components in the emitted SBOM, the system MUST emit a `tracing::warn!` listing the unmatched selector and continue. The scan MUST NOT fail.
- **VR-076-005**: When a `--component-id` selector matches multiple components (same PURL across different `bom-ref` values), the identifier MUST be attached to ALL matching components.
- **VR-076-006**: Per-component identifier emission MUST preserve existing per-component property/externalRef ordering and append new entries at the end, in lexical order by `(scheme, value)` per research §6. No churn for components that don't match any `--component-id` selector.

## Relationships

```text
mikebom sbom scan / mikebom trace run
    │
    ├── --subject-hash <algo>:<hex> (repeatable) ──┐
    │                                              ├── ScanArgs/RunArgs
    └── --component-id PURL=scheme:value (rep.) ──┘     │
                                                         ▼
                                  ┌──────────────────────────────┐
                                  │ ScanArtifacts                │
                                  │   .identifiers (incl.        │
                                  │    --subject-hash + auto-    │
                                  │    detected subject:)        │
                                  │   .component_identifiers     │
                                  └────────────┬─────────────────┘
                                               │
                                               ▼
                              Per-format emitters
                                ├── CDX        → metadata.component.externalReferences[type:attestation]
                                │                + components[].properties[name=scheme,value=value]
                                ├── SPDX 2.3   → Package.externalRefs[PERSISTENT-ID]
                                │                (both subject: and per-component user-defined)
                                └── SPDX 3     → Element.externalIdentifier[type=scheme,identifier=value]
                                                 (both subject: and per-component user-defined)

                              External SBOM-store consumer
                                ├── reads image SBOM components[].hashes[].sha256 == X
                                ├── searches store for SBOM with subject:sha256:X identifier
                                ├── correlates ✓
                                └── walks build SBOM's git: → matching source SBOM
```

## Backward compatibility

- No new `Cargo.toml` deps; no MSRV change; no nightly required.
- `BuiltinScheme` enum gains a new variant; existing `match` exhaustiveness in mikebom-cli is updated as part of this milestone (compile-time check).
- `ScanArtifacts` gains a new field with `default = vec![]`; existing struct-update-syntax callers continue to compile via `..Default::default()`.
- Existing milestone-073/074/075 byte-identity goldens stay byte-identical: no fixtures pass `--subject-hash` or `--component-id`, and the build-tier auto-detect is gated on having a non-empty subject set (which existing test fixtures don't have, since they're source-tier scans).
- The CDX `externalReferences[type=attestation]` carrier reuse is additive — milestone 073's `attestation:` IRI emissions are unchanged. Multi-entry arrays (one for IRI, one for subject hash) are valid CDX 1.6 per spec.
- New CLI flags (`--subject-hash`, `--component-id`) are opt-in; operators not passing them see no behavior change.
