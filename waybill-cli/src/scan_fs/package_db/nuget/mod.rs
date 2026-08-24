//! NuGet source-tree reader (milestone 106 US4, closes #275).
//!
//! waybill already encodes `pkg:nuget/<name>@<version>` PURLs and runs
//! deps.dev enrichment for the nuget system — the missing piece was
//! the filesystem detection source. This reader closes that gap by
//! walking the scan tree for `.csproj` / `.vbproj` / `.fsproj` files
//! and resolving each `<PackageReference>` against:
//!
//! 1. `packages.lock.json` adjacent to the project (FR-008) — when
//!    present, gives the pinned `resolved` version + transitive graph.
//! 2. `Directory.Packages.props` in any ancestor directory
//!    (FR-007a, CPM) — `<PackageVersion Include="X" Version="..."/>`
//!    map for `.csproj` references that omit `Version=`.
//! 3. Inline `Version=` on the `<PackageReference>` itself (FR-007).
//! 4. If none of the above resolves, emit a design-tier component
//!    (empty `version` field, versionless PURL `pkg:nuget/<name>`,
//!    `waybill:unresolved-reason` annotation) + `tracing::warn!`.
//!    This matches waybill's cross-ecosystem posture for operator-
//!    declared-but-not-resolved deps (see gem/cargo/opkg readers).
//!    Fixes #653 — previously emitted `pkg:nuget/<name>@unresolved`
//!    which is an invalid PURL that downstream SBOM consumers drop.
//!
//! Per FR-007b, `PrivateAssets="All"` / `IncludeAssets=...` /
//! `ExcludeAssets=...` map to `LifecycleScope::Build`, which flows
//! through the existing milestone-052 emission path to CDX
//! `scope: "excluded"` and SPDX 2.3 `BUILD_DEPENDENCY_OF`.
//!
//! Cross-platform (no `#[cfg(unix)]`); zero new Cargo dependencies.

mod csproj;
mod deps_json;
mod directory_build_props;
mod directory_packages_props;
mod msbuild_properties;
mod packages_lock;
mod pe_clr;
mod private_assets;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::PackageDbEntry;

// Milestone 664 US2 T044: shared-walker migration types.
use crate::scan_fs::walk_registry::{
    globset_from_patterns_case_insensitive, ReaderId, ReaderRegistration,
    ReaderRegistryBuilder, SharedWalker, SharedWalkerContext,
};
use std::sync::{Arc, Mutex};

const PROJECT_EXTENSIONS: &[&str] = &["csproj", "vbproj", "fsproj"];

/// Per-scan state — 3 vectors matching the legacy 3 walker sites'
/// outputs. Callback dispatches by extension.
#[derive(Default, Debug)]
pub(crate) struct NugetDiscoveredPaths {
    pub(crate) project_files: Vec<PathBuf>,
    pub(crate) deps_files: Vec<PathBuf>,
    pub(crate) dll_paths: Vec<PathBuf>,
}

/// Per-file callback. Case-insensitive extension check preserves legacy
/// `eq_ignore_ascii_case` behavior.
fn on_nuget_file(path: &Path, ctx: &SharedWalkerContext<'_>) {
    let Some(state) = ctx.state::<Mutex<NugetDiscoveredPaths>>(ReaderId::NUGET) else {
        return;
    };
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else { return };
    let name_lower = name.to_ascii_lowercase();

    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    // `.deps.json` — matches `foo.deps.json` (compound extension).
    if name_lower.ends_with(".deps.json") {
        guard.deps_files.push(path.to_path_buf());
        return;
    }
    // `.dll` — case-insensitive extension.
    if name_lower.ends_with(".dll") {
        guard.dll_paths.push(path.to_path_buf());
        return;
    }
    // Project files: `.csproj`, `.vbproj`, `.fsproj` — case-insensitive.
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        if PROJECT_EXTENSIONS
            .iter()
            .any(|target| ext.eq_ignore_ascii_case(target))
        {
            guard.project_files.push(path.to_path_buf());
        }
    }
}

/// Build the `ReaderRegistration`.
pub(crate) fn registration() -> anyhow::Result<ReaderRegistration> {
    let patterns = globset_from_patterns_case_insensitive(&[
        "**/*.csproj",
        "**/*.vbproj",
        "**/*.fsproj",
        "**/*.deps.json",
        "**/*.dll",
    ])?;
    Ok(ReaderRegistration {
        reader_id: ReaderId::NUGET,
        state: Some(Arc::new(Mutex::new(NugetDiscoveredPaths::default()))),
        patterns,
        on_file: Some(on_nuget_file),
        on_dir: None,
        descend_into: None,
    })
}

pub(crate) fn extract_paths(registration: &ReaderRegistration) -> NugetDiscoveredPaths {
    let Some(state_arc) = registration.state.as_ref() else {
        return NugetDiscoveredPaths::default();
    };
    let Some(mutex) = state_arc.downcast_ref::<Mutex<NugetDiscoveredPaths>>() else {
        return NugetDiscoveredPaths::default();
    };
    let mut guard = match mutex.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    std::mem::take(&mut *guard)
}

#[allow(dead_code)]
pub(crate) fn build_and_run(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    let reg = match registration() {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!(error = %err, "nuget: registration() failed");
            return Vec::new();
        }
    };
    let registry = match ReaderRegistryBuilder::new().register(reg).build() {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!(error = %err, "nuget: build() failed");
            return Vec::new();
        }
    };
    // Use max depth across the 3 legacy walkers (deps_json + pe_clr both
    // use 32; project_files uses 8). max=32 to preserve all three's
    // reachability.
    let mut walker = SharedWalker::new(rootfs, &registry, exclude_set).with_max_depth(32);
    walker.run();
    let _ = walker.finish();
    let nuget_reg = registry
        .registrations()
        .iter()
        .find(|r| r.reader_id == ReaderId::NUGET)
        .expect("nuget registration must be present");
    let paths = extract_paths(nuget_reg);
    finalize(paths, rootfs, exclude_set)
}

/// Walk `rootfs` for NuGet project files and emit one `PackageDbEntry`
/// per resolved `<PackageReference>` (or `packages.lock.json` entry).
/// Empty when no project files are found.
/// Legacy `pub fn read()` — retained during FR-004 coexistence.
#[allow(dead_code)]
pub fn read(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    let paths = NugetDiscoveredPaths {
        project_files: collect_project_files(rootfs, exclude_set),
        deps_files: deps_json::collect_deps_json_files(rootfs, exclude_set),
        dll_paths: pe_clr::collect_dll_paths(rootfs, exclude_set),
    };
    finalize(paths, rootfs, exclude_set)
}

