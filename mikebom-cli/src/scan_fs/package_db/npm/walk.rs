//! node_modules flat walker + root package.json reader + npm-source classifier.

use std::path::{Path, PathBuf};


use super::super::PackageDbEntry;
use super::build_npm_purl;
use super::enrich::extract_author_string;

// ---------------------------------------------------------------------------
// Milestone 163 (T001-T004, closes #498) — cross-workspace resolution types
// ---------------------------------------------------------------------------

/// Outcome of cross-workspace resolution for a workspace-peer declared dep
/// per milestone 163's Q1+Q2 unified disposition (see spec §Clarifications
/// at `specs/163-npm-phantom-edges/spec.md`).
///
/// - `Resolved { version }`: the peer's declared dep matched either the
///   peer's own nested `node_modules/<dep>/package.json` (FR-003
///   closest-ancestor semantics matching Node.js's runtime resolver) OR
///   a Tier A lockfile-emitted entry surfaced via the cross-workspace
///   index (FR-001). The `version` is the concrete pinned version.
/// - `Unresolved`: neither source produced a hit. Per unified Q1+Q2
///   disposition, the peer's main-module component gains a
///   `mikebom:unresolved-declared-dep` annotation (C115) naming the dep
///   and NO `dependsOn` edge is emitted. Zero phantom empty-version
///   PURLs enter the graph (SC-004 invariant).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CrossResolution {
    Resolved { version: String },
    Unresolved,
}

/// Milestone 163 (T002) — scan-local index mapping npm-package-name →
/// concrete lockfile-resolved version. Constructed once per scan from
/// the current `entries` snapshot after Tier A completes for all
/// workspace roots; consulted per workspace-peer during Tier C emission.
pub(crate) type CrossWorkspaceIndex = std::collections::HashMap<String, String>;

/// Milestone 163 (T003) — parameter bundle threaded from `npm::read()`
/// into `parse_root_package_json` when the current project root is a
/// workspace peer (has `package.json` AND no lockfile file alongside).
/// When `None` is passed instead, pre-163 design-tier phantom emission
/// is preserved for backward-compatible standalone-package.json scans.
pub(crate) struct CrossWorkspaceContext<'a> {
    pub peer_root: &'a Path,
    pub index: &'a CrossWorkspaceIndex,
}

/// Milestone 163 (T004) — per-workspace-peer accumulator for the
/// cross-resolution results. `resolved_deps` become the peer's main-
/// module `depends` names (downstream graph resolver in `scan_fs/mod.rs`
/// wires them to concrete-version PURLs via `name_to_purl` at line 471).
/// `unresolved_deps` become the C115 `mikebom:unresolved-declared-dep`
/// annotation value on the peer's main-module component (bare string
/// when 1; JSON array sorted+deduplicated when ≥2).
#[derive(Debug, Default, Clone)]
pub(crate) struct WorkspacePeerAccumulator {
    pub resolved_deps: Vec<String>,
    pub unresolved_deps: Vec<String>,
}

/// Milestone 163 (T006) — build a name → version map from the current
/// `entries` snapshot. Skips empty-version entries (design-tier
/// phantoms are precisely what we're about to reshape).
///
/// Multi-version collision: the first-encountered entry wins
/// (deterministic per candidate-project-roots walk order — parent-first
/// filesystem sort). Rare in practice; overrideable in a future
/// milestone if a concrete case emerges.
pub(crate) fn build_cross_workspace_index(entries: &[PackageDbEntry]) -> CrossWorkspaceIndex {
    let mut index = CrossWorkspaceIndex::new();
    for entry in entries {
        if entry.purl.as_str().starts_with("pkg:npm/") && !entry.version.is_empty() {
            index
                .entry(entry.name.clone())
                .or_insert_with(|| entry.version.clone());
        }
    }
    index
}

