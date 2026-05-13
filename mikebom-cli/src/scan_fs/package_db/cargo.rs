//! Read Cargo/Rust package metadata from `Cargo.lock`.
//!
//! Supported formats (FR-040, R9):
//!
//! - **v3** (Cargo ≥ 1.53): `[[package]]` blocks with `name`, `version`,
//!   `source`, `checksum`, `dependencies`.
//! - **v4** (Cargo ≥ 1.78): same shape, but the `[metadata]` table is
//!   gone — checksums live on the `[[package]]` entries themselves.
//!
//! Fail-closed formats:
//!
//! - **v1** (Cargo 1.x pre-dates the `version = N` header; the lockfile
//!   has a top-level `[root]` table instead). Returns
//!   [`CargoError::LockfileUnsupportedVersion`] with `version = 1`.
//! - **v2** (Cargo 1.x early Stable): Returns the same error with
//!   `version = 2`. Users regenerate via `cargo generate-lockfile` on
//!   any Rust ≥ 1.53.
//!
//! Source-kind classification (R10):
//! - `source = "registry+https://..."` → registry crate. Gets SHA-256
//!   `ContentHash` from `checksum`.
//! - `source = "git+https://..."` → git coord. `source_type = "git"`.
//! - `source = "path+file://..."` → workspace-local. `source_type =
//!   "path"`.
//! - `source` absent → the entry IS the workspace root. `source_type =
//!   "workspace"` (no component emitted at read time; left to the
//!   caller to decide whether to publish).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use mikebom_common::types::hash::ContentHash;
use mikebom_common::types::purl::{encode_purl_segment, Purl};

use super::PackageDbEntry;

/// Errors the cargo reader can raise. Only `LockfileUnsupportedVersion`
/// is fatal (FR-040 + CLI contract, mirroring the npm v1 refusal).
#[derive(Debug, thiserror::Error)]
pub enum CargoError {
    #[error("Cargo.lock v1/v2 not supported; regenerate with cargo ≥1.53")]
    LockfileUnsupportedVersion { path: PathBuf, version: u64 },
}

// Cargo workspaces are shallow by convention: a top-level Cargo.toml
// + per-member subdir + per-target subdir typically max-nests at 3-4
// levels; 6 covers any realistic layout. Defense-in-depth backstop
// for the canonicalize-keyed visited-set primary mechanism. Per
// milestone-054 FR-003.
const MAX_PROJECT_ROOT_DEPTH: usize = 6;

// ---------------------------------------------------------------------------
// Cargo.lock shape (serde deserialization)
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct CargoLock {
    #[serde(default)]
    version: Option<u64>,
    #[serde(default)]
    package: Vec<CargoPackage>,
}

#[derive(Debug, serde::Deserialize)]
struct CargoPackage {
    name: String,
    version: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    checksum: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
}

/// Classification of a `[[package]]` entry's `source = "..."` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    /// `registry+https://github.com/rust-lang/crates.io-index` (crates.io)
    /// or any alternate registry — the normal case.
    Registry,
    /// `git+https://...` — source-kind property `"git"`.
    Git,
    /// `path+file://...` — workspace-local, source-kind property `"path"`.
    Path,
    /// No `source =` key → the entry IS the workspace root (or a
    /// workspace-member that doesn't declare a source). Source-kind
    /// property `"workspace"`; no SHA-256 hash available.
    Workspace,
}

fn classify_source(source: Option<&str>) -> SourceKind {
    match source {
        None => SourceKind::Workspace,
        Some(s) if s.starts_with("registry+") => SourceKind::Registry,
        Some(s) if s.starts_with("git+") => SourceKind::Git,
        Some(s) if s.starts_with("path+") => SourceKind::Path,
        Some(_) => SourceKind::Registry, // unknown scheme; treat as registry
    }
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

fn build_cargo_purl(name: &str, version: &str) -> Option<Purl> {
    // purl-spec § Character encoding: name + version are
    // percent-encoded strings. `+` in semver build metadata (e.g.
    // `1.0.0+build.123`) MUST encode to `%2B`.
    Purl::new(&format!(
        "pkg:cargo/{}@{}",
        encode_purl_segment(name),
        encode_purl_segment(version),
    ))
    .ok()
}

fn package_to_entry(pkg: &CargoPackage, source_path: &str) -> Option<PackageDbEntry> {
    let purl = build_cargo_purl(&pkg.name, &pkg.version)?;
    let kind = classify_source(pkg.source.as_deref());
    let source_type = match kind {
        SourceKind::Registry => None,
        SourceKind::Git => Some("git".to_string()),
        SourceKind::Path => Some("path".to_string()),
        SourceKind::Workspace => Some("workspace".to_string()),
    };
    // Registry crates carry a SHA-256 checksum. Git / path / workspace
    // entries do not — leave licenses/hashes empty for them.
    let licenses = Vec::new();
    // Dependencies are encoded as `<name>` or `<name> <version>` or
    // `<name> <version> (registry+...)` per the Cargo.lock format
    // contract. Milestone 087 (issue #172): preserve the version when
    // present so that downstream `name_to_purl` lookups in
    // `scan_fs/mod.rs` can disambiguate same-name multi-version
    // crates (e.g. `clap_builder@4.5.21` vs `clap_builder@4.5.9` in
    // the same workspace). Cargo only emits the `<name> <version>`
    // form when ambiguity exists in the lockfile; the bare `<name>`
    // form is preserved for unambiguous deps. Strip only the
    // ` (source)` suffix.
    let depends: Vec<String> = pkg
        .dependencies
        .iter()
        .map(|d| match d.find(" (") {
            Some(idx) => d[..idx].to_string(),
            None => d.clone(),
        })
        .collect();
    // Registry crates have an accessible `~/.cargo/registry/src/...`
    // Cargo.toml carrying `authors = [...]` that the lockfile
    // doesn't. Offline-safe: miss → None.
    let maintainer = if matches!(kind, SourceKind::Registry) {
        registry_cache_authors(&pkg.name, &pkg.version)
    } else {
        None
    };
    Some(PackageDbEntry {
        purl,
        name: pkg.name.clone(),
        version: pkg.version.clone(),
        arch: None,
        source_path: source_path.to_string(),
        depends,
        maintainer,
        licenses,
        lifecycle_scope: None,
        requirement_range: None,
        source_type,
        buildinfo_status: None,
        evidence_kind: None,
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: None,
        confidence: None,
        binary_packed: None,
        raw_version: None,
        parent_purl: None,
        npm_role: None,
        co_owned_by: None,
        hashes: Vec::new(),
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations: Default::default(),
    })
}

/// Build a `ContentHash` from a `checksum = "..."` hex string. Cargo
/// always emits SHA-256 so we hard-code the algorithm. Returns `None`
/// when the value isn't a valid SHA-256 hex.
pub(crate) fn checksum_to_content_hash(hex: &str) -> Option<ContentHash> {
    ContentHash::sha256(hex).ok()
}

/// Read `~/.cargo/registry/src/index.crates.io-*/<crate>-<version>/Cargo.toml`
/// and extract `package.authors`. Returns the joined author string
/// when found, `None` otherwise. Offline-safe: missing cache, missing
/// crate, missing authors, or malformed TOML all map to `None`.
///
/// The cache dir has a hash suffix that varies per registry
/// (`index.crates.io-6f17d22bba15001f` is today's crates.io); glob
/// `src/` and probe each candidate. Cost: one `read_dir` + a handful
/// of `is_dir()` probes per crate — negligible next to the lockfile
/// walk.
fn registry_cache_authors(name: &str, version: &str) -> Option<String> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let src_root = std::path::PathBuf::from(home).join(".cargo/registry/src");
    let Ok(read_dir) = std::fs::read_dir(&src_root) else {
        return None;
    };
    let crate_dirname = format!("{name}-{version}");
    for entry in read_dir.flatten() {
        let registry_dir = entry.path();
        if !registry_dir.is_dir() {
            continue;
        }
        let cargo_toml = registry_dir.join(&crate_dirname).join("Cargo.toml");
        if let Ok(text) = std::fs::read_to_string(&cargo_toml) {
            if let Some(authors) = parse_authors_from_cargo_toml(&text) {
                return Some(authors);
            }
        }
    }
    None
}