/// Post-walker entry — takes precomputed paths + runs the 3-sub-reader
/// pipeline in the legacy order.
pub(crate) fn finalize(
    paths: NugetDiscoveredPaths,
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    let _ = exclude_set;
    // Defensive sort — shared walker sorts per-dir but cross-dir order
    // depends on descent. Legacy `collect_*_files` output was returned
    // in walker's natural order; sort both paths for FR-006 identity.
    let NugetDiscoveredPaths {
        mut project_files,
        mut deps_files,
        mut dll_paths,
    } = paths;
    project_files.sort();
    deps_files.sort();
    dll_paths.sort();

    let mut out = Vec::new();
    for project_path in &project_files {
        out.extend(read_one_project(rootfs, project_path));
    }
    // Milestone 129 US1A: `.deps.json` — the .NET runtime dependency sidecar.
    out.extend(deps_json::finalize(deps_files, rootfs));
    // Milestone 130 US3: `*.dll` CLR managed-assembly metadata.
    out.extend(pe_clr::finalize(dll_paths, rootfs));
    out
}

/// Milestone 114: delegates to `scan_fs::walk::safe_walk`.
fn collect_project_files(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: 8,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            candidate
                .file_name()
                .and_then(|s| s.to_str())
                .map(super::project_roots::should_skip_default_descent)
                .unwrap_or(true)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file()
            && path
                .extension()
                .and_then(|s| s.to_str())
                .map(|e| {
                    PROJECT_EXTENSIONS
                        .iter()
                        .any(|target| e.eq_ignore_ascii_case(target))
                })
                .unwrap_or(false)
        {
            out.push(path.to_path_buf());
        }
    });
    out
}

