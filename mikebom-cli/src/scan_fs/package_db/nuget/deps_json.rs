//! `.deps.json` reader — extracts NuGet package coordinates from
//! .NET runtime dependency sidecar files (milestone 129 US1A).
//!
//! `.deps.json` files are emitted by `dotnet publish` (and shipped
//! alongside the .NET SDK and runtime store) and carry the full
//! NuGet dependency graph that a managed application loads at
//! runtime. They are the ONLY ground-truth declaration of NuGet
//! dependencies in a production container image where `.csproj`
//! source manifests aren't shipped — the gap that left mikebom
//! emitting zero `pkg:nuget` PURLs on .NET-bearing images pre-129.
//!
//! Wire format (the subset we deserialize):
//!
//! ```text
//! {
//!   "runtimeTarget": {
//!     "name": ".NETCoreApp,Version=v8.0",
//!     "signature": ""
//!   },
//!   "libraries": {
//!     "MyApp/1.0.0":                              { "type": "project", ... },
//!     "Microsoft.AspNetCore.App.Ref/8.0.0":      { "type": "package", "sha512": "...", "path": "..." },
//!     "System.Text.Json/8.0.0":                  { "type": "package", "sha512": "...", "path": "..." }
//!   }
//! }
//! ```
//!
//! Only `libraries` map entries with `type: "package"` are emitted
//! as `pkg:nuget` components. `type: "project"` (first-party
//! assembly) and `type: "referenceassembly"` (compile-time-only
//! reference assemblies) are silently skipped per FR-009.
//!
//! The reader is offline-only (no network, no subprocess), respects
//! `--exclude-path`, and routes via the existing milestone-114
//! `safe_walk` helper. Malformed `.deps.json` files emit a single
//! `warn`-level log and skip; the surrounding scan continues per
//! FR-006.
//!
//! Zero new Cargo dependencies. Uses `serde_json` (already pervasive
//! across the workspace) for parse + `walk::safe_walk` for traversal.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::super::PackageDbEntry;
use crate::scan_fs::walk;

/// Subset of `.deps.json` we deserialize. Unknown top-level fields
/// (`targets`, `compilationOptions`, etc.) are silently ignored —
/// `serde` skips them by default.
#[derive(Debug, Deserialize)]
struct DotnetDepsJsonDocument {
    #[serde(rename = "runtimeTarget", default)]
    runtime_target: Option<RuntimeTarget>,
    #[serde(default)]
    libraries: BTreeMap<String, LibraryEntry>,
}

#[derive(Debug, Deserialize)]
struct RuntimeTarget {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct LibraryEntry {
    #[serde(rename = "type", default)]
    ty: String,
    #[serde(default)]
    path: Option<String>,
}

/// Walk `rootfs` for `*.deps.json` files and emit one `PackageDbEntry`
/// per `libraries` entry with `type: "package"`.
///
/// Returns empty when no `.deps.json` files are found — single-language
/// images (pure-Go, pure-Python) see this as a no-op and pay zero
/// SBOM bytes for the new reader.
pub fn read(
    rootfs: &Path,
    exclude_set: &super::super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    let deps_files = collect_deps_json_files(rootfs, exclude_set);
    if deps_files.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for path in deps_files {
        out.extend(read_one_deps_json(rootfs, &path));
    }
    out
}

/// Milestone 114 routing: walk via `safe_walk` for `*.deps.json`
/// extension. Skip default-descent skips (e.g. `.git/`, `target/`).
fn collect_deps_json_files(
    rootfs: &Path,
    exclude_set: &super::super::exclude_path::ExclusionSet,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = walk::WalkConfig {
        max_depth: 32,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            candidate
                .file_name()
                .and_then(|s| s.to_str())
                .map(super::super::project_roots::should_skip_default_descent)
                .unwrap_or(true)
        },
        exclude_set,
    };
    walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file() && is_deps_json(path) {
            out.push(path.to_path_buf());
        }
    });
    out
}

fn is_deps_json(path: &Path) -> bool {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(|name| name.ends_with(".deps.json"))
        .unwrap_or(false)
}

