//! Swift Package Manager `Package.resolved` lockfile parser.
//!
//! Schema-version dispatched on the top-level integer `version` field:
//!
//! - **v1** (Swift 5.0 — 5.5): `object.pins[]` with shape `{package,
//!   repositoryURL, state: {branch, revision, version}}`.
//! - **v2** (Swift 5.6 — 5.10): top-level `pins[]` with shape `{identity,
//!   kind, location, state: {revision, version}}`.
//! - **v3** (Swift 5.10+): same shape as v2 plus an optional `originHash`
//!   field on each pin. waybill IGNORES `originHash` in v0.1.
//!
//! PURL projection per `contracts/swift-lockfile-format.md` § "PURL
//! projection rules": strip `.git` suffix; HTTPS-form / SSH-form / deep-
//! namespace handling; commit-pinned mode uses the FULL 40-char revision
//! SHA as the version segment (clarification Q1 / FR-003).

use std::path::{Path, PathBuf};

use std::sync::LazyLock;

use waybill_common::types::purl::{encode_purl_segment, Purl};
use regex::Regex;
use thiserror::Error;

use super::super::PackageDbEntry;

/// Parsed `pins[]` entry from a `Package.resolved` lockfile. The schema-
/// version dispatcher collapses v1/v2/v3 shape differences into this
/// uniform internal representation.
#[derive(Debug, Clone)]
pub(super) struct SwiftLockfileEntry {
    /// Package name (lowercased per SwiftPM convention).
    /// v1 reads from `pins[].package`; v2/v3 from `pins[].identity`.
    pub(super) identity: String,
    /// Source-of-truth URL for PURL projection.
    /// v1 reads from `pins[].repositoryURL`; v2/v3 from `pins[].location`.
    /// The `.git` suffix is preserved here; stripping happens at
    /// projection time.
    pub(super) location: String,
    /// Resolved version when the `state.version` field is present;
    /// `None` for commit-pinned mode.
    pub(super) version: Option<String>,
    /// 40-char lowercase hex revision SHA. Required on every SwiftPM
    /// schema version; validation enforces the length + character class
    /// at parse time.
    pub(super) revision: String,
}

#[derive(Debug, Error)]
pub(super) enum SwiftLockfileError {
    #[error("Package.resolved at `{path}` unreadable: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Package.resolved at `{path}` is not valid JSON: {source}")]
    ParseJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("Package.resolved at `{path}` has unknown schema version `{version}` (expected 1, 2, or 3)")]
    UnknownVersion { path: PathBuf, version: i64 },
    #[error("Package.resolved at `{path}` missing required `pins[]` array")]
    MissingPinsArray { path: PathBuf },
}

/// Parse a `Package.resolved` lockfile into a vec of `PackageDbEntry`
/// records (per contracts/swift-lockfile-format.md § "Output: PackageDbEntry
/// shape"). Failures bubble up as `SwiftLockfileError`; per FR-009 the
/// caller logs + skips this file's components on error. Individual pin
/// failures (invalid revision, unparseable URL) emit `tracing::warn!`
/// for that entry only — other entries in the same file still emit.
pub(super) fn read_package_resolved(
    path: &Path,
) -> Result<Vec<PackageDbEntry>, SwiftLockfileError> {
    let bytes = std::fs::read(path).map_err(|source| SwiftLockfileError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|source| SwiftLockfileError::ParseJson {
            path: path.to_path_buf(),
            source,
        })?;
    let version = value
        .get("version")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| SwiftLockfileError::UnknownVersion {
            path: path.to_path_buf(),
            version: -1,
        })?;
    let entries = match version {
        1 => parse_v1(&value, path)?,
        2 | 3 => parse_v2_or_v3(&value, path)?,
        v => {
            return Err(SwiftLockfileError::UnknownVersion {
                path: path.to_path_buf(),
                version: v,
            });
        }
    };
    let source_path = path.to_string_lossy().into_owned();
    let out = entries
        .into_iter()
        .filter_map(|entry| project_to_package_db_entry(entry, &source_path, path))
        .collect();
    Ok(out)
}

fn parse_v1(value: &serde_json::Value, path: &Path) -> Result<Vec<SwiftLockfileEntry>, SwiftLockfileError> {
    let pins = value
        .get("object")
        .and_then(|o| o.get("pins"))
        .and_then(|p| p.as_array())
        .ok_or_else(|| SwiftLockfileError::MissingPinsArray {
            path: path.to_path_buf(),
        })?;
    Ok(pins.iter().filter_map(parse_pin_v1).collect())
}

