//! Tier 4: requirements*.txt parser (legacy / heterogeneous).
//!
//! Reads any `requirements*.txt` at the project root. Lower-tier source
//! than venv / lockfile because range specs may resolve to different
//! versions at install time; entries get `sbom_tier = "design"` when
//! the version is unpinned.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use waybill_common::types::purl::Purl;

use waybill_common::types::hash::{ContentHash, HashAlgorithm};

use super::super::name_validation::validate_pep508_name;
use super::super::project_roots::{listed_ancestor_class, listed_dot_dir_class};
use super::super::PackageDbEntry;
use super::{build_pypi_purl_str, normalize_pypi_name_for_purl, tokenise_requires_dist_name};

/// Milestone 670 T013 — FR-005a lifecycle-scope classification for a
/// discovered `requirements*.txt` file. Filename + parent-directory
/// heuristic; parent-dir signal wins over filename when both match.
///
/// Priority (per `contracts/requirements_txt.md` §Scope-tag derivation):
/// 1. Parent-dir name ∈ {`docs`, `doc`, `documentation`, `test`, `tests`,
///    `ci`, `.ci`} → Optional with parent-mapped scope name.
/// 2. Filename matches `requirements-<scope>*.txt` OR `<scope>-
///    requirements*.txt` where `<scope>` ∈ {`dev`, `test`, `docs`, `ci`}
///    → Optional with the matched scope.
/// 3. Otherwise → Main.
///
/// Parent-dir + filename matching is case-insensitive. Standard Python
/// convention (`docs/`, `tests/`) uses lowercase, but cpython ships
/// `Doc/requirements.txt` (capital D) — case-insensitive matching
/// covers both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RequirementsScope {
    Main,
    /// Static scope-name string — one of `"dev"`, `"test"`, `"docs"`, `"ci"`.
    Optional(&'static str),
}

impl RequirementsScope {
    pub(crate) fn as_optional_name(&self) -> Option<&'static str> {
        match self {
            RequirementsScope::Optional(name) => Some(name),
            RequirementsScope::Main => None,
        }
    }
}

/// FR-005a classifier — see [`RequirementsScope`] for the priority order.
pub(crate) fn classify_requirements_scope(path: &Path) -> RequirementsScope {
    // Priority 1: parent-directory signal (case-insensitive).
    if let Some(parent) = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
    {
        let parent_lower = parent.to_ascii_lowercase();
        match parent_lower.as_str() {
            "docs" | "doc" | "documentation" => return RequirementsScope::Optional("docs"),
            "test" | "tests" => return RequirementsScope::Optional("test"),
            "ci" | ".ci" => return RequirementsScope::Optional("ci"),
            _ => {}
        }
    }
    // Priority 2: filename signal.
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    name_scope(filename).map_or(RequirementsScope::Main, RequirementsScope::Optional)
}

/// Spec 1615 (#1614) research R1: the words a file name may use for each
/// C155 value. `docs`/`doc`/`documentation` is the parent-directory rule's
/// set; the long and plural forms are matched under the same word boundary.
const SCOPE_WORDS: [(&str, &[&str]); 4] = [
    ("dev", &["dev", "development"]),
    ("test", &["test", "tests", "testing"]),
    ("docs", &["doc", "docs", "documentation"]),
    ("ci", &["ci"]),
];

/// The C155 value `filename` names, if any.
fn name_scope(filename: &str) -> Option<&'static str> {
    SCOPE_WORDS
        .iter()
        .find(|(_, words)| words.iter().any(|w| matches_scope_filename(filename, w)))
        .map(|(scope, _)| *scope)
}

/// Spec 1615 (#1614) FR-002: the C155 value a docs or test directory gives.
fn docs_or_test_dir_scope(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "docs" | "doc" | "documentation" => Some("docs"),
        "test" | "tests" => Some("test"),
        _ => None,
    }
}

/// Spec 1615 (#1614) FR-002 to FR-004: the nearest ancestor of `path` below
/// `root` that is a listed dot-directory or a docs or test directory gives
/// the scope, in one nearest-first search. A path outside `root` keeps spec
/// 1613's search over its whole path, dot-directories only (research R3).
fn nearest_ancestor_scope(path: &Path, root: &Path) -> Option<&'static str> {
    let Ok(rel) = path.strip_prefix(root) else {
        return listed_ancestor_class(path, root).map(|c| c.requirements_scope());
    };
    rel.parent()?.components().rev().filter_map(|c| c.as_os_str().to_str()).find_map(|name| {
        listed_dot_dir_class(name).map(|c| c.requirements_scope()).or_else(|| docs_or_test_dir_scope(name))
    })
}

/// Spec 1613 (#1610, FR-004): [`classify_requirements_scope`], then the
/// nearest listed CI or docs dot-directory among the file's ancestors below
/// `root` when neither its parent directory nor its name gives a scope.
///
/// Spec 1615 (#1614): the same search also takes the nearest `docs`/`doc`/
/// `documentation` or `test`/`tests` directory ([`nearest_ancestor_scope`]).
pub(crate) fn classify_requirements_scope_under(path: &Path, root: &Path) -> RequirementsScope {
    match classify_requirements_scope(path) {
        RequirementsScope::Main => {
            nearest_ancestor_scope(path, root).map_or(RequirementsScope::Main, RequirementsScope::Optional)
        }
        scoped => scoped,
    }
}

/// Spec 1613 (#1610, FR-004a): whether the pip reader reads `path` as a
/// requirements file. Besides `requirements*.txt`, a name the scope
/// classifier labels (`<scope>-requirements*.txt`), and any
/// `*requirements*.txt` under a listed CI or docs dot-directory, which the
/// classifier labels by that directory. Other names (`tox-requirements.txt`)
/// are not read: nothing would label them, and they would read as runtime.
/// With `root` absent, every ancestor counts (discovery, which has no root).
pub(crate) fn is_requirements_file(path: &Path, root: Option<&Path>) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else { return false };
    if !name.ends_with(".txt") {
        return false;
    }
    if name.starts_with("requirements") {
        return true;
    }
    if name_scope(name).is_some() {
        return true;
    }
    name.to_ascii_lowercase().contains("requirements")
        && listed_ancestor_class(path, root.unwrap_or(Path::new(""))).is_some()
}

/// True when `filename` matches `requirements-<scope>*.txt` OR
/// `<scope>-requirements*.txt`, case-insensitive.
///
/// Word-boundary shape — `special` does NOT match `ci` scope even
/// though `special` contains the substring `ci`. Only recognized
/// tokens are `requirements-<scope>` or `<scope>-requirements`
/// followed by end-of-stem, another `-`, or `.` (before `.txt`).
fn matches_scope_filename(filename: &str, scope: &str) -> bool {
    let f = filename.to_ascii_lowercase();
    let stem = f.strip_suffix(".txt").unwrap_or(&f);
    // Shape 1: requirements-<scope>[-...]
    let prefix1 = format!("requirements-{scope}");
    if let Some(rest) = stem.strip_prefix(&prefix1) {
        if rest.is_empty() || rest.starts_with('-') || rest.starts_with('.') {
            return true;
        }
    }
    // Shape 2: <scope>-requirements[-...]
    let prefix2 = format!("{scope}-requirements");
    if let Some(rest) = stem.strip_prefix(&prefix2) {
        if rest.is_empty() || rest.starts_with('-') || rest.starts_with('.') {
            return true;
        }
    }
    false
}

/// C155, the scope of the requirements file a package was read from.
pub(crate) const ANN_REQ_FILE_SCOPE: &str = "waybill:python-req-file-scope";

