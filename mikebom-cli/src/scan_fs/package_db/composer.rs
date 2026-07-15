//! Milestone 138 — PHP/Composer ecosystem reader.
//!
//! Discovers Composer 2.x projects under the scan root via three input
//! artifacts:
//!
//! - `composer.json` (manifest) — main-module emission per FR-012;
//!   design-tier fallback for `require:` + `require-dev:` deps per FR-005
//!   when no sibling `composer.lock` exists.
//! - `composer.lock` (lockfile) — source-tier emission per FR-002 +
//!   FR-003 for every entry in `packages[]` and `packages-dev[]`.
//! - `vendor/composer/installed.json` (deployed) — deployed-tier
//!   emission per FR-006; the walker discovers EVERY installed.json
//!   under the scan root regardless of sibling-manifest pairing (Q2
//!   clarification — supports multi-layer container scans).
//!
//! PURL shapes per FR-003 (confirmed against the purl-spec
//! [composer-definition.md](https://github.com/package-url/purl-spec/blob/main/types-doc/composer-definition.md)):
//!
//! - **packagist** (default): `pkg:composer/<lc-vendor>/<lc-package>@<version>`.
//!   `?repository_url=<url>` qualifier appended for self-hosted mirrors
//!   (URL is NOT one of `https://packagist.org` / `https://repo.packagist.org`
//!   / `https://api.github.com`).
//! - **vcs** (git/svn/hg): same PURL form + `?vcs_url=<scheme>+<url>`
//!   qualifier per purl-spec cross-type convention. The resolved SHA is
//!   surfaced via `mikebom:vcs-ref` annotation rather than embedded in
//!   the version segment (Composer records the upstream tag in `version:`
//!   for VCS sources too).
//! - **path**: `pkg:generic/<lc-vendor>-<lc-package>@<version>`
//!   placeholder (vendor+name flattened with `-`) + `mikebom:source-type
//!   = "composer-path"` annotation as discriminator.
//! - **composer-plugin / metapackage** (`type:` field at lockfile entry
//!   level): standard Packagist PURL + `mikebom:source-type =
//!   "composer-plugin"` / `"composer-metapackage"` annotation.
//!
//! Vendor + package segments MUST be lowercased per purl-spec canonical
//! form (research §R4). The `name` field on the emitted PackageDbEntry
//! preserves the lockfile's literal case for display; only the PURL
//! identity is lowercased.
//!
//! `mikebom:source-type` values use the `composer-` prefix to avoid
//! collision with cargo's bare C1-row values (`git`/`path`/`registry`)
//! and Dart's `pub-` prefixed values — per the milestone-122 `kmp-` +
//! milestone-137 `pub-` precedent.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use mikebom_common::resolution::LifecycleScope;
use mikebom_common::types::hash::{ContentHash, HashAlgorithm};
use mikebom_common::types::purl::Purl;

use super::exclude_path::ExclusionSet;
use super::PackageDbEntry;

const MAX_COMPOSER_WALK_DEPTH: usize = 12;

/// Skip-set for the manifest walker (find `composer.json` files). We
/// MUST skip `vendor/` here — `vendor/<vendor>/<pkg>/composer.json`
/// files are per-package manifests that would re-emit every installed
/// dep as its own main-module.
fn should_skip_manifest_descent(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".svn" | ".hg" | "vendor" | "node_modules"
    )
}

