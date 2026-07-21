//! Milestone 137 — Dart/Flutter pub ecosystem reader.
//!
//! Discovers each `pubspec.yaml` under the scan root and emits one
//! main-module component per project (FR-012) plus one component per
//! lockfile entry when a sibling `pubspec.lock` is present (FR-002).
//! When the lockfile is absent or malformed, falls back to design-tier
//! emission from the manifest's `dependencies:` + `dev_dependencies:`
//! blocks (FR-005 / R7).
//!
//! PURL shapes per FR-003 (confirmed against the purl-spec
//! `pub-definition.md`):
//!
//! - **hosted**: `pkg:pub/<name>@<version>[?repository_url=<url>]`
//!   (qualifier omitted when `description.url` is `https://pub.dev` or
//!   the legacy `https://pub.dartlang.org`).
//! - **git**:    `pkg:pub/<name>@<resolved-sha>?vcs_url=git+<url>[#<subpath>]`
//!   (`git+` scheme prefix per purl-spec git-source convention;
//!   `description.path` surfaces as the `#<subpath>` fragment when
//!   non-trivial).
//! - **path**:   `pkg:generic/<name>@<version>` placeholder + the
//!   `waybill:source-type = "pub-path"` annotation (path-deps have no
//!   purl-spec-addressable identity per R1).
//! - **sdk**:    `pkg:pub/<sdk-name>@0.0.0` (the literal `0.0.0` is
//!   the purl-spec canonical example for SDK pseudo-deps).
//!
//! `waybill:source-type` values use the `pub-` prefix to avoid
//! collision with cargo's existing C1-row values (`git`/`path`/
//! `registry`) — per R3 and milestone-122's `kmp-` precedent.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use waybill_common::types::hash::ContentHash;
use waybill_common::types::purl::Purl;

use super::exclude_path::ExclusionSet;
use super::PackageDbEntry;

/// Per-project source-tree depth bound for the pubspec walker. Mirrors
/// the gem reader's MAX_PROJECT_ROOT_DEPTH (6) — Dart projects are
/// typically a flat or shallow `lib/` + `test/` + `pubspec.yaml` at
/// root; 6 covers Melos monorepos with `packages/<member>/pubspec.yaml`
/// and pub workspaces.
const MAX_DART_WALK_DEPTH: usize = 8;

/// Directories the dart walker MUST NOT descend into. These never carry
/// the developer's own `pubspec.yaml` and would only produce false-
/// positive components from per-package cached metadata.
fn should_skip_descent(name: &str) -> bool {
    matches!(
        name,
        // pub's per-project download cache
        ".dart_tool"
        // pub-cache hosted/<host>/<pkg>-<ver>/pubspec.yaml entries
        | ".pub-cache"
        // Flutter framework's own build output
        | "build"
        // VCS metadata
        | ".git"
        | ".hg"
        | ".svn"
        // Node-style nested deps (rare in Dart but cheap to skip)
        | "node_modules"
    )
}

/// Reader-private serde representation of a `pubspec.yaml` (subset
/// consumed by this reader per data-model.md).
#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct PubspecYaml {
    name: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    dependencies: BTreeMap<String, serde_yaml::Value>,
    #[serde(default)]
    dev_dependencies: BTreeMap<String, serde_yaml::Value>,
    #[serde(default)]
    dependency_overrides: BTreeMap<String, serde_yaml::Value>,
    #[serde(default)]
    environment: Option<serde_yaml::Value>,
}

/// Reader-private serde representation of a `pubspec.lock`.
#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct PubspecLock {
    #[serde(default)]
    packages: BTreeMap<String, LockfileEntry>,
    #[serde(default)]
    sdks: Option<serde_yaml::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct LockfileEntry {
    dependency: String,
    description: LockfileDescription,
    source: String,
    version: String,
}

