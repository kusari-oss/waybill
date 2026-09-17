//! Milestone 223: Pex lockfile JSON parser + LockedRequirement →
//! PackageDbEntry mapping.
//!
//! Schema shape verified empirically 2026-07-31 against Pants's own
//! dogfood lockfile at `github.com/pantsbuild/pants @ HEAD` file
//! `3rdparty/python/user_reqs.lock`. See
//! `specs/223-pants-pex-reader/research.md` §R1 for the full schema
//! plus extraction rules, and
//! `specs/223-pants-pex-reader/contracts/pex-lockfile-schema.md`
//! for the fail-open behavior boundaries.
//!
//! Milestone 672 extension: `strip_pants_frontmatter` recovers the
//! JSON body from Pants ≤ 2.29 lockfile files that prepend a
//! `//`-comment metadata block. See
//! `specs/672-pants-reader-follow-up/contracts/front_matter_stripper.md`
//! for the full behavioral contract (C1–C7) and
//! `specs/672-pants-reader-follow-up/research.md` §R2 for the
//! algorithm rationale.

use std::path::Path;

use serde::Deserialize;
use serde_json::json;
use waybill_common::types::hash::ContentHash;
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::resolve_classifier::classify_resolve_with_source;
use crate::scan_fs::package_db::pip::normalize_pypi_name_for_purl;
use crate::scan_fs::package_db::PackageDbEntry;

/// Top-level Pex lockfile shape. Only fields waybill consumes are
/// declared; unknown fields are ignored via serde's default behavior
/// (no `#[serde(deny_unknown_fields)]` — Pex adds top-level fields
/// regularly and we do not want format-evolution breakage).
#[derive(Debug, Deserialize)]
pub(crate) struct PexLockfile {
    /// Pex format version (e.g., "2.10.0"). Compatibility guard:
    /// `^2\.` accepted, anything else → the parser returns None + WARN.
    pub(crate) pex_version: String,
    /// One or more locked resolves. Typically len == 1; multi-platform
    /// locks can be > 1. Every resolve's `locked_requirements` are
    /// unioned into the emitted component list.
    pub(crate) locked_resolves: Vec<LockedResolve>,
    /// Milestone 868 (#887): the resolve's DECLARED top-level
    /// requirements, as PEP 508 strings (`"Jinja2~=3.1.6"`).
    ///
    /// This is what the resolve was asked to provide, stated by the
    /// lockfile rather than derived from graph shape. It is deliberately
    /// preferred over "packages nothing else in the resolve depends on".
    ///
    /// Measured across the nine resolves on the target (see
    /// `specs/868-resolve-ownership/measurements/`): 149 declared against
    /// 113 derived, and the derived figure falls to 80 depending only on
    /// whether `extra == "..."` markers are treated as real edges. A
    /// requested package can also be a transitive dependency of another
    /// requested package, which makes it a non-root while still being
    /// something the resolve was explicitly asked for.
    ///
    /// `tools/setuptools.lock` is the sharp case: `setuptools` and `wheel`
    /// each appear in the other's `requires_dists` behind extras markers,
    /// so deriving naively yields ZERO roots and the whole resolve stays
    /// unreachable. Reading the declaration needs no marker-evaluation
    /// policy at all, which is what contract A-2 asks for.
    #[serde(default)]
    pub(crate) requirements: Vec<String>,
}

/// One resolve block inside `locked_resolves`. Ignored fields:
/// `marker` + `platform_tag` (v1 doesn't reason about per-marker or
/// per-platform variants — see research.md §R1 alternatives-rejected).
#[derive(Debug, Deserialize)]
pub(crate) struct LockedResolve {
    #[serde(default)]
    pub(crate) locked_requirements: Vec<LockedRequirement>,
}

/// One locked distribution. Maps 1:1 to an emitted `PackageDbEntry`
/// in the happy path (per data-model.md §"PackageDbEntry field
/// mapping"). Validation rules per contracts/pex-lockfile-schema.md
/// §"Fail-open behavior boundaries" applied at conversion time.
#[derive(Debug, Deserialize)]
pub(crate) struct LockedRequirement {
    /// PyPI-canonicalized package name. Pex normalizes at generate
    /// time; we re-normalize per R3 for safety.
    pub(crate) project_name: String,
    /// Pinned version string. Empty → skip this entry with WARN
    /// (unpinned entries are FR-002-noncompliant).
    pub(crate) version: String,
    /// PEP 508 requirement strings for inter-package dependencies.
    /// Feeds `PackageDbEntry.depends` after project-name extraction.
    #[serde(default)]
    pub(crate) requires_dists: Vec<String>,
    /// Python version constraint (e.g. ">=3.8"). Recorded as
    /// `waybill:requires-python` annotation for downstream tooling.
    #[serde(default)]
    pub(crate) requires_python: Option<String>,
    /// One or more artifacts (typically wheel + sdist). Feeds
    /// `PackageDbEntry.hashes`. First artifact's URL drives the
    /// PyPI-vs-generic PURL type dispatch per R1.
    #[serde(default)]
    pub(crate) artifacts: Vec<Artifact>,
}

