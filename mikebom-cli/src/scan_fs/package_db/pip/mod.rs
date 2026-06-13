//! Read Python package metadata from a scanned filesystem.
//!
//! Three layered sources in order of authority (per spec FR-001..FR-005
//! and research.md R2 / R3):
//!
//! 1. **Installed venv**: `<root>/.../site-packages/<name>-<version>.dist-info/METADATA`
//!    — confidence 0.85, tier `deployed`. Ground truth: these packages are
//!    actually resolved and sitting on disk.
//! 2. **Lockfile**: `poetry.lock` (v1 and v2 formats) or `Pipfile.lock`
//!    — confidence 0.85, tier `source`. Authoritative about what WILL be
//!    installed if the lockfile is honoured.
//! 3. **Requirements file**: `requirements.txt` (and any `*.txt` matching
//!    pip's convention) — confidence 0.70, tier `design`. Best-guess:
//!    range specs may resolve to different versions depending on the
//!    registry state at install time.
//!
//! The public entry point [`read`] walks these in order and applies
//! drift resolution per research.md R8: a venv entry wins over a
//! lockfile entry for the same package; a lockfile entry wins over a
//! requirements.txt entry. Conversion to [`PackageDbEntry`] happens at
//! the module boundary so the rest of the scan pipeline (dedup, CPE
//! synthesis, compositions, deps.dev enrichment) handles Python the
//! same way it handles deb / apk today.
//!
//! `pyproject.toml`-only projects (no venv, no lockfile, no
//! requirements) emit zero components per FR-005 — `[project.dependencies]`
//! holds build specs, not resolved versions, so fabricating components
//! from it would bloat SBOMs with phantoms.

use std::path::{Path, PathBuf};

use mikebom_common::types::purl::encode_purl_segment;

use super::PackageDbEntry;


// ========================================================================
// Module structure (milestone 018)
// ========================================================================
//
// pip/ split layout (per specs/018-module-splits/contracts/module-boundaries.md):
//   - dist_info.rs       — Tier 1: venv PEP 376 walker + METADATA parser +
//                          extract_license + collect_claimed_paths
//   - poetry.rs          — Tier 2: poetry.lock v1/v2 parser
//   - pipfile.rs         — Tier 3: Pipfile.lock parser
//   - requirements_txt.rs — Tier 3: requirements*.txt parser
//
// This file (mod.rs) hosts the orchestrator (pub fn read), shared PURL
// helpers (build_pypi_purl_str / normalize_pypi_name_for_purl), the PEP 508
// requires-dist tokenizer (used by both dist_info and requirements_txt),
// the project-root walker, and the merge_without_override drift-resolution
// helper.

mod dist_info;
mod pipfile;
mod poetry;
mod requirements_txt;
mod uv_lock;

pub use dist_info::collect_claimed_paths;

/// Normalise a pypi package name into the form the packageurl-python
/// reference implementation emits in canonical PURLs: lowercase, with
/// every `_` replaced by `-`. Other separators (dots, multi-hyphens)
/// are preserved — PEP 503 collapses them but packageurl-python does
/// not, and we align with the reference impl for byte-for-byte
/// conformance per SC-004.
///
/// `component.name` (what we store on `ResolvedComponent` for CycloneDX
/// display) keeps the declared form from the source (e.g. `Flask`,
/// `MarkupSafe`); only the PURL goes through this transform.
pub(crate) fn normalize_pypi_name_for_purl(name: &str) -> String {
    name.replace('_', "-").to_lowercase()
}

/// Build a canonical pypi PURL string from (possibly mixed-case, possibly
/// underscored) name and version. Normalises both name and version per
/// the packageurl-python reference implementation, then runs each
/// through the common segment encoder so `+` → `%2B`.
fn build_pypi_purl_str(name: &str, version: &str) -> String {
    let normalized_name = normalize_pypi_name_for_purl(name);
    if version.is_empty() {
        format!("pkg:pypi/{}", encode_purl_segment(&normalized_name))
    } else {
        format!(
            "pkg:pypi/{}@{}",
            encode_purl_segment(&normalized_name),
            encode_purl_segment(version),
        )
    }
}

