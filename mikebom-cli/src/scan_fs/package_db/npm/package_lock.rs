//! package-lock.json v2/v3 parser.

use std::path::Path;


use super::super::PackageDbEntry;
use super::{build_npm_purl, NpmIntegrity};

pub(super) fn read_package_lock(rootfs: &Path, include_dev: bool) -> Option<Vec<PackageDbEntry>> {
    let path = rootfs.join("package-lock.json");
    let text = std::fs::read_to_string(&path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&text).ok()?;
    let source_path = path.to_string_lossy().into_owned();
    let out = parse_package_lock(&parsed, &source_path, include_dev);
    if out.is_empty() { None } else { Some(out) }
}

/// Parse a deserialised `package-lock.json` v2/v3 document. Iterates
/// the top-level `packages` object; skips the root entry (`""`) and
/// any workspace sub-roots (detected via `link: true`).
pub(crate) fn parse_package_lock(
    root: &serde_json::Value,
    source_path: &str,
    include_dev: bool,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    let Some(packages) = root.get("packages").and_then(|v| v.as_object()) else {
        return out;
    };

    // Sorted for determinism.
    let mut keys: Vec<&String> = packages.keys().collect();
    keys.sort();

    // Issue #262: build a (path_key → version) index up front so each
    // entry's depends-resolution can look up nested children. npm
    // installs the same package at multiple paths when version
    // conflicts force it; `node_modules/foo/node_modules/bar` is
    // bar's NESTED install (foo's specific version), distinct from
    // the HOISTED `node_modules/bar`. Without nested-aware
    // resolution, bare-name dep strings always match the hoisted
    // version, leaving nested installs as orphans.
    //
    // The index is filtered to entries that WILL be emitted as
    // components — same dev/optional/link filters as the main loop
    // below. Without this filter, the parser would emit
    // version-pinned dep strings like "fsevents 2.3.0" for nested
    // entries that downstream get filtered out (e.g. optional:true),
    // and the edge resolver would drop the edge as dangling.
    let mut path_versions: std::collections::HashMap<&str, &str> =
        std::collections::HashMap::with_capacity(keys.len());
    for &k in &keys {
        let Some(entry) = packages[k].as_object() else {
            continue;
        };
        if entry.get("link").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }
        let entry_is_dev = entry
            .get("dev")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let entry_is_optional = entry
            .get("optional")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !include_dev && (entry_is_dev || entry_is_optional) {
            continue;
        }
        if let Some(v) = entry.get("version").and_then(|x| x.as_str()) {
            if !v.is_empty() {
                path_versions.insert(k.as_str(), v);
            }
        }
    }

    for path_key in keys {
        if path_key.is_empty() {
            // Root project entry — skip.
            continue;
        }
        let entry = &packages[path_key];
        let Some(tbl) = entry.as_object() else { continue };

        // Workspace link — symlink to a sibling workspace, not a
        // published package. Skip.
        if tbl.get("link").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }

        // `dev: true` / `optional: true` propagate through the nested
        // tree. Filter at source before the caller's dedup pass.
        let is_dev = tbl
            .get("dev")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let is_optional = tbl
            .get("optional")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !include_dev && (is_dev || is_optional) {
            continue;
        }

        // Version is required.
        let version = tbl
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if version.is_empty() {
            continue;
        }

        // Name is either declared in the entry or derived from the
        // path key: last `node_modules/<scope>?/<name>` segment.
        let name = tbl
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| derive_name_from_path_key(path_key));
        if name.is_empty() {
            continue;
        }

        let Some(purl) = build_npm_purl(&name, &version) else {
            continue;
        };

        let hashes = tbl
            .get("integrity")
            .and_then(|v| v.as_str())
            .and_then(NpmIntegrity::parse)
            .and_then(|i| i.to_content_hash())
            .map(|h| vec![h])
            .unwrap_or_default();

        // Issue #262 + follow-up: resolve each declared dep against
        // the nested-path tree first, falling back to bare-name.
        // When a parent at `node_modules/<parent>` has a nested
        // child at `node_modules/<parent>/node_modules/<dep>`, emit
        // the version-qualified `<dep> <version>` form so the edge
        // resolver in `scan_fs/mod.rs` matches the nested install
        // rather than the hoisted version. Bare-name form is kept
        // for deps that resolve to the hoisted version (no nested
        // child exists for this parent). Mirrors cargo's milestone-
        // 087 disambiguation pattern (issue #172).
        //
        // Walks ALL four standard npm dep sections — `dependencies`,
        // `devDependencies`, `peerDependencies`,
        // `optionalDependencies`. The original PR #263 walked only
        // `dependencies`, leaving packages declared via peer/optional
        // sections orphan when they had nested installs. npm's
        // resolver hoists or nests packages from any of the four
        // sections uniformly — peer/optional declarations result in
        // the same `node_modules/<parent>/node_modules/<dep>` install
        // shape as regular `dependencies`, and the parent's
        // `package.json` is the authoritative source for the
        // version-spec the consumer needs.
        //
        // Deduplication: a single dep CAN appear in multiple
        // sections (e.g., peer + optional, or dep + dev) — the
        // version pin is the same in either case (the nested
        // install path is shared). A HashSet collects unique
        // (name → version-pinned-string) pairs.
        let mut depends_set: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        // Skip `peerDependencies` — semantically declarative ("the
        // consumer should have X installed"), not an install
        // relationship. npm v7+ auto-installs peers as a convenience,
        // but the SBOM `dependsOn` / `DEPENDS_ON` slot means "X
        // depends on Y" not "X expects Y to be present." Trivy and
        // syft also skip peer-edges. If a peer-installed package is
        // genuinely orphan in the dep graph, the orphan signal is
        // the correct one — the consumer (root or a direct
        // requirer) should declare the dep explicitly.
        for section in &[
            "dependencies",
            "devDependencies",
            "optionalDependencies",
        ] {
            if let Some(deps) = tbl.get(*section).and_then(|v| v.as_object()) {
                for dep_name in deps.keys() {
                    // Walk up the node_modules tree from this entry,
                    // mirroring npm's resolution algorithm: a package
                    // at `<...>/<parent>/.../X` resolves a declared
                    // dep `Y` by checking
                    // `<parent>/.../X/node_modules/Y` first, then
                    // walking up to `<parent>/.../node_modules/Y`,
                    // etc., until the top-level hoisted
                    // `node_modules/Y`. Whichever level finds Y
                    // first wins. Pre-fix only the immediate child
                    // path was checked, with bare-name fallback that
                    // the edge resolver's last-write-wins lookup
                    // could resolve to the wrong version when
                    // multiple installs of the same package exist.
                    let resolved = resolve_dep_via_node_modules_walk(
                        path_key,
                        dep_name,
                        &path_versions,
                    )
                    .map(|version| format!("{dep_name} {version}"))
                    .unwrap_or_else(|| dep_name.clone());
                    // BTreeMap preserves deterministic order + dedup.
                    // If the same dep_name appears in multiple
                    // sections, the version-pinned form (if any)
                    // wins via the existing-or-insert pattern:
                    // version-pinned strings contain a space, bare
                    // names don't — prefer the more specific form.
                    use std::collections::btree_map::Entry;
                    match depends_set.entry(dep_name.clone()) {
                        Entry::Vacant(v) => {
                            v.insert(resolved);
                        }
                        Entry::Occupied(mut o) => {
                            // Prefer version-pinned over bare-name.
                            if o.get().chars().filter(|c| *c == ' ').count() == 0
                                && resolved.contains(' ')
                            {
                                *o.get_mut() = resolved;
                            }
                        }
                    }
                }
            }
        }
        let depends: Vec<String> = depends_set.into_values().collect();

        out.push(PackageDbEntry {
            build_inclusion: None,
            purl,
            name,
            version,
            arch: None,
            source_path: source_path.to_string(),
            depends,
            maintainer: None,
            licenses: tbl
                .get("license")
                .and_then(|v| v.as_str())
                .and_then(|s| {
                    mikebom_common::types::license::SpdxExpression::try_canonical(s.trim()).ok()
                })
                .into_iter()
                .collect(),
            lifecycle_scope: if is_dev { Some(mikebom_common::resolution::LifecycleScope::Development) } else { Some(mikebom_common::resolution::LifecycleScope::Runtime) },
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
            hashes,
            sbom_tier: Some("source".to_string()),
            shade_relocation: None,
            extra_annotations: Default::default(),
            binary_role: None,
        });
    }

    // Issue #262 (sub-bug): dedup same-PURL entries preferring
    // Runtime over Development scope. The same package version can
    // appear at multiple install paths in a `node_modules/` tree —
    // typically one dev-scoped path (e.g.,
    // `node_modules/@babel/core/node_modules/ms`) and one runtime-
    // scoped path (e.g., `node_modules/send/node_modules/ms`). With
    // `--include-dev` enabled, the parser previously emitted ALL
    // such entries and the upstream `seen_purls` dedup at
    // `mod.rs:118` kept whichever came FIRST alphabetically by
    // path_key — typically a dev entry (scope-prefixed paths sort
    // before non-prefixed). This mis-tagged shared packages as Dev
    // and (pre-#262 fix) didn't matter because the dev-tagged
    // entries were orphans. After the #262 nested-version-pinning
    // fix, edges actually land on these dedup'd components, so the
    // Dev tag triggers `DEV_DEPENDENCY_OF` rewriting — which is
    // wrong if the package is also used at runtime by other paths.
    //
    // Rule: keep the Runtime variant when both Dev and Runtime
    // variants of the same PURL are present. If only one variant
    // exists, keep it. Stable across ordering — preserves the
    // existing seen_purls "first wins" semantic for the same-scope
    // case.
    let mut by_purl: std::collections::HashMap<String, usize> =
        std::collections::HashMap::with_capacity(out.len());
    let mut keep: Vec<bool> = vec![true; out.len()];
    for (idx, entry) in out.iter().enumerate() {
        let purl_str = entry.purl.as_str().to_string();
        use std::collections::hash_map::Entry;
        match by_purl.entry(purl_str) {
            Entry::Vacant(v) => {
                v.insert(idx);
            }
            Entry::Occupied(mut o) => {
                let existing_idx = *o.get();
                use mikebom_common::resolution::LifecycleScope;
                let existing_is_runtime = matches!(
                    out[existing_idx].lifecycle_scope,
                    Some(LifecycleScope::Runtime)
                );
                let new_is_runtime =
                    matches!(entry.lifecycle_scope, Some(LifecycleScope::Runtime));
                if new_is_runtime && !existing_is_runtime {
                    // Promote the runtime variant; drop the dev one
                    // from the existing slot.
                    keep[existing_idx] = false;
                    *o.get_mut() = idx;
                } else {
                    // Existing wins (either also runtime, or both
                    // dev — first-by-iteration semantic preserved).
                    keep[idx] = false;
                }
            }
        }
    }
    let mut deduped: Vec<PackageDbEntry> = Vec::with_capacity(out.len());
    for (idx, entry) in out.into_iter().enumerate() {
        if keep[idx] {
            deduped.push(entry);
        }
    }
    deduped
}