pub(super) fn read_requirements_files(rootfs: &Path, scan_root: &Path) -> Option<Vec<PackageDbEntry>> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(rootfs) else {
        return None;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && is_requirements_file(p, Some(scan_root)))
        .collect();
    paths.sort();
    for path in paths {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let source_path = path.to_string_lossy().into_owned();
        // Milestone 670 T013 — classify the file's lifecycle scope
        // per FR-005a. All entries parsed from this file share the
        // same scope tag.
        let scope = classify_requirements_scope_under(&path, scan_root);
        let parsed = parse_requirements(&text);
        // #1522: `requirements.txt` is also cget's file name. A file most
        // of whose lines are not PEP 508 is not a pip file; emitting its
        // few PEP 508-looking lines would fabricate PyPI packages.
        if parsed.rejected.len() * 2 > parsed.rejected.len() + parsed.entries.len() {
            tracing::warn!(
                path = %source_path,
                rejected = parsed.rejected.len(),
                accepted = parsed.entries.len(),
                "pip: most lines are not PEP 508 requirements; not reading this file as pip",
            );
            continue;
        }
        for line in &parsed.rejected {
            tracing::debug!(path = %source_path, line = %line, "pip: skipping a line that is not a PEP 508 requirement");
        }
        let mut file_entries: Vec<PackageDbEntry> = parsed
            .entries
            .into_iter()
            .filter_map(|entry| entry.into_package_db_entry(&source_path, scope))
            .collect();
        link_via_annotations(&mut file_entries, &parse_via(&text));
        out.extend(file_entries);
    }
    if out.is_empty() { None } else { Some(out) }
}

/// Spec 1537 (#1234): a compiled requirements file (pip-compile, uv pip
/// compile) pins the whole closure and records, under each pin, the pins
/// that pull it in:
///
/// ```text
/// amqp==5.1.1
///     # via kombu
/// click==8.1.3
///     # via
///     #   celery
///     #   flask
/// ```
///
/// Older pip-compile writes the same thing inline (`amqp==5.1.1  # via
/// kombu`). Returns each pin's name (PEP 503) with the names it is via. A
/// `via` that is not another pin — `-r requirements.in`, or the project
/// itself, `app (pyproject.toml)` — is kept here and matches nothing later.
pub(crate) fn parse_via(text: &str) -> BTreeMap<String, Vec<String>> {
    fn via_rest(comment: &str) -> Option<&str> {
        if comment == "via" {
            Some("")
        } else {
            comment.strip_prefix("via ")
        }
    }
    fn push(out: &mut BTreeMap<String, Vec<String>>, pin: &str, names: &str) {
        for item in names.split(',') {
            let item = item.split(" (").next().unwrap_or(item).trim();
            let item = item.split('[').next().unwrap_or(item).trim();
            if item.is_empty() || item.starts_with('-') {
                continue;
            }
            let name = normalize_pypi_name_for_purl(item);
            let parents = out.entry(pin.to_string()).or_default();
            if !parents.contains(&name) {
                parents.push(name);
            }
        }
    }
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let joined = text.replace("\\\n", " ");
    let mut current: Option<String> = None;
    let mut in_block = false;
    for raw in joined.lines() {
        let line = raw.trim();
        if line.is_empty() {
            current = None;
            in_block = false;
            continue;
        }
        if let Some(raw_comment) = line.strip_prefix('#') {
            let comment = raw_comment.trim();
            let Some(pin) = current.as_deref() else { continue };
            if let Some(rest) = via_rest(comment) {
                in_block = true;
                push(&mut out, pin, rest);
            } else if in_block && raw_comment.starts_with("  ") && !comment.is_empty() {
                // A block entry is indented under `# via`: `#   celery`.
                push(&mut out, pin, comment);
            } else {
                in_block = false;
            }
            continue;
        }
        in_block = false;
        let (req, comment) = match line.split_once(" #") {
            Some((before, after)) => (before.trim(), Some(after.trim())),
            None => (line, None),
        };
        current = match req.starts_with('-') {
            true => None,
            false => match parse_requirements_line(req) {
                Line::Entry(e) => Some(normalize_pypi_name_for_purl(&e.name)),
                Line::Skipped | Line::NotPep508 => None,
            },
        };
        if let (Some(pin), Some(rest)) = (current.as_deref(), comment.and_then(via_rest)) {
            push(&mut out, pin, rest);
        }
    }
    out
}

/// Writes each `# via` as an edge between pins of the same file: a pin
/// that is `via` another pin is that pin's dependency, named by its exact
/// PURL so a scan holding several versions of one name cannot bind the
/// edge to the wrong one. The declared-requirements pass then gives the
/// file's project an edge only to the pins nothing in the file reaches.
fn link_via_annotations(entries: &mut [PackageDbEntry], via: &BTreeMap<String, Vec<String>>) {
    if via.is_empty() {
        return;
    }
    let by_name: std::collections::HashMap<String, usize> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| (normalize_pypi_name_for_purl(&e.name), i))
        .collect();
    let mut adds: Vec<(usize, String)> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        for parent in via.get(&normalize_pypi_name_for_purl(&e.name)).into_iter().flatten() {
            if let Some(&q) = by_name.get(parent) {
                if q != i {
                    adds.push((q, e.purl.as_str().to_string()));
                }
            }
        }
    }
    for (q, purl) in adds {
        if !entries[q].depends.contains(&purl) {
            entries[q].depends.push(purl);
        }
    }
}

/// One line from a `requirements.txt`-style file, normalised.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RequirementsTxtEntry {
    pub name: String,
    /// Only populated for exactly-pinned (`==`) requirements. For
    /// ranges / unpinned / URL refs, left empty.
    ///
    /// Milestone 670 T012: also populated with the git rev extracted
    /// from `pkg @ git+https://...@<rev>` when the URL fragment
    /// carries a resolvable rev.
    pub version: String,
    /// Original raw line (including operators, extras, hash flags).
    /// Emitted as `waybill:requirement-range` on the component.
    pub range_spec: String,
    /// Non-registry source kind: `"url"` for `https://...`, `"local"`
    /// for `file:...`, `"git"` for `git+...`. None for registry-named
    /// requirements.
    pub source_type: Option<String>,
    /// Per-component content hashes from `--hash=alg:hex` flags. pip
    /// allows multiple `--hash=` flags per requirement (one per
    /// distribution file — sdist + per-platform wheels) and CDX
    /// `components[].hashes[]` is array-shaped, so all are emitted.
    pub hashes: Vec<ContentHash>,
    /// Milestone 670 T012 — PEP 508 direct-URL metadata for entries of
    /// shape `pkg @ git+https://...@rev` or `pkg @ https://...tar.gz`.
    /// `None` for registry-named requirements. When `Some`, the
    /// component emits a `waybill:direct-url-source` annotation with
    /// `{url, kind, resolved_rev}` matching pip's PEP 610
    /// `direct_url.json` metadata shape.
    pub direct_url: Option<DirectUrlRef>,
}

/// Milestone 670 T012 — captured PEP 508 direct-URL source metadata.
///
/// Populated when a requirements line matches the `pkg @ URL` shape.
/// Emitted as the `waybill:direct-url-source` component annotation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DirectUrlRef {
    /// The raw URL string as written in the requirements file.
    pub url: String,
    /// Classification: `"git"` for `git+...` URLs, `"url"` for plain
    /// HTTP/HTTPS URLs pointing at archives (tarballs, wheels).
    pub kind: &'static str,
    /// Statically extractable git rev, when the URL is a VCS reference
    /// with `@<rev>` in the path segment. `None` for archive URLs and
    /// git URLs without a rev suffix.
    pub resolved_rev: Option<String>,
}

impl DirectUrlRef {
    /// Attempt to parse a URL as a PEP 508 direct-URL source. Returns
    /// `None` if the URL doesn't match one of the recognized shapes
    /// (`git+...`, `http://`, `https://`, `hg+...`, `svn+...`, `bzr+...`,
    /// `file://`). VCS URLs with `@<rev>` in the path have the rev
    /// extracted; archive URLs are captured verbatim.
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        let url = raw.trim();
        if url.is_empty() {
            return None;
        }
        // VCS URL prefixes per PEP 440 §Direct references + pip docs.
        // Order matters: `git+https://...` starts with `git+`, not `http`.
        for vcs_prefix in ["git+", "hg+", "svn+", "bzr+"] {
            if url.starts_with(vcs_prefix) {
                let rev = extract_git_rev_from_url(url);
                return Some(DirectUrlRef {
                    url: url.to_string(),
                    kind: "git",
                    resolved_rev: rev,
                });
            }
        }
        if url.starts_with("http://") || url.starts_with("https://") {
            return Some(DirectUrlRef {
                url: url.to_string(),
                kind: "url",
                resolved_rev: None,
            });
        }
        if url.starts_with("file://") {
            return Some(DirectUrlRef {
                url: url.to_string(),
                kind: "local",
                resolved_rev: None,
            });
        }
        None
    }
}

