//! `bun.lock` parser — milestone 106 US2 (issue #278).
//!
//! Parses [Bun](https://bun.sh/)'s text-format lockfile (JSONC; binary
//! `bun.lockb` is explicitly out of scope per the issue). Sibling to
//! `package_lock.rs` (npm v2/v3) and `pnpm_lock.rs` (pnpm). Invoked
//! from [`super::read`] per-project-root after the existing
//! lockfile readers; tier-A authority same as npm + pnpm.
//!
//! Schema (Bun 1.2+):
//!
//! ```jsonc
//! // bun: lockfileVersion: 1
//! {
//!   "lockfileVersion": 1,
//!   "workspaces": {
//!     "": { "name": "root-name", "dependencies": {...} },
//!     "packages/web": { "name": "@my/web", "dependencies": {"@my/shared": "workspace:*"} },
//!     "packages/shared": { "name": "@my/shared" }
//!   },
//!   "packages": {
//!     "lodash": ["lodash@4.17.21", "sha512-..."],
//!     "@my/web": ["@my/web@workspace:packages/web"],
//!     "@my/shared": ["@my/shared@workspace:packages/shared"]
//!   },
//!   "overrides": { "lodash": "4.17.21" }
//! }
//! ```
//!
//! Per the Clarification Q1 of milestone 106, workspace handling
//! emits one main-module per workspace member + a synthetic
//! workspace-root + intra-workspace dependency edges.

use std::collections::HashSet;
use std::path::Path;

use super::super::workspace::{synthesize_workspace_root, workspace_root_name};
use super::super::PackageDbEntry;
use super::build_npm_purl;

/// Placeholder version used when a workspace member's `package.json`
/// is missing/unreadable or has no `version` field. Keeps the
/// resulting PURL well-formed without pretending to know the real
/// version. Workspace members are unversioned-by-design in many
/// monorepo setups, so this is a deliberate sentinel.
const WORKSPACE_MEMBER_VERSION_PLACEHOLDER: &str = "0.0.0";

/// Read `<rootfs>/bun.lock` if present. Returns None when absent or
/// unparseable. Pre-processes the file through the JSONC stripper to
/// remove `//` line comments and `/* */` block comments before
/// handing the result to `serde_json::from_str` — every real-world
/// `bun.lock` has at least the top-of-file `// bun: lockfileVersion: 1`
/// marker.
pub(super) fn read_bun_lock(
    rootfs: &Path,
    _include_dev: bool,
) -> Option<Vec<PackageDbEntry>> {
    let path = rootfs.join("bun.lock");
    let text = std::fs::read_to_string(&path).ok()?;
    let stripped = super::jsonc::strip_comments(&text);
    let parsed: serde_json::Value = match serde_json::from_str(&stripped) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "bun.lock JSONC parse failed; skipping (FR-010 warn-and-continue)"
            );
            return None;
        }
    };
    let source_path = path.to_string_lossy().into_owned();
    Some(parse_bun_lock(&parsed, &source_path, rootfs))
}