fn read_one_project(scan_root: &Path, project_path: &Path) -> Vec<PackageDbEntry> {
    let project_dir = match project_path.parent() {
        Some(d) => d,
        None => return Vec::new(),
    };
    let mut project_references = csproj::parse_project_file(project_path);
    let lockfile_path = project_dir.join("packages.lock.json");
    let lockfile = if lockfile_path.is_file() {
        packages_lock::parse(&lockfile_path)
    } else {
        None
    };
    let props_path =
        directory_packages_props::find_props_walking_up(project_dir, scan_root);
    let cpm_map = match &props_path {
        Some(p) => directory_packages_props::parse_props(p),
        None => Default::default(),
    };

    // #655 (FU-001): discover + parse nearest Directory.Build.props +
    // Directory.Build.targets, walking up from the project directory
    // bounded by scan_root. Contributes three things:
    //   - `<PackageReference>` elements the csproj implicitly inherits
    //     (typically from `test/Directory.Build.props` or
    //     `samples/Directory.Build.props`);
    //   - `<PackageVersion>` elements that extend the CPM version map;
    //   - `<PropertyGroup>` values that feed the FU-002 $()-ref
    //     substitution.
    let build_files = directory_build_props::discover(project_dir, scan_root);

    // Prepend build.props/build.targets package references to the
    // csproj's own list. Prepend order: build.props+build.targets
    // (imported before csproj) → csproj. The accumulator deduplicates
    // by (name, version_opt) so identical inherited-then-declared
    // entries collapse into one component with the source-file paths
    // merged.
    let mut merged_refs = build_files.package_references.clone();
    merged_refs.append(&mut project_references);
    let project_references = merged_refs;

    // #654 (FU-002): build a merged MSBuild `<PropertyGroup>` property
    // map so we can resolve `$(PropertyName)` references in Version
    // strings from every scope MSBuild would evaluate:
    //   build.props ⊕ build.targets ⊕ packages.props ⊕ csproj
    // where later overlays override earlier ones on collision. This
    // matches MSBuild's evaluation order except for the rare case
    // where a csproj-defined property gets re-overridden by a
    // Directory.Build.targets (imported LAST in MSBuild); we accept
    // that corner case for simplicity — the alternative complicates
    // the merge order without meaningful real-world benefit.
    let build_property_map = build_files.property_map.clone();
    let props_property_map = match &props_path {
        Some(p) => msbuild_properties::parse_properties_file(p),
        None => Default::default(),
    };
    let csproj_property_map = msbuild_properties::parse_properties_file(project_path);
    let property_map = msbuild_properties::merge(
        msbuild_properties::merge(build_property_map, props_property_map),
        csproj_property_map,
    );

    // Merge Directory.Build.{props,targets} CPM contributions with
    // Directory.Packages.props' own. Directory.Packages.props wins on
    // collision — it's the canonical CPM location; entries in
    // Directory.Build.props should be treated as fallbacks.
    let mut merged_cpm: BTreeMap<String, String> = build_files
        .cpm_extensions
        .clone()
        .into_iter()
        .collect();
    merged_cpm.extend(cpm_map);
    let cpm_map = merged_cpm;

    // Substitute `$()` refs in every CPM map value up-front so the
    // fall-through consumers below see already-resolved strings.
    // Values that still contain `$(` after substitution stay raw and
    // trip the design-tier fallback (#653) at emission time.
    let cpm_map: BTreeMap<String, String> = cpm_map
        .into_iter()
        .map(|(k, v)| {
            let (subbed, unresolved) =
                msbuild_properties::substitute_and_check(&v, &property_map);
            if unresolved {
                tracing::warn!(
                    project = %project_path.display(),
                    package = %k,
                    raw_version = %v,
                    substituted = %subbed,
                    "CPM Version= contains unresolved MSBuild property reference; will fall through to design-tier (#654)"
                );
            }
            (k, subbed)
        })
        .collect();

    // Build a (name -> source-paths) accumulator so the same
    // (name, version) coord collected from .csproj + props +
    // packages.lock.json merges into a single component with a
    // comma-joined `waybill:source-files` annotation.
    //
    // Version is `Option<String>` so unresolved declarations
    // (`(name, None)`) don't collide with resolved ones
    // (`(name, Some("1.2.3"))`), and get separately materialized as
    // design-tier versionless components per #653.
    let mut acc: BTreeMap<(String, Option<String>), AccEntry> = BTreeMap::new();

    // Build per-name dependency edges from the lockfile (one merged
    // set across all frameworks; the dedup pipeline collapses by
    // canonical PURL). Each entry's value is the set of immediate-dep
    // names from `packages.lock.json::dependencies.<framework>.<name>.dependencies`.
    let lock_edges: BTreeMap<String, BTreeSet<String>> = build_lock_edges(&lockfile);

    // Step 1: emit one entry per `.csproj` `<PackageReference>`.
    for r in &project_references {
        if r.include.is_empty() {
            tracing::warn!(
                path = %project_path.display(),
                "<PackageReference> missing Include attribute; skipping"
            );
            continue;
        }
        let lifecycle_scope = private_assets::classify(&r.attrs);
        // Resolve version with the precedence:
        //   lockfile (any framework) > inline Version= > CPM map > None
        // `None` triggers a design-tier + versionless-PURL emission
        // downstream instead of an `@unresolved` PURL literal (#653).
        let lock_resolved = lockfile.as_ref().and_then(|f| {
            f.dependencies
                .values()
                .filter_map(|fw| fw.get(&r.include))
                .map(|p| p.resolved.clone())
                .find(|v| !v.is_empty())
        });
        // #654: substitute `$(PropertyName)` refs in the inline
        // Version= against the merged property map. Unresolved refs
        // (`$(Foo)` where `Foo` isn't defined) leave the raw form in
        // place; the containment check below then treats the whole
        // value as unresolved and falls through to design-tier.
        let inline_version = r
            .version
            .clone()
            .filter(|v| !v.is_empty())
            .and_then(|v| {
                let (subbed, unresolved) =
                    msbuild_properties::substitute_and_check(&v, &property_map);
                if unresolved {
                    tracing::warn!(
                        project = %project_path.display(),
                        package = %r.include,
                        raw_version = %v,
                        substituted = %subbed,
                        "<PackageReference> Version= contains unresolved MSBuild property reference; will fall through to design-tier (#654)"
                    );
                    None
                } else {
                    Some(subbed)
                }
            });
        // CPM map values are already property-substituted above; drop
        // any residual `$(` here (defensive — belt + suspenders).
        let cpm_resolved = cpm_map
            .get(&r.include)
            .cloned()
            .filter(|v| !v.contains("$("));
        let resolved_version: Option<String> = lock_resolved
            .or(inline_version)
            .or(cpm_resolved);
        if resolved_version.is_none() {
            tracing::warn!(
                project = %project_path.display(),
                package = %r.include,
                "<PackageReference> version unresolved (no Version=, no CPM, no lockfile entry) — emitting design-tier versionless component"
            );
        }

        let key = (r.include.clone(), resolved_version.clone());
        let entry = acc.entry(key).or_default();
        entry.lifecycle_scope = entry.lifecycle_scope.or(lifecycle_scope);
        // #655: use the reference's own source_file (populated by
        // csproj::parse_project_file with the file each element was
        // extracted from). This correctly attributes inherited
        // references from Directory.Build.props/targets to the props
        // path rather than the consuming csproj.
        entry.sources.insert(
            r.source_file
                .clone()
                .unwrap_or_else(|| project_path.to_path_buf()),
        );
        if cpm_map.contains_key(&r.include) {
            if let Some(p) = &props_path {
                entry.sources.insert(p.clone());
            }
        }
        if lock_versioned_match(&lockfile, &r.include) {
            entry.sources.insert(lockfile_path.clone());
            entry.source_type = entry.source_type.take().or(Some("direct".to_string()));
        }
    }

    // Step 2: emit transitive deps from the lockfile that are NOT
    // already accounted for via .csproj references. Transitives are
    // tagged with `waybill:source-type: "transitive"`.
    if let Some(lock) = &lockfile {
        for packages in lock.dependencies.values() {
            for (name, pkg) in packages {
                if pkg.resolved.is_empty() {
                    continue;
                }
                if pkg.entry_type.eq_ignore_ascii_case("Project") {
                    // Project references are intra-solution links to
                    // another .csproj; out of scope for this milestone
                    // per contracts/nuget-packages-lock.md.
                    continue;
                }
                let key = (name.clone(), Some(pkg.resolved.clone()));
                let entry = acc.entry(key).or_default();
                entry.sources.insert(lockfile_path.clone());
                if pkg.entry_type.eq_ignore_ascii_case("Transitive")
                    && entry.source_type.is_none()
                {
                    entry.source_type = Some("transitive".to_string());
                }
            }
        }
    }

    // Materialize accumulated entries into PackageDbEntries.
    let mut out = Vec::new();
    for ((name, version_opt), acc_entry) in acc {
        let version_str = version_opt.as_deref().unwrap_or("");
        let Some(purl) = build_nuget_purl(&name, version_str) else {
            tracing::warn!(
                package = %name,
                version = %version_str,
                "nuget coord produced invalid PURL; skipping"
            );
            continue;
        };
        let depends: Vec<String> = lock_edges
            .get(&name)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        let mut extra_annotations: BTreeMap<String, serde_json::Value> = Default::default();
        if acc_entry.sources.len() > 1 {
            let joined = acc_entry
                .sources
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(",");
            extra_annotations.insert(
                "waybill:source-files".to_string(),
                serde_json::Value::String(joined),
            );
        }

        // #653: unresolved (None) → design-tier, versionless PURL,
        // empty version field, waybill:unresolved-reason annotation.
        // Resolved (Some) → source-tier as before.
        let sbom_tier = if version_opt.is_some() {
            "source"
        } else {
            extra_annotations.insert(
                "waybill:unresolved-reason".to_string(),
                serde_json::Value::String(
                    "no Version= on <PackageReference>, no CPM entry in Directory.Packages.props, no packages.lock.json entry".to_string(),
                ),
            );
            "design"
        };

        let primary_source = acc_entry
            .sources
            .iter()
            .next()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| project_path.to_string_lossy().to_string());

        out.push(PackageDbEntry {
            build_inclusion: None,
            purl,
            name,
            version: version_str.to_string(),
            arch: None,
            source_path: primary_source,
            depends,
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: acc_entry.lifecycle_scope,
            requirement_ranges: Vec::new(),
            source_type: acc_entry.source_type,
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
            sbom_tier: Some(sbom_tier.to_string()),
            shade_relocation: None,
            extra_annotations,
            binary_role: None,
        });
    }

    // Milestone 230 (FR-001 / FR-004 / FR-005 / FR-009) — emit one
    // main-module component per project file, populate its `depends`
    // with the root→direct dependency name set:
    //  * US1 (locked): union across every framework block in
    //    packages.lock.json of every entry typed Direct or
    //    CentralTransitive. Project entries stay skipped per FR-008.
    //  * US2 (unlocked): fall back to the project's
    //    <PackageReference Include=...> values.
    // Names are resolved to PURLs by the shared edge-emission loop
    // in `scan_fs/mod.rs:560+` at scan time (see research §R1).
    let main_module_depends: Vec<String> = if let Some(lock) = &lockfile {
        let mut names: BTreeSet<String> = BTreeSet::new();
        for framework in lock.dependencies.values() {
            for (name, pkg) in framework {
                if pkg.entry_type.eq_ignore_ascii_case("Direct")
                    || pkg.entry_type.eq_ignore_ascii_case("CentralTransitive")
                {
                    names.insert(name.clone());
                }
            }
        }
        names.into_iter().collect()
    } else {
        let mut names: BTreeSet<String> = BTreeSet::new();
        for r in &project_references {
            if !r.include.is_empty() {
                names.insert(r.include.clone());
            }
        }
        names.into_iter().collect()
    };
    if let Some(mm) = build_nuget_main_module_entry(
        project_path,
        &property_map,
        main_module_depends,
    ) {
        out.push(mm);
    }

    out
}

