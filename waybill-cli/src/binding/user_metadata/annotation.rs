//! Milestone 080 — `--annotator` + `--annotation-comment` paired
//! annotation data type and pair-count validator.

use super::creator::Creator;

/// A document-level annotation entry. User-supplied via
/// `--annotator <Type: Name> --annotation-comment <text>` pairs or via
/// `--metadata-file`'s `annotators[]` array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    pub annotator: Creator,
    pub comment: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Validate that `--annotator` and `--annotation-comment` were
/// supplied in equal counts (per VR-080-003 / FR-003).
///
/// Returns `Ok(())` on equal counts; returns
/// `Err((annotator_count, comment_count))` on mismatch so the caller
/// can format an operator-friendly diagnostic via
/// [`crate::binding::user_metadata::BuildUserMetadataError::AnnotatorPairCountMismatch`].
pub fn validate_annotator_pairs(
    annotator: &[String],
    comment: &[String],
) -> Result<(), (usize, usize)> {
    if annotator.len() != comment.len() {
        return Err((annotator.len(), comment.len()));
    }
    Ok(())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn equal_lengths_ok() {
        assert!(validate_annotator_pairs(
            &["Tool: A".into(), "Tool: B".into()],
            &["X".into(), "Y".into()]
        )
        .is_ok());
    }

    #[test]
    fn empty_lists_ok() {
        assert!(validate_annotator_pairs(&[], &[]).is_ok());
    }

    #[test]
    fn one_annotator_no_comment_fails() {
        let err = validate_annotator_pairs(&["Tool: A".into()], &[])
            .unwrap_err();
        assert_eq!(err, (1, 0));
    }

    #[test]
    fn no_annotator_one_comment_fails() {
        let err = validate_annotator_pairs(&[], &["X".into()]).unwrap_err();
        assert_eq!(err, (0, 1));
    }

    #[test]
    fn two_annotator_one_comment_fails() {
        let err = validate_annotator_pairs(
            &["Tool: A".into(), "Tool: B".into()],
            &["X".into()],
        )
        .unwrap_err();
        assert_eq!(err, (2, 1));
    }
}
