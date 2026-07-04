# Data Model: Milestone 159

**Date**: 2026-07-04
**Feature**: [spec.md](./spec.md)

Phase-1 data structures for the alias-detection + edge-rewrite pipeline. All types live in `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs` (new submodule).

## Entities

### `AliasResolution` (public struct)

The canonical output of alias detection — carries all the information needed for component emission + edge rewrite + annotation.

```rust
/// Result of parsing a lockfile alias entry.
///
/// Emitted for both:
///   - Pnpm value-side aliases: `react-helmet-async: '@slorber/react-helmet-async@1.3.0(peers)'`
///   - Yarn v1 key-side aliases: `"@cosmograph/cosmos@^1.1.1", "@cosmograph/cosmos@npm:@cosmos.gl/graph":`
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AliasResolution {
    /// The dep name as it appears in the depender's lockfile section.
    /// Preserved byte-identically from the lockfile (no URL-encoding,
    /// no case normalization). Emitted as the raw-string value of
    /// `mikebom:pnpm-alias` / `mikebom:yarn-alias` per FR-006.
    pub(crate) local_name: String,

    /// The actual npm-registry package name that the local name
    /// resolves to. Used as the emitted component's `name` field.
    pub(crate) aliased_name: String,

    /// The resolved version of the aliased package. Used as the
    /// emitted component's `version` field.
    pub(crate) aliased_version: String,

    /// The pnpm peer-suffix (`(react-dom@18.3.1(react@18.2.0))`) if
    /// present in the source lockfile value. Discarded per Q1
    /// clarification but retained in the type for future use (audit
    /// tooling, debugging). Not emitted anywhere in v1.
    pub(crate) pnpm_peer_suffix: Option<String>,

    /// Which lockfile ecosystem the alias originated from. Drives
    /// the annotation-name choice at emit-time:
    ///   - Pnpm → `mikebom:pnpm-alias`
    ///   - YarnV1 → `mikebom:yarn-alias`
    pub(crate) ecosystem: AliasEcosystem,
}
```

### `AliasEcosystem` (enum)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AliasEcosystem {
    Pnpm,
    YarnV1,
}

impl AliasEcosystem {
    /// Wire annotation name per FR-006.
    pub(crate) fn annotation_name(&self) -> &'static str {
        match self {
            Self::Pnpm => "mikebom:pnpm-alias",
            Self::YarnV1 => "mikebom:yarn-alias",
        }
    }
}
```

### `AliasMap` (type alias)

```rust
/// Per-lockfile alias-resolution table. Built during the first
/// pass over a lockfile; consumed during the second pass when
/// rewriting dep edges (FR-005).
///
/// Key: local-name (as used by other deps in the same lockfile).
/// Value: aliased canonical identity (name + version).
pub(crate) type AliasMap = std::collections::HashMap<String, AliasedIdentity>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AliasedIdentity {
    pub(crate) aliased_name: String,
    pub(crate) aliased_version: String,
}
```

## Public API

```rust
// mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs

/// Detect an alias in a pnpm-lock value string. Returns Some(alias)
/// when the value's canonical name differs from the local-name;
/// None when they match (no alias) or when the value fails to parse.
///
/// Handles both quoted and unquoted value shapes.
pub(crate) fn detect_pnpm_alias(
    local_name: &str,
    raw_value: &str,
) -> Option<AliasResolution>;

/// Detect an alias in a yarn v1 key line (comma-joined descriptor
/// list). Returns Some(alias) when any comma-separated spec uses
/// the `npm:` prefix; None otherwise.
///
/// The `resolved_version` param is the entry's `version:` value from
/// the yarn.lock body — required because the yarn v1 key doesn't
/// carry the resolved version, only the version-range spec.
pub(crate) fn detect_yarn_v1_alias(
    key_line: &str,
    resolved_version: &str,
) -> Option<AliasResolution>;

/// Rewrite a dep-name list to use aliased canonical names. Applied
/// to `PackageDbEntry.depends: Vec<String>` after alias detection
/// completes for a lockfile.
pub(crate) fn rewrite_dep_names(
    dep_names: &[String],
    alias_map: &AliasMap,
) -> Vec<String>;
```

## Validation Rules

- `AliasResolution.local_name` MUST NOT equal `AliasResolution.aliased_name` (detection is guarded against no-op self-aliases).
- `AliasResolution.aliased_version` MUST be non-empty; the yarn v1 case reads it from the entry's `version:` line, and the pnpm case parses it from the value string via `parse_pnpm_key`.
- `AliasEcosystem::annotation_name()` maps to exactly one of the two documented annotation names — no other values.
- `AliasMap` keys are byte-identical to lockfile-source local-names; no normalization.
- `rewrite_dep_names` MUST preserve any dep-name NOT in `alias_map` byte-identically (only aliased entries are rewritten).

## Relationships

```text
lockfile source
    ↓
Parser (pnpm_lock.rs OR yarn_lock.rs)
    │
    ├── First pass: for each entry, call detect_pnpm_alias / detect_yarn_v1_alias
    │       ↓
    │   Populate AliasMap: local-name → AliasedIdentity
    │       ↓
    │   Collect Vec<AliasResolution> for annotation emission
    │
    └── Second pass: for each PackageDbEntry
            ↓
        rewrite_dep_names(entry.depends, &alias_map) → aliased edges
            ↓
        entry.depends = rewritten list
            ↓
        Component with aliased-identity PURL + mikebom:*-alias annotation → emitted SBOM
```

## State Transitions

Not applicable — all types are value types produced once per lockfile-parse and consumed immediately. No mutation, no lifecycle.

## Existing Types Extended (not created here)

- **`PackageDbEntry`** (in `mikebom-cli/src/scan_fs/package_db/mod.rs`): milestone 159 adds an optional `extra_annotations` entry keyed `mikebom:pnpm-alias` OR `mikebom:yarn-alias` with a raw string value (the local-name). This uses the existing extra_annotations mechanism from milestone-127 / milestone-134 / milestone-158; no new struct field.
- **`ResolvedComponent`** (in `mikebom-common/src/resolution.rs`): unchanged. The alias annotation flows through the existing `extra_annotations: BTreeMap<String, Value>` field per the milestone-127 pattern.
