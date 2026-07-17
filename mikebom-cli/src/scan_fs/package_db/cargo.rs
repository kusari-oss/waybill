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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use mikebom_common::divergence::{
    DivergenceReason, DivergenceRecord, DIVERGENCE_SCHEMA_VERSION,
};
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

// -----------------------------------------------------------------
// Milestone 205 (#593) — cargo metadata subprocess + fallback types
// -----------------------------------------------------------------

/// Failure classes from `cargo metadata --format-version 1 --offline
/// --locked` shell-out. Every variant maps to the FR-004 fallback
/// path: emit a WARN log naming the workspace + reason, then populate
/// `activated_names` with the full `optional_names` set (safe over-
/// inclusion — every optional dep flips to Runtime so downstream vuln-
/// scanners never miss shipped deps). Display strings are wire-
/// stable per data-model E1 (tests grep stderr for them).
#[derive(Debug, thiserror::Error)]
pub(super) enum CargoMetadataResolveFailure {
    #[error("`cargo` binary not found on $PATH")]
    BinaryNotFound,
    #[error("`cargo metadata` exited with code {code}; stderr head: {stderr_head}")]
    NonZeroExit { code: i32, stderr_head: String },
    #[error("`cargo metadata` exceeded {timeout_secs}s timeout")]
    Timeout { timeout_secs: u64 },
    #[error("`cargo metadata` JSON parse failed: {source}")]
    ParseError { source: serde_json::Error },
    #[error("`cargo metadata` I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Milestone 205 (FR-002 / FR-006): read `MIKEBOM_CARGO_METADATA_TIMEOUT_SECS`
/// env var; parse as `u64`; clamp to `[1, 3600]`; default 60 on
/// absent/parse-fail. Mirrors m203's `resolve_render_timeout`
/// posture (silent parse-failure fallback matches every mikebom
/// env-var handler since m089).
fn resolve_cargo_metadata_timeout() -> Duration {
    const DEFAULT_SECS: u64 = 60;
    const MIN_SECS: u64 = 1;
    const MAX_SECS: u64 = 3600;
    let secs = std::env::var("MIKEBOM_CARGO_METADATA_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(|n| n.clamp(MIN_SECS, MAX_SECS))
        .unwrap_or(DEFAULT_SECS);
    Duration::from_secs(secs)
}

fn cargo_metadata_cap_stderr_lines(bytes: &[u8], max_lines: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.lines().take(max_lines).collect::<Vec<_>>().join("\n")
}

/// Milestone 205 (#593) — shell out to `cargo metadata --format-
/// version 1 --offline --locked` in `workspace_root`; parse the JSON;
/// return the union of `resolve.nodes[].deps[].name` — i.e., the set
/// of dep NAMES that Cargo's actual resolver activates under the
/// enabled feature set. Consumers (the classifier at line 1155)
/// treat this as the ground truth for "which optional deps are truly
/// optional in effect."
///
/// **Flag rationale** (data-model E3 + FR-006):
/// - `--offline` — REQUIRED per FR-006. Blocks cargo from reaching
///   over the wire to update the registry index. Without it a
///   workspace whose Cargo.toml declares a dep whose version isn't
///   in the local index cache would trigger a network fetch.
///
/// Note: `--locked` is intentionally NOT set. It rejects any
/// lockfile-vs-registry-cache checksum drift with an error — too
/// strict for typical operator workspaces (routinely-stale
/// Cargo.lock vs a slightly-newer registry cache is common and
/// benign). Under `--offline` alone, cargo may rewrite Cargo.lock
/// as a side effect if the manifest was touched — but for a scan
/// of a directory at rest that's a no-op; the operator's next
/// `cargo build` would do the same rewrite. FR-007 (determinism)
/// is preserved: mikebom's OWN output is a pure function of the
/// resolved metadata, which is deterministic given the workspace
/// state at scan time.
///
/// On failure (BinaryNotFound / NonZeroExit / Timeout / ParseError
/// / IoError) → FR-004 fallback handles uniformly.
///
/// Follows the m055 subprocess-with-timeout pattern (thread + mpsc
/// + recv_timeout) verbatim.
pub(super) fn resolve_activated_deps_via_cargo_metadata(
    workspace_root: &Path,
    timeout: Duration,
) -> Result<HashSet<String>, CargoMetadataResolveFailure> {
    use std::process::Command;
    use std::sync::mpsc;
    use std::thread;

    // Probe: `cargo --version` — fails fast on missing binary.
    match Command::new("cargo").arg("--version").output() {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(CargoMetadataResolveFailure::BinaryNotFound);
        }
        Err(e) => return Err(CargoMetadataResolveFailure::IoError(e)),
    }

    let (tx, rx) = mpsc::channel();
    let ws_owned = workspace_root.to_path_buf();
    thread::spawn(move || {
        let result = Command::new("cargo")
            .args(["metadata", "--format-version", "1", "--offline"])
            .current_dir(&ws_owned)
            .output();
        let _ = tx.send(result);
    });

    let output = match rx.recv_timeout(timeout) {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(CargoMetadataResolveFailure::IoError(e)),
        Err(_) => {
            return Err(CargoMetadataResolveFailure::Timeout {
                timeout_secs: timeout.as_secs(),
            });
        }
    };

    if !output.status.success() {
        return Err(CargoMetadataResolveFailure::NonZeroExit {
            code: output.status.code().unwrap_or(-1),
            stderr_head: cargo_metadata_cap_stderr_lines(&output.stderr, 20),
        });
    }

    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|source| CargoMetadataResolveFailure::ParseError { source })?;

    let mut activated: HashSet<String> = HashSet::new();
    if let Some(nodes) = parsed.pointer("/resolve/nodes").and_then(|v| v.as_array()) {
        for node in nodes {
            if let Some(deps) = node.get("deps").and_then(|v| v.as_array()) {
                for dep in deps {
                    if let Some(name) = dep.get("name").and_then(|v| v.as_str()) {
                        activated.insert(name.to_string());
                    }
                }
            }
        }
    }
    Ok(activated)
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
    //
    // Milestone 191 (#558): when version is empty (design-tier
    // component with no source-tier resolution), emit a versionless
    // PURL per purl-spec canonical form — no trailing `@`.
    let purl_str = if version.is_empty() {
        format!("pkg:cargo/{}", encode_purl_segment(name))
    } else {
        format!(
            "pkg:cargo/{}@{}",
            encode_purl_segment(name),
            encode_purl_segment(version),
        )
    };
    Purl::new(&purl_str).ok()
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
        build_inclusion: None,
        purl,
        name: pkg.name.clone(),
        version: pkg.version.clone(),
        arch: None,
        source_path: source_path.to_string(),
        depends,
        maintainer,
        licenses,
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
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
        binary_role: None,
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