/// Extract the `@<rev>` suffix from a `git+URL@rev` reference.
///
/// PEP 440 §Direct references allow `git+https://host/repo.git@rev`
/// where `<rev>` is a branch, tag, or commit SHA. The rev is the
/// substring between the LAST `@` in the URL path (excluding the
/// scheme separators like `git+ssh://user@host`) and either the URL
/// fragment (`#egg=`) or the URL end.
///
/// Returns `None` when no rev suffix is present.
fn extract_git_rev_from_url(url: &str) -> Option<String> {
    // Strip fragment (`#egg=...`) first so it doesn't confuse the
    // rev extraction.
    let before_frag = url.split_once('#').map(|(a, _)| a).unwrap_or(url);
    // Strip the scheme prefix (`git+`, then `https://` etc) so the
    // `user@host` colon in ssh URLs doesn't get mistaken for a rev.
    let after_scheme = before_frag
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(before_frag);
    // The LAST `@` in the remaining path is the rev separator per
    // PEP 440. If it's absent (or if the only `@` is the ssh user
    // separator `user@host`, which comes BEFORE any `/`), no rev.
    let last_at = after_scheme.rfind('@')?;
    // Guard: if `last_at` is in the host portion (before the first
    // `/`), it's the ssh user separator, not a rev. Only accept when
    // the `@` sits inside the path segment.
    let first_slash = after_scheme.find('/')?;
    if last_at < first_slash {
        return None;
    }
    let rev = after_scheme[last_at + 1..].trim();
    if rev.is_empty() {
        None
    } else {
        Some(rev.to_string())
    }
}

impl RequirementsTxtEntry {
    fn into_package_db_entry(
        self,
        source_path: &str,
        scope: RequirementsScope,
    ) -> Option<PackageDbEntry> {
        if self.name.is_empty() {
            return None;
        }
        // PURL for empty version: `pkg:pypi/<name>` (no @). packageurl
        // crate accepts this.
        let purl_str = build_pypi_purl_str(&self.name, &self.version);
        let purl = Purl::new(&purl_str).ok()?;
        // Tier: `source` when the requirement is exactly pinned
        // (`==` gives us a concrete version); `design` for ranges /
        // unpinned / URL refs where we kept the raw range string
        // but have no resolved version. A project that exclusively
        // pins its deps is authoritative for the pypi ecosystem —
        // `complete_ecosystems` keys off this.
        let tier = if self.version.is_empty() {
            "design"
        } else {
            "source"
        };
        // Milestone 236 (C151): on the design-tier branch, tag with
        // `waybill:unresolved-reason` naming the resolution boundary.
        // The requirement has no `==X.Y` pin and no lockfile fallback
        // resolved it upstream.
        let mut extra_annotations: std::collections::BTreeMap<
            String,
            serde_json::Value,
        > = Default::default();
        // Feature 1218 (#1214) T022 — C55 `waybill:source-mechanism`,
        // unconditionally on every entry this reader emits.
        //
        // Why a reader-level annotation for a three-component problem:
        // invariant I7's mechanism-C population contains one pypi
        // component (`go-opentelemetry`'s `codespell`, from a root
        // `requirements.txt` in a Go repository) beside two
        // `pkg:bazel/*` components that already carry
        // `bazel-http-archive`. A consumer could identify the bazel pair
        // machine-readably and had to parse the string
        // `"requirements.txt"` to identify the pip one. Annotating only
        // that component would be worse than annotating none — a reader
        // that labels some of its own output and not the rest cannot be
        // relied on either way — so this fires for all of them. Measured
        // radius: 41 components across five corpus targets.
        extra_annotations.insert(
            "waybill:source-mechanism".to_string(),
            serde_json::json!("pip-requirements-txt"),
        );
        if tier == "design" {
            // Milestone 670 T012: differentiate the m236 reason for PEP
            // 508 direct-URL entries — the resolution boundary is "URL
            // present but no rev extractable," which is diagnostically
            // distinct from "no version specifier on a registry entry."
            let reason = if self.direct_url.is_some() && self.version.is_empty() {
                "PEP 508 direct-URL entry; no rev extractable from URL"
            } else {
                "no version specifier in requirements.txt; no uv.lock / poetry.lock fallback"
            };
            extra_annotations.insert(
                "waybill:unresolved-reason".to_string(),
                serde_json::Value::String(reason.to_string()),
            );
        }
        // Milestone 670 T013 — FR-005a scope-heuristic annotation.
        // Present ONLY when the file's classification yielded an
        // Optional scope (dev/test/docs/ci). Runtime files (bare
        // `requirements.txt` at project-standard locations) omit the
        // annotation. Catalog row C158 registered by T016.
        if let Some(scope_name) = scope.as_optional_name() {
            extra_annotations.insert(
                ANN_REQ_FILE_SCOPE.to_string(),
                serde_json::Value::String(scope_name.to_string()),
            );
        }
        // Milestone 670 T012: `waybill:direct-url-source` annotation
        // for PEP 508 direct-URL entries. Carries `{url, kind, resolved_rev}`
        // matching pip's PEP 610 `direct_url.json` metadata shape.
        // Catalog row C154 registered by T016.
        if let Some(direct) = &self.direct_url {
            let mut obj = serde_json::Map::new();
            obj.insert("url".to_string(), serde_json::Value::String(direct.url.clone()));
            obj.insert("kind".to_string(), serde_json::Value::String(direct.kind.to_string()));
            obj.insert(
                "resolved_rev".to_string(),
                match &direct.resolved_rev {
                    Some(rev) => serde_json::Value::String(rev.clone()),
                    None => serde_json::Value::Null,
                },
            );
            extra_annotations.insert(
                "waybill:direct-url-source".to_string(),
                serde_json::Value::Object(obj),
            );
        }
        // Milestone 670 T013 — populate lifecycle_scope from FR-005a
        // classification. Optional-scope files (dev/test/docs/ci) mark
        // every emitted entry as `LifecycleScope::Optional`. Main-scope
        // files leave it None (Runtime by absence of tag; interacts
        // cleanly with m183's `is_none()` guard in
        // `apply_optional_derivation_annotation`).
        let lifecycle_scope = match scope {
            RequirementsScope::Main => None,
            RequirementsScope::Optional(_) => {
                Some(waybill_common::resolution::LifecycleScope::Optional)
            }
        };
        Some(PackageDbEntry {
            extra_source_paths: Vec::new(),
            depends_ecosystem: None,
            build_inclusion: None,
            purl,
            name: self.name,
            version: self.version,
            arch: None,
            source_path: source_path.to_string(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope,
            requirement_ranges: vec![self.range_spec],
            source_type: self.source_type,
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
            hashes: self.hashes,
            sbom_tier: Some(tier.to_string()),
            shade_relocation: None,
            extra_annotations,
            binary_role: None,
        })
    }
}

/// Parse raw `requirements.txt` text. Tolerates:
/// - `# comments` (full-line or trailing).
/// - Blank lines.
/// - `-r <other.txt>` includes (ignored this milestone; follow-up to recurse).
/// - `--hash=sha256:...` flags on their own line or trailing.
/// - URL refs (`https://...`, `git+...`, `file:...`).
/// - Pinned (`==`) and ranged (`>=`, `<`, `~=`, `!=`) requirements.
pub(crate) fn parse_requirements_file_text(text: &str) -> Vec<RequirementsTxtEntry> {
    parse_requirements(text).entries
}

/// The requirement lines of a `requirements.txt`-style file, split into
/// the ones that parsed and the ones that are not PEP 508 (#1522).
#[derive(Debug, Default)]
pub(crate) struct ParsedRequirements {
    pub entries: Vec<RequirementsTxtEntry>,
    pub rejected: Vec<String>,
}

pub(crate) fn parse_requirements(text: &str) -> ParsedRequirements {
    let mut out = ParsedRequirements::default();
    // Deal with line continuations: a trailing backslash joins to the
    // next line. Common in pinned-with-hash blocks.
    let joined = text.replace("\\\n", " ");
    for raw in joined.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // Strip full-line comments.
        if line.starts_with('#') {
            continue;
        }
        // Strip trailing comments (but only when clearly after a space).
        let line = match line.split_once(" #") {
            Some((before, _)) => before.trim(),
            None => line,
        };
        // Skip `-r`, `-c`, `--index-url`, etc. lines — meta-commands.
        if line.starts_with('-') {
            continue;
        }

        match parse_requirements_line(line) {
            Line::Entry(entry) => out.entries.push(entry),
            Line::Skipped => {}
            Line::NotPep508 => out.rejected.push(line.to_string()),
        }
    }
    out
}

