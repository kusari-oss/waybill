//! Milestone 224: coursier lockfile TOML parser + Entry → PackageDbEntry mapping.
//!
//! Coursier lockfiles produced by `pants generate-lockfiles` under
//! `3rdparty/jvm/*.lock`. Schema shape verified empirically 2026-08-01
//! against `github.com/pantsbuild/example-jvm @ main`. See
//! `specs/224-pants-coursier-jvm/research.md` §R1.
//!
//! The header block up to `# --- END PANTS LOCKFILE METADATA` is a
//! JSON document embedded in TOML comments (`# ` prefix per line). The
//! FR-011 discriminator is the literal `# --- BEGIN PANTS LOCKFILE METADATA`
//! substring; its absence identifies a standalone coursier lockfile
//! (not Pants-generated) and triggers an INFO-level skip (not WARN).
//!
//! Full fail-open behavior boundaries at
//! `specs/224-pants-coursier-jvm/contracts/coursier-lockfile-schema.md`
//! §"Fail-open behavior boundaries".

use std::path::Path;

use serde::Deserialize;
use serde_json::json;
use waybill_common::types::hash::ContentHash;
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::coordinate::parse_coord_string;
use super::resolve_classifier::classify_resolve;
use crate::scan_fs::package_db::PackageDbEntry;

/// Reason a candidate lockfile was skipped without producing entries.
/// The orchestrator tallies each variant into the FR-010 log's
/// corresponding counter (`lockfiles_skipped_non_pants` for
/// [`SkipReason::NotPants`], `lockfiles_skipped_corrupt` for the
/// other two).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum SkipReason {
    #[error("not a Pants-generated coursier lockfile; skipping")]
    NotPants,
    #[error("Pants metadata invalid: {0}")]
    MetadataInvalid(String),
    #[error("TOML body parse error: {0}")]
    TomlParseError(String),
}

/// Top-level coursier lockfile shape (post header-strip).
#[derive(Debug, Deserialize)]
pub(crate) struct CoursierLockfile {
    /// Zero or more locked distributions. May be empty for a
    /// partially-generated lockfile (INFO, not WARN).
    #[serde(default)]
    pub(crate) entries: Vec<Entry>,
}

/// One locked distribution. Maps 1:1 to an emitted `PackageDbEntry`
/// in the happy path.
#[derive(Debug, Deserialize)]
pub(crate) struct Entry {
    /// Coordinate strings for direct declared deps. Often empty in
    /// coursier lockfiles (dep graph lives in `dependencies[]`).
    #[serde(default, rename = "directDependencies")]
    pub(crate) direct_dependencies: Vec<String>,
    /// Coordinate strings for transitive resolved deps. This is the
    /// ground truth for the dependency graph.
    #[serde(default)]
    pub(crate) dependencies: Vec<String>,
    /// The artifact filename (e.g., `"guava-31.0.1-jre.jar"`). Recorded
    /// on the parsed struct for future diagnostic use; v1 does NOT
    /// emit this into the SBOM per data-model.md §"Decision on
    /// waybill:file-name".
    #[serde(default)]
    pub(crate) file_name: Option<String>,
    /// The Maven coordinate triple + optional classifier + packaging.
    pub(crate) coord: EntryCoord,
    /// Optional artifact hash. Absent when the artifact was resolved
    /// but not downloaded (rare — some `url=not_provided` scenarios).
    #[serde(default)]
    pub(crate) file_digest: Option<EntryFileDigest>,
}

/// Maven coordinate + optional qualifiers.
#[derive(Debug, Deserialize)]
pub(crate) struct EntryCoord {
    pub(crate) group: String,
    pub(crate) artifact: String,
    pub(crate) version: String,
    /// `"jar"` (Maven default), `"war"`, `"pom"`, `"aar"` (Android), etc.
    /// Emitted as PURL `?type=<value>` qualifier when non-`jar`.
    #[serde(default)]
    pub(crate) packaging: Option<String>,
    /// Optional Maven classifier (`"sources"`, `"javadoc"`, platform tags
    /// like `"linux-x86_64"`). Emitted as PURL `?classifier=<value>`
    /// qualifier when present + non-empty.
    #[serde(default)]
    pub(crate) classifier: Option<String>,
    /// Optional fetch URL. When present + non-empty, emitted as
    /// `waybill:source-url` annotation (reuses m223 C144 catalog row).
    #[serde(default)]
    pub(crate) url: Option<String>,
}

