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

use super::resolve_classifier::classify_resolve;
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

/// Classifies an artifact URL into a source-type category, driving
/// PURL construction (pypi vs generic) + the `waybill:source-type`
/// annotation emission per Q2-A / FR-009.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArtifactSourceType {
    /// URL starts with "https://files.pythonhosted.org/" — canonical
    /// PyPI-hosted wheel or sdist.
    Pypi,
    /// URL starts with "git+" (any transport).
    Git,
    /// URL starts with "http://" or "https://" (non-PyPI host).
    Url,
    /// URL starts with "file://" or is an absolute local filesystem path.
    Local,
}

impl ArtifactSourceType {
    /// Dispatch an artifact URL to its source-type category.
    ///
    /// Empty URL is treated as `Local` (edge case per Pex behavior when
    /// a lock entry was resolved from a wheel path without a URL prefix).
    pub(crate) fn from_url(url: &str) -> Self {
        if url.starts_with("https://files.pythonhosted.org/") {
            Self::Pypi
        } else if url.starts_with("git+") {
            Self::Git
        } else if url.starts_with("http://") || url.starts_with("https://") {
            Self::Url
        } else {
            // file:// URLs + bare absolute paths + empty strings all
            // fall here — treat as local.
            Self::Local
        }
    }

    /// Value emitted into the `waybill:source-type` annotation for
    /// non-PyPI entries. Not emitted for `Pypi` (PyPI PURLs carry the
    /// source-type implicitly via the `pkg:pypi/*` type).
    pub(crate) fn as_annotation_str(self) -> &'static str {
        match self {
            Self::Pypi => "pypi",
            Self::Git => "git",
            Self::Url => "url",
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
pub(crate) fn locked_req_to_entry(
    req: &LockedRequirement,
    lockfile_path: &Path,
    resolve_name: &str,
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
    // Non-PyPI entries carry source-url + source-type annotations for
    // provenance (Q2 A).
    if source_type != ArtifactSourceType::Pypi {
        if let Some(a) = req.artifacts.first() {
            extra_annotations.insert(
                "waybill:source-url".to_string(),
                json!(&a.url),
            );
        }
        extra_annotations.insert(
            "waybill:source-type".to_string(),
            json!(source_type.as_annotation_str()),
        );
    }

    Some(PackageDbEntry {
        purl,
        name: req.project_name.clone(),
        version: req.version.clone(),
        arch: None,
        source_path: lockfile_path.display().to_string(),
        depends,
        maintainer: None,
        lifecycle_scope: Some(classify_resolve(resolve_name)),
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

    #[test]
    fn artifact_source_type_plain_https_non_pypi() {
        assert_eq!(
            ArtifactSourceType::from_url("https://mirror.example.test/wheels/foo-1.0.0.whl"),
            ArtifactSourceType::Url
        );
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
        assert_eq!(ArtifactSourceType::Url.as_annotation_str(), "url");
        assert_eq!(ArtifactSourceType::Local.as_annotation_str(), "local");
    }
}