fn parse_pin_v1(pin: &serde_json::Value) -> Option<SwiftLockfileEntry> {
    let identity = pin.get("package").and_then(|v| v.as_str())?.to_lowercase();
    let location = pin.get("repositoryURL").and_then(|v| v.as_str())?.to_string();
    let state = pin.get("state")?;
    let revision = state.get("revision").and_then(|v| v.as_str())?.to_string();
    let version = state.get("version").and_then(|v| v.as_str()).map(String::from);
    Some(SwiftLockfileEntry {
        identity,
        location,
        version,
        revision,
    })
}

fn parse_v2_or_v3(
    value: &serde_json::Value,
    path: &Path,
) -> Result<Vec<SwiftLockfileEntry>, SwiftLockfileError> {
    let pins = value
        .get("pins")
        .and_then(|p| p.as_array())
        .ok_or_else(|| SwiftLockfileError::MissingPinsArray {
            path: path.to_path_buf(),
        })?;
    Ok(pins.iter().filter_map(parse_pin_v2).collect())
}

fn parse_pin_v2(pin: &serde_json::Value) -> Option<SwiftLockfileEntry> {
    let identity = pin.get("identity").and_then(|v| v.as_str())?.to_string();
    let location = pin.get("location").and_then(|v| v.as_str())?.to_string();
    let state = pin.get("state")?;
    let revision = state.get("revision").and_then(|v| v.as_str())?.to_string();
    let version = state.get("version").and_then(|v| v.as_str()).map(String::from);
    Some(SwiftLockfileEntry {
        identity,
        location,
        version,
        revision,
    })
}

static REVISION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9a-f]{40}$").expect("revision regex compiles"));

/// Project one `SwiftLockfileEntry` to a `PackageDbEntry`. Returns `None`
/// when the entry fails downstream validation (bad revision, unparseable
/// URL); each rejection emits `tracing::warn!` so operators can grep for
/// the affected pin.
fn project_to_package_db_entry(
    entry: SwiftLockfileEntry,
    source_path: &str,
    path: &Path,
) -> Option<PackageDbEntry> {
    if !REVISION_RE.is_match(&entry.revision) {
        tracing::warn!(
            path = %path.display(),
            identity = %entry.identity,
            revision = %entry.revision,
            "swift: pin has invalid revision (not 40-char lowercase hex); skipping this entry"
        );
        return None;
    }
    let version_segment = entry
        .version
        .clone()
        .unwrap_or_else(|| entry.revision.clone());
    let purl = match project_purl(&entry.location, &version_segment) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                identity = %entry.identity,
                location = %entry.location,
                error = %e,
                "swift: pin location unparseable; skipping this entry"
            );
            return None;
        }
    };
    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "waybill:source-files".to_string(),
        serde_json::Value::String(source_path.to_string()),
    );
    let source_type = if entry.version.is_none() {
        extra_annotations.insert(
            "waybill:source-revision".to_string(),
            serde_json::Value::String(entry.revision.clone()),
        );
        Some("git".to_string())
    } else {
        None
    };

    Some(PackageDbEntry {
        purl,
        name: entry.identity,
        version: version_segment,
        arch: None,
        source_path: source_path.to_string(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type,
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
        build_inclusion: None,
    })
}

#[derive(Debug, Error)]
pub(super) enum PurlProjectionError {
    #[error("location `{0}` does not match HTTPS or SSH URL patterns")]
    UnparseableLocation(String),
    #[error(
        "deep-namespace URL `{location}` unsupported in v0.1 (namespace `{namespace}` has more than one segment; the purl-spec swift type allows single-segment namespaces only)"
    )]
    DeepNamespaceUnsupported { location: String, namespace: String },
    #[error("PURL construction failed for `{location}`: {message}")]
    Construction { location: String, message: String },
}

static SSH_FORM_RE: LazyLock<Regex> = LazyLock::new(|| {
    // `git@host:namespace/name.git` or `host:namespace/name` — captures
    // host + path, drops the optional user segment per purl-spec.
    Regex::new(r"^(?:[^@/:]+@)?(?P<host>[^:/]+):(?P<path>[^\s]+?)(?:\.git)?$")
        .expect("ssh regex compiles")
});

