# Data Model — milestone 136

The brew reader does NOT introduce any new types in `mikebom-common`. Every brew-derived component flows through the existing `PackageDbEntry` → `ResolvedComponent` pipeline. The new types are reader-private serde-deserializing structs that mirror the subset of `INSTALL_RECEIPT.json` and the cask `Casks/<token>.json` mikebom consumes.

This document describes:

1. The reader-private `InstallReceipt` + `RuntimeDep` structs (formula parse intermediate).
2. The reader-private `CaskMetadata` struct (cask parse intermediate).
3. The `PackageDbEntry` field mapping for formulae and casks.
4. The integration with the cross-reader claim sets (NOT integrated for brew per milestone-136 deferral; documented for completeness).

## InstallReceipt (reader-private, formula)

The receipt parser's per-formula intermediate representation. Lives inside `mikebom-cli/src/scan_fs/package_db/brew.rs` only — NOT exposed across the crate boundary. Mirrors data-model.md's `InstallReceipt` schema choices from research §R2.

```rust
// Lives in mikebom-cli/src/scan_fs/package_db/brew.rs only.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields = false)]  // forward-compat: ignore unknown keys
#[allow(dead_code)]                    // not all parsed fields surface in v1
struct InstallReceipt {
    /// Homebrew version that wrote the receipt. Always present in modern
    /// receipts; informational only.
    #[serde(default)]
    homebrew_version: Option<String>,

    /// Source-provenance subdocument. May be absent on very old or
    /// path-installed formulae.
    #[serde(default)]
    source: Option<ReceiptSource>,

    /// Declared runtime dependencies. Added in 2016–2017; older receipts
    /// lack this field. When absent, the resulting component emits with
    /// an empty `depends` list (no dep graph for that formula — partial
    /// info is still valuable).
    #[serde(default)]
    runtime_dependencies: Vec<RuntimeDep>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ReceiptSource {
    /// Tap slug — "homebrew/core" for default tap, "<owner>/<tap>" for
    /// third-party. May be `null` on raw-path installs.
    #[serde(default)]
    tap: Option<String>,
    /// Spec discriminator: "stable" or "head". Informational only.
    #[serde(default)]
    spec: Option<String>,
    /// Tap repo's HEAD SHA at install time. Informational only.
    #[serde(default)]
    tap_git_head: Option<String>,
    /// Formula-file-level Git revision. Informational only.
    #[serde(default)]
    scm_revision: Option<String>,
    /// Versions subdocument carrying `stable` / `head` / etc. Not
    /// currently consumed — identity comes from the directory walk.
    #[serde(default)]
    versions: Option<serde_json::Value>,
    /// Absolute path to the formula's .rb file or cached API path.
    /// Informational only.
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct RuntimeDep {
    /// Tap-qualified dep name. For core deps: just the formula name
    /// (e.g., "openssl@3"). For third-party tap deps: full slug
    /// (e.g., "hashicorp/tap/terraform"). Required.
    full_name: String,
    /// Upstream version. Optional on older receipts.
    #[serde(default)]
    version: Option<String>,
    /// "<version>_<revision>" — round-trips the Cellar directory name.
    /// Preferred over `version` for PURL construction; falls back to
    /// `version` when absent.
    #[serde(default)]
    pkg_version: Option<String>,
    /// Homebrew rebuild counter.
    #[serde(default)]
    revision: Option<u32>,
    /// True when this dep is in the formula's own `depends_on` block;
    /// false when pulled transitively. Currently informational only;
    /// surfaced via the existing `lifecycle_scope` discipline could be
    /// a follow-up.
    #[serde(default)]
    declared_directly: Option<bool>,
}
```

### Validation rules

- `runtime_dependencies` entries with empty `full_name` are skipped (parser-side warn).
- Tap names like `"homebrew/core"` are treated as the default and trigger PURL `tap=` qualifier OMISSION per FR-003. All other tap values (including `null` → default) drop the qualifier.
- `pkg_version` is preferred over `version` for the dep target's PURL version segment when present; falls back to `version`; if both absent, the dep edge is skipped (cannot construct a valid PURL).

### Dep-name extraction (analysis-finding I1)

`runtime_dependencies[].full_name` is tap-qualified when the dep comes from a third-party tap (e.g., `"hashicorp/tap/terraform"`) and bare for core (e.g., `"openssl@3"`). The dep-resolver in `scan_fs/mod.rs::name_to_purl` matches against `PackageDbEntry.name`, which is ALWAYS the BARE name (the directory name in `Cellar/<formula>/<version>/`). Therefore the parser MUST normalize tap-qualified `full_name` to its bare form before recording the dep edge:

```rust
fn dep_bare_name(full_name: &str) -> &str {
    // Split on `/`; if 3+ segments, the last is the bare formula name;
    // segments [0..2] are the tap. If <3 segments, the whole string is
    // the bare name (a core-tap dep that omitted the tap prefix).
    full_name.rsplit('/').next().unwrap_or(full_name)
}
```

Examples:
- `"openssl@3"` → `"openssl@3"` (already bare)
- `"hashicorp/tap/terraform"` → `"terraform"`
- `"mongodb/brew/mongodb-community"` → `"mongodb-community"`

Without this normalization, third-party-tap dep edges silently fail to resolve (the resolver looks up `"hashicorp/tap/terraform"`, finds no component by that name, drops the edge). Closes analysis-finding I1.

### State transitions

None. `InstallReceipt` is constructed once per formula, consumed once to produce a `PackageDbEntry`, then dropped.

## CaskMetadata (reader-private, cask)

The cask parser's per-cask intermediate representation. Maps the subset of `Casks/<token>.json` mikebom consumes per research §R3.

```rust
// Lives in mikebom-cli/src/scan_fs/package_db/brew.rs only.
#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct CaskMetadata {
    /// Cask token (always matches the cask directory name).
    token: String,
    /// Installed version.
    version: String,
    /// Human-readable names; may carry multiple aliases.
    #[serde(default)]
    name: Vec<String>,
    /// Description; informational.
    #[serde(default)]
    desc: Option<String>,
    /// Upstream homepage URL; informational (NOT emitted as an external
    /// reference in v1 per the FR-012 deferral established in milestone
    /// 135 — see also Out-of-Scope).
    #[serde(default)]
    homepage: Option<String>,
    /// Download URL; informational.
    #[serde(default)]
    url: Option<String>,
    /// Download SHA-256; informational (NOT the installed-bytes hash).
    #[serde(default)]
    sha256: Option<String>,
    /// Cask DSL `depends_on` block — may declare formula deps but
    /// conventionally doesn't. NOT consumed in v1 (FR-005); casks
    /// emit without dep edges.
    #[serde(default)]
    depends_on: Option<serde_json::Value>,
    /// Installed artifacts (.app, .pkg, binary, etc.) — informational.
    #[serde(default)]
    artifacts: Vec<serde_json::Value>,
}
```

### Validation rules

- `token` and `version` MUST be present and non-empty. Missing either triggers warn-and-skip.
- Casks where `Casks/<token>.json` is absent and only `Casks/<token>.rb` exists trigger warn-and-skip per research §R3 — no Ruby parser per Principle I.

### State transitions

None. Same lifecycle as `InstallReceipt`.

## PackageDbEntry field mapping (formula)

Every brew-formula-derived `PackageDbEntry` populates the existing shared type's fields as follows:

| Field | Source | Notes |
|---|---|---|
| `purl` | `build_brew_purl(name=<dir-name>, version=<dir-name>, tap=<source.tap>, type=None)` | `pkg:brew/<name>@<version>[?tap=<owner>/<tap>]`; informal PURL type per R1 |
| `name` | Formula directory name (e.g., `"curl"`) | NOT from receipt — receipt has no top-level `name` |
| `version` | Formula version directory name (e.g., `"8.5.0"`) | NOT from receipt — version-segment lives in the path |
| `arch` | `None` | Homebrew is single-arch-per-install; arch info isn't user-facing |
| `source_path` | Absolute path to the formula's `INSTALL_RECEIPT.json` | Drives `ResolutionEvidence.source_file_paths` |
| `depends` | Dep names extracted from `runtime_dependencies[]` (each entry's `full_name` after stripping the tap prefix for default-tap deps) | Empty when receipt's `runtime_dependencies` array is absent |
| `maintainer` | `None` | Receipt has no maintainer field; Homebrew formulae are community-maintained |
| `licenses` | `Vec::new()` | License NOT in receipt (research §R2); out of scope for v1 per spec FR-011 deferral (analysis-finding U1). Extracting license requires Ruby parser (Principle I conflict) OR network call to `formulae.brew.sh` (FR-010 conflict). Tracked as follow-up. |
| `lifecycle_scope` | `None` | Homebrew has no dev/build/test distinction |
| `requirement_range` | `None` | Receipt pins exact version |
| `source_type` | `Some("brew")` | Identifier for downstream consumers |
| `evidence_kind` | `Some("brew-install-receipt")` | NEW value; add to the cyclonedx/builder.rs enum (T002b-equivalent) |
| `sbom_tier` | `Some("deployed")` | Same posture as dpkg/apk/rpm/alpm — installed-tier |
| `binary_class` / `binary_stripped` / `linkage_kind` / `detected_go` / `confidence` / `binary_packed` | `None` | N/A — these are binary-walker fields |
| `raw_version` | `None` | Could carry `pkg_version` (the Cellar `<ver>_<rev>` form) — deferred to follow-up; v1 keeps None |
| `parent_purl` | `None` | OS-package components are top-level |
| `npm_role` | `None` | N/A |
| `co_owned_by` | `None` | Cross-reader dual-ownership detection is a follow-up |
| `hashes` | `Vec::new()` | Receipt's `sha256` is the download hash, not the installed-bytes hash; deferred |
| `shade_relocation` | `None` | N/A |
| `extra_annotations` | `BTreeMap::new()` | No `mikebom:*` annotations introduced per Principle V (R1) |
| `binary_role` | `None` | N/A |
| `build_inclusion` | `None` | N/A |

### Validation rules (formula PackageDbEntry-level)

- `purl.as_str().starts_with("pkg:brew/")` for every emitted formula entry.
- `arch.is_none()` — Homebrew components don't carry an arch qualifier (matches the de-facto convention).
- `source_type.as_deref() == Some("brew")` for every brew entry.

## PackageDbEntry field mapping (cask)

Casks differ from formulae in only three ways. Same field mapping table, with these overrides:

| Field | Source | Notes |
|---|---|---|
| `purl` | `build_brew_purl(name=<cask-token>, version=<cask-version>, tap=<source-tap>, type=Some("cask"))` | `pkg:brew/<token>@<version>?type=cask[&tap=<owner>/<tap>]`; `type=cask` qualifier MUST be present for casks |
| `depends` | `Vec::new()` | Casks have no transitive dep graph per FR-005 |
| `evidence_kind` | `Some("brew-cask-metadata")` | Distinguishes cask provenance from formula receipt |

Other fields match the formula mapping.

### Cask validation rules

- `purl.as_str().contains("type=cask")` for every emitted cask entry.
- `depends.is_empty()` per FR-005.

## Cross-reader claim set contribution

**Not implemented in milestone 136 — deferred per spec Out-of-Scope.**

Following the alpm reader's pattern, a future `brew::collect_claimed_paths` would need to:

1. Walk each formula's Cellar contents (`<prefix>/Cellar/<formula>/<ver>/{bin,lib,share,etc}/`).
2. Resolve Homebrew's symlinks: `<prefix>/bin/<bin> → <prefix>/Cellar/<formula>/<ver>/bin/<bin>`, etc.
3. Register BOTH the Cellar-internal paths AND the resolved exposed paths into `claimed`.
4. Handle keg-only formulae (no `<prefix>/bin/` exposure).

This is a meaningfully larger surface than the alpm reader's flat `%FILES%` parse and warrants its own spec. The milestone-136 PR ships WITHOUT this integration — operators may see `pkg:generic/curl` duplicates alongside `pkg:brew/curl@8.5.0` on rootfs scans where the binary walker fires. Acceptable v1 limitation; tracked as a sibling follow-up issue post-merge.

## Integration with existing types

- `Purl` — `mikebom_common::types::purl::Purl::new` is the only entry point; constructs validated `pkg:brew/...` strings. New reader-local helper `build_brew_purl(name, version, tap, kind) -> Result<Purl>` encapsulates the qualifier rules (omit `tap=` for default `homebrew/core`; always emit `type=cask` for casks).
- `evidence_kind` enum gate in `mikebom-cli/src/generate/cyclonedx/builder.rs` — must be extended with two new values `"brew-install-receipt"` and `"brew-cask-metadata"` per the alpm-reader (milestone 135) precedent. Single-line additions to the `matches!` arm plus diagnostic-string update.
- `ResolvedComponent` — the existing conversion in `mikebom-cli/src/scan_fs/mod.rs:637` (where `PackageDbEntry.extra_annotations.clone()` flows into `ResolvedComponent.extra_annotations`) Just Works for brew — there's nothing brew-specific to thread through (no extra annotations emitted).
- `ScanDiagnostics` — brew doesn't introduce any new diagnostic fields; per-formula warn-and-skip is logged via `tracing::warn!` and doesn't surface in the document-level diagnostic bag.

## Out-of-scope data shapes

- **License extraction** from formula `.rb` source or cached API JSON: deferred to a follow-up parallel to the milestone-135 FR-012 deferral.
- **Homepage URL emission** as a wire-level external reference: same deferral as licenses.
- **Cask `depends_on.formula` dep graph**: out of scope for v1 (rarely populated in real-world casks).
- **`raw_version`** carrying the Cellar `<version>_<revision>` form: deferred — `version` from the directory name is sufficient identity.
- **`hashes`** populated from the receipt's `sha256` (download hash) — semantically incorrect for installed-bytes hash discipline. Deferred until proper installed-bytes hashing infrastructure exists.
- **File-claim tracker integration**: deferred per spec Out-of-Scope; symlink resolution is a separate spec.