#[derive(Default)]
struct AccEntry {
    lifecycle_scope: Option<waybill_common::resolution::LifecycleScope>,
    source_type: Option<String>,
    /// Source files contributing to this coord. `BTreeSet` for
    /// deterministic comma-join ordering.
    sources: BTreeSet<PathBuf>,
}

pub(super) fn build_nuget_purl(name: &str, version: &str) -> Option<Purl> {
    // #653: emit a versionless PURL when version is empty (design-tier
    // fall-through path from `read_one_project`). Matches the
    // gem/cargo/opkg readers' convention. `Purl::new` accepts the
    // no-`@` form; downstream consumers treat a versionless nuget PURL
    // as a design declaration rather than a vulnerability-scan target.
    let purl_str = if version.is_empty() {
        format!("pkg:nuget/{}", encode_purl_segment(name))
    } else {
        format!(
            "pkg:nuget/{}@{}",
            encode_purl_segment(name),
            encode_purl_segment(version),
        )
    };
    Purl::new(&purl_str).ok()
}

/// Milestone 230 (FR-010) — main-module version-derivation ladder.
///
/// Consults the merged MSBuild property map assembled by `read_one_project`
/// (build.props ⊕ build.targets ⊕ packages.props ⊕ csproj) for the
/// SDK-style version elements in this order:
///
/// 1. `<Version>` (canonical SDK-style version)
/// 2. `<VersionPrefix>` (+ `<VersionSuffix>` joined with `-` when set)
/// 3. `<AssemblyVersion>`
///
/// Values may contain `$(PropertyName)` references; each candidate is
/// property-substituted via `msbuild_properties::substitute_and_check`
/// and skipped when the result still holds an unresolved `$(...)`.
///
/// Returns an empty string when nothing resolves — the caller then
/// falls back to the `pkg:generic/<stem>@0.0.0` PURL shape per FR-003.
fn resolve_main_module_version(property_map: &msbuild_properties::PropertyMap) -> String {
    // (1) Direct <Version> element.
    if let Some(raw) = property_map.get("version") {
        let (subbed, unresolved) =
            msbuild_properties::substitute_and_check(raw, property_map);
        if !unresolved && !subbed.is_empty() {
            return subbed;
        }
    }
    // (2) <VersionPrefix> [+ <VersionSuffix>].
    if let Some(prefix_raw) = property_map.get("versionprefix") {
        let (prefix, prefix_unresolved) =
            msbuild_properties::substitute_and_check(prefix_raw, property_map);
        if !prefix_unresolved && !prefix.is_empty() {
            let suffix = property_map
                .get("versionsuffix")
                .and_then(|raw| {
                    let (subbed, unresolved) =
                        msbuild_properties::substitute_and_check(raw, property_map);
                    if unresolved || subbed.is_empty() {
                        None
                    } else {
                        Some(subbed)
                    }
                });
            return match suffix {
                Some(s) => format!("{}-{}", prefix, s),
                None => prefix,
            };
        }
    }
    // (3) <AssemblyVersion>.
    if let Some(raw) = property_map.get("assemblyversion") {
        let (subbed, unresolved) =
            msbuild_properties::substitute_and_check(raw, property_map);
        if !unresolved && !subbed.is_empty() {
            return subbed;
        }
    }
    // Nothing resolved.
    String::new()
}