/// Polymorphic `description:` per the lockfile's `source:` discriminator.
/// For `source: sdk` the YAML is a bare scalar string (the SDK name);
/// for hosted/git/path the YAML is a map. Use `#[serde(untagged)]` so
/// serde tries each variant in declared order.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
enum LockfileDescription {
    Sdk(String),
    Map(LockfileDescriptionMap),
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[allow(dead_code)]
struct LockfileDescriptionMap {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default, rename = "ref")]
    ref_: Option<String>,
    #[serde(default, rename = "resolved-ref")]
    resolved_ref: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    relative: Option<bool>,
}

/// Lifetime is per-scan; no persistence (matches every language-reader
/// since milestone 002).
pub fn read(
    rootfs: &Path,
    include_dev: bool,
    exclude_set: &ExclusionSet,
) -> Vec<PackageDbEntry> {
    let mut out: Vec<PackageDbEntry> = Vec::new();
    let mut seen_purls: HashSet<String> = HashSet::new();
    let mut projects = 0usize;
    let mut lockfile_projects = 0usize;
    let mut design_tier_projects = 0usize;
    let mut warned_lockfile_parse_failures = 0usize;

    for pubspec_path in find_dart_projects(rootfs, exclude_set) {
        let pubspec_yaml = match parse_pubspec_yaml(&pubspec_path) {
            Ok(y) if !y.name.is_empty() => y,
            Ok(_) => {
                tracing::warn!(
                    path = %pubspec_path.display(),
                    "dart: pubspec.yaml missing required `name:` field; skipping main-module",
                );
                continue;
            }
            Err(err) => {
                tracing::warn!(
                    path = %pubspec_path.display(),
                    error = %err,
                    "dart: failed to parse pubspec.yaml; skipping project",
                );
                continue;
            }
        };
        projects += 1;
        let project_dir = match pubspec_path.parent() {
            Some(d) => d,
            None => continue,
        };
        let lockfile_path = project_dir.join("pubspec.lock");

        let parsed_lockfile: Option<PubspecLock> = if lockfile_path.is_file() {
            match parse_pubspec_lock(&lockfile_path) {
                Ok(lock) => Some(lock),
                Err(err) => {
                    warned_lockfile_parse_failures += 1;
                    tracing::warn!(
                        path = %lockfile_path.display(),
                        error = %err,
                        "dart: failed to parse pubspec.lock, falling back to design-tier from pubspec.yaml",
                    );
                    None
                }
            }
        } else {
            None
        };

        // Emit the main-module first so dep-edge wiring can reference
        // its bom-ref (the orchestrator's seen_purls dedup keeps a
        // single main-module per project).
        if let Some(main_module) =
            emit_main_module(&pubspec_path, &pubspec_yaml, parsed_lockfile.as_ref(), include_dev)
        {
            let purl_key = main_module.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(main_module);
            }
        }

        match parsed_lockfile {
            Some(lock) => {
                lockfile_projects += 1;
                let entries = emit_lockfile_entries(&lockfile_path, &lock, include_dev);
                for entry in entries {
                    let purl_key = entry.purl.as_str().to_string();
                    if seen_purls.insert(purl_key) {
                        out.push(entry);
                    }
                }
            }
            None => {
                design_tier_projects += 1;
                let entries =
                    emit_design_tier_components(&pubspec_path, &pubspec_yaml, include_dev);
                for entry in entries {
                    let purl_key = entry.purl.as_str().to_string();
                    if seen_purls.insert(purl_key) {
                        out.push(entry);
                    }
                }
            }
        }
    }

    if !out.is_empty() || warned_lockfile_parse_failures > 0 {
        tracing::info!(
            rootfs = %rootfs.display(),
            entries = out.len(),
            projects,
            lockfile_projects,
            design_tier_projects,
            warned_lockfile_parse_failures,
            include_dev,
            "parsed pubspec.yaml + pubspec.lock entries",
        );
    }
    out
}

/// Walk the rootfs for `pubspec.yaml` files (milestone 114 safe_walk).
/// Output is lex-sorted for cross-platform deterministic discovery.
fn find_dart_projects(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_DART_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_descent(name)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file()
            && path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|n| n.eq_ignore_ascii_case("pubspec.yaml"))
                .unwrap_or(false)
        {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

fn parse_pubspec_yaml(path: &Path) -> anyhow::Result<PubspecYaml> {
    let bytes = std::fs::read(path)
        .map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    let parsed: PubspecYaml = serde_yaml::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("yaml parse failed: {e}"))?;
    Ok(parsed)
}