/// Parse one `.deps.json` and emit `PackageDbEntry`s for every
/// `type: "package"` library entry. Malformed files log `warn` and
/// emit nothing.
fn read_one_deps_json(rootfs: &Path, path: &Path) -> Vec<PackageDbEntry> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                err = %e,
                "failed to read .deps.json; skipping"
            );
            return Vec::new();
        }
    };
    let doc: DotnetDepsJsonDocument = match serde_json::from_slice(&bytes) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                err = %e,
                "failed to parse .deps.json; skipping"
            );
            return Vec::new();
        }
    };
    let runtime_target_name = doc
        .runtime_target
        .as_ref()
        .map(|rt| rt.name.clone())
        .filter(|s| !s.is_empty());

    let mut out = Vec::new();
    for (key, entry) in doc.libraries.iter() {
        match entry.ty.as_str() {
            "package" => {
                // good — emit
            }
            "project" | "referenceassembly" => {
                // FR-009: skip first-party / reference-only entries silently.
                continue;
            }
            other => {
                tracing::warn!(
                    path = %path.display(),
                    key = %key,
                    ty = %other,
                    "unknown .deps.json library type; skipping entry"
                );
                continue;
            }
        }
        let (name, version) = match split_library_key(key) {
            Some(pair) => pair,
            None => {
                tracing::warn!(
                    path = %path.display(),
                    key = %key,
                    "malformed .deps.json library key (expected `name/version`); skipping entry"
                );
                continue;
            }
        };
        let Some(purl) = super::build_nuget_purl(name, version) else {
            tracing::warn!(
                path = %path.display(),
                name = %name,
                version = %version,
                "nuget coord produced invalid PURL; skipping"
            );
            continue;
        };
        let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        extra_annotations.insert(
            "mikebom:source-mechanism".to_string(),
            serde_json::Value::String("dotnet-deps-json".to_string()),
        );
        if let Some(rt_name) = runtime_target_name.as_ref() {
            extra_annotations.insert(
                "mikebom:dotnet-runtime-target".to_string(),
                serde_json::Value::String(rt_name.clone()),
            );
        }
        // FR-007 declared-not-installed edge case: when `path` is
        // declared in the entry, check whether the assembly file
        // actually exists in the rootfs.
        if let Some(declared) = entry.path.as_ref().filter(|s| !s.is_empty()) {
            let assembly_present = check_assembly_present(rootfs, path, declared);
            if !assembly_present {
                extra_annotations.insert(
                    "mikebom:image-presence".to_string(),
                    serde_json::Value::String("declared-not-installed".to_string()),
                );
            }
        }

        out.push(PackageDbEntry {
            build_inclusion: None,
            purl,
            name: name.to_string(),
            version: version.to_string(),
            arch: None,
            source_path: path.to_string_lossy().to_string(),
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
            sbom_tier: Some("image".to_string()),
            shade_relocation: None,
            extra_annotations,
            binary_role: None,
        });
    }
    out
}

/// Split a `.deps.json` `libraries` map key `"<name>/<version>"`
/// into its two components. NuGet package names cannot contain `/`
/// (the spec forbids it); the first `/` is the delimiter.
fn split_library_key(key: &str) -> Option<(&str, &str)> {
    let idx = key.find('/')?;
    let name = &key[..idx];
    let version = &key[idx + 1..];
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name, version))
}