/// Extract `package.authors = [...]` from a Cargo.toml body. Returns
/// the authors joined with `", "`. Uses the `toml` crate (already a
/// workspace dep) for robust parsing — handles multi-line arrays,
/// quoting, escapes, and TOML structure edge cases the lockfile
/// parser doesn't need to care about.
fn parse_authors_from_cargo_toml(text: &str) -> Option<String> {
    let parsed: toml::Value = toml::from_str(text).ok()?;
    let authors = parsed
        .get("package")?
        .get("authors")?
        .as_array()?;
    let joined: Vec<String> = authors
        .iter()
        .filter_map(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if joined.is_empty() {
        None
    } else {
        Some(joined.join(", "))
    }
}

// ---------------------------------------------------------------------------
// Milestone 064 — Cargo source-tree main-module component
// ---------------------------------------------------------------------------

/// Map from absolute workspace-root `Cargo.toml` directory to that
/// workspace's `[workspace.package].version` (when declared). Used to
/// resolve `version.workspace = true` in member-crate manifests per
/// FR-001 + Assumption A2 of milestone 064.
#[derive(Debug, Default)]
pub(crate) struct WorkspaceContext {
    versions: HashMap<PathBuf, String>,
}

impl WorkspaceContext {
    /// Build the context by examining every passed manifest path for a
    /// `[workspace.package].version` declaration. Manifest files that
    /// don't declare one (the common case for member crates) contribute
    /// nothing; the resulting map is keyed by the manifest's parent
    /// directory so member-crate lookups via `lookup_for_member` can
    /// walk-up by path-prefix to find the enclosing workspace.
    pub(crate) fn build_from_manifests(manifests: &[PathBuf]) -> Self {
        let mut versions = HashMap::new();
        for manifest_path in manifests {
            let Ok(text) = std::fs::read_to_string(manifest_path) else {
                continue;
            };
            let Ok(parsed) = toml::from_str::<toml::Value>(&text) else {
                continue;
            };
            let Some(version) = parsed
                .get("workspace")
                .and_then(|w| w.get("package"))
                .and_then(|p| p.get("version"))
                .and_then(|v| v.as_str())
            else {
                continue;
            };
            let Some(parent_dir) = manifest_path.parent() else {
                continue;
            };
            versions.insert(parent_dir.to_path_buf(), version.to_string());
        }
        Self { versions }
    }

    /// Look up the workspace-inherited `[workspace.package].version`
    /// for a member manifest by walking up from the member's manifest
    /// directory until a workspace root is found. Returns `None` if no
    /// enclosing workspace declares the version.
    fn lookup_for_member(&self, member_manifest_dir: &Path) -> Option<&str> {
        let mut cursor = Some(member_manifest_dir);
        while let Some(dir) = cursor {
            if let Some(version) = self.versions.get(dir) {
                return Some(version.as_str());
            }
            cursor = dir.parent();
        }
        None
    }
}

/// Resolve the cargo main-module's version per FR-001 + Assumption A2:
/// 1. If `[package].version` is a literal string → return verbatim.
/// 2. If `[package].version.workspace = true` → look up workspace root.
/// 3. Otherwise → `0.0.0-unknown` placeholder (cross-host determinism).
fn resolve_cargo_main_module_version(
    manifest_dir: &Path,
    package_table: &toml::Value,
    workspace: &WorkspaceContext,
) -> String {
    let version = package_table.get("version");
    if let Some(s) = version.and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if version
        .and_then(|v| v.get("workspace"))
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        if let Some(resolved) = workspace.lookup_for_member(manifest_dir) {
            return resolved.to_string();
        }
    }
    "0.0.0-unknown".to_string()
}

/// Build the cargo main-module entry for a single `Cargo.toml`. Returns
/// `None` when the manifest has no `[package]` table (workspace-only
/// roots — FR-002) or when `name` cannot be resolved. Per FR-001 +
/// FR-001a + FR-004 + FR-005 + FR-006, the entry carries:
/// - PURL `pkg:cargo/<name>@<resolved-version>`
/// - `mikebom:component-role: "main-module"` (C40, supplementary)
/// - `sbom_tier = Some("source")` (FR-006)
/// - `parent_purl = None` (top-level — FR-001a)
/// - `licenses = vec![]` (FR-005; license detection is #103 follow-up)
/// - empty `depends` for now; FR-007 wiring populates direct-dep edges
///   by post-processing each manifest's `[dependencies]` /
///   `[dev-dependencies]` / `[build-dependencies]` tables (currently
///   plumbed via the existing scan_fs/mod.rs edge-emission rather than
///   here, mirroring milestone 053's Go pattern).
fn build_cargo_main_module_entry(
    manifest_path: &Path,
    workspace: &WorkspaceContext,
) -> Option<PackageDbEntry> {
    let text = std::fs::read_to_string(manifest_path).ok()?;
    let parsed: toml::Value = toml::from_str(&text).ok()?;
    let package = parsed.get("package")?;
    let name = package.get("name").and_then(|v| v.as_str())?;
    let manifest_dir = manifest_path.parent()?;
    let version = resolve_cargo_main_module_version(manifest_dir, package, workspace);
    let purl = build_cargo_purl(name, &version)?;
    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "mikebom:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );
    let source_path = format!("path+file://{}", manifest_dir.display());
    Some(PackageDbEntry {
        purl,
        name: name.to_string(),
        version,
        arch: None,
        source_path,
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_range: None,
        source_type: Some("workspace".to_string()),
        buildinfo_status: None,
        evidence_kind: None,
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: None,
        confidence: None,
        binary_packed: None,
        raw_version: None,
        parent_purl: None,
        npm_role: None,
        co_owned_by: None,
        hashes: Vec::new(),
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations,
    })
}

/// Record describing a duplicate main-module dropped during dedup,
/// returned in batch from `dedup_main_modules_by_purl` for
/// caller-side `tracing::warn!` emission per spec Clarifications Q1.
#[derive(Debug, Clone)]
pub(crate) struct DroppedDuplicate {
    pub purl: String,
    pub kept_path: String,
    pub dropped_path: String,
}

/// Dedup main-module entries by PURL, preserving the first occurrence
/// (deterministic on the existing walker order — `discover_workspace_
/// manifests` returns root-then-members in declaration order). Returns
/// the list of dropped duplicates for caller-side `tracing::warn!`
/// emission per FR-001 + spec Clarifications Q1. Non-main-module
/// entries (the predicate is C40-tag-driven) are left untouched even
/// if their PURLs would collide with each other.
pub(crate) fn dedup_main_modules_by_purl(
    entries: &mut Vec<PackageDbEntry>,
) -> Vec<DroppedDuplicate> {
    let mut dropped: Vec<DroppedDuplicate> = Vec::new();
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut keep: Vec<PackageDbEntry> = Vec::with_capacity(entries.len());
    for entry in std::mem::take(entries) {
        let is_main = entry
            .extra_annotations
            .get("mikebom:component-role")
            .and_then(|v| v.as_str())
            == Some("main-module");
        if !is_main {
            keep.push(entry);
            continue;
        }
        let purl = entry.purl.as_str().to_string();
        if let Some(kept_path) = seen.get(&purl) {
            dropped.push(DroppedDuplicate {
                purl: purl.clone(),
                kept_path: kept_path.clone(),
                dropped_path: entry.source_path.clone(),
            });
        } else {
            seen.insert(purl, entry.source_path.clone());
            keep.push(entry);
        }
    }
    *entries = keep;
    dropped
}

// ---------------------------------------------------------------------------
// Milestone 051 — Cargo.toml dev/build-dep classification (T004-T008)
// ---------------------------------------------------------------------------

/// Crate names declared in a single `Cargo.toml`'s three dependency
/// sections (plus the `target.<cfg>.*` variants of each). Drives the
/// milestone-051 dev-vs-prod classification: a crate appearing ONLY in
/// `dev_deps` or `build_deps` (and not in `prod_deps`) is tagged
/// `mikebom:dev-dependency = true` after BFS expansion through the
/// resolved Cargo.lock graph.
#[derive(Debug, Default, Clone)]
pub(crate) struct CargoTomlSections {
    pub prod_deps: HashSet<String>,
    pub dev_deps: HashSet<String>,
    pub build_deps: HashSet<String>,
}

impl CargoTomlSections {
    /// Union with another sections set — used to merge the workspace
    /// root + every member crate into a single workspace-wide
    /// classification view.
    fn union(&mut self, other: &CargoTomlSections) {
        self.prod_deps.extend(other.prod_deps.iter().cloned());
        self.dev_deps.extend(other.dev_deps.iter().cloned());
        self.build_deps.extend(other.build_deps.iter().cloned());
    }
}