    // Milestone 201 (FR-001, closes #587): stamp the workspace-toplevel
    // positive-identifier annotation when this manifest has BOTH a
    // [package] block (already checked above via package.get("name"))
    // AND a [workspace] block. Consumed downstream by scan_fs/mod.rs's
    // is_workspace_root stamping to distinguish workspace-ROOT crates
    // from workspace-MEMBER crates — cargo m064's augment-in-place
    // makes both types share the workspace Cargo.lock path in
    // evidence.source_file_paths, defeating the filesystem-based
    // check. Internal-emission-only: filtered from CDX/SPDX output
    // via is_internal_emission_key at root_selector.rs.
    if parsed.get("workspace").is_some() {
        extra_annotations.insert(
            "mikebom:is-cargo-workspace-toplevel".to_string(),
            serde_json::Value::Bool(true),
        );
    }

    // Milestone 116 — produces-binaries extraction per FR-005 (Cargo).
    // Three sources per Cargo's implicit-binary convention:
    //   (a) Explicit `[[bin]]` table entries.
    //   (b) Default-binary inference: `src/main.rs` exists → binary
    //       named after the package's `name` field.
    //   (c) Implicit `src/bin/*.rs` files (depth-1 only per Cargo docs).
    let mut binary_candidates: Vec<String> =
        extract_cargo_bin_table_names(&parsed);
    if manifest_dir.join("src").join("main.rs").is_file() {
        binary_candidates.push(name.to_string());
    }
    binary_candidates.extend(extract_cargo_src_bin_names(manifest_dir));
    crate::scan_fs::produces_binaries::stamp_into_annotations(
        &mut extra_annotations,
        binary_candidates,
    );

    let source_path = format!("path+file://{}", manifest_dir.display());
    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name: name.to_string(),
        version,
        arch: None,
        source_path,
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
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
        binary_role: None,
    })
}

/// Milestone 116 — extract explicit `[[bin]]` table entries' `name`
/// fields from a parsed `Cargo.toml`. Filters out entries without a
/// `name` field (those use Cargo's default-name-from-filename rule
/// which we cover via the `src/bin/*.rs` walk).
fn extract_cargo_bin_table_names(parsed: &toml::Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(arr) = parsed.get("bin").and_then(|v| v.as_array()) {
        for entry in arr {
            if let Some(n) = entry.get("name").and_then(|v| v.as_str()) {
                out.push(n.to_string());
            }
        }
    }
    out
}

/// Milestone 116 — enumerate Cargo's implicit `src/bin/<name>.rs`
/// binaries via `safe_walk` (depth-1 only per Cargo docs; nested
/// subdirectories of `src/bin/` are NOT implicit binaries). Returns
/// the file stems as candidate binary names.
fn extract_cargo_src_bin_names(manifest_dir: &Path) -> Vec<String> {
    let src_bin = manifest_dir.join("src").join("bin");
    if !src_bin.is_dir() {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    let exclude_set = super::exclude_path::ExclusionSet::new_empty();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: 1,
        should_skip: &|_p, _root| false,
        exclude_set: &exclude_set,
    };
    crate::scan_fs::walk::safe_walk(&src_bin, &cfg, |path| {
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rs") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                out.push(stem.to_string());
            }
        }
    });
    out
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

/// Milestone 134 — per-manifest accumulation for divergent-PURL
/// collision detection. Built alongside each `build_cargo_main_module_
/// entry` call so the dedup step has every colliding manifest's
/// declared dep set (and optional deep-hash) available.
///
/// Stays a private helper in the cargo reader — future ecosystem
/// expansions (npm / maven / pip / gem / go-binary) introduce their
/// own analogous candidate types and converge on the shared
/// [`DivergenceRecord`] at the format-emitter boundary.
#[derive(Debug, Clone)]
pub(crate) struct CargoManifestCandidate {
    /// Matches the deduped [`PackageDbEntry::source_path`] (the
    /// `"path+file://<dir>"` form). Used to look up the surviving
    /// entry in [`dedup_main_modules_by_purl`].
    pub entry_source_path: String,
    /// Rootfs-relative path to the manifest file itself (e.g.
    /// `"crates/foo/Cargo.toml"`), forward-slash normalized. This is
    /// the value that lands in the emitted `paths[]` field.
    pub display_path: String,
    /// Captured for symmetry / future ecosystem-agnostic reuse; the
    /// surviving entry's PURL is what drives dedup grouping today.
    #[allow(dead_code)]
    pub purl: Purl,
    /// Union of `[dependencies]` / `[dev-dependencies]` /
    /// `[build-dependencies]` table keys, sorted lex. Used for the
    /// `dep_sets_by_path` map and pairwise compare.
    pub declared_deps: BTreeSet<String>,
    /// Per-manifest deep-hash of the crate directory tree. Populated
    /// only when `--deep-hash` is set; otherwise `None`. Drives the
    /// `hashes_by_path` map and the `HashesDiffer` / `Both` reason
    /// classification when groups disagree.
    pub deep_hash: Option<String>,
}

/// Dedup main-module entries by PURL, preserving the first occurrence
/// (deterministic on the existing walker order — `discover_workspace_
/// manifests` returns root-then-members in declaration order). Returns
/// the list of dropped duplicates for caller-side `tracing::warn!`
/// emission per FR-001 + spec Clarifications Q1. Non-main-module
/// entries (the predicate is C40-tag-driven) are left untouched even
/// if their PURLs would collide with each other.
///
/// Milestone 134 note: divergence detection runs in a separate
/// helper, [`detect_divergent_collisions`], because milestone-064's
/// augment-in-place loop already collapses same-PURL Cargo.toml
/// files before this function sees `entries` — so the dedup function
/// rarely observes cargo same-PURL drops, and divergence has to be
/// detected against the candidates map directly.
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