/// FR-007 edge case: when a `.deps.json` library entry declares a
/// `path` (e.g. `"runtimes/linux-x64/lib/net8.0/Foo.dll"`), check
/// whether the corresponding assembly exists in the image rootfs.
///
/// The declared path is relative to a `.NET runtime store` root,
/// which by convention is typically the `.deps.json`'s parent
/// directory OR `/usr/share/dotnet/shared/<framework>/<version>/`.
/// We probe both: present at either path → component is "installed";
/// present at neither → "declared-not-installed".
///
/// This is a best-effort check — when neither probe finds the file,
/// we emit the annotation without claiming the assembly is truly
/// missing (it may be in a path we don't probe). The annotation
/// signals "scanner couldn't confirm presence", which is the actionable
/// signal for downstream auditors.
fn check_assembly_present(rootfs: &Path, deps_json_path: &Path, declared: &str) -> bool {
    if let Some(parent) = deps_json_path.parent() {
        let candidate = parent.join(declared);
        if candidate.is_file() {
            return true;
        }
        // Also probe one level above (.deps.json sometimes lives in
        // a `tools/` or `tasks/` subdir while the assembly is in a
        // sibling).
        if let Some(grandparent) = parent.parent() {
            if grandparent.join(declared).is_file() {
                return true;
            }
        }
    }
    // Also probe under the rootfs's dotnet runtime store layout per
    // FR-012. The declared path inside a `.deps.json` from the
    // runtime store is relative to the store root.
    let runtime_store_root = rootfs.join("usr/share/dotnet");
    if runtime_store_root.is_dir() {
        let candidate = runtime_store_root.join(declared);
        if candidate.is_file() {
            return true;
        }
    }
    false
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::scan_fs::package_db::exclude_path::ExclusionSet;
    use tempfile::TempDir;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn empty_exclusions() -> ExclusionSet {
        ExclusionSet::new_empty()
    }

    #[test]
    fn well_formed_deps_json_emits_one_component_per_package_library() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let deps_json = root.join("app/MyApp.deps.json");
        write(&deps_json, r#"{
            "runtimeTarget": { "name": ".NETCoreApp,Version=v8.0" },
            "libraries": {
                "MyApp/1.0.0": { "type": "project" },
                "Microsoft.Extensions.Logging/8.0.0": { "type": "package", "sha512": "sha512-aaa" },
                "System.Text.Json/8.0.0": { "type": "package", "sha512": "sha512-bbb" }
            }
        }"#);
        let entries = read(root, &empty_exclusions());
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        // MyApp is skipped (type=project); only the two `package`
        // entries emit.
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert!(names.contains(&"Microsoft.Extensions.Logging"));
        assert!(names.contains(&"System.Text.Json"));
        // PURL shape sanity.
        let logging = entries.iter().find(|e| e.name == "Microsoft.Extensions.Logging").unwrap();
        assert_eq!(logging.purl.as_str(), "pkg:nuget/Microsoft.Extensions.Logging@8.0.0");
        // sbom-tier = "image" per FR-001.
        assert_eq!(logging.sbom_tier.as_deref(), Some("image"));
        // source-mechanism annotation per FR-002.
        let mech = logging.extra_annotations.get("mikebom:source-mechanism").unwrap();
        assert_eq!(mech.as_str(), Some("dotnet-deps-json"));
        // runtime-target annotation.
        let rt = logging.extra_annotations.get("mikebom:dotnet-runtime-target").unwrap();
        assert_eq!(rt.as_str(), Some(".NETCoreApp,Version=v8.0"));
    }

    #[test]
    fn project_type_libraries_are_skipped_silently() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let deps_json = root.join("app.deps.json");
        write(&deps_json, r#"{
            "libraries": {
                "App/1.0.0": { "type": "project" },
                "OnlyProject/0.0.0": { "type": "project" }
            }
        }"#);
        let entries = read(root, &empty_exclusions());
        assert!(entries.is_empty());
    }

    #[test]
    fn referenceassembly_type_is_skipped() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let deps_json = root.join("a.deps.json");
        write(&deps_json, r#"{
            "libraries": {
                "Microsoft.NETCore.App.Ref/8.0.0": { "type": "referenceassembly" },
                "System.Text.Json/8.0.0": { "type": "package" }
            }
        }"#);
        let entries = read(root, &empty_exclusions());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "System.Text.Json");
    }

    #[test]
    fn unknown_library_type_skipped_with_warn() {
        // We can't easily capture tracing output in a unit test
        // without scaffolding, but at minimum confirm the entry
        // doesn't emit.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let deps_json = root.join("a.deps.json");
        write(&deps_json, r#"{
            "libraries": {
                "Foo/1.0.0": { "type": "unknown-future-variant" }
            }
        }"#);
        let entries = read(root, &empty_exclusions());
        assert!(entries.is_empty());
    }

    #[test]
    fn malformed_json_emits_nothing_and_does_not_panic() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let deps_json = root.join("a.deps.json");
        write(&deps_json, r#"{"libraries": {truncated"#);
        let entries = read(root, &empty_exclusions());
        assert!(entries.is_empty());
    }

    #[test]
    fn malformed_library_key_skipped_silently() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let deps_json = root.join("a.deps.json");
        write(&deps_json, r#"{
            "libraries": {
                "no-slash-here": { "type": "package" },
                "Good.Pkg/1.0.0":  { "type": "package" }
            }
        }"#);
        let entries = read(root, &empty_exclusions());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Good.Pkg");
    }

    #[test]
    fn declared_not_installed_annotation_emitted_when_assembly_missing() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let deps_json = root.join("app/MyApp.deps.json");
        write(&deps_json, r#"{
            "libraries": {
                "Foo.Bar/1.0.0": { "type": "package", "path": "foo.bar/1.0.0/lib/net8.0/Foo.Bar.dll" }
            }
        }"#);
        let entries = read(root, &empty_exclusions());
        assert_eq!(entries.len(), 1);
        let mech = entries[0]
            .extra_annotations
            .get("mikebom:image-presence")
            .and_then(|v| v.as_str());
        assert_eq!(mech, Some("declared-not-installed"));
    }

    #[test]
    fn declared_assembly_present_no_image_presence_annotation() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let deps_json = root.join("app/MyApp.deps.json");
        write(&deps_json, r#"{
            "libraries": {
                "Foo.Bar/1.0.0": { "type": "package", "path": "Foo.Bar.dll" }
            }
        }"#);
        // Place the assembly in the same dir as deps.json.
        write(&root.join("app/Foo.Bar.dll"), "");
        let entries = read(root, &empty_exclusions());
        assert_eq!(entries.len(), 1);
        assert!(
            !entries[0].extra_annotations.contains_key("mikebom:image-presence"),
            "annotation should be absent when assembly is present"
        );
    }

    #[test]
    fn runtime_store_layout_discovered_and_parsed() {
        // FR-012: the runtime-store layout (used by the .NET SDK
        // and runtime images) places .deps.json files at paths
        // like `/usr/share/dotnet/sdk/8.0.127/dotnet-watch.deps.json`.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let deps_json = root.join(
            "usr/share/dotnet/sdk/8.0.127/DotnetTools/dotnet-watch/8.0.127/tools/net8.0/any/dotnet-watch.deps.json",
        );
        write(&deps_json, r#"{
            "libraries": {
                "Humanizer.Core/2.14.1": { "type": "package", "sha512": "sha512-xxx" }
            }
        }"#);
        let entries = read(root, &empty_exclusions());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Humanizer.Core");
        assert_eq!(entries[0].version, "2.14.1");
        assert_eq!(entries[0].sbom_tier.as_deref(), Some("image"));
    }

    #[test]
    fn no_deps_json_in_tree_emits_empty_vec() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(&root.join("README.md"), "no .deps.json here");
        let entries = read(root, &empty_exclusions());
        assert!(entries.is_empty());
    }
}
