//! Milestone 080 — user-provided SBOM metadata.
//!
//! Operator-supplied metadata (creators, annotators, comments, scan-
//! target name) flows into emitted SBOMs at standards-native field
//! positions in CDX 1.6, SPDX 2.3, and SPDX 3. This module owns:
//!
//! - [`Creator`] / [`CreatorKind`] — `--creator <Type: Name>` parsed.
//! - [`Annotation`] — paired `--annotator` + `--annotation-comment`.
//! - [`MetadataFile`] / [`MetadataFileAnnotator`] — `--metadata-file`
//!   sidecar JSON shape.
//! - [`UserMetadata`] — aggregated structure consumed verbatim by
//!   per-format builders.
//! - [`merge_file_and_flags`] — combine file + raw CLI strings into
//!   a `UserMetadata` instance with the FR-006 conflict rules applied.
//!
//! Per Constitution Principle V the milestone audited CDX 1.6's
//! `bom.annotations[]` (research §1) and confirmed full native parity;
//! no `mikebom:` properties are introduced for this milestone.

pub mod annotation;
pub mod creator;
pub mod metadata_file;

use std::path::PathBuf;

pub use annotation::{validate_annotator_pairs, Annotation};
pub use creator::{parse_creator_str, Creator, CreatorKind, ParseCreatorError};
pub use metadata_file::{load_metadata_file, MetadataFile, MetadataFileAnnotator};

/// Errors emitted while building a [`UserMetadata`] from CLI flags +
/// `--metadata-file` input.
#[derive(Debug, thiserror::Error)]
pub enum BuildUserMetadataError {
    /// File and flag both specified the same single-valued field
    /// (`metadata_comment` or `scan_target_name`). Operator intent is
    /// ambiguous — fail with a clear diagnostic naming both sources.
    #[error(
        "{field} was specified both in --metadata-file ({file_value:?}) and via \
         the --{field} flag ({flag_value:?}); single-valued fields must come \
         from at most one source"
    )]
    ConflictError {
        field: String,
        file_value: String,
        flag_value: String,
    },

    /// A `--creator` / `--annotator` value (whether from a flag or from
    /// `--metadata-file`'s `creators[]` / `annotators[]`) failed to
    /// parse via [`parse_creator_str`].
    #[error(transparent)]
    ParseCreatorError(#[from] ParseCreatorError),

    /// `--annotator` and `--annotation-comment` did not appear in equal
    /// counts (per VR-080-003).
    #[error(
        "--annotator (count={annotator_count}) must be paired 1:1 with \
         --annotation-comment (count={comment_count}); each --annotator \
         MUST be immediately followed by exactly one --annotation-comment"
    )]
    AnnotatorPairCountMismatch {
        annotator_count: usize,
        comment_count: usize,
    },

    /// I/O failure opening / reading `--metadata-file`.
    #[error("failed to read --metadata-file {path}: {source}")]
    MetadataFileIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// `serde_json` failed to decode `--metadata-file`. Captures the
    /// path + the underlying parse-error message (which already carries
    /// line+column on JSON-syntax failures and the offending field
    /// name on `deny_unknown_fields` failures).
    #[error("failed to parse --metadata-file {path}: {source}")]
    MetadataFileParseError {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// Aggregator that the CLI parser populates from the merged
/// file-and-flag inputs. Per-format builders consume this verbatim.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserMetadata {
    /// Creators in insertion order — file entries first, then flag
    /// entries (per FR-006 + research §6).
    pub creators: Vec<Creator>,
    /// Annotations in insertion order — file entries first, then
    /// flag-pair entries.
    pub annotations: Vec<Annotation>,
    /// Single-valued document-level free-text comment.
    pub metadata_comment: Option<String>,
    /// Single-valued operator-supplied document/Sbom-level name.
    pub scan_target_name: Option<String>,
}

impl UserMetadata {
    /// Returns true iff at least one user-supplied metadata field is
    /// populated. Per-format builders can fast-skip when this is
    /// false to preserve byte-identical alpha.20 emission for the
    /// no-flag baseline (SC-010).
    pub fn is_active(&self) -> bool {
        !self.creators.is_empty()
            || !self.annotations.is_empty()
            || self.metadata_comment.is_some()
            || self.scan_target_name.is_some()
    }
}