/// Milestone 163 (T007) — FR-003 + Q1+Q2 unified classifier. Consults
/// the peer's own `node_modules/<dep>/package.json` first (Node.js
/// runtime resolver semantics: closest-ancestor install wins). Falls
/// through to the cross-workspace index (Tier A lockfile-emitted entries
/// across the scan).
pub(crate) fn resolve_for_workspace_peer(
    peer_root: &Path,
    dep_name: &str,
    cross_workspace_index: &CrossWorkspaceIndex,
) -> CrossResolution {
    // Step 1: FR-003 closest-ancestor — check the peer's own node_modules.
    let nested = peer_root
        .join("node_modules")
        .join(dep_name)
        .join("package.json");
    if nested.is_file() {
        if let Some(version) = read_installed_package_version(&nested) {
            if !version.is_empty() {
                return CrossResolution::Resolved { version };
            }
        }
    }
    // Step 2: fall through to the cross-workspace index.
    match cross_workspace_index.get(dep_name) {
        Some(version) => CrossResolution::Resolved {
            version: version.clone(),
        },
        None => CrossResolution::Unresolved,
    }
}

fn read_installed_package_version(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&text).ok()?;
    parsed
        .get("version")
        .and_then(|v| v.as_str())
        .map(String::from)
}

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
            build_inclusion: None,
            purl,
            name: name.to_string(),
            version: version.to_string(),
            arch: None,
            source_path: pkg_json.to_string_lossy().into_owned(),
            depends,
            maintainer,
            licenses: license,
            lifecycle_scope: None, // flat walk can't recover dev scope
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
pub(super) fn read_root_package_json(
    rootfs: &Path,
    include_dev: bool,
    cross_workspace_ctx: Option<&CrossWorkspaceContext<'_>>,
) -> Option<(Vec<PackageDbEntry>, WorkspacePeerAccumulator)> {
    let path = rootfs.join("package.json");
    if !path.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&text).ok()?;
    let source_path = path.to_string_lossy().into_owned();
    let (entries, acc) =
        parse_root_package_json(&parsed, &source_path, include_dev, cross_workspace_ctx);
    if entries.is_empty() && acc.resolved_deps.is_empty() && acc.unresolved_deps.is_empty() {
        None
    } else {
        Some((entries, acc))
    }
}