/// Skip-set for the installed.json walker — DOES NOT skip `vendor/`
/// (that's the target directory). Still skips VCS metadata and
/// node_modules.
fn should_skip_installed_json_descent(name: &str) -> bool {
    matches!(name, ".git" | ".svn" | ".hg" | "node_modules")
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ComposerJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default, rename = "type")]
    type_: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    require: BTreeMap<String, String>,
    #[serde(default, rename = "require-dev")]
    require_dev: BTreeMap<String, String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ComposerLock {
    #[serde(default)]
    packages: Vec<LockfilePackage>,
    #[serde(default, rename = "packages-dev")]
    packages_dev: Option<Vec<LockfilePackage>>,
    #[serde(default, rename = "plugin-api-version")]
    plugin_api_version: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct LockfilePackage {
    name: String,
    version: String,
    #[serde(default = "default_package_type", rename = "type")]
    type_: String,
    #[serde(default)]
    source: Option<LockfileSource>,
    #[serde(default)]
    dist: Option<LockfileDist>,
    #[serde(default)]
    require: BTreeMap<String, String>,
    #[serde(default)]
    license: Option<LockfileLicense>,
}

fn default_package_type() -> String {
    "library".to_string()
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[allow(dead_code)]
struct LockfileSource {
    #[serde(default, rename = "type")]
    type_: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    reference: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[allow(dead_code)]
struct LockfileDist {
    #[serde(default, rename = "type")]
    type_: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    shasum: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
enum LockfileLicense {
    Single(String),
    List(Vec<String>),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct InstalledJson {
    packages: Vec<LockfilePackage>,
    #[serde(default)]
    dev: bool,
    #[serde(default, rename = "dev-package-names")]
    dev_package_names: Vec<String>,
}

pub fn read(
    rootfs: &Path,
    _include_dev: bool,
    exclude_set: &ExclusionSet,
) -> Vec<PackageDbEntry> {
    let mut out: Vec<PackageDbEntry> = Vec::new();
    let mut seen_purls: HashSet<String> = HashSet::new();
    // Per-project lockfile PURL set, keyed by the project root path
    // (parent of `composer.lock`). Used by the installed.json pass for
    // orphan-detection (C1 + I3 remediation: Option<&HashSet<String>>
    // — None means "no sibling lockfile exists", which suppresses the
    // orphan annotation).
    let mut project_lockfile_purls: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    let mut manifest_projects = 0usize;
    let mut lockfile_projects = 0usize;
    let mut installed_json_files = 0usize;
    let mut warnings_emitted = 0usize;

    // Pass A: walk for composer.json manifests.
    for composer_json_path in find_composer_manifests(rootfs, exclude_set) {
        let manifest = match parse_composer_json(&composer_json_path) {
            Ok(m) => m,
            Err(err) => {
                warnings_emitted += 1;
                tracing::warn!(
                    path = %composer_json_path.display(),
                    error = %err,
                    "composer: failed to parse composer.json; skipping project",
                );
                continue;
            }
        };
        manifest_projects += 1;
        let Some(project_dir) = composer_json_path.parent() else {
            continue;
        };
        let lockfile_path = project_dir.join("composer.lock");

        let parsed_lockfile: Option<ComposerLock> = if lockfile_path.is_file() {
            match parse_composer_lock(&lockfile_path) {
                Ok(lock) => Some(lock),
                Err(err) => {
                    warnings_emitted += 1;
                    tracing::warn!(
                        path = %lockfile_path.display(),
                        error = %err,
                        "composer: failed to parse composer.lock, falling back to design-tier from composer.json",
                    );
                    None
                }
            }
        } else {
            None
        };

        // Main-module emission first so it has a stable bom-ref.
        if let Some(main_module) =
            emit_main_module(&composer_json_path, &manifest, parsed_lockfile.as_ref())
        {
            let purl_key = main_module.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(main_module);
            }
        }

        match parsed_lockfile {
            Some(lock) => {
                lockfile_projects += 1;
                // Record this project's lockfile PURLs for the
                // installed.json orphan-detection pass.
                let mut purl_set: HashSet<String> = HashSet::new();
                let entries = emit_lockfile_packages(&lockfile_path, &lock);
                for entry in entries {
                    let purl_key = entry.purl.as_str().to_string();
                    purl_set.insert(purl_key.clone());
                    if seen_purls.insert(purl_key) {
                        out.push(entry);
                    }
                }
                project_lockfile_purls.insert(project_dir.to_path_buf(), purl_set);
            }
            None => {
                let entries = emit_design_tier_components(&composer_json_path, &manifest);
                for entry in entries {
                    let purl_key = entry.purl.as_str().to_string();
                    if seen_purls.insert(purl_key) {
                        out.push(entry);
                    }
                }
            }
        }
    }

    // Pass B: walk for vendor/composer/installed.json files. Multi-
    // layer container support per Q2 — DISCOVER ALL such files
    // regardless of sibling-manifest pairing.
    for installed_json_path in find_installed_jsons(rootfs, exclude_set) {
        let parsed = match parse_installed_json(&installed_json_path) {
            Ok(p) => p,
            Err(err) => {
                warnings_emitted += 1;
                tracing::warn!(
                    path = %installed_json_path.display(),
                    error = %err,
                    "composer: failed to parse installed.json; skipping file",
                );
                continue;
            }
        };
        installed_json_files += 1;
        // Compute the sibling project root = parent of `vendor/` =
        // grandparent of installed.json.
        let project_root: Option<PathBuf> = installed_json_path
            .parent() // .../vendor/composer
            .and_then(|p| p.parent()) // .../vendor
            .and_then(|p| p.parent()) // project root
            .map(|p| p.to_path_buf());
        let sibling_lockfile_purls: Option<&HashSet<String>> = project_root
            .as_ref()
            .and_then(|root| project_lockfile_purls.get(root));

        let entries =
            emit_installed_json_components(&installed_json_path, &parsed, sibling_lockfile_purls);
        for entry in entries {
            let purl_key = entry.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(entry);
            }
        }
    }

    if !out.is_empty() || warnings_emitted > 0 {
        tracing::info!(
            rootfs = %rootfs.display(),
            entries = out.len(),
            manifest_projects,
            lockfile_projects,
            installed_json_files,
            warnings_emitted,
            "parsed composer.json + composer.lock + installed.json entries",
        );
    }
    out
}

fn find_composer_manifests(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_COMPOSER_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_manifest_descent(name)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file()
            && path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|n| n.eq_ignore_ascii_case("composer.json"))
                .unwrap_or(false)
        {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

fn find_installed_jsons(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_COMPOSER_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_installed_json_descent(name)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if !path.is_file() {
            return;
        }
        if path.file_name().and_then(|s| s.to_str()) != Some("installed.json") {
            return;
        }
        // Match the canonical `vendor/composer/installed.json` layout —
        // immediate parent is `composer/`, grandparent is `vendor/`.
        let parent = path.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str());
        let grandparent = path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str());
        if parent == Some("composer") && grandparent == Some("vendor") {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

fn parse_composer_json(path: &Path) -> anyhow::Result<ComposerJson> {
    let bytes = std::fs::read(path).map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| anyhow::anyhow!("json parse failed: {e}"))
}

fn parse_composer_lock(path: &Path) -> anyhow::Result<ComposerLock> {
    let bytes = std::fs::read(path).map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| anyhow::anyhow!("json parse failed: {e}"))
}

fn parse_installed_json(path: &Path) -> anyhow::Result<InstalledJson> {
    let bytes = std::fs::read(path).map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    // Composer 1 format detection per R3: root is a bare JSON array.
    // Reject with a specific error so callers warn-and-skip; the modern
    // wrapper-shape parse would fail with a generic deserialization
    // error otherwise.
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("json parse failed: {e}"))?;
    if value.is_array() {
        return Err(anyhow::anyhow!(
            "Composer 1 installed.json format (bare array) not supported"
        ));
    }
    serde_json::from_value(value).map_err(|e| anyhow::anyhow!("json parse failed: {e}"))
}

/// FR-012: emit one main-module per `composer.json` discovered.
/// Returns `None` when the manifest lacks a valid `name:` field
/// (per Q3 — lockfile deps still emit; only the project-root
/// component is skipped).
fn emit_main_module(
    composer_json_path: &Path,
    manifest: &ComposerJson,
    parsed_lockfile: Option<&ComposerLock>,
) -> Option<PackageDbEntry> {
    let name = manifest.name.as_deref()?;
    if name.is_empty() || !name.contains('/') {
        tracing::warn!(
            path = %composer_json_path.display(),
            "composer: composer.json `name:` field missing or malformed; skipping main-module",
        );
        return None;
    }
    // Milestone 197 US3 (#567): when the manifest carries no `version:`
    // (common for `require`-only application manifests), emit a
    // versionless canonical PURL (`pkg:composer/<name>`) per purl-spec
    // instead of the pre-m197 placeholder `pkg:composer/<name>@0.0.0-unknown`
    // — matches the m191 fix pattern applied to npm/cargo/maven/gem/pip.
    // The `PackageDbEntry.version` field still stores the placeholder
    // for `component.version` display in emitted SBOMs.
    let raw_version = manifest.version.clone();
    let version = raw_version
        .clone()
        .unwrap_or_else(|| "0.0.0-unknown".to_string());
    let lc_name = name.to_lowercase();
    let purl_str = if raw_version.as_deref().unwrap_or("").is_empty() {
        format!("pkg:composer/{lc_name}")
    } else {
        format!("pkg:composer/{lc_name}@{version}")
    };
    let purl = Purl::new(&purl_str).ok()?;

    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert(
        "mikebom:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );
    extra_annotations.insert(
        "mikebom:source-type".to_string(),
        serde_json::Value::String("composer-main-module".to_string()),
    );

    // Dep edges per FR-004: main-module → direct deps.
    // Lockfile mode: derive from manifest's require/require-dev (these
    // ARE the lockfile's direct-dep set; the lockfile itself doesn't
    // distinguish direct-vs-transitive). Design-tier mode: same source.
    // Post-resolution filter handles --exclude-scope dev.
    let _ = parsed_lockfile; // reserved for v1.1 transitive-edge work
    let mut depends: Vec<String> = manifest.require.keys().cloned().collect();
    depends.extend(manifest.require_dev.keys().cloned());

    Some(PackageDbEntry {
        purl,
        name: name.to_string(),
        version,
        arch: None,
        source_path: composer_json_path.to_string_lossy().into_owned(),
        depends,
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_range: None,
        source_type: Some("composer-main-module".to_string()),
        buildinfo_status: None,
        sbom_tier: Some("source".to_string()),
        evidence_kind: Some("composer-json".to_string()),
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
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
        build_inclusion: None,
    })
}

/// FR-002 + FR-003 + FR-009: one component per lockfile entry.
fn emit_lockfile_packages(
    lockfile_path: &Path,
    parsed_lockfile: &ComposerLock,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    let source_path = lockfile_path.to_string_lossy().into_owned();

    // packages[] = runtime; packages-dev[] = development.
    for pkg in &parsed_lockfile.packages {
        if let Some(entry) = build_entry_from_lockfile_package(
            pkg,
            &source_path,
            LifecycleScope::Runtime,
            "composer-lock",
            "source",
            None, // not orphan; this is the lockfile itself
        ) {
            out.push(entry);
        }
    }
    if let Some(dev) = &parsed_lockfile.packages_dev {
        for pkg in dev {
            if let Some(entry) = build_entry_from_lockfile_package(
                pkg,
                &source_path,
                LifecycleScope::Development,
                "composer-lock",
                "source",
                None,
            ) {
                out.push(entry);
            }
        }
    }
    out
}

/// FR-006 + Q1 clarification: deployed-tier emission from
/// `vendor/composer/installed.json` with orphan-detection.
/// `sibling_lockfile_purls`:
/// - `Some(set)` → a sibling `composer.lock` exists; entries with
///   PURL NOT in `set` are tagged `mikebom:lockfile-orphan = "true"`.
/// - `None` → no sibling lockfile; orphan annotation is suppressed
///   (the lockfile-vs-disk comparison is undefined).
fn emit_installed_json_components(
    installed_json_path: &Path,
    parsed: &InstalledJson,
    sibling_lockfile_purls: Option<&HashSet<String>>,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    let source_path = installed_json_path.to_string_lossy().into_owned();
    let dev_names: HashSet<&str> = parsed.dev_package_names.iter().map(|s| s.as_str()).collect();

    for pkg in &parsed.packages {
        let lifecycle = if dev_names.contains(pkg.name.as_str()) {
            LifecycleScope::Development
        } else {
            LifecycleScope::Runtime
        };
        // Build the PURL once via the shared helper so we can check it
        // against `sibling_lockfile_purls` BEFORE constructing the
        // PackageDbEntry — the orphan annotation needs to be in
        // `extra_annotations` at construction time.
        let Some(entry) = build_entry_from_lockfile_package(
            pkg,
            &source_path,
            lifecycle,
            "composer-installed-json",
            "deployed",
            sibling_lockfile_purls,
        ) else {
            continue;
        };
        out.push(entry);
    }
    out
}

/// Shared per-entry builder. The `orphan_check_against` argument:
/// - `None` → don't add the orphan annotation.
/// - `Some(set)` → add `mikebom:lockfile-orphan = "true"` ONLY when
///   the constructed PURL is NOT in `set` (lockfile-vs-disk drift).
fn build_entry_from_lockfile_package(
    pkg: &LockfilePackage,
    source_path: &str,
    lifecycle: LifecycleScope,
    evidence_kind: &str,
    sbom_tier: &str,
    orphan_check_against: Option<&HashSet<String>>,
) -> Option<PackageDbEntry> {
    let purl = match build_purl_for_package(pkg) {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!(
                name = %pkg.name,
                path = %source_path,
                error = %err,
                "composer: skipping malformed lockfile entry",
            );
            return None;
        }
    };
    let source_type_value = classify_source_type(pkg);
    let mut extra_annotations = build_extra_annotations(pkg, source_type_value);

    // Orphan-detection per Q1 + C1 remediation. Only fires when a
    // sibling lockfile EXISTS (Some) AND doesn't contain this PURL.
    if let Some(set) = orphan_check_against {
        if !set.contains(purl.as_str()) {
            extra_annotations.insert(
                "mikebom:lockfile-orphan".to_string(),
                serde_json::Value::String("true".to_string()),
            );
        }
    }

    // SHA-1 hash per FR-013 — only for Packagist entries (metapackages
    // have no `dist`).
    let hashes: Vec<ContentHash> = match (&pkg.dist, source_type_value) {
        (Some(d), value) if value != "composer-metapackage" => match &d.shasum {
            Some(hex) if hex.len() == 40 && hex.chars().all(|c| c.is_ascii_hexdigit()) => {
                match ContentHash::with_algorithm(HashAlgorithm::Sha1, hex) {
                    Ok(h) => vec![h],
                    Err(_) => Vec::new(),
                }
            }
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };

    Some(PackageDbEntry {
        purl,
        name: pkg.name.clone(),
        version: pkg.version.clone(),
        arch: None,
        source_path: source_path.to_string(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: Some(lifecycle),
        requirement_range: None,
        source_type: Some(source_type_value.to_string()),
        buildinfo_status: None,
        sbom_tier: Some(sbom_tier.to_string()),
        evidence_kind: Some(evidence_kind.to_string()),
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
        hashes,
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
        build_inclusion: None,
    })
}

/// FR-005: design-tier emission when no lockfile is present.
fn emit_design_tier_components(
    composer_json_path: &Path,
    manifest: &ComposerJson,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    let source_path = composer_json_path.to_string_lossy().into_owned();
    let manifest_name = manifest.name.as_deref().unwrap_or("");

    let runtime_iter = manifest.require.iter().map(|(k, v)| (k, v, false));
    let dev_iter = manifest.require_dev.iter().map(|(k, v)| (k, v, true));

    for (name, constraint, is_dev) in runtime_iter.chain(dev_iter) {
        // Skip platform requirements (`php`, `php-64bit`, `ext-*`,
        // `lib-*`, `composer-*`) — these aren't Packagist packages.
        if !name.contains('/') {
            continue;
        }
        if name == manifest_name {
            continue;
        }
        let lc_name = name.to_lowercase();
        let sanitized = sanitize_purl_version(constraint);
        let purl_str = format!("pkg:composer/{lc_name}@{sanitized}");
        let Ok(purl) = Purl::new(&purl_str) else {
            tracing::warn!(
                name = %name,
                constraint = %constraint,
                path = %composer_json_path.display(),
                "composer: skipping design-tier entry with non-PURL-safe constraint",
            );
            continue;
        };

        let lifecycle = if is_dev {
            LifecycleScope::Development
        } else {
            LifecycleScope::Runtime
        };
        let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        extra_annotations.insert(
            "mikebom:source-type".to_string(),
            serde_json::Value::String("composer-packagist".to_string()),
        );

        out.push(PackageDbEntry {
            purl,
            name: name.clone(),
            version: sanitized.clone(),
            arch: None,
            source_path: source_path.clone(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: Some(lifecycle),
            requirement_range: Some(constraint.clone()),
            source_type: Some("composer-packagist".to_string()),
            buildinfo_status: None,
            sbom_tier: Some("design".to_string()),
            evidence_kind: Some("composer-json".to_string()),
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
            shade_relocation: None,
            extra_annotations,
            binary_role: None,
            build_inclusion: None,
        });
    }
    out
}

/// PURL version segments forbid `/` per PURL spec. Replace with `_` so
/// design-tier constraints round-trip into a valid PURL. The raw
/// constraint is preserved verbatim in `requirement_range`.
fn sanitize_purl_version(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            '/' | '?' | '#' | ' ' => '_',
            other => other,
        })
        .collect()
}

/// FR-003: PURL construction per source/type discriminator.
///
/// Composer 2 lockfile shape note: most Packagist entries carry BOTH
/// `source` (upstream git info — `type: git`, url + reference) AND
/// `dist` (the actual Packagist-hosted zip — url + shasum). The
/// presence of `source.type: git` alone does NOT mean the entry is a
/// VCS-source-only dep — it usually means "Composer downloaded the
/// dist zip and tracked the upstream git reference for reproducibility."
///
/// Real VCS-source-only deps (operator declared `{"type":"vcs", ...}`
/// in `composer.json::repositories`) have `source` but no `dist`. Use
/// `dist.is_none()` as the discriminator for VCS-source-only vs
/// Packagist-backed entries.
fn build_purl_for_package(pkg: &LockfilePackage) -> Result<Purl, String> {
    if !pkg.name.contains('/') {
        return Err(format!(
            "package name not in <vendor>/<package> form: {}",
            pkg.name
        ));
    }
    if pkg.version.is_empty() {
        return Err(format!("package {} has empty version", pkg.name));
    }
    let lc_name = pkg.name.to_lowercase();
    let version = &pkg.version;
    let source_type = pkg.source.as_ref().and_then(|s| s.type_.as_deref());

    let purl_str = if source_type == Some("path") {
        // pkg:generic/<lc-vendor>-<lc-package>@<version> placeholder
        let flattened = lc_name.replace('/', "-");
        format!(
            "pkg:generic/{flattened}@{version}",
            version = sanitize_purl_version(version),
        )
    } else if let Some(scheme) = source_type
        .filter(|st| matches!(*st, "git" | "svn" | "hg") && pkg.dist.is_none())
    {
        // VCS-source-only entry: `source` present, NO `dist` →
        // operator-declared `"type":"vcs"` in composer.json repositories.
        let source = pkg.source.as_ref().expect("source present from match guard");
        if source.reference.as_deref().unwrap_or("").is_empty() {
            return Err(format!(
                "{} source missing source.reference for {}",
                scheme, pkg.name
            ));
        }
        let url = source
            .url
            .as_deref()
            .ok_or_else(|| format!("{} source missing url for {}", scheme, pkg.name))?;
        format!(
            "pkg:composer/{lc_name}@{version}?vcs_url={scheme}+{url}",
            url = minimal_qualifier_encode(url),
        )
    } else {
        // Default Packagist (with or without informational `source`),
        // composer-plugin, metapackage, or unknown. Determine
        // self-hosted vs default Packagist via dist.url.
        let mut s = format!("pkg:composer/{lc_name}@{version}");
        if let Some(dist) = &pkg.dist {
            if let Some(url) = dist.url.as_deref() {
                if let Some(base) = extract_url_base(url) {
                    if !is_default_packagist_host(&base) {
                        s.push_str("?repository_url=");
                        s.push_str(&minimal_qualifier_encode(&base));
                    }
                }
            }
        }
        s
    };

    Purl::new(&purl_str).map_err(|e| format!("PURL construction failed for {purl_str}: {e:?}"))
}

/// Returns the scheme + host portion of a URL via simple string slicing,
/// e.g. `https://repo.acme.example.com/p/foo` → `https://repo.acme.example.com`.
/// Returns `None` for malformed URLs (no `://`).
fn extract_url_base(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let after_scheme = &url[scheme_end + 3..];
    let host_end = after_scheme.find('/').unwrap_or(after_scheme.len());
    Some(format!("{}://{}", &url[..scheme_end], &after_scheme[..host_end]))
}

/// Per I1 remediation: three default URLs whose presence in `dist.url`
/// means "no `repository_url=` qualifier needed".
fn is_default_packagist_host(base: &str) -> bool {
    matches!(
        base,
        "https://packagist.org"
            | "https://repo.packagist.org"
            | "https://api.github.com"
    )
}

/// PURL qualifier-value encoding per the PURL spec's `pchar` rule.
fn minimal_qualifier_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '?' => out.push_str("%3F"),
            '#' => out.push_str("%23"),
            '&' => out.push_str("%26"),
            other => out.push(other),
        }
    }
    out
}