/// One downloadable artifact reference.
#[derive(Debug, Deserialize)]
pub(crate) struct Artifact {
    /// Always "sha256" in Pex 2.x. Recorded verbatim into
    /// `PackageDbEntry.hashes[].algorithm`.
    pub(crate) algorithm: String,
    /// Hex-encoded hash. Recorded into `PackageDbEntry.hashes[].value`.
    pub(crate) hash: String,
    /// Fetch URL. Prefix drives PURL type via `ArtifactSourceType::from_url`.
    pub(crate) url: String,
}

/// The canonical PyPI artifact host. Used only to decide whether an
/// artifact URL is worth recording (see `emits_source_url`), never to
/// decide whether something IS a PyPI distribution — that was the m901
/// defect.
const CANONICAL_PYPI_HOST: &str = "https://files.pythonhosted.org/";

/// Classifies an artifact URL into a source-type category, driving
/// PURL construction (pypi vs generic) + the `waybill:source-type`
/// annotation emission per Q2-A / FR-009.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArtifactSourceType {
    /// Resolved from a package index over http(s) — **any** index, not
    /// only `files.pythonhosted.org`.
    ///
    /// Issue #901: this used to require the canonical host, so every
    /// requirement served by a private index or mirror was demoted to
    /// `pkg:generic` and became invisible to advisory matching. On one
    /// real monorepo that was 1054 of 1826 components.
    ///
    /// The rule now matches `pip/uv_lock.rs`, which types by source KIND
    /// and never inspects a registry's host — so a uv project behind the
    /// same index was already correct while this reader was not.
    ///
    /// What it assumes: a matching name and version means the upstream
    /// project. That assumption can be wrong for an internally-built
    /// package whose name collides with a real one; #909 tracks it, and
    /// the artifact URL is retained on such components so the question
    /// stays answerable from the emitted document.
    Pypi,
    /// URL starts with "git+" (any transport). A VCS checkout is not a
    /// distribution of the named project, so it stays `pkg:generic`.
    Git,
    /// URL starts with "file://" or is an absolute local filesystem path.
    Local,
}

impl ArtifactSourceType {
    /// Dispatch an artifact URL to its source-type category.
    ///
    /// Empty URL is treated as `Local` (edge case per Pex behavior when
    /// a lock entry was resolved from a wheel path without a URL prefix).
    pub(crate) fn from_url(url: &str) -> Self {
        if url.starts_with("git+") {
            Self::Git
        } else if url.starts_with("http://") || url.starts_with("https://") {
            // Any index, canonical or private. Order matters: `git+https://`
            // is checked first, or a VCS checkout would land here.
            Self::Pypi
        } else {
            // file:// URLs + bare absolute paths + empty strings all
            // fall here — treat as local.
            Self::Local
        }
    }

    /// Whether this entry should carry `waybill:source-url`.
    ///
    /// Not "is it generic" — that was the pre-#901 gate, and it would now
    /// drop the URL from exactly the components whose provenance is
    /// non-obvious. The question is whether the URL tells a reader
    /// something the PURL does not: for a canonical PyPI wheel it does
    /// not, for anything else it does.
    ///
    /// Presence of `waybill:source-url` on a `pkg:pypi/*` component is
    /// therefore the marker for "index-resolved, non-canonical host" —
    /// the population #909 needs to reason about, at no extra machinery.
    pub(crate) fn emits_source_url(self, url: &str) -> bool {
        !matches!(self, Self::Pypi) || !url.starts_with(CANONICAL_PYPI_HOST)
    }

    /// Value emitted into the `waybill:source-type` annotation for
    /// non-PyPI entries. Not emitted for `Pypi` (PyPI PURLs carry the
    /// source-type implicitly via the `pkg:pypi/*` type).
    pub(crate) fn as_annotation_str(self) -> &'static str {
        match self {
            Self::Pypi => "pypi",
            Self::Git => "git",
            Self::Local => "local",
        }
    }
}

