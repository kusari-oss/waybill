//! Workspace emission helpers — milestone 106 phase 2C.
//!
//! Shared by the uv (`pip/uv_lock.rs`) and Bun (`npm/bun_lock.rs`)
//! readers when they detect a workspace section in the root manifest.
//! Provides:
//!
//! - `workspace_root_name`: derive the synthetic-root's name from the
//!   root manifest's `name` field, with a deterministic placeholder
//!   fallback.
//! - `synthesize_workspace_root`: construct the `PackageDbEntry` for
//!   the synthetic workspace-root component, with the
//!   `mikebom:component-role: "workspace-root"` annotation (C40
//!   open-enum value added in milestone 106 per research R3).
//!
//! The workspace emission model is documented in
//! `specs/106-ecosystem-coverage-expansion/contracts/workspace-emission.md`.

use std::path::Path;

use mikebom_common::types::purl::{encode_purl_segment, Purl};

use super::PackageDbEntry;

/// Placeholder used when the root manifest declares no `name` (or an
/// empty one). Surfaces cleanly in PURLs as `pkg:generic/workspace-root`
/// and stays deterministic across hosts.
const WORKSPACE_ROOT_PLACEHOLDER: &str = "workspace-root";

/// Derive the name for the synthetic workspace-root component. If the
/// root manifest's `name` field is present and non-empty, use it
/// verbatim. Otherwise return the placeholder `"workspace-root"`.
///
/// Empty-string-as-name is treated identically to absent — `serde`
/// will deserialize a missing field as `None`, but a string-typed
/// field that happens to be `""` (rare but possible) also doesn't
/// produce a meaningful PURL.
#[allow(dead_code)] // wired by US1 (T016) + US2 (T027) of milestone 106
pub(super) fn workspace_root_name(root_manifest_field: Option<&str>) -> String {
    match root_manifest_field {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => WORKSPACE_ROOT_PLACEHOLDER.to_string(),
    }
}

/// Construct the synthetic workspace-root `PackageDbEntry` per the
/// emission model in `contracts/workspace-emission.md`.
///
/// - PURL: `pkg:generic/<encoded-name>` (no version — workspace roots
///   are unversioned by design)
/// - `mikebom:component-role: "workspace-root"`
/// - `mikebom:source-files: "<source_path>"`
///
/// The returned `PackageDbEntry`'s `depends` field is left empty —
/// callers populate it with the resolved member PURLs after they walk
/// the workspace's member list. The dispatcher emits one
/// `dependsOn` edge from workspace-root → each member per the
/// emission policy.
///
/// Returns `None` if PURL construction fails (vanishingly rare — the
/// name has already been passed through `encode_purl_segment`).
#[allow(dead_code)] // wired by US1 + US2 of milestone 106
pub(super) fn synthesize_workspace_root(
    name: &str,
    source_path: &Path,
) -> Option<PackageDbEntry> {
    let encoded = encode_purl_segment(name);
    let purl = Purl::new(&format!("pkg:generic/{encoded}")).ok()?;

    let mut extra: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra.insert(
        "mikebom:component-role".to_string(),
        serde_json::Value::String("workspace-root".to_string()),
    );
    extra.insert(
        "mikebom:source-files".to_string(),
        serde_json::Value::String(source_path.to_string_lossy().into_owned()),
    );

    Some(PackageDbEntry {
        purl,
        name: name.to_string(),
        version: String::new(),
        arch: None,
        source_path: source_path.to_string_lossy().into_owned(),
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
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations: extra,
        binary_role: None,
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn synthesizes_root_with_explicit_name() {
        let path = PathBuf::from("/tmp/my-monorepo/pyproject.toml");
        let entry = synthesize_workspace_root("my-monorepo", &path).unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:generic/my-monorepo");
        assert_eq!(entry.name, "my-monorepo");
        assert_eq!(entry.version, "");
        assert_eq!(
            entry
                .extra_annotations
                .get("mikebom:component-role")
                .and_then(|v| v.as_str()),
            Some("workspace-root"),
        );
        assert_eq!(
            entry
                .extra_annotations
                .get("mikebom:source-files")
                .and_then(|v| v.as_str()),
            Some("/tmp/my-monorepo/pyproject.toml"),
        );
    }

    #[test]
    fn synthesizes_root_with_placeholder_name() {
        // When the root manifest has no `name` field, callers use
        // `workspace_root_name(None)` which returns the placeholder.
        let placeholder = workspace_root_name(None);
        assert_eq!(placeholder, "workspace-root");
        let path = PathBuf::from("/tmp/repo/package.json");
        let entry = synthesize_workspace_root(&placeholder, &path).unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:generic/workspace-root");
        assert_eq!(entry.name, "workspace-root");
    }

    #[test]
    fn annotation_is_workspace_root() {
        // Belt-and-suspenders: the synthesized root MUST carry the
        // exact C40 enum value introduced by milestone 106. The
        // string "workspace-root" is load-bearing — it's documented
        // in docs/reference/sbom-format-mapping.md's C40 row and
        // downstream consumers may filter on this exact value.
        let entry = synthesize_workspace_root(
            "foo",
            &PathBuf::from("/tmp/foo/pyproject.toml"),
        )
        .unwrap();
        let role = entry
            .extra_annotations
            .get("mikebom:component-role")
            .expect("workspace-root entry must carry component-role annotation");
        assert_eq!(role, &serde_json::Value::String("workspace-root".to_string()));
    }

    #[test]
    fn workspace_root_name_strips_whitespace() {
        assert_eq!(workspace_root_name(Some("  my-app  ")), "my-app");
    }

    #[test]
    fn workspace_root_name_falls_back_on_empty() {
        assert_eq!(workspace_root_name(Some("")), "workspace-root");
        assert_eq!(workspace_root_name(Some("   ")), "workspace-root");
        assert_eq!(workspace_root_name(None), "workspace-root");
    }
}