fn parse_pubspec_lock(path: &Path) -> anyhow::Result<PubspecLock> {
    let bytes = std::fs::read(path)
        .map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    let parsed: PubspecLock = serde_yaml::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("yaml parse failed: {e}"))?;
    Ok(parsed)
}

/// FR-012: emit one main-module per `pubspec.yaml` discovered.
/// Returns `None` only when `name` is empty (validated by caller).
fn emit_main_module(
    pubspec_path: &Path,
    pubspec_yaml: &PubspecYaml,
    parsed_lockfile: Option<&PubspecLock>,
    include_dev: bool,
) -> Option<PackageDbEntry> {
    // Milestone 197 US3 (#567): emit versionless canonical PURL per
    // purl-spec when pubspec.yaml has no `version:` — matches m191
    // fix pattern (npm/cargo/maven/gem/pip).
    let raw_version = pubspec_yaml.version.clone();
    let version = raw_version
        .clone()
        .unwrap_or_else(|| "0.0.0-unknown".to_string());
    let purl_str = if raw_version.as_deref().unwrap_or("").is_empty() {
        format!("pkg:pub/{}", pubspec_yaml.name)
    } else {
        format!("pkg:pub/{}@{}", pubspec_yaml.name, version)
    };
    let purl = Purl::new(&purl_str).ok()?;

    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert(
        "waybill:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );
    extra_annotations.insert(
        "waybill:source-type".to_string(),
        serde_json::Value::String("pub-main-module".to_string()),
    );

    // Dep edges per FR-004: main-module → direct deps.
    // Lockfile mode: lockfile's `direct main` + (if include_dev) `direct dev`
    //   + `direct overridden` (R2 lifecycle mapping).
    // Design-tier mode: pubspec.yaml's `dependencies` + (if include_dev) `dev_dependencies`.
    let depends: Vec<String> = if let Some(lock) = parsed_lockfile {
        lock.packages
            .iter()
            .filter(|(_, e)| match e.dependency.as_str() {
                "direct main" | "direct overridden" => true,
                "direct dev" => include_dev,
                _ => false,
            })
            .map(|(name, _)| name.clone())
            .collect()
    } else {
        let mut d: Vec<String> = pubspec_yaml.dependencies.keys().cloned().collect();
        if include_dev {
            d.extend(pubspec_yaml.dev_dependencies.keys().cloned());
        }
        d
    };

    Some(PackageDbEntry {
        purl,
        name: pubspec_yaml.name.clone(),
        version,
        arch: None,
        source_path: pubspec_path.to_string_lossy().into_owned(),
        depends,
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type: Some("pub-main-module".to_string()),
        buildinfo_status: None,
        sbom_tier: Some("source".to_string()),
        evidence_kind: Some("pubspec-yaml".to_string()),
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

/// FR-002 + FR-003 + FR-008: one component per lockfile entry, with
/// per-source-type PURL + annotation shape.
fn emit_lockfile_entries(
    lockfile_path: &Path,
    pubspec_lock: &PubspecLock,
    include_dev: bool,
) -> Vec<PackageDbEntry> {
    use waybill_common::resolution::LifecycleScope;

    let mut out = Vec::new();
    let source_path = lockfile_path.to_string_lossy().into_owned();
    for (name, entry) in &pubspec_lock.packages {
        if entry.dependency == "direct dev" && !include_dev {
            continue;
        }

        let purl = match build_purl_for_lockfile_entry(name, entry) {
            Ok(p) => p,
            Err(err) => {
                tracing::warn!(
                    name = %name,
                    path = %lockfile_path.display(),
                    error = %err,
                    "dart: skipping malformed lockfile entry",
                );
                continue;
            }
        };

        let source_type_value = match entry.source.as_str() {
            "hosted" => "pub-hosted",
            "git" => "pub-git",
            "path" => "pub-path",
            "sdk" => "pub-sdk",
            _ => continue,
        };
        let lifecycle_scope = if entry.dependency == "direct dev" {
            Some(LifecycleScope::Development)
        } else {
            Some(LifecycleScope::Runtime)
        };

        let hashes = match (&entry.source[..], &entry.description) {
            ("hosted", LockfileDescription::Map(m)) => match &m.sha256 {
                Some(hex) if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) => {
                    match ContentHash::sha256(hex) {
                        Ok(h) => vec![h],
                        Err(_) => Vec::new(),
                    }
                }
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };

        let extra_annotations = build_extra_annotations(entry, source_type_value);

        out.push(PackageDbEntry {
            purl,
            name: name.clone(),
            version: entry.version.clone(),
            arch: None,
            source_path: source_path.clone(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope,
            requirement_ranges: Vec::new(),
            source_type: Some(source_type_value.to_string()),
            buildinfo_status: None,
            sbom_tier: Some("source".to_string()),
            evidence_kind: Some("pubspec-lock".to_string()),
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
        });
    }
    out
}

/// FR-005: design-tier emission when no lockfile is present.
fn emit_design_tier_components(
    pubspec_path: &Path,
    pubspec_yaml: &PubspecYaml,
    include_dev: bool,
) -> Vec<PackageDbEntry> {
    use waybill_common::resolution::LifecycleScope;

    let mut out = Vec::new();
    let source_path = pubspec_path.to_string_lossy().into_owned();

    // Iterate runtime deps, then dev-deps when allowed. Skip the
    // main-module's own name (defensive — shouldn't appear but
    // pubspec.yaml is operator-authored).
    let runtime_iter = pubspec_yaml
        .dependencies
        .iter()
        .map(|(k, v)| (k, v, false));
    let dev_iter = pubspec_yaml
        .dev_dependencies
        .iter()
        .map(|(k, v)| (k, v, true));

    for (name, value, is_dev) in runtime_iter.chain(dev_iter) {
        if is_dev && !include_dev {
            continue;
        }
        if name == &pubspec_yaml.name {
            continue;
        }

        let constraint = constraint_from_yaml_value(value);
        let purl_str = format!("pkg:pub/{name}@{constraint}");
        let Ok(purl) = Purl::new(&purl_str) else {
            tracing::warn!(
                name = %name,
                constraint = %constraint,
                path = %pubspec_path.display(),
                "dart: skipping design-tier entry with non-PURL-safe constraint string",
            );
            continue;
        };

        let lifecycle_scope = if is_dev {
            Some(LifecycleScope::Development)
        } else {
            Some(LifecycleScope::Runtime)
        };
        let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        extra_annotations.insert(
            "waybill:source-type".to_string(),
            serde_json::Value::String("pub-hosted".to_string()),
        );

        out.push(PackageDbEntry {
            purl,
            name: name.clone(),
            version: constraint.clone(),
            arch: None,
            source_path: source_path.clone(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope,
            requirement_ranges: vec![constraint],
            source_type: Some("pub-hosted".to_string()),
            buildinfo_status: None,
            sbom_tier: Some("design".to_string()),
            evidence_kind: Some("pubspec-yaml".to_string()),
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

/// Extract a constraint string from the heterogeneous YAML value form
/// of a pubspec.yaml dependency entry. Scalar strings (`^1.0.0`) are
/// passed through verbatim. Map forms (`path:`, `git:`, `sdk:`) are
/// design-tier-best-effort and map to the literal `"unspecified"`
/// placeholder — design-tier emission cannot resolve non-hosted
/// sources without a lockfile.
fn constraint_from_yaml_value(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(s) => sanitize_purl_version(s),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Null => "unspecified".to_string(),
        _ => "unspecified".to_string(),
    }
}

/// PURL version segments forbid `/` (per PURL spec). Replace with `_`
/// for design-tier emission so the constraint round-trips into a valid
/// PURL. Other PURL-sensitive chars (e.g. `?`, `#`) are similarly
/// neutralized. The raw constraint is preserved verbatim in
/// `requirement_range`; this transform applies only to the version
/// segment.
fn sanitize_purl_version(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            '/' | '?' | '#' | ' ' => '_',
            other => other,
        })
        .collect()
}

/// FR-003 PURL construction per source type.
fn build_purl_for_lockfile_entry(
    name: &str,
    entry: &LockfileEntry,
) -> Result<Purl, String> {
    let purl_str = match (entry.source.as_str(), &entry.description) {
        ("hosted", LockfileDescription::Map(m)) => {
            let mut s = format!("pkg:pub/{}@{}", name, entry.version);
            if let Some(url) = m.url.as_deref() {
                if url != "https://pub.dev" && url != "https://pub.dartlang.org" {
                    s.push_str("?repository_url=");
                    s.push_str(&minimal_qualifier_encode(url));
                }
            }
            s
        }
        ("git", LockfileDescription::Map(m)) => {
            let resolved_ref = m
                .resolved_ref
                .as_deref()
                .ok_or_else(|| "git source missing resolved-ref".to_string())?;
            if resolved_ref.len() != 40
                || !resolved_ref.chars().all(|c| c.is_ascii_hexdigit())
            {
                return Err(format!("git resolved-ref not 40-char hex: {resolved_ref}"));
            }
            let url = m
                .url
                .as_deref()
                .ok_or_else(|| "git source missing url".to_string())?;
            let subpath = m
                .path
                .as_deref()
                .filter(|p| !p.is_empty() && *p != ".")
                .map(|p| format!("#{p}"))
                .unwrap_or_default();
            format!(
                "pkg:pub/{name}@{resolved_ref}?vcs_url=git+{url}{subpath}",
                url = minimal_qualifier_encode(url),
            )
        }
        ("path", LockfileDescription::Map(_)) => {
            format!("pkg:generic/{name}@{version}", version = sanitize_purl_version(&entry.version))
        }
        ("sdk", LockfileDescription::Sdk(_sdk_family)) => {
            // Per FR-011 + purl-spec canonical example: literal @0.0.0
            // (never derived from entry.version, though entry.version
            // is itself "0.0.0" per pub convention).
            format!("pkg:pub/{name}@0.0.0")
        }
        ("sdk", LockfileDescription::Map(_)) => {
            // Tolerant: some lockfile generators may emit sdk source
            // with a map-shaped description. Still emit per FR-011.
            format!("pkg:pub/{name}@0.0.0")
        }
        (other, _) => {
            return Err(format!(
                "unknown source discriminator: {other} (expected hosted|git|path|sdk)"
            ));
        }
    };
    Purl::new(&purl_str).map_err(|e| format!("PURL construction failed for {purl_str}: {e:?}"))
}

/// PURL qualifier-value encoding per the PURL spec's `pchar` rule.
/// Allows `:` / `/` / `@` / `=` (URL-readable); encodes only the chars
/// that would break PURL parsing (`?`, `#`, `&`) plus whitespace.
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

/// Per-source-type annotation bag (R3 + data-model.md).
fn build_extra_annotations(
    entry: &LockfileEntry,
    source_type_value: &str,
) -> BTreeMap<String, serde_json::Value> {
    let mut out: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    out.insert(
        "waybill:source-type".to_string(),
        serde_json::Value::String(source_type_value.to_string()),
    );
    match (&entry.source[..], &entry.description) {
        ("git", LockfileDescription::Map(m)) => {
            if let Some(r) = m.ref_.as_deref() {
                out.insert(
                    "waybill:vcs-ref".to_string(),
                    serde_json::Value::String(r.to_string()),
                );
            }
        }
        ("path", LockfileDescription::Map(m)) => {
            if let Some(p) = m.path.as_deref() {
                out.insert(
                    "waybill:path".to_string(),
                    serde_json::Value::String(p.to_string()),
                );
            }
        }
        ("sdk", LockfileDescription::Sdk(family)) => {
            out.insert(
                "waybill:sdk-name".to_string(),
                serde_json::Value::String(family.to_string()),
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

    fn hosted_map(url: &str, sha: &str) -> LockfileEntry {
        LockfileEntry {
            dependency: "direct main".into(),
            description: LockfileDescription::Map(LockfileDescriptionMap {
                name: Some("foo".into()),
                sha256: Some(sha.into()),
                url: Some(url.into()),
                ..Default::default()
            }),
            source: "hosted".into(),
            version: "1.2.3".into(),
        }
    }

    fn git_map(url: &str, resolved: &str, path: Option<&str>, ref_: Option<&str>) -> LockfileEntry {
        LockfileEntry {
            dependency: "direct main".into(),
            description: LockfileDescription::Map(LockfileDescriptionMap {
                url: Some(url.into()),
                resolved_ref: Some(resolved.into()),
                path: path.map(String::from),
                ref_: ref_.map(String::from),
                ..Default::default()
            }),
            source: "git".into(),
            version: "0.0.0".into(),
        }
    }

    #[test]
    fn hosted_default_pubdev_emits_bare_purl() {
        let e = hosted_map("https://pub.dev", "a".repeat(64).as_str());
        let p = build_purl_for_lockfile_entry("http", &e).unwrap();
        assert_eq!(p.as_str(), "pkg:pub/http@1.2.3");
    }

    #[test]
    fn hosted_legacy_dartlang_default_emits_bare_purl() {
        let e = hosted_map("https://pub.dartlang.org", "a".repeat(64).as_str());
        let p = build_purl_for_lockfile_entry("http", &e).unwrap();
        assert_eq!(p.as_str(), "pkg:pub/http@1.2.3");
    }

    #[test]
    fn hosted_self_hosted_emits_repository_url_qualifier() {
        let e = hosted_map("https://pub.acme.example.com", "a".repeat(64).as_str());
        let p = build_purl_for_lockfile_entry("internal_lib", &e).unwrap();
        assert_eq!(
            p.as_str(),
            "pkg:pub/internal_lib@1.2.3?repository_url=https://pub.acme.example.com"
        );
    }

    #[test]
    fn git_source_emits_resolved_sha_plus_vcs_url_with_subpath() {
        let resolved = "eb39649a76b87e8451baf75d10ce82ca3a3d5601";
        let e = git_map(
            "https://github.com/google/flutter-desktop-embedding.git",
            resolved,
            Some("plugins/window_size"),
            Some("master"),
        );
        let p = build_purl_for_lockfile_entry("window_size", &e).unwrap();
        assert_eq!(
            p.as_str(),
            format!(
                "pkg:pub/window_size@{resolved}?vcs_url=git+https://github.com/google/flutter-desktop-embedding.git#plugins/window_size"
            )
        );
    }

    #[test]
    fn git_source_omits_subpath_when_dot_or_empty() {
        let resolved = "eb39649a76b87e8451baf75d10ce82ca3a3d5601";
        let e_dot = git_map("https://example.com/r.git", resolved, Some("."), None);
        let p_dot = build_purl_for_lockfile_entry("r", &e_dot).unwrap();
        assert!(!p_dot.as_str().contains('#'), "got: {}", p_dot.as_str());

        let e_empty = git_map("https://example.com/r.git", resolved, Some(""), None);
        let p_empty = build_purl_for_lockfile_entry("r", &e_empty).unwrap();
        assert!(!p_empty.as_str().contains('#'), "got: {}", p_empty.as_str());
    }

    #[test]
    fn git_source_missing_resolved_ref_errors() {
        let e = LockfileEntry {
            dependency: "direct main".into(),
            description: LockfileDescription::Map(LockfileDescriptionMap {
                url: Some("https://example.com/r.git".into()),
                resolved_ref: None,
                ..Default::default()
            }),
            source: "git".into(),
            version: "0.0.0".into(),
        };
        assert!(build_purl_for_lockfile_entry("r", &e).is_err());
    }

    #[test]
    fn git_source_short_sha_errors() {
        let e = git_map("https://example.com/r.git", "abc123", None, None);
        assert!(build_purl_for_lockfile_entry("r", &e).is_err());
    }

    #[test]
    fn path_source_emits_generic_placeholder() {
        let e = LockfileEntry {
            dependency: "direct main".into(),
            description: LockfileDescription::Map(LockfileDescriptionMap {
                path: Some("../packages/my_local_lib".into()),
                relative: Some(true),
                ..Default::default()
            }),
            source: "path".into(),
            version: "0.1.0".into(),
        };
        let p = build_purl_for_lockfile_entry("my_local_lib", &e).unwrap();
        assert_eq!(p.as_str(), "pkg:generic/my_local_lib@0.1.0");
    }

    #[test]
    fn sdk_source_emits_zero_zero_zero_version() {
        let e = LockfileEntry {
            dependency: "direct main".into(),
            description: LockfileDescription::Sdk("flutter".into()),
            source: "sdk".into(),
            version: "0.0.0".into(),
        };
        let p = build_purl_for_lockfile_entry("flutter", &e).unwrap();
        assert_eq!(p.as_str(), "pkg:pub/flutter@0.0.0");

        let e2 = LockfileEntry {
            dependency: "direct main".into(),
            description: LockfileDescription::Sdk("flutter".into()),
            source: "sdk".into(),
            version: "0.0.0".into(),
        };
        let p2 = build_purl_for_lockfile_entry("flutter_test", &e2).unwrap();
        assert_eq!(p2.as_str(), "pkg:pub/flutter_test@0.0.0");
    }

    #[test]
    fn unknown_source_discriminator_errors() {
        let e = LockfileEntry {
            dependency: "direct main".into(),
            description: LockfileDescription::Map(LockfileDescriptionMap::default()),
            source: "hg".into(),
            version: "1.0.0".into(),
        };
        assert!(build_purl_for_lockfile_entry("foo", &e).is_err());
    }

    #[test]
    fn extra_annotations_carry_source_type() {
        let e = hosted_map("https://pub.dev", "a".repeat(64).as_str());
        let ann = build_extra_annotations(&e, "pub-hosted");
        assert_eq!(
            ann.get("waybill:source-type").and_then(|v| v.as_str()),
            Some("pub-hosted")
        );
    }

    #[test]
    fn extra_annotations_git_carries_vcs_ref() {
        let e = git_map(
            "https://example.com/r.git",
            &"a".repeat(40),
            None,
            Some("v1.0.0"),
        );
        let ann = build_extra_annotations(&e, "pub-git");
        assert_eq!(
            ann.get("waybill:vcs-ref").and_then(|v| v.as_str()),
            Some("v1.0.0")
        );
    }

    #[test]
    fn extra_annotations_sdk_carries_sdk_name() {
        let e = LockfileEntry {
            dependency: "direct main".into(),
            description: LockfileDescription::Sdk("flutter".into()),
            source: "sdk".into(),
            version: "0.0.0".into(),
        };
        let ann = build_extra_annotations(&e, "pub-sdk");
        assert_eq!(
            ann.get("waybill:sdk-name").and_then(|v| v.as_str()),
            Some("flutter")
        );
    }

    #[test]
    fn minimal_qualifier_encode_passes_through_url_chars() {
        assert_eq!(
            minimal_qualifier_encode("https://pub.acme.example.com"),
            "https://pub.acme.example.com"
        );
        assert_eq!(
            minimal_qualifier_encode("with space"),
            "with%20space"
        );
        assert_eq!(
            minimal_qualifier_encode("with#hash&amp"),
            "with%23hash%26amp"
        );
    }

    #[test]
    fn sanitize_purl_version_neutralizes_slashes() {
        assert_eq!(sanitize_purl_version("^1.0.0"), "^1.0.0");
        assert_eq!(sanitize_purl_version(">=1.0.0 <2.0.0"), ">=1.0.0_<2.0.0");
        assert_eq!(sanitize_purl_version("git/foo"), "git_foo");
    }
}
