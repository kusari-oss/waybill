//! node_modules flat walker + root package.json reader + npm-source classifier.

use std::path::{Path, PathBuf};


use super::super::PackageDbEntry;
use super::build_npm_purl;
use super::enrich::extract_author_string;

pub(super) fn read_node_modules(
    rootfs: &Path,
    scan_mode: crate::scan_fs::ScanMode,
) -> Option<Vec<PackageDbEntry>> {
    let nm = rootfs.join("node_modules");
    if !nm.is_dir() {
        return None;
    }
    let mut out = Vec::new();
    walk_node_modules(&nm, &mut out, scan_mode, false);
    if out.is_empty() { None } else { Some(out) }
}

/// Feature 005 US1 — detect paths inside npm's own internal package
/// tree (`**/node_modules/npm/node_modules/**`). When a component's
/// source path matches this glob, npm itself is the owner — not the
/// application being scanned.
///
/// Match rule: the path must contain the component sequence
/// `node_modules` → `npm` → `node_modules` anywhere, with `npm` as a
/// directory whose immediate parent is named `node_modules`. This is
/// the canonical layout npm v7+ installs.
///
/// Currently only exercised by unit tests; the npm walker handles the
/// internal-path filter inline today. Kept for the test surface.
#[allow(dead_code)]
pub(crate) fn is_npm_internal_path(path: &Path) -> bool {
    let comps: Vec<&str> = path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();
    // Find any window [a, b, c] where a == "node_modules" && b == "npm" && c == "node_modules".
    comps
        .windows(3)
        .any(|w| w[0] == "node_modules" && w[1] == "npm" && w[2] == "node_modules")
}