enum Line {
    Entry(RequirementsTxtEntry),
    /// A requirement whose environment marker excludes it.
    Skipped,
    /// Not a PEP 508 requirement, e.g. a cget `owner/repo@ref` line (#1522).
    NotPep508,
}

/// Parse a single non-blank, non-comment, non-meta requirements line.
fn parse_requirements_line(line: &str) -> Line {
    match parse_requirements_line_inner(line) {
        Some(entry) if validate_pep508_name(&entry.name).is_err() && entry.source_type.is_none() => {
            Line::NotPep508
        }
        Some(entry) => Line::Entry(entry),
        None if is_bare_at_reference(line) => Line::NotPep508,
        None => Line::Skipped,
    }
}

fn parse_requirements_line_inner(line: &str) -> Option<RequirementsTxtEntry> {
    // Split off `--hash=alg:hex` flags. pip allows MULTIPLE per
    // requirement (one for sdist + one per platform wheel) so collect
    // all of them. Each flag has form `--hash=<alg>:<hex>`.
    let body = line.split("--hash").next().unwrap_or(line).trim();
    let hashes = parse_hash_flags(line);

    // Milestone 670 T012: PEP 508 direct-URL entry (`pkg @ URL` shape).
    // PEP 508 mandates spaces around the `@` when the URL follows a name;
    // the presence of ` @ ` distinguishes this from `name[extras]` or
    // `name==version`. Handled BEFORE the bare-URL branches so the name-
    // prefix supersedes the `egg=` fragment fallback.
    if let Some((name_part, url_part)) = body.split_once(" @ ") {
        if let Some(direct) = DirectUrlRef::parse(url_part) {
            if let Some(name) = tokenise_requires_dist_name(name_part) {
                let version = direct.resolved_rev.clone().unwrap_or_default();
                let source_type = direct.kind.to_string();
                return Some(RequirementsTxtEntry {
                    name,
                    version,
                    range_spec: body.to_string(),
                    source_type: Some(source_type),
                    hashes,
                    direct_url: Some(direct),
                });
            }
        }
    }

    // URL-style sources (bare, without `pkg @ ` prefix).
    //
    // Milestone 670 T012 note: we DO populate `direct_url` for bare-URL
    // entries so the `waybill:direct-url-source` annotation surfaces
    // consistently across both shapes. Version stays empty on this
    // branch (design-tier) to preserve pre-m670 `pkg:pypi/<name>` PURL
    // shape — the git rev from `@ref` in a bare-git-URL is *branch/tag*
    // metadata, not a resolved version. The name-prefixed shape (`pkg
    // @ git+URL@rev`) treats the rev AS the resolved version because
    // the PEP 508 syntax makes that intent explicit.
    if body.starts_with("git+") {
        // e.g. `git+https://github.com/foo/bar.git@rev#egg=bar`
        let name = egg_fragment(body).unwrap_or_else(|| "unknown".to_string());
        let direct = DirectUrlRef::parse(body);
        return Some(RequirementsTxtEntry {
            name,
            version: String::new(),
            range_spec: body.to_string(),
            source_type: Some("git".to_string()),
            hashes,
            direct_url: direct,
        });
    }
    if body.starts_with("http://") || body.starts_with("https://") {
        let name = egg_fragment(body).unwrap_or_else(|| "unknown".to_string());
        let direct = DirectUrlRef::parse(body);
        return Some(RequirementsTxtEntry {
            name,
            version: String::new(),
            range_spec: body.to_string(),
            source_type: Some("url".to_string()),
            hashes,
            direct_url: direct,
        });
    }
    if body.starts_with("file:") || body.starts_with('.') || body.starts_with('/') {
        let name = egg_fragment(body).unwrap_or_else(|| "unknown".to_string());
        return Some(RequirementsTxtEntry {
            name,
            version: String::new(),
            range_spec: body.to_string(),
            source_type: Some("local".to_string()),
            hashes,
            direct_url: None,
        });
    }

    // `name@<not a URL>` with no whitespace is cget's `name@version` or
    // `owner/repo@ref`, not PEP 508, whose direct reference needs a URL
    // after the `@` (#1522). The spaced `name @ <not a URL>` form keeps
    // its lenient registry parse (milestone 670).
    if is_bare_at_reference(body) {
        return None;
    }

    // Registry-style: `name[extras] OP version, OP version; marker`.
    // Reuse the PEP 508 tokeniser for the name; detect `==` for pinning.
    let name = tokenise_requires_dist_name(body)?;

    // Look for a single `==` pin to populate `version`.
    let version = pinned_version_from(body).unwrap_or_default();

    Some(RequirementsTxtEntry {
        name,
        version,
        range_spec: body.to_string(),
        source_type: None,
        hashes,
        direct_url: None,
    })
}

/// `true` for `name@rest` where the `@` directly follows the name and
/// `rest` is not a URL a PEP 508 direct reference accepts.
fn is_bare_at_reference(body: &str) -> bool {
    let body = body.split("--hash").next().unwrap_or(body).trim();
    let Some(at) = body.find('@') else {
        return false;
    };
    let name = &body[..at];
    if name.is_empty() || name.contains(char::is_whitespace) || name.contains(['[', '<', '>', '=', '!', '~', ';']) {
        return false;
    }
    let rest = body[at + 1..].split_whitespace().next().unwrap_or("");
    DirectUrlRef::parse(rest).is_none()
}

/// Extract every `--hash=<alg>:<hex>` flag from a requirements line.
/// Tolerates both `--hash=sha256:abc` and `--hash sha256:abc` shapes
/// (pip accepts the latter via getopt-style spacing). Unknown
/// algorithms are silently dropped (not just sha256/512/1 — md5
/// also gets through ContentHash::with_algorithm but pip docs only
/// list sha256/384/512).
fn parse_hash_flags(line: &str) -> Vec<ContentHash> {
    let mut out = Vec::new();
    // Iterate on `--hash` substring; each occurrence is followed by
    // either `=` or ` ` then `<alg>:<hex>`.
    let mut rest = line;
    while let Some(idx) = rest.find("--hash") {
        let after = &rest[idx + "--hash".len()..];
        // Skip the separator (`=` or whitespace).
        let after = after.trim_start_matches(|c: char| c == '=' || c.is_whitespace());
        // Take up to the next whitespace or end.
        let token_end = after
            .find(|c: char| c.is_whitespace())
            .unwrap_or(after.len());
        let token = &after[..token_end];
        if let Some((alg_str, hex)) = token.split_once(':') {
            if let Some(alg) = parse_hash_alg(alg_str) {
                if let Ok(hash) = ContentHash::with_algorithm(alg, hex) {
                    if !out.contains(&hash) {
                        out.push(hash);
                    }
                }
            }
        }
        rest = &after[token_end..];
    }
    out
}

fn parse_hash_alg(s: &str) -> Option<HashAlgorithm> {
    match s.to_ascii_lowercase().as_str() {
        "sha256" => Some(HashAlgorithm::Sha256),
        "sha512" => Some(HashAlgorithm::Sha512),
        "sha1" => Some(HashAlgorithm::Sha1),
        // sha384 not in HashAlgorithm yet; pip uses sha256/384/512.
        // md5 is supported by ContentHash but pip rejects md5 hashes.
        _ => None,
    }
}

