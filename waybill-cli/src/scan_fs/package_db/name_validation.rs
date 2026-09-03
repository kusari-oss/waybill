//! Reader-agnostic package-name validation. Feature 677 (issue #768).
//!
//! Contract at `specs/677-pep508-name-validation/contracts/name-validation-module.md`.
//!
//! Motivation: waybill readers historically emit `pkg:<ecosystem>/<name>`
//! components using whatever name string a manifest declares, without
//! validating the name against the ecosystem's authoritative naming rules.
//! Cookiecutter-style project templates leave literal Jinja placeholders
//! like `{{package-name}}` in `pyproject.toml`, and pre-fix the pip reader
//! emitted `pkg:pypi/{{package-name}}@0.0.0` phantom components (issue #768).
//!
//! This module holds the reusable helper called out by spec 677 FR-004:
//! per-ecosystem name predicates + a structured error type. First cut ships
//! `is_pep508_name` / `validate_pep508_name` for the pip reader. Future
//! readers add sibling `is_<ecosystem>_name` / `validate_<ecosystem>_name`
//! functions per the extension pattern documented in the contract.
//!
//! Principle IX (Accuracy) motivation: "PURL resolution ... MUST be
//! validated before inclusion; ambiguous or low-confidence matches MUST be
//! flagged rather than silently included as definitive."

use std::sync::OnceLock;

use regex::Regex;

/// Structured failure reason for a name-validation attempt. Attached to
/// the operator-facing WARN log when a manifest's name is rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NameValidationError {
    /// Name is empty or contains only whitespace.
    Empty,
    /// Name contains characters or shape outside the ecosystem's regex.
    /// The `reason` field carries a human-readable message for the WARN log.
    Malformed { reason: String },
}

impl std::fmt::Display for NameValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NameValidationError::Empty => {
                f.write_str("name is empty or whitespace-only")
            }
            NameValidationError::Malformed { reason } => {
                write!(f, "name malformed: {reason}")
            }
        }
    }
}

/// Compile-once PEP 508 name regex.
/// PEP 508 §"Names": name MUST start and end with alphanumeric; interior
/// characters may be `A-Za-z0-9`, `.`, `-`, or `_`.
fn pep508_name_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^[A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?$")
            .expect("valid PEP 508 name regex")
    })
}

/// PEP 508 name predicate (boolean form). Returns `true` iff `name`
/// matches PEP 508's name production. Case is preserved (the character
/// class covers both A-Z and a-z explicitly).
///
/// Reference: <https://peps.python.org/pep-0508/> §"Names".
pub(crate) fn is_pep508_name(name: &str) -> bool {
    pep508_name_regex().is_match(name)
}

/// PEP 508 name validator (structured error form). Returns `Ok(())` on
/// match; otherwise returns a `NameValidationError` naming the specific
/// failure reason so the caller can emit a diagnostic WARN log.
///
/// Reason-selection is ordered most-specific to least-specific:
/// (1) empty/whitespace → `Empty`; (2) first char non-alphanumeric → "must
/// start with alphanumeric"; (3) last char non-alphanumeric → "must end
/// with alphanumeric"; (4) otherwise the name contains an invalid interior
/// character → "contains invalid character(s)".
pub(crate) fn validate_pep508_name(name: &str) -> Result<(), NameValidationError> {
    if name.trim().is_empty() {
        return Err(NameValidationError::Empty);
    }
    if is_pep508_name(name) {
        return Ok(());
    }
    // Post-match reason resolution. The regex has already rejected — we
    // just need to name WHY for the operator's diagnostic.
    let first_char = name.chars().next();
    let last_char = name.chars().next_back();
    let is_alnum = |c: char| c.is_ascii_alphanumeric();
    match (first_char, last_char) {
        (Some(c), _) if !is_alnum(c) => Err(NameValidationError::Malformed {
            reason: "must start with alphanumeric character".to_string(),
        }),
        (_, Some(c)) if !is_alnum(c) => Err(NameValidationError::Malformed {
            reason: "must end with alphanumeric character".to_string(),
        }),
        _ => Err(NameValidationError::Malformed {
            reason: "contains invalid character(s); allowed: A-Z a-z 0-9 . - _"
                .to_string(),
        }),
    }
}