/// Parse a `Cargo.toml` file and extract the three dependency sections
/// (plus their `target.<cfg>.*` counterparts). Returns `None` on parse
/// failure (warn-and-skip per plan R3 — a malformed Cargo.toml in some
/// workspace member shouldn't abort the whole scan).
pub(crate) fn parse_cargo_toml(path: &Path) -> Option<CargoTomlSections> {
    let text = std::fs::read_to_string(path).ok()?;
    let parsed: toml::Value = match toml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "Cargo.toml parse failed — skipping dev/build classification \
                 for this manifest",
            );
            return None;
        }
    };
    let mut out = CargoTomlSections::default();
    collect_section_keys(&parsed, "dependencies", &mut out.prod_deps);
    collect_section_keys(&parsed, "dev-dependencies", &mut out.dev_deps);
    collect_section_keys(&parsed, "build-dependencies", &mut out.build_deps);
    // Walk every `target.<cfg>` table for its three section variants.
    if let Some(target_table) = parsed.get("target").and_then(|v| v.as_table()) {
        for (_cfg, target_value) in target_table {
            collect_section_keys(target_value, "dependencies", &mut out.prod_deps);
            collect_section_keys(target_value, "dev-dependencies", &mut out.dev_deps);
            collect_section_keys(target_value, "build-dependencies", &mut out.build_deps);
        }
    }
    Some(out)
}

fn collect_section_keys(parsed: &toml::Value, section: &str, out: &mut HashSet<String>) {
    let Some(table) = parsed.get(section).and_then(|v| v.as_table()) else {
        return;
    };
    for (key, value) in table {
        // Cargo allows `foo = { package = "real-name", version = "1" }`.
        // The renamed key in the TOML doesn't match the resolved
        // `[[package]] name` in Cargo.lock; the inline `package = "..."`
        // override does. Honor it when present.
        let resolved_name = value
            .as_table()
            .and_then(|t| t.get("package"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| key.clone());
        out.insert(resolved_name);
    }
}

/// Discover every `Cargo.toml` reachable from the lockfile's project
/// root: the immediate sibling, plus any workspace members declared
/// via `[workspace] members = [...]` (with simple glob expansion for
/// `crate-*` / `crates/*` patterns). Returns absolute paths in
/// deterministic insertion order.
///
/// Per plan R1 fallback: if a glob escalates beyond simple star
/// matching, we just include every directory under the matched parent
/// — same effective semantic for typical layouts.
pub(crate) fn discover_workspace_manifests(lockfile: &Path) -> Vec<PathBuf> {
    let Some(project_root) = lockfile.parent() else {
        return Vec::new();
    };
    let root_manifest = project_root.join("Cargo.toml");
    if !root_manifest.is_file() {
        return Vec::new();
    }
    let mut out = vec![root_manifest.clone()];
    // Read the root manifest's `[workspace] members = [...]` array.
    let Ok(text) = std::fs::read_to_string(&root_manifest) else {
        return out;
    };
    let Ok(parsed) = toml::from_str::<toml::Value>(&text) else {
        return out;
    };
    let Some(members) = parsed
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
    else {
        return out;
    };
    for member in members {
        let Some(pattern) = member.as_str() else {
            continue;
        };
        expand_workspace_member(project_root, pattern, &mut out);
    }
    out
}

fn expand_workspace_member(project_root: &Path, pattern: &str, out: &mut Vec<PathBuf>) {
    if let Some(prefix) = pattern.strip_suffix("/*") {
        // Glob `<prefix>/*`: include every immediate subdirectory of
        // `<project_root>/<prefix>` whose Cargo.toml exists.
        let parent = project_root.join(prefix);
        let Ok(read_dir) = std::fs::read_dir(&parent) else {
            return;
        };
        for entry in read_dir.flatten() {
            let dir = entry.path();
            if dir.is_dir() {
                let manifest = dir.join("Cargo.toml");
                if manifest.is_file() {
                    out.push(manifest);
                }
            }
        }
    } else if pattern.contains('*') {
        // Plan R1 fallback: more complex glob → include every
        // descendant Cargo.toml under the pattern's leading literal
        // prefix. Conservative; over-includes but never misses.
        let leading = pattern.split('*').next().unwrap_or("");
        let parent = project_root.join(leading);
        find_descendant_manifests(&parent, 0, out);
    } else {
        // Literal path.
        let manifest = project_root.join(pattern).join("Cargo.toml");
        if manifest.is_file() {
            out.push(manifest);
        }
    }
}

fn find_descendant_manifests(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth >= MAX_PROJECT_ROOT_DEPTH {
        return;
    }
    let manifest = dir.join("Cargo.toml");
    if manifest.is_file() {
        out.push(manifest);
    }
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if should_skip_descent(name) {
            continue;
        }
        find_descendant_manifests(&path, depth + 1, out);
    }
}

/// Compute the prod-reachable closure of `(name, version)` tuples by
/// BFS-walking `Cargo.lock`'s resolved dep graph from the direct-prod
/// crate names of the workspace.
///
/// Cargo.lock encodes per-package dep edges as
/// `dependencies = ["bar 1.0.0 (registry+https://...)"]`. Each string
/// is `<name> [<version>] [(<source>)]`; we extract the name +
/// (when present) version to look up the next package node.
///
/// Production-wins-over-dev (FR-003): a crate reachable from BOTH a
/// prod and a dev edge lands in this set, so the classifier correctly
/// retains it as production. Same precedence rule as Go US2 / npm.
fn compute_cargo_prod_set(
    lock: &CargoLock,
    direct_prod: &HashSet<String>,
) -> HashSet<(String, String)> {
    cargo_bfs_closure(lock, direct_prod)
}

/// Milestone 052/part-2: BFS-walk Cargo.lock from a set of seed crate
/// names through the resolved `[[package]] dependencies = [...]` graph.
/// Same algorithm as `compute_cargo_prod_set` — extracted as a shared
/// helper now that we walk multiple seed sets (prod-direct AND
/// build-direct).
fn cargo_bfs_closure(
    lock: &CargoLock,
    direct_seeds: &HashSet<String>,
) -> HashSet<(String, String)> {
    // Build a quick (name, version) → CargoPackage index for BFS
    // traversal. When multiple `[[package]]` rows share a name (rare
    // but legal — workspace members + transitive multi-version
    // resolutions), keep them all.
    let mut by_name: HashMap<&str, Vec<&CargoPackage>> = HashMap::new();
    for pkg in &lock.package {
        by_name.entry(pkg.name.as_str()).or_default().push(pkg);
    }
    let mut visited: HashSet<(String, String)> = HashSet::new();
    let mut frontier: Vec<(&str, Option<&str>)> = direct_seeds
        .iter()
        .map(|name| (name.as_str(), None::<&str>))
        .collect();
    while let Some((name, version)) = frontier.pop() {
        // Resolve to one or more concrete package nodes.
        let Some(candidates) = by_name.get(name) else {
            continue;
        };
        for pkg in candidates {
            if let Some(target_version) = version {
                if pkg.version != target_version {
                    continue;
                }
            }
            let key = (pkg.name.clone(), pkg.version.clone());
            if !visited.insert(key) {
                continue;
            }
            for dep in &pkg.dependencies {
                let mut parts = dep.split_whitespace();
                let dep_name = match parts.next() {
                    Some(n) => n,
                    None => continue,
                };
                let dep_version = parts.next().filter(|p| !p.starts_with('('));
                frontier.push((dep_name, dep_version));
            }
        }
    }
    visited
}

