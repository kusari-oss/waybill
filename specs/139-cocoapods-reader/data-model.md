# Data Model — milestone 139

The CocoaPods reader does NOT introduce any new types in `mikebom-common`. Every CocoaPods-derived component flows through the existing `PackageDbEntry` → `ResolvedComponent` pipeline. The new types are reader-private serde-deserializing structs that mirror the subset of `Podfile.lock` (+ `Podfile` regex-extracted records) the reader consumes.

## PodfileLockDoc (reader-private, lockfile)

The lockfile parser's intermediate representation. Note the `PODS:` field uses `serde_yaml::Value` rather than a typed enum — entries are heterogeneous (bare string OR single-key map per Phase 0 research) and post-parse dispatch is simpler than a `#[serde(untagged)]` enum.

```rust
#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct PodfileLockDoc {
    /// `PODS:` — array of pod-spec entries. Each element is either a
    /// bare YAML string (no transitive deps) OR a single-key map
    /// (key = pod-spec string, value = transitive-dep array). Use
    /// post-parse dispatch on `serde_yaml::Value` to distinguish.
    #[serde(default, rename = "PODS")]
    pods: Vec<serde_yaml::Value>,
    /// `DEPENDENCIES:` — array of direct-dep names with optional version
    /// constraints (e.g., `"AFNetworking (~> 4.0)"` or just `"Firebase/Core"`).
    /// Drives FR-004 main-module dep-edge attribution.
    #[serde(default, rename = "DEPENDENCIES")]
    dependencies: Vec<String>,
    /// `EXTERNAL SOURCES:` — per-pod source overrides. Map keyed by
    /// pod name; values are Ruby-symbol-keyed maps (`:git`/`:path`/
    /// `:branch`/`:tag`/`:commit`/`:podspec` per Phase 0 research).
    /// Drives FR-003 source-type discrimination.
    #[serde(default, rename = "EXTERNAL SOURCES")]
    external_sources: std::collections::BTreeMap<String, serde_yaml::Value>,
    /// `CHECKOUT OPTIONS:` — per-pod RESOLVED git SHAs written by
    /// `pod install` (per Q2 + Phase 0 research). Includes the
    /// resolved 40-char SHA in `:commit` even when Podfile specified
    /// `:branch`/`:tag`. Drives `mikebom:vcs-ref` annotation for
    /// git-source pods.
    #[serde(default, rename = "CHECKOUT OPTIONS")]
    checkout_options: std::collections::BTreeMap<String, serde_yaml::Value>,
    /// `SPEC CHECKSUMS:` — SHA-1 of each pod's podspec, hex-encoded.
    /// ROOT-keyed (subspecs share the parent's checksum) per Phase 0
    /// research. Drives FR-008 SHA-1 hash emission.
    #[serde(default, rename = "SPEC CHECKSUMS")]
    spec_checksums: std::collections::BTreeMap<String, String>,
    /// Informational; not consumed v1.
    #[serde(default, rename = "PODFILE CHECKSUM")]
    podfile_checksum: Option<String>,
    /// Informational; not consumed v1.
    #[serde(default, rename = "COCOAPODS")]
    cocoapods: Option<String>,
}
```

### Validation rules

- `pods` MAY be empty (fresh `pod install` failure); reader emits just the main-module per R7.
- Per-entry malformed (PODS element neither string nor single-key map) → warn + skip that single entry per R7.
- `external_sources` values are heterogeneous Ruby-symbol-keyed maps; access via `value.get("git")` / `value.get(":git")` (YAML rendering of Ruby symbols varies — both forms accepted via fallback chain).

## PodsEntry (parsed PODS element)

Post-`serde_yaml::Value` dispatch shape:

```rust
#[derive(Debug, Clone)]
struct PodsEntry {
    /// Pod name (may contain `/` for subspecs — `Firebase/Core`).
    name: String,
    /// Pinned version (parenthesized form `(4.0.1)` stripped by parser).
    version: String,
    /// Transitive dep names (informational v1; transitive edges deferred to v1.1).
    #[allow(dead_code)]
    transitive_deps: Vec<String>,
}

impl PodsEntry {
    /// Returns Some(root) when name contains `/`, None for non-subspec pods.
    fn root_pod_name(&self) -> Option<&str> {
        self.name.split_once('/').map(|(r, _)| r)
    }

    /// Returns Some(subpath) when name contains `/`, None for non-subspec.
    /// For `Firebase/Database/Realtime` → `Some("Database/Realtime")`.
    fn subpath(&self) -> Option<&str> {
        self.name.split_once('/').map(|(_, s)| s)
    }
}
```

