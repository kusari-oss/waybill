# Data Model — milestone 138

The Composer reader does NOT introduce any new types in `mikebom-common`. Every Composer-derived component flows through the existing `PackageDbEntry` → `ResolvedComponent` pipeline. The new types are reader-private serde-deserializing structs that mirror the subset of `composer.json`, `composer.lock`, and `vendor/composer/installed.json` the reader consumes.

## ComposerJson (reader-private, manifest)

The manifest parser's per-project intermediate representation. Lives inside `mikebom-cli/src/scan_fs/package_db/composer.rs` only.

```rust
#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ComposerJson {
    /// `name:` — required for any composer.json that emits a main-module
    /// per FR-012. Form: `<vendor>/<package>`. May be absent for private
    /// applications (Q3 clarification).
    #[serde(default)]
    name: Option<String>,
    /// `version:` — optional. Composer infers from VCS tags at install
    /// time when absent; many application projects simply don't declare.
    /// Falls back to `"0.0.0-unknown"` per the cargo (milestone 064)
    /// main-module convention.
    #[serde(default)]
    version: Option<String>,
    /// `type:` — informational; not consumed for emission.
    #[serde(default)]
    r#type: Option<String>,
    /// `description:` — informational; not consumed.
    #[serde(default)]
    description: Option<String>,
    /// `require:` — declared direct runtime deps (constraint strings).
    /// Used in design-tier mode (no lockfile) per FR-005 and for
    /// main-module dep-edge wiring per FR-004.
    #[serde(default)]
    require: std::collections::BTreeMap<String, String>,
    /// `require-dev:` — declared dev-only deps. Used in design-tier
    /// mode + tagged with `lifecycle-scope = Development` per FR-005.
    #[serde(default, rename = "require-dev")]
    require_dev: std::collections::BTreeMap<String, String>,
}
```

### Validation rules

- `name` MUST be non-empty AND contain `/` (vendor/package form) for the project to emit a main-module component. Otherwise warn-and-skip main-module per Q3 clarification.
- `version` MAY be absent; falls back to `"0.0.0-unknown"`.
- Dep-constraint values are always scalar strings in composer.json (no map form like Dart's `path:`/`git:` directives — those go in the separate `repositories:` block, which we don't consume v1).

## ComposerLock + LockfilePackage (reader-private, lockfile)

```rust
#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ComposerLock {
    /// `packages:` — runtime deps. Always present in a valid Composer 2
    /// lockfile (may be empty array).
    #[serde(default)]
    packages: Vec<LockfilePackage>,
    /// `packages-dev:` — dev-only deps. May be null (operator-omitted)
    /// or empty array (no dev-deps declared).
    #[serde(default, rename = "packages-dev")]
    packages_dev: Option<Vec<LockfilePackage>>,
    /// `plugin-api-version:` — Composer 2 sentinel; not consumed v1.
    #[serde(default, rename = "plugin-api-version")]
    plugin_api_version: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct LockfilePackage {
    /// `name:` — required, `<vendor>/<package>` form.
    name: String,
    /// `version:` — required (verbatim from upstream; preserves `v` prefix
    /// for git-tag versions).
    version: String,
    /// `type:` — required. `library` / `metapackage` / `composer-plugin` /
    /// `composer-installer` / open-ended `[a-z0-9-]+`.
    #[serde(default = "default_type")]
    r#type: String,
    /// `source:` — present for VCS / path sources. May be absent for pure
    /// `dist`-only entries (rare; some metapackages).
    #[serde(default)]
    source: Option<LockfileSource>,
    /// `dist:` — present for Packagist / dist-shaped sources. May be
    /// absent for `source.type: path` entries (no downloadable artifact).
    #[serde(default)]
    dist: Option<LockfileDist>,
    /// `require:` — informational v1 (transitive edges deferred to v1.1
    /// per FR-004).
    #[serde(default)]
    require: std::collections::BTreeMap<String, String>,
    /// `license:` — polymorphic per composer-schema.json: string OR
    /// array. Handled via `#[serde(untagged)]` LockfileLicense enum.
    /// Not consumed for emission v1 per spec Out-of-Scope (license
    /// extraction deferred); preserved in struct for future use.
    #[serde(default)]
    license: Option<LockfileLicense>,
}

