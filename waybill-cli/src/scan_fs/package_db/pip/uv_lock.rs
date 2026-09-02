//! Tier 4: `uv.lock` parser — milestone 106 US1 (issue #276).
//!
//! Parses [uv](https://docs.astral.sh/uv/)'s lockfile format (TOML with
//! a top-level `[[package]]` array, structurally similar to
//! `Cargo.lock`). Sibling reader to `poetry.rs` and `pipfile.rs`;
//! invoked from [`super::read`] per-project-root alongside the other
//! Python lockfile readers.
//!
//! Schema (uv 0.5+):
//!
//! ```toml
//! version = 1
//! requires-python = ">=3.11"
//!
//! [[package]]
//! name = "httpx"
//! version = "0.27.2"
//! source = { registry = "https://pypi.org/simple" }
//! dependencies = [
//!     { name = "anyio" },
//!     { name = "certifi" },
//! ]
//!
//! [[package]]
//! name = "my-workspace-member"
//! version = "0.1.0"
//! source = { editable = "apps/web" }   # or { virtual = "..." }
//! ```
//!
//! Workspace handling per Clarification Q1 of milestone 106: when the
//! root `pyproject.toml` declares `[tool.uv.workspace]` with a `members`
//! array, waybill emits a synthetic workspace-root + each member as
//! main-module + intra-workspace dependency edges. The workspace-root
//! component is built by [`super::super::workspace::synthesize_workspace_root`].

use std::collections::HashSet;
use std::path::Path;

use waybill_common::types::hash::ContentHash;
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::super::workspace::{synthesize_workspace_root, workspace_root_name};
use super::super::PackageDbEntry;
use super::build_pypi_purl_str;

/// Milestone 674: default PyPI registry URL. Non-default values
/// trigger emission of the `waybill:pypi-source-url` annotation so
/// downstream consumers can distinguish public-PyPI resolves from
/// private-mirror resolves.
const DEFAULT_PYPI_REGISTRY: &str = "https://pypi.org/simple";