/// Milestone 230 (research R3 + R5) — main-module name resolution.
/// Reads `<AssemblyName>` from the merged property map (running through
/// `msbuild_properties::substitute` for any `$(...)` refs); falls back
/// to the project file's filename stem when unset or unresolvable.
fn resolve_main_module_name(
    project_path: &Path,
    property_map: &msbuild_properties::PropertyMap,
) -> String {
    if let Some(raw) = property_map.get("assemblyname") {
        let (subbed, unresolved) =
            msbuild_properties::substitute_and_check(raw, property_map);
        if !unresolved && !subbed.is_empty() {
            return subbed;
        }
    }
    project_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Milestone 230 (FR-001 / FR-002 / FR-003) — build the NuGet main-
/// module `PackageDbEntry` for one project file. Mirrors the shape
/// established by cargo m064's `build_cargo_main_module_entry`
/// (`cargo.rs:504+`) and gem m069's `build_gem_main_module_entry`.
///
/// `depends` is the pre-computed root→direct dependency name list
/// (union across TFMs for locked projects; `<PackageReference Include>`
/// names for unlocked projects). Names are resolved to PURLs by the
/// shared edge-emission loop in `scan_fs/mod.rs:560+` at scan time.
///
/// Returns `None` only when both PURL construction paths fail
/// (unreachable in practice — matches m064's defensive-None convention).
fn build_nuget_main_module_entry(
    project_path: &Path,
    property_map: &msbuild_properties::PropertyMap,
    depends: Vec<String>,
) -> Option<PackageDbEntry> {
    let name = resolve_main_module_name(project_path, property_map);
    if name.is_empty() {
        return None;
    }
    let version = resolve_main_module_version(property_map);
    // Milestone 230 FR-003: pkg:nuget/<AssemblyName>@<version> when a
    // version resolves; pkg:generic/<project-stem>@0.0.0 fallback when
    // nothing does. The fallback matches the reporter's proposed shape
    // and matches waybill's cross-ecosystem posture for unversioned
    // source-tree entities.
    let purl = if version.is_empty() {
        let stem = project_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| name.clone());
        Purl::new(&format!(
            "pkg:generic/{}@0.0.0",
            encode_purl_segment(&stem)
        ))
        .ok()?
    } else {
        build_nuget_purl(&name, &version)?
    };

    let mut extra_annotations: BTreeMap<String, serde_json::Value> = Default::default();
    extra_annotations.insert(
        "waybill:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );

    let source_path = format!("path+file://{}", project_path.display());
    let effective_version = if version.is_empty() {
        "0.0.0".to_string()
    } else {
        version
    };

    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name,
        version: effective_version,
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

fn build_lock_edges(
    lockfile: &Option<packages_lock::PackagesLockFile>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let Some(f) = lockfile else {
        return out;
    };
    for fw in f.dependencies.values() {
        for (pkg_name, pkg) in fw {
            if pkg.dependencies.is_empty() {
                continue;
            }
            let bucket = out.entry(pkg_name.clone()).or_default();
            for dep_name in pkg.dependencies.keys() {
                bucket.insert(dep_name.clone());
            }
        }
    }
    out
}

fn lock_versioned_match(
    lockfile: &Option<packages_lock::PackagesLockFile>,
    include: &str,
) -> bool {
    let Some(f) = lockfile else {
        return false;
    };
    f.dependencies
        .values()
        .any(|fw| fw.contains_key(include))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::resolution::LifecycleScope;

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    /// Milestone 230 — filter `read()` output to package-level entries
    /// only (i.e., strip main-module components introduced in m230).
    /// Existing tests below were written pre-m230 and assert exact
    /// package-component counts; the FR-006 byte-parity contract lets
    /// those assertions stand as-is when read through this filter.
    /// Main-module-specific behavior is exercised by new m230 tests.
    fn read_pkgs(root: &Path) -> Vec<PackageDbEntry> {
        read(root, &Default::default())
            .into_iter()
            .filter(|e| {
                e.extra_annotations
                    .get("waybill:component-role")
                    .and_then(|v| v.as_str())
                    != Some("main-module")
            })
            .collect()
    }

    #[test]
    fn resolves_legacy_csproj_version() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.SampleLib" Version="1.2.3" />
  </ItemGroup>
</Project>"#,
        );
        let entries = read_pkgs(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:nuget/MikebomFixture.SampleLib@1.2.3"
        );
        assert!(entries[0].lifecycle_scope.is_none());
    }

    #[test]
    fn resolves_via_cpm_when_version_absent() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "Directory.Packages.props",
            r#"<Project>
  <ItemGroup>
    <PackageVersion Include="MikebomFixture.Cpm" Version="9.0.1" />
  </ItemGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.Cpm" />
  </ItemGroup>
</Project>"#,
        );
        let entries = read_pkgs(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "MikebomFixture.Cpm");
        assert_eq!(entries[0].version, "9.0.1");
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:nuget/MikebomFixture.Cpm@9.0.1"
        );
        // Both .csproj and props paths must appear in source-files.
        let source_files = entries[0]
            .extra_annotations
            .get("waybill:source-files")
            .and_then(|v| v.as_str())
            .unwrap();
        assert!(source_files.contains("App.csproj"));
        assert!(source_files.contains("Directory.Packages.props"));
    }

    #[test]
    fn lockfile_overrides_csproj_version() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.SampleLib" Version="1.2.3" />
  </ItemGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "packages.lock.json",
            r#"{
                "version": 1,
                "dependencies": {
                    "net8.0": {
                        "MikebomFixture.SampleLib": {
                            "type": "Direct",
                            "resolved": "1.2.4"
                        },
                        "MikebomFixture.Trans": {
                            "type": "Transitive",
                            "resolved": "0.5.0"
                        }
                    }
                }
            }"#,
        );
        let entries = read_pkgs(tmp.path());
        // SampleLib should pick up the lockfile's 1.2.4 (not csproj's 1.2.3).
        let sample = entries
            .iter()
            .find(|e| e.name == "MikebomFixture.SampleLib")
            .unwrap();
        assert_eq!(sample.version, "1.2.4");
        // Transitive must also appear.
        let trans = entries
            .iter()
            .find(|e| e.name == "MikebomFixture.Trans")
            .unwrap();
        assert_eq!(trans.version, "0.5.0");
        assert_eq!(trans.source_type.as_deref(), Some("transitive"));
    }

    #[test]
    fn private_assets_all_emits_build_scope() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.SourceLink" Version="1.0.0" PrivateAssets="All" />
  </ItemGroup>
