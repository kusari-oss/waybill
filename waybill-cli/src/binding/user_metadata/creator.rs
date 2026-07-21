//! Milestone 080 — `--creator <Type: Name>` parsing + the [`Creator`]
//! / [`CreatorKind`] data types.

/// A creator/contributor entry on the emitted SBOM. User-supplied via
/// `--creator <Type: Name>` or via `--metadata-file`'s `creators[]`
/// array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Creator {
    pub kind: CreatorKind,
    pub name: String,
}

/// SPDX 2.3 `Creator:` field type. Case-sensitive per VR-080-001.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatorKind {
    Tool,
    Organization,
    Person,
}

impl CreatorKind {
    /// SPDX 2.3 prefix string used when serializing into
    /// `creationInfo.creators[]`.
    pub fn spdx_prefix(self) -> &'static str {
        match self {
            CreatorKind::Tool => "Tool:",
            CreatorKind::Organization => "Organization:",
            CreatorKind::Person => "Person:",
        }
    }
}

/// Errors emitted while parsing a `Type: Name` string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseCreatorError {
    /// Input was missing the `:` separator (e.g., `"Tool foo"`).
    #[error(
        "creator value {0:?} is missing the ':' separator (expected form: \
         '<Type>: <Name>' where <Type> is one of Tool, Organization, Person)"
    )]
    MissingSeparator(String),

    /// Type prefix was not one of the three SPDX 2.3 spec values.
    #[error(
        "creator value {input:?} has invalid type prefix {prefix:?}; \
         valid types are Tool, Organization, Person (case-sensitive)"
    )]
    InvalidPrefix { input: String, prefix: String },

    /// Name portion (after `:` + optional whitespace) was empty.
    #[error("creator value {0:?} has an empty Name portion")]
    EmptyName(String),

    /// Name portion contained one or more control characters
    /// (excluding plain TAB / NEWLINE if those are tolerated upstream;
    /// here we reject ALL control chars per VR-080-002).
    #[error(
        "creator value {input:?} contains a control character at byte \
         offset {byte_offset}; control characters are not permitted in \
         Name portions"
    )]
    ControlCharInName { input: String, byte_offset: usize },
}

/// Parse a `Type: Name` string into a [`Creator`].
///
/// Validation per VR-080-001 + VR-080-002:
/// - Type prefix MUST be one of `Tool`, `Organization`, `Person`
///   (case-sensitive).
/// - Whitespace between `:` and `Name` is trimmed (so `"Tool: foo"`,
///   `"Tool:foo"`, and `"Tool:   foo"` all parse to the same value).
/// - Name portion MUST be non-empty.
/// - Name portion MUST NOT contain control characters.
pub fn parse_creator_str(s: &str) -> Result<Creator, ParseCreatorError> {
    let Some(idx) = s.find(':') else {
        return Err(ParseCreatorError::MissingSeparator(s.to_string()));
    };
    let prefix = &s[..idx];
    let raw_name = &s[idx + 1..];
    let name = raw_name.trim_start();

    let kind = match prefix {
        "Tool" => CreatorKind::Tool,
        "Organization" => CreatorKind::Organization,
        "Person" => CreatorKind::Person,
        other => {
            return Err(ParseCreatorError::InvalidPrefix {
                input: s.to_string(),
                prefix: other.to_string(),
            });
        }
    };

    if name.is_empty() {
        return Err(ParseCreatorError::EmptyName(s.to_string()));
    }
    // VR-080-002 — reject control characters in the Name portion.
    // We use char::is_control rather than just ASCII to catch the
    // full Unicode Cc / control-character ranges.
    for (offset, c) in name.char_indices() {
        if c.is_control() {
            return Err(ParseCreatorError::ControlCharInName {
                input: s.to_string(),
                byte_offset: offset,
            });
        }
    }

    Ok(Creator {
        kind,
        name: name.to_string(),
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parse_tool_with_space_after_colon() {
        let c = parse_creator_str("Tool: my-pipeline").unwrap();
        assert_eq!(c.kind, CreatorKind::Tool);
        assert_eq!(c.name, "my-pipeline");
    }

    #[test]
    fn parse_tool_without_space_after_colon() {
        let c = parse_creator_str("Tool:my-pipeline").unwrap();
        assert_eq!(c.kind, CreatorKind::Tool);
        assert_eq!(c.name, "my-pipeline");
    }

    #[test]
    fn parse_tool_with_extra_whitespace() {
        let c = parse_creator_str("Tool:    spaced-name").unwrap();
        assert_eq!(c.name, "spaced-name");
    }

    #[test]
    fn parse_organization() {
        let c = parse_creator_str("Organization: ACME Corp").unwrap();
        assert_eq!(c.kind, CreatorKind::Organization);
        assert_eq!(c.name, "ACME Corp");
    }

    #[test]
    fn parse_person() {
        let c = parse_creator_str("Person: Alice <alice@example.com>").unwrap();
        assert_eq!(c.kind, CreatorKind::Person);
        assert_eq!(c.name, "Alice <alice@example.com>");
    }

    #[test]
    fn parse_invalid_prefix_bot_rejected() {
        let err = parse_creator_str("Bot: foo").unwrap_err();
        assert!(
            matches!(err, ParseCreatorError::InvalidPrefix { ref prefix, .. } if prefix == "Bot")
        );
    }

    #[test]
    fn parse_invalid_prefix_service_rejected() {
        let err = parse_creator_str("Service: foo").unwrap_err();
        assert!(matches!(
            err,
            ParseCreatorError::InvalidPrefix { ref prefix, .. } if prefix == "Service"
        ));
    }

    #[test]
    fn parse_lowercase_prefix_is_rejected() {
        // Case-sensitive per VR-080-001.
        let err = parse_creator_str("tool: foo").unwrap_err();
        assert!(matches!(
            err,
            ParseCreatorError::InvalidPrefix { ref prefix, .. } if prefix == "tool"
        ));
    }

    #[test]
    fn parse_missing_separator_rejected() {
        let err = parse_creator_str("Tool foo").unwrap_err();
        assert!(matches!(err, ParseCreatorError::MissingSeparator(_)));
    }

    #[test]
    fn parse_empty_name_rejected() {
        let err = parse_creator_str("Tool: ").unwrap_err();
        assert!(matches!(err, ParseCreatorError::EmptyName(_)));
    }

    #[test]
    fn parse_empty_name_no_whitespace_rejected() {
        let err = parse_creator_str("Tool:").unwrap_err();
        assert!(matches!(err, ParseCreatorError::EmptyName(_)));
    }

    #[test]
    fn parse_control_char_in_name_rejected() {
        let err = parse_creator_str("Tool: foo\x01bar").unwrap_err();
        assert!(matches!(
            err,
            ParseCreatorError::ControlCharInName { .. }
        ));
    }

    #[test]
    fn parse_newline_in_name_rejected() {
        let err = parse_creator_str("Tool: foo\nbar").unwrap_err();
        assert!(matches!(
            err,
            ParseCreatorError::ControlCharInName { .. }
        ));
    }

    #[test]
    fn spdx_prefix_returns_correct_strings() {
        assert_eq!(CreatorKind::Tool.spdx_prefix(), "Tool:");
        assert_eq!(CreatorKind::Organization.spdx_prefix(), "Organization:");
        assert_eq!(CreatorKind::Person.spdx_prefix(), "Person:");
    }
}