/// Milestone 052/part-2: BFS-walk from `[build-dependencies]` direct
/// seeds. A crate reachable through the build-dep graph but NOT
/// through the prod-dep graph is tagged `LifecycleScope::Build`
/// (compile-time only, not in the runtime artifact). Production wins:
/// a crate in BOTH graphs ends up in `prod_set` (computed first) and
/// the build-set classification doesn't apply. Same call shape as
/// `compute_cargo_prod_set` for consistency.
fn compute_cargo_build_set(
    lock: &CargoLock,
    direct_build: &HashSet<String>,
) -> HashSet<(String, String)> {
    cargo_bfs_closure(lock, direct_build)
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

/// Parse one `Cargo.lock` file. Emits typed error for v1/v2; otherwise
/// returns the flattened entry list for v3/v4. When `prod_set` is
/// non-empty (a Cargo.toml was found alongside the lockfile), entries
/// whose `(name, version)` is NOT in the set are tagged
/// `is_dev = Some(true)` per milestone 051. When `prod_set` is empty
/// (lockfile-only, no Cargo.toml beside it) entries pass through with
/// `is_dev = None` — we can't classify without dep-section info.
fn parse_lockfile(
    path: &Path,
    prod_set: &HashSet<(String, String)>,
    build_set: &HashSet<(String, String)>,
) -> Result<Vec<PackageDbEntry>, CargoError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return Ok(Vec::new()),
    };
    let doc: CargoLock = match toml::from_str(&text) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "Cargo.lock parse failed — emitting zero cargo components",
            );
            return Ok(Vec::new());
        }
    };
    // Absent version field → pre-v3 (v1 or v2). Cargo never wrote a
    // `version = ` key before v3; its absence IS the signal.
    match doc.version {
        None => {
            // Could be v1 (has `[root]`) or v2 (has `[[package]]` but no
            // version). Both refuse per FR-040.
            let version_hint = if text.contains("[root]") { 1 } else { 2 };
            return Err(CargoError::LockfileUnsupportedVersion {
                path: path.to_path_buf(),
                version: version_hint,
            });
        }
        Some(v) if v < 3 => {
            return Err(CargoError::LockfileUnsupportedVersion {
                path: path.to_path_buf(),
                version: v,
            });
        }
        _ => {}
    }
    let source_path = path.to_string_lossy().into_owned();
    let mut out: Vec<PackageDbEntry> = Vec::new();
    for pkg in &doc.package {
        // Milestone 087 (issue #172): emit source=None entries
        // (workspace root + workspace members + path deps) as
        // PackageDbEntry rows. The original conformance-bug-2 fix
        // skipped them to avoid a self-referential FP for the
        // workspace root, but milestone-064's main-module emission
        // (Phase A below) now augments the workspace root in-place
        // with the C40 supplementary tag, so the FP no longer
        // exists. Emitting workspace MEMBERS is required so that
        // multi-version-same-name lookups (e.g. `clap_builder
        // 4.5.21` in clap-rs/clap's workspace) resolve correctly
        // via the dual-key insert in `scan_fs/mod.rs`.
        if let Some(mut entry) = package_to_entry(pkg, &source_path) {
            // Attach SHA-256 ContentHash to registry crates only.
            // Git / path entries don't carry a checksum in the lockfile.
            if classify_source(pkg.source.as_deref()) == SourceKind::Registry {
                if let Some(ref checksum) = pkg.checksum {
                    if let Some(hash) = checksum_to_content_hash(checksum) {
                        entry.hashes.push(hash);
                    }
                }
            }
            entry.source_path = source_path.clone();
            // Milestone 052/part-2: 4-way classifier. Production wins
            // over Build wins over Development per FR-005's priority
            // hierarchy. When prod_set is empty (no Cargo.toml found
            // alongside the lockfile) leave lifecycle_scope = None —
            // we can't classify without dep-section info.
            if !prod_set.is_empty() {
                use mikebom_common::resolution::LifecycleScope;
                let key = (pkg.name.clone(), pkg.version.clone());
                if prod_set.contains(&key) {
                    entry.lifecycle_scope = Some(LifecycleScope::Runtime);
                } else if build_set.contains(&key) {
                    entry.lifecycle_scope = Some(LifecycleScope::Build);
                } else {
                    // Reachable from neither prod nor build closures —
                    // it's in the lockfile because it's a dev-dep
                    // (criterion, proptest, etc.).
                    entry.lifecycle_scope = Some(LifecycleScope::Development);
                }
            }
            out.push(entry);
        }
    }
    Ok(out)
}

/// Public entry point — walks `rootfs` for `Cargo.lock` files, parses
/// each, and returns the flattened entry list. v1/v2 at any root
/// short-circuits with the typed error.
///
/// Milestone 051: per-lockfile, parses sibling/workspace `Cargo.toml`
/// files to identify dev/build deps, BFS-expands the prod closure
/// against the lockfile, and tags entries outside that closure with
/// `is_dev = Some(true)`. When `include_dev = false`, tagged entries
/// are dropped (mirrors maven.rs:1786 + go-source-set filter).
pub fn read(rootfs: &Path, include_dev: bool) -> Result<Vec<PackageDbEntry>, CargoError> {
    let mut out: Vec<PackageDbEntry> = Vec::new();
    let mut seen_purls: HashSet<String> = HashSet::new();
    let mut tagged_dev = 0usize;
    let mut dropped = 0usize;
    let mut main_modules_emitted = 0usize;

    // Milestone 064 — Phase A: discover EVERY Cargo.toml under rootfs
    // (independent of Cargo.lock presence — library crates without a
    // committed lockfile must still emit their project-self component
    // per FR-001). Build the WorkspaceContext from the same set so
    // version.workspace=true resolution finds the workspace root.
    // We collect main-module PURLs in a set so Phase B (lockfile
    // emission) can merge the C40 supplementary tag onto same-PURL
    // entries WITHOUT losing their lockfile-derived `depends` list.
    // Order matters: Phase B FIRST (builds the dep graph), Phase A
    // SECOND (adds main-modules for crates not seen in any lockfile,
    // and tags lockfile-derived workspace-member entries with C40).
    let all_manifests = find_cargo_manifests(rootfs);
    let workspace_ctx = WorkspaceContext::build_from_manifests(&all_manifests);

    // Milestone 064 — Phase B: per-lockfile dependency emission
    // (existing milestone-051 flow). The workspace_sections are
    // rebuilt per-lockfile using `discover_workspace_manifests`
    // (which honors `[workspace] members = [...]` rather than walking
    // the filesystem) so dev/build classification works exactly as
    // before. Phase A's main-module emission is layered on top below.
    for lock_path in find_cargo_lockfiles(rootfs) {
        let manifests = discover_workspace_manifests(&lock_path);
        let mut workspace_sections = CargoTomlSections::default();
        for manifest_path in &manifests {
            if let Some(sections) = parse_cargo_toml(manifest_path) {
                workspace_sections.union(&sections);
            }
        }
        // Re-read the lockfile structurally to drive the BFS prod-set
        // and build-set computations. parse_lockfile_doc returns the
        // typed CargoLock (lighter than re-running parse_lockfile,
        // which converts to PackageDbEntry).
        let (prod_set, build_set) = match parse_lockfile_doc(&lock_path) {
            Ok(Some(doc)) => {
                let prod = compute_cargo_prod_set(&doc, &workspace_sections.prod_deps);
                let build = compute_cargo_build_set(&doc, &workspace_sections.build_deps);
                (prod, build)
            }
            _ => (HashSet::new(), HashSet::new()),
        };
        let entries = parse_lockfile(&lock_path, &prod_set, &build_set)?;
        for entry in entries {
            let purl_key = entry.purl.as_str().to_string();
            // Drop dev entries when --include-dev is off.
            if mikebom_common::resolution::lifecycle_scope_is_legacy_dev(&entry.lifecycle_scope) {
                tagged_dev += 1;
                if !include_dev {
                    dropped += 1;
                    continue;
                }
            }
            if seen_purls.insert(purl_key) {
                out.push(entry);
            }
        }
    }

    // Milestone 065 (#126): collect workspace-root `dependencies = [...]`
    // declarations from every lockfile in scope. parse_lockfile skips
    // workspace-root [[package]] entries (source = None) to prevent
    // self-referential FPs, but their dep lists ARE the canonical
    // direct-dep set for the project. We re-read the lockfiles here
    // and harvest them for milestone-064 main-module population.
    let mut workspace_root_deps: HashMap<(String, String), Vec<String>> =
        HashMap::new();
    for lock_path in find_cargo_lockfiles(rootfs) {
        if let Ok(Some(doc)) = parse_lockfile_doc(&lock_path) {
            for pkg in &doc.package {
                if pkg.source.is_some() {
                    continue;
                }
                // Milestone 087 (issue #172): preserve `<name> <version>`
                // form (strip only the ` (source)` suffix) so the
                // `name_to_purl` lookup in `scan_fs/mod.rs` can
                // disambiguate same-name multi-version crates. Same
                // logic as `package_to_entry` above.
                let dep_names: Vec<String> = pkg
                    .dependencies
                    .iter()
                    .map(|d| match d.find(" (") {
                        Some(idx) => d[..idx].to_string(),
                        None => d.clone(),
                    })
                    .collect();
                workspace_root_deps
                    .entry((pkg.name.clone(), pkg.version.clone()))
                    .or_default()
                    .extend(dep_names);
            }
        }
    }

    // Milestone 064 — Phase A (deferred): main-module emission.
    // For each Cargo.toml with [package], either:
    //   (a) augment an existing same-PURL lockfile-derived entry with
    //       the C40 supplementary tag (preserving its `depends` list,
    //       which the lockfile resolved correctly); OR
    //   (b) emit a new main-module entry (for crates without a
    //       Cargo.lock in scope — library crates, fixture mins).
    // This ordering avoids losing lockfile-derived dep lists for
    // workspace members.
    //
    // Milestone 065 (#126): also populate `depends` from
    // `workspace_root_deps` — the lockfile's `[[package]]` block
    // for the workspace-root entry carries the project's direct-dep
    // set even though parse_lockfile skipped emitting that entry.
    for manifest_path in &all_manifests {
        let Some(mut synthesized) =
            build_cargo_main_module_entry(manifest_path, &workspace_ctx)
        else {
            continue;
        };
        // Pull in workspace-root's dependency list from the lockfile
        // (#126). Keyed by (name, version) — same shape as
        // workspace_root_deps's entries.
        let lookup_key = (synthesized.name.clone(), synthesized.version.clone());
        if let Some(deps) = workspace_root_deps.get(&lookup_key) {
            synthesized.depends.extend(deps.iter().cloned());
        }
        let purl_key = synthesized.purl.as_str().to_string();
        if let Some(existing) = out.iter_mut().find(|e| e.purl.as_str() == purl_key) {
            // (a) augment in-place with C40 + sbom_tier:source
            for (k, v) in synthesized.extra_annotations.iter() {
                existing
                    .extra_annotations
                    .entry(k.clone())
                    .or_insert_with(|| v.clone());
            }
            if existing.sbom_tier.is_none() {
                existing.sbom_tier = synthesized.sbom_tier.clone();
            }
            // Merge workspace-root deps into existing entry. Dedup
            // against existing depends — lockfile-resolved transitive
            // deps may already include some of these names.
            let existing_deps: HashSet<String> =
                existing.depends.iter().cloned().collect();
            for d in &synthesized.depends {
                if !existing_deps.contains(d) {
                    existing.depends.push(d.clone());
                }
            }
            // Mark as top-level (main-modules are linker roots, never
            // children of another component).
            existing.parent_purl = None;
            main_modules_emitted += 1;
        } else if seen_purls.insert(purl_key) {
            // (b) net-new main-module (no lockfile entry collided)
            out.push(synthesized);
            main_modules_emitted += 1;
        }
    }

    // Milestone 064 same-PURL dedup. Collapses same-PURL collisions
    // (vendored copies, examples/ mirrors, target/package extractions)
    // per FR-001 + Q1. Non-main-module entries are untouched
    // (already deduped by `seen_purls`).
    let dedup_drops = dedup_main_modules_by_purl(&mut out);
    if !dedup_drops.is_empty() {
        let dropped_paths: Vec<String> = dedup_drops
            .iter()
            .map(|d| d.dropped_path.clone())
            .collect();
        let kept_path = dedup_drops
            .first()
            .map(|d| d.kept_path.clone())
            .unwrap_or_default();
        let example_purl = dedup_drops
            .first()
            .map(|d| d.purl.clone())
            .unwrap_or_default();
        tracing::warn!(
            count = dedup_drops.len(),
            example_purl = %example_purl,
            kept = %kept_path,
            dropped = ?dropped_paths,
            "cargo: deduped same-PURL Cargo.toml files",
        );
    }
    if !out.is_empty() {
        tracing::info!(
            rootfs = %rootfs.display(),
            entries = out.len(),
            main_modules_emitted,
            same_purl_duplicates_dropped = dedup_drops.len(),
            tagged_dev,
            dropped_when_no_include_dev = dropped,
            include_dev,
            "parsed Cargo lockfiles",
        );
    }
    Ok(out)
}

