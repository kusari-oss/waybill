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
    // `<name> <version> (registry+...)`. Take just the name.
    let depends: Vec<String> = pkg
        .dependencies
        .iter()
        .map(|d| {
            d.split_whitespace()
                .next()
                .unwrap_or(d)
                .to_string()
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
        // Conformance bug 2 fix: skip workspace-root / path-only
        // entries. A Cargo.lock `[[package]]` with no `source` field
        // IS the scanned project (or a workspace member), not a
        // dependency. Emitting it produces a self-referential FP like
        // `pkg:cargo/my-app@0.1.0` in the SBOM. Mirrors npm.rs's
        // path_key == "" skip at parse_package_lock.
        //
        // Trade-off: path dependencies (`path = "../foo"` in Cargo.toml)
        // also have `source = None` and will be dropped. Same behavior
        // as npm (which skips `link: true` workspace entries).
        // Revisit via `--include-path-deps` if a real use case appears.
        if pkg.source.is_none() {
            tracing::debug!(
                name = %pkg.name,
                version = %pkg.version,
                "skipping cargo workspace-root/path entry (source absent)",
            );
            continue;
        }
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
    for lock_path in find_cargo_lockfiles(rootfs) {
        // Per-lockfile: build the workspace-wide CargoTomlSections by
        // unioning the root manifest + every workspace member.
        let mut workspace_sections = CargoTomlSections::default();
        for manifest_path in discover_workspace_manifests(&lock_path) {
            if let Some(sections) = parse_cargo_toml(&manifest_path) {
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
    if !out.is_empty() {
        tracing::info!(
            rootfs = %rootfs.display(),
            entries = out.len(),
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
    walk_for_cargo_lockfiles(rootfs, 0, &mut out);
    out
}

fn walk_for_cargo_lockfiles(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let lock = dir.join("Cargo.lock");
    if lock.is_file() {
        out.push(lock);
    }
    if depth >= MAX_PROJECT_ROOT_DEPTH {
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
        walk_for_cargo_lockfiles(&path, depth + 1, out);
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
        // Cargo.lock. Uses a registry dep alongside the workspace root
        // so the registry dep's presence confirms the walk succeeded
        // (the workspace root is filtered by design — see
        // parse_lockfile_skips_workspace_root).
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
        // Workspace root is filtered.
        assert!(!entries.iter().any(|e| e.name == "api-crate"));
    }

    #[test]
    fn parse_lockfile_skips_workspace_root() {
        // A Cargo.lock with a workspace root (no source) and one registry
        // dep should yield only the registry dep. The root is the
        // scanned project itself — emitting it as a component produces
        // a self-referential FP. Matches npm.rs's path_key == "" skip.
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
        assert_eq!(entries.len(), 1, "expected 1 entry, got {entries:?}");
        assert_eq!(entries[0].name, "serde");
    }

    #[test]
    fn parse_lockfile_skips_all_workspace_members() {
        // Multi-crate workspace: all members have source = None and
        // should all be filtered, leaving only registry deps.
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
        assert_eq!(entries.len(), 1, "only the registry dep should remain");
        assert_eq!(entries[0].name, "anyhow");
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
}