/// sha256 fingerprint (+ optional serialized byte length).
#[derive(Debug, Deserialize)]
pub(crate) struct EntryFileDigest {
    /// Hex-encoded sha256. Recorded as `ContentHash::sha256` on the
    /// PackageDbEntry.
    pub(crate) fingerprint: String,
    /// Byte length of the serialized artifact. Kept on the struct for
    /// future diagnostic use; v1 does NOT emit this into the SBOM.
    #[serde(default)]
    pub(crate) serialized_bytes_length: Option<u64>,
}

/// Pants header metadata JSON (embedded inside the `# ` comment block
/// above the TOML body). Only `version` gates further parsing.
#[derive(Debug, Deserialize)]
pub(crate) struct PantsMetadata {
    /// Metadata schema version. Only `1` is supported; other values
    /// trigger WARN + skip of the whole lockfile.
    pub(crate) version: u32,
    /// Original coordinate strings the operator passed to
    /// `pants generate-lockfiles`. Diagnostic only; not extracted.
    #[serde(default)]
    pub(crate) generated_with_requirements: Vec<String>,
}

/// FR-011 discriminator: the literal Pants header substring.
const PANTS_HEADER_BEGIN: &str = "# --- BEGIN PANTS LOCKFILE METADATA";
const PANTS_HEADER_END: &str = "# --- END PANTS LOCKFILE METADATA";

/// Return true iff the line begins the Pants metadata header block.
/// Tight prefix-match on the trimmed line: prevents false positives
/// from documentation comments that mention the marker inline.
fn is_header_begin(line: &str) -> bool {
    line.trim_start().starts_with(PANTS_HEADER_BEGIN)
}

/// Return true iff the line ends the Pants metadata header block.
fn is_header_end(line: &str) -> bool {
    line.trim_start().starts_with(PANTS_HEADER_END)
}

/// Parse a coursier lockfile per contracts/coursier-lockfile-schema.md
/// §"Fail-open behavior boundaries":
///
/// 1. Absent Pants header → `Err(SkipReason::NotPants)` (INFO at caller).
/// 2. Header present but embedded metadata JSON invalid or
///    `version != 1` → `Err(SkipReason::MetadataInvalid)` (WARN).
/// 3. Header valid, but TOML body malformed → `Err(SkipReason::TomlParseError)` (WARN).
///
/// The caller is responsible for adding path context to any log line.
pub(crate) fn parse(bytes: &[u8]) -> Result<CoursierLockfile, SkipReason> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| SkipReason::TomlParseError(format!("invalid UTF-8: {e}")))?;

    if !text.lines().any(is_header_begin) {
        return Err(SkipReason::NotPants);
    }

    // Extract lines between BEGIN and END; strip the leading `# `
    // from each; concatenate to form the metadata JSON blob.
    let mut in_block = false;
    let mut json_lines: Vec<&str> = Vec::new();
    for line in text.lines() {
        if is_header_begin(line) {
            in_block = true;
            continue;
        }
        if is_header_end(line) {
            in_block = false;
            continue;
        }
        if in_block {
            let stripped = line
                .strip_prefix("# ")
                .or_else(|| line.strip_prefix("#"))
                .unwrap_or(line);
            json_lines.push(stripped);
        }
    }
    let json_blob = json_lines.join("\n");
    let metadata: PantsMetadata = serde_json::from_str(&json_blob).map_err(|e| {
        SkipReason::MetadataInvalid(format!("metadata JSON parse: {e}"))
    })?;
    if metadata.version != 1 {
        return Err(SkipReason::MetadataInvalid(format!(
            "unsupported metadata version {} (expected 1)",
            metadata.version,
        )));
    }
    // Diagnostic — the field exists for schema documentation only.
    let _ = metadata.generated_with_requirements;

    // Strip the header comment block from the TOML body. The header
    // spans a contiguous run of `# `-prefixed lines that includes
    // BEGIN and END markers; everything BEFORE the first BEGIN marker
    // and everything AFTER the END marker is TOML body. Some Pants
    // versions also emit a plain comment banner (e.g., "This lockfile
    // was autogenerated...") that is safe to keep — TOML ignores it.
    let body = strip_pants_header(text);

    let lock: CoursierLockfile = toml::from_str(&body)
        .map_err(|e| SkipReason::TomlParseError(format!("{e}")))?;
    Ok(lock)
}