</Project>"#,
        );
        let entries = read_pkgs(tmp.path());
        assert_eq!(entries.len(), 1);
        assert!(matches!(
            entries[0].lifecycle_scope,
            Some(LifecycleScope::Build)
        ));
    }

    #[test]
    fn unresolved_version_emits_design_tier_versionless_purl() {
        // #653: previously emitted `pkg:nuget/<name>@unresolved`
        // which is an invalid PURL literal that downstream SBOM
        // consumers (Trivy, DependencyTrack) drop or error on.
        // Now the reader falls through to a design-tier component
        // with a versionless PURL + `waybill:unresolved-reason`
        // annotation, matching the cross-ecosystem convention
        // (see gem/cargo/opkg readers).
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.NoVersion" />
  </ItemGroup>
</Project>"#,
        );
        let entries = read_pkgs(tmp.path());
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.name, "MikebomFixture.NoVersion");
        assert_eq!(e.version, "");
        assert_eq!(e.purl.as_str(), "pkg:nuget/MikebomFixture.NoVersion");
        assert!(!e.purl.as_str().contains("@unresolved"));
        assert!(!e.purl.as_str().contains('@'));
        assert_eq!(e.sbom_tier.as_deref(), Some("design"));
        let reason = e
            .extra_annotations
            .get("waybill:unresolved-reason")
            .and_then(|v| v.as_str())
            .unwrap();
        assert!(reason.contains("no Version="));
        assert!(reason.contains("no CPM entry"));
        assert!(reason.contains("no packages.lock.json"));
    }

    #[test]
    fn unresolved_and_resolved_declarations_dedup_separately() {
        // A csproj that declares one resolved and one unresolved
        // dep should emit exactly two components — one source-tier
        // versioned, one design-tier versionless. Prior to #653 the
        // unresolved one collided at (name, "unresolved") which was
        // accidentally distinguishing but produced an invalid PURL.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.Resolved" Version="1.0.0" />
    <PackageReference Include="MikebomFixture.Unresolved" />
  </ItemGroup>
</Project>"#,
        );
        let entries = read_pkgs(tmp.path());
        assert_eq!(entries.len(), 2);
        let resolved = entries
            .iter()
            .find(|e| e.name == "MikebomFixture.Resolved")
            .unwrap();
        assert_eq!(resolved.version, "1.0.0");
        assert_eq!(resolved.purl.as_str(), "pkg:nuget/MikebomFixture.Resolved@1.0.0");
        assert_eq!(resolved.sbom_tier.as_deref(), Some("source"));
        let unresolved = entries
            .iter()
            .find(|e| e.name == "MikebomFixture.Unresolved")
            .unwrap();
        assert_eq!(unresolved.version, "");
        assert_eq!(unresolved.purl.as_str(), "pkg:nuget/MikebomFixture.Unresolved");
        assert_eq!(unresolved.sbom_tier.as_deref(), Some("design"));
    }

    #[test]
    fn msbuild_property_ref_in_csproj_version_resolves_via_same_file_propertygroup() {
        // #654: `<PackageReference Version="$(Ver)">` with `<Ver>1.2.3</Ver>`
        // in the same csproj's `<PropertyGroup>` resolves cleanly.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <PropertyGroup>
    <MikebomFixtureVer>1.2.3</MikebomFixtureVer>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.PropRef" Version="$(MikebomFixtureVer)" />
  </ItemGroup>
</Project>"#,
        );
        let entries = read_pkgs(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].version, "1.2.3");
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:nuget/MikebomFixture.PropRef@1.2.3"
        );
        assert!(!entries[0].purl.as_str().contains("$("));
    }

    #[test]
    fn msbuild_property_ref_in_props_version_resolves_via_props_propertygroup() {
        // #654: RestSharp-shape — Directory.Packages.props declares
        // both the property AND the CPM PackageVersion Version=$(Ver).
        // The csproj references it CPM-style (no inline Version=).
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "Directory.Packages.props",
            r#"<Project>
  <PropertyGroup Condition="'$(TargetFramework)' == 'net10.0'">
    <SystemTextJsonVer>10.0.0</SystemTextJsonVer>
  </PropertyGroup>
  <ItemGroup>
    <PackageVersion Include="MikebomFixture.CpmProp" Version="$(SystemTextJsonVer)" />
  </ItemGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.CpmProp" />
  </ItemGroup>
</Project>"#,
        );
        let entries = read_pkgs(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].version, "10.0.0");
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:nuget/MikebomFixture.CpmProp@10.0.0"
        );
        assert!(!entries[0].purl.as_str().contains("$("));
    }

    #[test]
    fn unresolved_msbuild_property_falls_through_to_design_tier() {
        // #654 + #653: when `$(SomeProp)` isn't defined anywhere the
        // parser can see (e.g., because SomeProp lives in an unimported
        // Directory.Build.props — see FU-001), the component emits as
        // design-tier + versionless PURL rather than shipping a broken
        // `pkg:nuget/X@$(SomeProp)` literal.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.MissingProp" Version="$(NotDefinedAnywhere)" />
  </ItemGroup>
</Project>"#,
        );
        let entries = read_pkgs(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "MikebomFixture.MissingProp");
        assert_eq!(entries[0].version, "");
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:nuget/MikebomFixture.MissingProp"
        );
        assert_eq!(entries[0].sbom_tier.as_deref(), Some("design"));
        assert!(!entries[0].purl.as_str().contains("$("));
        assert!(!entries[0].purl.as_str().contains("@"));
    }

    #[test]
    fn csproj_property_group_overlays_props_property_group() {
        // MSBuild evaluation order: the csproj is closer to the
        // consumer than an ancestor Directory.Packages.props, so a
        // property defined in both takes the csproj's value.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "Directory.Packages.props",
            r#"<Project>
  <PropertyGroup>
    <SharedVer>1.0.0</SharedVer>
  </PropertyGroup>
  <ItemGroup>
    <PackageVersion Include="MikebomFixture.Overlay" Version="$(SharedVer)" />
  </ItemGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <PropertyGroup>
    <SharedVer>2.0.0</SharedVer>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.Overlay" />
  </ItemGroup>
</Project>"#,
        );
        let entries = read_pkgs(tmp.path());
        assert_eq!(entries.len(), 1);
        // csproj's SharedVer=2.0.0 overrides the props' 1.0.0.
        assert_eq!(entries[0].version, "2.0.0");
    }

    #[test]
    fn directory_build_props_contributes_inherited_package_references() {
        // #655 (FU-001) — RestSharp shape. `test/Directory.Build.props`
        // declares xunit + coverlet; every csproj under `test/`
        // inherits both. Prior to this fix waybill silently missed
        // these packages.
        let tmp = tempfile::tempdir().unwrap();
        let scan_root = tmp.path();
        let test_dir = scan_root.join("test");
        std::fs::create_dir_all(&test_dir).unwrap();
        write(
            &test_dir,
            "Directory.Build.props",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.Xunit" Version="2.9.2" />
    <PackageReference Include="MikebomFixture.Coverlet" Version="6.0.2" />
  </ItemGroup>
</Project>"#,
        );
        // Put an .csproj under test/ that declares its own reference
        // in addition to the inherited ones.
        write(
            &test_dir,
            "TestProject.csproj",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.Local" Version="1.0.0" />
  </ItemGroup>
</Project>"#,
        );
        let entries = read_pkgs(scan_root);
        let names: BTreeSet<_> = entries.iter().map(|e| e.name.clone()).collect();
        assert!(names.contains("MikebomFixture.Xunit"), "inherited xunit missing");
        assert!(names.contains("MikebomFixture.Coverlet"), "inherited coverlet missing");
        assert!(names.contains("MikebomFixture.Local"), "csproj-local ref missing");
        // Inherited references attribute their source_path to the
        // Directory.Build.props, not the csproj.
        let xunit = entries
            .iter()
            .find(|e| e.name == "MikebomFixture.Xunit")
            .unwrap();
        assert!(
            xunit.source_path.contains("Directory.Build.props"),
            "inherited ref source_path should point to Directory.Build.props; got {}",
            xunit.source_path
        );
    }

    #[test]
    fn directory_build_props_property_group_feeds_substitution() {
        // #655 + #654 — property defined in Directory.Build.props
        // should resolve $() refs in the csproj's inline Version=.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "Directory.Build.props",
            r#"<Project>
  <PropertyGroup>
    <MikebomVer>3.0.1</MikebomVer>
  </PropertyGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.PropInherit" Version="$(MikebomVer)" />
  </ItemGroup>
</Project>"#,
        );
        let entries = read_pkgs(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].version, "3.0.1");
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:nuget/MikebomFixture.PropInherit@3.0.1"
        );
    }

    #[test]
    fn directory_build_props_package_version_extends_cpm() {
        // #655 — Directory.Build.props can declare `<PackageVersion>`
        // elements that behave as CPM fallbacks when
        // Directory.Packages.props doesn't declare the package.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "Directory.Build.props",
            r#"<Project>
  <ItemGroup>
    <PackageVersion Include="MikebomFixture.BuildCpm" Version="7.7.7" />
  </ItemGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.BuildCpm" />
  </ItemGroup>
</Project>"#,
        );
        let entries = read_pkgs(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].version, "7.7.7");
    }

    #[test]
    fn packages_props_wins_over_build_props_for_same_cpm_key() {
        // #655 — when both Directory.Build.props and
        // Directory.Packages.props declare the same PackageVersion,
        // Directory.Packages.props wins (canonical CPM location).
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "Directory.Build.props",
            r#"<Project>
  <ItemGroup>
    <PackageVersion Include="MikebomFixture.CpmClash" Version="1.0.0" />
  </ItemGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "Directory.Packages.props",
            r#"<Project>
  <ItemGroup>
    <PackageVersion Include="MikebomFixture.CpmClash" Version="2.0.0" />
  </ItemGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.CpmClash" />
  </ItemGroup>
</Project>"#,
        );
        let entries = read_pkgs(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].version, "2.0.0", "packages.props should win");
    }

    #[test]
    fn vbproj_and_fsproj_recognized() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.vbproj",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.VbLib" Version="1.0.0" />
  </ItemGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "App.fsproj",
            r#"<Project>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.FsLib" Version="2.0.0" />
  </ItemGroup>