/// Milestone 134 — compute divergence records over the per-manifest
/// candidates collected during Phase A AND stamp the per-component
/// `mikebom:duplicate-purl-divergent` annotation on the deduped
/// surviving entry.
///
/// Runs independently of [`dedup_main_modules_by_purl`] because
/// milestone-064's Phase A augment-in-place loop already collapses
/// same-PURL Cargo.toml files into one `PackageDbEntry` before this
/// function sees them — so the divergence detector cannot rely on
/// the dedup function's drop list. Instead we group every collected
/// `CargoManifestCandidate` by PURL and emit a `DivergenceRecord`
/// for every group with 2+ candidates whose declared dep sets (or
/// deep hashes) differ.
pub(crate) fn detect_divergent_collisions(
    entries: &mut [PackageDbEntry],
    candidates: &[CargoManifestCandidate],
) -> Vec<DivergenceRecord> {
    let mut by_purl: BTreeMap<String, Vec<&CargoManifestCandidate>> = BTreeMap::new();
    for c in candidates {
        by_purl
            .entry(c.purl.as_str().to_string())
            .or_default()
            .push(c);
    }
    let mut divergences: Vec<DivergenceRecord> = Vec::new();
    for (purl_str, mut group) in by_purl {
        if group.len() < 2 {
            continue;
        }
        // Determinism: sort by display path (lex) for stable wire
        // output across runs.
        group.sort_by(|a, b| a.display_path.cmp(&b.display_path));
        let display_paths: Vec<String> =
            group.iter().map(|c| c.display_path.clone()).collect();

        let mut dep_sets: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut hashes: BTreeMap<String, String> = BTreeMap::new();
        let mut all_have_hashes = true;
        for c in &group {
            dep_sets.insert(
                c.display_path.clone(),
                c.declared_deps.iter().cloned().collect::<Vec<_>>(),
            );
            if let Some(h) = &c.deep_hash {
                hashes.insert(c.display_path.clone(), h.clone());
            } else {
                all_have_hashes = false;
            }
        }
        let dep_values: Vec<&Vec<String>> = display_paths
            .iter()
            .filter_map(|p| dep_sets.get(p))
            .collect();
        let deps_differ = dep_values.windows(2).any(|w| w[0] != w[1]);
        let hashes_differ = if all_have_hashes && hashes.len() == display_paths.len() {
            let hash_values: Vec<&String> = display_paths
                .iter()
                .filter_map(|p| hashes.get(p))
                .collect();
            hash_values.windows(2).any(|w| w[0] != w[1])
        } else {
            false
        };
        if !deps_differ && !hashes_differ {
            continue;
        }
        let reason = match (deps_differ, hashes_differ) {
            (true, true) => DivergenceReason::Both,
            (true, false) => DivergenceReason::DepsDiffer,
            (false, true) => DivergenceReason::HashesDiffer,
            (false, false) => unreachable!("guarded above"),
        };
        let Ok(purl) = Purl::new(&purl_str) else {
            continue;
        };
        let record = DivergenceRecord {
            v: DIVERGENCE_SCHEMA_VERSION,
            purl,
            reason,
            paths: display_paths,
            dep_sets_by_path: if matches!(
                reason,
                DivergenceReason::DepsDiffer | DivergenceReason::Both
            ) {
                Some(dep_sets)
            } else {
                None
            },
            hashes_by_path: if matches!(
                reason,
                DivergenceReason::HashesDiffer | DivergenceReason::Both
            ) {
                Some(hashes)
            } else {
                None
            },
        };
        debug_assert!(record.validate().is_ok(), "{:?}", record.validate());

        // Stamp the per-component annotation on the surviving entry
        // (matched by PURL).
        if let Some(entry) = entries.iter_mut().find(|e| e.purl.as_str() == purl_str) {
            let value = serde_json::to_value(&record)
                .expect("DivergenceRecord serializes infallibly");
            entry.extra_annotations.insert(
                "mikebom:duplicate-purl-divergent".to_string(),
                value,
            );
        }
        divergences.push(record);
    }
    divergences
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
    /// Milestone 179 US3 — names declared in `[dependencies]` (or
    /// `[target.<cfg>.dependencies]`) with `optional = true`. These
    /// are feature-gated: they only participate in a build when a
    /// `[features]` entry activates them via `dep:<name>`. mikebom
    /// tags matching resolved components with
    /// [`LifecycleScope::Optional`] so SPDX 2.3 emits
    /// `OPTIONAL_DEPENDENCY_OF` and CDX emits `scope: "excluded"`.
    /// Companion annotation: `mikebom:optional-derivation =
    /// cargo-optional-true`.
    pub optional_deps: HashSet<String>,
    /// Milestone 200 (FR-001, closes #585) — root `[package].name`
    /// values collected from every parseable workspace `Cargo.toml`.
    /// Used at the classifier cascade to short-circuit workspace-root
    /// [[package]] entries to `LifecycleScope::Runtime` regardless of
    /// prod-set BFS membership. NOT added to `prod_deps` because
    /// Cargo.lock's per-[[package]] `dependencies = [...]` list
    /// unifies runtime + dev + build deps — walking BFS from the root's
    /// name would pull dev/build deps into `prod_set`, violating
    /// FR-003. The classifier check `pkg.source.is_none() && pkg.name
    /// ∈ root_names` targets the workspace-root [[package]] itself
    /// without affecting its transitive closure.
    pub root_names: HashSet<String>,
}