// SAFETY (milestone-054 walker audit): structural recursion
// bounded by node_modules/<pkg>/[node_modules/]/<pkg>/... layout —
// the walker only descends into `<pkg>/node_modules/` (line ~160)
// and `@scope/<pkg>` directories (line ~80), not arbitrary
// subdirectories. A symlink loop would have to bypass the
// `pkg_json = child.join("package.json")` existence check at line
// ~86, which any plausible loop fixture would fail. Adding an
// explicit canonicalize-keyed visited-set is tracked in #108
// alongside the broader walker-migration. Per FR-001 audit rubric
// option (b): structural-bounded-by-construction.
fn walk_node_modules(
    nm: &Path,
    out: &mut Vec<PackageDbEntry>,
    scan_mode: crate::scan_fs::ScanMode,
    in_npm_internals: bool,
) {
    let Ok(rd) = std::fs::read_dir(nm) else { return };
    let mut children: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    children.sort();
    let parent_name = nm.file_name().and_then(|s| s.to_str()).unwrap_or("");
    for child in children {
        let name_os = child.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name_os.starts_with('.') {
            continue;
        }
        // Feature 005 US1: an `npm` directory whose parent is
        // `node_modules` is the root of npm's own bundled package tree.
        // In --path mode the operator is scanning an application tree, so
        // its own tooling is out of scope — skip entirely. In --image
        // mode the target is the whole filesystem, so we emit the
        // internals but tag each with `npm_role=internal` so downstream
        // consumers can filter or classify them.
        let is_npm_self_root = parent_name == "node_modules" && name_os == "npm";
        if is_npm_self_root && scan_mode == crate::scan_fs::ScanMode::Path {
            continue;
        }
        if name_os.starts_with('@') {
            // Scoped directory — recurse one level to find the actual
            // packages under it. Propagates `in_npm_internals` so scoped
            // deps under npm's own tree stay tagged.
            walk_node_modules(&child, out, scan_mode, in_npm_internals);
            continue;
        }
        if !child.is_dir() {
            continue;
        }
        let pkg_json = child.join("package.json");
        let Ok(text) = std::fs::read_to_string(&pkg_json) else { continue };
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(name) = parsed.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(version) = parsed.get("version").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(purl) = build_npm_purl(name, version) else { continue };
        let license = parsed
            .get("license")
            .and_then(|v| v.as_str())
            .and_then(|s| {
                mikebom_common::types::license::SpdxExpression::try_canonical(s.trim()).ok()
            })
            .into_iter()
            .collect();
        // Walk all four standard npm dep sections — Tier B's
        // installed-tree walker uses the dep's OWN package.json as
        // the source of truth, and packages declared via
        // peer/optional sections must contribute incoming edges
        // just like regular dependencies. BTreeSet for dedup +
        // deterministic ordering.
        let mut depends_set: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        // Skip `peerDependencies` — declarative, not an install
        // relationship. See package_lock.rs for the full rationale.
        for section in &[
            "dependencies",
            "devDependencies",
            "optionalDependencies",
        ] {
            if let Some(obj) = parsed.get(*section).and_then(|v| v.as_object()) {
                for key in obj.keys() {
                    depends_set.insert(key.clone());
                }
            }
        }
        let depends: Vec<String> = depends_set.into_iter().collect();
        let maintainer = extract_author_string(&parsed);
        // Feature 005 US1: tag entries emitted from inside npm's own
        // bundled tree with `npm_role=internal`. `in_npm_internals` is
        // set by the caller for everything under the `npm` self-root;
        // `is_npm_self_root` catches the npm package itself on the
        // entry it emits directly (the `package.json` at the root).
        let npm_role = if in_npm_internals || is_npm_self_root {
            Some("internal".to_string())
        } else {
            None
        };
        out.push(PackageDbEntry {
            purl,
            name: name.to_string(),
            version: version.to_string(),
            arch: None,
            source_path: pkg_json.to_string_lossy().into_owned(),
            depends,
            maintainer,
            licenses: license,
            lifecycle_scope: None, // flat walk can't recover dev scope
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
            npm_role,
            co_owned_by: None,
            hashes: Vec::new(),
            sbom_tier: Some("deployed".to_string()),
            shade_relocation: None,
            extra_annotations: Default::default(),
            binary_role: None,
        });

        // Feature 005 US1: in --image mode, after emitting the `npm`
        // package itself, also descend into its private `node_modules/`
        // to surface the bundled dep graph (~200 entries on a typical
        // node base image). Those entries inherit `in_npm_internals =
        // true` so they get tagged correctly.
        if is_npm_self_root && scan_mode == crate::scan_fs::ScanMode::Image {
            let nested = child.join("node_modules");
            if nested.is_dir() {
                walk_node_modules(&nested, out, scan_mode, true);
            }
        }
    }
}

/// Overlay `maintainer` on lockfile-derived entries by reading the
/// corresponding installed `package.json` under the project's
/// `node_modules/`. Silent no-op when the tree isn't present; only
pub(super) fn read_root_package_json(rootfs: &Path, include_dev: bool) -> Option<Vec<PackageDbEntry>> {
    let path = rootfs.join("package.json");
    if !path.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&text).ok()?;
    let source_path = path.to_string_lossy().into_owned();
    let out = parse_root_package_json(&parsed, &source_path, include_dev);
    if out.is_empty() { None } else { Some(out) }
}