/// Public entry point. Walks the scan root for Python package sources
/// and emits one `PackageDbEntry` per unique package identity. Drift
/// between sources is resolved per R8 (venv > lockfile > requirements).
///
/// * `include_dev` — when true, Poetry / Pipfile entries flagged as
///   dev-only are included; when false they're filtered out at source.
///   Venv dist-info and requirements.txt entries don't carry a dev/prod
///   distinction and are always emitted.
pub fn read(
    rootfs: &Path,
    include_dev: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    let mut entries: Vec<PackageDbEntry> = Vec::new();

    // Tier 1: installed venvs. The venv enumerator already handles
    // standard venv layouts (`.venv/`, `/usr/lib/python*/`, etc.) —
    // it runs once against the rootfs regardless of project-root
    // structure because site-packages trees are globally addressable.
    let venv_entries = dist_info::read_venv_dist_info(rootfs);
    let had_venv = !venv_entries.is_empty();
    entries.extend(venv_entries);

    // Tiers 2 + 3: per-project-root tier readers. A "project root" is
    // any directory containing a Python project marker (poetry.lock,
    // Pipfile.lock, requirements*.txt, or pyproject.toml). This makes
    // the scanner handle arbitrary layouts with one mechanism:
    // - Single project at rootfs (directory scan) — one root, same as
    //   before.
    // - Container image with `/usr/src/app/pyproject.toml` — walker
    //   finds that directory without a hard-coded path list.
    // - Monorepo with `services/api/requirements.txt`,
    //   `services/worker/Pipfile.lock`, etc. — each becomes its own
    //   root, so per-service declarations surface.
    let mut had_project_marker = false;
    for project_root in candidate_python_project_roots(rootfs, exclude_set) {
        // A project is anything holding a lockfile / requirements /
        // pyproject; track this for the "pyproject.toml only" skip log
        // below. Tier 1 venv does NOT count as a project root here —
        // that's installed state, not a project declaration.
        had_project_marker = true;

        if let Some(lockfile_entries) = poetry::read_poetry_lock(&project_root, include_dev) {
            merge_without_override(&mut entries, lockfile_entries);
        }
        if let Some(lockfile_entries) = pipfile::read_pipfile_lock(&project_root, include_dev) {
            merge_without_override(&mut entries, lockfile_entries);
        }
        // Milestone 106 US1 (issue #276): uv.lock support. Sibling to
        // poetry / pipfile readers — dispatched per-project-root with
        // the same merge_without_override dedup semantics. Returns
        // workspace-root + members + transitives when the root
        // pyproject.toml declares [tool.uv.workspace].
        if let Some(lockfile_entries) = uv_lock::read_uv_lock(&project_root, include_dev) {
            merge_without_override(&mut entries, lockfile_entries);
        }
        if let Some(req_entries) = requirements_txt::read_requirements_files(&project_root) {
            merge_without_override(&mut entries, req_entries);
        }
    }

    // If the root has a `pyproject.toml` but nothing else, log the skip
    // so operators can tell an empty-output run from "we didn't find
    // anything to scan." Per FR-024. The rootfs-level check stays
    // unchanged so the existing pyproject-only behavior is preserved.
    if entries.is_empty()
        && !had_venv
        && !had_project_marker
        && rootfs.join("pyproject.toml").is_file()
    {
        tracing::info!(
            rootfs = %rootfs.display(),
            "python project detected but no venv, lockfile, or requirements.txt — skipping"
        );
    }

    // Milestone 068 — Phase A: emit one main-module per pyproject.toml
    // with PEP 621 [project] table. Augment-existing-or-emit-new
    // pattern mirrors cargo (064) / npm (066). Editable-install merge
    // (FR-011): when a Tier-1 venv-derived entry from above shares the
    // same PURL, augment in-place — venv evidence wins for sbom_tier /
    // hashes, Phase A adds the C40 tag + parent_purl: None.
    let mut main_modules_emitted = 0usize;
    let mut poetry_skips = 0usize;
    for project_root in candidate_python_project_roots(rootfs, exclude_set) {
        let (synthesized, was_poetry_only) =
            build_pip_main_module_entry(&project_root);
        if was_poetry_only {
            poetry_skips += 1;
            tracing::info!(
                manifest = %project_root.join("pyproject.toml").display(),
                "pip: skipping main-module emission for [tool.poetry]-only pyproject.toml — Poetry schema deferred per #104",
            );
            continue;
        }
        let Some(synthesized) = synthesized else {
            continue;
        };
        let purl_key = synthesized.purl.as_str().to_string();
        if let Some(existing) = entries.iter_mut().find(|e| e.purl.as_str() == purl_key) {
            // FR-011: augment-existing — when a same-PURL Tier-1 venv or
            // lockfile-derived entry exists, layer C40 + parent_purl
            // None on top while preserving the existing entry's
            // sbom_tier / hashes / evidence_kind (venv evidence wins).
            for (k, v) in synthesized.extra_annotations.iter() {
                existing
                    .extra_annotations
                    .entry(k.clone())
                    .or_insert_with(|| v.clone());
            }
            existing.parent_purl = None;
            // Merge synthesized depends into existing depends, dedup —
            // Phase A's PEP 621 dep set may be a superset of what the
            // lockfile / requirements.txt resolved (extras not pinned
            // there).
            let existing_deps: std::collections::HashSet<String> =
                existing.depends.iter().cloned().collect();
            for d in &synthesized.depends {
                if !existing_deps.contains(d) {
                    existing.depends.push(d.clone());
                }
            }
            // sbom_tier: preserve existing if set (venv "deployed" or
            // lockfile "source" wins); only fall back to synthesized's
            // "source" when existing is None.
            if existing.sbom_tier.is_none() {
                existing.sbom_tier = synthesized.sbom_tier.clone();
            }
            main_modules_emitted += 1;
        } else {
            entries.push(synthesized);
            main_modules_emitted += 1;
        }
    }

    // Milestone 068 same-PURL dedup. Rare given site-packages/__pycache__
    // are excluded from manifest discovery, but defensive (mirrors the
    // cargo / npm convention).
    let dedup_drops = dedup_pip_main_modules_by_purl(&mut entries);
    if !dedup_drops.is_empty() {
        let dropped_paths: Vec<String> = dedup_drops
            .iter()
            .map(|d| d.dropped_path.clone())
            .collect();
        let kept_path = dedup_drops
            .first()
            .map(|d| d.kept_path.clone())
            .unwrap_or_default();
        let example_purl = dedup_drops
            .first()
            .map(|d| d.purl.clone())
            .unwrap_or_default();
        tracing::warn!(
            count = dedup_drops.len(),
            example_purl = %example_purl,
            kept = %kept_path,
            dropped = ?dropped_paths,
            "pip: deduped same-PURL pyproject.toml files",
        );
    }
    if main_modules_emitted > 0 || poetry_skips > 0 {
        tracing::info!(
            rootfs = %rootfs.display(),
            main_modules_emitted,
            poetry_only_skips = poetry_skips,
            same_purl_duplicates_dropped = dedup_drops.len(),
            "pip: emitted main-module components",
        );
    }

    entries
}