// ------------------------------------------------------------------
// Unit tests — per contracts/name-validation-module.md testing table
// ------------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    // Row 1: `django` accepts
    #[test]
    fn accepts_lowercase_name() {
        assert!(is_pep508_name("django"));
        assert_eq!(validate_pep508_name("django"), Ok(()));
    }

    // Row 2: `Django` accepts (case-preserved)
    #[test]
    fn accepts_titlecase_name() {
        assert!(is_pep508_name("Django"));
        assert_eq!(validate_pep508_name("Django"), Ok(()));
    }

    // Row 3: `PyYAML` accepts (case-mixed)
    #[test]
    fn accepts_case_mixed_name() {
        assert!(is_pep508_name("PyYAML"));
        assert_eq!(validate_pep508_name("PyYAML"), Ok(()));
    }

    // Row 4: `my-pkg` accepts (hyphen separator)
    #[test]
    fn accepts_hyphen_separator() {
        assert!(is_pep508_name("my-pkg"));
        assert_eq!(validate_pep508_name("my-pkg"), Ok(()));
    }

    // Row 5: `my.pkg` accepts (dot separator)
    #[test]
    fn accepts_dot_separator() {
        assert!(is_pep508_name("my.pkg"));
        assert_eq!(validate_pep508_name("my.pkg"), Ok(()));
    }

    // Row 6: `my_pkg` accepts (underscore separator)
    #[test]
    fn accepts_underscore_separator() {
        assert!(is_pep508_name("my_pkg"));
        assert_eq!(validate_pep508_name("my_pkg"), Ok(()));
    }

    // Row 7: `zope.interface` accepts (multi-segment dotted)
    #[test]
    fn accepts_multi_segment_dotted() {
        assert!(is_pep508_name("zope.interface"));
        assert_eq!(validate_pep508_name("zope.interface"), Ok(()));
    }

    // Row 8: `""` → `Empty`
    #[test]
    fn rejects_empty() {
        assert!(!is_pep508_name(""));
        assert_eq!(validate_pep508_name(""), Err(NameValidationError::Empty));
    }

    // Row 9: `"   "` → `Empty` (whitespace-only)
    #[test]
    fn rejects_whitespace_only() {
        assert!(!is_pep508_name("   "));
        assert_eq!(
            validate_pep508_name("   "),
            Err(NameValidationError::Empty)
        );
    }

    // Row 10: `"{{package-name}}"` → `Malformed(must start with alphanumeric)`
    #[test]
    fn rejects_jinja_placeholder() {
        assert!(!is_pep508_name("{{package-name}}"));
        assert_eq!(
            validate_pep508_name("{{package-name}}"),
            Err(NameValidationError::Malformed {
                reason: "must start with alphanumeric character".to_string(),
            })
        );
    }

    // Row 11: `".pkg"` → `Malformed(must start with alphanumeric)`
    #[test]
    fn rejects_leading_separator() {
        assert!(!is_pep508_name(".pkg"));
        assert_eq!(
            validate_pep508_name(".pkg"),
            Err(NameValidationError::Malformed {
                reason: "must start with alphanumeric character".to_string(),
            })
        );
    }

    // Row 12: `"pkg-"` → `Malformed(must end with alphanumeric)`
    #[test]
    fn rejects_trailing_separator() {
        assert!(!is_pep508_name("pkg-"));
        assert_eq!(
            validate_pep508_name("pkg-"),
            Err(NameValidationError::Malformed {
                reason: "must end with alphanumeric character".to_string(),
            })
        );
    }

    // Row 13: `"pkg@2"` → `Malformed(contains invalid character(s))`
    #[test]
    fn rejects_invalid_interior_char_at_sign() {
        assert!(!is_pep508_name("pkg@2"));
        assert_eq!(
            validate_pep508_name("pkg@2"),
            Err(NameValidationError::Malformed {
                reason: "contains invalid character(s); allowed: A-Z a-z 0-9 . - _".to_string(),
            })
        );
    }

    // Row 14: `"pkg name"` → `Malformed(contains invalid character(s))`
    #[test]
    fn rejects_invalid_interior_char_whitespace() {
        assert!(!is_pep508_name("pkg name"));
        assert_eq!(
            validate_pep508_name("pkg name"),
            Err(NameValidationError::Malformed {
                reason: "contains invalid character(s); allowed: A-Z a-z 0-9 . - _".to_string(),
            })
        );
    }
}