</Project>"#,
        );
        let entries = read_pkgs(tmp.path());
        assert_eq!(entries.len(), 2);
        let names: BTreeSet<_> = entries.iter().map(|e| e.name.clone()).collect();
        assert!(names.contains("MikebomFixture.VbLib"));
        assert!(names.contains("MikebomFixture.FsLib"));
    }

    // =========================================================
    // Milestone 230 — main-module + root→direct edges
    // =========================================================

    fn read_main_modules(root: &Path) -> Vec<PackageDbEntry> {
        read(root, &Default::default())
            .into_iter()
            .filter(|e| {
                e.extra_annotations
                    .get("waybill:component-role")
                    .and_then(|v| v.as_str())
                    == Some("main-module")
            })
            .collect()
    }

    #[test]
    fn main_module_edges_from_lockfile_direct() {
        // T005 (US1) — locked project, single TFM, Direct entry produces
        // main-module → package edge.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <PropertyGroup>
    <Version>1.0.0</Version>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.SampleLib" />
  </ItemGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "packages.lock.json",
            r#"{
  "version": 1,
  "dependencies": {
    "net8.0": {
      "MikebomFixture.SampleLib": {
        "type": "Direct",
        "requested": "1.2.3",
        "resolved": "1.2.3",
        "contentHash": "aaaa",
        "dependencies": {}
      }
    }
  }
}"#,
        );
        let main_modules = read_main_modules(tmp.path());
        assert_eq!(main_modules.len(), 1, "one main-module per project");
        let mm = &main_modules[0];
        assert_eq!(mm.name, "App");
        assert_eq!(mm.version, "1.0.0");
        assert_eq!(mm.purl.as_str(), "pkg:nuget/App@1.0.0");
        assert_eq!(mm.sbom_tier.as_deref(), Some("source"));
        assert!(
            mm.depends.iter().any(|d| d == "MikebomFixture.SampleLib"),
            "main-module depends must include the Direct lockfile entry; got {:?}",
            mm.depends
        );
    }

    #[test]
    fn main_module_edges_from_lockfile_central_transitive() {
        // T006 (US1) — CPM versionless PackageReference resolved to a
        // CentralTransitive entry in the lockfile still counts as
        // root→direct per FR-004.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "Directory.Packages.props",
            r#"<Project>
  <PropertyGroup>
    <ManagePackageVersionsCentrally>true</ManagePackageVersionsCentrally>
  </PropertyGroup>
  <ItemGroup>
    <PackageVersion Include="MikebomFixture.SharedLib" Version="5.6.7" />
  </ItemGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <PropertyGroup>
    <Version>1.0.0</Version>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.SharedLib" />
  </ItemGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "packages.lock.json",
            r#"{
  "version": 1,
  "dependencies": {
    "net8.0": {
      "MikebomFixture.SharedLib": {
        "type": "CentralTransitive",
        "requested": "5.6.7",
        "resolved": "5.6.7",
        "contentHash": "bbbb",
        "dependencies": {}
      }
    }
  }
}"#,
        );
        let main_modules = read_main_modules(tmp.path());
        assert_eq!(main_modules.len(), 1);
        assert!(
            main_modules[0]
                .depends
                .iter()
                .any(|d| d == "MikebomFixture.SharedLib"),
            "CentralTransitive should feed main-module depends per FR-004; got {:?}",
            main_modules[0].depends
        );
    }

    #[test]
    fn main_module_excludes_transitive_and_project_entries() {
        // T007 (US1) — Transitive entries MUST NOT appear on the
        // main-module's depends per FR-004. Project entries MUST NOT
        // appear per FR-007 + FR-008 (verified transitively via the
        // existing Project-skip at nuget/mod.rs:321). C1 remediation.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <PropertyGroup>
    <Version>1.0.0</Version>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.RootLib" />
  </ItemGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "packages.lock.json",
            r#"{
  "version": 1,
  "dependencies": {
    "net8.0": {
      "MikebomFixture.RootLib": {
        "type": "Direct",
        "requested": "1.0.0",
        "resolved": "1.0.0",
        "contentHash": "aaaa",
        "dependencies": { "MikebomFixture.OnlyTransitive": "2.0.0" }
      },
      "MikebomFixture.OnlyTransitive": {
        "type": "Transitive",
        "resolved": "2.0.0",
        "contentHash": "bbbb",
        "dependencies": {}
      },
      "MikebomFixture.NestedProject": {
        "type": "Project",
        "dependencies": {}
      }
    }
  }
}"#,
        );
        let main_modules = read_main_modules(tmp.path());
        assert_eq!(main_modules.len(), 1);
        let deps: BTreeSet<_> = main_modules[0].depends.iter().cloned().collect();
        assert!(
            deps.contains("MikebomFixture.RootLib"),
            "Direct entry present; got {:?}",
            deps
        );
        assert!(
            !deps.contains("MikebomFixture.OnlyTransitive"),
            "Transitive entry MUST NOT be on main-module.depends; got {:?}",
            deps
        );
        assert!(
            !deps.contains("MikebomFixture.NestedProject"),
            "Project entry MUST NOT be on main-module.depends (FR-007 + FR-008); got {:?}",
            deps
        );
    }

    #[test]
    fn main_module_multi_tfm_union() {
        // T008 (US1) — multi-TFM projects emit one main-module whose
        // depends is the UNION of Direct+CentralTransitive across every
        // framework block (FR-009). Same-name entries dedup by name.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <PropertyGroup>
    <Version>1.0.0</Version>
    <TargetFrameworks>net6.0;net8.0</TargetFrameworks>
  </PropertyGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "packages.lock.json",
            r#"{
  "version": 1,
  "dependencies": {
    "net6.0": {
      "MikebomFixture.Shared": {
        "type": "Direct",
        "resolved": "1.0.0",
        "contentHash": "aaaa",
        "dependencies": {}
      }
    },
    "net8.0": {
      "MikebomFixture.Shared": {
        "type": "Direct",
        "resolved": "1.0.0",
        "contentHash": "aaaa",
        "dependencies": {}
      },
      "MikebomFixture.OnlyNet8": {
        "type": "Direct",
        "resolved": "3.0.0",
        "contentHash": "cccc",
        "dependencies": {}
      }
    }
  }
}"#,
        );
        let main_modules = read_main_modules(tmp.path());
        assert_eq!(main_modules.len(), 1);
        let deps: BTreeSet<_> = main_modules[0].depends.iter().cloned().collect();
        assert!(deps.contains("MikebomFixture.Shared"));
        assert!(deps.contains("MikebomFixture.OnlyNet8"));
        assert_eq!(deps.len(), 2, "dedup by name across TFMs; got {:?}", deps);
    }

    #[test]
    fn main_module_assembly_name_override() {
        // T009 (US1) — <AssemblyName> takes precedence over the project
        // filename stem for the PURL name segment (research §R3).
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <PropertyGroup>
    <Version>2.0.0</Version>
    <AssemblyName>Contoso.Framework</AssemblyName>
  </PropertyGroup>
</Project>"#,
        );
        let main_modules = read_main_modules(tmp.path());
        assert_eq!(main_modules.len(), 1);
        assert_eq!(main_modules[0].name, "Contoso.Framework");
        assert_eq!(
            main_modules[0].purl.as_str(),
            "pkg:nuget/Contoso.Framework@2.0.0",
            "AssemblyName drives PURL name segment"
        );
    }

    #[test]
    fn main_module_version_ladder_falls_through_to_generic() {
        // T010 (US1) — no <Version>, no <VersionPrefix>, no
        // <AssemblyVersion> → pkg:generic/<stem>@0.0.0 per FR-003 + FR-010.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
  </PropertyGroup>
</Project>"#,
        );
        let main_modules = read_main_modules(tmp.path());
        assert_eq!(main_modules.len(), 1);
        assert_eq!(main_modules[0].purl.as_str(), "pkg:generic/App@0.0.0");
        assert_eq!(main_modules[0].version, "0.0.0");
    }

    #[test]
    fn main_module_version_ladder_prefers_version_prefix_suffix() {
        // T010b (US1) — <VersionPrefix> + <VersionSuffix> concatenate
        // with "-" per FR-010.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <PropertyGroup>
    <VersionPrefix>3.1.4</VersionPrefix>
    <VersionSuffix>preview.1</VersionSuffix>
  </PropertyGroup>
</Project>"#,
        );
        let main_modules = read_main_modules(tmp.path());
        assert_eq!(main_modules.len(), 1);
        assert_eq!(main_modules[0].version, "3.1.4-preview.1");
    }

    #[test]
    fn main_module_unlocked_derives_from_package_reference() {
        // T015 (US2) — no packages.lock.json → design-tier fallback
        // from <PackageReference> per FR-005.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <PropertyGroup>
    <Version>1.0.0</Version>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.SampleLib" Version="1.2.3" />
  </ItemGroup>
</Project>"#,
        );
        let main_modules = read_main_modules(tmp.path());
        assert_eq!(main_modules.len(), 1);
        assert!(
            main_modules[0]
                .depends
                .iter()
                .any(|d| d == "MikebomFixture.SampleLib"),
            "unlocked fallback: <PackageReference> feeds main-module depends; got {:?}",
            main_modules[0].depends
        );
    }

    #[test]
    fn main_module_unlocked_cpm_versionless_still_edges() {
        // T017 (US2) — CPM versionless <PackageReference> resolved via
        // Directory.Packages.props, no packages.lock.json. The
        // main-module's depends still includes the include-name.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "Directory.Packages.props",
            r#"<Project>
  <ItemGroup>
    <PackageVersion Include="MikebomFixture.SharedLib" Version="5.6.7" />
  </ItemGroup>
</Project>"#,
        );
        write(
            tmp.path(),
            "App.csproj",
            r#"<Project>
  <PropertyGroup>
    <Version>1.0.0</Version>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.SharedLib" />
  </ItemGroup>
</Project>"#,
        );
        let main_modules = read_main_modules(tmp.path());
        assert_eq!(main_modules.len(), 1);
        assert!(
            main_modules[0]
                .depends
                .iter()
                .any(|d| d == "MikebomFixture.SharedLib"),
            "unlocked CPM: main-module still edges to the include-name; got {:?}",
            main_modules[0].depends
        );
    }

    #[test]
    fn main_module_mixed_locked_and_unlocked_solution() {
        // T016 (US2) — two projects, one locked one not; each
        // main-module reaches its own direct deps, no crossover.
        let tmp = tempfile::tempdir().unwrap();
        // Locked project
        std::fs::create_dir_all(tmp.path().join("locked")).unwrap();
        write(
            &tmp.path().join("locked"),
            "Locked.csproj",
            r#"<Project>
  <PropertyGroup><Version>1.0.0</Version></PropertyGroup>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.LockedLib" />
  </ItemGroup>
</Project>"#,
        );
        write(
            &tmp.path().join("locked"),
            "packages.lock.json",
            r#"{"version":1,"dependencies":{"net8.0":{
                "MikebomFixture.LockedLib":{
                    "type":"Direct","resolved":"1.0.0","contentHash":"aaaa","dependencies":{}
                }
            }}}"#,
        );
        // Unlocked project
        std::fs::create_dir_all(tmp.path().join("unlocked")).unwrap();
        write(
            &tmp.path().join("unlocked"),
            "Unlocked.csproj",
            r#"<Project>
  <PropertyGroup><Version>2.0.0</Version></PropertyGroup>
  <ItemGroup>
    <PackageReference Include="MikebomFixture.UnlockedLib" Version="9.9.9" />
  </ItemGroup>
</Project>"#,
        );
        let mut main_modules = read_main_modules(tmp.path());
        main_modules.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(main_modules.len(), 2);
        // Locked project depends
        let locked = main_modules
            .iter()
            .find(|m| m.name == "Locked")
            .expect("Locked main-module missing");
        assert!(
            locked.depends.contains(&"MikebomFixture.LockedLib".to_string()),
            "Locked main-module misses lockfile dep; got {:?}",
            locked.depends
        );
        assert!(
            !locked.depends.contains(&"MikebomFixture.UnlockedLib".to_string()),
            "Locked main-module leaked unlocked dep; got {:?}",
            locked.depends
        );
        // Unlocked project depends
        let unlocked = main_modules
            .iter()
            .find(|m| m.name == "Unlocked")
            .expect("Unlocked main-module missing");
        assert!(
            unlocked
                .depends
                .contains(&"MikebomFixture.UnlockedLib".to_string()),
            "Unlocked main-module misses PackageReference dep; got {:?}",
            unlocked.depends
        );
        assert!(
            !unlocked
                .depends
                .contains(&"MikebomFixture.LockedLib".to_string()),
            "Unlocked main-module leaked locked dep; got {:?}",
            unlocked.depends
        );
    }
}