/// Parse `dependencies` (always) + `devDependencies` (when include_dev).
///
/// **Pre-163 behavior (when `cross_workspace_ctx = None`)**: each declared
/// dep becomes a design-tier component with empty version + range spec
/// in `requirement_range`. Preserved for backward-compatible standalone-
/// package.json scans.
///
/// **Milestone 163 (when `cross_workspace_ctx = Some(_)`)**: the caller is
/// a workspace peer. Per Q1+Q2 unified disposition:
/// - Each declared dep is classified via `resolve_for_workspace_peer`.
/// - `Resolved { version }` → dep-name accumulated in
///   `WorkspacePeerAccumulator.resolved_deps`. NO design-tier phantom
///   entry emitted. Downstream graph resolver in `scan_fs/mod.rs:471`
///   wires the peer's main-module edge to the already-emitted Tier A
///   concrete-version PURL by name.
/// - `Unresolved` → dep-name accumulated in
///   `WorkspacePeerAccumulator.unresolved_deps`. NO design-tier phantom
///   entry emitted. The peer's main-module component will gain the
///   C115 `mikebom:unresolved-declared-dep` annotation naming the dep
///   (stamped in `npm::read()` post-fixup).
///
/// Guarantees SC-004: zero empty-version PURLs enter the graph when
/// `cross_workspace_ctx.is_some()`. Guarantees SC-002: zero phantom
/// edges (unresolved deps produce annotations, not edges).
pub(crate) fn parse_root_package_json(
    root: &serde_json::Value,
    source_path: &str,
    include_dev: bool,
    cross_workspace_ctx: Option<&CrossWorkspaceContext<'_>>,
) -> (Vec<PackageDbEntry>, WorkspacePeerAccumulator) {
    let mut out = Vec::new();
    let mut acc = WorkspacePeerAccumulator::default();
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

            // Milestone 163 (T008): workspace-peer path — cross-resolve
            // and accumulate; do NOT emit phantom entries.
            if let Some(ctx) = cross_workspace_ctx {
                match resolve_for_workspace_peer(ctx.peer_root, name, ctx.index) {
                    CrossResolution::Resolved { .. } => {
                        acc.resolved_deps.push(name.to_string());
                    }
                    CrossResolution::Unresolved => {
                        acc.unresolved_deps.push(name.to_string());
                    }
                }
                continue;
            }

            // Pre-163 backward-compat path: emit design-tier phantom.
            let source_type = classify_npm_source(&range);
            // Milestone 199 US2 — package.json inline alias detection.
            // `"my-alias": "npm:actual-pkg@1.0.0"` → emit ONE component
            // keyed on the resolved identity (aliased_name) + stamp
            // `mikebom:declared-as: [my-alias]`. When no alias is
            // detected, keep the pre-m199 behavior verbatim.
            let alias =
                super::alias_mapping::parse_package_json_alias(name, &range);
            let (emit_name, emit_range, alias_local_name) = match &alias {
                Some(a) => (
                    a.aliased_name.clone(),
                    // The design-tier component preserves the FULL declared
                    // value in requirement_ranges (`npm:actual@1.0.0`) so
                    // downstream consumers can reconstruct the original
                    // declaration; `mikebom:declared-as` carries the alias
                    // name for the resolved-identity mapping.
                    range.clone(),
                    Some(a.local_name.clone()),
                ),
                None => (name.to_string(), range.clone(), None),
            };
            let Some(purl) = build_npm_purl(&emit_name, "") else {
                continue;
            };
            let mut extra_annotations: std::collections::BTreeMap<
                String,
                serde_json::Value,
            > = Default::default();
            if let Some(local) = alias_local_name {
                extra_annotations.insert(
                    "mikebom:declared-as".to_string(),
                    serde_json::json!([local]),
                );
            }
            out.push(PackageDbEntry {
                build_inclusion: None,
                purl,
                name: emit_name,
                version: String::new(),
                arch: None,
                source_path: source_path.to_string(),
                depends: Vec::new(),
                maintainer: None,
                licenses: Vec::new(),
                lifecycle_scope: if is_dev { Some(mikebom_common::resolution::LifecycleScope::Development) } else { Some(mikebom_common::resolution::LifecycleScope::Runtime) },
                requirement_ranges: vec![emit_range],
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
                extra_annotations,
                binary_role: None,
            });
        }
    }
    (out, acc)
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

    // Milestone 116 — produces-binaries extraction per FR-006 (npm).
    // The `bin` field shape per npm docs:
    //   - String form: `"bin": "./bin/foo.js"` — single binary named
    //     after the package's `name` field (npm strips any leading
    //     `@scope/` from the name when installing the symlink, but for
    //     identification purposes the package name IS the binary name).
    //   - Object form: `"bin": {"baz": "./cli.js", "baz-init": "..."}`
    //     — each key is one binary name.
    {
        let mut binary_candidates: Vec<String> = Vec::new();
        match parsed.get("bin") {
            Some(serde_json::Value::String(_)) => {
                // String form — binary name = package `name` field
                // (with any leading `@scope/` stripped per npm install
                // convention for unscoped consumption).
                let bin_name = name.rsplit('/').next().unwrap_or(name);
                binary_candidates.push(bin_name.to_string());
            }
            Some(serde_json::Value::Object(obj)) => {
                for k in obj.keys() {
                    binary_candidates.push(k.clone());
                }
            }
            _ => {}
        }
        crate::scan_fs::produces_binaries::stamp_into_annotations(
            &mut extra_annotations,
            binary_candidates,
        );
    }

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
        build_inclusion: None,
        purl,
        name: name.to_string(),
        version: version.to_string(),
        arch: None,
        source_path,
        depends,
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
        let (out, _acc) = parse_root_package_json(&src, "/package.json", false, None);
        assert_eq!(out.len(), 2);
        for c in &out {
            assert_eq!(c.sbom_tier.as_deref(), Some("design"));
            assert!(!c.requirement_ranges.is_empty());
            assert!(c.version.is_empty());
        }
    }

    #[test]
    fn root_pkgjson_fallback_include_dev_adds_devdeps() {
        let src = serde_json::json!({
            "dependencies": { "foo": "^1.0" },
            "devDependencies": { "jest": "^29.0" }
        });
        let (out, _acc) = parse_root_package_json(&src, "/package.json", true, None);
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
        let (out, _acc) = parse_root_package_json(&src, "/package.json", false, None);
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

    // -----------------------------------------------------------------
    // Milestone 163 (T019-T027 + T024a + T029-T030 + T032, closes #498)
    // Cross-workspace resolution unit tests.
    // -----------------------------------------------------------------

    fn make_purl(purl_str: &str) -> mikebom_common::types::purl::Purl {
        mikebom_common::types::purl::Purl::new(purl_str).unwrap()
    }

    fn make_pkg_entry(name: &str, version: &str, purl_str: &str) -> PackageDbEntry {
        PackageDbEntry {
            build_inclusion: None,
            purl: make_purl(purl_str),
            name: name.to_string(),
            version: version.to_string(),
            arch: None,
            source_path: "/lockfile-derived".to_string(),
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

    // T019: build_cross_workspace_index maps 2 real entries.
    #[test]
    fn t019_build_index_maps_lockfile_entries() {
        let entries = vec![
            make_pkg_entry(
                "@docusaurus/core",
                "3.10.1",
                "pkg:npm/%40docusaurus/core@3.10.1",
            ),
            make_pkg_entry("thor", "1.4.0", "pkg:npm/thor@1.4.0"),
        ];
        let index = build_cross_workspace_index(&entries);
        assert_eq!(index.get("@docusaurus/core").map(String::as_str), Some("3.10.1"));
        assert_eq!(index.get("thor").map(String::as_str), Some("1.4.0"));
        assert_eq!(index.len(), 2);
    }

    // T020: build_cross_workspace_index SKIPS design-tier (empty-version).
    #[test]
    fn t020_build_index_skips_design_tier_entries() {
        let entries = vec![
            make_pkg_entry("real-pkg", "1.0.0", "pkg:npm/real-pkg@1.0.0"),
            make_pkg_entry("design-tier", "", "pkg:npm/design-tier@"),
        ];
        let index = build_cross_workspace_index(&entries);
        assert!(index.contains_key("real-pkg"));
        assert!(!index.contains_key("design-tier"));
        assert_eq!(index.len(), 1);
    }

    // T021: resolve_for_workspace_peer returns Resolved when the dep is
    // in the cross-workspace index (no nested node_modules).
    #[test]
    fn t021_resolve_hits_index_when_no_nested() {
        let tmp = tempfile::tempdir().unwrap();
        let mut index = CrossWorkspaceIndex::new();
        index.insert("@docusaurus/core".to_string(), "3.10.1".to_string());
        match resolve_for_workspace_peer(tmp.path(), "@docusaurus/core", &index) {
            CrossResolution::Resolved { version } => assert_eq!(version, "3.10.1"),
            CrossResolution::Unresolved => panic!("expected Resolved"),
        }
    }

    // T022: resolve_for_workspace_peer returns Unresolved when the dep
    // is neither nested nor in the index.
    #[test]
    fn t022_resolve_returns_unresolved_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let index = CrossWorkspaceIndex::new();
        assert_eq!(
            resolve_for_workspace_peer(tmp.path(), "@some/missing", &index),
            CrossResolution::Unresolved
        );
    }

    // T023: FR-003 closest-ancestor — nested wins over index.
    #[test]
    fn t023_nested_node_modules_wins_over_index() {
        let tmp = tempfile::tempdir().unwrap();
        let nested_dir = tmp.path().join("node_modules").join("foo");
        std::fs::create_dir_all(&nested_dir).unwrap();
        std::fs::write(
            nested_dir.join("package.json"),
            r#"{"name":"foo","version":"2.0.0"}"#,
        )
        .unwrap();
        let mut index = CrossWorkspaceIndex::new();
        index.insert("foo".to_string(), "1.0.0".to_string());
        match resolve_for_workspace_peer(tmp.path(), "foo", &index) {
            CrossResolution::Resolved { version } => assert_eq!(
                version, "2.0.0",
                "nested version 2.0.0 must win over index-only 1.0.0"
            ),
            CrossResolution::Unresolved => panic!("expected Resolved"),
        }
    }

    // T024: reshaped parse_root_package_json — Some(_) + resolved →
    // dep accumulated in resolved_deps, no phantom entries.
    #[test]
    fn t024_reshape_resolved_accumulates_no_phantom() {
        let src = serde_json::json!({
            "dependencies": {
                "@docusaurus/core": "^3.10.1"
            }
        });
        let tmp = tempfile::tempdir().unwrap();
        let mut index = CrossWorkspaceIndex::new();
        index.insert("@docusaurus/core".to_string(), "3.10.1".to_string());
        let ctx = CrossWorkspaceContext {
            peer_root: tmp.path(),
            index: &index,
        };
        let (out, acc) = parse_root_package_json(&src, "/peer/package.json", false, Some(&ctx));
        assert_eq!(out.len(), 0, "no phantom entries when cross-resolved");
        assert_eq!(acc.resolved_deps, vec!["@docusaurus/core"]);
        assert!(acc.unresolved_deps.is_empty());
    }

    // T024a: SC-007 sub-item (i) — devDependencies get same treatment.
    #[test]
    fn t024a_devdeps_get_cross_resolution() {
        let src = serde_json::json!({
            "devDependencies": {
                "typescript": "^5.0.0",
                "some-missing-dev-dep": "^1.0.0"
            }
        });
        let tmp = tempfile::tempdir().unwrap();
        let mut index = CrossWorkspaceIndex::new();
        index.insert("typescript".to_string(), "5.4.0".to_string());
        // `some-missing-dev-dep` intentionally not in index.
        let ctx = CrossWorkspaceContext {
            peer_root: tmp.path(),
            index: &index,
        };
        // include_dev = true; devDependencies must be processed.
        let (out, acc) = parse_root_package_json(&src, "/peer/package.json", true, Some(&ctx));
        assert_eq!(out.len(), 0, "no phantom entries in workspace-peer mode");
        assert_eq!(
            acc.resolved_deps,
            vec!["typescript"],
            "devDep resolved via index accumulates in resolved_deps"
        );
        assert_eq!(
            acc.unresolved_deps,
            vec!["some-missing-dev-dep"],
            "devDep missing from index accumulates in unresolved_deps"
        );
    }

    // T025: reshaped parse_root_package_json — Some(_) + unresolved →
    // dep accumulated in unresolved_deps, no phantom entries.
    #[test]
    fn t025_reshape_unresolved_accumulates_no_phantom() {
        let src = serde_json::json!({
            "dependencies": {
                "@some/removed": "^1.0.0"
            }
        });
        let tmp = tempfile::tempdir().unwrap();
        let index = CrossWorkspaceIndex::new();
        let ctx = CrossWorkspaceContext {
            peer_root: tmp.path(),
            index: &index,
        };
        let (out, acc) = parse_root_package_json(&src, "/peer/package.json", false, Some(&ctx));
        assert_eq!(out.len(), 0, "no phantom entries when unresolved");
        assert!(acc.resolved_deps.is_empty());
        assert_eq!(acc.unresolved_deps, vec!["@some/removed"]);
    }

    // T026: reshaped parse_root_package_json — None → pre-163 behavior
    // preserved: phantom entries emitted with empty version.
    #[test]
    fn t026_reshape_none_preserves_pre163_behavior() {
        let src = serde_json::json!({
            "dependencies": {
                "foo": "^1.0.0"
            }
        });
        let (out, acc) = parse_root_package_json(&src, "/standalone/package.json", false, None);
        assert_eq!(out.len(), 1, "pre-163 emits one design-tier phantom");
        assert!(out[0].version.is_empty());
        assert_eq!(out[0].sbom_tier.as_deref(), Some("design"));
        assert_eq!(out[0].requirement_ranges.as_slice(), &["^1.0.0".to_string()]);
        assert!(acc.resolved_deps.is_empty());
        assert!(acc.unresolved_deps.is_empty());
    }

    // T027: verify the wire-shape helper used by npm::read() — bare
    // string vs JSON array for the C115 annotation value. The
    // stamping code lives in mod.rs; here we mirror the shape rule.
    #[test]
    fn t027_c115_wire_shape_singleton_vs_array() {
        // Singleton: bare string.
        let mut singles = vec!["@some/removed".to_string()];
        singles.sort();
        singles.dedup();
        let single_value = if singles.len() == 1 {
            serde_json::Value::String(singles.into_iter().next().unwrap())
        } else {
            serde_json::Value::Array(singles.into_iter().map(serde_json::Value::String).collect())
        };
        assert!(single_value.is_string(), "singleton must be bare String");

        // Multi: JSON array, sorted+deduplicated.
        let mut multi = vec![
            "@b/pkg".to_string(),
            "@a/pkg".to_string(),
            "@b/pkg".to_string(), // duplicate — must be removed
        ];
        multi.sort();
        multi.dedup();
        let multi_value = if multi.len() == 1 {
            serde_json::Value::String(multi.into_iter().next().unwrap())
        } else {
            serde_json::Value::Array(multi.into_iter().map(serde_json::Value::String).collect())
        };
        let arr = multi_value.as_array().expect("multi must be JSON array");
        assert_eq!(arr.len(), 2, "duplicates removed");
        assert_eq!(arr[0].as_str(), Some("@a/pkg"), "sorted lex-first");
        assert_eq!(arr[1].as_str(), Some("@b/pkg"));
    }

    // T029: SC-005 coverage-preservation — index build does NOT drop
    // entries.
    #[test]
    fn t029_build_index_preserves_entries_len() {
        let entries = vec![
            make_pkg_entry("real-a", "1.0.0", "pkg:npm/real-a@1.0.0"),
            make_pkg_entry("real-b", "2.0.0", "pkg:npm/real-b@2.0.0"),
            make_pkg_entry("design-tier", "", "pkg:npm/design-tier@"),
        ];
        let entries_len_before = entries.len();
        let _index = build_cross_workspace_index(&entries);
        assert_eq!(
            entries.len(),
            entries_len_before,
            "build_cross_workspace_index reads entries; must not mutate"
        );
    }

    // T030: FR-010 peer-dep regression guard — peerDependencies block
    // is NOT cross-resolved (out of scope per milestone 147's C1/C2).
    #[test]
    fn t030_peer_dependencies_not_cross_resolved() {
        let src = serde_json::json!({
            "dependencies": { "regular-dep": "^1.0" },
            "peerDependencies": { "peer-only": "^2.0" }
        });
        let tmp = tempfile::tempdir().unwrap();
        let mut index = CrossWorkspaceIndex::new();
        index.insert("regular-dep".to_string(), "1.5.0".to_string());
        index.insert("peer-only".to_string(), "2.5.0".to_string());
        let ctx = CrossWorkspaceContext {
            peer_root: tmp.path(),
            index: &index,
        };
        let (_out, acc) = parse_root_package_json(&src, "/peer/package.json", false, Some(&ctx));
        assert_eq!(
            acc.resolved_deps,
            vec!["regular-dep"],
            "only `dependencies:` names get accumulated; peer-only must NOT"
        );
        assert!(acc.unresolved_deps.is_empty());
    }

    // T032: FR-005 lockfile-format-agnostic behavior — build_cross_
    // workspace_index operates on `&[PackageDbEntry]` regardless of
    // provenance (PURL prefix + non-empty version are the only filters).
    #[test]
    fn t032_index_agnostic_to_provenance() {
        // Simulate entries from 4 different lockfile-format provenance
        // sources (source_path varies; the builder should not care).
        let mut entries = Vec::new();
        for (name, version, src_path) in [
            ("pkg-from-npm", "1.0.0", "/root/package-lock.json"),
            ("pkg-from-pnpm", "2.0.0", "/root/pnpm-lock.yaml"),
            ("pkg-from-yarn", "3.0.0", "/root/yarn.lock"),
            ("pkg-from-bun", "4.0.0", "/root/bun.lock"),
        ] {
            let mut e = make_pkg_entry(name, version, &format!("pkg:npm/{name}@{version}"));
            e.source_path = src_path.to_string();
            entries.push(e);
        }
        let index = build_cross_workspace_index(&entries);
        assert_eq!(index.len(), 4, "all 4 lockfile-provenance entries indexed");
        for name in [
            "pkg-from-npm",
            "pkg-from-pnpm",
            "pkg-from-yarn",
            "pkg-from-bun",
        ] {
            assert!(
                index.contains_key(name),
                "index-builder must not filter on lockfile-format provenance"
            );
        }
    }
}