/// Parse a `Cargo.lock` file into the typed `CargoLock` document
/// without converting to `PackageDbEntry`. Used by the milestone-051
/// prod-set BFS — needs raw `[[package]] dependencies = [...]` edges
/// before the per-entry classification + drop logic runs.
///
/// Returns `Ok(None)` on read/parse failure (warn-and-skip), `Err` on
/// fatal v1/v2 lockfile-version refusal.
fn parse_lockfile_doc(path: &Path) -> Result<Option<CargoLock>, CargoError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };
    let doc: CargoLock = match toml::from_str(&text) {
        Ok(d) => d,
        Err(_) => return Ok(None),
    };
    match doc.version {
        None => {
            let version_hint = if text.contains("[root]") { 1 } else { 2 };
            Err(CargoError::LockfileUnsupportedVersion {
                path: path.to_path_buf(),
                version: version_hint,
            })
        }
        Some(v) if v < 3 => Err(CargoError::LockfileUnsupportedVersion {
            path: path.to_path_buf(),
            version: v,
        }),
        _ => Ok(Some(doc)),
    }
}

fn find_cargo_lockfiles(rootfs: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut visited: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    walk_for_cargo_lockfiles(rootfs, 0, &mut visited, &mut out);
    out
}

/// Walk for every `Cargo.toml` reachable from `rootfs` (subject to
/// `should_skip_descent` — `vendor/`, `target/`, etc. are pruned).
/// Used by milestone 064 main-module emission, which is NOT gated on
/// `Cargo.lock` presence: library crates with no committed lockfile
/// must still emit a project-self component per FR-001. Output is in
/// deterministic walk order (alphabetical-like via `read_dir` for a
/// given platform — preserved for golden cross-host stability via the
/// dedup-by-PURL convention).
fn find_cargo_manifests(rootfs: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut visited: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    walk_for_cargo_manifests(rootfs, 0, &mut visited, &mut out);
    out
}

fn walk_for_cargo_manifests(
    dir: &Path,
    depth: usize,
    visited: &mut std::collections::HashSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) {
    let manifest = dir.join("Cargo.toml");
    if manifest.is_file() {
        out.push(manifest);
    }
    if depth >= MAX_PROJECT_ROOT_DEPTH {
        return;
    }
    let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(key) {
        tracing::debug!(
            path = %dir.display(),
            "walker: cycle/visited skip (manifest discovery)",
        );
        return;
    }
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read_dir.flatten().collect();
    // Sort by file name so the walk order is deterministic across
    // platforms (read_dir order is filesystem-dependent on macOS vs
    // Linux). The dedup-by-PURL pass relies on first-discovered-wins
    // determinism per FR-001.
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if should_skip_descent(name) {
            continue;
        }
        walk_for_cargo_manifests(&path, depth + 1, visited, out);
    }
}

/// Milestone 054 FR-001/FR-002/FR-003: canonicalize-keyed visited
/// set + max-depth backstop prevents unbounded recursion on symlink
/// loops (e.g., `linkToRoot -> .` test fixtures).
fn walk_for_cargo_lockfiles(
    dir: &Path,
    depth: usize,
    visited: &mut std::collections::HashSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) {
    let lock = dir.join("Cargo.lock");
    if lock.is_file() {
        out.push(lock);
    }
    if depth >= MAX_PROJECT_ROOT_DEPTH {
        return;
    }
    let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(key) {
        tracing::debug!(
            path = %dir.display(),
            "walker: cycle/visited skip",
        );
        return;
    }
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if should_skip_descent(name) {
            continue;
        }
        walk_for_cargo_lockfiles(&path, depth + 1, visited, out);
    }
}

fn should_skip_descent(name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }
    matches!(
        name,
        "target" | "vendor" | "node_modules" | "dist" | "__pycache__"
    )
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parse_authors_from_cargo_toml_joins_array() {
        let text = r#"
[package]
name = "serde"
version = "1.0.197"
authors = ["Erick Tryzelaar <erick.tryzelaar@gmail.com>", "David Tolnay <dtolnay@gmail.com>"]
edition = "2021"
"#;
        assert_eq!(
            parse_authors_from_cargo_toml(text).as_deref(),
            Some(
                "Erick Tryzelaar <erick.tryzelaar@gmail.com>, David Tolnay <dtolnay@gmail.com>",
            ),
        );
    }

    #[test]
    fn parse_authors_handles_single_author() {
        let text = r#"
[package]
name = "tiny"
authors = ["Solo Dev"]
"#;
        assert_eq!(
            parse_authors_from_cargo_toml(text).as_deref(),
            Some("Solo Dev"),
        );
    }

    #[test]
    fn parse_authors_returns_none_when_field_missing() {
        let text = r#"
[package]
name = "noauth"
version = "0.1"
"#;
        assert!(parse_authors_from_cargo_toml(text).is_none());
    }

    #[test]
    fn parse_authors_returns_none_on_empty_array() {
        let text = r#"
[package]
authors = []
"#;
        assert!(parse_authors_from_cargo_toml(text).is_none());
    }

    #[test]
    fn parse_authors_tolerates_malformed_toml() {
        assert!(parse_authors_from_cargo_toml("not valid =@ toml").is_none());
    }

    #[test]
    fn lockfile_unsupported_version_display_matches_contract() {
        let err = CargoError::LockfileUnsupportedVersion {
            path: PathBuf::from("/tmp/Cargo.lock"),
            version: 2,
        };
        assert_eq!(
            err.to_string(),
            "Cargo.lock v1/v2 not supported; regenerate with cargo ≥1.53"
        );
    }

    fn write_lockfile(dir: &Path, body: &str) -> PathBuf {
        let p = dir.join("Cargo.lock");
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn parses_v3_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        let body = r#"
version = 3

[[package]]
name = "serde"
version = "1.0.197"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3fb1c753d2daa1c9a31c65b0c2e1b8b3f6eafbbaa32a9c0b48da3b0b4e2b92d7"
dependencies = ["serde_derive"]

[[package]]
name = "serde_derive"
version = "1.0.197"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "0000000000000000000000000000000000000000000000000000000000000001"
"#;
        let path = write_lockfile(dir.path(), body);
        let entries = parse_lockfile(&path, &HashSet::new(), &HashSet::new()).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.name == "serde"));
        let serde = entries.iter().find(|e| e.name == "serde").unwrap();
        assert_eq!(serde.depends, vec!["serde_derive".to_string()]);
        assert_eq!(serde.sbom_tier.as_deref(), Some("source"));
        assert_eq!(serde.source_type, None); // registry source
    }

    #[test]
    fn parses_v4_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        let body = r#"