fn default_type() -> String { "library".to_string() }

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[allow(dead_code)]
struct LockfileSource {
    /// VCS driver name (`git` / `svn` / `hg`) OR `path` for local
    /// filesystem sources. NOT enum-restricted in composer-schema.json;
    /// unknown values trigger warn-and-skip per R7.
    #[serde(default)]
    r#type: Option<String>,
    /// VCS remote URL OR path-source path (relative to project root,
    /// preserved verbatim).
    #[serde(default)]
    url: Option<String>,
    /// VCS resolved SHA (always 40-char hex for git; svn/hg formats vary).
    /// Surfaced as `mikebom:vcs-ref` evidence rather than PURL version
    /// segment per R4.
    #[serde(default)]
    reference: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[allow(dead_code)]
struct LockfileDist {
    #[serde(default)]
    r#type: Option<String>,
    /// Packagist download URL OR self-hosted mirror URL.
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    reference: Option<String>,
    /// SHA-1 of the downloaded zip per FR-013.
    #[serde(default)]
    shasum: Option<String>,
}

/// composer-schema.json: `"license": { "type": ["string", "array"] }`.
/// Untagged enum lets serde try each variant in declared order.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
enum LockfileLicense {
    Single(String),
    List(Vec<String>),
}
```

### Validation rules

- `name` MUST be non-empty AND contain exactly one `/` (vendor/package form); otherwise warn-and-skip that single entry per R7.
- `version` MUST be non-empty; otherwise warn-and-skip per R7.
- `source.type` MUST be one of `git` / `svn` / `hg` / `path` when present; unknown values warn-and-skip per R7.
- For `source.type: git` (most common VCS), `source.reference` MUST be present + 40-char hex; absent triggers warn-and-skip.
- For `source.type: path`, `source.url` is treated as a path (relative or absolute, verbatim from lockfile); no validation beyond non-empty.

## InstalledJson (reader-private, deployed-tier)

```rust
#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct InstalledJson {
    /// Composer 2 wrapper field. The inner entries mirror LockfilePackage
    /// shape exactly (per spec § Key Entities).
    packages: Vec<LockfilePackage>,
    /// `dev: true|false` — informational (matches `dev-package-names`
    /// truthiness; not consumed for per-entry classification).
    #[serde(default)]
    dev: bool,
    /// Authoritative dev-classifier for installed-tier emission per FR-009.
    /// Entries in this list (by `name`) get `lifecycle-scope = Development`.
    #[serde(default, rename = "dev-package-names")]
    dev_package_names: Vec<String>,
}
```

### Validation rules

- A top-level JSON array (Composer 1 format) MUST trigger warn-and-skip per R3 + spec Out-of-Scope. Detect by attempting `serde_json::from_slice::<serde_json::Value>` first and checking `.is_array()` before the typed parse.
- `packages` MUST be present (the only required field for Composer 2 shape).
- `dev_package_names` MAY be empty (no dev-only deps installed).

## PackageDbEntry field mapping (per source type)

### Common fields (all source types)

| Field | Source | Notes |
|---|---|---|
| `name` | Lockfile/installed entry's `name` field, OR `composer.json::name` for main-module | Verbatim (NOT lowercased — PURL is the lowercased form, but `name` field preserves source case for display) |
| `version` | Lockfile/installed entry's `version` field, OR `composer.json::version` for main-module | Verbatim |
| `arch` | `None` | Composer components are architecture-independent |
| `source_path` | Absolute path to the owning `composer.lock` / `composer.json` / `installed.json` | Drives `ResolutionEvidence.source_file_paths` |
| `maintainer` | `None` | Not consumed v1 (lockfile `authors[]` is deferred) |
| `lifecycle_scope` | `Runtime` for `packages[]` entries; `Development` for `packages-dev[]` entries OR `installed.json::dev-package-names[]` matches; `None` for design-tier non-dev entries | Per FR-009 |
| `requirement_range` | `None` for lockfile/installed-tier entries; `Some(constraint-string)` for design-tier per FR-005 | |
| `evidence_kind` | `Some("composer-lock")` for lockfile-derived; `Some("composer-json")` for design-tier; `Some("composer-installed-json")` for deployed-tier | NEW values added to cyclonedx/builder.rs enum |
| `sbom_tier` | `Some("source")` for lockfile-derived; `Some("design")` for design-tier (FR-005); `Some("deployed")` for installed.json (FR-006) | |
| `binary_class` / `binary_stripped` / `linkage_kind` / `detected_go` / `confidence` / `binary_packed` | `None` | N/A for source-tree language reader |
| `raw_version` | `None` | |
| `parent_purl` | `None` | Top-level (lockfile entries are flat) |
| `npm_role` / `co_owned_by` / `shade_relocation` / `binary_role` / `build_inclusion` | `None` | N/A |
| `licenses` | `Vec::new()` | License deferred per spec Out-of-Scope (mirrors milestone-135 FR-012 + 136 FR-011 + 137 deferrals) |
| `extra_annotations` | Source-type discriminator (see per-source-type rows) | See below |

### Per-source-type fields

| Source | `purl` | `source_type` (in `PackageDbEntry.source_type`) | `extra_annotations` extras | `hashes` |
|---|---|---|---|---|
| **packagist (default)** | `pkg:composer/<lc-vendor>/<lc-package>@<version>` | `Some("composer-packagist")` | `mikebom:source-type = "composer-packagist"` | When `dist.shasum` present (Composer's lockfile carries SHA-1 inline for Packagist entries): `vec![ContentHash::sha1(<hex>)]` (FR-013); else empty |
| **packagist (self-hosted)** | `pkg:composer/<lc-vendor>/<lc-package>@<version>?repository_url=<url>` | `Some("composer-packagist")` | `mikebom:source-type = "composer-packagist"` | Same as above |
| **vcs (git/svn/hg)** | `pkg:composer/<lc-vendor>/<lc-package>@<version>?vcs_url=<scheme>+<url>` | `Some("composer-vcs")` | `mikebom:source-type = "composer-vcs"`; `mikebom:vcs-ref = "<source.reference>"` (the resolved 40-char SHA) | Empty (VCS source has no download hash; `dist.shasum` may be absent for vcs-shaped entries) |
| **path** | `pkg:generic/<lc-vendor>-<lc-package>@<version>` (vendor+name flattened with `-`) | `Some("composer-path")` | `mikebom:source-type = "composer-path"`; `mikebom:path = "<source.url>"` | Empty |
| **composer-plugin** (`type: composer-plugin` or `composer-installer`) | Standard Packagist form per top row | `Some("composer-plugin")` | `mikebom:source-type = "composer-plugin"`; `mikebom:composer-type = "<type-field-verbatim>"` (preserves `composer-plugin` vs legacy `composer-installer` distinction) | Same as packagist (these are still Packagist-hosted) |
| **metapackage** (`type: metapackage`) | Standard Packagist form per top row | `Some("composer-metapackage")` | `mikebom:source-type = "composer-metapackage"` | Empty (metapackages have no downloadable artifact) |

### Deployed-tier (installed.json) additional fields

When emitted from `vendor/composer/installed.json`:
- `sbom_tier = Some("deployed")` (not `"source"`)
- `evidence_kind = Some("composer-installed-json")`
- `source_path` points to the `installed.json` file (not lockfile)
- All other per-source-type fields per the above table

When the installed.json entry is **orphan** (sibling `composer.lock` EXISTS but doesn't contain the entry's name+version) per Q1 clarification:
- Additional `extra_annotations["mikebom:lockfile-orphan"] = serde_json::Value::String("true".into())` (string value per CycloneDX 1.6 `componentProperty.value` wire-format constraint — `extra_annotations` is `BTreeMap<String, serde_json::Value>` so the value type is flexible, but downstream CDX emission requires string-coerced output)

When NO sibling `composer.lock` exists for the project (deployed-tier-only scan — e.g., container image stripped of manifests), the orphan annotation is NOT emitted; entries emit as standard `deployed`-tier components.

### Main-module field mapping (per FR-012)

For each scanned `composer.json` with non-empty `name:` field, one additional `PackageDbEntry` emits with:

| Field | Value |
|---|---|
| `purl` | `pkg:composer/<lc-vendor>/<lc-package>@<composer.json.version-or-"0.0.0-unknown">` |
| `name` | `composer.json::name` (verbatim — NOT lowercased; the PURL is the lowercased form) |
| `version` | `composer.json::version` (or `"0.0.0-unknown"` fallback) |
| `source_path` | Absolute path to `composer.json` |
| `evidence_kind` | `Some("composer-json")` |
| `sbom_tier` | `Some("source")` |
| `source_type` | `Some("composer-main-module")` |
| `extra_annotations` | `mikebom:component-role = "main-module"` + `mikebom:source-type = "composer-main-module"` |
| `depends` | Names of direct deps from the project's lockfile (per FR-004) OR from `composer.json::require` + `require-dev` in design-tier mode |

### Dep edges

- **Lockfile mode**: main-module's `depends` is populated from `composer.json::require` keys (+ `require-dev` keys; the post-resolution filter handles `--exclude-scope dev`). The lockfile's per-entry `require:` arrays are NOT consumed for inter-package edges (transitive edges deferred to v1.1 per FR-004).
- **Design-tier mode**: main-module's `depends` populated from `composer.json::require` + `require-dev` keys directly.
- **Deployed-tier mode**: no main-module emission (installed.json doesn't carry a project-root component); per-entry edges deferred to v1.1.

## Validation invariants (per spec FR-* + Constitution)

- `purl.as_str().starts_with("pkg:composer/")` OR `purl.as_str().starts_with("pkg:generic/")` for every emitted Composer entry.
- For `pkg:composer/` entries: vendor + name segments MUST be lowercased per purl-spec canonical form.
- `source_type` value MUST be one of `composer-packagist` / `composer-vcs` / `composer-path` / `composer-plugin` / `composer-metapackage` / `composer-main-module`.
- `evidence_kind` MUST be one of `composer-lock` / `composer-json` / `composer-installed-json` (the cyclonedx/builder.rs enum extension).
- For VCS entries: `purl.as_str().contains("?vcs_url=")`.
- For deployed-tier orphan entries (sibling lockfile exists but lacks the entry): `extra_annotations.get("mikebom:lockfile-orphan").and_then(|v| v.as_str()) == Some("true")`.

## Out-of-scope data shapes

- License extraction from lockfile's `license:` field (cross-reader follow-up; mirrors milestone-135 FR-012 + 136 FR-011 + 137 deferrals).
- Per-package `authors[]` / `homepage` / `description` extraction (same deferral).
- `composer.json::autoload.psr-4` / `psr-0` namespace emission (spec Out-of-Scope explicitly).
- Pre-Composer-2 lockfile + installed.json formats (rare in 2026; explicitly out of spec scope; warn-and-skip on detection).
- Transitive dep edges from individual lockfile entries' `require:` maps — v1 emits main-module → direct deps only; transitive components surface but their inter-edges are deferred to v1.1.
- `repositories[]` block in `composer.json` (custom repository declarations beyond Packagist). The lockfile's per-entry `dist.url` / `source.url` is the authoritative discriminator at install time; design-tier mode doesn't currently resolve repository overrides.