### Parser (`fn parse_pods_entry(value: &serde_yaml::Value) -> Option<PodsEntry>`)

Dispatches on `Value` shape:
- `Value::String(s)` — parse `s` via `parse_pod_spec_string` (extract `"Name (version)"`).
- `Value::Mapping(m)` with exactly 1 key — key is the pod-spec string; value is the transitive-deps array.
- Other shapes → warn + None.

`parse_pod_spec_string` regex: `^(?P<name>[^ ]+) \((?P<version>[^)]+)\)$`.

## PodfileTargetInfo (regex-extracted from Podfile)

```rust
#[derive(Debug, Clone, Default)]
struct PodfileTargetInfo {
    /// First `target '<name>' do` block name; FR-012 main-module derivation.
    first_target_name: Option<String>,
    /// `pod 'Name' [, '<constraint>']` declarations across the whole file
    /// (target nesting flattened per R3). Used in design-tier emission.
    declared_pods: Vec<DeclaredPod>,
}

#[derive(Debug, Clone)]
struct DeclaredPod {
    name: String,
    /// First-positional version constraint (`'~> 4.0'`) when present.
    constraint: Option<String>,
}
```

### Regex patterns

- Target: `(?m)^\s*target\s+['"]([^'"]+)['"]\s+do\b`
- Pod: `(?m)^\s*pod\s+['"]([^'"]+)['"](?:\s*,\s*['"]([^'"]+)['"])?`

Comments (`#`) stripped per-line before regex match.

## PackageDbEntry field mapping (per source type)

### Common fields (all source types)

| Field | Source | Notes |
|---|---|---|
| `name` | PODS entry's pod-name (case-preserved verbatim per purl-spec — CocoaPods is case-sensitive) | Includes subspec path (`Firebase/Core` literal) |
| `version` | PODS entry's parenthesized version, parentheses stripped | Verbatim |
| `arch` | `None` | N/A for source-tree language reader |
| `source_path` | Absolute path to the owning `Podfile.lock` / `Podfile` / `Manifest.lock` | Drives `ResolutionEvidence.source_file_paths` |
| `maintainer` | `None` | Not consumed v1 |
| `lifecycle_scope` | `Some(LifecycleScope::Runtime)` for all pods | CocoaPods doesn't carry runtime/dev classification at the lockfile level; per-target attribution deferred to v1.1 |
| `requirement_range` | `None` for lockfile-derived; `Some(constraint)` for design-tier per FR-005 | |
| `evidence_kind` | `"cocoapods-podfile-lock"` (source-tier) / `"cocoapods-podfile"` (design-tier) / `"cocoapods-manifest-lock"` (deployed-tier per Q3) | NEW values added to cyclonedx/builder.rs enum |
| `sbom_tier` | `"source"` (lockfile-derived) / `"design"` (FR-005) / `"deployed"` (Manifest.lock-only per Q3) | |
| `binary_class` / `binary_stripped` / `linkage_kind` / `detected_go` / `confidence` / `binary_packed` | `None` | N/A |
| `raw_version` / `parent_purl` / `npm_role` / `co_owned_by` / `shade_relocation` / `binary_role` / `build_inclusion` | `None` | N/A |
| `licenses` | `Vec::new()` | License deferred per spec Out-of-Scope |
| `extra_annotations` | Source-type discriminator + source-specific extras (see per-source-type rows) | |

### Per-source-type fields