/// Project a SwiftPM lockfile `location` URL into a `pkg:swift/<host>/
/// <namespace>/<name>@<version>` PURL per contracts/swift-lockfile-format.md
/// § "PURL projection rules". Strips the `.git` suffix; handles HTTPS
/// and SSH forms with single-segment namespaces.
///
/// Deep-namespace URLs (e.g., GitLab subgroups
/// `https://gitlab.com/group/subgroup/project.git`) are NOT supported in
/// v0.1: the purl-spec swift type allows only single-segment namespaces,
/// and the upstream `packageurl` crate's parser rejects multi-segment
/// forms. Operators using GitLab subgroups receive a `tracing::warn!`
/// and the affected entry drops. Deep-namespace handling is deferred to
/// a future phase pending a purl-spec swift-type extension.
pub(super) fn project_purl(location: &str, version: &str) -> Result<Purl, PurlProjectionError> {
    let (host, namespace, name) = parse_location(location)?;
    if namespace.contains('/') {
        return Err(PurlProjectionError::DeepNamespaceUnsupported {
            location: location.to_string(),
            namespace: namespace.clone(),
        });
    }
    let encoded_ns = encode_purl_segment(&namespace);
    let encoded_name = encode_purl_segment(&name);
    let encoded_version = encode_purl_segment(version);
    let purl_str = format!(
        "pkg:swift/{}/{}/{}@{}",
        host, encoded_ns, encoded_name, encoded_version
    );
    Purl::new(&purl_str).map_err(|e| PurlProjectionError::Construction {
        location: location.to_string(),
        message: e.to_string(),
    })
}