/// Parse an already-deserialized `bun.lock` JSON value. Public-in-
/// module for unit testing. `rootfs` is used to read workspace member
/// `package.json` files for the version field; tests can pass a
/// tempdir.
pub(crate) fn parse_bun_lock(
    root: &serde_json::Value,
    source_path: &str,
    rootfs: &Path,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();

    // Extract `overrides` map; entries here win over any version
    // declared in `packages`. We apply at registry-package emission
    // time (the un-overridden version is NOT also emitted).
    let overrides: std::collections::BTreeMap<String, String> = root
        .get("overrides")
        .and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    // Step 1: detect workspace from the `workspaces` map. Skip the
    // root entry (key="") — capture its `name` field for the
    // synthetic workspace-root component, and record each member's
    // name in a set so we can both (a) tag them with
    // component-role: "main-module" and (b) skip them in the
    // packages-map walk below.
    let mut workspace_root_name_field: Option<String> = None;
    let mut workspace_member_names: HashSet<String> = HashSet::new();

    if let Some(workspaces) = root.get("workspaces").and_then(|v| v.as_object()) {
        for (path, info) in workspaces {
            let name = info.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
            if path.is_empty() {
                workspace_root_name_field = name;
                continue;
            }
            let Some(member_name) = name else { continue };
            workspace_member_names.insert(member_name.clone());

            // Read the member's package.json for the version field.
            // Absent / unreadable → use the placeholder.
            let member_pkg_json_path = rootfs.join(path).join("package.json");
            let version = std::fs::read_to_string(&member_pkg_json_path)
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| {
                    v.get("version")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| WORKSPACE_MEMBER_VERSION_PLACEHOLDER.to_string());

            // Intra-workspace edges: walk the member's `dependencies`
            // field; any value starting with `workspace:` is a
            // sibling-workspace dep. Record its NAME (the key) in
            // `depends`.
            let depends: Vec<String> = info
                .get("dependencies")
                .and_then(|v| v.as_object())
                .map(|m| {
                    m.iter()
                        .filter(|(_, v)| {
                            v.as_str()
                                .map(|s| s.starts_with("workspace:"))
                                .unwrap_or(false)
                        })
                        .map(|(k, _)| k.clone())
                        .collect()
                })
                .unwrap_or_default();

            let Some(purl) = build_npm_purl(&member_name, &version) else {
                tracing::warn!(
                    workspace_member = %member_name,
                    "bun workspace member produced invalid PURL; skipping"
                );
                continue;
            };

            let mut extra: std::collections::BTreeMap<String, serde_json::Value> =
                Default::default();
            extra.insert(
                "mikebom:component-role".to_string(),
                serde_json::Value::String("main-module".to_string()),
            );

            out.push(PackageDbEntry {
                build_inclusion: None,
                purl,
                name: member_name.clone(),
                version,
                arch: None,
                source_path: source_path.to_string(),
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
                extra_annotations: extra,
                binary_role: None,
            });
        }
    }

    // Step 2: walk the `packages` map. Each value is an array whose
    // FIRST element is the canonical `<name>@<source-spec>` string.
    // Source-specs:
    //   - Semver (e.g. `4.17.21`)            → registry package
    //   - `workspace:<path>`                 → workspace member (skip — already emitted in step 1)
    //   - `https://...` / `git+...`          → URL / git source (treat as registry for now)
    if let Some(packages) = root.get("packages").and_then(|v| v.as_object()) {
        for (_key, value) in packages {
            let Some(spec) = value
                .as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
            else {
                continue;
            };
            // Split on the rightmost `@` so scoped names like
            // `@types/node@22.5.0` parse correctly.
            let Some((name, source_spec)) = spec.rsplit_once('@') else {
                continue;
            };
            if name.is_empty() || source_spec.is_empty() {
                continue;
            }
            // Workspace entries already emitted in step 1.
            if source_spec.starts_with("workspace:") {
                continue;
            }
            // Skip workspace members that also appear in packages
            // (some bun versions duplicate them).
            if workspace_member_names.contains(name) {
                continue;
            }

            // Override resolution: if an `overrides` entry names this
            // package, the override version wins. The un-overridden
            // version is NOT also emitted.
            let resolved_version = overrides
                .get(name)
                .cloned()
                .unwrap_or_else(|| source_spec.to_string());

            let Some(purl) = build_npm_purl(name, &resolved_version) else {
                tracing::warn!(
                    package = %name,
                    version = %resolved_version,
                    "bun.lock packages entry produced invalid PURL; skipping"
                );
                continue;
            };

            out.push(PackageDbEntry {
                build_inclusion: None,
                purl,
                name: name.to_string(),
                version: resolved_version,
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
                extra_annotations: Default::default(),
                binary_role: None,
            });
        }
    }

    // Step 3: synthesize the workspace-root component when a
    // workspace was detected. Workspace-root's `depends` lists each
    // member by name, producing dependsOn edges to each member in
    // the emitted SBOM (per FR-015).
    if !workspace_member_names.is_empty() {
        let root_pkg_json = rootfs.join("package.json");
        let root_name = workspace_root_name(workspace_root_name_field.as_deref());
        if let Some(mut root_entry) = synthesize_workspace_root(&root_name, &root_pkg_json) {
            root_entry.depends = workspace_member_names.iter().cloned().collect();
            out.push(root_entry);
        }
    }

    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn emits_basic_npm_components() {
        let src = r#"// bun: lockfileVersion: 1
{
  "lockfileVersion": 1,
  "workspaces": { "": { "name": "test-app" } },
  "packages": {
    "lodash": ["lodash@4.17.21", "sha512-aaa"],
    "express": ["express@4.18.2", "sha512-bbb"]
  }
}"#;
        let stripped = super::super::jsonc::strip_comments(src);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", tmp.path());
        // No workspace members declared → no workspace-root synthesis.
        // Just the 2 registry packages.
        assert_eq!(entries.len(), 2, "got: {entries:?}");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"lodash"));
        assert!(names.contains(&"express"));
        let lodash = entries.iter().find(|e| e.name == "lodash").unwrap();
        assert_eq!(lodash.purl.as_str(), "pkg:npm/lodash@4.17.21");
    }

    #[test]
    fn encodes_scoped_packages() {
        let src = r#"// bun: lockfileVersion: 1
{
  "lockfileVersion": 1,
  "workspaces": { "": { "name": "test" } },
  "packages": {
    "@types/node": ["@types/node@22.5.0", "sha512-..."]
  }
}"#;
        let stripped = super::super::jsonc::strip_comments(src);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", tmp.path());
        assert_eq!(entries.len(), 1);
        // PURL must URL-encode the `@` in the scope segment per PURL spec.
        assert_eq!(entries[0].purl.as_str(), "pkg:npm/%40types/node@22.5.0");
    }

    #[test]
    fn override_version_wins() {
        // overrides map sets lodash to 4.17.21; the packages-map entry
        // has 4.17.20 (different version). Override wins; un-overridden
        // version is NOT emitted as a separate component.
        let src = r#"// bun: lockfileVersion: 1
{
  "lockfileVersion": 1,
  "workspaces": { "": { "name": "test" } },
  "packages": {
    "lodash": ["lodash@4.17.20", "sha512-..."]
  },
  "overrides": {
    "lodash": "4.17.21"
  }
}"#;
        let stripped = super::super::jsonc::strip_comments(src);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", tmp.path());
        assert_eq!(entries.len(), 1, "got: {entries:?}");
        assert_eq!(entries[0].version, "4.17.21");
        assert_eq!(entries[0].purl.as_str(), "pkg:npm/lodash@4.17.21");
    }

    #[test]
    fn emits_workspace_shape() {
        // Synthetic Bun workspace: 2 members (@my/web, @my/shared) +
        // 1 external dep (lodash). @my/web depends on @my/shared via
        // workspace:* source-spec. Expected output:
        //   - workspace-root component (pkg:generic/my-monorepo)
        //   - @my/web component (main-module + dependsOn @my/shared)
        //   - @my/shared component (main-module)
        //   - lodash component (no role)
        let tmp = tempfile::tempdir().unwrap();
        let rootfs = tmp.path();
        write(
            &rootfs.join("package.json"),
            r#"{ "name": "my-monorepo", "workspaces": ["packages/*"] }"#,
        );
        write(
            &rootfs.join("packages/web/package.json"),
            r#"{ "name": "@my/web", "version": "1.0.0", "dependencies": { "@my/shared": "workspace:*", "lodash": "^4.17.21" } }"#,
        );
        write(
            &rootfs.join("packages/shared/package.json"),
            r#"{ "name": "@my/shared", "version": "0.5.0" }"#,
        );
        let lockfile_src = r#"// bun: lockfileVersion: 1
{
  "lockfileVersion": 1,
  "workspaces": {
    "": { "name": "my-monorepo" },
    "packages/web": {
      "name": "@my/web",
      "dependencies": {
        "@my/shared": "workspace:*",
        "lodash": "^4.17.21"
      }
    },
    "packages/shared": {
      "name": "@my/shared"
    }
  },
  "packages": {
    "lodash": ["lodash@4.17.21", "sha512-..."],
    "@my/web": ["@my/web@workspace:packages/web"],
    "@my/shared": ["@my/shared@workspace:packages/shared"]
  }
}"#;
        let stripped = super::super::jsonc::strip_comments(lockfile_src);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", rootfs);

        // 2 members + 1 external + 1 synthetic workspace-root = 4
        assert_eq!(entries.len(), 4, "got: {entries:?}");

        let web = entries.iter().find(|e| e.name == "@my/web").unwrap();
        assert_eq!(web.purl.as_str(), "pkg:npm/%40my/web@1.0.0");
        assert_eq!(
            web.extra_annotations
                .get("mikebom:component-role")
                .and_then(|v| v.as_str()),
            Some("main-module"),
        );
        assert_eq!(web.depends, vec!["@my/shared".to_string()]);

        let shared = entries.iter().find(|e| e.name == "@my/shared").unwrap();
        assert_eq!(shared.purl.as_str(), "pkg:npm/%40my/shared@0.5.0");
        assert_eq!(
            shared
                .extra_annotations
                .get("mikebom:component-role")
                .and_then(|v| v.as_str()),
            Some("main-module"),
        );

        let lodash = entries.iter().find(|e| e.name == "lodash").unwrap();
        assert!(!lodash
            .extra_annotations
            .contains_key("mikebom:component-role"));

        let ws_root = entries
            .iter()
            .find(|e| e.purl.as_str() == "pkg:generic/my-monorepo")
            .expect("workspace-root component must be emitted");
        assert_eq!(
            ws_root
                .extra_annotations
                .get("mikebom:component-role")
                .and_then(|v| v.as_str()),
            Some("workspace-root"),
        );
        let mut depends_sorted = ws_root.depends.clone();
        depends_sorted.sort();
        assert_eq!(
            depends_sorted,
            vec!["@my/shared".to_string(), "@my/web".to_string()]
        );
    }

    #[test]
    fn workspace_member_uses_placeholder_when_no_pkg_json() {
        // Edge: workspace member declared in bun.lock but its
        // package.json is missing/unreadable. Use placeholder
        // "0.0.0" version rather than panicking.
        let tmp = tempfile::tempdir().unwrap();
        let rootfs = tmp.path();
        let lockfile_src = r#"{
  "lockfileVersion": 1,
  "workspaces": {
    "": { "name": "root" },
    "packages/orphan": { "name": "@my/orphan" }
  }
}"#;
        let parsed: serde_json::Value = serde_json::from_str(lockfile_src).unwrap();
        let entries = parse_bun_lock(&parsed, "/tmp/bun.lock", rootfs);
        let orphan = entries.iter().find(|e| e.name == "@my/orphan").unwrap();
        assert_eq!(orphan.version, "0.0.0");
        assert_eq!(orphan.purl.as_str(), "pkg:npm/%40my/orphan@0.0.0");
    }
}