/// Max depth for the recursive Python project-root search. Same budget
/// as `candidate_project_roots` in `npm.rs` — covers realistic monorepo
/// plus image layouts (`usr/src/app/services/api/` = 4 levels) without
/// running away into deep source trees.
const MAX_PROJECT_ROOT_DEPTH: usize = 6;

/// Enumerate every directory under `rootfs` that looks like a Python
/// project root (holds a poetry.lock, Pipfile.lock, requirements*.txt,
/// or pyproject.toml). Always includes `rootfs` itself so the single-
/// project case is unchanged. Recurses up to `MAX_PROJECT_ROOT_DEPTH`
/// levels via the shared
/// [`super::project_roots::walk_for_project_roots`] helper.
fn candidate_python_project_roots(
    rootfs: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PathBuf> {
    use super::project_roots::should_skip_default_descent;
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_PROJECT_ROOT_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            // Default skip set + python's `site-packages` (handled
            // separately by `read_venv_dist_info`).
            candidate
                .file_name()
                .and_then(|s| s.to_str())
                .map(|name| should_skip_default_descent(name) || name == "site-packages")
                .unwrap_or(true)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_dir() && has_python_project_marker(path) {
            out.push(path.to_path_buf());
        }
    });
    out
}

/// True when `dir` holds any Python project-root marker. Installed
/// state (site-packages, dist-info) is NOT a project marker — it's
/// the output of a project, handled by `read_venv_dist_info` on its
/// own pass.
fn has_python_project_marker(dir: &Path) -> bool {
    if dir.join("poetry.lock").is_file()
        || dir.join("Pipfile.lock").is_file()
        || dir.join("uv.lock").is_file()
        || dir.join("pyproject.toml").is_file()
    {
        return true;
    }
    // `requirements*.txt` is a glob — scan the top-level of `dir`.
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("requirements") && name.ends_with(".txt") {
                    return true;
                }
            }
        }
    }
    false
}

/// Merge `additions` into `entries`, dropping any addition whose PURL
/// already exists in `entries`. Preserves insertion order; additions
/// that DO land are appended at the tail.
fn merge_without_override(
    entries: &mut Vec<PackageDbEntry>,
    additions: Vec<PackageDbEntry>,
) {
    use std::collections::HashSet;
    let existing: HashSet<String> = entries
        .iter()
        .map(|e| e.purl.as_str().to_string())
        .collect();
    for a in additions {
        if !existing.contains(a.purl.as_str()) {
            entries.push(a);
        }
    }
}

// ---------------------------------------------------------------------------
// Milestone 068 — pip source-tree main-module component (PEP 621 pyproject.toml)
// ---------------------------------------------------------------------------