/// Milestone 674: parsed representation of the `source = { ... }`
/// inline table on a uv.lock `[[package]]` entry. Six variants match
/// uv's own source-model enumeration.
///
/// See `specs/674-uv-lock-reader/contracts/source_variants.md` for
/// per-variant PURL construction rules + annotation emission rules.
/// Extracted from the caller-side `toml::Value` (rather than serde-
/// deserialized) to preserve m106's permissive shape-drift tolerance.
enum UvSourceShape<'a> {
    /// `source = { registry = "..." }` — the common case. Emits as
    /// `pkg:pypi/*`.
    Registry { url: &'a str },
    /// `source = { git = "URL", rev = "SHA" }` — cloned git repo.
    /// Emits as `pkg:generic/*` with source-type + source-url
    /// annotations naming `<git-url>@<rev>`.
    Git { url: &'a str, rev: &'a str },
    /// `source = { path = "..." }` — local filesystem package.
    /// Emits as `pkg:generic/*` with source-type=path.
    Path { path: &'a str },
    /// `source = { url = "..." }` — direct HTTP(s) install source.
    /// Emits as `pkg:generic/*` with source-type=url.
    Url { url: &'a str },
    /// `source = { editable = "..." }` — pyproject-in-place install.
    /// KEPT as `pkg:pypi/*` for m106 workspace-mode backward-compat.
    /// Workspace members get `waybill:component-role=main-module`
    /// per m106 Clarification Q1.
    Editable,
    /// `source = { virtual = "..." }` — no-code pseudo-package.
    /// KEPT as `pkg:pypi/*` for m106 backward-compat (m183 US3 test
    /// uses `virtual = "."` on the pyproject's own package).
    Virtual,
    /// Unknown / missing `source` table — treated as source-only /
    /// permissive fallback. Emits as `pkg:pypi/*` (m106 behavior).
    Unknown,
}

impl<'a> UvSourceShape<'a> {
    /// Extract the source variant from a `[[package]].source` table.
    /// Returns `UvSourceShape::Unknown` when the source key doesn't
    /// match any of the 6 known variants — permissive-tolerance for
    /// future uv schema drift.
    fn from_table(source_tbl: Option<&'a toml::value::Table>) -> Self {
        let Some(tbl) = source_tbl else {
            return UvSourceShape::Unknown;
        };
        if let Some(url) = tbl.get("registry").and_then(|v| v.as_str()) {
            return UvSourceShape::Registry { url };
        }
        if let Some(url) = tbl.get("git").and_then(|v| v.as_str()) {
            let rev = tbl.get("rev").and_then(|v| v.as_str()).unwrap_or("");
            return UvSourceShape::Git { url, rev };
        }
        if let Some(path) = tbl.get("path").and_then(|v| v.as_str()) {
            return UvSourceShape::Path { path };
        }
        if let Some(url) = tbl.get("url").and_then(|v| v.as_str()) {
            return UvSourceShape::Url { url };
        }
        if tbl.contains_key("editable") {
            return UvSourceShape::Editable;
        }
        if tbl.contains_key("virtual") {
            return UvSourceShape::Virtual;
        }
        UvSourceShape::Unknown
    }
}

/// Milestone 674: extract SHA-256 hashes from a uv.lock `[[package]]`
/// entry's `sdist` (Option<Table>) + `wheels` (Array<Table>). Every
/// hash is stored as `sha256:<hex>` in uv.lock; we extract just the
/// hex and construct a `ContentHash`. Dedup by hex-value to avoid
/// duplicate hashes when many wheel platforms share the same content
/// hash.
fn extract_sha256_hashes(pkg_tbl: &toml::value::Table) -> Vec<ContentHash> {
    let mut hashes: Vec<ContentHash> = Vec::new();
    let push_from_table = |out: &mut Vec<ContentHash>, tbl: &toml::value::Table| {
        if let Some(hash_str) = tbl.get("hash").and_then(|v| v.as_str()) {
            if let Some(hex) = hash_str.strip_prefix("sha256:") {
                if let Ok(h) = ContentHash::sha256(hex) {
                    out.push(h);
                }
            }
        }
    };
    if let Some(sdist_tbl) = pkg_tbl.get("sdist").and_then(|v| v.as_table()) {
        push_from_table(&mut hashes, sdist_tbl);
    }
    if let Some(wheels_arr) = pkg_tbl.get("wheels").and_then(|v| v.as_array()) {
        for wheel in wheels_arr {
            if let Some(wheel_tbl) = wheel.as_table() {
                push_from_table(&mut hashes, wheel_tbl);
            }
        }
    }
    hashes.sort_by(|a, b| a.value.as_str().cmp(b.value.as_str()));
    hashes.dedup_by(|a, b| a.value.as_str() == b.value.as_str());
    hashes
}

/// Read `<rootfs>/uv.lock` if present. Returns None when absent or
/// unparseable. Mirrors `read_poetry_lock` and `read_pipfile_lock` in
/// shape so the dispatcher can call all three uniformly.
pub(super) fn read_uv_lock(rootfs: &Path, _include_dev: bool) -> Option<Vec<PackageDbEntry>> {
    let path = rootfs.join("uv.lock");
    let text = std::fs::read_to_string(&path).ok()?;
    let parsed: toml::Value = match toml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "uv.lock parse failed; skipping (FR-010 warn-and-continue)"
            );
            return None;
        }
    };
    let source_path = path.to_string_lossy().into_owned();
    Some(parse_uv_lock(&parsed, &source_path, rootfs))
}