/// Walk a raw CLI argument vector once and verify strict positional
/// interleaving of `--annotator` / `--annotation-comment` pairs per
/// the milestone-080 Q1 clarification: each `--annotator` MUST be
/// immediately followed by exactly one `--annotation-comment`. The
/// check is best-effort UX-friendly per research §3 — it surfaces the
/// `--annotator A --annotator B --annotation-comment C` typo case as
/// an early error before clap's parallel `Vec<String>` collection
/// flattens the ordering. Returns the bad position+token on failure
/// so the caller can format an actionable error.
///
/// Tolerates other flags interspersed; only the `--annotator` ↔
/// `--annotation-comment` adjacency is enforced.
pub fn validate_annotator_pair_interleaving<I, S>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let argv: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    let mut i = 0;
    while i < argv.len() {
        let tok = &argv[i];
        let is_annotator = tok == "--annotator" || tok.starts_with("--annotator=");
        if is_annotator {
            // The "next pair item" is either the next argv entry (when
            // the operator wrote `--annotator <VAL>`) or the same entry
            // (when they wrote `--annotator=<VAL>`); either way the next
            // *flag* should be `--annotation-comment`.
            let comment_idx = if tok.starts_with("--annotator=") {
                i + 1
            } else {
                i + 2
            };
            let next_flag: Option<&String> = argv.get(comment_idx);
            let ok = next_flag
                .map(|s| s == "--annotation-comment" || s.starts_with("--annotation-comment="))
                .unwrap_or(false);
            if !ok {
                return Err(format!(
                    "expected --annotation-comment to immediately follow --annotator at \
                     position {i}; found {:?}. Each --annotator MUST be paired \
                     with exactly one --annotation-comment per the milestone-080 \
                     positional-pairing clarification.",
                    next_flag.map(|s| s.as_str()).unwrap_or("<end of args>")
                ));
            }
        }
        i += 1;
    }
    Ok(())
}

