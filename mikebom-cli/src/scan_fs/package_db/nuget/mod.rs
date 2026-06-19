//! NuGet source-tree reader (milestone 106 US4, closes #275).
//!
//! mikebom already encodes `pkg:nuget/<name>@<version>` PURLs and runs
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
//! 4. `unresolved` sentinel + `tracing::warn!` if none of the above
//!    resolves.
//!
//! Per FR-007b, `PrivateAssets="All"` / `IncludeAssets=...` /
//! `ExcludeAssets=...` map to `LifecycleScope::Build`, which flows
//! through the existing milestone-052 emission path to CDX
//! `scope: "excluded"` and SPDX 2.3 `BUILD_DEPENDENCY_OF`.
//!
//! Cross-platform (no `#[cfg(unix)]`); zero new Cargo dependencies.

mod csproj;
mod deps_json;
mod directory_packages_props;
mod packages_lock;
mod pe_clr;
mod private_assets;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mikebom_common::types::purl::{encode_purl_segment, Purl};

use super::PackageDbEntry;

const PROJECT_EXTENSIONS: &[&str] = &["csproj", "vbproj", "fsproj"];
const UNRESOLVED_VERSION_SENTINEL: &str = "unresolved";

/// Walk `rootfs` for NuGet project files and emit one `PackageDbEntry`
/// per resolved `<PackageReference>` (or `packages.lock.json` entry).
/// Empty when no project files are found.
pub fn read(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    let project_files = collect_project_files(rootfs, exclude_set);
    let mut out = Vec::new();
    for project_path in project_files {
        out.extend(read_one_project(rootfs, &project_path));
    }
    // Milestone 129 US1A: also walk the rootfs for `.deps.json`
    // files (the .NET runtime dependency sidecar emitted by
    // `dotnet publish` and shipped throughout the SDK / runtime
    // store layouts). This is the path that closes the 1,489-package
    // nuget gap surfaced by the remediation-planner image audit
    // where no source manifests are present.
    out.extend(deps_json::read(rootfs, exclude_set));
    // Milestone 130 US3: walk the rootfs for `*.dll` files carrying
    // CLR managed-assembly metadata (PE files with a non-zero
    // IMAGE_OPTIONAL_HEADER.DataDirectory[14] / COR20 header). Closes
    // the .NET reference-assemblies + MSBuild-tasks-DLL gap on
    // images that ship the dotnet SDK or runtime store. Resource
    // assemblies (de/fr/ja/... resources.dll) dedup per FR-024 via
    // an intra-reader culture-set accumulator.
    out.extend(pe_clr::read(rootfs, exclude_set));
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
    let project_references = csproj::parse_project_file(project_path);
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

    // Build a (name -> source-paths) accumulator so the same
    // (name, version) coord collected from .csproj + props +
    // packages.lock.json merges into a single component with a
    // comma-joined `mikebom:source-files` annotation.
    let mut acc: BTreeMap<(String, String), AccEntry> = BTreeMap::new();

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
        //   lockfile (any framework) > inline Version= > CPM map > unresolved
        let lock_resolved = lockfile.as_ref().and_then(|f| {
            f.dependencies
                .values()
                .filter_map(|fw| fw.get(&r.include))
                .map(|p| p.resolved.clone())
                .find(|v| !v.is_empty())
        });
        let resolved_version = lock_resolved
            .or_else(|| r.version.clone().filter(|v| !v.is_empty()))
            .or_else(|| cpm_map.get(&r.include).cloned())
            .unwrap_or_else(|| {
                tracing::warn!(
                    project = %project_path.display(),
                    package = %r.include,
                    "<PackageReference> version unresolved (no Version=, no CPM, no lockfile entry)"
                );
                UNRESOLVED_VERSION_SENTINEL.to_string()
            });

        let key = (r.include.clone(), resolved_version.clone());
        let entry = acc.entry(key).or_default();
        entry.lifecycle_scope = entry.lifecycle_scope.or(lifecycle_scope);
        entry.sources.insert(project_path.to_path_buf());
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
    // tagged with `mikebom:source-type: "transitive"`.
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
                let key = (name.clone(), pkg.resolved.clone());
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
    for ((name, version), acc_entry) in acc {
        let Some(purl) = build_nuget_purl(&name, &version) else {
            tracing::warn!(
                package = %name,
                version = %version,
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
                "mikebom:source-files".to_string(),
                serde_json::Value::String(joined),
            );
        }

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
            version,
            arch: None,
            source_path: primary_source,
            depends,
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: acc_entry.lifecycle_scope,
            requirement_range: None,
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
            sbom_tier: Some("source".to_string()),
            shade_relocation: None,
            extra_annotations,
            binary_role: None,
        });
    }
    out
}

#[derive(Default)]
struct AccEntry {
    lifecycle_scope: Option<mikebom_common::resolution::LifecycleScope>,
    source_type: Option<String>,
    /// Source files contributing to this coord. `BTreeSet` for
    /// deterministic comma-join ordering.
    sources: BTreeSet<PathBuf>,
}

pub(super) fn build_nuget_purl(name: &str, version: &str) -> Option<Purl> {
    Purl::new(&format!(
        "pkg:nuget/{}@{}",
        encode_purl_segment(name),
        encode_purl_segment(version),
    ))
    .ok()
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
    use mikebom_common::resolution::LifecycleScope;

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
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
        let entries = read(tmp.path(), &Default::default());
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
        let entries = read(tmp.path(), &Default::default());
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
            .get("mikebom:source-files")
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
        let entries = read(tmp.path(), &Default::default());
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
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 1);
        assert!(matches!(
            entries[0].lifecycle_scope,
            Some(LifecycleScope::Build)
        ));
    }

    #[test]
    fn unresolved_version_uses_sentinel_and_warns() {
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
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].version, "unresolved");
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
        let entries = read(tmp.path(), &Default::default());
        assert_eq!(entries.len(), 2);
        let names: BTreeSet<_> = entries.iter().map(|e| e.name.clone()).collect();
        assert!(names.contains("MikebomFixture.VbLib"));
        assert!(names.contains("MikebomFixture.FsLib"));
    }
}