/// Strip the `# --- BEGIN ... END ---` block plus its inner `#`-lines
/// out of the input, leaving TOML body content intact.
fn strip_pants_header(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_block = false;
    for line in text.lines() {
        if is_header_begin(line) {
            in_block = true;
            continue;
        }
        if is_header_end(line) {
            in_block = false;
            continue;
        }
        if in_block {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Convert one `Entry` into a `PackageDbEntry` per data-model.md
/// §"PackageDbEntry field mapping". Returns `None` + WARN on:
/// - empty group / artifact / version
/// - PURL construction failure
pub(crate) fn entry_to_package_db_entry(
    entry: &Entry,
    lockfile_path: &Path,
    resolve_name: &str,
) -> Option<PackageDbEntry> {
    let group = entry.coord.group.trim();
    let artifact = entry.coord.artifact.trim();
    let version = entry.coord.version.trim();
    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        tracing::warn!(
            lockfile = %lockfile_path.display(),
            resolve = %resolve_name,
            "pants-coursier-jvm reader: entry has empty group/artifact/version; skipping"
        );
        return None;
    }

    // Inline PURL construction per research.md §R3 option B. Mirrors
    // the maven.rs pattern for classifier/type qualifiers.
    let mut purl_str = format!(
        "pkg:maven/{}/{}@{}",
        encode_purl_segment(group),
        encode_purl_segment(artifact),
        encode_purl_segment(version),
    );
    // Append qualifiers: ?classifier=<c>&type=<packaging>. Only include
    // packaging when it's set AND non-`jar` (jar is the Maven default;
    // emitting it clutters PURLs). Only include classifier when it's
    // set AND non-empty.
    let has_classifier = entry
        .coord
        .classifier
        .as_deref()
        .filter(|s| !s.is_empty())
        .is_some();
    let has_packaging = entry
        .coord
        .packaging
        .as_deref()
        .filter(|s| !s.is_empty() && *s != "jar")
        .is_some();
    if has_classifier || has_packaging {
        let mut first = true;
        if let Some(cls) = entry
            .coord
            .classifier
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            purl_str.push_str(if first { "?" } else { "&" });
            purl_str.push_str("classifier=");
            purl_str.push_str(&encode_purl_segment(cls));
            first = false;
        }
        if let Some(pkg) = entry
            .coord
            .packaging
            .as_deref()
            .filter(|s| !s.is_empty() && *s != "jar")
        {
            purl_str.push_str(if first { "?" } else { "&" });
            purl_str.push_str("type=");
            purl_str.push_str(&encode_purl_segment(pkg));
        }
    }
    let purl = Purl::new(&purl_str)
        .map_err(|e| {
            tracing::warn!(
                lockfile = %lockfile_path.display(),
                resolve = %resolve_name,
                purl_str = %purl_str,
                error = %e,
                "pants-coursier-jvm reader: PURL construction failed; skipping entry"
            );
        })
        .ok()?;

    // Dep edges: parse dependencies[] coord strings; drop unparseables
    // with a WARN (per fail-open matrix "Coordinate-string parse
    // failure → Skip THIS edge; continue"). Emit downstream as
    // `<group>:<artifact>` (no version) to match the Maven reader's
    // `depends` convention at `maven.rs:4121` — the shared
    // `name_to_purl` index at `scan_fs/mod.rs:578-585` inserts an
    // extra `"groupId:artifactId"` key for every maven entry so edge
    // resolution works without version-string coordination.
    let depends: Vec<String> = entry
        .dependencies
        .iter()
        .filter_map(|s| match parse_coord_string(s) {
            Some(c) => Some(format!("{}:{}", c.group, c.artifact)),
            None => {
                tracing::warn!(
                    lockfile = %lockfile_path.display(),
                    resolve = %resolve_name,
                    coord_string = %s,
                    "pants-coursier-jvm reader: dependency coord string unparseable; dropping edge"
                );
                None
            }
        })
        .collect();

    // Hashes: at most one sha256 per entry.
    let hashes: Vec<ContentHash> = entry
        .file_digest
        .as_ref()
        .and_then(|d| ContentHash::sha256(&d.fingerprint).ok())
        .into_iter()
        .collect();

    // Annotations: always pants-resolve; source-url iff non-empty.
    let mut extra_annotations = std::collections::BTreeMap::new();
    extra_annotations.insert(
        "waybill:pants-resolve".to_string(),
        json!(resolve_name),
    );
    if let Some(url) = entry.coord.url.as_deref().filter(|s| !s.is_empty()) {
        extra_annotations.insert(
            "waybill:source-url".to_string(),
            json!(url),
        );
    }

    // Silence dead-code warnings for direct_dependencies + file_name +
    // serialized_bytes_length; the fields are declared for schema
    // documentation and future use per data-model.md.
    let _ = &entry.direct_dependencies;
    let _ = &entry.file_name;
    if let Some(fd) = entry.file_digest.as_ref() {
        let _ = fd.serialized_bytes_length;
    }

    Some(PackageDbEntry {
        purl,
        name: artifact.to_string(),
        version: version.to_string(),
        arch: None,
        source_path: lockfile_path.display().to_string(),
        depends,
        maintainer: None,
        lifecycle_scope: Some(classify_resolve(resolve_name)),
        requirement_ranges: Vec::new(),
        source_type: None,
        licenses: Vec::new(),
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
    use std::path::PathBuf;

    const VALID_PANTS_LOCKFILE: &str = r#"# --- BEGIN PANTS LOCKFILE METADATA: DO NOT EDIT OR REMOVE ---
# {
#   "version": 1,
#   "generated_with_requirements": []
# }
# --- END PANTS LOCKFILE METADATA ---

[[entries]]
directDependencies = []
dependencies = []
file_name = "core-1.0.0.jar"

[entries.coord]
group = "dev.waybill.fixture"
artifact = "core"
version = "1.0.0"

[entries.file_digest]
fingerprint = "0000000000000000000000000000000000000000000000000000000000000001"
"#;

    // ---- parse() tests ----

    #[test]
    fn parse_valid_pants_coursier_lockfile() {
        let lock = parse(VALID_PANTS_LOCKFILE.as_bytes()).expect("valid parse");
        assert_eq!(lock.entries.len(), 1);
        assert_eq!(lock.entries[0].coord.group, "dev.waybill.fixture");
        assert_eq!(lock.entries[0].coord.artifact, "core");
        assert_eq!(lock.entries[0].coord.version, "1.0.0");
    }

    #[test]
    fn parse_missing_header_returns_notpants() {
        // Valid TOML but no Pants metadata header — should be
        // classified as standalone coursier lockfile (NOT corrupt).
        let text = r#"
[[entries]]
directDependencies = []
dependencies = []

[entries.coord]
group = "com.example"
artifact = "lib"
version = "1.0.0"
"#;
        let err = parse(text.as_bytes()).unwrap_err();
        assert_eq!(err, SkipReason::NotPants);
    }

    #[test]
    fn parse_bad_metadata_version_returns_metadatainvalid() {
        let text = r#"# --- BEGIN PANTS LOCKFILE METADATA: DO NOT EDIT OR REMOVE ---
# {"version": 99}
# --- END PANTS LOCKFILE METADATA ---
[[entries]]
[entries.coord]
group = "g"
artifact = "a"
version = "v"
"#;
        let err = parse(text.as_bytes()).unwrap_err();
        assert!(
            matches!(err, SkipReason::MetadataInvalid(_)),
            "expected MetadataInvalid, got {err:?}",
        );
    }

    #[test]
    fn parse_malformed_toml_body_returns_tomlparseerror() {
        let text = r#"# --- BEGIN PANTS LOCKFILE METADATA: DO NOT EDIT OR REMOVE ---
# {"version": 1}
# --- END PANTS LOCKFILE METADATA ---
this is = = not = valid toml
"#;
        let err = parse(text.as_bytes()).unwrap_err();
        assert!(
            matches!(err, SkipReason::TomlParseError(_)),
            "expected TomlParseError, got {err:?}",
        );
    }

    // ---- entry_to_package_db_entry() tests ----

    fn make_entry(
        group: &str,
        artifact: &str,
        version: &str,
        packaging: Option<&str>,
        classifier: Option<&str>,
        url: Option<&str>,
        digest: Option<&str>,
    ) -> Entry {
        Entry {
            direct_dependencies: Vec::new(),
            dependencies: Vec::new(),
            file_name: None,
            coord: EntryCoord {
                group: group.to_string(),
                artifact: artifact.to_string(),
                version: version.to_string(),
                packaging: packaging.map(String::from),
                classifier: classifier.map(String::from),
                url: url.map(String::from),
            },
            file_digest: digest.map(|d| EntryFileDigest {
                fingerprint: d.to_string(),
                serialized_bytes_length: None,
            }),
        }
    }

    #[test]
    fn entry_happy_path_plain() {
        let entry = make_entry(
            "com.example",
            "lib",
            "1.0.0",
            None,
            None,
            None,
            Some("0000000000000000000000000000000000000000000000000000000000000001"),
        );
        let out = entry_to_package_db_entry(&entry, &PathBuf::from("/x/default.lock"), "default")
            .expect("emit ok");
        assert_eq!(out.purl.as_str(), "pkg:maven/com.example/lib@1.0.0");
        assert_eq!(out.name, "lib");
        assert_eq!(out.version, "1.0.0");
        assert_eq!(out.hashes.len(), 1);
        assert_eq!(out.sbom_tier.as_deref(), Some("source"));
        assert_eq!(
            out.extra_annotations
                .get("waybill:pants-resolve")
                .and_then(|v| v.as_str()),
            Some("default"),
        );
        assert!(!out.extra_annotations.contains_key("waybill:source-url"));
    }

    #[test]
    fn entry_with_classifier_and_war_packaging() {
        let entry = make_entry(
            "com.example",
            "native",
            "1.0.0",
            Some("so"),
            Some("linux-x86_64"),
            None,
            None,
        );
        let out = entry_to_package_db_entry(&entry, &PathBuf::from("/x/default.lock"), "default")
            .expect("emit ok");
        let purl = out.purl.as_str();
        assert!(purl.contains("classifier=linux-x86_64"), "purl: {purl}");
        assert!(purl.contains("type=so"), "purl: {purl}");
    }

    #[test]
    fn entry_with_url_emits_source_url_annotation() {
        let entry = make_entry(
            "com.example",
            "lib",
            "1.0.0",
            None,
            None,
            Some("https://mirror.example.test/lib-1.0.0.jar"),
            None,
        );
        let out = entry_to_package_db_entry(&entry, &PathBuf::from("/x/default.lock"), "default")
            .expect("emit ok");
        assert_eq!(
            out.extra_annotations
                .get("waybill:source-url")
                .and_then(|v| v.as_str()),
            Some("https://mirror.example.test/lib-1.0.0.jar"),
        );
    }

    #[test]
    fn entry_with_empty_version_skipped() {
        let entry = make_entry("com.example", "lib", "", None, None, None, None);
        let out = entry_to_package_db_entry(&entry, &PathBuf::from("/x/default.lock"), "default");
        assert!(out.is_none());
    }
}