/// Milestone 674 FR-002 Pants fallback entry-point: parse a
/// uv.lock byte-slice into `PackageDbEntry` records. Used by
/// `pants::mod::read` when `pants::lockfile::parse` fails on a
/// Pants-declared file (the file may be a uv.lock, not a Pex JSON
/// lockfile — modern Pants supports uv as a resolver backend).
///
/// `pants_resolve_name` is threaded into every emitted component
/// as a `waybill:pants-resolve` annotation, matching the m223
/// Pants scope-tag convention.
///
/// Returns `None` on parse failure (WARN-and-skip per m223 contract).
pub(crate) fn parse_uv_lock_bytes(
    bytes: &[u8],
    source_path: &str,
    pants_resolve_name: &str,
) -> Option<Vec<PackageDbEntry>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let parsed: toml::Value = toml::from_str(text)
        .map_err(|e| {
            tracing::warn!(
                source_path = %source_path,
                error = %e,
                "uv.lock (Pants FR-002 fallback) parse failed; skipping"
            );
        })
        .ok()?;
    // Version-gate: only accept version=1 (matches milestone 106).
    let version = parsed.get("version").and_then(|v| v.as_integer()).unwrap_or(-1);
    if version != 1 {
        tracing::warn!(
            source_path = %source_path,
            version = version,
            "uv.lock (Pants FR-002 fallback): unsupported schema version (expected 1); skipping"
        );
        return None;
    }
    let mut entries = parse_uv_lock(&parsed, source_path, Path::new("/tmp/nonexistent"));
    // Retroactively decorate every emitted entry with the Pants
    // resolve-name annotation. Simpler than threading the param
    // through the parse loop — the annotation bag is a BTreeMap
    // that the emit loop already populates.
    for entry in &mut entries {
        entry.extra_annotations.insert(
            "waybill:pants-resolve".to_string(),
            serde_json::Value::String(pants_resolve_name.to_string()),
        );
    }
    Some(entries)
}