/// Parse `dependencies` (always) + `devDependencies` (when include_dev).
/// Each key becomes a design-tier component with the range spec in
/// `requirement_range` and `source_type` set for non-registry sources.
pub(crate) fn parse_root_package_json(
    root: &serde_json::Value,
    source_path: &str,
    include_dev: bool,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    for (section, is_dev) in [("dependencies", false), ("devDependencies", true)] {
        if is_dev && !include_dev {
            continue;
        }
        let Some(obj) = root.get(section).and_then(|v| v.as_object()) else {
            continue;
        };
        let mut names: Vec<&String> = obj.keys().collect();
        names.sort();
        for name in names {
            let range = obj[name].as_str().unwrap_or("").to_string();
            let source_type = classify_npm_source(&range);
            // Empty version for range-specs (spec FR-007a).
            let Some(purl) = build_npm_purl(name, "") else {
                continue;
            };
            out.push(PackageDbEntry {
                purl,
                name: name.to_string(),
                version: String::new(),
                arch: None,
                source_path: source_path.to_string(),
                depends: Vec::new(),
                maintainer: None,
                licenses: Vec::new(),
                lifecycle_scope: if is_dev { Some(mikebom_common::resolution::LifecycleScope::Development) } else { Some(mikebom_common::resolution::LifecycleScope::Runtime) },
                requirement_range: Some(range),
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
                sbom_tier: Some("design".to_string()),
                shade_relocation: None,
                extra_annotations: Default::default(),
                binary_role: None,
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Milestone 066 — npm source-tree main-module component
// ---------------------------------------------------------------------------

/// Record describing a duplicate main-module dropped during dedup,
/// returned in batch from `dedup_npm_main_modules_by_purl` for
/// caller-side `tracing::warn!` emission. Mirrors cargo milestone 064.
#[derive(Debug, Clone)]
pub(crate) struct DroppedDuplicate {
    pub purl: String,
    pub kept_path: String,
    pub dropped_path: String,
}

/// Build the npm main-module entry for a single `package.json`.
///
/// Returns `None` when:
/// - The manifest has no `name` field (FR-001).
/// - The manifest has `private: true` AND no `version` field (per
///   issue #104's explicit guidance — the author has signaled "not
///   a publishable artifact").
///
/// Otherwise emits a `PackageDbEntry` with:
/// - PURL `pkg:npm/<name>@<version>` (or `pkg:npm/%40<scope>/<name>@<version>`
///   for scoped names) via `build_npm_purl` which already handles
///   PURL scope encoding.
/// - `version` is the literal `package.json#version` if declared,
///   else the literal `0.0.0-unknown` placeholder per spec Q1
///   (matches cargo's milestone-064 ladder behavior).
/// - `parent_purl: None` (top-level — FR-001a).
/// - `sbom_tier: Some("source")` (FR-006).
/// - `extra_annotations` carries `mikebom:component-role: main-module`
///   (C40, FR-004).
/// - `licenses: vec![]` (FR-005; license detection is #103 follow-up).
/// - `depends` populated from `dependencies`/`devDependencies`/
///   `peerDependencies`/`optionalDependencies` keys (FR-007).
pub(crate) fn build_npm_main_module_entry(
    project_root: &Path,
) -> Option<PackageDbEntry> {
    let manifest_path = project_root.join("package.json");
    let text = std::fs::read_to_string(&manifest_path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&text).ok()?;
    let name = parsed.get("name").and_then(|v| v.as_str())?;
    let version_field = parsed.get("version").and_then(|v| v.as_str());
    let is_private = parsed
        .get("private")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // FR-001 + #104: skip when author has signaled "not a publishable
    // artifact" via private: true + no version. Workspace roots
    // commonly use this pattern; their members emit per-member
    // main-modules separately via FR-002.
    if is_private && version_field.is_none() {
        return None;
    }
    // FR-001 + spec Q1: literal version → use it; missing → placeholder.
    // npm has no equivalent of cargo's `version.workspace = true`
    // inheritance, so the resolution ladder is two-step (literal →
    // placeholder) instead of cargo's three-step.
    let version = version_field.unwrap_or("0.0.0-unknown");
    let purl = build_npm_purl(name, version)?;
    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "mikebom:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );
    let source_path = format!("path+file://{}", project_root.display());
    // Collect direct-dep names from the four npm dep sections per
    // FR-007. If a `package-lock.json` is present alongside the
    // manifest, version-pin each dep via the same walk-up
    // resolution `parse_package_lock` uses — this ensures the
    // root's bare-name dep strings like "@eslint/js" don't fall
    // through to the edge resolver's last-write-wins lookup (which
    // produces the wrong version when multiple installs of the
    // same package exist, e.g. a hoisted v9.21.0 alongside a
    // nested v8.57.1 under eslint@8). The walk-up starts at empty
    // prefix (representing the project root), so the first
    // successful lookup is the hoisted `node_modules/<dep>` —
    // which IS what the root sees per npm's resolver algorithm.
    // Skip `peerDependencies` — declarative, not install-relational.
    // Even for the root project, peer deps express an EXPECTATION
    // about the consumer (which is the user themselves at this
    // level — meaningless to express as an install edge).
    let mut dep_names: Vec<String> = Vec::new();
    for section in [
        "dependencies",
        "devDependencies",
        "optionalDependencies",
    ] {
        if let Some(obj) = parsed.get(section).and_then(|v| v.as_object()) {
            for key in obj.keys() {
                dep_names.push(key.clone());
            }
        }
    }
    // Pre-build the lockfile path_versions index (cheap: parses the
    // lockfile once; only fires when a lockfile is present
    // alongside the manifest, which is the typical npm project
    // layout). Falls back to bare-name emission when no lockfile
    // exists (library packages without committed lockfiles).
    let path_versions: std::collections::HashMap<String, String> = project_root
        .join("package-lock.json")
        .canonicalize()
        .ok()
        .and_then(|p| std::fs::read_to_string(&p).ok())
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|root| {
            root.get("packages")
                .and_then(|v| v.as_object())
                .map(|packages| {
                    let mut out: std::collections::HashMap<String, String> =
                        std::collections::HashMap::with_capacity(packages.len());
                    for (k, v) in packages {
                        if k.is_empty() {
                            continue;
                        }
                        if let Some(tbl) = v.as_object() {
                            if tbl.get("link").and_then(|v| v.as_bool()) == Some(true) {
                                continue;
                            }
                            if let Some(version) =
                                tbl.get("version").and_then(|v| v.as_str())
                            {
                                if !version.is_empty() {
                                    out.insert(k.clone(), version.to_string());
                                }
                            }
                        }
                    }
                    out
                })
        })
        .unwrap_or_default();
    let depends: Vec<String> = dep_names
        .into_iter()
        .map(|dep_name| {
            // Walk up from the empty prefix (root). The first
            // successful lookup is the hoisted `node_modules/<dep>`.
            let mut prefix: &str = "";
            loop {
                let candidate = format!("{prefix}/node_modules/{dep_name}");
                let candidate = candidate.trim_start_matches('/');
                if let Some(version) = path_versions.get(candidate) {
                    return format!("{dep_name} {version}");
                }
                if let Some(idx) = prefix.rfind("/node_modules/") {
                    prefix = &prefix[..idx];
                } else {
                    // Reached the top — no install found anywhere.
                    return dep_name;
                }
            }
        })
        .collect();
    Some(PackageDbEntry {
        purl,
        name: name.to_string(),
        version: version.to_string(),
        arch: None,
        source_path,
        depends,
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
        extra_annotations,
        binary_role: None,
    })
}

/// Dedup main-module entries by PURL, preserving the first occurrence
/// (deterministic on the existing alphabetical walker order). Returns
/// the list of dropped duplicates for caller-side `tracing::warn!`
/// emission. Predicate is C40-tag-driven; non-main-module entries
/// are untouched.
///
/// Mirrors `cargo::dedup_main_modules_by_purl` from milestone 064.
pub(crate) fn dedup_npm_main_modules_by_purl(
    entries: &mut Vec<PackageDbEntry>,
) -> Vec<DroppedDuplicate> {
    let mut dropped: Vec<DroppedDuplicate> = Vec::new();
    let mut seen: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
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

fn classify_npm_source(range: &str) -> Option<String> {
    if range.starts_with("file:") || range.starts_with('.') || range.starts_with('/') {
        Some("local".to_string())
    } else if range.starts_with("git+") || range.starts_with("git://") {
        Some("git".to_string())
    } else if range.starts_with("http://") || range.starts_with("https://") {
        Some("url".to_string())
    } else {
        None
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    #[test]
    fn root_pkgjson_fallback_emits_design_tier_deps_only_by_default() {
        let src = serde_json::json!({
            "name": "myapp",
            "version": "0.1.0",
            "dependencies": { "requests": "^1.0", "foo": "*" },
            "devDependencies": { "jest": "^29.0" }
        });
        let out = parse_root_package_json(&src, "/package.json", false);
        assert_eq!(out.len(), 2);
        for c in &out {
            assert_eq!(c.sbom_tier.as_deref(), Some("design"));
            assert!(c.requirement_range.is_some());
            assert!(c.version.is_empty());
        }
    }

    #[test]
    fn root_pkgjson_fallback_include_dev_adds_devdeps() {
        let src = serde_json::json!({
            "dependencies": { "foo": "^1.0" },
            "devDependencies": { "jest": "^29.0" }
        });
        let out = parse_root_package_json(&src, "/package.json", true);
        assert_eq!(out.len(), 2);
        let jest = out.iter().find(|c| c.name == "jest").unwrap();
        assert_eq!(jest.lifecycle_scope, Some(mikebom_common::resolution::LifecycleScope::Development));
    }

    #[test]
    fn root_pkgjson_classifies_non_registry_sources() {
        let src = serde_json::json!({
            "dependencies": {
                "local-pkg": "file:./lib",
                "git-pkg": "git+https://github.com/foo/bar.git",
                "url-pkg": "https://example.com/pkg.tgz",
                "registry-pkg": "^1.0.0"
            }
        });
        let out = parse_root_package_json(&src, "/package.json", false);
        let source_types: std::collections::HashMap<String, Option<String>> = out
            .into_iter()
            .map(|c| (c.name, c.source_type))
            .collect();
        assert_eq!(source_types["local-pkg"].as_deref(), Some("local"));
        assert_eq!(source_types["git-pkg"].as_deref(), Some("git"));
        assert_eq!(source_types["url-pkg"].as_deref(), Some("url"));
        assert!(source_types["registry-pkg"].is_none());
    }

    #[test]
    fn is_npm_internal_path_matches_canonical_glob() {
        use std::path::Path;
        // T017 — the canonical npm v7+ bundled tree layout. All these
        // paths contain a `node_modules → npm → node_modules` segment
        // run, which is the shape `is_npm_internal_path` matches.
        let cases_true: &[&str] = &[
            "usr/lib/node_modules/npm/node_modules/foo",
            "usr/local/lib/node_modules/npm/node_modules/@scope/bar",
            "opt/node/lib/node_modules/npm/node_modules/baz",
            // Nested — npm vendored inside an app's own tree (rare but
            // happens when a bundler ships a self-contained CLI).
            "app/node_modules/foo/node_modules/npm/node_modules/inner",
        ];
        for p in cases_true {
            assert!(
                is_npm_internal_path(Path::new(p)),
                "expected true for {p}"
            );
        }
        // README-style files directly under node_modules/npm (not inside
        // a further node_modules segment) are NOT internals — they're
        // metadata on the npm package itself.
        assert!(!is_npm_internal_path(Path::new("node_modules/npm/README.md")));
    }

    #[test]
    fn is_npm_internal_path_rejects_false_positives() {
        use std::path::Path;
        // T018 — paths that LOOK similar but don't match the required
        // three-segment `node_modules → npm → node_modules` sequence.
        let cases_false: &[&str] = &[
            "some/node_modules/foo",
            "etc/node_modules/something",
            // Directory name must be EXACTLY `npm`, not `npm-stuff` etc.
            "foo/npm-stuff/node_modules/bar",
            // `npm` directly under a non-`node_modules` parent isn't
            // the self-root.
            "usr/share/npm/node_modules/foo",
        ];
        for p in cases_false {
            assert!(
                !is_npm_internal_path(Path::new(p)),
                "expected false for {p}"
            );
        }
    }
}