impl CargoTomlSections {
    /// Union with another sections set — used to merge the workspace
    /// root + every member crate into a single workspace-wide
    /// classification view.
    fn union(&mut self, other: &CargoTomlSections) {
        self.prod_deps.extend(other.prod_deps.iter().cloned());
        self.dev_deps.extend(other.dev_deps.iter().cloned());
        self.build_deps.extend(other.build_deps.iter().cloned());
        self.optional_deps.extend(other.optional_deps.iter().cloned());
        self.root_names.extend(other.root_names.iter().cloned());
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
    // Milestone 179 US3 — track `optional = true` entries within
    // `[dependencies]`. Only the runtime table can carry the flag;
    // Cargo disallows it (or ignores it) in `[dev-dependencies]` and
    // `[build-dependencies]`. This is FR-015 precedence enforced at
    // the reader boundary: if the manifest already classified the dep
    // as dev/build, the optional flag inside that table doesn't
    // reclassify it.
    collect_optional_dep_keys(&parsed, "dependencies", &mut out.optional_deps);
    // Walk every `target.<cfg>` table for its three section variants.
    if let Some(target_table) = parsed.get("target").and_then(|v| v.as_table()) {
        for (_cfg, target_value) in target_table {
            collect_section_keys(target_value, "dependencies", &mut out.prod_deps);
            collect_section_keys(target_value, "dev-dependencies", &mut out.dev_deps);
            collect_section_keys(target_value, "build-dependencies", &mut out.build_deps);
            collect_optional_dep_keys(target_value, "dependencies", &mut out.optional_deps);
        }
    }
    // Milestone 200 (FR-001, closes #585): record the manifest's root
    // `[package].name` in the `root_names` set for later classifier
    // short-circuit. This is DELIBERATELY separate from `prod_deps` —
    // seeding the prod-set BFS from `[package].name` would over-reach
    // because Cargo.lock's per-[[package]] `dependencies = [...]` list
    // unifies runtime + dev + build deps, so BFS-walking the root
    // would pull dev/build deps into `prod_set` (FR-003 violation).
    // The classifier at cargo.rs:1099+ checks `pkg.source.is_none() &&
    // pkg.name ∈ root_names` and short-circuits to Runtime WITHOUT
    // affecting the BFS closure. Virtual workspaces (no [package] block)
    // no-op via the None arm; single-crate projects and multi-crate
    // workspace roots both get their name recorded here.
    if let Some(root_name) = parsed
        .get("package")
        .and_then(|v| v.as_table())
        .and_then(|t| t.get("name"))
        .and_then(|v| v.as_str())
    {
        out.root_names.insert(root_name.to_string());
    }
    Some(out)
}

/// Milestone 179 US3 — variant of [`collect_section_keys`] that
/// records only entries with `optional = true`. Called against the
/// `[dependencies]` and `[target.<cfg>.dependencies]` tables (the
/// runtime dep tables); dev/build tables are excluded per FR-015.
fn collect_optional_dep_keys(parsed: &toml::Value, section: &str, out: &mut HashSet<String>) {
    let Some(table) = parsed.get(section).and_then(|v| v.as_table()) else {
        return;
    };
    for (key, value) in table {
        // A dep is optional iff its value is an inline table
        // containing `optional = true`. The short `foo = "1.0.0"`
        // string form and inline tables without the flag are
        // regular runtime deps.
        let is_optional = value
            .as_table()
            .and_then(|t| t.get("optional"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !is_optional {
            continue;
        }
        // Honor the `package = "..."` rename same as
        // collect_section_keys — the resolved lockfile name is what
        // downstream tags against.
        let resolved_name = value
            .as_table()
            .and_then(|t| t.get("package"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| key.clone());
        out.insert(resolved_name);
    }
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
    optional_names: &HashSet<String>,
    activated_names: &HashSet<String>,
    root_names: &HashSet<String>,
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
                // Milestone 179 US3 — Optional wins over Runtime for
                // direct optional-declared deps. The name-only match
                // scopes to direct deps (name matches a manifest
                // `[dependencies].<name>` with `optional = true`);
                // transitives-of-optional-deps stay Runtime because
                // Cargo resolves them regardless of feature flags.
                // Precedence per FR-015: manifest-declared dev/build
                // scope still wins (Cargo disallows `optional = true`
                // inside `[dev-dependencies]` / `[build-dependencies]`
                // per the reader-boundary guard in
                // `collect_optional_dep_keys`).
                // Milestone 200 (FR-002, closes #585): workspace-root
                // [[package]] entries — those with `source = None` AND
                // whose name matches a manifest `[package].name` — get
                // Runtime unconditionally. They ARE the workspace's
                // deliverable, never build-plumbing. This short-circuit
                // targets ONLY the root entry itself; the BFS closure is
                // untouched, so dev/build classification of other entries
                // remains correct per FR-003.
                let is_workspace_root =
                    pkg.source.is_none() && root_names.contains(&pkg.name);
                if is_workspace_root {
                    entry.lifecycle_scope = Some(LifecycleScope::Runtime);
                } else if prod_set.contains(&key)
                    && optional_names.contains(&pkg.name)
                    && !activated_names.contains(&pkg.name)
                {
                    // Milestone 205 (#593): dep is TRULY Optional iff
                    // declared `optional = true` in some workspace
                    // manifest AND NOT activated by the resolved
                    // feature set (per `cargo metadata --format-
                    // version 1 --offline --locked`
                    // `resolve.nodes[].deps[]`). When cargo metadata
                    // failed, the CALLER at `read` has already WARNed
                    // (FR-004) and populated `activated_names` with
                    // ALL `optional_names` — making this branch
                    // unreachable (safe over-inclusion so vuln-
                    // scanners never miss shipped deps).
                    entry.lifecycle_scope = Some(LifecycleScope::Optional);
                    entry.extra_annotations.insert(
                        "mikebom:optional-derivation".to_string(),
                        serde_json::Value::String("cargo-optional-true".to_string()),
                    );
                } else if prod_set.contains(&key) {
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
/// Output of [`read`]. `entries` is the standard `PackageDbEntry`
/// list (unchanged contract). `divergences` carries milestone-134's
/// per-collision [`DivergenceRecord`]s — empty when no divergent
/// same-PURL collisions were detected. The orchestrator in
/// `package_db/mod.rs::read_all` aggregates these into a
/// [`CollisionsSummary`] for the document-scope annotation channel.
#[derive(Debug, Default)]
pub struct CargoReadOutput {
    pub entries: Vec<PackageDbEntry>,
    pub divergences: Vec<DivergenceRecord>,
}

pub fn read(
    rootfs: &Path,
    include_dev: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Result<CargoReadOutput, CargoError> {
    // Milestone 134 — gate per-manifest deep-hash via env var so the
    // candidate-accumulation loop doesn't add cost on the default path.
    // Plumbed in from `scan_fs::scan_path` (which reads the `--deep-hash`
    // CLI flag) the same way `MIKEBOM_INCLUDE_VENDORED` is plumbed for
    // the C/C++ readers — avoids churning the 75-callsite `cargo::read`
    // signature for an opt-in observability feature.
    let deep_hash_enabled = std::env::var("MIKEBOM_DEEP_HASH")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
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
    let all_manifests = find_cargo_manifests(rootfs, exclude_set);
    let workspace_ctx = WorkspaceContext::build_from_manifests(&all_manifests);

    // Milestone 064 — Phase B: per-lockfile dependency emission
    // (existing milestone-051 flow). The workspace_sections are
    // rebuilt per-lockfile using `discover_workspace_manifests`
    // (which honors `[workspace] members = [...]` rather than walking
    // the filesystem) so dev/build classification works exactly as
    // before. Phase A's main-module emission is layered on top below.
    for lock_path in find_cargo_lockfiles(rootfs, exclude_set) {
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

        // Milestone 205 (#593): resolve Cargo's actual feature-
        // activation set via `cargo metadata --format-version 1
        // --offline`. Fallback per FR-004: on failure (cargo absent,
        // timeout, non-zero exit, missing Cargo.toml), preserve
        // pre-m205 name-only classification (activated_names stays
        // empty → classifier's `!activated_names.contains(&pkg.name)`
        // always evaluates true → optional deps classify as Optional
        // per manifest declaration). WARN log names the workspace +
        // failure class so operators see when they're in reduced-
        // fidelity mode.
        //
        // Rationale for fallback semantics: preserving pre-m205
        // classification is strictly zero-regression — nobody who
        // worked before m205 is broken by the fallback. The reporter's
        // case (#593, `test-vaultwarden`) has a warm cargo cache, so
        // `cargo metadata --offline` succeeds → fix applies. Cold-
        // cache environments (fresh CI runners, lockfile-only test
        // fixtures) get pre-m205 behavior + a WARN log naming the
        // reason. The WARN is the operator-visible signal of reduced
        // fidelity per Constitution Principle X (transparency).
        //
        // Skip-gate: if no Cargo.toml exists in workspace_root,
        // cargo metadata can't run meaningfully. Skip silently
        // (no WARN — this is a legit lockfile-only test-fixture
        // scenario, not a failure). Same fallback semantic.
        let workspace_root = lock_path.parent().unwrap_or(&lock_path);
        let cargo_metadata_timeout = resolve_cargo_metadata_timeout();
        let activated_names: HashSet<String> = if !workspace_root
            .join("Cargo.toml")
            .exists()
        {
            HashSet::new()
        } else {
            match resolve_activated_deps_via_cargo_metadata(
                workspace_root,
                cargo_metadata_timeout,
            ) {
                Ok(names) => names,
                Err(e) => {
                    tracing::warn!(
                        workspace = %workspace_root.display(),
                        reason = %e,
                        "cargo metadata failed; falling back to pre-m205 name-only \
                         optional classification (reduced fidelity — feature-activated \
                         optional deps may be misclassified as scope=excluded; install \
                         cargo binary + populate registry cache for full-fidelity)"
                    );
                    HashSet::new()
                }
            }
        };

        let entries = parse_lockfile(
            &lock_path,
            &prod_set,
            &build_set,
            &workspace_sections.optional_deps,
            &activated_names,
            &workspace_sections.root_names,
        )?;
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
    for lock_path in find_cargo_lockfiles(rootfs, exclude_set) {
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
    // Milestone 134 — accumulate per-manifest dep sets (and optional
    // deep-hash) keyed by the `PackageDbEntry.source_path` form so
    // the dedup step can compute divergence records.
    let mut candidates_by_entry_source: HashMap<String, CargoManifestCandidate> =
        HashMap::new();
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

        // Milestone 134 — accumulate this manifest's candidate before
        // dedup. `declared_deps` is the union of [dependencies] +
        // [dev-dependencies] + [build-dependencies] (FR-002).
        let declared_deps: BTreeSet<String> =
            parse_cargo_toml(manifest_path)
                .map(|s| {
                    let mut all: BTreeSet<String> = BTreeSet::new();
                    all.extend(s.prod_deps.iter().cloned());
                    all.extend(s.dev_deps.iter().cloned());
                    all.extend(s.build_deps.iter().cloned());
                    all
                })
                .unwrap_or_default();
        let display_path =
            crate::scan_fs::sbom_path::normalize_sbom_path_relative(
                &manifest_path.to_string_lossy(),
                Some(rootfs),
            );
        let deep_hash = if deep_hash_enabled {
            manifest_path
                .parent()
                .and_then(compute_cargo_crate_deep_hash)
        } else {
            None
        };
        let candidate = CargoManifestCandidate {
            entry_source_path: synthesized.source_path.clone(),
            display_path,
            purl: synthesized.purl.clone(),
            declared_deps,
            deep_hash,
        };
        candidates_by_entry_source
            .insert(candidate.entry_source_path.clone(), candidate);

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
    // Milestone 134 — divergence detection runs over the accumulated
    // candidates, NOT the dedup drop list. Phase A's augment-in-place
    // loop above already collapses same-PURL Cargo.toml files into
    // one `PackageDbEntry`, so the dedup function rarely sees
    // duplicates from cargo. The candidates_by_entry_source map
    // carries every Cargo.toml's dep set independently, which is the
    // ground truth for divergence comparison.
    let candidate_vec: Vec<CargoManifestCandidate> =
        candidates_by_entry_source.values().cloned().collect();
    let divergences = detect_divergent_collisions(&mut out, &candidate_vec);
    // FR-008 — divergent-PURL collisions emit a `tracing::warn!`
    // alongside the new annotation so existing log-watching
    // automation that filters on cargo same-PURL events keeps
    // working. Pre-milestone-134 the warn only fired for
    // dedup_drops; milestone-064's augment-in-place loop already
    // collapses same-PURL Cargo.toml files in `out`, so the
    // divergence detector is the only place divergent collisions
    // surface.
    for d in &divergences {
        tracing::warn!(
            purl = %d.purl.as_str(),
            reason = ?d.reason,
            paths = ?d.paths,
            "cargo: detected divergent same-PURL Cargo.toml collision",
        );
    }
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
            divergent_purl_collisions = divergences.len(),
            "parsed Cargo lockfiles",
        );
    }
    Ok(CargoReadOutput {
        entries: out,
        divergences,
    })
}

/// Milestone 134 US2 helper — deterministic SHA-256 of every file
/// under `crate_dir`, exclusive of build output. Walks the directory
/// via `safe_walk`, sorts by relative path, concatenates per-file
/// SHA-256s, returns the SHA-256 of the concatenation as a hex string.
///
/// Used ONLY when `--deep-hash` is set (the `MIKEBOM_DEEP_HASH=1`
/// env-var gate). Returns `None` when the dir is unreadable or empty,
/// so the calling code falls back to the dep-set-only divergence
/// path. Excludes `target/` and dotfiles to stay deterministic across
/// developer machines (the same `cargo build` on two hosts can leave
/// different debug-info bytes in `target/`).
fn compute_cargo_crate_deep_hash(crate_dir: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    let exclude_set = super::exclude_path::ExclusionSet::new_empty();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: 16,
        should_skip: &|path, _root| {
            // Skip target/ and any dotfile/dotdir — keeps the hash
            // stable across "did developer run `cargo build`" noise.
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name == "target" || name.starts_with('.') {
                    return true;
                }
            }
            false
        },
        exclude_set: &exclude_set,
    };
    crate::scan_fs::walk::safe_walk(crate_dir, &cfg, |path| {
        if path.is_file() {
            let rel = path
                .strip_prefix(crate_dir)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            files.push((rel, path.to_path_buf()));
        }
    });
    if files.is_empty() {
        return None;
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut outer = Sha256::new();
    for (rel, path) in &files {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let inner = Sha256::digest(&bytes);
        outer.update(rel.as_bytes());
        outer.update(b"\0");
        outer.update(inner);
    }
    let digest = outer.finalize();
    Some(hex_encode(&digest))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
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

/// Milestone 114: delegates to the shared `scan_fs::walk::safe_walk`
/// helper. Pre-114 this was a hand-rolled recursive walker; the
/// canonicalize-keyed visited-set + depth bound + milestone-113
/// ExclusionSet check now all live in the helper.
fn find_cargo_lockfiles(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_PROJECT_ROOT_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            candidate
                .file_name()
                .and_then(|s| s.to_str())
                .map(should_skip_descent)
                .unwrap_or(true)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_dir() {
            let lock = path.join("Cargo.lock");
            if lock.is_file() {
                out.push(lock);
            }
        }
    });
    // Pre-114 walker sorted children by name before iterating to
    // ensure cross-platform deterministic walk order (the dedup-by-
    // PURL convention relies on first-discovered-wins per FR-001).
    // Post-114 we preserve that by sorting the output Vec — the lex
    // order of full paths matches the pre-order DFS sorted-children
    // order for any plausible monorepo layout.
    out.sort();
    out
}

/// Walk for every `Cargo.toml` reachable from `rootfs` (subject to
/// `should_skip_descent` — `vendor/`, `target/`, etc. are pruned).
/// Used by milestone 064 main-module emission, which is NOT gated on
/// `Cargo.lock` presence: library crates with no committed lockfile
/// must still emit a project-self component per FR-001.
///
/// Milestone 114: delegates to `scan_fs::walk::safe_walk`. Output is
/// sorted lex by path for cross-platform determinism (matches pre-114
/// pre-order DFS sorted-children order).
fn find_cargo_manifests(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_PROJECT_ROOT_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            candidate
                .file_name()
                .and_then(|s| s.to_str())
                .map(should_skip_descent)
                .unwrap_or(true)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_dir() {
            let manifest = path.join("Cargo.toml");
            if manifest.is_file() {
                out.push(manifest);
            }
        }
    });
    out.sort();
    out
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
    fn cargo_optional_true_populates_optional_deps() {
        // Milestone 179 US3 (T023) — `optional = true` in the
        // `[dependencies]` table populates the new `optional_deps`
        // set on `CargoTomlSections`. Downstream this drives
        // `LifecycleScope::Optional` + `mikebom:optional-derivation
        // = "cargo-optional-true"` on the resolved component.
        let text = r#"
[package]
name = "my-app"
version = "0.1.0"

[dependencies]
serde = "1"
foo = { version = "1", optional = true }

[features]
foo-support = ["dep:foo"]
"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), text).unwrap();
        let sections = parse_cargo_toml(tmp.path()).expect("parse succeeds");
        assert!(sections.prod_deps.contains("foo"));
        assert!(sections.prod_deps.contains("serde"));
        // FR-008: only `foo` is optional; `serde` is a regular runtime dep.
        assert!(sections.optional_deps.contains("foo"));
        assert!(!sections.optional_deps.contains("serde"));
        assert!(sections.dev_deps.is_empty());
        assert!(sections.build_deps.is_empty());
    }

    #[test]
    fn cargo_dev_dependencies_with_optional_stays_development() {
        // Milestone 179 US3 (T024 / FR-015) — a dep declared inside
        // `[dev-dependencies]` with `optional = true` MUST stay
        // classified as Development at the reader boundary. The
        // `collect_optional_dep_keys` helper only scans
        // `[dependencies]` (and `[target.<cfg>.dependencies]`), not
        // the dev/build tables — enforcing the manifest-scope
        // precedence in FR-015 before it reaches the classifier at
        // parse_lockfile.
        let text = r#"
[package]
name = "my-app"
version = "0.1.0"

[dev-dependencies]
baz = { version = "1", optional = true }
"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), text).unwrap();
        let sections = parse_cargo_toml(tmp.path()).expect("parse succeeds");
        // baz is dev-scope; NOT tagged as optional (FR-015 precedence).
        assert!(sections.dev_deps.contains("baz"));
        assert!(!sections.optional_deps.contains("baz"));
        assert!(sections.prod_deps.is_empty());
        // Milestone 200: [package].name = "my-app" recorded in root_names
        // (separate set — dev/build classification of baz is unaffected).
        assert!(sections.root_names.contains("my-app"));
    }

    #[test]
    fn cargo_optional_with_package_rename_uses_resolved_name() {
        // Milestone 179 US3 — Cargo allows `foo = { package = "real-foo",
        // version = "1", optional = true }`. The lockfile records the
        // resolved name (`real-foo`), not the manifest key. The
        // `optional_deps` set MUST use the resolved name so the
        // downstream `optional_names.contains(&pkg.name)` check in
        // parse_lockfile matches the lockfile entry.
        let text = r#"
[package]
name = "my-app"
version = "0.1.0"

[dependencies]
foo = { package = "real-foo", version = "1", optional = true }
"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), text).unwrap();
        let sections = parse_cargo_toml(tmp.path()).expect("parse succeeds");
        assert!(sections.optional_deps.contains("real-foo"));
        assert!(!sections.optional_deps.contains("foo"));
    }

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
        let entries = parse_lockfile(&path, &HashSet::new(), &HashSet::new(), &HashSet::new(), &HashSet::new(), &HashSet::new()).unwrap();
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
        let entries = parse_lockfile(&path, &HashSet::new(), &HashSet::new(), &HashSet::new(), &HashSet::new(), &HashSet::new()).unwrap();
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
        let entries = parse_lockfile(&path, &HashSet::new(), &HashSet::new(), &HashSet::new(), &HashSet::new(), &HashSet::new()).unwrap();
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
        match parse_lockfile(&path, &HashSet::new(), &HashSet::new(), &HashSet::new(), &HashSet::new(), &HashSet::new()) {
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
        match parse_lockfile(&path, &HashSet::new(), &HashSet::new(), &HashSet::new(), &HashSet::new(), &HashSet::new()) {
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
        let entries = read(dir.path(), false, &Default::default()).unwrap().entries;
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
        let entries = read(dir.path(), false, &Default::default()).unwrap().entries;
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
        let entries = read(dir.path(), false, &Default::default()).unwrap().entries;
        assert_eq!(entries.len(), 4, "all four entries should be present");
        assert!(entries.iter().any(|e| e.name == "my-app"));
        assert!(entries.iter().any(|e| e.name == "my-lib"));
        assert!(entries.iter().any(|e| e.name == "my-cli"));
        assert!(entries.iter().any(|e| e.name == "anyhow"));
    }

    #[test]
    fn read_empty_rootfs_returns_zero() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read(dir.path(), false, &Default::default())
            .unwrap()
            .entries
            .is_empty());
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
            read(dir.path(), false, &Default::default()),
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
        // Milestone 200 (FR-001): [package].name recorded in root_names
        // (NOT prod_deps — separate set avoids over-reaching the BFS
        // closure per data-model E2 regression risk).
        assert!(sections.root_names.contains("demo"));
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
        // Milestone 200: [package].name recorded in root_names.
        assert!(sections.root_names.contains("demo"));
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
        // Milestone 200: [package].name = "x" recorded in root_names.
        assert!(sections.root_names.contains("x"));
    }

    // ---- Milestone 200 (issue #585) — workspace-root [package] seed ----

    /// FR-001: `parse_cargo_toml` records the root `[package].name` into
    /// `CargoTomlSections.root_names` (separate from `prod_deps` to avoid
    /// over-reaching the BFS closure). The classifier at `parse_lockfile`
    /// short-circuits workspace-root [[package]] entries to Runtime by
    /// checking `pkg.source.is_none() && pkg.name ∈ root_names`.
    #[test]
    fn parse_cargo_toml_seeds_root_package_name_into_root_names_m200() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cargo.toml");
        std::fs::write(
            &path,
            r#"
[package]
name = "myapp"
version = "0.1.0"

[dependencies]
foo = "1.0"
"#,
        )
        .unwrap();
        let sections = parse_cargo_toml(&path).expect("parsed");
        // Existing behavior preserved: [dependencies] key in prod_deps.
        assert!(sections.prod_deps.contains("foo"));
        // Milestone 200 new behavior: [package].name in root_names only.
        assert!(sections.root_names.contains("myapp"));
        assert!(
            !sections.prod_deps.contains("myapp"),
            "root name MUST NOT leak into prod_deps — that would over-reach BFS closure (FR-003)"
        );
    }

    /// FR-004: virtual workspace (no `[package]` block, only `[workspace]`)
    /// MUST NOT synthetic-seed any name into root_names or prod_deps.
    #[test]
    fn parse_cargo_toml_virtual_workspace_omits_root_package_seed_m200() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cargo.toml");
        std::fs::write(&path, "[workspace]\nmembers = [\"a\", \"b\"]\n").unwrap();
        let sections = parse_cargo_toml(&path).expect("parsed");
        assert!(
            sections.prod_deps.is_empty(),
            "virtual workspace must NOT synthetic-seed prod_deps; got {:?}",
            sections.prod_deps
        );
        assert!(
            sections.root_names.is_empty(),
            "virtual workspace must NOT synthetic-seed root_names; got {:?}",
            sections.root_names
        );
    }

