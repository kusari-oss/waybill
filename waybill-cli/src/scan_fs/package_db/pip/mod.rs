//! Read Python package metadata from a scanned filesystem.
//!
//! Four layered sources in order of authority (per spec FR-001..FR-005,
//! research.md R2 / R3, and milestone 670's m018-policy reversal):
//!
//! 1. **Installed venv**: `<root>/.../site-packages/<name>-<version>.dist-info/METADATA`
//!    — confidence 0.85, tier `deployed`. Ground truth: these packages are
//!    actually resolved and sitting on disk.
//! 2. **Lockfile**: `poetry.lock` (v1 and v2 formats), `Pipfile.lock`, or
//!    `uv.lock` — confidence 0.85, tier `source`. Authoritative about
//!    what WILL be installed if the lockfile is honoured.
//! 3. **Requirements file**: `requirements.txt` (and any `*.txt` matching
//!    pip's convention) — confidence 0.70, tier `design`. Best-guess:
//!    range specs may resolve to different versions depending on the
//!    registry state at install time.
//! 4. **Manifest (`pyproject.toml`)**: PEP 621 `[project.dependencies]`,
//!    PEP 735 `[dependency-groups]`, and Poetry-legacy
//!    `[tool.poetry.dependencies]` / `[tool.poetry.group.*.dependencies]`
//!    sections — tier `design`. Emitted with `version = "unresolved"` and
//!    a m236 `waybill:unresolved-reason = "declared in pyproject.toml; no
//!    uv.lock / poetry.lock / Pipfile.lock fallback"` when the tier-2/3
//!    sources are absent for a given package. Enables SBOM coverage for
//!    modern Python projects that ship `pyproject.toml` without pinning a
//!    lockfile in-tree (e.g. `microsoft/markitdown`).
//!
//! The public entry point [`read`] walks these in order and applies
//! drift resolution per research.md R8: a venv entry wins over a
//! lockfile entry for the same package; a lockfile entry wins over a
//! requirements.txt entry; a requirements.txt entry wins over a
//! manifest entry. Conversion to [`PackageDbEntry`] happens at
//! the module boundary so the rest of the scan pipeline (dedup, CPE
//! synthesis, compositions, deps.dev enrichment) handles Python the
//! same way it handles deb / apk today.
//!
//! **History**: The milestone-018-era policy was "`pyproject.toml`-only
//! projects emit zero components — `[project.dependencies]` holds build
//! specs, not resolved versions, so fabricating components from it would
//! bloat SBOMs with phantoms." Milestone 670 reverses this: pyproject-
//! declared deps ARE emitted as design-tier components with the m236
//! `unresolved-reason` annotation surfacing the boundary. The
//! phantoms-argument doesn't apply once we tier them as `design` +
//! `waybill:unresolved-reason` — downstream consumers can filter design-
//! tier or the specific reason string when they only want resolved
//! components. The under-detection surfaced by the 2026-08-31 sweep
//! (issue #743) made the completeness cost of the m018 policy visible:
//! markitdown (4 → target ≥30), OctoPrint (3 → target ≥30), cpython
//! (16 → target ≥25).

use std::path::{Path, PathBuf};

use waybill_common::types::purl::encode_purl_segment;

use super::name_validation::{validate_pep508_name, NameValidationError};
use super::PackageDbEntry;

// Milestone 664 US2 T036: shared-walker migration types.
use crate::scan_fs::walk_registry::{
    globset_from_patterns, ReaderId, ReaderRegistration, ReaderRegistryBuilder,
    SharedWalker, SharedWalkerContext,
};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};


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
pub(crate) mod uv_lock;

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

// Milestone 664 US2 T036: shared-walker types + registration + finalize.
// The legacy `pub fn read()` doc block that follows describes the LEGACY
// entry point's behavior; the shared-walker path (via `run_shared_walker_pilot`
// in package_db/mod.rs) invokes `finalize()` directly with precomputed paths.

/// Milestone 664 US2 T036: per-scan state carried through
/// `ReaderRegistration.state`. Accumulates the set of directories
/// containing at least one Python project-root marker as the shared
/// walker traverses; post-walker `finalize()` iterates the sorted
/// unique dirs and runs the existing Tier-1 venv + Tier-2/3 lockfile
/// pipeline. `HashSet` here provides free dedup — the same marker
/// file only ever has ONE parent directory, but different marker
/// files (pyproject.toml + poetry.lock) in the SAME dir would both
/// try to insert; the set collapses them.
///
/// Wrapped in `Mutex` for future-parallel-dispatch safety per FR-012's
/// post-milestone follow-on.
#[derive(Default, Debug)]
pub(crate) struct PipDiscoveredPaths {
    pub(crate) project_roots: HashSet<PathBuf>,
}

/// True if any ancestor directory (including `dir` itself) is named
/// `site-packages`. Mirrors pip's legacy `candidate_python_project_roots`
/// skip predicate, which excluded `site-packages` on top of the shared
/// `should_skip_default_descent` set. Tier-1 venv reading handles those
/// paths on its own separate pass.
fn has_site_packages_ancestor(dir: &Path) -> bool {
    dir.ancestors().any(|d| {
        d.file_name()
            .and_then(|s| s.to_str())
            .map(|name| name == "site-packages")
            .unwrap_or(false)
    })
}

/// Per-file callback. Records the marker file's parent directory in
/// state. The HashSet inside state auto-dedupes multi-marker dirs.
fn on_pip_file(path: &Path, ctx: &SharedWalkerContext<'_>) {
    let Some(state) = ctx.state::<Mutex<PipDiscoveredPaths>>(ReaderId::PIP) else {
        return;
    };
    let Some(dir) = path.parent() else { return };
    if has_site_packages_ancestor(dir) {
        return;
    }
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.project_roots.insert(dir.to_path_buf());
}

/// Build the `ReaderRegistration` for this reader.
pub(crate) fn registration() -> anyhow::Result<ReaderRegistration> {
    let patterns = globset_from_patterns(&[
        "**/pyproject.toml",
        "**/poetry.lock",
        "**/Pipfile.lock",
        "**/uv.lock",
        "**/requirements*.txt",
    ])?;
    Ok(ReaderRegistration {
        reader_id: ReaderId::PIP,
        state: Some(Arc::new(Mutex::new(PipDiscoveredPaths::default()))),
        patterns,
        on_file: Some(on_pip_file),
        on_dir: None,
        descend_into: None,
    })
}

/// Extract the accumulated per-scan state via `std::mem::take`.
pub(crate) fn extract_paths(registration: &ReaderRegistration) -> PipDiscoveredPaths {
    let Some(state_arc) = registration.state.as_ref() else {
        return PipDiscoveredPaths::default();
    };
    let Some(mutex) = state_arc.downcast_ref::<Mutex<PipDiscoveredPaths>>() else {
        return PipDiscoveredPaths::default();
    };
    let mut guard = match mutex.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    std::mem::take(&mut *guard)
}

/// Coexistence-period entry point — mini-registry per reader.
/// **Post-T033**: `read_all` uses the consolidated shared-walker pilot;
/// this fn is retained as a shortcut for tests + single-reader debug.
#[allow(dead_code)]
pub(crate) fn build_and_run(
    rootfs: &Path,
    include_dev: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    let reg = match registration() {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!(error = %err, "pip: registration() failed");
            return Vec::new();
        }
    };
    let registry = match ReaderRegistryBuilder::new().register(reg).build() {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!(error = %err, "pip: build() failed");
            return Vec::new();
        }
    };
    let mut walker = SharedWalker::new(rootfs, &registry, exclude_set)
        .with_max_depth(MAX_PROJECT_ROOT_DEPTH);
    walker.run();
    let _ = walker.finish();
    let pip_reg = registry
        .registrations()
        .iter()
        .find(|r| r.reader_id == ReaderId::PIP)
        .expect("pip registration must be present");
    let paths = extract_paths(pip_reg);
    finalize(rootfs, paths, include_dev, exclude_set)
}