/// Extract `egg=<name>` from a URL-style requirement, if present.
fn egg_fragment(url: &str) -> Option<String> {
    let frag = url.split_once('#')?.1;
    for part in frag.split('&') {
        if let Some(value) = part.strip_prefix("egg=") {
            let clean = value.split('[').next().unwrap_or(value);
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
    }
    None
}

/// If the requirement has a single `==` pin (and no disjunction like
/// `==1.0 || ==2.0`, which pip doesn't support anyway), return that
/// version string. Returns None for ranges.
fn pinned_version_from(body: &str) -> Option<String> {
    // Drop any env marker first.
    let head = body.split_once(';').map(|x| x.0).unwrap_or(body);
    // Look for `==` as an exact operator. Ignore `!=` and `~=`.
    for part in head.split(',') {
        let p = part.trim();
        if let Some(rest) = p.strip_prefix("==") {
            return Some(rest.trim().to_string());
        }
        // `name == 1.0` form: find `==` after some whitespace.
        if let Some(idx) = p.find("==") {
            // Ensure it's an `==` operator, not a substring of `===` etc.
            let before = &p[..idx];
            let after = &p[idx + 2..];
            if !before.ends_with('=') && !after.starts_with('=') {
                return Some(after.trim().trim_start_matches(' ').to_string());
            }
        }
    }
    None
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    #[test]
    fn requirements_txt_pinned_populates_version_and_range() {
        let entries = parse_requirements_file_text("requests==2.31.0\n");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "requests");
        assert_eq!(entries[0].version, "2.31.0");
        assert_eq!(entries[0].range_spec, "requests==2.31.0");
        assert!(entries[0].source_type.is_none());
    }

    #[test]
    fn requirements_txt_ranged_leaves_version_empty() {
        let entries = parse_requirements_file_text("requests>=2,<3\n");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "requests");
        assert!(entries[0].version.is_empty());
        assert_eq!(entries[0].range_spec, "requests>=2,<3");
    }

    #[test]
    fn requirements_txt_bare_name_empty_version() {
        let entries = parse_requirements_file_text("requests\n");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "requests");
        assert!(entries[0].version.is_empty());
    }

    #[test]
    fn requirements_txt_strips_comments_and_blank_lines() {
        let text = "\
# top comment
requests==2.31.0  # trailing comment
# another

urllib3>=2  # with space before hash
";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "requests");
        assert_eq!(entries[1].name, "urllib3");
    }

    #[test]
    fn requirements_txt_skips_meta_commands() {
        let text = "\
-r other.txt
--index-url https://pypi.org/simple/
requests==2.31.0
";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "requests");
    }

    #[test]
    fn requirements_txt_strips_hash_flags() {
        let text = "\
requests==2.31.0 --hash=sha256:abc123 --hash=sha256:def456
urllib3>=2 \\
    --hash=sha256:zzz
";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "requests");
        assert_eq!(entries[1].name, "urllib3");
    }

    #[test]
    fn parse_hash_flags_captures_single_sha256() {
        let line = "requests==2.31.0 --hash=sha256:58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f";
        let hashes = parse_hash_flags(line);
        assert_eq!(hashes.len(), 1);
        assert_eq!(hashes[0].algorithm, HashAlgorithm::Sha256);
        assert_eq!(
            hashes[0].value.as_str(),
            "58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f"
        );
    }

    #[test]
    fn parse_hash_flags_captures_multiple() {
        // pip allows multiple --hash= flags (sdist + per-platform wheel).
        let sha256 = "a".repeat(64);
        let sha512 = "b".repeat(128);
        let line = format!(
            "requests==2.31.0 --hash=sha256:{sha256} --hash=sha512:{sha512}"
        );
        let hashes = parse_hash_flags(&line);
        assert_eq!(hashes.len(), 2);
        // Order preserved (first --hash flag → first slot).
        assert_eq!(hashes[0].algorithm, HashAlgorithm::Sha256);
        assert_eq!(hashes[1].algorithm, HashAlgorithm::Sha512);
    }

    #[test]
    fn parse_hash_flags_dedups_identical_entries() {
        // Pathological: same hash specified twice. Only emit once.
        let line = "x==1 --hash=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa --hash=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let hashes = parse_hash_flags(line);
        assert_eq!(hashes.len(), 1);
    }

    #[test]
    fn parse_hash_flags_drops_unknown_algorithm() {
        let line = "x==1 --hash=md4:dead --hash=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let hashes = parse_hash_flags(line);
        assert_eq!(hashes.len(), 1);
        assert_eq!(hashes[0].algorithm, HashAlgorithm::Sha256);
    }

    #[test]
    fn parse_hash_flags_returns_empty_when_absent() {
        let line = "requests==2.31.0";
        assert!(parse_hash_flags(line).is_empty());
    }

    #[test]
    fn parse_hash_flags_handles_space_separator() {
        // pip also accepts `--hash sha256:abc` (getopt-style).
        let line = "x==1 --hash sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let hashes = parse_hash_flags(line);
        assert_eq!(hashes.len(), 1);
    }

    #[test]
    fn requirements_txt_threads_hashes_through_to_entry() {
        let text = "requests==2.31.0 --hash=sha256:58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].hashes.len(), 1);
        let pdb = entries[0]
            .clone()
            .into_package_db_entry("/req.txt", RequirementsScope::Main)
            .expect("converts");
        assert_eq!(pdb.hashes.len(), 1);
        assert_eq!(pdb.hashes[0].algorithm, HashAlgorithm::Sha256);
    }

    #[test]
    fn requirements_txt_git_url_source_type() {
        let text = "git+https://github.com/psf/requests.git@main#egg=requests\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "requests");
        assert_eq!(entries[0].source_type.as_deref(), Some("git"));
        assert!(entries[0].version.is_empty());
    }

    #[test]
    fn requirements_txt_https_url_source_type() {
        let text = "https://example.com/pkg/foo-1.0.tar.gz#egg=foo\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "foo");
        assert_eq!(entries[0].source_type.as_deref(), Some("url"));
    }

    #[test]
    fn requirements_txt_file_ref_source_type() {
        let text = "file:./local/pkg#egg=local-pkg\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "local-pkg");
        assert_eq!(entries[0].source_type.as_deref(), Some("local"));
    }

    #[test]
    fn requirements_txt_pinned_produces_source_tier() {
        // Exact pin (`==`) means the requirement IS authoritative for
        // the version — same semantics as a cargo.lock line.
        // `complete_ecosystems` keys off source/deployed tier, so
        // pinned requirements.txt entries mark the pypi ecosystem
        // complete and drive sbomqs `sbom_completeness_declared`.
        let entry = RequirementsTxtEntry {
            name: "requests".into(),
            version: "2.31.0".into(),
            range_spec: "requests==2.31.0".into(),
            source_type: None,
            hashes: Vec::new(),
            direct_url: None,
        };
        let pdb = entry.into_package_db_entry("/req.txt", RequirementsScope::Main).expect("converts");
        assert_eq!(pdb.sbom_tier.as_deref(), Some("source"));
        assert_eq!(pdb.requirement_ranges.as_slice(), &["requests==2.31.0".to_string()]);
    }

    #[test]
    fn requirements_txt_unpinned_stays_design_tier() {
        // Range / unpinned requirements have no resolved version —
        // tier stays `design`, same as a package.json dependency
        // block without a lockfile.
        let entry = RequirementsTxtEntry {
            name: "requests".into(),
            version: "".into(),
            range_spec: "requests>=2.0".into(),
            source_type: None,
            hashes: Vec::new(),
            direct_url: None,
        };
        let pdb = entry.into_package_db_entry("/req.txt", RequirementsScope::Main).expect("converts");
        assert_eq!(pdb.sbom_tier.as_deref(), Some("design"));
    }

    #[test]
    fn m236_pip_design_tier_carries_unresolved_reason() {
        // Milestone 236 (C151): unpinned requirement (design-tier)
        // MUST carry `waybill:unresolved-reason` naming pip's
        // resolution boundary.
        let entry = RequirementsTxtEntry {
            name: "waybill-fixture-pip".into(),
            version: String::new(),
            range_spec: "waybill-fixture-pip>=2.0".into(),
            source_type: None,
            hashes: Vec::new(),
            direct_url: None,
        };
        let pdb = entry.into_package_db_entry("/req.txt", RequirementsScope::Main).expect("converts");
        assert_eq!(pdb.sbom_tier.as_deref(), Some("design"));
        let reason = pdb
            .extra_annotations
            .get("waybill:unresolved-reason")
            .expect("C151 annotation present on design-tier pip component");
        assert_eq!(
            reason.as_str().unwrap(),
            "no version specifier in requirements.txt; no uv.lock / poetry.lock fallback",
        );
    }

    #[test]
    fn m236_pip_source_tier_does_not_carry_unresolved_reason() {
        // FR-004 negative assertion: source-tier components MUST NOT
        // carry the C151 annotation.
        let entry = RequirementsTxtEntry {
            name: "waybill-fixture-pip".into(),
            version: "1.2.3".into(),
            range_spec: "waybill-fixture-pip==1.2.3".into(),
            source_type: None,
            hashes: Vec::new(),
            direct_url: None,
        };
        let pdb = entry.into_package_db_entry("/req.txt", RequirementsScope::Main).expect("converts");
        assert_eq!(pdb.sbom_tier.as_deref(), Some("source"));
        assert!(
            !pdb.extra_annotations.contains_key("waybill:unresolved-reason"),
            "source-tier components MUST NOT carry C151 annotation",
        );
    }

    /// Feature 1218 (#1214) T023 — every entry this reader emits carries
    /// C55 `waybill:source-mechanism`, regardless of tier or scope.
    ///
    /// Uniform by design. Invariant I7's mechanism-C population needs it on
    /// one component, but a reader that annotates part of its own output is
    /// not something a consumer can rely on either way. Mirrors the
    /// equivalent assertion in `scan_fs/package_db/bazel.rs`.
    #[test]
    fn m1218_every_requirements_entry_carries_source_mechanism() {
        for (version, range, scope) in [
            ("2.28.0", "requests==2.28.0", RequirementsScope::Main),
            ("", "requests>=2", RequirementsScope::Main),
        ] {
            let entry = RequirementsTxtEntry {
                name: "requests".into(),
                version: version.into(),
                range_spec: range.into(),
                source_type: None,
                direct_url: None,
                hashes: Vec::new(),
            };
            let pdb = entry
                .into_package_db_entry("/req.txt", scope)
                .expect("converts");
            assert_eq!(
                pdb.extra_annotations
                    .get("waybill:source-mechanism")
                    .and_then(|v| v.as_str()),
                Some("pip-requirements-txt"),
                "every requirements.txt entry identifies its build system \
                 machine-readably; got {:?}",
                pdb.extra_annotations.get("waybill:source-mechanism"),
            );
        }
    }

    #[test]
    fn requirements_txt_empty_version_purl_well_formed() {
        let entry = RequirementsTxtEntry {
            name: "requests".into(),
            version: String::new(),
            range_spec: "requests>=2".into(),
            source_type: None,
            hashes: Vec::new(),
            direct_url: None,
        };
        let pdb = entry.into_package_db_entry("/req.txt", RequirementsScope::Main).expect("converts");
        // packageurl-rs accepts `pkg:pypi/<name>` without @version.
        assert!(pdb.purl.as_str().starts_with("pkg:pypi/requests"));
    }

    // -----------------------------------------------------------------
    // Milestone 670 T012: PEP 508 direct-URL entry (`pkg @ URL` shape)
    // -----------------------------------------------------------------

    #[test]
    fn m670_pep508_direct_url_git_with_rev_populates_version() {
        // PEP 508 `pkg @ git+URL@<rev>` — the rev is the resolved
        // version per Q5 clarification. source_type=git, direct_url
        // annotation with resolved_rev populated.
        let text =
            "pygments @ git+https://github.com/pygments/pygments.git@2cad2642\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "pygments");
        assert_eq!(entries[0].version, "2cad2642");
        assert_eq!(entries[0].source_type.as_deref(), Some("git"));

        let direct = entries[0]
            .direct_url
            .as_ref()
            .expect("direct_url populated");
        assert_eq!(direct.kind, "git");
        assert_eq!(direct.resolved_rev.as_deref(), Some("2cad2642"));

        // Emission check: waybill:direct-url-source annotation on the
        // emitted component carries the {url, kind, resolved_rev}
        // structure per pip's PEP 610 direct_url.json metadata shape.
        let pdb = entries[0]
            .clone()
            .into_package_db_entry("/req.txt", RequirementsScope::Main)
            .expect("converts");
        assert_eq!(pdb.purl.as_str(), "pkg:pypi/pygments@2cad2642");
        let anno = pdb
            .extra_annotations
            .get("waybill:direct-url-source")
            .expect("m670 T012 annotation present");
        assert_eq!(anno["kind"].as_str(), Some("git"));
        assert_eq!(anno["resolved_rev"].as_str(), Some("2cad2642"));
        assert!(anno["url"].as_str().unwrap().contains("pygments.git"));
    }

    #[test]
    fn m670_pep508_direct_url_git_without_rev_leaves_version_empty() {
        // git+URL without `@rev` suffix — no rev extractable, tier
        // stays design, m236 reason uses the T012-locked
        // "PEP 508 direct-URL entry; no rev extractable" string.
        let text = "somepkg @ git+https://github.com/foo/somepkg.git\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "somepkg");
        assert!(entries[0].version.is_empty());
        assert_eq!(entries[0].source_type.as_deref(), Some("git"));
        let direct = entries[0]
            .direct_url
            .as_ref()
            .expect("direct_url populated");
        assert_eq!(direct.kind, "git");
        assert!(direct.resolved_rev.is_none());

        let pdb = entries[0]
            .clone()
            .into_package_db_entry("/req.txt", RequirementsScope::Main)
            .expect("converts");
        assert_eq!(pdb.sbom_tier.as_deref(), Some("design"));
        assert_eq!(
            pdb.extra_annotations
                .get("waybill:unresolved-reason")
                .and_then(|v| v.as_str()),
            Some("PEP 508 direct-URL entry; no rev extractable from URL"),
            "T012 locked reason string differentiates direct-URL from unpinned-requirements case",
        );
    }

    #[test]
    fn m670_pep508_direct_url_https_tarball_no_rev() {
        // Real cpython case: `pygments @ https://.../pygments-<sha>.tar.gz`.
        // No git rev semantics; version stays empty; annotation records
        // the URL for provenance.
        let text = "pygments @ https://github.com/pygments/pygments/archive/2cad2642.tar.gz\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "pygments");
        assert!(entries[0].version.is_empty());
        assert_eq!(entries[0].source_type.as_deref(), Some("url"));
        let direct = entries[0]
            .direct_url
            .as_ref()
            .expect("direct_url populated");
        assert_eq!(direct.kind, "url");
        assert!(direct.resolved_rev.is_none());
        assert!(direct.url.ends_with(".tar.gz"));
    }

    #[test]
    fn m670_pep508_direct_url_invalid_url_falls_back_to_registry_parse() {
        // If the `@` separator is present but the following string
        // doesn't match a supported URL scheme, the direct-URL detector
        // returns None and the parser falls through to the registry-
        // shape branch. The tokeniser stops at `@`, so the name is
        // still extracted correctly and the component emits as a
        // design-tier registry entry.
        let text = "requests @ not-a-real-scheme\n";
        let entries = parse_requirements_file_text(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "requests");
        // Falls through to registry-style — no direct_url populated,
        // source_type None, no version pin.
        assert!(entries[0].direct_url.is_none());
        assert_eq!(entries[0].source_type, None);
        assert!(entries[0].version.is_empty());
    }

    // -----------------------------------------------------------------
    // Milestone 670 T013: FR-005a scope-heuristic tests
    // -----------------------------------------------------------------

    #[test]
    fn m670_scope_classify_parent_docs_wins_regardless_of_filename() {
        let path = std::path::PathBuf::from("/proj/docs/requirements.txt");
        assert_eq!(
            classify_requirements_scope(&path),
            RequirementsScope::Optional("docs"),
        );
        // Case-insensitive: cpython ships `Doc/requirements.txt`.
        let path = std::path::PathBuf::from("/proj/Doc/requirements.txt");
        assert_eq!(
            classify_requirements_scope(&path),
            RequirementsScope::Optional("docs"),
        );
        // `documentation/` also maps to `docs`.
        let path = std::path::PathBuf::from("/proj/documentation/requirements.txt");
        assert_eq!(
            classify_requirements_scope(&path),
            RequirementsScope::Optional("docs"),
        );
    }

    #[test]
    fn m670_scope_classify_parent_tests_and_ci_variants() {
        for (dirname, expected) in [
            ("tests", "test"),
            ("test", "test"),
            ("Tests", "test"),
            ("ci", "ci"),
            (".ci", "ci"),
        ] {
            let path = std::path::PathBuf::from(format!("/proj/{dirname}/requirements.txt"));
            assert_eq!(
                classify_requirements_scope(&path),
                RequirementsScope::Optional(expected),
                "parent-dir `{dirname}` should map to scope `{expected}`",
            );
        }
    }

    #[test]
    fn m670_scope_classify_filename_signals() {
        for (fname, expected) in [
            ("requirements-dev.txt", Some("dev")),
            ("requirements-test.txt", Some("test")),
            ("requirements-docs.txt", Some("docs")),
            ("requirements-ci.txt", Some("ci")),
            ("dev-requirements.txt", Some("dev")),
            ("test-requirements.txt", Some("test")),
            ("docs-requirements.txt", Some("docs")),
            // Suffix-with-more-tokens after scope name still matches.
            ("requirements-dev-frozen.txt", Some("dev")),
            // Bare `requirements.txt` (no scope token) → Main.
            ("requirements.txt", None),
            // Word-boundary check — `requirements-hypothesis.txt` doesn't
            // match any scope keyword.
            ("requirements-hypothesis.txt", None),
            // Word-boundary check — `special` contains substring `ci`
            // but MUST NOT match the ci scope (regression guard for
            // the substring-based first draft).
            ("requirements-special.txt", None),
            // Spec 1615 (#1614) research R1: every word of a scope's set.
            ("requirements-doc.txt", Some("docs")),
            ("doc-requirements.txt", Some("docs")),
            ("requirements-documentation.txt", Some("docs")),
            ("requirements-development.txt", Some("dev")),
            ("requirements-tests.txt", Some("test")),
            ("testing-requirements.txt", Some("test")),
            ("requirements-testing-frozen.txt", Some("test")),
            ("Requirements-Doc.txt", Some("docs")),
            // ... and the boundary still holds.
            ("requirements-docker.txt", None),
            ("requirements-testinfra.txt", None),
            ("requirements-devel.txt", None),
            ("requirements_dev.txt", None),
        ] {
            let path = std::path::PathBuf::from(format!("/proj/{fname}"));
            let got = classify_requirements_scope(&path);
            let expected_scope = match expected {
                Some(name) => RequirementsScope::Optional(name),
                None => RequirementsScope::Main,
            };
            assert_eq!(
                got, expected_scope,
                "filename `{fname}` classified as {got:?}, expected {expected_scope:?}",
            );
        }
    }

    #[test]
    fn m670_scope_classify_parent_wins_over_filename() {
        // Parent-dir `docs/` beats filename `requirements-dev.txt`.
        // (Contrived — a real project wouldn't mix signals — but the
        // priority order is contract-locked.)
        let path = std::path::PathBuf::from("/proj/docs/requirements-dev.txt");
        assert_eq!(
            classify_requirements_scope(&path),
            RequirementsScope::Optional("docs"),
        );
    }

    #[test]
    fn m670_optional_scope_emits_lifecycle_scope_optional_and_annotation() {
        let entry = RequirementsTxtEntry {
            name: "sphinx".into(),
            version: "".into(),
            range_spec: "sphinx>=7".into(),
            source_type: None,
            hashes: Vec::new(),
            direct_url: None,
        };
        let pdb = entry
            .into_package_db_entry("/proj/docs/requirements.txt", RequirementsScope::Optional("docs"))
            .expect("converts");
        assert_eq!(
            pdb.lifecycle_scope,
            Some(waybill_common::resolution::LifecycleScope::Optional),
            "docs-scoped entries emit as Optional",
        );
        assert_eq!(
            pdb.extra_annotations
                .get("waybill:python-req-file-scope")
                .and_then(|v| v.as_str()),
            Some("docs"),
            "annotation carries the derived scope name",
        );
    }

    #[test]
    fn m670_main_scope_omits_annotation_and_lifecycle_scope() {
        // FR-005a: bare `requirements.txt` at project-standard location →
        // Main → no annotation, lifecycle_scope stays None (Runtime-by-
        // absence-of-tag, matching pre-m670 behavior).
        let entry = RequirementsTxtEntry {
            name: "requests".into(),
            version: "2.31.0".into(),
            range_spec: "requests==2.31.0".into(),
            source_type: None,
            hashes: Vec::new(),
            direct_url: None,
        };
        let pdb = entry
            .into_package_db_entry("/proj/requirements.txt", RequirementsScope::Main)
            .expect("converts");
        assert_eq!(pdb.lifecycle_scope, None);
        assert!(
            !pdb.extra_annotations
                .contains_key("waybill:python-req-file-scope"),
            "Main-scope entries omit the annotation",
        );
    }

    /// #1522: the issue's cget repro, plus an unpinned `owner/repo`. Each
    /// line is rejected on its own: `owner/repo` is not a PEP 508 name, and
    /// `name@version` is not a PEP 508 direct reference (that needs a URL).
    #[test]
    fn s1522_cget_lines_are_not_pep508_requirements() {
        let text = "waybill-fixture-zlib@1.3.1\n\
                    waybill-org/waybill-fixture-repo@v2.0.0 -DFOO=ON\n\
                    waybill-org/waybill-fixture-sha@38c52448b1a4996b3e0e435a877d02441098b1dd\n\
                    waybill-org/waybill-fixture-unpinned\n";
        let parsed = parse_requirements(text);
        assert!(parsed.entries.is_empty(), "{:?}", parsed.entries);
        assert_eq!(parsed.rejected.len(), 4, "{:?}", parsed.rejected);
    }

    /// #1522 control: valid PEP 508 lines, including both direct-reference
    /// shapes, are still accepted and nothing is rejected.
    #[test]
    fn s1522_valid_pep508_lines_still_parse() {
        let text = "requests==2.31.0\n\
                    numpy>=1.26; python_version >= \"3.9\"\n\
                    waybill-fixture-direct @ https://example.invalid/waybill-fixture-direct-1.0.tar.gz\n\
                    Django[bcrypt]~=5.0\n";
        let parsed = parse_requirements(text);
        let names: Vec<&str> = parsed.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["requests", "numpy", "waybill-fixture-direct", "Django"]);
        assert!(parsed.rejected.is_empty(), "{:?}", parsed.rejected);
    }

    /// #1522: a file most of whose lines are rejected is not a pip file.
    /// It emits nothing, not even its one PEP 508-looking line.
    #[test]
    fn s1522_mostly_rejected_file_emits_nothing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("requirements.txt"),
            "waybill-org/waybill-fixture-repo@v2.0.0\n\
             waybill-org/waybill-fixture-sha@38c52448b1a4996b3e0e435a877d02441098b1dd\n\
             waybill-fixture-bare\n",
        )
        .unwrap();
        assert!(read_requirements_files(dir.path(), dir.path()).is_none());
    }

    /// #1522 control: a pip file with one bad line keeps its good lines.
    #[test]
    fn s1522_one_bad_line_in_a_pip_file_is_dropped_alone() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("requirements.txt"),
            "requests==2.31.0\nurllib3>=2\nwaybill-org/waybill-fixture-repo@v2.0.0\n",
        )
        .unwrap();
        let out = read_requirements_files(dir.path(), dir.path()).unwrap();
        let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["requests", "urllib3"]);
    }

    /// Spec 1537 (#1234): flask's `examples/celery/requirements.txt`
    /// shape, pip-compile 7 block form.
    const COMPILED: &str = "#\n#    pip-compile pyproject.toml\n#\namqp==5.1.1\n    # via kombu\ncelery[redis]==5.2.7\n    # via waybill-fixture-app (pyproject.toml)\nclick==8.1.3\n    # via\n    #   celery\n    #   click-repl\nclick-repl==0.2.0\n    # via celery\nkombu==5.2.4\n    # via celery\nvine==5.0.0\n    # via\n    #   amqp\n    #   celery\n";

    #[test]
    fn s1537_parse_via_reads_the_block_form() {
        let via = parse_via(COMPILED);
        assert_eq!(via["amqp"], vec!["kombu"]);
        assert_eq!(via["celery"], vec!["waybill-fixture-app"]);
        assert_eq!(via["click"], vec!["celery", "click-repl"]);
        assert_eq!(via["vine"], vec!["amqp", "celery"]);
    }

    #[test]
    fn s1537_parse_via_reads_the_inline_form_and_skips_includes() {
        let via = parse_via("amqp==5.1.1  # via kombu, Celery\nkombu==5.2.4  # via -r requirements.in\n");
        assert_eq!(via["amqp"], vec!["kombu", "celery"]);
        assert!(!via.contains_key("kombu"), "`-r` names a file, not a pin");
    }

    #[test]
    fn s1537_a_comment_that_is_not_via_ends_the_block() {
        let via = parse_via("a==1\n    # via b\n    # this is a note\n    #   c\nb==2\nc==3\n");
        assert_eq!(via["a"], vec!["b"], "`#   c` after an unrelated comment is not a via name");
    }

    #[test]
    fn s1537_a_hand_written_file_has_no_via() {
        assert!(parse_via("# runtime\nrequests==2.31.0  # pinned for CVE\nflask>=2\n").is_empty());
    }

    #[test]
    fn s1537_via_becomes_exact_purl_edges_between_pins_of_one_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), COMPILED).unwrap();
        let out = read_requirements_files(dir.path(), dir.path()).unwrap();
        let deps = |name: &str| -> Vec<String> {
            let mut d = out.iter().find(|e| e.name == name).unwrap().depends.clone();
            d.sort();
            d
        };
        assert_eq!(deps("celery"), vec![
            "pkg:pypi/click-repl@0.2.0".to_string(),
            "pkg:pypi/click@8.1.3".to_string(),
            "pkg:pypi/kombu@5.2.4".to_string(),
            "pkg:pypi/vine@5.0.0".to_string(),
        ]);
        assert_eq!(deps("kombu"), vec!["pkg:pypi/amqp@5.1.1".to_string()]);
        assert_eq!(deps("amqp"), vec!["pkg:pypi/vine@5.0.0".to_string()]);
        assert_eq!(deps("click-repl"), vec!["pkg:pypi/click@8.1.3".to_string()]);
        assert!(deps("vine").is_empty() && deps("click").is_empty());
    }


    // Spec 1613 (#1610): FR-004a selector and FR-004 ancestor rule.

    #[test]
    fn s1613_the_selector_reads_labelled_names_only() {
        let root = Path::new("/r");
        for (rel, read) in [
            ("requirements.txt", true),
            ("requirements-dev.txt", true),
            ("dev-requirements.txt", true),
            ("docs-requirements.txt", true),
            ("tox-requirements.txt", false),
            ("all_requirements.txt", false),
            // Spec 1615 research R2: a name the classifier now labels is read.
            ("doc-requirements.txt", true),
            ("development-requirements.txt", true),
            ("testing-requirements.txt", true),
            ("tests-requirements.txt", true),
            ("docker-requirements.txt", false),
            (".ci/all_requirements.txt", true),
            ("x/.github/build_requirements.txt", true),
            (".ci/notes.txt", false),
            ("requirements.in", false),
        ] {
            assert_eq!(is_requirements_file(&root.join(rel), Some(root)), read, "{rel}");
        }
        assert!(
            !is_requirements_file(Path::new("/r/.ci/all_requirements.txt"), Some(Path::new("/r/.ci"))),
            "a listed directory above the scan root does not count"
        );
        assert!(is_requirements_file(Path::new("/r/.ci/all_requirements.txt"), None), "discovery has no root");
    }

    #[test]
    fn s1613_the_ancestor_rule_comes_after_the_parent_and_the_name() {
        let root = Path::new("/r");
        for (rel, want) in [
            (".ci/metrics/requirements.txt", Some("ci")),
            (".github/requirements.txt", Some("ci")),
            ("docs/.sphinx/requirements.txt", Some("docs")),
            (".ci/tests/requirements.txt", Some("test")),
            (".github/docs/requirements.txt", Some("docs")),
            (".github/requirements-dev.txt", Some("dev")),
            (".sphinx/ci/requirements.txt", Some("ci")),
            ("src/requirements.txt", None),
        ] {
            assert_eq!(classify_requirements_scope_under(&root.join(rel), root).as_optional_name(), want, "{rel}");
        }
    }

    #[test]
    fn s1613_read_requirements_files_reads_and_labels_widened_names() {
        let dir = tempfile::tempdir().unwrap();
        let ci = dir.path().join(".ci");
        std::fs::create_dir_all(&ci).unwrap();
        std::fs::write(ci.join("all_requirements.txt"), "waybill-fixture-all==1.0\n").unwrap();
        std::fs::write(ci.join("tox-requirements.txt"), "waybill-fixture-tox==1.0\n").unwrap();
        let out = read_requirements_files(&ci, dir.path()).unwrap();
        let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["waybill-fixture-all", "waybill-fixture-tox"], "under .ci every *requirements*.txt is read");
        assert!(out.iter().all(|e| e.lifecycle_scope.is_some()), "and labelled, so none reads as runtime");
        let top = tempfile::tempdir().unwrap();
        std::fs::write(top.path().join("tox-requirements.txt"), "waybill-fixture-tox==1.0\n").unwrap();
        assert!(read_requirements_files(top.path(), top.path()).is_none_or(|v| v.is_empty()), "tox-requirements.txt is not read");
    }

    /// Spec 1615 (#1614) research R3: the nearest docs or test directory, or
    /// listed dot-directory, below the scan root labels a file the parent and
    /// the name do not.
    #[test]
    fn s1615_nearest_docs_or_test_ancestor_labels_the_file() {
        let root = Path::new("/r");
        for (rel, want) in [
            ("p/docs/sphinx/requirements.txt", Some("docs")),
            ("p/Doc/x/y/requirements.txt", Some("docs")),
            ("p/documentation/site/requirements.txt", Some("docs")),
            ("p/tests/corpus/dbt/requirements-x.txt", Some("test")),
            ("p/TEST/integration/requirements.txt", Some("test")),
            // Nearest wins, across both kinds.
            ("p/tests/docs/sphinx/requirements.txt", Some("docs")),
            ("p/docs/tests/x/requirements.txt", Some("test")),
            ("p/.github/docs/x/requirements.txt", Some("docs")),
            ("p/docs/.github/x/requirements.txt", Some("ci")),
            ("p/test/x/.sphinx/requirements.txt", Some("docs")),
            // The parent and the name still come first.
            ("p/tests/fixtures/requirements-docs.txt", Some("docs")),
            ("p/docs/x/requirements-dev.txt", Some("dev")),
            ("p/docs/tests/requirements.txt", Some("test")),
            // Exact names only (FR-005).
            ("p/examples/a/requirements.txt", None),
            ("p/benchmarks/b/requirements.txt", None),
            ("p/tools/c/requirements.txt", None),
            ("p/test_data/d/requirements.txt", None),
            ("p/documents/e/requirements.txt", None),
            ("p/testing_utils/f/requirements.txt", None),
            ("p/doctor/g/requirements.txt", None),
            ("p/x/requirements.txt", None),
        ] {
            assert_eq!(classify_requirements_scope_under(&root.join(rel), root).as_optional_name(), want, "{rel}");
        }
    }

    /// Spec 1615 FR-004 and research R3: the scan root bounds the search,
    /// and a path outside the root gets the dot-directory search only.
    #[test]
    fn s1615_the_scan_root_bounds_the_search() {
        let scope = |p: &str, root: &str| classify_requirements_scope_under(Path::new(p), Path::new(root)).as_optional_name();
        assert_eq!(scope("/w/tests/p/x/requirements.txt", "/w/tests"), None, "the root's own name");
        assert_eq!(scope("/w/docs/p/x/requirements.txt", "/w/docs/p"), None, "an ancestor of the root");
        assert_eq!(scope("/w/docs/p/x/requirements.txt", "/w"), Some("docs"));
        assert_eq!(scope("/home/tests/p/x/requirements.txt", "/elsewhere"), None, "outside the root: no docs/test names");
        assert_eq!(scope("/home/.ci/p/x/requirements.txt", "/elsewhere"), Some("ci"), "outside the root: dot-directories as before");
    }
}