/// Parse Pex lockfile bytes. Returns `None` on any error per the
/// fail-open contract (contracts/pex-lockfile-schema.md §"Fail-open
/// behavior boundaries"). Rejects `pex_version` not matching `^2\.`.
///
/// The caller is responsible for adding the source path context to
/// any WARN log (this function doesn't know the file's path).
/// Milestone 672: strip a leading `//`-comment metadata block (Pants
/// ≤ 2.29 lockfile shape) from `bytes`. Returns the slice starting at
/// the first non-`//` non-whitespace line, or `&[]` if the entire
/// input was `//`-commented.
///
/// This is a pure function — no allocation, no error path, no
/// persistent state. Callers pass its output directly to
/// `serde_json::from_slice`.
///
/// Complexity: O(prefix-length) — the loop bails out at the first
/// non-`//` line. On clean-JSON input (first non-whitespace byte is
/// `{`), the function returns after examining a single line
/// (contract C3 + C4 idempotence).
///
/// See `specs/672-pants-reader-follow-up/contracts/front_matter_stripper.md`
/// for the full behavioral contract.
pub(crate) fn strip_pants_frontmatter(bytes: &[u8]) -> &[u8] {
    let mut pos = 0;
    loop {
        // Save the start of this line; we may need to return it as
        // the body if it isn't a `//` line.
        let line_start = pos;
        // Skip leading whitespace within this line (space + tab
        // only per contract C1; other whitespace treated as
        // non-`//` and terminates the strip loop).
        while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t') {
            pos += 1;
        }
        // Check for `//`. If EOF or the next 2 bytes aren't `//`,
        // this line is the JSON body's start — return from
        // `line_start` (contract C1: whitespace before the JSON body
        // is preserved so operators see byte-identity for files that
        // don't need stripping).
        if pos + 1 >= bytes.len() || &bytes[pos..pos + 2] != b"//" {
            return &bytes[line_start..];
        }
        // This line is a `//` comment. Advance to just past the next
        // `\n` (contract C2 — content is opaque; we don't interpret
        // the metadata).
        match bytes[pos..].iter().position(|&b| b == b'\n') {
            Some(offset) => pos += offset + 1,
            // No trailing newline → entire remaining input is a `//`
            // comment (contract C6). Return empty slice; downstream
            // `serde_json::from_slice(&[])` fails with standard EOF
            // error → m223 fail-open WARN + skip.
            None => return &[],
        }
    }
}

/// Milestone 673 FR-003: content-detect a `.lock` file as a valid
/// PEX lockfile by checking for `pex_version: "2.x"` at the JSON
/// top level. Used as the wide-scope FR-001/FR-002 discovery gate
/// per `specs/673-pants-lockfile-layouts/contracts/content_detection.md`.
///
/// Steps:
/// 1. Strip `//`-frontmatter (m672 `strip_pants_frontmatter`).
/// 2. Parse as `serde_json::Value` (permissive top-level JSON).
/// 3. Return `obj["pex_version"].as_str().is_some_and(|s| s.starts_with("2."))`.
///
/// Returns `false` on any parse failure or missing / wrong-version
/// field — caller silent-skips per m673 FR-004.
///
/// Pure function — no allocation beyond the parse buffer, no error
/// path (returns `bool`), no persistent state.
///
/// Complexity: O(file-size) linear parse. Sub-millisecond on non-JSON
/// rejects (parse errors early); < 5 ms on real PEX shapes.
pub(crate) fn is_pex_lockfile_content(bytes: &[u8]) -> bool {
    let body = strip_pants_frontmatter(bytes);
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return false;
    };
    value
        .get("pex_version")
        .and_then(|v| v.as_str())
        .is_some_and(|s| s.starts_with("2."))
}

/// Parse a Pex lockfile from raw file bytes.
///
/// Returns `Some((lockfile, was_legacy_shape))` on success. The
/// `was_legacy_shape` flag is `true` when the m672 front-matter
/// stripper consumed at least one leading `//` line (Pants ≤ 2.29
/// legacy shape) — callers thread this into `LegacyShapeCounter` to
/// feed the FR-013 log field.
///
/// Milestone 672 (clarify Q3, uniform-strip): every parse attempt
/// routes through `strip_pants_frontmatter` before handing bytes to
/// `serde_json`. On clean-JSON files the stripper is a no-op
/// (contract C4 idempotence) — the byte slice's start address is
/// unchanged.
pub(crate) fn parse(bytes: &[u8]) -> Option<(PexLockfile, bool)> {
    let body = strip_pants_frontmatter(bytes);
    let was_legacy_shape = body.len() < bytes.len();
    let lock: PexLockfile = serde_json::from_slice(body)
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                "pants-pex reader: failed to parse Pex lockfile as JSON; skipping"
            );
        })
        .ok()?;
    if !lock.pex_version.starts_with("2.") {
        tracing::warn!(
            pex_version = %lock.pex_version,
            "pants-pex reader: unsupported Pex lockfile version (expected 2.x); skipping"
        );
        return None;
    }
    Some((lock, was_legacy_shape))
}