/// Parse an already-deserialised `uv.lock` TOML document. Public-in-
/// module so unit tests can drive parsing without disk I/O.
pub(crate) fn parse_uv_lock(
    root: &toml::Value,
    source_path: &str,
    rootfs: &Path,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();

    let Some(packages) = root.get("package").and_then(|v| v.as_array()) else {
        return out;
    };

    // Detect workspace mode by reading the root pyproject.toml's
    // `[tool.uv.workspace]` block. When present, collect the set of
    // workspace member names so we can tag them with
    // `waybill:component-role: "main-module"` at emission time and
    // emit a synthetic workspace-root above them.
    let workspace_info = detect_workspace(rootfs);

    // Milestone 183 US3 — accumulate the set of extras-gated child
    // names declared under any `[package.optional-dependencies].<extra>`
    // sub-table. Diamond-shape (FR-005) is per-package: a name that
    // ALSO appears in the same package's `dependencies = [...]` array
    // is excluded (Runtime wins). Applied via the shared post-pass
    // helper `super::apply_optional_derivation_annotation` after the
    // main emission loop — this ensures the classification only fires
    // for entries the loop actually emitted (workspace members with
    // a matching top-level `[[package]]` block).
    let mut optional_child_names: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for pkg in packages {
        let Some(tbl) = pkg.as_table() else {
            continue;
        };
        let primary_dep_names: std::collections::HashSet<&str> = tbl
            .get("dependencies")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|dep| dep.as_table()?.get("name")?.as_str())
                    .collect()
            })
            .unwrap_or_default();
        if let Some(opt_table) = tbl.get("optional-dependencies").and_then(|v| v.as_table()) {
            for (_extra_name, arr) in opt_table {
                if let Some(deps_arr) = arr.as_array() {
                    for dep in deps_arr {
                        if let Some(child_name) = dep
                            .as_table()
                            .and_then(|t| t.get("name"))
                            .and_then(|v| v.as_str())
                        {
                            if !primary_dep_names.contains(child_name) {
                                optional_child_names.insert(child_name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    for pkg in packages {
        let Some(tbl) = pkg.as_table() else {
            continue;
        };
        let name = tbl.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
        let version = tbl.get("version").and_then(|v| v.as_str()).unwrap_or("").trim();
        if name.is_empty() {
            // Per spec edge case "source-only entry": uv.lock entries
            // without a top-level `name` are unresolvable. Warn and skip.
            tracing::warn!(
                source_path = %source_path,
                "uv.lock entry has no `name` field; skipping"
            );
            continue;
        }
        if version.is_empty() {
            // Source-only entry (e.g. git source without resolved version
            // at lockfile-write time). Warn and skip per spec US1
            // scenario 4 + FR-010.
            tracing::warn!(
                package = %name,
                source_path = %source_path,
                "uv.lock entry has no `version`; skipping"
            );
            continue;
        }

        // `dependencies` is an array of inline tables in uv.lock:
        //   dependencies = [{ name = "anyio" }, { name = "certifi" }]
        // Distinct from poetry.lock which uses a nested table.
        let depends: Vec<String> = tbl
            .get("dependencies")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|dep| dep.as_table()?.get("name")?.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();


        // Milestone 674: parse the `source = { ... }` inline table
        // into a UvSourceShape. Drives per-variant PURL selection
        // + variant-specific annotations. Registry / Editable /
        // Virtual / Unknown all emit as `pkg:pypi/*` (preserves
        // m106 workspace-mode backward-compat); Git / Path / Url
        // emit as `pkg:generic/*` per m674 FR-005 through FR-007.
        let source_tbl = tbl.get("source").and_then(|v| v.as_table());
        let source_shape = UvSourceShape::from_table(source_tbl);

        let purl_str = match &source_shape {
            UvSourceShape::Git { .. }
            | UvSourceShape::Path { .. }
            | UvSourceShape::Url { .. } => format!(
                "pkg:generic/{}@{}",
                encode_purl_segment(name),
                encode_purl_segment(version),
            ),
            _ => build_pypi_purl_str(name, version),
        };
        let Ok(purl) = Purl::new(&purl_str) else {
            tracing::warn!(
                package = %name,
                version = %version,
                "uv.lock entry produced invalid PURL; skipping"
            );
            continue;
        };

        let mut extra: std::collections::BTreeMap<String, serde_json::Value> =
            Default::default();

        // Milestone 674 (C157): every uv.lock-emitted component
        // carries a `waybill:python-lockfile-format=uv` annotation
        // for downstream format-provenance visibility.
        extra.insert(
            "waybill:python-lockfile-format".to_string(),
            serde_json::Value::String("uv".to_string()),
        );

        // Milestone 674: per-variant annotations for non-registry
        // sources + non-default registry URLs.
        let source_kind: Option<String> = match &source_shape {
            UvSourceShape::Registry { url } => {
                if *url != DEFAULT_PYPI_REGISTRY {
                    extra.insert(
                        "waybill:pypi-source-url".to_string(),
                        serde_json::Value::String(url.to_string()),
                    );
                }
                None
            }
            UvSourceShape::Git { url, rev } => {
                // Milestone 674: waybill:source-url annotation carries
                // the resolved git URL + rev. The `source_type` field
                // below drives the `waybill:source-type` emission via
                // the standard PackageDbEntry serialization (no
                // manual annotation insert here to avoid emit-pipeline
                // collision).
                extra.insert(
                    "waybill:source-url".to_string(),
                    serde_json::Value::String(format!("{}@{}", url, rev)),
                );
                Some("git".to_string())
            }
            UvSourceShape::Path { path } => {
                extra.insert(
                    "waybill:source-url".to_string(),
                    serde_json::Value::String(format!("file://{}", path)),
                );
                Some("local".to_string())
            }
            UvSourceShape::Url { url } => {
                extra.insert(
                    "waybill:source-url".to_string(),
                    serde_json::Value::String(url.to_string()),
                );
                Some("url".to_string())
            }
            UvSourceShape::Editable
            | UvSourceShape::Virtual
            | UvSourceShape::Unknown => None,
        };

        // Component-role annotation for workspace members (per
        // Clarification Q1 of milestone 106).
        if let Some(ws) = &workspace_info {
            if ws.member_names.contains(name) {
                extra.insert(
                    "waybill:component-role".to_string(),
                    serde_json::Value::String("main-module".to_string()),
                );
            }
        }

        // Milestone 674: FR-002 Pants integration — the caller can
        // pre-populate `pants_resolve_name` when this parse is
        // invoked from the Pants FR-002 fallback path. See
        // `pants/mod.rs::read` for the caller-side wiring.
        // (For the standalone-uv path — `read_uv_lock` — this is
        // always None; a helper wrapper `parse_uv_lock_bytes` below
        // exposes the pants-context entry.)

        // Milestone 674: hash extraction from sdist + wheels.
        let hashes = extract_sha256_hashes(tbl);

        out.push(PackageDbEntry {
            build_inclusion: None,
            purl,
            name: name.to_string(),
            version: version.to_string(),
            arch: None,
            source_path: source_path.to_string(),
            depends,
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: None,
            requirement_ranges: Vec::new(),
            source_type: source_kind,
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
            extra_annotations: extra,
            binary_role: None,
        });
    }

    // If we detected a workspace, also emit the synthetic
    // workspace-root component with dependsOn edges to each member
    // (per FR-015 + contracts/workspace-emission.md).
    if let Some(ws) = workspace_info {
        let root_pyproject = rootfs.join("pyproject.toml");
        let root_name = workspace_root_name(ws.root_name.as_deref());
        if let Some(mut root_entry) = synthesize_workspace_root(&root_name, &root_pyproject) {
            // Populate workspace-root's `depends` with each emitted
            // member's name. The downstream emitter resolves these to
            // dependsOn edges against the actual member PURLs.
            root_entry.depends = ws.member_names.iter().cloned().collect();
            out.push(root_entry);
        }
    }

    // Milestone 183 US3 — apply the shared post-pass. Marks every
    // emitted entry whose name is in `optional_child_names` AND whose
    // `lifecycle_scope.is_none()` (Decision 3 lockfile-precedence
    // guard: since uv.lock is itself a lockfile, entries typically
    // reach the post-pass with `None` and get classified here; the
    // guard is a defense-in-depth pin) with `LifecycleScope::Optional`
    // + the `waybill:optional-derivation = "pip-optional-dependencies"`
    // annotation.
    super::apply_optional_derivation_annotation(&mut out, &optional_child_names);

    out
}

/// Information collected from the root `pyproject.toml`'s
/// `[tool.uv.workspace]` block when present.
struct WorkspaceInfo {
    /// Set of workspace-member project names (the `name` field from
    /// each member's own `pyproject.toml`'s `[project]` table).
    member_names: HashSet<String>,
    /// The root manifest's `name` field, if present. Used to derive
    /// the synthetic workspace-root component's name.
    root_name: Option<String>,
}

/// Detect whether the project at `rootfs` declares a uv workspace.
/// Returns `Some(WorkspaceInfo)` when the root `pyproject.toml` has
/// `[tool.uv.workspace]` with a `members` array. The member names
/// are resolved by reading each member-path's own `pyproject.toml`.
fn detect_workspace(rootfs: &Path) -> Option<WorkspaceInfo> {
    let root_path = rootfs.join("pyproject.toml");
    let text = std::fs::read_to_string(&root_path).ok()?;
    let parsed: toml::Value = toml::from_str(&text).ok()?;

    let workspace_block = parsed
        .get("tool")
        .and_then(|v| v.get("uv"))
        .and_then(|v| v.get("workspace"))?;
    let members_arr = workspace_block.get("members").and_then(|v| v.as_array())?;

    let mut member_names: HashSet<String> = HashSet::new();
    for m in members_arr {
        let Some(rel) = m.as_str() else { continue };
        let member_pyproject = rootfs.join(rel).join("pyproject.toml");
        let member_text = match std::fs::read_to_string(&member_pyproject) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    member = %rel,
                    path = %member_pyproject.display(),
                    error = %e,
                    "uv workspace member's pyproject.toml unreadable; skipping member"
                );
                continue;
            }
        };
        let member_parsed: toml::Value = match toml::from_str(&member_text) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    member = %rel,
                    error = %e,
                    "uv workspace member's pyproject.toml malformed; skipping member"
                );
                continue;
            }
        };
        if let Some(name) = member_parsed
            .get("project")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
        {
            member_names.insert(name.trim().to_string());
        }
    }

    let root_name = parsed
        .get("project")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());

    Some(WorkspaceInfo {
        member_names,
        root_name,
    })
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
    fn emits_basic_pypi_components() {
        let src = r#"
version = 1
requires-python = ">=3.11"

[[package]]
name = "httpx"
version = "0.27.2"
source = { registry = "https://pypi.org/simple" }

[[package]]
name = "pydantic"
version = "2.9.2"
source = { registry = "https://pypi.org/simple" }
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_uv_lock(&parsed, "/tmp/uv.lock", tmp.path());
        assert_eq!(entries.len(), 2, "got: {entries:?}");
        let httpx = entries.iter().find(|e| e.name == "httpx").unwrap();
        assert_eq!(httpx.purl.as_str(), "pkg:pypi/httpx@0.27.2");
        let pydantic = entries.iter().find(|e| e.name == "pydantic").unwrap();
        assert_eq!(pydantic.purl.as_str(), "pkg:pypi/pydantic@2.9.2");
    }

    #[test]
    fn emits_dependency_edges() {
        let src = r#"
version = 1

[[package]]
name = "httpx"
version = "0.27.2"
source = { registry = "https://pypi.org/simple" }
dependencies = [
    { name = "anyio" },
    { name = "certifi" },
]

[[package]]
name = "anyio"
version = "4.4.0"
source = { registry = "https://pypi.org/simple" }

[[package]]
name = "certifi"
version = "2024.8.30"
source = { registry = "https://pypi.org/simple" }
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_uv_lock(&parsed, "/tmp/uv.lock", tmp.path());
        assert_eq!(entries.len(), 3);
        let httpx = entries.iter().find(|e| e.name == "httpx").unwrap();
        assert_eq!(httpx.depends, vec!["anyio".to_string(), "certifi".to_string()]);
    }

    #[test]
    fn warns_on_source_only_entry() {
        // Entry with no name+version → skipped, no panic, no entry in output.
        let src = r#"
version = 1

[[package]]
name = "good-package"
version = "1.0.0"
source = { registry = "https://pypi.org/simple" }

[[package]]
name = "bad-no-version"
source = { registry = "https://pypi.org/simple" }
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let entries = parse_uv_lock(&parsed, "/tmp/uv.lock", tmp.path());
        assert_eq!(entries.len(), 1, "got: {entries:?}");
        assert_eq!(entries[0].name, "good-package");
    }

    #[test]
    fn emits_workspace_root_and_members() {
        // Synthetic uv workspace: root pyproject declares members
        // apps/web and libs/shared; each member has its own pyproject
        // with a [project] name field; uv.lock has [[package]] entries
        // for each member + their deps.
        let tmp = tempfile::tempdir().unwrap();
        let rootfs = tmp.path();
        write(
            &rootfs.join("pyproject.toml"),
            r#"
[project]
name = "my-monorepo"

[tool.uv.workspace]
members = ["apps/web", "libs/shared"]
"#,
        );
        write(
            &rootfs.join("apps/web/pyproject.toml"),
            r#"
[project]
name = "web"
"#,
        );
        write(
            &rootfs.join("libs/shared/pyproject.toml"),
            r#"
[project]
name = "shared"
"#,
        );
        let lockfile_src = r#"
version = 1

[[package]]
name = "web"
version = "0.1.0"
source = { editable = "apps/web" }
dependencies = [
    { name = "shared" },
]

[[package]]
name = "shared"
version = "0.1.0"
source = { editable = "libs/shared" }

[[package]]
name = "httpx"
version = "0.27.2"
source = { registry = "https://pypi.org/simple" }
"#;
        let parsed: toml::Value = toml::from_str(lockfile_src).unwrap();
        let entries = parse_uv_lock(&parsed, "/tmp/uv.lock", rootfs);

        // Expect: 2 workspace-member components + 1 PyPI component + 1
        // synthetic workspace-root = 4 total.
        assert_eq!(entries.len(), 4, "got: {entries:?}");

        // Workspace members carry component-role: main-module.
        let web = entries.iter().find(|e| e.name == "web").unwrap();
        assert_eq!(
            web.extra_annotations
                .get("waybill:component-role")
                .and_then(|v| v.as_str()),
            Some("main-module"),
        );
        let shared = entries.iter().find(|e| e.name == "shared").unwrap();
        assert_eq!(
            shared
                .extra_annotations
                .get("waybill:component-role")
                .and_then(|v| v.as_str()),
            Some("main-module"),
        );

        // External PyPI dep does NOT carry component-role.
        let httpx = entries.iter().find(|e| e.name == "httpx").unwrap();
        assert!(!httpx.extra_annotations.contains_key("waybill:component-role"));

        // Synthetic workspace-root component is present.
        let ws_root = entries
            .iter()
            .find(|e| e.purl.as_str() == "pkg:generic/my-monorepo")
            .expect("workspace-root component must be emitted");
        assert_eq!(
            ws_root
                .extra_annotations
                .get("waybill:component-role")
                .and_then(|v| v.as_str()),
            Some("workspace-root"),
        );
        // Workspace-root depends on each member by name.
        let mut depends_sorted = ws_root.depends.clone();
        depends_sorted.sort();
        assert_eq!(depends_sorted, vec!["shared".to_string(), "web".to_string()]);

        // Intra-workspace edge: web depends on shared.
        assert_eq!(web.depends, vec!["shared".to_string()]);
    }

    #[test]
    fn emits_intra_workspace_edge_only_when_declared() {
        // Independent workspace members (no inter-dep declared) MUST
        // NOT have edges between them. This is the "foo and bar with
        // no dependency" requirement from Clarification Q1.
        let tmp = tempfile::tempdir().unwrap();
        let rootfs = tmp.path();
        write(
            &rootfs.join("pyproject.toml"),
            r#"
[project]
name = "tools"

[tool.uv.workspace]
members = ["foo", "bar"]
"#,
        );
        write(&rootfs.join("foo/pyproject.toml"), r#"
[project]
name = "foo"
"#);
        write(&rootfs.join("bar/pyproject.toml"), r#"
[project]
name = "bar"
"#);
        let lockfile_src = r#"
version = 1

[[package]]
name = "foo"
version = "0.1.0"
source = { editable = "foo" }

[[package]]
name = "bar"
version = "0.1.0"
source = { editable = "bar" }
"#;
        let parsed: toml::Value = toml::from_str(lockfile_src).unwrap();
        let entries = parse_uv_lock(&parsed, "/tmp/uv.lock", rootfs);
        let foo = entries.iter().find(|e| e.name == "foo").unwrap();
        let bar = entries.iter().find(|e| e.name == "bar").unwrap();
        assert!(
            !foo.depends.contains(&"bar".to_string()),
            "foo MUST NOT depend on bar (no declared edge); got: {:?}",
            foo.depends,
        );
        assert!(
            !bar.depends.contains(&"foo".to_string()),
            "bar MUST NOT depend on foo (no declared edge); got: {:?}",
            bar.depends,
        );
    }

    // ── Milestone 183 US3 — uv.lock optional-dependencies classification ──

    #[test]
    fn optional_dependencies_sub_table_classifies() {
        // A `[package.optional-dependencies].dev = [{ name = "pytest" }]`
        // sub-table must cause the pytest [[package]] entry (also
        // present as a top-level [[package]] block) to classify as
        // Optional + carry the derivation annotation.
        let src = r#"
version = 1
requires-python = ">=3.11"

[[package]]
name = "my-app"
version = "0.1.0"
source = { virtual = "." }

[package.optional-dependencies]
dev = [{ name = "pytest" }]

[[package]]
name = "pytest"
version = "7.4.0"
source = { registry = "https://pypi.org/simple" }
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let out = parse_uv_lock(&parsed, "/uv.lock", Path::new("/tmp/nonexistent"));
        let pytest = out
            .iter()
            .find(|e| e.name == "pytest")
            .expect("pytest emitted");
        assert_eq!(
            pytest.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Optional)
        );
        assert_eq!(
            pytest.extra_annotations.get("waybill:optional-derivation"),
            Some(&serde_json::Value::String("pip-optional-dependencies".to_string()))
        );

        // Regression pin: my-app itself stays unclassified (main-module
        // shape). Not classified by the post-pass because its name
        // isn't in `optional_child_names`.
        let my_app = out
            .iter()
            .find(|e| e.name == "my-app")
            .expect("my-app emitted");
        assert!(!my_app
            .extra_annotations
            .contains_key("waybill:optional-derivation"));
    }

    #[test]
    fn uv_lock_diamond_shape_runtime_wins() {
        // FR-005: pytest in BOTH the primary `dependencies` array AND
        // an `optional-dependencies.<extra>` array of the same package
        // — Runtime wins, no derivation annotation.
        let src = r#"
version = 1

[[package]]
name = "my-app"
version = "0.1.0"
source = { virtual = "." }
dependencies = [{ name = "pytest" }]

[package.optional-dependencies]
test = [{ name = "pytest" }]

[[package]]
name = "pytest"
version = "7.4.0"
source = { registry = "https://pypi.org/simple" }
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let out = parse_uv_lock(&parsed, "/uv.lock", Path::new("/tmp/nonexistent"));
        let pytest = out
            .iter()
            .find(|e| e.name == "pytest")
            .expect("pytest emitted");
        assert!(
            pytest.lifecycle_scope
                != Some(waybill_common::resolution::LifecycleScope::Optional),
            "diamond-shape violated: pytest classified as Optional"
        );
        assert!(!pytest
            .extra_annotations
            .contains_key("waybill:optional-derivation"));
    }

    #[test]
    fn uv_lock_optional_absent_stays_none() {
        // Regression pin: no `[package.optional-dependencies]` sub-table
        // anywhere → no classification, no annotation. Pre-m183 behavior
        // preserved for byte-identity per SC-005.
        let src = r#"
version = 1

[[package]]
name = "my-app"
version = "0.1.0"
source = { virtual = "." }
dependencies = [{ name = "httpx" }]

[[package]]
name = "httpx"
version = "0.27.2"
source = { registry = "https://pypi.org/simple" }
"#;
        let parsed: toml::Value = toml::from_str(src).unwrap();
        let out = parse_uv_lock(&parsed, "/uv.lock", Path::new("/tmp/nonexistent"));
        for entry in &out {
            assert!(
                !entry
                    .extra_annotations
                    .contains_key("waybill:optional-derivation"),
                "unexpected derivation annotation on {}: {:?}",
                entry.name,
                entry.extra_annotations
            );
            // Also: lifecycle_scope stays None (pre-m183 behavior for
            // uv.lock's Tier-2 emission).
            assert!(
                entry.lifecycle_scope.is_none(),
                "unexpected lifecycle_scope on {}: {:?}",
                entry.name,
                entry.lifecycle_scope
            );
        }
    }
}