/// Legacy `pub fn read()` — retained during FR-004 coexistence.
/// Post-m664-US2-T036, `read_all` calls the consolidated shared-walker
/// pilot which invokes `finalize()` directly with precomputed
/// `project_roots`.
#[allow(dead_code)]
pub fn read(
    rootfs: &Path,
    include_dev: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    // Legacy path: compute project_roots via the LEGACY safe_walk-based
    // `candidate_python_project_roots`.
    let project_root_vec = candidate_python_project_roots(rootfs, exclude_set);
    let paths = PipDiscoveredPaths {
        project_roots: project_root_vec.into_iter().collect(),
    };
    finalize(rootfs, paths, include_dev, exclude_set)
}

/// Post-walker entry — takes discovered project_roots and runs the
/// full Tier-1 venv + Tier-2/3 lockfile pipeline. Semantics-preserving
/// with the pre-milestone `read()` via defensive sort of project_roots
/// before iteration (so lexicographic order of processing is stable
/// regardless of walker vs safe_walk entry-point).
pub(crate) fn finalize(
    rootfs: &Path,
    paths: PipDiscoveredPaths,
    include_dev: bool,
    _exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    // Sort project_roots deterministically for FR-006 byte-identity.
    // HashSet iteration order is nondeterministic; the pre-milestone
    // `candidate_python_project_roots` sorted implicitly via `Vec::sort`
    // inside `safe_walk`-driven discovery.
    let mut project_roots: Vec<PathBuf> = paths.project_roots.into_iter().collect();
    project_roots.sort();

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
    for project_root in &project_roots {
        // A project is anything holding a lockfile / requirements /
        // pyproject; track this for the "pyproject.toml only" skip log
        // below. Tier 1 venv does NOT count as a project root here —
        // that's installed state, not a project declaration.
        had_project_marker = true;

        if let Some(lockfile_entries) = poetry::read_poetry_lock(project_root, include_dev) {
            merge_without_override(&mut entries, lockfile_entries);
        }
        if let Some(lockfile_entries) = pipfile::read_pipfile_lock(project_root, include_dev) {
            merge_without_override(&mut entries, lockfile_entries);
        }
        // Milestone 106 US1 (issue #276): uv.lock support. Sibling to
        // poetry / pipfile readers — dispatched per-project-root with
        // the same merge_without_override dedup semantics. Returns
        // workspace-root + members + transitives when the root
        // pyproject.toml declares [tool.uv.workspace].
        if let Some(lockfile_entries) = uv_lock::read_uv_lock(project_root, include_dev) {
            merge_without_override(&mut entries, lockfile_entries);
        }
        if let Some(req_entries) = requirements_txt::read_requirements_files(project_root) {
            merge_without_override(&mut entries, req_entries);
        }
    }

    // Milestone 670 PR-1 T004 — m018 policy reversal. After Tier-1/2/3
    // readers have exhausted their sources, emit design-tier components
    // for every pyproject.toml-declared dep whose NAME is not already
    // covered. Name-based dedup preserves lockfile-authoritative
    // precedence: `pkg:pypi/requests@2.31.0` from `poetry.lock` blocks
    // `pkg:pypi/requests@unresolved` from `[project.dependencies]`. Cross-
    // project-root dedup grows the covered set as manifest entries land,
    // so a name declared in two sibling pyprojects surfaces exactly once
    // (with the first-project-root's evidence).
    //
    // See the module-level docstring (updated by T002) and
    // specs/670-pip-under-detection-fix/plan.md for the m018 → m670
    // policy transition.
    //
    // Feature 677 (issue #768) — pre-filter project_roots by PEP 508
    // validation of `[project].name` / `[tool.poetry].name`. Manifests
    // whose effective name fails validation are dropped WHOLESALE:
    // both this pyproject_declared_deps loop AND the subsequent
    // build_pip_main_module_entry loop skip the rejected root. One
    // WARN log per rejection is emitted by the filter itself.
    let (project_roots, names_rejected) = filter_project_roots_by_name(&project_roots);

    let mut covered_names: std::collections::HashSet<String> =
        entries.iter().map(|e| e.name.clone()).collect();
    for project_root in &project_roots {
        let manifest_entries = pyproject_declared_deps(project_root);
        for entry in manifest_entries {
            if covered_names.insert(entry.name.clone()) {
                entries.push(entry);
            }
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
    // Milestone 670 T005: `poetry_skips` retained as a always-zero
    // counter so the diagnostic log below preserves its wire shape.
    // Poetry-legacy manifests no longer skip main-module emission —
    // the counter's continued 0 value is the wire-visible signal that
    // the pre-m670 skip is deprecated.
    let poetry_skips = 0usize;
    // Milestone 183 US2 — accumulate the union of optional direct-dep
    // names across every project root's pyproject.toml. Applied via
    // `apply_optional_derivation_annotation` at the end of `read`. The
    // helper's `is_none()` guard enforces Decision 3 lockfile-precedence
    // — pyproject-based classification never overrides a lockfile
    // classification.
    let mut optional_names_from_manifests: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for project_root in &project_roots {
        optional_names_from_manifests.extend(optional_deps_from_pyproject(project_root));
        // Milestone 670 T005: Poetry-legacy pyproject.tomls now emit a
        // main-module component (previously suppressed per issue #104).
        // The second tuple element (`_was_poetry_legacy`) is retained
        // for future telemetry but no longer gates emission.
        let (synthesized, _was_poetry_legacy) =
            build_pip_main_module_entry(project_root);
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
    if main_modules_emitted > 0 || poetry_skips > 0 || names_rejected > 0 {
        tracing::info!(
            rootfs = %rootfs.display(),
            main_modules_emitted,
            poetry_only_skips = poetry_skips,
            same_purl_duplicates_dropped = dedup_drops.len(),
            names_rejected,
            "pip: emitted main-module components",
        );
    }

    // Milestone 183 US2 — final classifier pass. Marks every entry
    // whose name is in the manifest-collected optional-name set AND
    // whose `lifecycle_scope.is_none()` (Decision 3 lockfile-precedence
    // guard) with `LifecycleScope::Optional` + the C122 derivation
    // annotation. No-op when the manifest never declared any optional
    // deps (byte-identity SC-005 for non-optional projects).
    apply_optional_derivation_annotation(&mut entries, &optional_names_from_manifests);

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
/// - `extra_annotations` carries `waybill:component-role: "main-module"`
///   (C40, FR-004).
/// - `licenses: vec![]` (FR-005; license detection is #103 follow-up).
/// - `depends`: direct-dep package names extracted from
///   `[project.dependencies]` and each `[project.optional-dependencies].*`
///   array. PEP 508 requirement strings are split on whitespace and
///   the first token is taken as the package name (consistent with
///   how `requirements_txt.rs` handles the same shape — markers and
///   version specifiers are stripped).
///
/// Returns `(Option<PackageDbEntry>, bool)`. Second tuple element is
/// `is_poetry_legacy` — `true` when the entry was built from the
/// `[tool.poetry]` fallback path (no PEP 621 `[project]` table present).
///
/// Pre-m670 this flag was `was_poetry_only` and gated a caller-side
/// skip (issue #104). Milestone 670 T005 reverses that policy: Poetry-
/// legacy manifests DO emit a main-module (sourced from
/// `[tool.poetry].name/.version`) and declared deps come from
/// [`pyproject_declared_deps`] as separate design-tier components.
/// The flag is retained as an informational signal (unused by callers
/// today; kept for future telemetry).
///
// Extract the effective main-module name from a parsed pyproject.toml.
// Mirrors the extraction logic used by `build_pip_main_module_entry`
// (line ~650): prefer `[project].name`, fall back to `[tool.poetry].name`
// for Poetry-legacy manifests, otherwise return `None` (no name declared).
//
// Introduced by feature 677 (issue #768) — pulled out as a helper so
// `filter_project_roots_by_name` and `build_pip_main_module_entry` share
// one source of truth. Without shared extraction, the filter could
// rescue different manifests than the emitter rejects, breaking whole-
// manifest reject semantics.
fn extract_pyproject_effective_name(
    parsed: &toml::Value,
) -> Option<String> {
    let project_table = parsed.get("project");
    let poetry_table = parsed.get("tool").and_then(|t| t.get("poetry"));
    let source_table = match (project_table, poetry_table) {
        (Some(project), _) => project,
        (None, Some(poetry)) => poetry,
        (None, None) => return None,
    };
    source_table
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Feature 677 (issue #768): pre-filter `project_roots` by PEP 508
/// `[project].name` validation. Manifests whose effective name (via
/// `extract_pyproject_effective_name`) fails PEP 508 are dropped
/// wholesale — both the main-module component AND the declared-deps
/// list from that manifest are suppressed. Manifests with no readable
/// name / no readable pyproject.toml pass through unchanged (existing
/// downstream logic handles absent-name cases).
///
/// Returns `(retained_roots, rejected_count)`. Emits one WARN log per
/// rejected manifest with structured fields `manifest`, `name`, `reason`
/// so operators can locate the offending template directory.
///
/// Ordering guarantee: the returned Vec preserves the input order of
/// non-rejected roots (`retain`-style semantics), matching what the
/// downstream loops in `read()` expect for deterministic emission.
fn filter_project_roots_by_name(project_roots: &[PathBuf]) -> (Vec<PathBuf>, usize) {
    let mut retained = Vec::with_capacity(project_roots.len());
    let mut rejected = 0usize;
    for project_root in project_roots {
        let manifest_path = project_root.join("pyproject.toml");
        let Ok(text) = std::fs::read_to_string(&manifest_path) else {
            // No readable pyproject.toml — existing downstream logic
            // silently skips the emission; nothing to validate here.
            retained.push(project_root.clone());
            continue;
        };
        let Ok(parsed) = toml::from_str::<toml::Value>(&text) else {
            // Unparseable — existing behavior is silent skip. Preserve.
            retained.push(project_root.clone());
            continue;
        };
        let Some(name) = extract_pyproject_effective_name(&parsed) else {
            // No `[project].name` or `[tool.poetry].name` — no emission
            // possible anyway. Preserve the root (existing downstream
            // logic handles the absent-name path).
            retained.push(project_root.clone());
            continue;
        };
        match validate_pep508_name(&name) {
            Ok(()) => retained.push(project_root.clone()),
            Err(err) => {
                tracing::warn!(
                    manifest = %manifest_path.display(),
                    name = %name,
                    reason = %match &err {
                        NameValidationError::Empty => "empty or whitespace-only".to_string(),
                        NameValidationError::Malformed { reason } => reason.clone(),
                    },
                    "pip: pyproject.toml [project].name failed PEP 508 validation; skipping whole manifest"
                );
                rejected += 1;
            }
        }
    }
    (retained, rejected)
}

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
    let poetry_table = parsed.get("tool").and_then(|t| t.get("poetry"));
    let has_poetry_table = poetry_table.is_some();
    // Milestone 670 T005 — reverses the pre-m670 "Poetry-legacy = skip
    // main-module" policy (issue #104). Poetry-legacy pyproject.tomls
    // NOW emit a main-module component, sourced from `[tool.poetry]`
    // when PEP 621 `[project]` is absent. Declared deps from the
    // Poetry-legacy sections are handled by `pyproject_declared_deps`
    // (m670 PR-1 T003) as separate design-tier components; the
    // `depends` list on this main-module stays empty for the
    // Poetry-legacy branch (v1 scope — depends fabrication for graph
    // edges is a follow-up).
    let (source_table, is_poetry_legacy) = match (project_table, poetry_table) {
        (Some(project), _) => (project, false),
        (None, Some(poetry)) => (poetry, true),
        (None, None) => return (None, false),
    };
    let project = source_table;
    let Some(name) = project.get("name").and_then(|v| v.as_str()) else {
        return (None, has_poetry_table);
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
            // Poetry-legacy pyproject.tomls that omit `[tool.poetry].version`
            // are common (pyproject templates leave it blank; setuptools-scm
            // populates it at build time). Don't spam a warn log for the
            // Poetry-legacy branch — the placeholder is expected there.
            if !is_poetry_legacy {
                tracing::warn!(
                    manifest = %manifest_path.display(),
                    name = %name,
                    "pip: pyproject.toml [project] has neither `version` nor `dynamic = [\"version\"]` — using 0.0.0-unknown placeholder",
                );
            }
            "0.0.0-unknown".to_string()
        }
    };
    let purl_str = build_pypi_purl_str(name, &version);
    let Ok(purl) = waybill_common::types::purl::Purl::new(&purl_str) else {
        return (None, has_poetry_table);
    };
    // Direct deps from [project.dependencies] and
    // [project.optional-dependencies].* per FR-007. PEP 508 strings:
    // take the first whitespace-or-`[<>=;`-delimited token as the name.
    //
    // Milestone 670 T005: PEP 621 branch only. Poetry-legacy declared
    // deps come from [tool.poetry.dependencies] (dict-shape, not
    // array-shape) and are emitted as first-class design-tier
    // components by `pyproject_declared_deps` (T003). Populating this
    // `depends` list from Poetry-legacy is deferred — v1 emits an
    // empty depends for that branch (graph-edge fabrication is a
    // follow-up).
    let mut depends: Vec<String> = Vec::new();
    if !is_poetry_legacy {
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
    }
    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "waybill:component-role".to_string(),
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
    };
    // Milestone 670 T005: second tuple element = `is_poetry_legacy`
    // (was `was_poetry_only` pre-m670). Informational only; the caller
    // no longer treats `true` as a skip signal.
    (Some(entry), is_poetry_legacy)
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

/// Milestone 670 PR-1 — locked m236 reason string for pyproject.toml-declared
/// deps. Reserved in `specs/236-unresolved-reason/contracts/per-reader-strings.md`
/// (milestone 670 section). Wired into
/// `waybill-cli/tests/unresolved_reason_universal.rs::locked_reason_strings()`
/// alongside the source-side emission.
pub(crate) const MANIFEST_UNRESOLVED_REASON: &str =
    "declared in pyproject.toml; no uv.lock / poetry.lock / Pipfile.lock fallback";

/// Milestone 670 PR-1 — emit design-tier `PackageDbEntry` components for
/// every dependency declared in a `project_root`'s `pyproject.toml`.
/// Reverses the milestone-018 "pyproject-only = 0 components" policy that
/// was documented at `mod.rs:1-28` prior to milestone 670 T002.
///
/// Reads (precedence: PEP 621 wins over Poetry-legacy when `[project]`
/// is present):
///
/// 1. **PEP 621** `[project.dependencies]` (Runtime).
/// 2. **PEP 621** `[project.optional-dependencies].<group>` (Optional).
/// 3. **PEP 735** `[dependency-groups].<group>` (Optional).
/// 4. **Poetry-legacy** `[tool.poetry.dependencies]` — read ONLY when
///    `[project]` is absent (Runtime).
/// 5. **Poetry-legacy** `[tool.poetry.dev-dependencies]` (Development).
/// 6. **Poetry-legacy** `[tool.poetry.group.<name>.dependencies]`
///    (Development when `<name>` ∈ {`dev`, `test`}; else Optional).
///
/// Each emitted entry carries:
/// - PURL `pkg:pypi/<pep503-normalized-name>@unresolved`
/// - `version = "unresolved"`
/// - `sbom_tier = Some("design")`
/// - `source_path = "path+file://<pyproject.toml>"`
/// - `requirement_ranges = vec![<raw-constraint>]` when a constraint was declared
/// - `extra_annotations`:
///   - `waybill:unresolved-reason` = [`MANIFEST_UNRESOLVED_REASON`]
///   - `waybill:version-constraint` = raw constraint string (when present)
///   - `waybill:optional-derivation` = per-section derivation label
///     (for Optional / Development scopes only)
///
/// Skips:
/// - `python` itself under `[tool.poetry.dependencies]` (Poetry declares
///   the Python interpreter here; not a package)
/// - Environment-marker-filtered entries (e.g. `; extra == 'dev'`) via
///   [`tokenise_requires_dist_name`]
/// - Empty / malformed entries
///
/// Diamond precedence (mirrors m183 Decision 3): if the same name appears
/// in `[project.dependencies]` AND `[project.optional-dependencies]`,
/// Runtime wins — one entry with `LifecycleScope::Runtime` and no
/// `waybill:optional-derivation` annotation.
///
/// Returns `Vec<PackageDbEntry>`. Empty when the manifest is missing,
/// unparseable, or declares no deps. Callers use a name-based dedup at
/// wire-in (T004) so lockfile-resolved entries retain priority over
/// manifest-unresolved ones.
pub(crate) fn pyproject_declared_deps(project_root: &Path) -> Vec<PackageDbEntry> {
    let manifest_path = project_root.join("pyproject.toml");
    let Ok(text) = std::fs::read_to_string(&manifest_path) else {
        return Vec::new();
    };
    let Ok(parsed) = toml::from_str::<toml::Value>(&text) else {
        return Vec::new();
    };

    // Extract the raw PEP 508 constraint substring — everything after the
    // name (and any `[extras]` block) up to a `;` marker separator. Empty
    // string means unpinned (no constraint declared).
    let take_constraint = |raw: &str| -> String {
        let raw = raw.trim();
        let head = match raw.split_once(';') {
            Some((h, _marker)) => h.trim(),
            None => raw,
        };
        // Skip past name — first non-name char (mirrors take_first_token
        // in build_pip_main_module_entry).
        let name_end = head
            .find([' ', '\t', '[', '(', '<', '>', '=', '~', '!'])
            .unwrap_or(head.len());
        let after_name = &head[name_end..];
        // Skip past `[extras]` block if present.
        let after_extras = if let Some(rest) = after_name.trim_start().strip_prefix('[') {
            if let Some(idx) = rest.find(']') {
                &rest[idx + 1..]
            } else {
                after_name
            }
        } else {
            after_name
        };
        after_extras.trim().to_string()
    };

    // Match the m068 main-module convention: `source_path` points at the
    // PROJECT ROOT directory (URI form), NOT the manifest file. Two reasons:
    // (a) m176's `derive_workspace_root` at scan_fs/workspace_root.rs:38
    //     expects the URI-form path to be a directory; passing the manifest
    //     path would yield `subproject_a/pyproject.toml` as a workspace
    //     member instead of the intended `subproject_a`, breaking m176's
    //     `waybill:workspace-member` deduplication (regression caught by
    //     workspace_visibility::t007). (b) The m068 main-module entry from
    //     `build_pip_main_module_entry` uses this exact shape, so the m191
    //     reconciler merges by same source_path when it collides.
    let source_path = format!("path+file://{}", project_root.display());

    // Constructor for one entry — reused across every section below.
    let build_entry =
        |raw_dep: &str,
         scope: waybill_common::resolution::LifecycleScope,
         optional_derivation: Option<&str>|
         -> Option<PackageDbEntry> {
            let name = tokenise_requires_dist_name(raw_dep)?;
            if name.is_empty() {
                return None;
            }
            let purl_str = build_pypi_purl_str(&name, "unresolved");
            let purl = waybill_common::types::purl::Purl::new(&purl_str).ok()?;
            let constraint = take_constraint(raw_dep);
            let requirement_ranges = if constraint.is_empty() {
                Vec::new()
            } else {
                vec![constraint.clone()]
            };
            let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
                Default::default();
            extra_annotations.insert(
                "waybill:unresolved-reason".to_string(),
                serde_json::Value::String(MANIFEST_UNRESOLVED_REASON.to_string()),
            );
            if !constraint.is_empty() {
                extra_annotations.insert(
                    "waybill:version-constraint".to_string(),
                    serde_json::Value::String(constraint),
                );
            }
            if let Some(derivation) = optional_derivation {
                extra_annotations.insert(
                    "waybill:optional-derivation".to_string(),
                    serde_json::Value::String(derivation.to_string()),
                );
            }
            Some(PackageDbEntry {
                build_inclusion: None,
                purl,
                name,
                version: "unresolved".to_string(),
                arch: None,
                source_path: source_path.clone(),
                depends: Vec::new(),
                maintainer: None,
                licenses: Vec::new(),
                lifecycle_scope: Some(scope),
                requirement_ranges,
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
                sbom_tier: Some("design".to_string()),
                shade_relocation: None,
                extra_annotations,
                binary_role: None,
            })
        };

    // De-duplicate by name across the whole manifest — Runtime wins over
    // Optional / Development (m183 Decision 3 diamond precedence).
    // Insertion order tracked so PEP 621 -> PEP 735 -> Poetry-legacy
    // ordering is stable in output.
    let mut by_name: std::collections::BTreeMap<String, PackageDbEntry> =
        std::collections::BTreeMap::new();
    let mut push_or_upgrade = |entry: PackageDbEntry| {
        use waybill_common::resolution::LifecycleScope;
        let existing = by_name.get(&entry.name);
        let new_is_runtime = matches!(entry.lifecycle_scope, Some(LifecycleScope::Runtime));
        let existing_is_runtime = matches!(
            existing.and_then(|e| e.lifecycle_scope),
            Some(LifecycleScope::Runtime)
        );
        match existing {
            None => {
                by_name.insert(entry.name.clone(), entry);
            }
            Some(_) if new_is_runtime && !existing_is_runtime => {
                by_name.insert(entry.name.clone(), entry);
            }
            Some(_) => { /* preserve existing */ }
        }
    };

    let project_table = parsed.get("project");
    let has_project = project_table.is_some();
    let tool_poetry = parsed.get("tool").and_then(|t| t.get("poetry"));

    // Section 1: PEP 621 [project.dependencies]
    if let Some(project) = project_table {
        if let Some(deps) = project.get("dependencies").and_then(|v| v.as_array()) {
            for d in deps.iter().filter_map(|v| v.as_str()) {
                if let Some(entry) = build_entry(
                    d,
                    waybill_common::resolution::LifecycleScope::Runtime,
                    None,
                ) {
                    push_or_upgrade(entry);
                }
            }
        }

        // Section 2: PEP 621 [project.optional-dependencies].<group>
        if let Some(opt_table) = project
            .get("optional-dependencies")
            .and_then(|v| v.as_table())
        {
            for (group_name, deps) in opt_table {
                if let Some(arr) = deps.as_array() {
                    let derivation =
                        format!("pip-pyproject-optional-dependencies:{group_name}");
                    for d in arr.iter().filter_map(|v| v.as_str()) {
                        if let Some(entry) = build_entry(
                            d,
                            waybill_common::resolution::LifecycleScope::Optional,
                            Some(&derivation),
                        ) {
                            push_or_upgrade(entry);
                        }
                    }
                }
            }
        }
    }

    // Section 3: PEP 735 [dependency-groups].<group> (parallel to PEP 621).
    if let Some(dep_groups) = parsed.get("dependency-groups").and_then(|v| v.as_table()) {
        for (group_name, deps) in dep_groups {
            if let Some(arr) = deps.as_array() {
                let derivation = format!("pep-735-dependency-groups:{group_name}");
                for d in arr.iter().filter_map(|v| v.as_str()) {
                    if let Some(entry) = build_entry(
                        d,
                        waybill_common::resolution::LifecycleScope::Optional,
                        Some(&derivation),
                    ) {
                        push_or_upgrade(entry);
                    }
                }
            }
        }
    }

    // Section 4-6: Poetry-legacy — only when [project] is absent so PEP
    // 621 stays authoritative for mid-migration projects (matching how
    // uv reads them).
    if !has_project {
        if let Some(poetry) = tool_poetry {
            // Section 4: [tool.poetry.dependencies] → Runtime (skip `python`)
            if let Some(deps) = poetry.get("dependencies").and_then(|v| v.as_table()) {
                for (name, spec) in deps {
                    if name == "python" {
                        continue;
                    }
                    // Poetry deps are TOML tables; the constraint can be
                    // either a string ("^1.0") or a subtable ({version =
                    // "^1.0", extras = [...], ...}). Reconstruct a PEP 508-
                    // shaped string so the shared build_entry closure works.
                    let raw = poetry_dep_to_pep508(name, spec);
                    if let Some(entry) = build_entry(
                        &raw,
                        waybill_common::resolution::LifecycleScope::Runtime,
                        None,
                    ) {
                        push_or_upgrade(entry);
                    }
                }
            }
            // Section 5: [tool.poetry.dev-dependencies] → Development
            if let Some(deps) = poetry.get("dev-dependencies").and_then(|v| v.as_table()) {
                for (name, spec) in deps {
                    if name == "python" {
                        continue;
                    }
                    let raw = poetry_dep_to_pep508(name, spec);
                    if let Some(entry) = build_entry(
                        &raw,
                        waybill_common::resolution::LifecycleScope::Development,
                        Some("poetry-legacy-dev-dependencies"),
                    ) {
                        push_or_upgrade(entry);
                    }
                }
            }
            // Section 6: [tool.poetry.group.<name>.dependencies]
            if let Some(groups) = poetry.get("group").and_then(|v| v.as_table()) {
                for (group_name, group_val) in groups {
                    if let Some(deps) = group_val
                        .get("dependencies")
                        .and_then(|v| v.as_table())
                    {
                        // dev / test → Development; everything else → Optional
                        let (scope, derivation) = match group_name.as_str() {
                            "dev" | "test" => (
                                waybill_common::resolution::LifecycleScope::Development,
                                format!("poetry-legacy-group:{group_name}"),
                            ),
                            _ => (
                                waybill_common::resolution::LifecycleScope::Optional,
                                format!("poetry-legacy-group:{group_name}"),
                            ),
                        };
                        for (name, spec) in deps {
                            if name == "python" {
                                continue;
                            }
                            let raw = poetry_dep_to_pep508(name, spec);
                            if let Some(entry) = build_entry(&raw, scope, Some(&derivation)) {
                                push_or_upgrade(entry);
                            }
                        }
                    }
                }
            }
        }
    }

    by_name.into_values().collect()
}

/// Reconstruct a PEP 508-shaped string from a Poetry-legacy dep entry.
///
/// Poetry deps can be either:
/// - String scalar: `requests = "^2.28"` — `spec.as_str() = Some("^2.28")`
/// - Table: `requests = { version = "^2.28", extras = ["security"] }` —
///   `spec.as_table()` with a `version` key
/// - Path / git / url: table without a `version` key — surfaced with an
///   empty constraint so build_entry treats it as unpinned.
///
/// The returned string is fed back through [`tokenise_requires_dist_name`]
/// and `take_constraint` in [`pyproject_declared_deps`], which recover the
/// name and constraint respectively. Extras and markers are intentionally
/// dropped for v1 — Poetry-legacy fidelity is scope-limited per spec Q3.
fn poetry_dep_to_pep508(name: &str, spec: &toml::Value) -> String {
    match spec {
        toml::Value::String(constraint) => format!("{name} {constraint}"),
        toml::Value::Table(t) => {
            if let Some(constraint) = t.get("version").and_then(|v| v.as_str()) {
                format!("{name} {constraint}")
            } else {
                name.to_string()
            }
        }
        _ => name.to_string(),
    }
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
            .get("waybill:component-role")
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

/// Milestone 183 US2 — collect the set of direct-dep names declared
/// under any `[project.optional-dependencies].<extra>` array of the
/// project's `pyproject.toml`, MINUS any name that also appears in
/// `[project.dependencies]` (diamond-shape: Runtime wins per FR-005).
///
/// Returns an empty HashSet when:
///   * `pyproject.toml` is absent
///   * pyproject.toml is unparseable
///   * `[project.optional-dependencies]` table is absent
///
/// The name-extraction rules match `build_pip_main_module_entry`'s
/// `take_first_token` closure (PEP 508 first-token split) so the
/// returned names align with what the graph resolver will see when
/// building edges from the main-module's `depends` list.
fn optional_deps_from_pyproject(
    project_root: &std::path::Path,
) -> std::collections::HashSet<String> {
    let manifest_path = project_root.join("pyproject.toml");
    let Ok(text) = std::fs::read_to_string(&manifest_path) else {
        return std::collections::HashSet::new();
    };
    let Ok(parsed) = toml::from_str::<toml::Value>(&text) else {
        return std::collections::HashSet::new();
    };
    let Some(project) = parsed.get("project") else {
        return std::collections::HashSet::new();
    };

    // PEP 508 first-token extractor — mirror the closure in
    // `build_pip_main_module_entry` for consistency.
    let take_first_token = |s: &str| -> String {
        s.chars()
            .take_while(|c| {
                !matches!(c, ' ' | '\t' | '[' | ']' | '<' | '>' | '=' | ';' | '~' | '!')
            })
            .collect::<String>()
            .trim()
            .to_string()
    };

    let mut regular: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(deps) = project.get("dependencies").and_then(|v| v.as_array()) {
        for d in deps.iter().filter_map(|v| v.as_str()) {
            let token = take_first_token(d);
            if !token.is_empty() {
                regular.insert(token);
            }
        }
    }

    let mut optional: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(opt_table) = project
        .get("optional-dependencies")
        .and_then(|v| v.as_table())
    {
        for (_extra_name, deps) in opt_table {
            if let Some(arr) = deps.as_array() {
                for d in arr.iter().filter_map(|v| v.as_str()) {
                    let token = take_first_token(d);
                    if !token.is_empty() {
                        optional.insert(token);
                    }
                }
            }
        }
    }

    // FR-005 diamond-shape: Runtime wins. Remove any name that also
    // appears in `[project.dependencies]`.
    optional.retain(|name| !regular.contains(name));
    optional
}

/// Milestone 183 (US2 + US3) — apply `LifecycleScope::Optional` +
/// `waybill:optional-derivation = "pip-optional-dependencies"` to each
/// entry whose name is in `optional_names` AND whose `lifecycle_scope`
/// is currently `None`.
///
/// The `is_none()` guard enforces Decision 3 lockfile-precedence: any
/// entry already classified by a lockfile reader (Runtime, Development,
/// or Optional) is left untouched. This prevents the manifest-based
/// (pyproject.toml) or downstream-reader (uv.lock) classification from
/// overriding a lockfile's ground-truth classification.
///
/// Called at the end of `read` after all lockfile / manifest readers
/// have run. Reused by US2 (pyproject.toml classifier) and US3 (uv.lock
/// classifier).
fn apply_optional_derivation_annotation(
    entries: &mut [PackageDbEntry],
    optional_names: &std::collections::HashSet<String>,
) {
    if optional_names.is_empty() {
        return;
    }
    for entry in entries.iter_mut() {
        if !optional_names.contains(&entry.name) {
            continue;
        }
        if entry.lifecycle_scope.is_some() {
            // Lockfile-precedence per Decision 3: skip already-classified
            // entries (Runtime, Development, Optional). The lockfile's
            // ground-truth wins over the manifest-based classification.
            continue;
        }
        entry.lifecycle_scope =
            Some(waybill_common::resolution::LifecycleScope::Optional);
        entry.extra_annotations.insert(
            "waybill:optional-derivation".to_string(),
            serde_json::Value::String("pip-optional-dependencies".to_string()),
        );
    }
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
        // to lowercase with `_` → `-`. Waybill follows suit so PURLs
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
                .get("waybill:component-role")
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
    fn build_pip_main_module_poetry_only_emits_main_module_post_m670() {
        // Milestone 670 T005: reverses the pre-m670 policy that suppressed
        // main-module emission for [tool.poetry]-only pyproject.tomls
        // (issue #104). The main-module now emits from [tool.poetry].name/
        // .version; declared deps come from `pyproject_declared_deps`
        // as separate design-tier components (T003). The tuple's second
        // element flips to `true` (informational — no longer gates the
        // caller's skip).
        let tmp = tempfile::tempdir().unwrap();
        write_pyproject(
            tmp.path(),
            r#"
[tool.poetry]
name = "poetry-only-app"
version = "1.0.0"
"#,
        );
        let (entry, is_poetry_legacy) = build_pip_main_module_entry(tmp.path());
        let entry = entry.expect("m670: Poetry-only pyproject.toml emits main-module");
        assert_eq!(entry.name, "poetry-only-app");
        assert_eq!(entry.version, "1.0.0");
        assert_eq!(entry.purl.as_str(), "pkg:pypi/poetry-only-app@1.0.0");
        assert_eq!(
            entry
                .extra_annotations
                .get("waybill:component-role")
                .and_then(|v| v.as_str()),
            Some("main-module")
        );
        assert!(
            entry.depends.is_empty(),
            "m670 v1: Poetry-legacy main-module emits empty depends; deps handled by pyproject_declared_deps"
        );
        assert!(is_poetry_legacy);
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
        let purl = waybill_common::types::purl::Purl::new(&purl_str).unwrap();
        let mut extra: std::collections::BTreeMap<String, serde_json::Value> =
            Default::default();
        extra.insert(
            "waybill:component-role".to_string(),
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

    // ── Milestone 183 T004 — apply_optional_derivation_annotation ────

    #[test]
    fn apply_annotation_marks_matching_and_unclassified_entries() {
        let a = make_main_module_entry("foo", "1.0", "/tmp/foo/pyproject.toml");
        let b = make_main_module_entry("bar", "2.0", "/tmp/bar/pyproject.toml");
        // Both have `lifecycle_scope: None` and no derivation annotation.
        assert!(a.lifecycle_scope.is_none());
        assert!(b.lifecycle_scope.is_none());

        let mut entries = vec![a.clone(), b.clone()];
        let mut optional_names = std::collections::HashSet::new();
        optional_names.insert("foo".to_string());
        apply_optional_derivation_annotation(&mut entries, &optional_names);

        // `foo` is classified + annotated; `bar` is untouched.
        assert_eq!(
            entries[0].lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Optional)
        );
        assert_eq!(
            entries[0].extra_annotations.get("waybill:optional-derivation"),
            Some(&serde_json::Value::String("pip-optional-dependencies".to_string()))
        );
        assert!(entries[1].lifecycle_scope.is_none());
        assert!(!entries[1]
            .extra_annotations
            .contains_key("waybill:optional-derivation"));

        // Clone the shadowed originals to silence unused warnings + prove
        // the helper mutated `entries`, not `a`/`b`.
        let _ = (a, b);
    }

    // ── Milestone 183 T008 — optional_deps_from_pyproject helper ────

    #[test]
    fn optional_deps_from_pyproject_extracts_names() {
        // Single-extra + multi-extra: names from `dev` AND `test` extras
        // both surface. PEP 508 markers/versions stripped.
        let tempdir = tempfile::tempdir().unwrap();
        let manifest = r#"
[project]
name = "sample-app"
version = "1.0"
dependencies = ["requests>=2.0", "urllib3; python_version < '3.10'"]

[project.optional-dependencies]
dev = ["pytest>=7.0", "black"]
test = ["pytest-cov[toml]"]
docs = ["sphinx"]
"#;
        std::fs::write(tempdir.path().join("pyproject.toml"), manifest).unwrap();
        let out = optional_deps_from_pyproject(tempdir.path());
        // Everything from `[project.optional-dependencies]` shows up.
        assert!(out.contains("pytest"), "expected pytest, got: {out:?}");
        assert!(out.contains("black"), "expected black, got: {out:?}");
        assert!(out.contains("pytest-cov"), "expected pytest-cov, got: {out:?}");
        assert!(out.contains("sphinx"), "expected sphinx, got: {out:?}");
        // Nothing from `[project.dependencies]` sneaks in.
        assert!(!out.contains("requests"), "requests leaked as optional: {out:?}");
        assert!(!out.contains("urllib3"), "urllib3 leaked as optional: {out:?}");
    }

    #[test]
    fn optional_deps_diamond_shape_runtime_wins() {
        // FR-005: a name in BOTH `[project.dependencies]` AND
        // `[project.optional-dependencies].<extra>` must NOT be
        // classified as optional. Runtime wins.
        let tempdir = tempfile::tempdir().unwrap();
        let manifest = r#"
[project]
name = "sample-app"
version = "1.0"
dependencies = ["pytest"]

[project.optional-dependencies]
test = ["pytest", "pytest-cov"]
"#;
        std::fs::write(tempdir.path().join("pyproject.toml"), manifest).unwrap();
        let out = optional_deps_from_pyproject(tempdir.path());
        // `pytest` is in `[project.dependencies]` → removed.
        assert!(!out.contains("pytest"), "diamond-shape violated: {out:?}");
        // `pytest-cov` is optional-only → kept.
        assert!(out.contains("pytest-cov"), "expected pytest-cov, got: {out:?}");
    }

    #[test]
    fn main_module_dep_split_records_optional_names_and_applies_post_pass() {
        // US2 end-to-end: after `read` runs on a pyproject-only project
        // (no lockfile), extras-gated deps must show up as
        // `LifecycleScope::Optional` + carry the C122 derivation
        // annotation. Regular deps stay untouched.
        //
        // The test fixture needs BOTH the pyproject.toml AND matching
        // dist-info / lockfile / requirements entries to instantiate a
        // component that the post-pass can classify. Since we only have
        // pyproject.toml here, we instead check that the `read` function
        // returns a `Vec<PackageDbEntry>` that includes the main-module
        // component + verifies the optional-name-set collection reaches
        // the post-pass. We do this by seeding a synthetic entry for
        // `pytest` in a co-located requirements.txt (which reads as a
        // Tier-3 source with `lifecycle_scope: None`) and confirming
        // the post-pass upgrades it.
        let tempdir = tempfile::tempdir().unwrap();
        std::fs::write(
            tempdir.path().join("pyproject.toml"),
            r#"
[project]
name = "sample-app"
version = "1.0.0"
dependencies = ["requests>=2.0"]

[project.optional-dependencies]
dev = ["pytest>=7.0"]
"#,
        )
        .unwrap();
        std::fs::write(
            tempdir.path().join("requirements.txt"),
            "requests==2.31.0\npytest==7.4.0\n",
        )
        .unwrap();

        let exclude_set = crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty();
        let entries = read(tempdir.path(), /*include_dev=*/ true, &exclude_set);

        let pytest = entries
            .iter()
            .find(|e| e.name == "pytest")
            .expect("pytest entry from requirements.txt");
        assert_eq!(
            pytest.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Optional),
            "pytest should be Optional per [project.optional-dependencies].dev"
        );
        assert_eq!(
            pytest.extra_annotations.get("waybill:optional-derivation"),
            Some(&serde_json::Value::String("pip-optional-dependencies".to_string())),
        );

        // Regression pin per FR-005: requests is in
        // `[project.dependencies]` — must NOT get the Optional
        // classification via the post-pass.
        let requests = entries
            .iter()
            .find(|e| e.name == "requests")
            .expect("requests entry from requirements.txt");
        assert_ne!(
            requests.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Optional),
            "requests must not be classified as Optional"
        );
        assert!(!requests
            .extra_annotations
            .contains_key("waybill:optional-derivation"));
    }

    #[test]
    fn optional_deps_absent_returns_empty() {
        // Regression pin: no `[project.optional-dependencies]` table →
        // empty HashSet, no panics.
        let tempdir = tempfile::tempdir().unwrap();
        let manifest = r#"
[project]
name = "sample-app"
version = "1.0"
dependencies = ["requests"]
"#;
        std::fs::write(tempdir.path().join("pyproject.toml"), manifest).unwrap();
        let out = optional_deps_from_pyproject(tempdir.path());
        assert!(out.is_empty(), "expected empty, got: {out:?}");

        // Also: missing pyproject.toml altogether → empty HashSet.
        let tempdir2 = tempfile::tempdir().unwrap();
        assert!(optional_deps_from_pyproject(tempdir2.path()).is_empty());
    }

    #[test]
    fn apply_annotation_skips_already_classified_entries() {
        // Decision 3 lockfile-precedence: an entry with `Some(_)`
        // lifecycle_scope MUST NOT be re-classified by the post-pass.
        let mut runtime_entry =
            make_main_module_entry("locked-runtime", "1.0", "/tmp/proj/poetry.lock");
        runtime_entry.lifecycle_scope =
            Some(waybill_common::resolution::LifecycleScope::Runtime);
        let mut dev_entry =
            make_main_module_entry("locked-dev", "1.0", "/tmp/proj/poetry.lock");
        dev_entry.lifecycle_scope =
            Some(waybill_common::resolution::LifecycleScope::Development);

        let mut entries = vec![runtime_entry, dev_entry];
        let mut optional_names = std::collections::HashSet::new();
        optional_names.insert("locked-runtime".to_string());
        optional_names.insert("locked-dev".to_string());
        apply_optional_derivation_annotation(&mut entries, &optional_names);

        // Both remain in their pre-existing classifications; no annotation
        // is added by the post-pass.
        assert_eq!(
            entries[0].lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Runtime)
        );
        assert_eq!(
            entries[1].lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Development)
        );
        for e in &entries {
            assert!(!e
                .extra_annotations
                .contains_key("waybill:optional-derivation"));
        }
    }

    // ── Milestone 191 (#558) — build_pypi_purl_str versionless shape ──
    // NOTE: pip already had the empty-version branch pre-m191; these
    // tests lock the behavior in per FR-011 / SC-006.

    #[test]
    fn build_pypi_purl_str_empty_version_emits_versionless_shape() {
        let s = build_pypi_purl_str("requests", "");
        assert_eq!(s, "pkg:pypi/requests");
    }

    #[test]
    fn build_pypi_purl_str_nonempty_version_byte_identical_to_pre_m191() {
        let s = build_pypi_purl_str("requests", "2.31.0");
        assert_eq!(s, "pkg:pypi/requests@2.31.0");
    }

    // -----------------------------------------------------------------
    // Milestone 670 PR-1 (T003): pyproject_declared_deps tests
    // -----------------------------------------------------------------

    fn m670_write_pyproject(tmp: &tempfile::TempDir, contents: &str) {
        std::fs::write(tmp.path().join("pyproject.toml"), contents).unwrap();
    }

    fn m670_find_entry<'a>(entries: &'a [PackageDbEntry], name: &str) -> &'a PackageDbEntry {
        entries
            .iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("no entry named {name}; got: {:?}",
                entries.iter().map(|e| &e.name).collect::<Vec<_>>()))
    }

    #[test]
    fn m670_pyproject_missing_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        // No pyproject.toml at all.
        assert!(pyproject_declared_deps(tmp.path()).is_empty());
    }

    #[test]
    fn m670_pyproject_unparseable_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        m670_write_pyproject(&tmp, "this is [not valid = toml");
        assert!(pyproject_declared_deps(tmp.path()).is_empty());
    }

    #[test]
    fn m670_pep621_dependencies_emit_runtime_design_tier() {
        let tmp = tempfile::tempdir().unwrap();
        m670_write_pyproject(
            &tmp,
            r#"
[project]
name = "example"
version = "1.0.0"
dependencies = [
    "requests>=2.28",
    "click>=8.0",
    "urllib3",
]
"#,
        );
        let entries = pyproject_declared_deps(tmp.path());
        assert_eq!(entries.len(), 3);
        for e in &entries {
            assert_eq!(e.version, "unresolved");
            assert_eq!(e.sbom_tier.as_deref(), Some("design"));
            assert_eq!(
                e.lifecycle_scope,
                Some(waybill_common::resolution::LifecycleScope::Runtime)
            );
            assert_eq!(
                e.extra_annotations
                    .get("waybill:unresolved-reason")
                    .and_then(|v| v.as_str()),
                Some(MANIFEST_UNRESOLVED_REASON)
            );
            // No optional-derivation on Runtime entries.
            assert!(!e.extra_annotations.contains_key("waybill:optional-derivation"));
        }
        let requests = m670_find_entry(&entries, "requests");
        assert_eq!(requests.purl.as_str(), "pkg:pypi/requests@unresolved");
        assert_eq!(
            requests.extra_annotations
                .get("waybill:version-constraint")
                .and_then(|v| v.as_str()),
            Some(">=2.28")
        );
        // Unpinned entry has no version-constraint annotation.
        let urllib3 = m670_find_entry(&entries, "urllib3");
        assert!(!urllib3.extra_annotations.contains_key("waybill:version-constraint"));
    }

    #[test]
    fn m670_pep621_optional_dependencies_emit_optional_scope_with_derivation() {
        let tmp = tempfile::tempdir().unwrap();
        m670_write_pyproject(
            &tmp,
            r#"
[project]
name = "example"
version = "1.0.0"

[project.optional-dependencies]
docs = ["sphinx>=7"]
test = ["pytest>=8"]
"#,
        );
        let entries = pyproject_declared_deps(tmp.path());
        assert_eq!(entries.len(), 2);
        let sphinx = m670_find_entry(&entries, "sphinx");
        assert_eq!(
            sphinx.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Optional)
        );
        assert_eq!(
            sphinx.extra_annotations
                .get("waybill:optional-derivation")
                .and_then(|v| v.as_str()),
            Some("pip-pyproject-optional-dependencies:docs")
        );
        let pytest = m670_find_entry(&entries, "pytest");
        assert_eq!(
            pytest.extra_annotations
                .get("waybill:optional-derivation")
                .and_then(|v| v.as_str()),
            Some("pip-pyproject-optional-dependencies:test")
        );
    }

    #[test]
    fn m670_pep735_dependency_groups_emit_optional_scope() {
        let tmp = tempfile::tempdir().unwrap();
        m670_write_pyproject(
            &tmp,
            r#"
[project]
name = "example"
version = "1.0.0"

[dependency-groups]
lint = ["ruff>=0.5"]
"#,
        );
        let entries = pyproject_declared_deps(tmp.path());
        assert_eq!(entries.len(), 1);
        let ruff = m670_find_entry(&entries, "ruff");
        assert_eq!(
            ruff.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Optional)
        );
        assert_eq!(
            ruff.extra_annotations
                .get("waybill:optional-derivation")
                .and_then(|v| v.as_str()),
            Some("pep-735-dependency-groups:lint")
        );
    }

    #[test]
    fn m670_poetry_legacy_only_emits_runtime_and_skips_python() {
        let tmp = tempfile::tempdir().unwrap();
        m670_write_pyproject(
            &tmp,
            r#"
[tool.poetry]
name = "example"
version = "1.0.0"

[tool.poetry.dependencies]
python = "^3.11"
requests = "^2.28"
click = { version = "^8.0", extras = ["colorama"] }
"#,
        );
        let entries = pyproject_declared_deps(tmp.path());
        assert_eq!(entries.len(), 2, "python should be skipped");
        assert!(entries.iter().all(|e| e.name != "python"));
        let requests = m670_find_entry(&entries, "requests");
        assert_eq!(
            requests.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Runtime)
        );
        assert_eq!(
            requests.extra_annotations
                .get("waybill:version-constraint")
                .and_then(|v| v.as_str()),
            Some("^2.28")
        );
        let click = m670_find_entry(&entries, "click");
        // Table-form constraint (`version = "^8.0"`) is recovered.
        assert_eq!(
            click.extra_annotations
                .get("waybill:version-constraint")
                .and_then(|v| v.as_str()),
            Some("^8.0")
        );
    }

    #[test]
    fn m670_poetry_dev_dependencies_emit_development_scope() {
        let tmp = tempfile::tempdir().unwrap();
        m670_write_pyproject(
            &tmp,
            r#"
[tool.poetry]
name = "example"

[tool.poetry.dependencies]
python = "^3.11"

[tool.poetry.dev-dependencies]
pytest = "^8.0"
"#,
        );
        let entries = pyproject_declared_deps(tmp.path());
        let pytest = m670_find_entry(&entries, "pytest");
        assert_eq!(
            pytest.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Development)
        );
        assert_eq!(
            pytest.extra_annotations
                .get("waybill:optional-derivation")
                .and_then(|v| v.as_str()),
            Some("poetry-legacy-dev-dependencies")
        );
    }

    #[test]
    fn m670_poetry_group_test_emits_development_group_docs_emits_optional() {
        let tmp = tempfile::tempdir().unwrap();
        m670_write_pyproject(
            &tmp,
            r#"
[tool.poetry]
name = "example"

[tool.poetry.dependencies]
python = "^3.11"

[tool.poetry.group.test.dependencies]
pytest = "^8"

[tool.poetry.group.docs.dependencies]
sphinx = "^7"
"#,
        );
        let entries = pyproject_declared_deps(tmp.path());
        let pytest = m670_find_entry(&entries, "pytest");
        assert_eq!(
            pytest.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Development)
        );
        assert_eq!(
            pytest.extra_annotations
                .get("waybill:optional-derivation")
                .and_then(|v| v.as_str()),
            Some("poetry-legacy-group:test")
        );
        let sphinx = m670_find_entry(&entries, "sphinx");
        assert_eq!(
            sphinx.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Optional)
        );
        assert_eq!(
            sphinx.extra_annotations
                .get("waybill:optional-derivation")
                .and_then(|v| v.as_str()),
            Some("poetry-legacy-group:docs")
        );
    }

    #[test]
    fn m670_pep621_diamond_runtime_wins_over_optional() {
        let tmp = tempfile::tempdir().unwrap();
        m670_write_pyproject(
            &tmp,
            r#"
[project]
name = "example"
version = "1.0.0"
dependencies = ["shared>=1"]

[project.optional-dependencies]
extras = ["shared>=1.5"]
"#,
        );
        let entries = pyproject_declared_deps(tmp.path());
        assert_eq!(entries.len(), 1, "diamond should collapse to one entry");
        let shared = m670_find_entry(&entries, "shared");
        assert_eq!(
            shared.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Runtime),
            "Runtime should win over Optional per m183 Decision 3"
        );
        assert!(!shared.extra_annotations.contains_key("waybill:optional-derivation"));
    }

    #[test]
    fn m670_pep621_precedence_over_poetry_legacy() {
        // When [project] is present, Poetry-legacy sections are ignored.
        let tmp = tempfile::tempdir().unwrap();
        m670_write_pyproject(
            &tmp,
            r#"
[project]
name = "example"
version = "1.0.0"
dependencies = ["pep621-only"]

[tool.poetry.dependencies]
python = "^3.11"
poetry-only = "^1"
"#,
        );
        let entries = pyproject_declared_deps(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "pep621-only");
        assert!(entries.iter().all(|e| e.name != "poetry-only"));
    }

    #[test]
    fn m670_extras_stripped_from_name_but_constraint_preserved() {
        let tmp = tempfile::tempdir().unwrap();
        m670_write_pyproject(
            &tmp,
            r#"
[project]
name = "example"
version = "1.0.0"
dependencies = ["requests[security]>=2.28"]
"#,
        );
        let entries = pyproject_declared_deps(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "requests");
        assert_eq!(
            entries[0].extra_annotations
                .get("waybill:version-constraint")
                .and_then(|v| v.as_str()),
            Some(">=2.28")
        );
    }

    #[test]
    fn m670_source_path_points_at_project_root_directory() {
        // m670: source_path uses the m068 main-module convention —
        // `path+file://<project_root>` (directory), NOT the manifest file
        // itself. This is required so m176's workspace-member derivation
        // (which strips scan-root prefix from URI-form paths without
        // calling `parent()`) yields the correct workspace member name.
        // See workspace_visibility::t007 for the regression this shape
        // prevents.
        let tmp = tempfile::tempdir().unwrap();
        m670_write_pyproject(
            &tmp,
            r#"
[project]
name = "example"
version = "1.0.0"
dependencies = ["requests"]
"#,
        );
        let entries = pyproject_declared_deps(tmp.path());
        assert_eq!(entries.len(), 1);
        assert!(entries[0].source_path.starts_with("path+file://"));
        // The suffix is the temp-dir path itself, not `pyproject.toml`.
        assert!(
            !entries[0].source_path.ends_with("pyproject.toml"),
            "source_path should NOT point at the manifest file: {}",
            entries[0].source_path
        );
        assert!(
            entries[0].source_path.ends_with(
                tmp.path().to_string_lossy().as_ref()
            ),
            "source_path should point at the project-root directory: {}",
            entries[0].source_path
        );
    }
}