/// Extract the project name from a PEP 508 requirement string.
/// Strips version specifiers, extras, and environment markers.
///
/// Examples:
/// - `"requests"` → `"requests"`
/// - `"requests>=2.0.0"` → `"requests"`
/// - `"typing-extensions>=4.0.0; python_version < \"3.9\""` → `"typing-extensions"`
/// - `"waybill-fixture[extra1,extra2]==1.0"` → `"waybill-fixture"`
fn extract_pep508_project_name(req: &str) -> String {
    let end = req
        .find(['<', '>', '=', '~', '!', '[', ';', '('])
        .unwrap_or(req.len());
    req[..end].trim().to_string()
}

/// Convert a `LockedRequirement` from a Pex lockfile into a
/// `PackageDbEntry` suitable for the m191 reconciler + emit pipeline.
///
/// Returns `None` + WARN log on validation failures per
/// contracts/pex-lockfile-schema.md §"Output contract":
/// - empty `project_name`
/// - empty `version`
/// - PURL construction failure
///
/// Field mapping matches data-model.md §"PackageDbEntry field mapping".
/// Milestone 868 (#887) — build the component that OWNS a resolve.
///
/// A resolve's dependency graph is read correctly today and connected to
/// nothing: on the measured target 760 well-formed edges sit in a document
/// where 1 of 331 components is reachable from the root. Nothing in the model
/// said which component a resolve belongs to. This is that component.
///
/// It is **not a package**. It has no upstream, nothing to fetch, and no
/// vulnerability surface of its own — a consumer treating it as a package
/// would try to resolve it against an index and fail. The `pkg:generic/`
/// type alone does not convey that (real fetchable things use it too), so
/// the nature is carried explicitly per FR-002a.
///
/// `depends` is the lockfile's DECLARED `requirements`, not the packages
/// nothing else depends on. See the field doc on `PexLockfile::requirements`.
pub(crate) fn resolve_component_entry(
    lock: &PexLockfile,
    lockfile_path: &Path,
    resolve_name: &str,
    declared_by_tool: bool,
) -> Option<PackageDbEntry> {
    if resolve_name.trim().is_empty() {
        return None;
    }
    let purl = Purl::new(&format!(
        "pkg:generic/{}",
        encode_purl_segment(resolve_name)
    ))
    .ok()?;

    // One classification, used for both the scope and the recorded evidence
    // strength, so the two cannot disagree about the same resolve.
    let classification = classify_resolve_with_source(resolve_name, declared_by_tool);

    // PEP 508 strings -> distribution names. `Jinja2~=3.1.6` -> `jinja2`,
    // `SQLAlchemy[postgresql_asyncpg]~=1.4.54` -> `sqlalchemy`.
    let mut depends: Vec<String> = Vec::new();
    for raw in &lock.requirements {
        let name: String = raw
            .trim()
            .chars()
            .take_while(|c| !matches!(c, ' ' | '\t' | '[' | '(' | '<' | '>' | '=' | '!' | '~' | ';' | ','))
            .collect();
        if name.is_empty() {
            continue;
        }
        depends.push(normalize_pypi_name_for_purl(&name));
    }
    depends.sort();
    depends.dedup();

    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "waybill:component-kind".to_string(),
        json!("lockfile-resolve"),
    );
    extra_annotations.insert("waybill:pants-resolve".to_string(), json!(resolve_name));
    // FR-003c: whether this classification rests on a declaration or on the
    // weaker name heuristic. Recorded per component so the aggregate count is
    // reconstructible from the document rather than only asserted by it.
    extra_annotations.insert(
        "waybill:resolve-classification-source".to_string(),
        json!(classification.1.as_wire_str()),
    );

    Some(PackageDbEntry {
        depends_ecosystem: Some("pypi".to_string()),
        build_inclusion: None,
        purl,
        name: resolve_name.to_string(),
        version: String::new(),
        arch: None,
        source_path: lockfile_path.display().to_string(),
        depends,
        maintainer: None,
        licenses: Vec::new(),
        // A resolve a tool declares is build-time; otherwise runtime, which
        // over-reports rather than hiding (FR-003b).
        lifecycle_scope: Some(classification.0),
        requirement_ranges: Vec::new(),
        source_type: None,
        buildinfo_status: None,
        evidence_kind: None,
        sbom_tier: Some("source".to_string()),
        extra_annotations,
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
        // A resolve is a declared grouping, not a distribution: there is no
        // artifact to hash.
        hashes: Vec::new(),
        shade_relocation: None,
        binary_role: None,
    })
}