/// Record describing a duplicate main-module dropped during dedup,
/// returned in batch from `dedup_pip_main_modules_by_purl` for
/// caller-side `tracing::warn!` emission. Mirrors cargo (064) / npm (066).
#[derive(Debug, Clone)]
pub(crate) struct DroppedDuplicate {
    pub purl: String,
    pub kept_path: String,
    pub dropped_path: String,
}

/// Build the pip main-module entry for a single `pyproject.toml`.
///
/// Returns `None` when:
/// - `pyproject.toml` is absent, malformed, or unreadable.
/// - `[project]` table is absent (Poetry-only schema or non-Python
///   `pyproject.toml`). Per FR-002, a `tracing::info!` is emitted at
///   the orchestration site (not here) when a `[tool.poetry]`-only
///   schema is detected, so operators can see the deliberate skip.
/// - `[project].name` is absent.
///
/// Otherwise emits a `PackageDbEntry` with:
/// - PURL `pkg:pypi/<pep503-normalized-name>@<version>` via
///   `build_pypi_purl_str`.
/// - `version`: literal `[project].version` if present, else
///   `"0.0.0-unknown"` placeholder per FR-001 + spec Q1 (matching
///   the cross-host determinism convention from milestones 053/064/066).
///   When `[project].dynamic` contains `"version"`, the placeholder
///   is the documented deferral target — no setuptools-scm shellout.
/// - `parent_purl: None` (top-level — FR-001a).
/// - `sbom_tier: Some("source")` (FR-006); overridden to `"deployed"`
///   downstream when augment-existing merges with a Tier-1 venv entry
///   (FR-011, in `read()`).
/// - `extra_annotations` carries `mikebom:component-role: "main-module"`
///   (C40, FR-004).
/// - `licenses: vec![]` (FR-005; license detection is #103 follow-up).
/// - `depends`: direct-dep package names extracted from
///   `[project.dependencies]` and each `[project.optional-dependencies].*`
///   array. PEP 508 requirement strings are split on whitespace and
///   the first token is taken as the package name (consistent with
///   how `requirements_txt.rs` handles the same shape — markers and
///   version specifiers are stripped).
///
/// Returns `(Option<PackageDbEntry>, bool)` where the bool flag is
/// `true` if this manifest was Poetry-only (`[tool.poetry]` present
/// AND `[project]` absent), so the caller can emit FR-002's
/// info-level skip log without re-reading the manifest.
pub(crate) fn build_pip_main_module_entry(
    project_root: &Path,
) -> (Option<PackageDbEntry>, bool) {
    let manifest_path = project_root.join("pyproject.toml");
    let Ok(text) = std::fs::read_to_string(&manifest_path) else {
        return (None, false);
    };
    let Ok(parsed) = toml::from_str::<toml::Value>(&text) else {
        return (None, false);
    };
    let project_table = parsed.get("project");
    let has_poetry_table = parsed
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .is_some();
    // FR-002: Poetry-only (no [project], yes [tool.poetry]) → skip
    // emission and signal to caller for the info-level log.
    if project_table.is_none() {
        return (None, has_poetry_table);
    }
    let project = project_table.expect("checked above");
    let Some(name) = project.get("name").and_then(|v| v.as_str()) else {
        return (None, false);
    };
    // Resolve version per FR-001 + spec Q1:
    //   1. literal `[project].version` string → use verbatim
    //   2. otherwise → `"0.0.0-unknown"` placeholder
    // The dynamic-version case (`[project].dynamic` contains "version")
    // and the missing-field case both fall through to step 2; the
    // missing-without-dynamic case additionally emits a warn-level log
    // since that's a malformed PEP 621 manifest.
    let version_field = project.get("version").and_then(|v| v.as_str());
    let dynamic_has_version = project
        .get("dynamic")
        .and_then(|v| v.as_array())
        .is_some_and(|arr| {
            arr.iter().any(|x| x.as_str() == Some("version"))
        });
    let version = match (version_field, dynamic_has_version) {
        (Some(v), _) => v.to_string(),
        (None, true) => "0.0.0-unknown".to_string(),
        (None, false) => {
            tracing::warn!(
                manifest = %manifest_path.display(),
                name = %name,
                "pip: pyproject.toml [project] has neither `version` nor `dynamic = [\"version\"]` — using 0.0.0-unknown placeholder",
            );
            "0.0.0-unknown".to_string()
        }
    };
    let purl_str = build_pypi_purl_str(name, &version);
    let Ok(purl) = mikebom_common::types::purl::Purl::new(&purl_str) else {
        return (None, has_poetry_table);
    };
    // Direct deps from [project.dependencies] and
    // [project.optional-dependencies].* per FR-007. PEP 508 strings:
    // take the first whitespace-or-`[<>=;`-delimited token as the name.
    let mut depends: Vec<String> = Vec::new();
    let take_first_token = |s: &str| -> String {
        s.chars()
            .take_while(|c| {
                !matches!(c, ' ' | '\t' | '[' | ']' | '<' | '>' | '=' | ';' | '~' | '!')
            })
            .collect::<String>()
            .trim()
            .to_string()
    };
    if let Some(deps) = project.get("dependencies").and_then(|v| v.as_array()) {
        for d in deps.iter().filter_map(|v| v.as_str()) {
            let token = take_first_token(d);
            if !token.is_empty() {
                depends.push(token);
            }
        }
    }
    if let Some(opt_table) = project
        .get("optional-dependencies")
        .and_then(|v| v.as_table())
    {
        for (_extra_name, deps) in opt_table {
            if let Some(arr) = deps.as_array() {
                for d in arr.iter().filter_map(|v| v.as_str()) {
                    let token = take_first_token(d);
                    if !token.is_empty() {
                        depends.push(token);
                    }
                }
            }
        }
    }
    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "mikebom:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );

    // Milestone 116 — produces-binaries extraction per FR-007 (pip).
    // PEP 621 `[project.scripts]` and `[project.gui-scripts]` are
    // tables mapping `<binary-name>` → `<module:func>`. Each key is one
    // produced binary name. Setup.cfg fallback (`[options.entry_points]`
    // `console_scripts` + `gui_scripts`) runs when neither pyproject
    // key exists OR when pyproject exists but declares no scripts —
    // supports legacy + mid-migration projects per spec clarification.
    {
        let mut binary_candidates: Vec<String> = Vec::new();
        for key in ["scripts", "gui-scripts"] {
            if let Some(table) = project.get(key).and_then(|v| v.as_table()) {
                for entry_name in table.keys() {
                    binary_candidates.push(entry_name.clone());
                }
            }
        }
        if binary_candidates.is_empty() {
            binary_candidates.extend(extract_pip_setupcfg_scripts(project_root));
        }
        crate::scan_fs::produces_binaries::stamp_into_annotations(
            &mut extra_annotations,
            binary_candidates,
        );
    }

    let source_path = format!("path+file://{}", project_root.display());
    let entry = PackageDbEntry {
        build_inclusion: None,
        purl,
        name: name.to_string(),
        version,
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
    };
    (Some(entry), false)
}

