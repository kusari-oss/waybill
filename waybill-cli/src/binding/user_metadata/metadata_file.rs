//! Milestone 080 — `--metadata-file <path.json>` sidecar input. Schema
//! is `#[serde(deny_unknown_fields)]` so typos surface as crisp parse
//! errors at CLI startup.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use serde::Deserialize;

use super::BuildUserMetadataError;

/// Schema for the `--metadata-file <path.json>` sidecar input.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MetadataFile {
    /// Raw `Type: Name` strings — same shape as the `--creator` flag.
    /// Parsed via [`super::creator::parse_creator_str`] downstream.
    #[serde(default)]
    pub creators: Vec<String>,
    /// Structured annotator/comment pairs. Each entry's `type_name` is
    /// parsed via [`super::creator::parse_creator_str`] downstream.
    #[serde(default)]
    pub annotators: Vec<MetadataFileAnnotator>,
    /// Single-valued document-level free-text comment. Equivalent to
    /// `--metadata-comment`.
    #[serde(default)]
    pub metadata_comment: Option<String>,
    /// Single-valued operator-supplied document/Sbom-level name.
    /// Equivalent to `--scan-target-name`.
    #[serde(default)]
    pub scan_target_name: Option<String>,
}

/// One annotator/comment pair inside a [`MetadataFile`]. Mirrors the
/// CLI flag pair `--annotator <type_name>` + `--annotation-comment
/// <comment>` but in unambiguous JSON-object form.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MetadataFileAnnotator {
    pub type_name: String,
    pub comment: String,
}

/// Load a [`MetadataFile`] from disk. Returns
/// [`BuildUserMetadataError::MetadataFileIo`] on read failure or
/// [`BuildUserMetadataError::MetadataFileParseError`] on JSON-syntax /
/// `deny_unknown_fields` violations.
pub fn load_metadata_file(path: &Path) -> Result<MetadataFile, BuildUserMetadataError> {
    let file = File::open(path).map_err(|e| BuildUserMetadataError::MetadataFileIo {
        path: path.to_path_buf(),
        source: e,
    })?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).map_err(|e| {
        BuildUserMetadataError::MetadataFileParseError {
            path: path.to_path_buf(),
            source: e,
        }
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_tempfile(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn load_valid_full_file() {
        let f = write_tempfile(
            r#"{
  "creators": ["Tool: T1", "Organization: O", "Person: P"],
  "annotators": [
    {"type_name": "Tool: reviewer", "comment": "Approved"}
  ],
  "metadata_comment": "Release v1.0.0",
  "scan_target_name": "demo"
}"#,
        );
        let m = load_metadata_file(f.path()).unwrap();
        assert_eq!(m.creators.len(), 3);
        assert_eq!(m.annotators.len(), 1);
        assert_eq!(m.metadata_comment.as_deref(), Some("Release v1.0.0"));
        assert_eq!(m.scan_target_name.as_deref(), Some("demo"));
    }

    #[test]
    fn load_empty_object_ok() {
        let f = write_tempfile("{}");
        let m = load_metadata_file(f.path()).unwrap();
        assert_eq!(m, MetadataFile::default());
    }

    #[test]
    fn load_unknown_top_level_field_fails() {
        let f = write_tempfile(r#"{"creator": ["Tool: T1"]}"#);
        let err = load_metadata_file(f.path()).unwrap_err();
        match err {
            BuildUserMetadataError::MetadataFileParseError { source, .. } => {
                let msg = source.to_string();
                assert!(
                    msg.contains("creator"),
                    "expected error message to name the offending field; got: {msg}"
                );
            }
            other => panic!("expected MetadataFileParseError, got {other:?}"),
        }
    }

    #[test]
    fn load_unknown_field_in_annotators_fails() {
        let f = write_tempfile(
            r#"{"annotators": [{"type_name": "Tool: T", "comment": "c", "extra": "x"}]}"#,
        );
        let err = load_metadata_file(f.path()).unwrap_err();
        assert!(matches!(
            err,
            BuildUserMetadataError::MetadataFileParseError { .. }
        ));
    }

    #[test]
    fn load_malformed_json_fails_with_position() {
        let f = write_tempfile(r#"{"creators": ["#); // truncated
        let err = load_metadata_file(f.path()).unwrap_err();
        match err {
            BuildUserMetadataError::MetadataFileParseError { source, .. } => {
                let msg = source.to_string();
                assert!(
                    source.line() > 0 || msg.contains("line") || msg.contains("column"),
                    "expected JSON parse error to carry line/column; got: {msg}"
                );
            }
            other => panic!("expected MetadataFileParseError, got {other:?}"),
        }
    }

    #[test]
    fn load_missing_file_fails_with_io_error() {
        let path = std::path::PathBuf::from("/nonexistent/mikebom-080-test.json");
        let err = load_metadata_file(&path).unwrap_err();
        assert!(matches!(
            err,
            BuildUserMetadataError::MetadataFileIo { .. }
        ));
    }
}