pub(crate) fn locked_req_to_entry(
    req: &LockedRequirement,
    lockfile_path: &Path,
    resolve_name: &str,
    // Milestone 868 (#887) — whether a `pants.toml` tool section declares
    // this resolve via `install_from_resolve`. The declaration decides the
    // lifecycle; the name allowlist is the fallback where nothing declares
    // (FR-003a, contract A-4).
    declared_by_tool: bool,
) -> Option<PackageDbEntry> {
    if req.project_name.trim().is_empty() {
        tracing::warn!(
            lockfile = %lockfile_path.display(),
            resolve = %resolve_name,
            "pants-pex reader: locked entry has empty project_name; skipping"
        );
        return None;
    }
    if req.version.trim().is_empty() {
        tracing::warn!(
            lockfile = %lockfile_path.display(),
            resolve = %resolve_name,
            project_name = %req.project_name,
            "pants-pex reader: locked entry has empty version; skipping"
        );
        return None;
    }

    let normalized_name = normalize_pypi_name_for_purl(&req.project_name);

    // Dispatch on first artifact's URL: if any artifact is non-PyPI,
    // the whole entry is treated as pkg:generic/* (PyPI wheels can't
    // be mixed with non-PyPI sources in one lock entry per R1).
    let source_type = req
        .artifacts
        .first()
        .map(|a| ArtifactSourceType::from_url(&a.url))
        .unwrap_or(ArtifactSourceType::Local); // no artifacts → treat as local

    let purl_str = match source_type {
        ArtifactSourceType::Pypi => format!(
            "pkg:pypi/{}@{}",
            encode_purl_segment(&normalized_name),
            encode_purl_segment(&req.version),
        ),
        _ => format!(
            "pkg:generic/{}@{}",
            encode_purl_segment(&normalized_name),
            encode_purl_segment(&req.version),
        ),
    };
    let purl = Purl::new(&purl_str)
        .map_err(|e| {
            tracing::warn!(
                lockfile = %lockfile_path.display(),
                resolve = %resolve_name,
                project_name = %req.project_name,
                purl_str = %purl_str,
                error = %e,
                "pants-pex reader: PURL construction failed; skipping entry"
            );
        })
        .ok()?;

    // Extract dependency edges from PEP 508 requires_dists.
    let depends: Vec<String> = req
        .requires_dists
        .iter()
        .map(|r| extract_pep508_project_name(r))
        .filter(|n| !n.is_empty())
        .map(|n| normalize_pypi_name_for_purl(&n))
        .collect();

    // Emit one ContentHash per artifact. Non-sha256 algorithms are
    // future-proofed via the algorithm's own from-string path — Pex 2.x
    // always uses sha256, but we don't hardcode that.
    let hashes: Vec<ContentHash> = req
        .artifacts
        .iter()
        .filter_map(|a| {
            if a.algorithm == "sha256" {
                ContentHash::sha256(&a.hash).ok()
            } else {
                None
            }
        })
        .collect();

    // Build the annotation bag.
    let mut extra_annotations = std::collections::BTreeMap::new();
    extra_annotations.insert(
        "waybill:pants-resolve".to_string(),
        json!(resolve_name),
    );
    if let Some(rp) = &req.requires_python {
        if !rp.is_empty() {
            extra_annotations.insert(
                "waybill:requires-python".to_string(),
                json!(rp),
            );
        }
    }
    // Provenance annotations (Q2 A, amended by #901).
    //
    // `source-url` is now gated on whether the URL is informative rather
    // than on the PURL type: a private-index wheel is `pkg:pypi` after
    // #901 but its host is still worth recording, and dropping it would
    // make #909's collision question unanswerable from the document.
    //
    // `source-type` stays gated on the type, because for a `pkg:pypi`
    // component it would only ever restate what the PURL already says.
    if let Some(a) = req.artifacts.first() {
        if source_type.emits_source_url(&a.url) {
            extra_annotations.insert(
                "waybill:source-url".to_string(),
                json!(&a.url),
            );
        }
    }
    if source_type != ArtifactSourceType::Pypi {
        extra_annotations.insert(
            "waybill:source-type".to_string(),
            json!(source_type.as_annotation_str()),
        );
    }

    Some(PackageDbEntry {
        depends_ecosystem: None,
        purl,
        name: req.project_name.clone(),
        version: req.version.clone(),
        arch: None,
        source_path: lockfile_path.display().to_string(),
        depends,
        maintainer: None,
        lifecycle_scope: Some(classify_resolve_with_source(resolve_name, declared_by_tool).0),
        requirement_ranges: Vec::new(),
        source_type: None, // Distinct from ArtifactSourceType — this
                           // field is for pip/npm source-URL kinds, not
                           // for our Pex classification.
        licenses: Vec::new(), // Pex format doesn't carry licenses.
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
        extra_annotations,
        binary_role: None,
        build_inclusion: None,
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // Milestone 672 T004 (US1): `strip_pants_frontmatter` unit tests.
    // Covers the 9-row test matrix at
    // `specs/672-pants-reader-follow-up/contracts/front_matter_stripper.md`.
    // -----------------------------------------------------------------

    #[test]
    fn strip_clean_json_is_idempotent_noop() {
        // Contract C4: on clean-JSON input the returned slice must
        // begin at the same byte address as the input.
        let bytes: &[u8] = br#"{"pex_version":"2.10.0"}"#;
        let out = strip_pants_frontmatter(bytes);
        assert_eq!(out, bytes);
        // Same start address guarantees zero-copy on the happy path.
        assert_eq!(out.as_ptr(), bytes.as_ptr());
    }

    #[test]
    fn strip_single_leading_comment_line() {
        let bytes: &[u8] = b"// header\n{\"pex_version\":\"2.10.0\"}";
        let out = strip_pants_frontmatter(bytes);
        assert_eq!(out, br#"{"pex_version":"2.10.0"}"#);
    }

    #[test]
    fn strip_indented_leading_comment() {
        let bytes: &[u8] = b"  // indent\n{\"pex_version\":\"2.10.0\"}";
        let out = strip_pants_frontmatter(bytes);
        assert_eq!(out, br#"{"pex_version":"2.10.0"}"#);
    }

    #[test]
    fn strip_tab_prefixed_leading_comment() {
        let bytes: &[u8] = b"\t\t// tabbed\n{\"pex_version\":\"2.10.0\"}";
        let out = strip_pants_frontmatter(bytes);
        assert_eq!(out, br#"{"pex_version":"2.10.0"}"#);
    }

    #[test]
    fn strip_multi_line_comment_block() {
        let bytes: &[u8] = b"// a\n// b\n// c\n{\"pex_version\":\"2.10.0\"}";
        let out = strip_pants_frontmatter(bytes);
        assert_eq!(out, br#"{"pex_version":"2.10.0"}"#);
    }

    #[test]
    fn strip_fully_commented_no_trailing_newline_returns_empty() {
        // Contract C6: fully-commented input with no trailing
        // newline returns `&[]`.
        let bytes: &[u8] = b"// only comments";
        let out = strip_pants_frontmatter(bytes);
        assert!(out.is_empty());
    }

    #[test]
    fn strip_fully_commented_with_trailing_newline_returns_empty() {
        // Contract C6: fully-commented input with a trailing newline
        // (but no JSON body) returns `&[]`.
        let bytes: &[u8] = b"// hdr\n// hdr\n";
        let out = strip_pants_frontmatter(bytes);
        assert!(out.is_empty());
    }

    #[test]
    fn strip_blank_line_terminates_loop() {
        // A leading blank line is NOT a `//` comment — the loop
        // bails at the first line and preserves the blank lines
        // (implementation detail: whitespace-only lines are treated
        // as non-`//` and terminate the strip).
        let bytes: &[u8] = b"\n\n{\"pex_version\":\"2.10.0\"}";
        let out = strip_pants_frontmatter(bytes);
        assert_eq!(out, b"\n\n{\"pex_version\":\"2.10.0\"}");
    }

    #[test]
    fn strip_preserves_embedded_slash_slash_in_string() {
        // Contract C7: `//` bytes that appear AFTER the first non-
        // `//` line are untouched. The stripper is bounded to the
        // leading prefix only.
        let bytes: &[u8] =
            br#"{"foo": "// this is inside a JSON string; must survive"}"#;
        let out = strip_pants_frontmatter(bytes);
        assert_eq!(out, bytes);
    }

    #[test]
    fn strip_realistic_pants_frontmatter() {
        // Real-world happy path (early-adopter shape observed
        // 2026-09-01, per research.md §R1).
        let bytes: &[u8] = b"\
// This lockfile was autogenerated by Pants. To regenerate, run:
//
//    ./pants generate-lockfiles --resolve=python-default
//
// --- BEGIN PANTS LOCKFILE METADATA: DO NOT EDIT OR REMOVE ---
// {
//   \"version\": 3,
//   \"valid_for_interpreter_constraints\": []
// }
// --- END PANTS LOCKFILE METADATA ---
{
  \"allow_builds\": true,
  \"pex_version\": \"2.10.0\"
}
";
        let out = strip_pants_frontmatter(bytes);
        assert!(
            out.starts_with(b"{\n  \"allow_builds\": true"),
            "expected JSON body to start after the metadata block, got: {:?}",
            std::str::from_utf8(&out[..out.len().min(60)]).unwrap_or("<non-utf8>")
        );
    }

    // -----------------------------------------------------------------
    // Milestone 673 T003 (US1/US2/US3): `is_pex_lockfile_content` unit
    // tests. Covers the 15-row test matrix at
    // `specs/673-pants-lockfile-layouts/contracts/content_detection.md`.
    // -----------------------------------------------------------------

    #[test]
    fn is_pex_content_accepts_clean_pex_2x() {
        // Contract C1: accept `{"pex_version": "2.10.0", ...}`.
        let bytes = br#"{"pex_version":"2.10.0","locked_resolves":[]}"#;
        assert!(is_pex_lockfile_content(bytes));
    }

    #[test]
    fn is_pex_content_accepts_pex_2x_with_slash_slash_frontmatter() {
        // Contract C1 + C6: `//`-frontmatter stripping applies before
        // content-detection.
        let bytes = b"// header\n{\"pex_version\":\"2.10.0\"}";
        assert!(is_pex_lockfile_content(bytes));
    }

    #[test]
    fn is_pex_content_accepts_pex_20_prerelease() {
        // Contract C1: any string starting with `2.` accepts.
        let bytes = br#"{"pex_version":"2.0.0-rc.1"}"#;
        assert!(is_pex_lockfile_content(bytes));
    }

    #[test]
    fn is_pex_content_rejects_pex_1x() {
        // Contract C2: Pex 1.x is out of scope (m223 accept-criterion).
        let bytes = br#"{"pex_version":"1.9.0"}"#;
        assert!(!is_pex_lockfile_content(bytes));
    }

    #[test]
    fn is_pex_content_rejects_hypothetical_pex_3x() {
        // Contract C2: prefix-match `^2\.` only.
        let bytes = br#"{"pex_version":"3.0.0"}"#;
        assert!(!is_pex_lockfile_content(bytes));
    }

    #[test]
    fn is_pex_content_rejects_cargo_lockfile_toml_shape() {
        // Contract C3: non-JSON reject.
        let bytes = b"version = 3\n[[package]]\nname = \"foo\"\nversion = \"1.0.0\"\n";
        assert!(!is_pex_lockfile_content(bytes));
    }

    #[test]
    fn is_pex_content_rejects_poetry_lockfile_toml_shape() {
        // Contract C3: non-JSON reject.
        let bytes = b"[metadata]\nlock-version = \"2.0\"\npython-versions = \"^3.10\"\n";
        assert!(!is_pex_lockfile_content(bytes));
    }

    #[test]
    fn is_pex_content_rejects_bun_lock_jsonc_shape() {
        // Contract C3: bun.lock uses JSONC with embedded `//` comments
        // AFTER the first byte (so the m672 stripper doesn't help).
        // `serde_json` rejects the comments as invalid JSON.
        let bytes = b"{\n  \"lockfileVersion\": 1,\n  // bun uses JSONC\n  \"workspaces\": {}\n}";
        assert!(!is_pex_lockfile_content(bytes));
    }

    #[test]
    fn is_pex_content_rejects_empty_file() {
        // Contract C4: empty input rejects (empty JSON parse fails).
        assert!(!is_pex_lockfile_content(&[]));
    }

    #[test]
    fn is_pex_content_rejects_empty_object() {
        // Contract C5: valid JSON without `pex_version` field.
        let bytes = b"{}";
        assert!(!is_pex_lockfile_content(bytes));
    }

    #[test]
    fn is_pex_content_rejects_integer_pex_version() {
        // Contract C5: `pex_version` must be a string, not integer.
        let bytes = br#"{"pex_version":2}"#;
        assert!(!is_pex_lockfile_content(bytes));
    }

    #[test]
    fn is_pex_content_rejects_null_pex_version() {
        // Contract C5: `pex_version` must be a string, not null.
        let bytes = br#"{"pex_version":null}"#;
        assert!(!is_pex_lockfile_content(bytes));
    }

    #[test]
    fn is_pex_content_rejects_top_level_array() {
        // Contract C5: top-level must be an object.
        // `.get("pex_version")` on a Value::Array returns None.
        let bytes = br#"["pex_version","2.10.0"]"#;
        assert!(!is_pex_lockfile_content(bytes));
    }

    #[test]
    fn is_pex_content_rejects_unterminated_json() {
        // Contract C3: partial JSON that fails to parse.
        let bytes = br#"{"pex_version":"2.10.0","corrupted":"#;
        assert!(!is_pex_lockfile_content(bytes));
    }

    #[test]
    fn is_pex_content_rejects_binary_garbage() {
        // Contract C3: non-UTF-8 binary content.
        let bytes = &[0xff, 0xfe, 0xfd, 0xfc, 0xfb][..];
        assert!(!is_pex_lockfile_content(bytes));
    }

    #[test]
    fn is_pex_content_rejects_fully_commented_input() {
        // Contract C4 + m672 stripper C6: after `//`-stripping the
        // slice is empty, which fails the JSON parse (empty-input).
        let bytes = b"// only comments\n// no json body\n";
        assert!(!is_pex_lockfile_content(bytes));
    }

    #[test]
    fn artifact_source_type_pypi_url() {
        assert_eq!(
            ArtifactSourceType::from_url(
                "https://files.pythonhosted.org/packages/xx/foo-1.0.0-py3-none-any.whl"
            ),
            ArtifactSourceType::Pypi
        );
    }

    #[test]
    fn artifact_source_type_git_url() {
        assert_eq!(
            ArtifactSourceType::from_url("git+https://github.com/example/repo.git@abc123"),
            ArtifactSourceType::Git
        );
    }

    /// #901 — an http(s) artifact is index-resolved and types as PyPI
    /// whichever host served it. This used to assert `Url`, which is the
    /// defect: every private-index requirement was demoted to pkg:generic
    /// and became invisible to advisory matching.
    #[test]
    fn artifact_source_type_private_index_is_pypi() {
        assert_eq!(
            ArtifactSourceType::from_url("https://mirror.example.test/wheels/foo-1.0.0.whl"),
            ArtifactSourceType::Pypi
        );
    }

    /// Ordering guard. `git+https://` starts with neither `http://` nor
    /// `https://`, but a careless reordering of the branches would make it
    /// match and turn every VCS checkout into a claimed PyPI release.
    #[test]
    fn artifact_source_type_git_over_https_is_not_pypi() {
        assert_eq!(
            ArtifactSourceType::from_url("git+https://example.test/org/repo.git@abc123"),
            ArtifactSourceType::Git
        );
    }

    /// #901 — the artifact URL is recorded exactly when it says something
    /// the PURL does not. Presence on a pkg:pypi component is what marks it
    /// as index-resolved from a non-canonical host (#909 depends on this).
    #[test]
    fn source_url_emitted_only_when_informative() {
        let canonical = "https://files.pythonhosted.org/packages/xx/foo-1.0.0-py3-none-any.whl";
        let private = "https://mirror.example.test/wheels/foo-1.0.0-py3-none-any.whl";
        assert!(
            !ArtifactSourceType::Pypi.emits_source_url(canonical),
            "a canonical PyPI wheel's URL restates the PURL"
        );
        assert!(
            ArtifactSourceType::Pypi.emits_source_url(private),
            "a private-index wheel's host is the only record of its provenance"
        );
        assert!(ArtifactSourceType::Git.emits_source_url("git+https://example.test/r.git"));
        assert!(ArtifactSourceType::Local.emits_source_url("file:///opt/wheels/foo.whl"));
    }

    #[test]
    fn artifact_source_type_file_url() {
        assert_eq!(
            ArtifactSourceType::from_url("file:///opt/wheels/foo-1.0.0.whl"),
            ArtifactSourceType::Local
        );
    }

    #[test]
    fn artifact_source_type_absolute_local_path() {
        assert_eq!(
            ArtifactSourceType::from_url("/opt/wheels/foo-1.0.0.whl"),
            ArtifactSourceType::Local
        );
    }

    #[test]
    fn artifact_source_type_annotation_strings() {
        assert_eq!(ArtifactSourceType::Pypi.as_annotation_str(), "pypi");
        assert_eq!(ArtifactSourceType::Git.as_annotation_str(), "git");
        assert_eq!(ArtifactSourceType::Local.as_annotation_str(), "local");
    }
}