    /// FR-005: two independent workspace-root Cargo.toml files parsed via
    /// separate `parse_cargo_toml` invocations MUST NOT cross-seed each
    /// other's `[package].name`. Verifies per-manifest boundary isolation.
    #[test]
    fn parse_cargo_toml_isolates_root_package_across_independent_workspaces_m200() {
        let dir = tempfile::tempdir().unwrap();
        let path_a = dir.path().join("app-a.toml");
        let path_b = dir.path().join("app-b.toml");
        std::fs::write(
            &path_a,
            "[package]\nname = \"app-a\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            &path_b,
            "[package]\nname = \"app-b\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let sections_a = parse_cargo_toml(&path_a).expect("parsed a");
        let sections_b = parse_cargo_toml(&path_b).expect("parsed b");
        assert!(sections_a.root_names.contains("app-a"));
        assert!(!sections_a.root_names.contains("app-b"));
        assert!(sections_b.root_names.contains("app-b"));
        assert!(!sections_b.root_names.contains("app-a"));
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
        let result =
            find_cargo_lockfiles(tmp.path(), &super::super::exclude_path::ExclusionSet::new_empty());
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

    // ---- Milestone 201 (issue #587) — `mikebom:is-cargo-workspace-toplevel` ----

    /// FR-001: cargo m064 stamps the workspace-toplevel positive-
    /// identifier annotation when the manifest has BOTH [package] AND
    /// [workspace] blocks. This is the signal the m201 fix propagates
    /// through to scan_fs/mod.rs's is_workspace_root stamping.
    #[test]
    fn build_cargo_main_module_entry_stamps_workspace_toplevel_annotation_m201() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = write_manifest(
            tmp.path(),
            r#"
[package]
name = "app"
version = "0.1.0"

[workspace]
members = ["helper"]
"#,
        );
        let ctx = WorkspaceContext::default();
        let entry = build_cargo_main_module_entry(&manifest, &ctx).unwrap();
        let annot = entry
            .extra_annotations
            .get("mikebom:is-cargo-workspace-toplevel")
            .and_then(|v| v.as_bool());
        assert_eq!(annot, Some(true));
    }

    /// FR-001 corollary: workspace MEMBER Cargo.tomls (only `[package]`,
    /// no `[workspace]`) MUST NOT get the annotation. This is what
    /// distinguishes root from member post-m201.
    #[test]
    fn build_cargo_main_module_entry_omits_workspace_toplevel_for_member_crate_m201() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = write_manifest(
            tmp.path(),
            r#"
[package]
name = "helper"
version = "0.1.0"
"#,
        );
        let ctx = WorkspaceContext::default();
        let entry = build_cargo_main_module_entry(&manifest, &ctx).unwrap();
        assert!(
            !entry
                .extra_annotations
                .contains_key("mikebom:is-cargo-workspace-toplevel"),
            "workspace-member Cargo.toml MUST NOT get the toplevel annotation"
        );
    }