/// Build a [`UserMetadata`] from CLI flags + optional `--metadata-file`.
///
/// Per FR-006 + research §6:
/// - `creators` = `file.creators` parsed + `flag_creators` parsed,
///   in that order (file first, then flag).
/// - `annotations` = `file.annotators` parsed + `flag_annotators` /
///   `flag_annotation_comments` paired by index, in that order.
/// - `metadata_comment` / `scan_target_name`: if BOTH file AND flag
///   are `Some`, fail with [`BuildUserMetadataError::ConflictError`].
///   Otherwise use whichever is `Some` (or `None`).
///
/// `emission_timestamp` sets all annotations' `timestamp` field
/// uniformly (matches the SBOM's `creationInfo.created` value).
pub fn merge_file_and_flags(
    file: Option<MetadataFile>,
    flag_creators: Vec<String>,
    flag_annotators: Vec<String>,
    flag_annotation_comments: Vec<String>,
    flag_metadata_comment: Option<String>,
    flag_scan_target_name: Option<String>,
    emission_timestamp: chrono::DateTime<chrono::Utc>,
) -> Result<UserMetadata, BuildUserMetadataError> {
    // VR-080-003 — annotator/annotation-comment pair-count check on
    // the flag values BEFORE we touch file values.
    validate_annotator_pairs(&flag_annotators, &flag_annotation_comments)
        .map_err(|(annotator_count, comment_count)| {
            BuildUserMetadataError::AnnotatorPairCountMismatch {
                annotator_count,
                comment_count,
            }
        })?;

    let mut creators: Vec<Creator> = Vec::new();
    let mut annotations: Vec<Annotation> = Vec::new();

    // File creators / annotators come first (research §6).
    if let Some(ref f) = file {
        for raw in &f.creators {
            creators.push(parse_creator_str(raw)?);
        }
        for entry in &f.annotators {
            let annotator = parse_creator_str(&entry.type_name)?;
            annotations.push(Annotation {
                annotator,
                comment: entry.comment.clone(),
                timestamp: emission_timestamp,
            });
        }
    }

    // Flag creators next.
    for raw in &flag_creators {
        creators.push(parse_creator_str(raw)?);
    }

    // Flag annotator/annotation-comment pairs by index.
    for (annotator_raw, comment) in flag_annotators
        .iter()
        .zip(flag_annotation_comments.iter())
    {
        let annotator = parse_creator_str(annotator_raw)?;
        annotations.push(Annotation {
            annotator,
            comment: comment.clone(),
            timestamp: emission_timestamp,
        });
    }

    // Conflict resolution on single-valued fields (VR-080-005).
    let metadata_comment = match (
        file.as_ref().and_then(|f| f.metadata_comment.clone()),
        flag_metadata_comment,
    ) {
        (Some(file_v), Some(flag_v)) => {
            return Err(BuildUserMetadataError::ConflictError {
                field: "metadata_comment".to_string(),
                file_value: file_v,
                flag_value: flag_v,
            });
        }
        (Some(v), None) | (None, Some(v)) => Some(v),
        (None, None) => None,
    };
    let scan_target_name = match (
        file.as_ref().and_then(|f| f.scan_target_name.clone()),
        flag_scan_target_name,
    ) {
        (Some(file_v), Some(flag_v)) => {
            return Err(BuildUserMetadataError::ConflictError {
                field: "scan_target_name".to_string(),
                file_value: file_v,
                flag_value: flag_v,
            });
        }
        (Some(v), None) | (None, Some(v)) => Some(v),
        (None, None) => None,
    };

    Ok(UserMetadata {
        creators,
        annotations,
        metadata_comment,
        scan_target_name,
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn ts() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap()
    }

    #[test]
    fn merge_with_no_inputs_is_inactive() {
        let r = merge_file_and_flags(None, vec![], vec![], vec![], None, None, ts())
            .unwrap();
        assert!(!r.is_active());
    }

    #[test]
    fn merge_flag_only_creator() {
        let r = merge_file_and_flags(
            None,
            vec!["Tool: my-pipeline".to_string()],
            vec![],
            vec![],
            None,
            None,
            ts(),
        )
        .unwrap();
        assert_eq!(r.creators.len(), 1);
        assert_eq!(r.creators[0].kind, CreatorKind::Tool);
        assert_eq!(r.creators[0].name, "my-pipeline");
    }

    #[test]
    fn merge_file_creators_precede_flag_creators() {
        let file = MetadataFile {
            creators: vec!["Tool: A".into(), "Tool: B".into()],
            annotators: vec![],
            metadata_comment: None,
            scan_target_name: None,
        };
        let r = merge_file_and_flags(
            Some(file),
            vec!["Tool: C".into(), "Tool: D".into()],
            vec![],
            vec![],
            None,
            None,
            ts(),
        )
        .unwrap();
        let names: Vec<&str> = r.creators.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["A", "B", "C", "D"]);
    }

    #[test]
    fn merge_conflict_on_metadata_comment_fails() {
        let file = MetadataFile {
            creators: vec![],
            annotators: vec![],
            metadata_comment: Some("file-X".into()),
            scan_target_name: None,
        };
        let err = merge_file_and_flags(
            Some(file),
            vec![],
            vec![],
            vec![],
            Some("flag-Y".into()),
            None,
            ts(),
        )
        .unwrap_err();
        match err {
            BuildUserMetadataError::ConflictError {
                field,
                file_value,
                flag_value,
            } => {
                assert_eq!(field, "metadata_comment");
                assert_eq!(file_value, "file-X");
                assert_eq!(flag_value, "flag-Y");
            }
            other => panic!("expected ConflictError, got {other:?}"),
        }
    }

    #[test]
    fn merge_conflict_on_scan_target_name_fails() {
        let file = MetadataFile {
            creators: vec![],
            annotators: vec![],
            metadata_comment: None,
            scan_target_name: Some("file-S".into()),
        };
        let err = merge_file_and_flags(
            Some(file),
            vec![],
            vec![],
            vec![],
            None,
            Some("flag-S".into()),
            ts(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            BuildUserMetadataError::ConflictError { ref field, .. } if field == "scan_target_name"
        ));
    }

    #[test]
    fn merge_pair_count_mismatch_fails() {
        let err = merge_file_and_flags(
            None,
            vec![],
            vec!["Tool: A".into(), "Tool: B".into()],
            vec!["only-one-comment".into()],
            None,
            None,
            ts(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            BuildUserMetadataError::AnnotatorPairCountMismatch {
                annotator_count: 2,
                comment_count: 1
            }
        ));
    }

    #[test]
    fn interleaving_ok_for_paired_flags() {
        let argv = vec![
            "mikebom",
            "sbom",
            "scan",
            "--path",
            ".",
            "--annotator",
            "Tool: A",
            "--annotation-comment",
            "X",
            "--annotator",
            "Tool: B",
            "--annotation-comment",
            "Y",
        ];
        assert!(validate_annotator_pair_interleaving(argv).is_ok());
    }

    #[test]
    fn interleaving_rejects_two_annotators_one_comment() {
        let argv = vec![
            "--annotator",
            "Tool: A",
            "--annotator",
            "Tool: B",
            "--annotation-comment",
            "C",
        ];
        let err = validate_annotator_pair_interleaving(argv).unwrap_err();
        assert!(err.contains("--annotation-comment"), "got: {err}");
    }

    #[test]
    fn interleaving_rejects_dangling_annotator() {
        let argv = vec!["--annotator", "Tool: A"];
        assert!(validate_annotator_pair_interleaving(argv).is_err());
    }

    #[test]
    fn interleaving_handles_equals_form() {
        let argv = vec!["--annotator=Tool: A", "--annotation-comment=X"];
        assert!(validate_annotator_pair_interleaving(argv).is_ok());
    }

    #[test]
    fn interleaving_no_annotator_is_ok() {
        let argv = vec!["mikebom", "sbom", "scan", "--path", "."];
        assert!(validate_annotator_pair_interleaving(argv).is_ok());
    }

    #[test]
    fn merge_file_annotators_precede_flag_annotators() {
        let file = MetadataFile {
            creators: vec![],
            annotators: vec![MetadataFileAnnotator {
                type_name: "Tool: from-file".into(),
                comment: "file comment".into(),
            }],
            metadata_comment: None,
            scan_target_name: None,
        };
        let r = merge_file_and_flags(
            Some(file),
            vec![],
            vec!["Tool: from-flag".into()],
            vec!["flag comment".into()],
            None,
            None,
            ts(),
        )
        .unwrap();
        assert_eq!(r.annotations.len(), 2);
        assert_eq!(r.annotations[0].annotator.name, "from-file");
        assert_eq!(r.annotations[1].annotator.name, "from-flag");
    }
}