/// Derive a package name from a `packages` path key like
/// `node_modules/foo` or `node_modules/@scope/bar` or deeply nested
/// `node_modules/foo/node_modules/bar`. The real name is always the
/// segment (or scope+segment) that follows the LAST `node_modules/`.
/// Walk up the `node_modules/<...>` path tree from `parent_path_key`,
/// returning the version of `dep_name` at the closest ancestor that
/// has a matching `node_modules/<dep_name>` install. Mirrors npm's
/// actual resolution algorithm.
///
/// For example, given:
/// - `parent_path_key = "node_modules/foo/node_modules/bar"`
/// - `dep_name = "baz"`
///
/// The lookup order is:
/// 1. `node_modules/foo/node_modules/bar/node_modules/baz`
/// 2. `node_modules/foo/node_modules/baz`
/// 3. `node_modules/baz` (hoisted)
///
/// Returns `None` if `dep_name` isn't installed at any level. This
/// is rare in well-formed lockfiles but can happen when a dep is
/// declared but not actually resolved (e.g., `optionalDependencies`
/// that failed install).
fn resolve_dep_via_node_modules_walk<'a>(
    parent_path_key: &str,
    dep_name: &str,
    path_versions: &std::collections::HashMap<&'a str, &'a str>,
) -> Option<&'a str> {
    let mut prefix = parent_path_key;
    loop {
        let candidate = format!("{prefix}/node_modules/{dep_name}");
        if let Some(version) = path_versions.get(candidate.as_str()) {
            return Some(*version);
        }
        // Walk up: find the last "/node_modules/" segment; the
        // ancestor's path_key is the prefix BEFORE that occurrence.
        // When no "/node_modules/" remains, we've passed the root
        // package's own dir — try the top-level hoisted location
        // `node_modules/<dep>` as the final step.
        if let Some(idx) = prefix.rfind("/node_modules/") {
            prefix = &prefix[..idx];
        } else {
            // Top-level hoisted lookup. `prefix` at this point is
            // something like `node_modules/<pkg>` (or `node_modules`
            // for the unusual case of a recursive call up from a
            // top-level dir). Try `node_modules/<dep>`.
            let top = format!("node_modules/{dep_name}");
            return path_versions.get(top.as_str()).copied();
        }
    }
}

