//! Milestone 139 — CocoaPods ecosystem reader.
//!
//! Discovers CocoaPods 1.0+ projects under the scan root via three
//! input artifacts:
//!
//! - `Podfile.lock` (YAML, source-tier) — primary lockfile per FR-002.
//! - `Podfile` (Ruby DSL, regex-extracted) — design-tier fallback per
//!   FR-005 + main-module-name source per FR-012.
//! - `Pods/Manifest.lock` (YAML, same shape as Podfile.lock) —
//!   deployed-tier per FR-006 + Q3 clarification, but only when no
//!   sibling `Podfile.lock` exists (FR-011 dedup).
//!
//! PURL shapes per FR-003 (confirmed against the purl-spec
//! [cocoapods-definition.md](https://github.com/package-url/purl-spec/blob/main/types-doc/cocoapods-definition.md)):
//!
//! - **trunk** (default): `pkg:cocoapods/<pod>@<version>`.
//! - **trunk subspec**: `pkg:cocoapods/<root>@<version>#<subpath>` —
//!   per Phase 0 research correction, subspecs use the PURL `#subpath`
//!   mechanism (NOT a `?subspec=` qualifier — that was the initial
//!   spec guess). Multi-level subspecs preserve `/` between segments.
//! - **git**: `pkg:cocoapods/<pod>@<version>?vcs_url=git+<url>` — URL
//!   from `EXTERNAL SOURCES{:git}`; resolved 40-char SHA from `CHECKOUT
//!   OPTIONS{:commit}` flows into `mikebom:vcs-ref` annotation per Q2.
//! - **path**: `pkg:generic/<flattened-pod>@<version>` — pod name with
//!   any `/` flattened to `-` per I2 remediation (matches milestone-138
//!   composer convention; avoids `pkg:generic/<namespace>/<name>`
//!   ambiguity per purl-spec base rules).
//!
//! Pod names are CASE-PRESERVED verbatim per purl-spec (CocoaPods is
//! case-sensitive, unlike Composer's lowercase requirement).
//!
//! `SPEC CHECKSUMS:` is ROOT-keyed per Phase 0 correction: subspec
//! components look up SHA-1 by root pod name (all subspecs of a root
//! share the same checksum).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use mikebom_common::resolution::LifecycleScope;
use mikebom_common::types::hash::{ContentHash, HashAlgorithm};
use mikebom_common::types::purl::Purl;

use super::exclude_path::ExclusionSet;
use super::PackageDbEntry;

const MAX_COCOAPODS_WALK_DEPTH: usize = 12;

fn should_skip_manifest_descent(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".svn" | ".hg" | "Pods" | "node_modules" | "build" | "DerivedData"
    )
}