/// Extract `(host, namespace, name)` from a location URL. Strips the
/// `.git` suffix; handles HTTPS-form, HTTPS-without-suffix, SSH-form,
/// and deep-namespace (GitLab subgroups).
fn parse_location(location: &str) -> Result<(String, String, String), PurlProjectionError> {
    // HTTPS-form first (the dominant case).
    if let Some(after_scheme) = location
        .strip_prefix("https://")
        .or_else(|| location.strip_prefix("http://"))
    {
        let trimmed = after_scheme.strip_suffix(".git").unwrap_or(after_scheme);
        // Split off the host (first `/`).
        let (host, path) = trimmed.split_once('/').ok_or_else(|| {
            PurlProjectionError::UnparseableLocation(location.to_string())
        })?;
        // The path is `<namespace-segments...>/<name>`. The LAST segment
        // is the name; everything before is the namespace.
        let path = path.trim_end_matches('/');
        let (namespace, name) = path.rsplit_once('/').ok_or_else(|| {
            PurlProjectionError::UnparseableLocation(location.to_string())
        })?;
        return Ok((
            host.to_string(),
            namespace.to_string(),
            name.to_string(),
        ));
    }
    // SSH-form fallback: `git@host:namespace/name.git`.
    if let Some(captures) = SSH_FORM_RE.captures(location) {
        let host = captures
            .name("host")
            .map(|m| m.as_str())
            .ok_or_else(|| PurlProjectionError::UnparseableLocation(location.to_string()))?;
        let path = captures
            .name("path")
            .map(|m| m.as_str())
            .ok_or_else(|| PurlProjectionError::UnparseableLocation(location.to_string()))?;
        let path = path.trim_end_matches('/');
        let (namespace, name) = path.rsplit_once('/').ok_or_else(|| {
            PurlProjectionError::UnparseableLocation(location.to_string())
        })?;
        return Ok((
            host.to_string(),
            namespace.to_string(),
            name.to_string(),
        ));
    }
    Err(PurlProjectionError::UnparseableLocation(location.to_string()))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_lockfile(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    // ---------- PURL projection ----------

    #[test]
    fn https_with_git_suffix_strips_suffix() {
        let purl = project_purl("https://github.com/apple/swift-log.git", "1.5.4").unwrap();
        assert_eq!(
            purl.as_str(),
            "pkg:swift/github.com/apple/swift-log@1.5.4"
        );
    }

    #[test]
    fn https_without_git_suffix_works() {
        let purl = project_purl("https://github.com/apple/swift-log", "1.5.4").unwrap();
        assert_eq!(
            purl.as_str(),
            "pkg:swift/github.com/apple/swift-log@1.5.4"
        );
    }

    #[test]
    fn ssh_form_strips_user_and_git_suffix() {
        let purl =
            project_purl("git@gitlab.acme.com:internal/lib.git", "0.1.0").unwrap();
        assert_eq!(
            purl.as_str(),
            "pkg:swift/gitlab.acme.com/internal/lib@0.1.0"
        );
    }

    #[test]
    fn deep_namespace_unsupported_in_v0_1() {
        // GitLab subgroups produce multi-segment namespaces. The purl-spec
        // swift type only allows single-segment namespaces, so v0.1
        // returns a `DeepNamespaceUnsupported` error and the caller logs
        // + drops this pin. Deferred to a future phase.
        let err = project_purl("https://gitlab.com/group/subgroup/project.git", "2.0.0")
            .unwrap_err();
        assert!(
            matches!(err, PurlProjectionError::DeepNamespaceUnsupported { .. }),
            "expected DeepNamespaceUnsupported, got {:?}",
            err
        );
    }

    #[test]
    fn full_sha_works_as_version_segment() {
        let sha = "abc123def456abc123def456abc123def456abcd";
        let purl =
            project_purl("https://github.com/apple/swift-log.git", sha).unwrap();
        assert_eq!(
            purl.as_str(),
            format!("pkg:swift/github.com/apple/swift-log@{}", sha)
        );
    }

    #[test]
    fn unparseable_location_errs() {
        let err = project_purl("not-a-url", "1.0.0").unwrap_err();
        assert!(matches!(err, PurlProjectionError::UnparseableLocation(_)));
    }

    // ---------- Schema dispatch ----------

    #[test]
    fn v2_happy_path_emits_components() {
        let f = write_lockfile(
            r#"{
                "version": 2,
                "pins": [
                    {
                        "identity": "swift-log",
                        "kind": "remoteSourceControl",
                        "location": "https://github.com/apple/swift-log.git",
                        "state": {
                            "revision": "abc123def456abc123def456abc123def456abcd",
                            "version": "1.5.4"
                        }
                    }
                ]
            }"#,
        );
        let entries = read_package_resolved(f.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:swift/github.com/apple/swift-log@1.5.4"
        );
        assert_eq!(entries[0].version, "1.5.4");
        assert_eq!(entries[0].source_type, None);
        assert_eq!(entries[0].sbom_tier.as_deref(), Some("source"));
    }

    #[test]
    fn v3_with_origin_hash_ignored() {
        let f = write_lockfile(
            r#"{
                "version": 3,
                "originHash": "ignored-by-waybill",
                "pins": [
                    {
                        "identity": "swift-log",
                        "kind": "remoteSourceControl",
                        "location": "https://github.com/apple/swift-log.git",
                        "state": {
                            "revision": "abc123def456abc123def456abc123def456abcd",
                            "version": "1.5.4"
                        }
                    }
                ]
            }"#,
        );
        let entries = read_package_resolved(f.path()).unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn v1_happy_path_reads_object_pins() {
        let f = write_lockfile(
            r#"{
                "version": 1,
                "object": {
                    "pins": [
                        {
                            "package": "swift-log",
                            "repositoryURL": "https://github.com/apple/swift-log.git",
                            "state": {
                                "branch": null,
                                "revision": "abc123def456abc123def456abc123def456abcd",
                                "version": "1.4.4"
                            }
                        }
                    ]
                }
            }"#,
        );
        let entries = read_package_resolved(f.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "swift-log");
        assert_eq!(entries[0].version, "1.4.4");
    }

    #[test]
    fn unknown_version_returns_err() {
        let f = write_lockfile(r#"{"version": 4, "pins": []}"#);
        let err = read_package_resolved(f.path()).unwrap_err();
        assert!(matches!(err, SwiftLockfileError::UnknownVersion { .. }));
    }

    #[test]
    fn missing_pins_array_returns_err() {
        let f = write_lockfile(r#"{"version": 2}"#);
        let err = read_package_resolved(f.path()).unwrap_err();
        assert!(matches!(err, SwiftLockfileError::MissingPinsArray { .. }));
    }

    #[test]
    fn malformed_json_returns_parse_err() {
        let f = write_lockfile("not json");
        let err = read_package_resolved(f.path()).unwrap_err();
        assert!(matches!(err, SwiftLockfileError::ParseJson { .. }));
    }

    #[test]
    fn invalid_revision_skips_entry_but_continues() {
        let f = write_lockfile(
            r#"{
                "version": 2,
                "pins": [
                    {
                        "identity": "bad-pin",
                        "location": "https://github.com/example/bad.git",
                        "state": { "revision": "not-40-char-hex" }
                    },
                    {
                        "identity": "good-pin",
                        "location": "https://github.com/example/good.git",
                        "state": {
                            "revision": "abc123def456abc123def456abc123def456abcd",
                            "version": "1.0.0"
                        }
                    }
                ]
            }"#,
        );
        let entries = read_package_resolved(f.path()).unwrap();
        // Bad pin dropped; good pin emerges.
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "good-pin");
    }

    #[test]
    fn commit_pinned_uses_full_sha_as_version() {
        let sha = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        let f = write_lockfile(&format!(
            r#"{{
                "version": 2,
                "pins": [
                    {{
                        "identity": "commit-pinned",
                        "location": "https://github.com/example/cp.git",
                        "state": {{ "revision": "{sha}" }}
                    }}
                ]
            }}"#
        ));
        let entries = read_package_resolved(f.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.version, sha);
        assert_eq!(
            e.purl.as_str(),
            format!("pkg:swift/github.com/example/cp@{}", sha)
        );
        assert_eq!(e.source_type.as_deref(), Some("git"));
        assert_eq!(
            e.extra_annotations
                .get("waybill:source-revision")
                .and_then(|v| v.as_str()),
            Some(sha)
        );
    }
}