/// Dedup main-module entries by PURL, preserving the first occurrence.
/// Mirrors cargo's `dedup_main_modules_by_purl` from milestone 064 T010.
/// Predicate is C40-tag-driven; non-main-module pip entries are
/// untouched even if their PURLs would collide.
/// Milestone 116 — fallback for projects whose binary names live in
/// `setup.cfg`'s `[options.entry_points]` table rather than (or in
/// addition to) `pyproject.toml`. Two key names contribute names:
/// `console_scripts` and `gui_scripts`. Each line under those keys is
/// `<binary-name> = <module>:<func>`; we take the LHS of the `=`.
fn extract_pip_setupcfg_scripts(project_root: &Path) -> Vec<String> {
    let setupcfg_path = project_root.join("setup.cfg");
    let Ok(text) = std::fs::read_to_string(&setupcfg_path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut in_entry_points = false;
    let mut in_scripts_subkey = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(section) = trimmed
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
        {
            in_entry_points = section == "options.entry_points";
            in_scripts_subkey = false;
            continue;
        }
        if !in_entry_points {
            continue;
        }
        // setup.cfg sub-key shape: `console_scripts =` or
        // `gui_scripts =` on its own line followed by indented entries.
        if let Some(key) = trimmed.strip_suffix('=').map(str::trim) {
            in_scripts_subkey =
                matches!(key, "console_scripts" | "gui_scripts");
            continue;
        }
        if !in_scripts_subkey || trimmed.is_empty() {
            continue;
        }
        // Entry shape: `<name> = <module>:<func>`. Take the LHS.
        if let Some((name, _)) = trimmed.split_once('=') {
            let name = name.trim();
            if !name.is_empty() {
                out.push(name.to_string());
            }
        }
    }
    out
}