| Source | `purl` | `source_type` | `extra_annotations` extras | `hashes` |
|---|---|---|---|---|
| **trunk** (no EXTERNAL SOURCES entry) | `pkg:cocoapods/<pod>@<version>` (for subspec: `pkg:cocoapods/<root>@<version>#<subpath>`) | `Some("cocoapods-trunk")` | `mikebom:source-type = "cocoapods-trunk"`; for subspec: `mikebom:subspec = "<subpath>"` (informational; redundant with PURL but easier to query) | When `spec_checksums.get(root_pod_name)` returns 40-char hex: `vec![ContentHash::with_algorithm(HashAlgorithm::Sha1, hex)]` per FR-008 (ROOT-keyed lookup); else empty |
| **git** (EXTERNAL SOURCES entry with `:git`) | `pkg:cocoapods/<pod>@<version>?vcs_url=git+<url>` | `Some("cocoapods-git")` | `mikebom:source-type = "cocoapods-git"`; `mikebom:vcs-ref = "<resolved-sha-from-CHECKOUT-OPTIONS>"` when present; `mikebom:vcs-declared-ref = "<operator-declared-ref-from-EXTERNAL-SOURCES>"` when distinct from resolved | Empty (git source has no podspec checksum in SPEC CHECKSUMS for non-trunk pods) |
| **path** (EXTERNAL SOURCES entry with `:path`) | `pkg:generic/<flattened-pod>@<version>` (placeholder per R1; `<flattened-pod>` = pod name with `/` replaced by `-` per I2 remediation, matching milestone-138 composer convention — avoids `pkg:generic/<namespace>/<name>` ambiguity) | `Some("cocoapods-path")` | `mikebom:source-type = "cocoapods-path"`; `mikebom:path = "<EXTERNAL-SOURCES-path-value>"`; for path-sourced subspecs, also `mikebom:subspec = "<original-subspec-path>"` for original-form recovery | Empty |

### Main-module field mapping (per FR-012 + Q1 cascade)

For each iOS project root (parent of `Podfile.lock` / `Podfile` / `Manifest.lock`):

| Field | Value |
|---|---|
| `purl` | `pkg:cocoapods/<app-name>@0.0.0-unknown` |
| `name` | App-name per Q1 cascade: (1) first `target '<name>' do` block in Podfile, (2) parent-dir basename fallback |
| `version` | `"0.0.0-unknown"` (CocoaPods doesn't carry a project-level version) |
| `source_path` | Absolute path to whichever artifact triggered emission (`Podfile.lock` preferred) |
| `evidence_kind` | `Some("cocoapods-podfile")` (matches the manifest that derived the name) |
| `sbom_tier` | `Some("source")` (matches sibling lockfile; or `Some("deployed")` when Manifest.lock-only per Q3) |
| `source_type` | `Some("cocoapods-main-module")` |
| `extra_annotations` | `mikebom:component-role = "main-module"` + `mikebom:source-type = "cocoapods-main-module"` |
| `depends` | Names from lockfile's `DEPENDENCIES:` array (lockfile mode) OR from Podfile's `pod` declarations (design-tier mode) |

### Dep edges

- **Lockfile mode**: main-module's `depends` populated from `DEPENDENCIES:` array names (lockfile pre-resolves them; the PODS entry for each name is the lockfile-pinned version). Transitive edges deferred to v1.1 per FR-004.
- **Design-tier mode**: main-module's `depends` populated from `Podfile`'s `pod` declarations.
- **Deployed-tier mode (Manifest.lock-only)**: same as lockfile mode; uses Manifest.lock's `DEPENDENCIES:`.

## Validation invariants (per spec FR-* + Constitution)

- `purl.as_str().starts_with("pkg:cocoapods/")` OR `purl.as_str().starts_with("pkg:generic/")` for every emitted CocoaPods entry.
- For subspec entries (name contains `/`): `purl.as_str().contains('#')`.
- `source_type` value MUST be one of `cocoapods-trunk` / `cocoapods-git` / `cocoapods-path` / `cocoapods-main-module`.
- `evidence_kind` MUST be one of `cocoapods-podfile-lock` / `cocoapods-podfile` / `cocoapods-manifest-lock`.
- For git entries with CHECKOUT OPTIONS present: `extra_annotations.get("mikebom:vcs-ref").and_then(|v| v.as_str())` returns a 40-char hex string.
- SHA-1 hashes attached to subspec components share the parent root pod's checksum (root-keyed per FR-008).

## Out-of-scope data shapes

- License extraction from per-pod podspec files in `Pods/<PodName>/<PodName>.podspec` (cross-reader follow-up).
- `Pods/<pod>/` directory walking for deployed-tier evidence (spec Out-of-Scope).
- Private CocoaPods spec repo provenance via `SPEC REPOS:` (spec Out-of-Scope; deferred).
- Per-target dep attribution from multi-target Podfiles (FR-010; deferred to v1.1).
- Transitive dep edges from PODS-entry sub-arrays (FR-004; deferred to v1.1).
- syft/trivy compatibility shape (`pkg:cocoapods/Firebase/Database@1.0.0` name-folded form) via `mikebom:also-known-as` annotation (R1; deferred to v1.1).
- Pre-1.0 lockfile format (spec Out-of-Scope; warn-and-skip on detection — no `SPEC CHECKSUMS:` section is the sentinel).