/// FR-003 source-type classification. Plugin/metapackage classification
/// takes precedence over source-type (plugin packages are typically
/// Packagist-hosted; the operator-mental-model bucket matters more).
///
/// VCS-source-only vs Packagist discrimination: a VCS-source-only entry
/// (operator declared `"type":"vcs"` in `composer.json::repositories`)
/// has `source.type` ∈ {git/svn/hg} AND NO `dist`. Real-world Packagist
/// entries carry BOTH `source` (informational upstream git) AND `dist`
/// (the actual Packagist-hosted download); these are classified as
/// `composer-packagist`. See `build_purl_for_package` for the symmetric
/// PURL-construction logic.
fn classify_source_type(pkg: &LockfilePackage) -> &'static str {
    match pkg.type_.as_str() {
        "composer-plugin" | "composer-installer" => return "composer-plugin",
        "metapackage" => return "composer-metapackage",
        _ => {}
    }
    let source_type = pkg.source.as_ref().and_then(|s| s.type_.as_deref());
    match (source_type, pkg.dist.is_some()) {
        (Some("path"), _) => "composer-path",
        (Some("git") | Some("svn") | Some("hg"), false) => "composer-vcs",
        _ => "composer-packagist",
    }
}

/// Per-source-type annotation bag.
fn build_extra_annotations(
    pkg: &LockfilePackage,
    source_type_value: &str,
) -> BTreeMap<String, serde_json::Value> {
    let mut out: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    out.insert(
        "mikebom:source-type".to_string(),
        serde_json::Value::String(source_type_value.to_string()),
    );
    match source_type_value {
        "composer-vcs" => {
            if let Some(s) = &pkg.source {
                if let Some(reference) = s.reference.as_deref() {
                    out.insert(
                        "mikebom:vcs-ref".to_string(),
                        serde_json::Value::String(reference.to_string()),
                    );
                }
            }
        }
        "composer-path" => {
            if let Some(s) = &pkg.source {
                if let Some(url) = s.url.as_deref() {
                    out.insert(
                        "mikebom:path".to_string(),
                        serde_json::Value::String(url.to_string()),
                    );
                }
            }
        }
        "composer-plugin" => {
            out.insert(
                "mikebom:composer-type".to_string(),
                serde_json::Value::String(pkg.type_.clone()),
            );
        }
        _ => {}
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn pkg_packagist(name: &str, version: &str, dist_url: Option<&str>, shasum: Option<&str>) -> LockfilePackage {
        LockfilePackage {
            name: name.into(),
            version: version.into(),
            type_: "library".into(),
            source: Some(LockfileSource {
                type_: None,
                url: dist_url.map(String::from),
                reference: None,
            }),
            dist: Some(LockfileDist {
                type_: Some("zip".into()),
                url: dist_url.map(String::from),
                reference: None,
                shasum: shasum.map(String::from),
            }),
            require: BTreeMap::new(),
            license: None,
        }
    }

    fn pkg_vcs(name: &str, version: &str, url: &str, reference: &str) -> LockfilePackage {
        LockfilePackage {
            name: name.into(),
            version: version.into(),
            type_: "library".into(),
            source: Some(LockfileSource {
                type_: Some("git".into()),
                url: Some(url.into()),
                reference: Some(reference.into()),
            }),
            dist: None,
            require: BTreeMap::new(),
            license: None,
        }
    }

    fn pkg_path(name: &str, version: &str, path: &str) -> LockfilePackage {
        LockfilePackage {
            name: name.into(),
            version: version.into(),
            type_: "library".into(),
            source: Some(LockfileSource {
                type_: Some("path".into()),
                url: Some(path.into()),
                reference: None,
            }),
            dist: None,
            require: BTreeMap::new(),
            license: None,
        }
    }

    fn pkg_with_type(name: &str, version: &str, ptype: &str) -> LockfilePackage {
        LockfilePackage {
            name: name.into(),
            version: version.into(),
            type_: ptype.into(),
            source: None,
            dist: Some(LockfileDist {
                type_: Some("zip".into()),
                url: Some("https://api.github.com/repos/foo/bar/zipball/abc".into()),
                reference: None,
                shasum: None,
            }),
            require: BTreeMap::new(),
            license: None,
        }
    }

    #[test]
    fn packagist_default_emits_bare_purl() {
        let p = pkg_packagist("symfony/console", "v7.0.4", Some("https://api.github.com/repos/symfony/console/zipball/abc"), Some(&"a".repeat(40)));
        let purl = build_purl_for_package(&p).unwrap();
        assert_eq!(purl.as_str(), "pkg:composer/symfony/console@v7.0.4");
    }

    #[test]
    fn packagist_self_hosted_emits_repository_url_qualifier() {
        let p = pkg_packagist("acme/internal_lib", "2.0.0", Some("https://repo.acme.example.com/dist/acme/internal_lib/abc.zip"), Some(&"a".repeat(40)));
        let purl = build_purl_for_package(&p).unwrap();
        assert_eq!(
            purl.as_str(),
            "pkg:composer/acme/internal_lib@2.0.0?repository_url=https://repo.acme.example.com"
        );
    }

    #[test]
    fn packagist_default_repo_packagist_org_omits_qualifier() {
        let p = pkg_packagist("foo/bar", "1.0.0", Some("https://repo.packagist.org/p/foo/bar.zip"), None);
        let purl = build_purl_for_package(&p).unwrap();
        assert_eq!(purl.as_str(), "pkg:composer/foo/bar@1.0.0");
    }

    #[test]
    fn packagist_default_packagist_org_omits_qualifier() {
        let p = pkg_packagist("foo/bar", "1.0.0", Some("https://packagist.org/p/foo/bar.zip"), None);
        let purl = build_purl_for_package(&p).unwrap();
        assert_eq!(purl.as_str(), "pkg:composer/foo/bar@1.0.0");
    }

    #[test]
    fn vendor_name_lowercased_per_purl_spec() {
        let p = pkg_packagist("ACME/MyLib", "1.0.0", None, None);
        let purl = build_purl_for_package(&p).unwrap();
        assert_eq!(purl.as_str(), "pkg:composer/acme/mylib@1.0.0");
    }

    #[test]
    fn vcs_source_emits_vcs_url_qualifier() {
        let resolved = "eb39649a76b87e8451baf75d10ce82ca3a3d5601";
        let p = pkg_vcs("acme/my-fork", "dev-main", "https://github.com/acme/my-fork.git", resolved);
        let purl = build_purl_for_package(&p).unwrap();
        assert_eq!(
            purl.as_str(),
            "pkg:composer/acme/my-fork@dev-main?vcs_url=git+https://github.com/acme/my-fork.git"
        );
    }

    #[test]
    fn vcs_source_missing_reference_errors() {
        let mut p = pkg_vcs("acme/r", "1.0.0", "https://example.com/r.git", "abc");
        p.source.as_mut().unwrap().reference = None;
        assert!(build_purl_for_package(&p).is_err());
    }

    #[test]
    fn vcs_source_empty_reference_errors() {
        let mut p = pkg_vcs("acme/r", "1.0.0", "https://example.com/r.git", "abc");
        p.source.as_mut().unwrap().reference = Some(String::new());
        assert!(build_purl_for_package(&p).is_err());
    }

    #[test]
    fn path_source_emits_generic_placeholder_with_flattened_vendor() {
        let p = pkg_path("acme/local-lib", "0.1.0", "../packages/local-lib");
        let purl = build_purl_for_package(&p).unwrap();
        assert_eq!(purl.as_str(), "pkg:generic/acme-local-lib@0.1.0");
    }

    #[test]
    fn package_name_without_slash_errors() {
        let p = pkg_packagist("nameonly", "1.0.0", None, None);
        assert!(build_purl_for_package(&p).is_err());
    }

    #[test]
    fn empty_version_errors() {
        let p = pkg_packagist("foo/bar", "", None, None);
        assert!(build_purl_for_package(&p).is_err());
    }

    #[test]
    fn classify_source_type_plugin_precedence() {
        let p = pkg_with_type("composer/installers", "v2.3.0", "composer-plugin");
        assert_eq!(classify_source_type(&p), "composer-plugin");
    }

    #[test]
    fn classify_source_type_legacy_installer_maps_to_plugin() {
        let p = pkg_with_type("legacy/installer", "1.0.0", "composer-installer");
        assert_eq!(classify_source_type(&p), "composer-plugin");
    }

    #[test]
    fn classify_source_type_metapackage() {
        let p = pkg_with_type("symfony/symfony", "v7.0.4", "metapackage");
        assert_eq!(classify_source_type(&p), "composer-metapackage");
    }

    #[test]
    fn classify_source_type_packagist_default() {
        let p = pkg_packagist("symfony/console", "v7.0.4", None, None);
        assert_eq!(classify_source_type(&p), "composer-packagist");
    }

    #[test]
    fn classify_source_type_vcs() {
        let p = pkg_vcs("acme/fork", "dev-main", "https://example.com/r.git", &"a".repeat(40));
        assert_eq!(classify_source_type(&p), "composer-vcs");
    }

    #[test]
    fn classify_source_type_path() {
        let p = pkg_path("acme/local", "0.1.0", "../local");
        assert_eq!(classify_source_type(&p), "composer-path");
    }

    #[test]
    fn build_extra_annotations_packagist_only_carries_source_type() {
        let p = pkg_packagist("foo/bar", "1.0.0", None, None);
        let ann = build_extra_annotations(&p, "composer-packagist");
        assert_eq!(
            ann.get("mikebom:source-type").and_then(|v| v.as_str()),
            Some("composer-packagist")
        );
        assert!(!ann.contains_key("mikebom:vcs-ref"));
    }

    #[test]
    fn build_extra_annotations_vcs_carries_vcs_ref() {
        let p = pkg_vcs("acme/fork", "dev-main", "https://example.com/r.git", "eb39649abc");
        let ann = build_extra_annotations(&p, "composer-vcs");
        assert_eq!(
            ann.get("mikebom:vcs-ref").and_then(|v| v.as_str()),
            Some("eb39649abc")
        );
    }

    #[test]
    fn build_extra_annotations_path_carries_path() {
        let p = pkg_path("acme/local", "0.1.0", "../packages/local");
        let ann = build_extra_annotations(&p, "composer-path");
        assert_eq!(
            ann.get("mikebom:path").and_then(|v| v.as_str()),
            Some("../packages/local")
        );
    }

    #[test]
    fn build_extra_annotations_plugin_carries_composer_type() {
        let p = pkg_with_type("composer/installers", "v2.3.0", "composer-plugin");
        let ann = build_extra_annotations(&p, "composer-plugin");
        assert_eq!(
            ann.get("mikebom:composer-type").and_then(|v| v.as_str()),
            Some("composer-plugin")
        );
    }

    #[test]
    fn extract_url_base_strips_path() {
        assert_eq!(
            extract_url_base("https://repo.acme.example.com/path/to/pkg.zip"),
            Some("https://repo.acme.example.com".to_string())
        );
        assert_eq!(
            extract_url_base("https://packagist.org"),
            Some("https://packagist.org".to_string())
        );
    }

    #[test]
    fn extract_url_base_returns_none_for_malformed() {
        assert_eq!(extract_url_base("not-a-url"), None);
    }

    #[test]
    fn is_default_packagist_host_recognizes_three() {
        assert!(is_default_packagist_host("https://packagist.org"));
        assert!(is_default_packagist_host("https://repo.packagist.org"));
        assert!(is_default_packagist_host("https://api.github.com"));
        assert!(!is_default_packagist_host("https://repo.acme.example.com"));
    }

    #[test]
    fn license_polymorphism_string_or_array() {
        // Single-string form
        let json_single = r#"{"name":"foo/bar","version":"1.0","type":"library","license":"MIT"}"#;
        let pkg_single: LockfilePackage = serde_json::from_str(json_single).unwrap();
        assert!(matches!(pkg_single.license, Some(LockfileLicense::Single(_))));

        // Array form
        let json_array = r#"{"name":"foo/bar","version":"1.0","type":"library","license":["MIT","Apache-2.0"]}"#;
        let pkg_array: LockfilePackage = serde_json::from_str(json_array).unwrap();
        assert!(matches!(pkg_array.license, Some(LockfileLicense::List(_))));
    }

    #[test]
    fn installed_json_composer_2_wrapper_parses() {
        let json = r#"{
            "packages": [{"name":"foo/bar","version":"1.0","type":"library"}],
            "dev": true,
            "dev-package-names": ["phpunit/phpunit"]
        }"#;
        let parsed: InstalledJson = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.packages.len(), 1);
        assert!(parsed.dev);
        assert_eq!(parsed.dev_package_names, vec!["phpunit/phpunit".to_string()]);
    }

    #[test]
    fn sanitize_purl_version_neutralizes_slashes() {
        assert_eq!(sanitize_purl_version("^7.0"), "^7.0");
        assert_eq!(sanitize_purl_version(">=1.0 <2.0"), ">=1.0_<2.0");
        assert_eq!(sanitize_purl_version("git/foo"), "git_foo");
    }
}