pub(crate) fn dedup_pip_main_modules_by_purl(
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


// -----------------------------------------------------------------------
// Tier 1 support: PEP 508 Requires-Dist tokenizer
// -----------------------------------------------------------------------

/// Extract the bare package name from a PEP 508 requirement string.
/// Returns `None` if the environment marker (e.g. `; python_version < "3.10"`)
/// evaluates to false for the current interpreter, or if parsing fails.
///
/// Handles:
/// - Bare names: `requests`
/// - Names with extras: `requests[security]`
/// - Names with version specs: `requests >= 2.28, < 3`
/// - Environment markers: `requests ; python_version >= "3.8"`
/// - Combined: `requests[security] (>= 2.28) ; python_version >= "3.8"`
pub(crate) fn tokenise_requires_dist_name(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    // Split on `;` for env markers. Preserve only the LHS for name
    // extraction; evaluate the marker to decide whether to emit.
    let (head, marker) = match raw.split_once(';') {
        Some((h, m)) => (h.trim(), Some(m.trim())),
        None => (raw, None),
    };

    // Evaluate marker (best-effort): if the marker references
    // sys_platform, python_version, or similar and evaluates to false,
    // drop the requirement.
    if let Some(m) = marker {
        if !marker_probably_matches(m) {
            return None;
        }
    }

    // Extract the name — everything up to the first separator:
    // space, `[` (extras), `(` (version spec), `<`, `>`, `=`, `!`, `~`, `@`.
    let end = head
        .find(|c: char| {
            c.is_whitespace()
                || matches!(c, '[' | '(' | '<' | '>' | '=' | '!' | '~' | '@')
        })
        .unwrap_or(head.len());
    let name = head[..end].trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Best-effort PEP 508 environment-marker evaluator. We only handle the
/// common cases (`python_version`, `sys_platform`, `platform_system`)
/// and return true conservatively for anything we can't evaluate — it's
/// better to include a possibly-unused dep than to silently drop one we
/// didn't understand.
fn marker_probably_matches(marker: &str) -> bool {
    // Quick conservative check: if the marker mentions "extra ==", treat
    // as false (extras are opt-in and we don't request any).
    if marker.contains("extra ==") {
        return false;
    }
    // Everything else: conservative true. The full PEP 508 grammar is
    // out of scope for the scanner's "identify packages" purpose; edge
    // cases at most cause a slight over-inclusion which the dedup path
    // cleans up.
    true
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    #[test]
    fn tokenise_bare_name() {
        assert_eq!(tokenise_requires_dist_name("requests"), Some("requests".into()));
    }

    #[test]
    fn tokenise_name_with_extras() {
        assert_eq!(
            tokenise_requires_dist_name("requests[security,socks]"),
            Some("requests".into())
        );
    }

    #[test]
    fn tokenise_name_with_version_spec() {
        assert_eq!(
            tokenise_requires_dist_name("requests >= 2.28, < 3"),
            Some("requests".into())
        );
        assert_eq!(
            tokenise_requires_dist_name("requests>=2.28"),
            Some("requests".into())
        );
    }

    #[test]
    fn tokenise_name_with_env_marker_that_probably_matches() {
        assert_eq!(
            tokenise_requires_dist_name("requests ; python_version >= \"3.8\""),
            Some("requests".into())
        );
    }

    #[test]
    fn tokenise_env_marker_with_extra_drops_requirement() {
        // `extra ==` markers mean "only when this optional extra is
        // requested" — we don't request any, so drop the dep.
        assert_eq!(
            tokenise_requires_dist_name("pytest ; extra == 'dev'"),
            None
        );
    }

    #[test]
    fn tokenise_empty_returns_none() {
        assert_eq!(tokenise_requires_dist_name(""), None);
        assert_eq!(tokenise_requires_dist_name("   "), None);
    }

    #[test]
    fn normalize_pypi_name_lowercases_and_flips_underscores() {
        // Reference impl (packageurl-python) canonicalises pypi names
        // to lowercase with `_` → `-`. Mikebom follows suit so PURLs
        // round-trip byte-for-byte (SC-004).
        assert_eq!(normalize_pypi_name_for_purl("Flask"), "flask");
        assert_eq!(normalize_pypi_name_for_purl("MarkupSafe"), "markupsafe");
        assert_eq!(normalize_pypi_name_for_purl("Jinja2"), "jinja2");
        assert_eq!(
            normalize_pypi_name_for_purl("zope.interface"),
            "zope.interface" // dots preserved per reference impl
        );
        assert_eq!(
            normalize_pypi_name_for_purl("typing_extensions"),
            "typing-extensions"
        );
        assert_eq!(
            normalize_pypi_name_for_purl("Pillow_SIMD"),
            "pillow-simd"
        );
    }

    #[test]
    fn build_pypi_purl_str_emits_canonical_form() {
        // Declared-form input → canonical output.
        assert_eq!(
            build_pypi_purl_str("Flask", "3.0.0"),
            "pkg:pypi/flask@3.0.0"
        );
        assert_eq!(
            build_pypi_purl_str("MarkupSafe", "2.1.3"),
            "pkg:pypi/markupsafe@2.1.3"
        );
        assert_eq!(
            build_pypi_purl_str("typing_extensions", "4.9.0"),
            "pkg:pypi/typing-extensions@4.9.0"
        );
    }

    #[test]
    fn monorepo_finds_requirements_in_each_service() {
        // Multi-service Python layout — no single top-level project
        // marker; each service has its own requirements.txt.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for (svc, pkg) in [("api", "fastapi"), ("worker", "celery"), ("web", "flask")] {
            let svc_dir = root.join("services").join(svc);
            std::fs::create_dir_all(&svc_dir).unwrap();
            std::fs::write(
                svc_dir.join("requirements.txt"),
                format!("{pkg}==1.0.0\n"),
            )
            .unwrap();
        }
        let out = read(root, false, &Default::default());
        let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"fastapi"), "got {names:?}");
        assert!(names.contains(&"celery"), "got {names:?}");
        assert!(names.contains(&"flask"), "got {names:?}");
    }

    #[test]
    fn python_walk_finds_nested_pyproject_under_usr_src() {
        // Image-style layout: pyproject.toml + requirements.txt live
        // at /usr/src/app/, rootfs is /.
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("usr/src/app");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(
            app.join("pyproject.toml"),
            "[project]\nname = \"myapp\"\n",
        )
        .unwrap();
        std::fs::write(app.join("requirements.txt"), "httpx==0.25.0\n").unwrap();
        let out = read(dir.path(), false, &Default::default());
        // Pre-068: only `httpx` (the requirements.txt-derived dep).
        // Post-068: `httpx` + the milestone-068 main-module component
        // emitted from the same project's pyproject.toml [project] table.
        assert_eq!(out.len(), 2);
        let names: std::collections::HashSet<&str> =
            out.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains("httpx"));
        assert!(names.contains("myapp"));
    }

    #[test]
    fn python_walk_skips_venv_and_node_modules_noise() {
        // Planted stray pyproject.toml / requirements.txt inside
        // venv/ and node_modules/ — both must be ignored by the walk.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for noisy_parent in ["venv/lib/python3.11/site-packages/evil", "node_modules/evil"] {
            let noisy = root.join(noisy_parent);
            std::fs::create_dir_all(&noisy).unwrap();
            std::fs::write(
                noisy.join("requirements.txt"),
                "should-not-appear==9.9.9\n",
            )
            .unwrap();
        }
        let out = read(root, false, &Default::default());
        assert!(
            !out.iter().any(|e| e.name == "should-not-appear"),
            "walker must not descend into venv/ or node_modules/"
        );
    }

    // -------------------------------------------------------------------
    // Milestone 068 — main-module emission helpers (T007)
    // -------------------------------------------------------------------

    fn write_pyproject(dir: &std::path::Path, contents: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("pyproject.toml"), contents).unwrap();
    }

    #[test]
    fn build_pip_main_module_pep621_basic_emits_entry() {
        let tmp = tempfile::tempdir().unwrap();
        write_pyproject(
            tmp.path(),
            r#"
[project]
name = "my_pkg"
version = "1.0.0"
"#,
        );
        let (entry, was_poetry_only) = build_pip_main_module_entry(tmp.path());
        assert!(!was_poetry_only);
        let entry = entry.unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:pypi/my-pkg@1.0.0");
        assert_eq!(entry.name, "my_pkg"); // verbatim manifest value
        assert_eq!(entry.version, "1.0.0");
        assert_eq!(entry.parent_purl, None);
        assert_eq!(entry.sbom_tier.as_deref(), Some("source"));
        assert_eq!(
            entry
                .extra_annotations
                .get("mikebom:component-role")
                .and_then(|v| v.as_str()),
            Some("main-module")
        );
    }

    #[test]
    fn build_pip_main_module_pep503_normalizes_name_in_purl() {
        let tmp = tempfile::tempdir().unwrap();
        write_pyproject(
            tmp.path(),
            r#"
[project]
name = "Some_Package_Name"
version = "0.5.0"
"#,
        );
        let (entry, _) = build_pip_main_module_entry(tmp.path());
        let entry = entry.unwrap();
        // PEP 503 normalization (per existing
        // `normalize_pypi_name_for_purl`): underscore → hyphen,
        // lowercase. Dots are preserved (matches the existing
        // `normalize_pypi_name_for_purl` helper which mirrors the
        // packageurl-python reference impl, NOT strict PEP 503).
        assert_eq!(entry.purl.as_str(), "pkg:pypi/some-package-name@0.5.0");
        assert_eq!(entry.name, "Some_Package_Name");
    }

    #[test]
    fn build_pip_main_module_dynamic_version_uses_placeholder() {
        let tmp = tempfile::tempdir().unwrap();
        write_pyproject(
            tmp.path(),
            r#"
[project]
name = "dyn-app"
dynamic = ["version"]
"#,
        );
        let (entry, _) = build_pip_main_module_entry(tmp.path());
        let entry = entry.unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:pypi/dyn-app@0.0.0-unknown");
        assert_eq!(entry.version, "0.0.0-unknown");
    }

    #[test]
    fn build_pip_main_module_poetry_only_returns_none_with_flag() {
        let tmp = tempfile::tempdir().unwrap();
        write_pyproject(
            tmp.path(),
            r#"
[tool.poetry]
name = "poetry-only-app"
version = "1.0.0"
"#,
        );
        let (entry, was_poetry_only) = build_pip_main_module_entry(tmp.path());
        assert!(entry.is_none());
        assert!(was_poetry_only);
    }

    #[test]
    fn build_pip_main_module_both_schemas_emits_from_project() {
        // FR-003: when both [project] and [tool.poetry] are present,
        // emit from [project] (the standards-native PEP 621 source).
        let tmp = tempfile::tempdir().unwrap();
        write_pyproject(
            tmp.path(),
            r#"
[project]
name = "shim-app"
version = "2.0.0"

[tool.poetry]
name = "shim-app"
version = "1.0.0"
"#,
        );
        let (entry, was_poetry_only) = build_pip_main_module_entry(tmp.path());
        assert!(!was_poetry_only);
        let entry = entry.unwrap();
        // [project].version wins (2.0.0), not [tool.poetry].version (1.0.0)
        assert_eq!(entry.version, "2.0.0");
    }

    #[test]
    fn build_pip_main_module_missing_version_no_dynamic_emits_placeholder() {
        let tmp = tempfile::tempdir().unwrap();
        write_pyproject(
            tmp.path(),
            r#"
[project]
name = "broken-pep621"
"#,
        );
        let (entry, _) = build_pip_main_module_entry(tmp.path());
        let entry = entry.unwrap();
        // Lenient parse: emit with placeholder + warn (warn isn't
        // captured here but the placeholder behavior is verified).
        assert_eq!(entry.version, "0.0.0-unknown");
    }

    #[test]
    fn build_pip_main_module_missing_project_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        write_pyproject(
            tmp.path(),
            r#"
[build-system]
requires = ["setuptools"]
"#,
        );
        let (entry, was_poetry_only) = build_pip_main_module_entry(tmp.path());
        assert!(entry.is_none());
        assert!(!was_poetry_only); // no [tool.poetry] either, so flag is false
    }

    #[test]
    fn build_pip_main_module_emits_direct_deps_from_dependencies() {
        let tmp = tempfile::tempdir().unwrap();
        write_pyproject(
            tmp.path(),
            r#"
[project]
name = "with-deps"
version = "1.0.0"
dependencies = [
  "requests>=2.0",
  "click ~= 8.0",
  "rich; python_version >= '3.10'",
]
"#,
        );
        let (entry, _) = build_pip_main_module_entry(tmp.path());
        let entry = entry.unwrap();
        // PEP 508 first-token extraction: name only (no specs, no markers).
        let names: std::collections::HashSet<String> =
            entry.depends.iter().cloned().collect();
        assert!(names.contains("requests"));
        assert!(names.contains("click"));
        assert!(names.contains("rich"));
    }

    fn make_main_module_entry(name: &str, version: &str, source_path: &str) -> PackageDbEntry {
        let purl_str = build_pypi_purl_str(name, version);
        let purl = mikebom_common::types::purl::Purl::new(&purl_str).unwrap();
        let mut extra: std::collections::BTreeMap<String, serde_json::Value> =
            Default::default();
        extra.insert(
            "mikebom:component-role".to_string(),
            serde_json::Value::String("main-module".to_string()),
        );
        PackageDbEntry {
            build_inclusion: None,
            purl,
            name: name.to_string(),
            version: version.to_string(),
            arch: None,
            source_path: source_path.to_string(),
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
        }
    }

    #[test]
    fn dedup_pip_main_modules_no_collision_returns_empty() {
        let mut entries = vec![
            make_main_module_entry("a", "1.0.0", "/tmp/a"),
            make_main_module_entry("b", "1.0.0", "/tmp/b"),
        ];
        let drops = dedup_pip_main_modules_by_purl(&mut entries);
        assert_eq!(entries.len(), 2);
        assert!(drops.is_empty());
    }

    #[test]
    fn dedup_pip_main_modules_two_same_purl_keeps_first() {
        let mut entries = vec![
            make_main_module_entry("foo", "1.2.3", "/tmp/proj/pyproject.toml"),
            make_main_module_entry("foo", "1.2.3", "/tmp/proj/vendor/pyproject.toml"),
        ];
        let drops = dedup_pip_main_modules_by_purl(&mut entries);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source_path, "/tmp/proj/pyproject.toml");
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].dropped_path, "/tmp/proj/vendor/pyproject.toml");
    }
}