    /// FR-004 preservation: standalone single-crate cargo projects (no
    /// `[workspace]` block) MUST NOT get the annotation. Their root
    /// election falls through to a different m127 ladder branch that
    /// still picks the single crate as metadata.component.
    #[test]
    fn build_cargo_main_module_entry_omits_workspace_toplevel_for_single_crate_m201() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = write_manifest(
            tmp.path(),
            r#"
[package]
name = "single"
version = "0.1.0"
"#,
        );
        let ctx = WorkspaceContext::default();
        let entry = build_cargo_main_module_entry(&manifest, &ctx).unwrap();
        assert!(
            !entry
                .extra_annotations
                .contains_key("mikebom:is-cargo-workspace-toplevel"),
            "single-crate cargo project MUST NOT get the toplevel annotation \
             (would over-stamp if [workspace] absence isn't checked)"
        );
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
            build_inclusion: None,
            purl,
            name: name.to_string(),
            version: version.to_string(),
            arch: None,
            source_path: source_path.to_string(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: None,
            requirement_ranges: Vec::new(),
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
            binary_role: None,
        }
    }

    fn make_regular_entry(name: &str, version: &str) -> PackageDbEntry {
        let purl = build_cargo_purl(name, version).unwrap();
        PackageDbEntry {
            build_inclusion: None,
            purl,
            name: name.to_string(),
            version: version.to_string(),
            arch: None,
            source_path: String::new(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: None,
            requirement_ranges: Vec::new(),
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
            binary_role: None,
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
        let mut entries = vec![
            make_regular_entry("foo", "1.2.3"),
            make_regular_entry("foo", "1.2.3"),
        ];
        let drops = dedup_main_modules_by_purl(&mut entries);
        assert_eq!(entries.len(), 2);
        assert!(drops.is_empty());
    }

    // -------- Milestone 134 — divergence detection unit tests --------

    fn make_candidate(
        display_path: &str,
        purl_str: &str,
        deps: &[&str],
        deep_hash: Option<&str>,
    ) -> CargoManifestCandidate {
        let mut dep_set: BTreeSet<String> = BTreeSet::new();
        for d in deps {
            dep_set.insert(d.to_string());
        }
        CargoManifestCandidate {
            entry_source_path: format!("path+file:///fake/{display_path}"),
            display_path: display_path.to_string(),
            purl: Purl::new(purl_str).unwrap(),
            declared_deps: dep_set,
            deep_hash: deep_hash.map(|s| s.to_string()),
        }
    }

    #[test]
    fn divergence_deps_differ_emits_record_and_stamps_annotation() {
        let mut entries = vec![make_main_module_entry(
            "foo",
            "1.2.3",
            "path+file:///tmp/crates/foo",
        )];
        let candidates = vec![
            make_candidate(
                "crates/foo/Cargo.toml",
                "pkg:cargo/foo@1.2.3",
                &["serde", "tokio"],
                None,
            ),
            make_candidate(
                "vendor/foo/Cargo.toml",
                "pkg:cargo/foo@1.2.3",
                &["anyhow", "serde", "tokio"],
                None,
            ),
        ];
        let divergences = detect_divergent_collisions(&mut entries, &candidates);
        assert_eq!(divergences.len(), 1);
        let rec = &divergences[0];
        assert_eq!(rec.purl.as_str(), "pkg:cargo/foo@1.2.3");
        assert_eq!(rec.reason, DivergenceReason::DepsDiffer);
        assert_eq!(
            rec.paths,
            vec![
                "crates/foo/Cargo.toml".to_string(),
                "vendor/foo/Cargo.toml".to_string(),
            ]
        );
        assert!(entries[0]
            .extra_annotations
            .contains_key("mikebom:duplicate-purl-divergent"));
    }

    #[test]
    fn divergence_identical_deps_emits_no_record() {
        let mut entries = vec![make_main_module_entry(
            "foo",
            "1.2.3",
            "path+file:///tmp/crates/foo",
        )];
        let candidates = vec![
            make_candidate(
                "crates/foo/Cargo.toml",
                "pkg:cargo/foo@1.2.3",
                &["serde", "tokio"],
                None,
            ),
            make_candidate(
                "vendor/foo/Cargo.toml",
                "pkg:cargo/foo@1.2.3",
                &["serde", "tokio"],
                None,
            ),
        ];
        let divergences = detect_divergent_collisions(&mut entries, &candidates);
        assert!(divergences.is_empty());
        assert!(!entries[0]
            .extra_annotations
            .contains_key("mikebom:duplicate-purl-divergent"));
    }

    #[test]
    fn divergence_hashes_differ_emits_record() {
        let mut entries = vec![make_main_module_entry(
            "foo",
            "1.2.3",
            "path+file:///tmp/a/foo",
        )];
        let candidates = vec![
            make_candidate(
                "a/foo/Cargo.toml",
                "pkg:cargo/foo@1.2.3",
                &["serde"],
                Some(&"aa".repeat(32)),
            ),
            make_candidate(
                "b/foo/Cargo.toml",
                "pkg:cargo/foo@1.2.3",
                &["serde"],
                Some(&"bb".repeat(32)),
            ),
        ];
        let divergences = detect_divergent_collisions(&mut entries, &candidates);
        assert_eq!(divergences.len(), 1);
        assert_eq!(divergences[0].reason, DivergenceReason::HashesDiffer);
        assert!(divergences[0].hashes_by_path.is_some());
    }

    #[test]
    fn divergence_both_when_deps_and_hashes_diverge() {
        let mut entries = vec![make_main_module_entry(
            "foo",
            "1.2.3",
            "path+file:///tmp/a/foo",
        )];
        let candidates = vec![
            make_candidate(
                "a/foo/Cargo.toml",
                "pkg:cargo/foo@1.2.3",
                &["serde"],
                Some(&"aa".repeat(32)),
            ),
            make_candidate(
                "b/foo/Cargo.toml",
                "pkg:cargo/foo@1.2.3",
                &["serde", "tokio"],
                Some(&"bb".repeat(32)),
            ),
        ];
        let divergences = detect_divergent_collisions(&mut entries, &candidates);
        assert_eq!(divergences.len(), 1);
        assert_eq!(divergences[0].reason, DivergenceReason::Both);
        assert!(divergences[0].dep_sets_by_path.is_some());
        assert!(divergences[0].hashes_by_path.is_some());
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

    // ── Milestone 191 (#558) — build_cargo_purl versionless shape ──

    #[test]
    fn build_cargo_purl_empty_version_emits_versionless_shape() {
        let p = build_cargo_purl("serde", "").expect("empty-version permitted");
        assert_eq!(p.as_str(), "pkg:cargo/serde");
    }

    #[test]
    fn build_cargo_purl_nonempty_version_byte_identical_to_pre_m191() {
        let p = build_cargo_purl("serde", "1.0.203").expect("non-empty");
        assert_eq!(p.as_str(), "pkg:cargo/serde@1.0.203");
    }

    // ── T007 m205 (#593) — cargo metadata resolver + failure enum ──
    //
    // Env-var-mutating tests require serial execution. Invoke via
    // `cargo test ... -- --test-threads=1` when running the full
    // suite; individual `-- resolve_cargo_metadata_timeout` invocations
    // naturally serialize matching tests. Matches m203 pattern.

    fn with_cargo_metadata_timeout_env<F: FnOnce()>(value: Option<&str>, f: F) {
        let prev = std::env::var("MIKEBOM_CARGO_METADATA_TIMEOUT_SECS").ok();
        match value {
            Some(v) => std::env::set_var("MIKEBOM_CARGO_METADATA_TIMEOUT_SECS", v),
            None => std::env::remove_var("MIKEBOM_CARGO_METADATA_TIMEOUT_SECS"),
        }
        f();
        match prev {
            Some(v) => std::env::set_var("MIKEBOM_CARGO_METADATA_TIMEOUT_SECS", v),
            None => std::env::remove_var("MIKEBOM_CARGO_METADATA_TIMEOUT_SECS"),
        }
    }

    #[test]
    fn resolve_cargo_metadata_timeout_default_when_env_var_absent_m205() {
        with_cargo_metadata_timeout_env(None, || {
            assert_eq!(resolve_cargo_metadata_timeout(), Duration::from_secs(60));
        });
    }

    #[test]
    fn resolve_cargo_metadata_timeout_honors_env_var_m205() {
        with_cargo_metadata_timeout_env(Some("42"), || {
            assert_eq!(resolve_cargo_metadata_timeout(), Duration::from_secs(42));
        });
    }

    #[test]
    fn resolve_cargo_metadata_timeout_clamps_below_min_m205() {
        with_cargo_metadata_timeout_env(Some("0"), || {
            assert_eq!(resolve_cargo_metadata_timeout(), Duration::from_secs(1));
        });
    }

    #[test]
    fn resolve_cargo_metadata_timeout_clamps_above_max_m205() {
        with_cargo_metadata_timeout_env(Some("99999"), || {
            assert_eq!(resolve_cargo_metadata_timeout(), Duration::from_secs(3600));
        });
    }

    #[test]
    fn resolve_cargo_metadata_timeout_ignores_parse_error_m205() {
        with_cargo_metadata_timeout_env(Some("notanumber"), || {
            assert_eq!(resolve_cargo_metadata_timeout(), Duration::from_secs(60));
        });
    }

    #[test]
    fn cargo_metadata_resolve_failure_display_formats_all_variants_m205() {
        let e = CargoMetadataResolveFailure::BinaryNotFound;
        assert_eq!(format!("{e}"), "`cargo` binary not found on $PATH");

        let e = CargoMetadataResolveFailure::NonZeroExit {
            code: 101,
            stderr_head: "the lock file needs to be updated".into(),
        };
        assert_eq!(
            format!("{e}"),
            "`cargo metadata` exited with code 101; stderr head: the lock file needs to be updated"
        );

        let e = CargoMetadataResolveFailure::Timeout { timeout_secs: 60 };
        assert_eq!(format!("{e}"), "`cargo metadata` exceeded 60s timeout");

        let src = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let e = CargoMetadataResolveFailure::ParseError { source: src };
        assert!(
            format!("{e}").starts_with("`cargo metadata` JSON parse failed: "),
            "actual: {e}",
        );

        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
        let e = CargoMetadataResolveFailure::from(io_err);
        assert_eq!(format!("{e}"), "`cargo metadata` I/O error: nope");
    }

    #[test]
    fn cargo_metadata_cap_stderr_lines_truncates_at_max_m205() {
        let bytes = b"line1\nline2\nline3\nline4\nline5";
        assert_eq!(cargo_metadata_cap_stderr_lines(bytes, 3), "line1\nline2\nline3");
        assert_eq!(cargo_metadata_cap_stderr_lines(b"only", 20), "only");
    }
}