# Data Model — milestone 077 root component override

The milestone introduces one new public struct (`RootComponentOverride`) on `ScanArtifacts` and one new private validator function for CLI parse-time input. Otherwise composes existing 073-076 types.

## Entities

### `RootComponentOverride` (NEW, public, in `mikebom-cli/src/generate/mod.rs`)

```rust
#[derive(Debug, Clone, Default)]
pub struct RootComponentOverride {
    /// When `Some(name)`, replaces the auto-derived
    /// `metadata.component.name` (CDX) / main-module
    /// `Package.name` (SPDX 2.3) / root element name (SPDX 3) with
    /// `name`. Validated at CLI parse per VR-077-001.
    pub name: Option<String>,

    /// When `Some(version)`, replaces the auto-derived version
    /// field across all three formats. Validated at CLI parse per
    /// VR-077-001.
    pub version: Option<String>,
}

impl RootComponentOverride {
    /// Returns true iff at least one field is set. Used by per-format
    /// builders to decide whether to filter manifest-derived main-
    /// module components from the emitted `components[]` array per
    /// the 2026-05-06 clean-replacement clarification.
    pub fn is_active(&self) -> bool {
        self.name.is_some() || self.version.is_some()
    }
}
```

Field on `ScanArtifacts`:

```rust
pub struct ScanArtifacts<'a> {
    // ... existing fields including identifiers (073), source_document_binding (072),
    // component_identifiers (076), keep_credentials_in_identifiers (075) ...
    /// Milestone 077: operator-supplied overrides for the root
    /// component's name + version.
    pub root_override: RootComponentOverride,
}
```

Default to `RootComponentOverride::default()` for back-compat; existing call sites with `..Default::default()` syntax continue to compile.

### `ValidatedRootField` (conceptual; materialized as `String`)

The output of `validate_root_field`. Same string the operator typed, returned only after passing the FR-006 validation rule (no whitespace / control / `?` / `#`). The `Result<String, String>` shape matches the existing milestone-076 `parse_component_id_flag` pattern.

## Functions (public surface added by this milestone)

### `validate_root_field` (NEW, private to `cli/scan_cmd.rs` or shared module)

```rust
fn validate_root_field(value: &str, flag_name: &str) -> Result<String, String>;
```

Behavior per research §4: rejects empty, whitespace, control characters, `?`, `#`. Accepts any other non-empty UTF-8. Used as clap value_parser for both `--root-name` and `--root-version`.

### `percent_encode_purl_name` (NEW, private to `generate/`)

```rust
fn percent_encode_purl_name(s: &str) -> String;
```

RFC 3986 percent-encoding for the PURL `name` segment. Preserves `[A-Za-z0-9._~-]`; percent-encodes everything else (UTF-8-aware). Used ONLY in the override path's PURL construction; the existing `url_friendly` helper at `spdx/v3_document.rs:410` is unchanged for the non-override path per research §1.

## Validation rules

- **VR-077-001**: `--root-name` and `--root-version` values MUST be rejected at CLI parse time when they contain ANY of: ASCII whitespace (`\s`), ASCII control characters (`\x00`–`\x1F`, `\x7F`), `?`, or `#`. Empty values rejected. All other UTF-8 characters accepted.
- **VR-077-002**: When `RootComponentOverride.name.is_some()`, the per-format builders MUST use the override value verbatim in the displayed `name` field of the root component AND derive `bom-ref`/`purl`/`cpe` from the override (not from the auto-derived values).
- **VR-077-003**: When `RootComponentOverride.is_active()` returns true, the per-format builders MUST filter out manifest-derived main-module components (identified by `mikebom:component-role = main-module` property) from the emitted `components[]` array per the 2026-05-06 clean-replacement clarification. The filter applies uniformly whether `name`, `version`, or both are set.
- **VR-077-004**: Override emission MUST be deterministic — same `RootComponentOverride` value + same scan target → byte-identical output across re-runs.

## Relationships

```text
mikebom sbom scan / mikebom trace run
    │
    ├── ScanArgs.root_name  : Option<String>   (clap --root-name)
    │   ScanArgs.root_version: Option<String>   (clap --root-version)
    │   (each parsed via `validate_root_field` value_parser)
    │
    └── ScanArtifacts.root_override : RootComponentOverride
            │
            ├── if .is_active() → filter out main-module components
            │                     before per-format build
            │
            └── per-format builders consume:
                ├── CDX  metadata.rs:42 build_metadata + cyclonedx/builder.rs:163 build
                │       → root component's name, version, bom-ref, purl, cpe all
                │         use override values (with percent_encode_purl_name on PURL)
                ├── SPDX 2.3  spdx/document.rs                                        ↓ same
                └── SPDX 3    spdx/v3_document.rs                                     ↓ same
```

## Backward compatibility

- No new `Cargo.toml` deps; no MSRV change; no nightly required.
- `RootComponentOverride` is `#[derive(Default)]` so existing `ScanArtifacts` constructions with `..Default::default()` syntax continue to compile.
- `ScanArgs.root_name` and `ScanArgs.root_version` default to `None`; operators not passing the flags get byte-identical pre-077 behavior.
- Existing milestone-073/074/075/076 byte-identity goldens stay byte-identical: no fixtures pass `--root-name` / `--root-version`, no fixtures hit the manifest-drop path.
- The new `percent_encode_purl_name` helper is NEW code; it doesn't replace the existing `url_friendly`. Non-override paths are unchanged.
- CPE format identical to alpha.17 for the no-flag case; only the name/version fields within the CPE differ when override fires.