fn should_skip_installed_descent(name: &str) -> bool {
    matches!(name, ".git" | ".svn" | ".hg" | "node_modules" | "DerivedData")
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct PodfileLockDoc {
    #[serde(default, rename = "PODS")]
    pods: Vec<serde_yaml::Value>,
    #[serde(default, rename = "DEPENDENCIES")]
    dependencies: Vec<String>,
    #[serde(default, rename = "EXTERNAL SOURCES")]
    external_sources: BTreeMap<String, serde_yaml::Value>,
    #[serde(default, rename = "CHECKOUT OPTIONS")]
    checkout_options: BTreeMap<String, serde_yaml::Value>,
    #[serde(default, rename = "SPEC CHECKSUMS")]
    spec_checksums: BTreeMap<String, String>,
    #[serde(default, rename = "PODFILE CHECKSUM")]
    podfile_checksum: Option<String>,
    #[serde(default, rename = "COCOAPODS")]
    cocoapods: Option<String>,
}

#[derive(Debug, Clone)]
struct PodsEntry {
    /// Pod name; may contain `/` for subspecs (`Firebase/Core`).
    name: String,
    /// Pinned version (parentheses stripped).
    version: String,
}

impl PodsEntry {
    fn root_pod_name(&self) -> &str {
        self.name.split_once('/').map(|(r, _)| r).unwrap_or(&self.name)
    }

    fn subpath(&self) -> Option<&str> {
        self.name.split_once('/').map(|(_, s)| s)
    }
}

#[derive(Debug, Clone, Default)]
struct PodfileTargetInfo {
    first_target_name: Option<String>,
    declared_pods: Vec<DeclaredPod>,
}

#[derive(Debug, Clone)]
struct DeclaredPod {
    name: String,
    constraint: Option<String>,
}

pub fn read(
    rootfs: &Path,
    _include_dev: bool,
    exclude_set: &ExclusionSet,
) -> Vec<PackageDbEntry> {
    let mut out: Vec<PackageDbEntry> = Vec::new();
    let mut seen_purls: HashSet<String> = HashSet::new();
    // Track parsed Podfile.lock project dirs so Pass B can skip
    // Manifest.lock dedup-conflicts (FR-011) and Pass C can skip
    // design-tier dedup-conflicts.
    let mut lockfile_dirs: HashSet<PathBuf> = HashSet::new();
    let mut warned = 0usize;
    let mut emitted_lockfile = 0usize;
    let mut emitted_manifest = 0usize;
    let mut emitted_design = 0usize;

    // Pass A — Podfile.lock walker.
    for lockfile_path in find_podfile_locks(rootfs, exclude_set) {
        let doc = match parse_podfile_lock(&lockfile_path) {
            Ok(d) => d,
            Err(err) => {
                warned += 1;
                tracing::warn!(
                    path = %lockfile_path.display(),
                    error = %err,
                    "cocoapods: failed to parse Podfile.lock; skipping project",
                );
                continue;
            }
        };
        let Some(project_dir) = lockfile_path.parent() else {
            continue;
        };
        lockfile_dirs.insert(project_dir.to_path_buf());

        // Look for sibling Podfile to derive main-module name.
        let podfile_path = project_dir.join("Podfile");
        let podfile_info = if podfile_path.is_file() {
            parse_podfile(&podfile_path).ok()
        } else {
            None
        };

        if let Some(main_module) = emit_main_module(
            project_dir,
            podfile_path.is_file().then_some(podfile_path.as_path()),
            Some(&lockfile_path),
            Some(&doc),
            podfile_info.as_ref(),
            "source",
        ) {
            let purl_key = main_module.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(main_module);
            }
        }

        if doc.spec_checksums.is_empty() && !doc.pods.is_empty() {
            // Pre-1.0 lockfile detection per I1 remediation —
            // graceful-degrade (still emit components, just without
            // hashes) per Principle VIII Completeness.
            tracing::info!(
                path = %lockfile_path.display(),
                "cocoapods: Podfile.lock lacks SPEC CHECKSUMS section (likely pre-1.0 format); emitting components without SHA-1 hashes",
            );
        }

        let entries = emit_lockfile_components(
            &lockfile_path,
            &doc,
            "source",
            "cocoapods-podfile-lock",
        );
        emitted_lockfile += entries.len();
        for entry in entries {
            let purl_key = entry.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(entry);
            }
        }
    }

    // Pass B — Pods/Manifest.lock walker. Skip when sibling Podfile.lock
    // already parsed (FR-011 dedup).
    for manifest_path in find_manifest_locks(rootfs, exclude_set) {
        // Project root = parent of Pods/ = grandparent of Manifest.lock.
        let project_root = manifest_path
            .parent() // .../Pods
            .and_then(|p| p.parent()) // project root
            .map(|p| p.to_path_buf());
        if let Some(ref root) = project_root {
            if lockfile_dirs.contains(root) {
                continue; // FR-011 dedup
            }
        }

        let doc = match parse_podfile_lock(&manifest_path) {
            Ok(d) => d,
            Err(err) => {
                warned += 1;
                tracing::warn!(
                    path = %manifest_path.display(),
                    error = %err,
                    "cocoapods: failed to parse Pods/Manifest.lock; skipping",
                );
                continue;
            }
        };

        let project_dir_path = project_root.as_deref().unwrap_or(rootfs);
        let podfile_path = project_dir_path.join("Podfile");
        let podfile_info = if podfile_path.is_file() {
            parse_podfile(&podfile_path).ok()
        } else {
            None
        };

        if let Some(main_module) = emit_main_module(
            project_dir_path,
            podfile_path.is_file().then_some(podfile_path.as_path()),
            Some(&manifest_path),
            Some(&doc),
            podfile_info.as_ref(),
            "deployed",
        ) {
            let purl_key = main_module.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(main_module);
            }
        }

        let entries = emit_lockfile_components(
            &manifest_path,
            &doc,
            "deployed",
            "cocoapods-manifest-lock",
        );
        emitted_manifest += entries.len();
        for entry in entries {
            let purl_key = entry.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(entry);
            }
        }
    }

    // Pass C — design-tier (Podfile only, no sibling Podfile.lock).
    for podfile_path in find_podfiles(rootfs, exclude_set) {
        let Some(project_dir) = podfile_path.parent() else {
            continue;
        };
        if lockfile_dirs.contains(project_dir) {
            continue; // lockfile already emitted in Pass A
        }
        let podfile_info = match parse_podfile(&podfile_path) {
            Ok(info) => info,
            Err(err) => {
                warned += 1;
                tracing::warn!(
                    path = %podfile_path.display(),
                    error = %err,
                    "cocoapods: failed to parse Podfile; skipping",
                );
                continue;
            }
        };

        if let Some(main_module) = emit_main_module(
            project_dir,
            Some(&podfile_path),
            None,
            None,
            Some(&podfile_info),
            "design",
        ) {
            let purl_key = main_module.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(main_module);
            }
        }

        let entries = emit_design_tier_components(&podfile_path, &podfile_info);
        emitted_design += entries.len();
        for entry in entries {
            let purl_key = entry.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(entry);
            }
        }
    }

    if !out.is_empty() || warned > 0 {
        tracing::info!(
            rootfs = %rootfs.display(),
            entries = out.len(),
            emitted_lockfile,
            emitted_manifest,
            emitted_design,
            warned,
            "parsed Podfile.lock + Podfile + Manifest.lock entries",
        );
    }
    out
}