version = 4

[[package]]
name = "anyhow"
version = "1.0.80"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "5ad32ce52e4161730f7098c077cd2ed6229b5804ccf99e5366be1ab72a98b4e1"
"#;
        let path = write_lockfile(dir.path(), body);
        let entries = parse_lockfile(&path, &HashSet::new(), &HashSet::new()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "anyhow");
    }

    #[test]
    fn git_source_gets_source_type_property() {
        let dir = tempfile::tempdir().unwrap();
        let body = r#"
version = 3

[[package]]
name = "my-fork"
version = "0.1.0"
source = "git+https://github.com/me/my-fork?branch=main#abc123"
"#;
        let path = write_lockfile(dir.path(), body);
        let entries = parse_lockfile(&path, &HashSet::new(), &HashSet::new()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source_type.as_deref(), Some("git"));
    }

    #[test]
    fn v1_lockfile_refused_with_contract_error() {
        let dir = tempfile::tempdir().unwrap();
        // v1 lockfiles have [root] and no version = field.
        let body = r#"
[root]
name = "app"
version = "0.1.0"
dependencies = []
"#;
        let path = write_lockfile(dir.path(), body);
        match parse_lockfile(&path, &HashSet::new(), &HashSet::new()) {
            Err(CargoError::LockfileUnsupportedVersion { version, .. }) => {
                assert_eq!(version, 1);
            }
            other => panic!("expected v1 refusal, got {other:?}"),
        }
    }

    #[test]
    fn v2_lockfile_refused_with_contract_error() {
        let dir = tempfile::tempdir().unwrap();
        // v2 lockfiles have [[package]] but no version = key.
        let body = r#"
[[package]]
name = "x"
version = "0.1.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "0000000000000000000000000000000000000000000000000000000000000000"
"#;
        let path = write_lockfile(dir.path(), body);
        match parse_lockfile(&path, &HashSet::new(), &HashSet::new()) {
            Err(CargoError::LockfileUnsupportedVersion { version, .. }) => {
                assert_eq!(version, 2);
            }
            other => panic!("expected v2 refusal, got {other:?}"),
        }
    }

    #[test]
    fn source_classification() {
        assert_eq!(classify_source(None), SourceKind::Workspace);
        assert_eq!(
            classify_source(Some("registry+https://x")),
            SourceKind::Registry
        );
        assert_eq!(
            classify_source(Some("git+https://github.com/x/y")),
            SourceKind::Git
        );
        assert_eq!(
            classify_source(Some("path+file:///absolute/path")),
            SourceKind::Path
        );
    }

    #[test]
    fn read_walks_nested_workspace() {
        // Verifies the walker descends into subdirectories to find
        // Cargo.lock. Per milestone 087 (issue #172), source=None
        // entries (workspace root + workspace members + path deps)
        // are emitted as PackageDbEntry rows so that multi-version-
        // same-name lookups in `scan_fs/mod.rs` can resolve to the
        // correct PURL. Milestone-064's Phase A augment-in-place
        // merge (cargo.rs::read) handles workspace-root labeling.
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("services").join("api");
        std::fs::create_dir_all(&sub).unwrap();
        let body = r#"
version = 3

[[package]]
name = "api-crate"
version = "0.1.0"

[[package]]
name = "serde"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "9e8eabd0c76a9f2678a5e1a5c0c7b8b8c99999999999999999999999999999999"
"#;
        std::fs::write(sub.join("Cargo.lock"), body).unwrap();
        let entries = read(dir.path(), false).unwrap();
        // Walk found the nested Cargo.lock and emitted the registry dep.
        assert!(entries.iter().any(|e| e.name == "serde"));
        // Milestone 087: workspace root IS now emitted (was filtered
        // pre-087); enables version-disambiguation lookups.
        assert!(entries.iter().any(|e| e.name == "api-crate"));
    }

    #[test]
    fn parse_lockfile_emits_workspace_root() {
        // A Cargo.lock with a workspace root (no source) and one
        // registry dep yields BOTH entries. Milestone 087 (issue
        // #172): the workspace root is emitted as a PackageDbEntry
        // with source_type = "workspace". The original conformance-
        // bug-2 self-referential FP is no longer produced because
        // milestone-064's Phase A augment-in-place merge labels the
        // workspace root with the C40 supplementary tag.
        let dir = tempfile::tempdir().unwrap();
        let body = r#"
version = 3

[[package]]
name = "my-app"
version = "0.1.0"

[[package]]
name = "serde"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"
"#;
        std::fs::write(dir.path().join("Cargo.lock"), body).unwrap();
        let entries = read(dir.path(), false).unwrap();
        assert_eq!(entries.len(), 2, "expected 2 entries, got {entries:?}");
        let app = entries
            .iter()
            .find(|e| e.name == "my-app")
            .expect("workspace root must be emitted");
        assert_eq!(app.source_type.as_deref(), Some("workspace"));
        assert!(entries.iter().any(|e| e.name == "serde"));
    }

    #[test]
    fn parse_lockfile_emits_all_workspace_members() {
        // Multi-crate workspace: per milestone 087 (issue #172), all
        // source = None entries (workspace root + workspace members)
        // are emitted so multi-version-same-name lookups can resolve
        // to the correct PURL. Each member gets source_type =
        // "workspace".
        let dir = tempfile::tempdir().unwrap();
        let body = r#"
version = 3

[[package]]
name = "my-app"
version = "0.1.0"

[[package]]
name = "my-lib"
version = "0.1.0"

[[package]]
name = "my-cli"
version = "0.1.0"

[[package]]
name = "anyhow"
version = "1.0.80"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "0a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393"
"#;
        std::fs::write(dir.path().join("Cargo.lock"), body).unwrap();
        let entries = read(dir.path(), false).unwrap();
        assert_eq!(entries.len(), 4, "all four entries should be present");
        assert!(entries.iter().any(|e| e.name == "my-app"));
        assert!(entries.iter().any(|e| e.name == "my-lib"));
        assert!(entries.iter().any(|e| e.name == "my-cli"));
        assert!(entries.iter().any(|e| e.name == "anyhow"));
    }

    #[test]
    fn read_empty_rootfs_returns_zero() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read(dir.path(), false).unwrap().is_empty());
    }

    #[test]
    fn read_v1_propagates_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.lock"),
            "[root]\nname = \"x\"\nversion = \"0.1.0\"\ndependencies = []\n",
        )
        .unwrap();
        assert!(matches!(
            read(dir.path(), false),
            Err(CargoError::LockfileUnsupportedVersion { version: 1, .. })
        ));
    }

    // ---- Milestone 051 — Cargo.toml dev/build classification ----

    #[test]
    fn parse_cargo_toml_extracts_three_sections() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cargo.toml");
        std::fs::write(
            &path,
            r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
serde = "1"
tokio = { version = "1", features = ["full"] }

[dev-dependencies]
criterion = "0.5"
proptest = "1"

[build-dependencies]
cc = "1"
"#,
        )
        .unwrap();
        let sections = parse_cargo_toml(&path).expect("parsed");
        assert!(sections.prod_deps.contains("serde"));
        assert!(sections.prod_deps.contains("tokio"));
        assert!(sections.dev_deps.contains("criterion"));
        assert!(sections.dev_deps.contains("proptest"));
        assert!(sections.build_deps.contains("cc"));
        assert_eq!(sections.prod_deps.len(), 2);
        assert_eq!(sections.dev_deps.len(), 2);
        assert_eq!(sections.build_deps.len(), 1);
    }

    #[test]
    fn parse_cargo_toml_walks_target_cfg_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cargo.toml");
        std::fs::write(
            &path,
            r#"
[package]
name = "demo"
version = "0.1.0"

[target."cfg(unix)".dev-dependencies]
nix = "0.27"

[target."cfg(windows)".build-dependencies]
winres = "0.1"
"#,
        )
        .unwrap();
        let sections = parse_cargo_toml(&path).expect("parsed");
        assert!(sections.dev_deps.contains("nix"));
        assert!(sections.build_deps.contains("winres"));
        assert!(sections.prod_deps.is_empty());
    }

    #[test]
    fn parse_cargo_toml_returns_none_on_malformed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cargo.toml");
        std::fs::write(&path, "this is = not valid [[ toml\n").unwrap();
        assert!(parse_cargo_toml(&path).is_none());
    }

    #[test]
    fn parse_cargo_toml_absent_sections_yield_empty_sets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cargo.toml");
        std::fs::write(
            &path,
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let sections = parse_cargo_toml(&path).expect("parsed");
        assert!(sections.prod_deps.is_empty());
        assert!(sections.dev_deps.is_empty());
        assert!(sections.build_deps.is_empty());
    }

    #[test]
    fn parse_cargo_toml_honors_package_rename() {
        // `foo = { package = "real-name", version = "1" }` resolves
        // to crate `real-name` in Cargo.lock, not `foo`.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cargo.toml");
        std::fs::write(
            &path,
            r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
foo = { package = "real-name", version = "1" }
"#,
        )
        .unwrap();
        let sections = parse_cargo_toml(&path).expect("parsed");
        assert!(sections.prod_deps.contains("real-name"));
        assert!(!sections.prod_deps.contains("foo"));
    }

    fn lock_pkg(name: &str, version: &str, deps: &[&str]) -> CargoPackage {
        CargoPackage {
            name: name.to_string(),
            version: version.to_string(),
            source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
            checksum: None,
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn compute_prod_set_returns_just_direct_when_no_transitives() {
        let lock = CargoLock {
            version: Some(3),
            package: vec![lock_pkg("foo", "1.0.0", &[])],
        };
        let mut direct = HashSet::new();
        direct.insert("foo".to_string());
        let prod = compute_cargo_prod_set(&lock, &direct);
        assert_eq!(prod.len(), 1);
        assert!(prod.contains(&("foo".to_string(), "1.0.0".to_string())));
    }

    #[test]
    fn compute_prod_set_walks_three_level_chain() {
        let lock = CargoLock {
            version: Some(3),
            package: vec![
                lock_pkg("a", "1.0.0", &["b 2.0.0"]),
                lock_pkg("b", "2.0.0", &["c 3.0.0"]),
                lock_pkg("c", "3.0.0", &[]),
            ],
        };
        let mut direct = HashSet::new();
        direct.insert("a".to_string());
        let prod = compute_cargo_prod_set(&lock, &direct);
        assert_eq!(prod.len(), 3);
        assert!(prod.contains(&("a".to_string(), "1.0.0".to_string())));
        assert!(prod.contains(&("b".to_string(), "2.0.0".to_string())));
        assert!(prod.contains(&("c".to_string(), "3.0.0".to_string())));
    }

    #[test]
    fn compute_prod_set_production_wins_over_dev() {
        // `shared` is reachable from BOTH the prod root `a` and (in
        // a real workspace) a dev root. From the BFS perspective —
        // which is seeded from prod-direct only — `shared` lands
        // in the prod set when reachable through prod chain.
        let lock = CargoLock {
            version: Some(3),
            package: vec![
                lock_pkg("a", "1.0.0", &["shared 1.0.0"]),
                lock_pkg("dev-only", "1.0.0", &["shared 1.0.0"]),
                lock_pkg("shared", "1.0.0", &[]),
            ],
        };
        let mut direct = HashSet::new();
        direct.insert("a".to_string());
        let prod = compute_cargo_prod_set(&lock, &direct);
        assert!(prod.contains(&("shared".to_string(), "1.0.0".to_string())));
        // dev-only is NOT in the prod set — exactly what we want.
        assert!(!prod.contains(&("dev-only".to_string(), "1.0.0".to_string())));
    }

    #[test]
    fn compute_prod_set_empty_seed_returns_empty() {
        let lock = CargoLock {
            version: Some(3),
            package: vec![lock_pkg("foo", "1.0.0", &[])],
        };
        let prod = compute_cargo_prod_set(&lock, &HashSet::new());
        assert!(prod.is_empty());
    }

    #[test]
    fn discover_workspace_manifests_includes_root_only_when_no_workspace() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"solo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("Cargo.lock"), "version = 3\n").unwrap();
        let manifests = discover_workspace_manifests(&dir.path().join("Cargo.lock"));
        assert_eq!(manifests.len(), 1);
        assert!(manifests[0].ends_with("Cargo.toml"));
    }

    #[test]
    fn discover_workspace_manifests_expands_glob_members() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("Cargo.lock"), "version = 3\n").unwrap();
        std::fs::create_dir_all(dir.path().join("crates/foo")).unwrap();
        std::fs::create_dir_all(dir.path().join("crates/bar")).unwrap();
        std::fs::write(
            dir.path().join("crates/foo/Cargo.toml"),
            "[package]\nname = \"foo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("crates/bar/Cargo.toml"),
            "[package]\nname = \"bar\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let manifests = discover_workspace_manifests(&dir.path().join("Cargo.lock"));
        // Root + 2 members.
        assert_eq!(manifests.len(), 3);
    }

    /// Milestone 054 SC-002 + FR-009: walker terminates promptly on
    /// a synthesized minimal symlink-loop fixture instead of hanging.
    ///
    /// Milestone 100: `#[cfg(unix)]` — POSIX-only symlink API.
    #[cfg(unix)]
    #[test]
    fn walks_symlink_loop_without_hanging() {
        let tmp = tempfile::tempdir().unwrap();
        let loop_dir = tmp.path().join("loop");
        std::fs::create_dir_all(&loop_dir).unwrap();
        std::os::unix::fs::symlink(&loop_dir, loop_dir.join("link")).unwrap();
        let result = find_cargo_lockfiles(tmp.path());
        // No Cargo.lock in the synthesized fixture; the test only
        // asserts the call returned (didn't hang).
        assert!(result.is_empty());
    }

    // -------------------------------------------------------------------
    // Milestone 064 — main-module emission helpers (T013)
    // -------------------------------------------------------------------

    fn write_manifest(dir: &Path, contents: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join("Cargo.toml");
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn workspace_context_records_workspace_package_version() {
        let tmp = tempfile::tempdir().unwrap();
        let root_path = write_manifest(
            tmp.path(),
            r#"
[workspace]
members = ["a"]

[workspace.package]
version = "1.0.0"
"#,
        );
        let ctx = WorkspaceContext::build_from_manifests(std::slice::from_ref(&root_path));
        assert_eq!(
            ctx.lookup_for_member(tmp.path()).map(|s| s.to_string()),
            Some("1.0.0".to_string()),
        );
    }

    #[test]
    fn workspace_context_walks_up_for_member() {
        let tmp = tempfile::tempdir().unwrap();
        let root_manifest = write_manifest(
            tmp.path(),
            r#"
[workspace]
members = ["a"]

[workspace.package]
version = "2.5.0"
"#,
        );
        let member_dir = tmp.path().join("a");
        std::fs::create_dir_all(&member_dir).unwrap();
        let ctx = WorkspaceContext::build_from_manifests(&[root_manifest]);
        // Member crate dir walks up to the workspace root.
        assert_eq!(
            ctx.lookup_for_member(&member_dir).map(|s| s.to_string()),
            Some("2.5.0".to_string()),
        );
    }

    #[test]
    fn workspace_context_returns_none_when_no_workspace_package_version() {
        let tmp = tempfile::tempdir().unwrap();
        let root = write_manifest(
            tmp.path(),
            r#"
[workspace]
members = ["a"]
"#,
        );
        let ctx = WorkspaceContext::build_from_manifests(&[root]);
        assert!(ctx.lookup_for_member(tmp.path()).is_none());
    }

    #[test]
    fn resolve_version_uses_literal_string() {
        let pkg: toml::Value = toml::from_str(r#"name = "x"
version = "3.4.5"
"#).unwrap();
        let ctx = WorkspaceContext::default();
        let resolved =
            resolve_cargo_main_module_version(Path::new("/tmp/x"), &pkg, &ctx);
        assert_eq!(resolved, "3.4.5");
    }

    #[test]
    fn resolve_version_resolves_workspace_inheritance() {
        let pkg: toml::Value = toml::from_str(r#"name = "x"
version = { workspace = true }
"#).unwrap();
        let mut ctx = WorkspaceContext::default();
        ctx.versions.insert(
            PathBuf::from("/tmp/myproject"),
            "0.7.2".to_string(),
        );
        let resolved = resolve_cargo_main_module_version(
            Path::new("/tmp/myproject/crates/x"),
            &pkg,
            &ctx,
        );
        assert_eq!(resolved, "0.7.2");
    }

    #[test]
    fn resolve_version_falls_back_to_placeholder_when_unresolvable() {
        let pkg: toml::Value = toml::from_str(r#"name = "x"
version = { workspace = true }
"#).unwrap();
        let ctx = WorkspaceContext::default();
        let resolved =
            resolve_cargo_main_module_version(Path::new("/tmp/x"), &pkg, &ctx);
        assert_eq!(resolved, "0.0.0-unknown");
    }

    #[test]
    fn build_cargo_main_module_entry_basic_package() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = write_manifest(
            tmp.path(),
            r#"
[package]
name = "foo"
version = "1.2.3"
edition = "2021"
"#,
        );
        let ctx = WorkspaceContext::default();
        let entry = build_cargo_main_module_entry(&manifest, &ctx).unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:cargo/foo@1.2.3");
        assert_eq!(entry.name, "foo");
        assert_eq!(entry.version, "1.2.3");
        assert_eq!(entry.parent_purl, None);
        assert_eq!(entry.sbom_tier.as_deref(), Some("source"));
        assert!(entry.licenses.is_empty());
        assert_eq!(
            entry
                .extra_annotations
                .get("mikebom:component-role")
                .and_then(|v| v.as_str()),
            Some("main-module"),
        );
    }

    #[test]
    fn build_cargo_main_module_entry_workspace_only_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = write_manifest(
            tmp.path(),
            r#"
[workspace]
members = ["a"]
"#,
        );
        let ctx = WorkspaceContext::default();
        assert!(build_cargo_main_module_entry(&manifest, &ctx).is_none());
    }

    #[test]
    fn build_cargo_main_module_entry_resolves_workspace_version() {
        let tmp = tempfile::tempdir().unwrap();
        let root_manifest = write_manifest(
            tmp.path(),
            r#"
[workspace]
members = ["a"]

[workspace.package]
version = "0.5.0"
"#,
        );
        let member_dir = tmp.path().join("a");
        let member_manifest = write_manifest(
            &member_dir,
            r#"
[package]
name = "a"
version.workspace = true
"#,
        );
        let ctx = WorkspaceContext::build_from_manifests(&[
            root_manifest,
            member_manifest.clone(),
        ]);
        let entry =
            build_cargo_main_module_entry(&member_manifest, &ctx).unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:cargo/a@0.5.0");
    }

    #[test]
    fn build_cargo_main_module_entry_preserves_hyphenated_name() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = write_manifest(
            tmp.path(),
            r#"
[package]
name = "foo-bar"
version = "1.0.0"
"#,
        );
        let ctx = WorkspaceContext::default();
        let entry = build_cargo_main_module_entry(&manifest, &ctx).unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:cargo/foo-bar@1.0.0");
        assert_eq!(entry.name, "foo-bar");
    }

    #[test]
    fn build_cargo_main_module_entry_preserves_pre_release_version() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = write_manifest(
            tmp.path(),
            r#"
[package]
name = "x"
version = "0.1.0-alpha.11"
"#,
        );
        let ctx = WorkspaceContext::default();
        let entry = build_cargo_main_module_entry(&manifest, &ctx).unwrap();
        // Pre-release and build-metadata SemVer parts pass through PURL
        // segment encoding intact (`-` and `.` are unreserved).
        assert!(entry.purl.as_str().contains("0.1.0-alpha.11"));
    }

    fn make_main_module_entry(
        name: &str,
        version: &str,
        source_path: &str,
    ) -> PackageDbEntry {
        let purl = build_cargo_purl(name, version).unwrap();
        let mut extra: std::collections::BTreeMap<String, serde_json::Value> =
            Default::default();
        extra.insert(
            "mikebom:component-role".to_string(),
            serde_json::Value::String("main-module".to_string()),
        );
        PackageDbEntry {
            purl,
            name: name.to_string(),
            version: version.to_string(),
            arch: None,
            source_path: source_path.to_string(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: None,
            requirement_range: None,
            source_type: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            raw_version: None,
            parent_purl: None,
            npm_role: None,
            co_owned_by: None,
            hashes: Vec::new(),
            sbom_tier: Some("source".to_string()),
            shade_relocation: None,
            extra_annotations: extra,
        }
    }

    fn make_regular_entry(name: &str, version: &str) -> PackageDbEntry {
        let purl = build_cargo_purl(name, version).unwrap();
        PackageDbEntry {
            purl,
            name: name.to_string(),
            version: version.to_string(),
            arch: None,
            source_path: String::new(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: None,
            requirement_range: None,
            source_type: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            raw_version: None,
            parent_purl: None,
            npm_role: None,
            co_owned_by: None,
            hashes: Vec::new(),
            sbom_tier: None,
            shade_relocation: None,
            extra_annotations: Default::default(),
        }
    }

    #[test]
    fn dedup_no_collisions_returns_empty() {
        let mut entries = vec![
            make_main_module_entry("a", "1.0.0", "/tmp/a"),
            make_main_module_entry("b", "1.0.0", "/tmp/b"),
        ];
        let drops = dedup_main_modules_by_purl(&mut entries);
        assert_eq!(entries.len(), 2);
        assert!(drops.is_empty());
    }

    #[test]
    fn dedup_two_same_purl_keeps_first() {
        let mut entries = vec![
            make_main_module_entry("foo", "1.2.3", "/tmp/crates/foo"),
            make_main_module_entry("foo", "1.2.3", "/tmp/vendor/foo"),
        ];
        let drops = dedup_main_modules_by_purl(&mut entries);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source_path, "/tmp/crates/foo");
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].purl, "pkg:cargo/foo@1.2.3");
        assert_eq!(drops[0].kept_path, "/tmp/crates/foo");
        assert_eq!(drops[0].dropped_path, "/tmp/vendor/foo");
    }

    #[test]
    fn dedup_three_same_purl_drops_two() {
        let mut entries = vec![
            make_main_module_entry("foo", "1.2.3", "/tmp/a/foo"),
            make_main_module_entry("foo", "1.2.3", "/tmp/b/foo"),
            make_main_module_entry("foo", "1.2.3", "/tmp/c/foo"),
        ];
        let drops = dedup_main_modules_by_purl(&mut entries);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source_path, "/tmp/a/foo");
        assert_eq!(drops.len(), 2);
    }

    #[test]
    fn dedup_does_not_touch_non_main_module_entries() {
        // Two regular entries with the same PURL — caller is responsible
        // for that dedup, not us. Our predicate only fires on main-module
        // tagged entries.
        let mut entries = vec![
            make_regular_entry("foo", "1.2.3"),
            make_regular_entry("foo", "1.2.3"),
        ];
        let drops = dedup_main_modules_by_purl(&mut entries);
        assert_eq!(entries.len(), 2);
        assert!(drops.is_empty());
    }

    // Milestone 087 (issue #172): regression for the multi-version
    // disambiguation bug. `package_to_entry` MUST preserve the
    // `<name> <version>` form when Cargo emits it (multi-version
    // ambiguity); the bare `<name>` form is preserved for
    // unambiguous deps. The ` (source)` suffix MUST be stripped.
    #[test]
    fn package_to_entry_preserves_version_disambiguation() {
        let pkg = CargoPackage {
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
            source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
            checksum: None,
            dependencies: vec![
                "foo".to_string(),
                "bar 1.0.0".to_string(),
                "baz 2.0.0 (registry+https://github.com/rust-lang/crates.io-index)".to_string(),
            ],
        };
        let entry = package_to_entry(&pkg, "/tmp/Cargo.lock").unwrap();
        assert_eq!(
            entry.depends,
            vec![
                "foo".to_string(),
                "bar 1.0.0".to_string(),
                "baz 2.0.0".to_string(),
            ],
        );
    }
}