fn derive_name_from_path_key(key: &str) -> String {
    let idx = match key.rfind("node_modules/") {
        Some(i) => i + "node_modules/".len(),
        None => return String::new(),
    };
    let tail = &key[idx..];
    // Scoped: "@scope/name" — two segments.
    if tail.starts_with('@') {
        let parts: Vec<&str> = tail.splitn(3, '/').collect();
        if parts.len() >= 2 {
            return format!("{}/{}", parts[0], parts[1]);
        }
    }
    // Unscoped: single segment.
    tail.split('/').next().unwrap_or("").to_string()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::super::{read, NpmError};
    use super::*;
    #[test]
    fn derive_name_handles_flat_and_scoped_and_nested() {
        assert_eq!(derive_name_from_path_key("node_modules/foo"), "foo");
        assert_eq!(
            derive_name_from_path_key("node_modules/@scope/bar"),
            "@scope/bar"
        );
        assert_eq!(
            derive_name_from_path_key("node_modules/foo/node_modules/bar"),
            "bar"
        );
        assert_eq!(
            derive_name_from_path_key("node_modules/foo/node_modules/@scope/baz"),
            "@scope/baz"
        );
    }

    #[test]
    fn package_lock_v3_basic() {
        let src = serde_json::json!({
            "name": "myapp",
            "lockfileVersion": 3,
            "packages": {
                "": { "name": "myapp", "version": "0.1.0" },
                "node_modules/lodash": {
                    "version": "4.17.21",
                    "integrity": "sha512-MJ7MSJwS1utMxA9QyQLytNDtd+5RGnx+7fIK+4qg9hvLABzzXAIaFMqoD6YFUYaCQPkMInyGdz6TQEsE7bPdCg==",
                    "license": "MIT"
                },
                "node_modules/eslint": {
                    "version": "8.0.0",
                    "dev": true
                }
            }
        });
        let out = parse_package_lock(&src, "/package-lock.json", false);
        assert_eq!(out.len(), 1, "dev entry filtered by default");
        assert_eq!(out[0].name, "lodash");
        assert_eq!(out[0].version, "4.17.21");
        assert_eq!(out[0].sbom_tier.as_deref(), Some("source"));
        assert_eq!(out[0].lifecycle_scope, Some(mikebom_common::resolution::LifecycleScope::Runtime));
        // Hash extraction is covered by `integrity_round_trips_to_content_hash`;
        // once PackageDbEntry gains a hashes field we re-assert here.
    }

    #[test]
    fn package_lock_v3_include_dev_surfaces_dev_packages() {
        let src = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/eslint": { "version": "8.0.0", "dev": true }
            }
        });
        let out = parse_package_lock(&src, "/package-lock.json", true);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].lifecycle_scope, Some(mikebom_common::resolution::LifecycleScope::Development));
    }

    #[test]
    fn package_lock_skips_workspace_link_entries() {
        let src = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/my-workspace": { "resolved": "../my-workspace", "link": true }
            }
        });
        let out = parse_package_lock(&src, "/package-lock.json", true);
        assert!(out.is_empty());
    }

    #[test]
    fn package_lock_scoped_package_emits_encoded_purl() {
        let src = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/@angular/core": { "version": "16.0.0", "license": "MIT" }
            }
        });
        let out = parse_package_lock(&src, "/package-lock.json", false);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].purl.as_str(), "pkg:npm/%40angular/core@16.0.0");
    }

    #[test]
    fn package_lock_skips_optional_by_default() {
        let src = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/fsevents": { "version": "2.3.0", "optional": true }
            }
        });
        let out_default = parse_package_lock(&src, "/p.json", false);
        assert!(out_default.is_empty());
        let out_all = parse_package_lock(&src, "/p.json", true);
        assert_eq!(out_all.len(), 1);
    }

    #[test]
    fn v1_lockfile_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package-lock.json"),
            r#"{"name":"old","lockfileVersion":1,"dependencies":{}}"#,
        )
        .unwrap();
        let err = read(dir.path(), false, crate::scan_fs::ScanMode::Path).unwrap_err();
        assert!(
            matches!(err, NpmError::LockfileV1Unsupported { .. }),
            "got {err:?}"
        );
        // Error message matches the contract.
        assert_eq!(
            err.to_string(),
            "package-lock.json v1 not supported; regenerate with npm ≥7"
        );
    }

    #[test]
    fn v2_and_v3_lockfiles_do_not_trigger_refusal() {
        for v in [2, 3] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(
                dir.path().join("package-lock.json"),
                format!(r#"{{"lockfileVersion":{v},"packages":{{}}}}"#),
            )
            .unwrap();
            let res = read(dir.path(), false, crate::scan_fs::ScanMode::Path);
            assert!(res.is_ok(), "v{v} lockfile should parse without refusal");
        }
    }

    // --- issue #262: nested-node_modules version-pinning --------------------

    #[test]
    fn nested_dep_emits_version_pinned_string() {
        // Issue #262 reproducer shape: mlly@1.0.0 depends on pathe;
        // a NESTED pathe@2.0.3 is installed under mlly's own
        // node_modules. The hoisted pathe@1.1.2 is also present.
        // Parser should emit mlly.depends = ["pathe 2.0.3"] so the
        // edge resolver in scan_fs/mod.rs picks the nested version.
        let lockfile = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/pathe": { "version": "1.1.2" },
                "node_modules/mlly": {
                    "version": "1.0.0",
                    "dependencies": { "pathe": "^2.0.0" }
                },
                "node_modules/mlly/node_modules/pathe": { "version": "2.0.3" }
            }
        });
        let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);
        let mlly = entries
            .iter()
            .find(|e| e.name == "mlly" && e.version == "1.0.0")
            .expect("mlly entry");
        assert_eq!(
            mlly.depends,
            vec!["pathe 2.0.3".to_string()],
            "mlly should depend on the NESTED pathe@2.0.3 (version-pinned), not bare 'pathe'"
        );
        // Both pathes emitted as separate components.
        assert!(entries.iter().any(|e| e.name == "pathe" && e.version == "1.1.2"));
        assert!(entries.iter().any(|e| e.name == "pathe" && e.version == "2.0.3"));
    }

    #[test]
    fn hoisted_only_dep_resolves_to_hoisted_version_pin() {
        // When there's no nested install, depends walks up to the
        // hoisted node_modules/<dep> entry and pins to its version.
        // (Pre-walk-up fix: this returned bare-name, relying on the
        // edge resolver's name_to_purl last-write-wins lookup —
        // which produces the wrong version when multiple parents
        // pin different versions of the same package. The walk-up
        // produces a deterministic version-pinned reference.)
        let lockfile = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/pathe": { "version": "1.1.2" },
                "node_modules/mlly": {
                    "version": "1.0.0",
                    "dependencies": { "pathe": "^1.0.0" }
                }
            }
        });
        let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);
        let mlly = entries.iter().find(|e| e.name == "mlly").expect("mlly");
        assert_eq!(
            mlly.depends,
            vec!["pathe 1.1.2".to_string()],
            "walk-up resolution should find the hoisted pathe@1.1.2 and pin"
        );
    }

    #[test]
    fn scoped_package_nested_under_parent_is_version_pinned() {
        // Scoped packages (`@scope/pkg`) follow the same path shape:
        // node_modules/<parent>/node_modules/@scope/pkg. Parser must
        // resolve them too.
        let lockfile = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/@types/node": { "version": "20.0.0" },
                "node_modules/some-tool": {
                    "version": "1.0.0",
                    "dependencies": { "@types/node": "^18.0.0" }
                },
                "node_modules/some-tool/node_modules/@types/node": { "version": "18.16.0" }
            }
        });
        let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);
        let some_tool = entries.iter().find(|e| e.name == "some-tool").expect("some-tool");
        assert_eq!(
            some_tool.depends,
            vec!["@types/node 18.16.0".to_string()],
            "scoped package nested under parent should be version-pinned: {:?}",
            some_tool.depends
        );
    }

    #[test]
    fn multiple_nested_deps_each_version_pinned_independently() {
        // A parent with two nested deps — each gets its own version-
        // pinned reference.
        let lockfile = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/foo": {
                    "version": "1.0.0",
                    "dependencies": { "a": "^2.0.0", "b": "^3.0.0" }
                },
                "node_modules/foo/node_modules/a": { "version": "2.5.0" },
                "node_modules/foo/node_modules/b": { "version": "3.4.0" }
            }
        });
        let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);
        let foo = entries.iter().find(|e| e.name == "foo").expect("foo");
        let mut sorted = foo.depends.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            vec!["a 2.5.0".to_string(), "b 3.4.0".to_string()],
            "each nested dep should be independently version-pinned"
        );
    }

    #[test]
    fn mixed_nested_and_hoisted_deps_are_disambiguated() {
        // A parent with one nested dep and one hoisted-only dep —
        // version-pinned vs bare-name forms coexist correctly.
        let lockfile = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/b": { "version": "3.0.0" },
                "node_modules/foo": {
                    "version": "1.0.0",
                    "dependencies": { "a": "^2.0.0", "b": "^3.0.0" }
                },
                "node_modules/foo/node_modules/a": { "version": "2.5.0" }
            }
        });
        let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);
        let foo = entries.iter().find(|e| e.name == "foo").expect("foo");
        let mut sorted = foo.depends.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            vec!["a 2.5.0".to_string(), "b 3.0.0".to_string()],
            "nested dep version-pinned to nested; hoisted dep version-pinned to hoisted (walk-up)"
        );
    }

    // --- post-#263 follow-up: peer/optional sections + deeper nesting -----

    #[test]
    fn peer_dependencies_are_skipped_declarative_not_install() {
        // peerDependencies are declarative — they express "the
        // consumer should have X" not "this package depends on X."
        // npm v7+ auto-installs peers as a convenience, but the
        // SBOM `dependsOn` slot means "X depends on Y" not "X
        // expects Y to be present." Trivy and syft also skip
        // peer-edges; mikebom matches.
        //
        // Reproducer: mlly declares `pathe` ONLY via
        // peerDependencies (no regular dependency). Even though
        // pathe is installed at multiple paths, mlly should NOT
        // emit any edge to pathe — let the package that ACTUALLY
        // requires pathe declare the edge instead.
        let lockfile = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/pathe": { "version": "1.1.2" },
                "node_modules/mlly": {
                    "version": "1.0.0",
                    "peerDependencies": { "pathe": "^2.0.0" }
                },
                "node_modules/mlly/node_modules/pathe": { "version": "2.0.3" }
            }
        });
        let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);
        let mlly = entries.iter().find(|e| e.name == "mlly").expect("mlly");
        assert!(
            mlly.depends.is_empty(),
            "`mlly` only declares `pathe` via peerDependencies — no edge should emit; got: {:?}",
            mlly.depends
        );
    }

    #[test]
    fn optional_dependencies_get_version_pinned_too() {
        // optionalDependencies fsevents — a classic real-world case.
        let lockfile = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/fsevents": { "version": "2.3.0" },
                "node_modules/chokidar": {
                    "version": "3.5.0",
                    "optionalDependencies": { "fsevents": "~2.3.0" }
                },
                "node_modules/chokidar/node_modules/fsevents": { "version": "2.3.3" }
            }
        });
        let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);
        let chokidar = entries.iter().find(|e| e.name == "chokidar").expect("chokidar");
        assert!(
            chokidar.depends.contains(&"fsevents 2.3.3".to_string()),
            "chokidar's optionalDependencies fsevents should pin to nested; got: {:?}",
            chokidar.depends
        );
    }

    #[test]
    fn dev_dependencies_get_version_pinned_too() {
        let lockfile = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/jest-helper": { "version": "1.0.0" },
                "node_modules/mocha": {
                    "version": "10.2.0",
                    "dev": true,
                    "devDependencies": { "jest-helper": "^2.0.0" }
                },
                "node_modules/mocha/node_modules/jest-helper": {
                    "version": "2.5.0",
                    "dev": true
                }
            }
        });
        let entries = parse_package_lock(&lockfile, "/tmp/lock.json", true);
        let mocha = entries.iter().find(|e| e.name == "mocha").expect("mocha");
        assert!(
            mocha.depends.contains(&"jest-helper 2.5.0".to_string()),
            "mocha's devDependencies jest-helper should pin to nested; got: {:?}",
            mocha.depends
        );
    }

    #[test]
    fn deps_in_multiple_sections_get_deduped_with_version_pin_preferred() {
        // Some packages legitimately list the same dep in multiple
        // sections (e.g., dep + optional). When both forms resolve,
        // the version-pinned form should win over the bare-name
        // form in the deduplicated output. peerDependencies is
        // skipped entirely (declarative-not-install) so the dedup
        // case is now restricted to dep / dev / optional.
        let lockfile = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/foo": {
                    "version": "1.0.0",
                    "dependencies": { "bar": "^1.0.0" },
                    "optionalDependencies": { "bar": "^1.0.0" }
                },
                "node_modules/foo/node_modules/bar": { "version": "1.5.0" }
            }
        });
        let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);
        let foo = entries.iter().find(|e| e.name == "foo").expect("foo");
        assert_eq!(
            foo.depends,
            vec!["bar 1.5.0".to_string()],
            "deduped depends should contain only the version-pinned form, not both bare and pinned; got: {:?}",
            foo.depends
        );
    }

    #[test]
    fn walk_up_resolution_picks_hoisted_when_parent_lacks_nested() {
        // Real-world molcajete bug: d3-dsv declares `commander: "7"`
        // and has NO nested commander; a DIFFERENT parent
        // (editorconfig) has a nested `commander@10.0.1`. Pre-walk-
        // up fix, d3-dsv's bare-name "commander" fell through to
        // name_to_purl's last-write-wins, which picked v10 instead
        // of the hoisted v7. Walk-up resolution correctly pins
        // d3-dsv → commander@7.2.0.
        let lockfile = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/commander": { "version": "7.2.0" },
                "node_modules/d3-dsv": {
                    "version": "3.0.1",
                    "dependencies": { "commander": "7" }
                },
                "node_modules/editorconfig": {
                    "version": "1.0.4",
                    "dependencies": { "commander": "^10.0.0" }
                },
                "node_modules/editorconfig/node_modules/commander": {
                    "version": "10.0.1"
                }
            }
        });
        let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);

        let d3 = entries.iter().find(|e| e.name == "d3-dsv").expect("d3-dsv");
        assert_eq!(
            d3.depends,
            vec!["commander 7.2.0".to_string()],
            "d3-dsv must pin to hoisted commander@7.2.0 (no nested install), not the unrelated nested commander@10"
        );

        let edit = entries.iter().find(|e| e.name == "editorconfig").expect("editorconfig");
        assert_eq!(
            edit.depends,
            vec!["commander 10.0.1".to_string()],
            "editorconfig must pin to its own nested commander@10.0.1"
        );
    }

    #[test]
    fn deep_nesting_resolves_at_each_level() {
        // Three-level chain: pkg-a → pkg-b → pkg-c, all nested
        // under each other due to version conflicts up the tree.
        //
        // node_modules/pkg-a/                v1
        //   node_modules/pkg-b/              v2 (nested under pkg-a)
        //     node_modules/pkg-c/            v3 (nested under pkg-b under pkg-a)
        //
        // Each entry's depends should pin to the child at its own
        // nested level — verifying the lookup `<path>/node_modules/<dep>`
        // correctly resolves arbitrarily-deep chains.
        let lockfile = serde_json::json!({
            "lockfileVersion": 3,
            "packages": {
                "node_modules/pkg-a": {
                    "version": "1.0.0",
                    "dependencies": { "pkg-b": "^2.0.0" }
                },
                "node_modules/pkg-a/node_modules/pkg-b": {
                    "version": "2.0.0",
                    "dependencies": { "pkg-c": "^3.0.0" }
                },
                "node_modules/pkg-a/node_modules/pkg-b/node_modules/pkg-c": {
                    "version": "3.0.0"
                }
            }
        });
        let entries = parse_package_lock(&lockfile, "/tmp/lock.json", false);

        let a = entries.iter().find(|e| e.name == "pkg-a").expect("pkg-a");
        assert_eq!(a.depends, vec!["pkg-b 2.0.0".to_string()]);

        let b = entries
            .iter()
            .find(|e| e.name == "pkg-b" && e.version == "2.0.0")
            .expect("pkg-b at v2");
        assert_eq!(
            b.depends,
            vec!["pkg-c 3.0.0".to_string()],
            "level-2 nested entry should pin its level-3 nested child; got: {:?}",
            b.depends
        );

        let c = entries
            .iter()
            .find(|e| e.name == "pkg-c" && e.version == "3.0.0")
            .expect("pkg-c at v3");
        assert!(c.depends.is_empty(), "leaf entry has no deps");
    }
}