fn find_podfile_locks(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_COCOAPODS_WALK_DEPTH,
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
            && path.file_name().and_then(|s| s.to_str()) == Some("Podfile.lock")
        {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

fn find_manifest_locks(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_COCOAPODS_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_installed_descent(name)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if !path.is_file() {
            return;
        }
        if path.file_name().and_then(|s| s.to_str()) != Some("Manifest.lock") {
            return;
        }
        // Canonical `<project>/Pods/Manifest.lock` layout — immediate
        // parent dir is `Pods`.
        let parent_dir = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str());
        if parent_dir == Some("Pods") {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

fn find_podfiles(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_COCOAPODS_WALK_DEPTH,
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
            && path.file_name().and_then(|s| s.to_str()) == Some("Podfile")
        {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

fn parse_podfile_lock(path: &Path) -> anyhow::Result<PodfileLockDoc> {
    let bytes = std::fs::read(path).map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    serde_yaml::from_slice(&bytes).map_err(|e| anyhow::anyhow!("yaml parse failed: {e}"))
}

fn parse_podfile(path: &Path) -> anyhow::Result<PodfileTargetInfo> {
    static TARGET_RE: OnceLock<Regex> = OnceLock::new();
    static POD_RE: OnceLock<Regex> = OnceLock::new();
    let target_re = TARGET_RE.get_or_init(|| {
        Regex::new(r#"^\s*target\s+['"]([^'"]+)['"]\s+do\b"#).expect("static target regex")
    });
    let pod_re = POD_RE.get_or_init(|| {
        Regex::new(r#"^\s*pod\s+['"]([^'"]+)['"](?:\s*,\s*['"]([^'"]+)['"])?"#)
            .expect("static pod regex")
    });

    let text = std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    let mut info = PodfileTargetInfo::default();
    for raw_line in text.lines() {
        // Strip line comments.
        let line = raw_line
            .split_once('#')
            .map(|(s, _)| s)
            .unwrap_or(raw_line)
            .trim_end();

        if info.first_target_name.is_none() {
            if let Some(captures) = target_re.captures(line) {
                if let Some(m) = captures.get(1) {
                    info.first_target_name = Some(m.as_str().to_string());
                }
            }
        }

        if let Some(captures) = pod_re.captures(line) {
            let name = match captures.get(1) {
                Some(m) => m.as_str().to_string(),
                None => continue,
            };
            let constraint = captures.get(2).map(|m| m.as_str().to_string());
            info.declared_pods.push(DeclaredPod { name, constraint });
        }
    }
    Ok(info)
}

fn parse_pods_entry(value: &serde_yaml::Value) -> Option<PodsEntry> {
    match value {
        serde_yaml::Value::String(s) => parse_pod_spec_string(s),
        serde_yaml::Value::Mapping(m) if m.len() == 1 => {
            let key = m.iter().next()?.0;
            let s = key.as_str()?;
            parse_pod_spec_string(s)
        }
        _ => None,
    }
}

fn parse_pod_spec_string(s: &str) -> Option<PodsEntry> {
    // Format: "PodName (version)" — split on " ("  and strip trailing ")".
    let (name, rest) = s.split_once(" (")?;
    let version = rest.strip_suffix(')')?;
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some(PodsEntry {
        name: name.to_string(),
        version: version.to_string(),
    })
}

fn parse_dep_name(dep_string: &str) -> String {
    // Strip trailing parenthesized constraint: "AFNetworking (~> 4.0)" → "AFNetworking".
    let trimmed = dep_string.trim();
    match trimmed.split_once(" (") {
        Some((name, _)) => name.trim().to_string(),
        None => trimmed.to_string(),
    }
}

/// FR-012 + Q1 cascade: derive main-module name from Podfile target
/// block when available, else fall back to project_dir basename.
fn emit_main_module(
    project_dir: &Path,
    podfile_path: Option<&Path>,
    lockfile_path: Option<&Path>,
    doc: Option<&PodfileLockDoc>,
    podfile_info: Option<&PodfileTargetInfo>,
    sbom_tier: &str,
) -> Option<PackageDbEntry> {
    let app_name = podfile_info
        .and_then(|p| p.first_target_name.clone())
        .or_else(|| {
            project_dir
                .file_name()
                .and_then(|s| s.to_str())
                .map(String::from)
        })?;
    if app_name.is_empty() {
        tracing::warn!(
            project_dir = %project_dir.display(),
            "cocoapods: could not derive main-module name (no Podfile target + empty parent-dir basename); skipping",
        );
        return None;
    }
    let purl_str = format!("pkg:cocoapods/{app_name}@0.0.0-unknown");
    let purl = Purl::new(&purl_str).ok()?;

    let source_path = podfile_path
        .map(|p| p.to_string_lossy().into_owned())
        .or_else(|| lockfile_path.map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_else(|| project_dir.to_string_lossy().into_owned());

    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert(
        "mikebom:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );
    extra_annotations.insert(
        "mikebom:source-type".to_string(),
        serde_json::Value::String("cocoapods-main-module".to_string()),
    );

    let depends: Vec<String> = if let Some(d) = doc {
        d.dependencies.iter().map(|s| parse_dep_name(s)).collect()
    } else if let Some(p) = podfile_info {
        p.declared_pods.iter().map(|d| d.name.clone()).collect()
    } else {
        Vec::new()
    };

    Some(PackageDbEntry {
        purl,
        name: app_name,
        version: "0.0.0-unknown".to_string(),
        arch: None,
        source_path,
        depends,
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_range: None,
        source_type: Some("cocoapods-main-module".to_string()),
        buildinfo_status: None,
        sbom_tier: Some(sbom_tier.to_string()),
        evidence_kind: Some("cocoapods-podfile".to_string()),
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

fn emit_lockfile_components(
    source_path_buf: &Path,
    doc: &PodfileLockDoc,
    sbom_tier: &str,
    evidence_kind: &str,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    let source_path = source_path_buf.to_string_lossy().into_owned();
    for value in &doc.pods {
        let Some(entry) = parse_pods_entry(value) else {
            tracing::warn!(
                path = %source_path_buf.display(),
                "cocoapods: skipping malformed PODS entry",
            );
            continue;
        };
        let purl = match build_purl_for_pods_entry(
            &entry,
            &doc.external_sources,
            &doc.checkout_options,
        ) {
            Ok(p) => p,
            Err(err) => {
                tracing::warn!(
                    name = %entry.name,
                    path = %source_path_buf.display(),
                    error = %err,
                    "cocoapods: skipping malformed lockfile entry",
                );
                continue;
            }
        };
        let source_type_value =
            classify_source_type(&entry, &doc.external_sources);
        let extra_annotations = build_extra_annotations(
            &entry,
            source_type_value,
            &doc.external_sources,
            &doc.checkout_options,
        );

        // SHA-1 hash per FR-008: ROOT-keyed SPEC CHECKSUMS lookup;
        // only for trunk pods (git/path entries don't have SPEC
        // CHECKSUMS entries).
        let hashes: Vec<ContentHash> = if source_type_value == "cocoapods-trunk" {
            match doc.spec_checksums.get(entry.root_pod_name()) {
                Some(hex)
                    if hex.len() == 40
                        && hex.chars().all(|c| c.is_ascii_hexdigit()) =>
                {
                    match ContentHash::with_algorithm(HashAlgorithm::Sha1, hex) {
                        Ok(h) => vec![h],
                        Err(_) => Vec::new(),
                    }
                }
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };

        out.push(PackageDbEntry {
            purl,
            name: entry.name.clone(),
            version: entry.version.clone(),
            arch: None,
            source_path: source_path.clone(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: Some(LifecycleScope::Runtime),
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
        });
    }
    out
}

/// FR-005: design-tier emission from Podfile `pod` declarations.
fn emit_design_tier_components(
    podfile_path: &Path,
    info: &PodfileTargetInfo,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    let source_path = podfile_path.to_string_lossy().into_owned();
    let main_module_name = info.first_target_name.as_deref().unwrap_or("");

    for decl in &info.declared_pods {
        if decl.name == main_module_name {
            continue;
        }
        let constraint = decl.constraint.clone().unwrap_or_else(|| "unspecified".to_string());
        let sanitized = sanitize_purl_version(&constraint);
        let purl_str = format!("pkg:cocoapods/{}@{}", decl.name, sanitized);
        let Ok(purl) = Purl::new(&purl_str) else {
            tracing::warn!(
                name = %decl.name,
                constraint = %constraint,
                path = %podfile_path.display(),
                "cocoapods: skipping design-tier entry with non-PURL-safe constraint",
            );
            continue;
        };

        let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        extra_annotations.insert(
            "mikebom:source-type".to_string(),
            serde_json::Value::String("cocoapods-trunk".to_string()),
        );

        out.push(PackageDbEntry {
            purl,
            name: decl.name.clone(),
            version: sanitized.clone(),
            arch: None,
            source_path: source_path.clone(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: Some(LifecycleScope::Runtime),
            requirement_range: Some(decl.constraint.clone().unwrap_or_default()),
            source_type: Some("cocoapods-trunk".to_string()),
            buildinfo_status: None,
            sbom_tier: Some("design".to_string()),
            evidence_kind: Some("cocoapods-podfile".to_string()),
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

fn sanitize_purl_version(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            '/' | '?' | '#' | ' ' => '_',
            other => other,
        })
        .collect()
}

/// PURL qualifier-value encoding per the PURL spec's `pchar` rule.
/// Mirrors the helper in composer.rs / dart.rs. Allows `:` / `/` / `@`
/// / `=` (URL-readable); encodes only the chars that would break PURL
/// parsing.
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

/// FR-003 PURL construction per source type.
fn build_purl_for_pods_entry(
    entry: &PodsEntry,
    external_sources: &BTreeMap<String, serde_yaml::Value>,
    _checkout_options: &BTreeMap<String, serde_yaml::Value>,
) -> Result<Purl, String> {
    if entry.name.is_empty() {
        return Err("empty pod name".to_string());
    }
    if entry.version.is_empty() {
        return Err(format!("pod {} has empty version", entry.name));
    }

    // EXTERNAL SOURCES lookup: try entry.name first, then fall back
    // to root pod (subspecs typically inherit external-source overrides
    // from their root).
    let external = external_sources
        .get(&entry.name)
        .or_else(|| external_sources.get(entry.root_pod_name()));

    let purl_str = if let Some(ext) = external {
        // Path source — flatten `/` to `-` per I2 remediation.
        if lookup_yaml_ruby_symbol(ext, "path").is_some() {
            let flattened = entry.name.replace('/', "-");
            format!("pkg:generic/{flattened}@{}", entry.version)
        } else if let Some(git_value) = lookup_yaml_ruby_symbol(ext, "git") {
            let url = git_value
                .as_str()
                .ok_or_else(|| format!("git url not a string for {}", entry.name))?;
            if url.is_empty() {
                return Err(format!("empty git url for {}", entry.name));
            }
            format!(
                "pkg:cocoapods/{}@{}?vcs_url=git+{}",
                entry.name,
                entry.version,
                minimal_qualifier_encode(url),
            )
        } else {
            // EXTERNAL SOURCES entry exists but is neither :git nor
            // :path (could be :podspec — rare; treat as trunk-ish).
            build_trunk_or_subspec_purl(entry)
        }
    } else {
        build_trunk_or_subspec_purl(entry)
    };

    Purl::new(&purl_str).map_err(|e| format!("PURL construction failed for {purl_str}: {e:?}"))
}

fn build_trunk_or_subspec_purl(entry: &PodsEntry) -> String {
    match entry.subpath() {
        Some(subpath) => format!(
            "pkg:cocoapods/{}@{}#{}",
            entry.root_pod_name(),
            entry.version,
            subpath,
        ),
        None => format!("pkg:cocoapods/{}@{}", entry.name, entry.version),
    }
}

fn lookup_yaml_ruby_symbol<'a>(
    value: &'a serde_yaml::Value,
    key: &str,
) -> Option<&'a serde_yaml::Value> {
    let map = value.as_mapping()?;
    // Try both `:git` (Ruby-symbol-keyed YAML) and `git` (plain string)
    // since serde_yaml rendering varies depending on the CocoaPods
    // version that wrote the lockfile.
    let with_colon = format!(":{key}");
    if let Some(v) = map.get(serde_yaml::Value::String(with_colon)) {
        return Some(v);
    }
    map.get(serde_yaml::Value::String(key.to_string()))
}

fn classify_source_type(
    entry: &PodsEntry,
    external_sources: &BTreeMap<String, serde_yaml::Value>,
) -> &'static str {
    let external = external_sources
        .get(&entry.name)
        .or_else(|| external_sources.get(entry.root_pod_name()));
    match external {
        Some(ext) if lookup_yaml_ruby_symbol(ext, "path").is_some() => "cocoapods-path",
        Some(ext) if lookup_yaml_ruby_symbol(ext, "git").is_some() => "cocoapods-git",
        _ => "cocoapods-trunk",
    }
}

fn build_extra_annotations(
    entry: &PodsEntry,
    source_type_value: &str,
    external_sources: &BTreeMap<String, serde_yaml::Value>,
    checkout_options: &BTreeMap<String, serde_yaml::Value>,
) -> BTreeMap<String, serde_json::Value> {
    let mut out: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    out.insert(
        "mikebom:source-type".to_string(),
        serde_json::Value::String(source_type_value.to_string()),
    );
    let external = external_sources
        .get(&entry.name)
        .or_else(|| external_sources.get(entry.root_pod_name()));

    match source_type_value {
        "cocoapods-trunk" => {
            if let Some(sub) = entry.subpath() {
                out.insert(
                    "mikebom:subspec".to_string(),
                    serde_json::Value::String(sub.to_string()),
                );
            }
        }
        "cocoapods-git" => {
            // Resolved SHA from CHECKOUT OPTIONS per Q2.
            if let Some(co) = checkout_options
                .get(&entry.name)
                .or_else(|| checkout_options.get(entry.root_pod_name()))
            {
                if let Some(commit) = lookup_yaml_ruby_symbol(co, "commit")
                    .and_then(|v| v.as_str())
                {
                    if commit.len() == 40 && commit.chars().all(|c| c.is_ascii_hexdigit()) {
                        out.insert(
                            "mikebom:vcs-ref".to_string(),
                            serde_json::Value::String(commit.to_string()),
                        );
                    }
                }
            }
            // Operator-declared ref from EXTERNAL SOURCES, when distinct
            // from resolved SHA.
            if let Some(ext) = external {
                for ref_key in &["commit", "tag", "branch"] {
                    if let Some(v) = lookup_yaml_ruby_symbol(ext, ref_key)
                        .and_then(|v| v.as_str())
                    {
                        let resolved = out
                            .get("mikebom:vcs-ref")
                            .and_then(|x| x.as_str())
                            .unwrap_or("");
                        if v != resolved {
                            out.insert(
                                "mikebom:vcs-declared-ref".to_string(),
                                serde_json::Value::String(v.to_string()),
                            );
                        }
                        break;
                    }
                }
            }
        }
        "cocoapods-path" => {
            if let Some(ext) = external {
                if let Some(p) = lookup_yaml_ruby_symbol(ext, "path")
                    .and_then(|v| v.as_str())
                {
                    out.insert(
                        "mikebom:path".to_string(),
                        serde_json::Value::String(p.to_string()),
                    );
                }
            }
            // For path-sourced subspecs, preserve the original subspec
            // path for recovery per I2 remediation.
            if let Some(sub) = entry.subpath() {
                out.insert(
                    "mikebom:subspec".to_string(),
                    serde_json::Value::String(sub.to_string()),
                );
            }
        }
        _ => {}
    }
    // Discard unused HashMap import.
    let _ = HashMap::<(), ()>::new;
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn pods_entry(name: &str, version: &str) -> PodsEntry {
        PodsEntry { name: name.into(), version: version.into() }
    }

    #[test]
    fn parse_pod_spec_string_extracts_name_and_version() {
        let p = parse_pod_spec_string("AFNetworking (4.0.1)").unwrap();
        assert_eq!(p.name, "AFNetworking");
        assert_eq!(p.version, "4.0.1");
    }

    #[test]
    fn parse_pod_spec_string_handles_subspec() {
        let p = parse_pod_spec_string("Firebase/Core (10.20.0)").unwrap();
        assert_eq!(p.name, "Firebase/Core");
        assert_eq!(p.version, "10.20.0");
        assert_eq!(p.root_pod_name(), "Firebase");
        assert_eq!(p.subpath(), Some("Core"));
    }

    #[test]
    fn parse_pod_spec_string_multi_level_subspec() {
        let p = parse_pod_spec_string("Firebase/Database/Realtime (10.20.0)").unwrap();
        assert_eq!(p.name, "Firebase/Database/Realtime");
        assert_eq!(p.root_pod_name(), "Firebase");
        assert_eq!(p.subpath(), Some("Database/Realtime"));
    }

    #[test]
    fn parse_pod_spec_string_rejects_malformed() {
        assert!(parse_pod_spec_string("malformed").is_none());
        assert!(parse_pod_spec_string("Name ()").is_none());
        assert!(parse_pod_spec_string(" (1.0)").is_none());
    }

    #[test]
    fn parse_pods_entry_handles_string_form() {
        let v: serde_yaml::Value = serde_yaml::from_str("- 'AFNetworking (4.0.1)'").unwrap();
        let first = v.as_sequence().unwrap().first().unwrap();
        let p = parse_pods_entry(first).unwrap();
        assert_eq!(p.name, "AFNetworking");
    }

    #[test]
    fn parse_pods_entry_handles_map_form() {
        let yaml = "- AFNetworking (4.0.1):\n  - dep1\n  - dep2\n";
        let v: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let first = v.as_sequence().unwrap().first().unwrap();
        let p = parse_pods_entry(first).unwrap();
        assert_eq!(p.name, "AFNetworking");
        assert_eq!(p.version, "4.0.1");
    }

    #[test]
    fn parse_dep_name_strips_constraint() {
        assert_eq!(parse_dep_name("AFNetworking (~> 4.0)"), "AFNetworking");
        assert_eq!(parse_dep_name("Firebase/Core"), "Firebase/Core");
        assert_eq!(parse_dep_name("  Padded  "), "Padded");
    }

    fn empty_map() -> BTreeMap<String, serde_yaml::Value> {
        BTreeMap::new()
    }

    #[test]
    fn build_purl_trunk_default() {
        let e = pods_entry("AFNetworking", "4.0.1");
        let p = build_purl_for_pods_entry(&e, &empty_map(), &empty_map()).unwrap();
        assert_eq!(p.as_str(), "pkg:cocoapods/AFNetworking@4.0.1");
    }

    #[test]
    fn build_purl_subspec_uses_subpath_form() {
        let e = pods_entry("Firebase/Core", "10.20.0");
        let p = build_purl_for_pods_entry(&e, &empty_map(), &empty_map()).unwrap();
        assert_eq!(p.as_str(), "pkg:cocoapods/Firebase@10.20.0#Core");
    }

    #[test]
    fn build_purl_multi_level_subspec_preserves_slashes() {
        let e = pods_entry("Firebase/Database/Realtime", "10.20.0");
        let p = build_purl_for_pods_entry(&e, &empty_map(), &empty_map()).unwrap();
        assert_eq!(p.as_str(), "pkg:cocoapods/Firebase@10.20.0#Database/Realtime");
    }

    #[test]
    fn build_purl_case_preserved() {
        // CocoaPods is case-sensitive per purl-spec; mixed-case names round-trip.
        let e = pods_entry("AFNetworking", "1.0.0");
        let p = build_purl_for_pods_entry(&e, &empty_map(), &empty_map()).unwrap();
        assert_eq!(p.as_str(), "pkg:cocoapods/AFNetworking@1.0.0");
    }

    fn external_with_git(url: &str) -> BTreeMap<String, serde_yaml::Value> {
        let mut external: BTreeMap<String, serde_yaml::Value> = BTreeMap::new();
        let yaml = format!(":git: \"{url}\"\n:branch: main\n");
        let val: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        external.insert("MyFork".to_string(), val);
        external
    }

    fn checkout_with_commit(sha: &str) -> BTreeMap<String, serde_yaml::Value> {
        let mut co: BTreeMap<String, serde_yaml::Value> = BTreeMap::new();
        let yaml = format!(":commit: \"{sha}\"\n");
        let val: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        co.insert("MyFork".to_string(), val);
        co
    }

    #[test]
    fn build_purl_git_source_emits_vcs_url_qualifier() {
        let e = pods_entry("MyFork", "1.5.0");
        let external = external_with_git("https://github.com/foo/my-fork.git");
        let p = build_purl_for_pods_entry(&e, &external, &empty_map()).unwrap();
        assert_eq!(
            p.as_str(),
            "pkg:cocoapods/MyFork@1.5.0?vcs_url=git+https://github.com/foo/my-fork.git"
        );
    }

    #[test]
    fn build_purl_path_source_flattens_subspec_slash_to_hyphen() {
        // I2 remediation regression: path-sourced subspec MUST flatten
        // `/` to `-` to avoid pkg:generic/<namespace>/<name> ambiguity.
        let e = pods_entry("Firebase/Core", "10.20.0");
        let mut external: BTreeMap<String, serde_yaml::Value> = BTreeMap::new();
        let yaml = ":path: \"../firebase-core\"\n";
        let val: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        external.insert("Firebase/Core".to_string(), val);
        let p = build_purl_for_pods_entry(&e, &external, &empty_map()).unwrap();
        assert_eq!(p.as_str(), "pkg:generic/Firebase-Core@10.20.0");
    }

    #[test]
    fn build_purl_path_source_no_subspec() {
        let e = pods_entry("LocalLib", "0.1.0");
        let mut external: BTreeMap<String, serde_yaml::Value> = BTreeMap::new();
        let yaml = ":path: \"../local\"\n";
        let val: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        external.insert("LocalLib".to_string(), val);
        let p = build_purl_for_pods_entry(&e, &external, &empty_map()).unwrap();
        assert_eq!(p.as_str(), "pkg:generic/LocalLib@0.1.0");
    }

    #[test]
    fn classify_source_type_trunk_default() {
        let e = pods_entry("AFNetworking", "4.0.1");
        assert_eq!(classify_source_type(&e, &empty_map()), "cocoapods-trunk");
    }

    #[test]
    fn classify_source_type_git_via_external() {
        let e = pods_entry("MyFork", "1.5.0");
        let external = external_with_git("https://example.com/r.git");
        assert_eq!(classify_source_type(&e, &external), "cocoapods-git");
    }

    #[test]
    fn classify_source_type_path() {
        let e = pods_entry("LocalLib", "0.1.0");
        let mut external: BTreeMap<String, serde_yaml::Value> = BTreeMap::new();
        let val: serde_yaml::Value = serde_yaml::from_str(":path: \"../l\"\n").unwrap();
        external.insert("LocalLib".to_string(), val);
        assert_eq!(classify_source_type(&e, &external), "cocoapods-path");
    }

    #[test]
    fn build_extra_annotations_trunk_subspec_carries_subspec_annotation() {
        let e = pods_entry("Firebase/Core", "10.20.0");
        let ann = build_extra_annotations(&e, "cocoapods-trunk", &empty_map(), &empty_map());
        assert_eq!(
            ann.get("mikebom:subspec").and_then(|v| v.as_str()),
            Some("Core")
        );
    }

    #[test]
    fn build_extra_annotations_git_carries_vcs_ref_and_declared_ref() {
        let e = pods_entry("MyFork", "1.5.0");
        let external = external_with_git("https://example.com/r.git");
        let sha = "eb39649a76b87e8451baf75d10ce82ca3a3d5601";
        let checkout = checkout_with_commit(sha);
        let ann = build_extra_annotations(&e, "cocoapods-git", &external, &checkout);
        assert_eq!(
            ann.get("mikebom:vcs-ref").and_then(|v| v.as_str()),
            Some(sha)
        );
        assert_eq!(
            ann.get("mikebom:vcs-declared-ref").and_then(|v| v.as_str()),
            Some("main")
        );
    }

    #[test]
    fn build_extra_annotations_path_carries_path_and_subspec() {
        let e = pods_entry("Firebase/Core", "10.20.0");
        let mut external: BTreeMap<String, serde_yaml::Value> = BTreeMap::new();
        let val: serde_yaml::Value = serde_yaml::from_str(":path: \"../firebase-core\"\n").unwrap();
        external.insert("Firebase/Core".to_string(), val);
        let ann = build_extra_annotations(&e, "cocoapods-path", &external, &empty_map());
        assert_eq!(
            ann.get("mikebom:path").and_then(|v| v.as_str()),
            Some("../firebase-core")
        );
        assert_eq!(
            ann.get("mikebom:subspec").and_then(|v| v.as_str()),
            Some("Core")
        );
    }

    #[test]
    fn lookup_yaml_ruby_symbol_handles_both_key_styles() {
        let yaml_symbol: serde_yaml::Value = serde_yaml::from_str(":git: \"https://x\"\n").unwrap();
        assert!(lookup_yaml_ruby_symbol(&yaml_symbol, "git").is_some());

        let yaml_plain: serde_yaml::Value = serde_yaml::from_str("git: \"https://x\"\n").unwrap();
        assert!(lookup_yaml_ruby_symbol(&yaml_plain, "git").is_some());
    }

    #[test]
    fn parse_podfile_extracts_target_and_pods() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Podfile");
        std::fs::write(
            &path,
            "target 'MyApp' do\n  pod 'AFNetworking', '~> 4.0'\n  pod 'SDWebImage'\nend\n",
        )
        .unwrap();
        let info = parse_podfile(&path).unwrap();
        assert_eq!(info.first_target_name.as_deref(), Some("MyApp"));
        assert_eq!(info.declared_pods.len(), 2);
        assert_eq!(info.declared_pods[0].name, "AFNetworking");
        assert_eq!(info.declared_pods[0].constraint.as_deref(), Some("~> 4.0"));
        assert_eq!(info.declared_pods[1].name, "SDWebImage");
        assert!(info.declared_pods[1].constraint.is_none());
    }

    #[test]
    fn parse_podfile_handles_double_quotes() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Podfile");
        std::fs::write(&path, "target \"MyApp\" do\n  pod \"AFNetworking\"\nend\n").unwrap();
        let info = parse_podfile(&path).unwrap();
        assert_eq!(info.first_target_name.as_deref(), Some("MyApp"));
        assert_eq!(info.declared_pods.len(), 1);
    }

    #[test]
    fn parse_podfile_strips_comments() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Podfile");
        std::fs::write(
            &path,
            "# Top-level comment\ntarget 'MyApp' do  # trailing comment\n  pod 'AFNetworking'\n# pod 'Commented'\nend\n",
        )
        .unwrap();
        let info = parse_podfile(&path).unwrap();
        assert_eq!(info.first_target_name.as_deref(), Some("MyApp"));
        assert_eq!(info.declared_pods.len(), 1);
    }

    #[test]
    fn sanitize_purl_version_neutralizes_unsafe_chars() {
        assert_eq!(sanitize_purl_version("~> 4.0"), "~>_4.0");
        assert_eq!(sanitize_purl_version(">= 1.0 <2.0"), ">=_1.0_<2.0");
    }
}
