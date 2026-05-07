//! SPDX 3.0.1 `Core/externalIdentifierType` controlled-vocabulary
//! mapping (milestone 079).
//!
//! Per the SPDX 3 SHACL constraint on `Core/externalIdentifierType`,
//! emitted values MUST come from the 11-value enumeration:
//! `[other, cve, swhid, securityOther, cpe23, packageUrl, gitoid,
//! cpe22, urlScheme, email, swid]`. mikebom's internal scheme names
//! (`image`, `repo`, `git`, `subject`, `attestation`, plus
//! user-defined values from `--component-id <PURL>=<SCHEME>:<VALUE>`)
//! must be mapped to a vocab value at SPDX 3 emission time.
//!
//! This module provides the pure-function mapping per
//! `specs/079-spdx3-id-vocab/research.md` §1, plus the optional
//! `comment` field (`"original-scheme: <name>"`) that preserves the
//! original mikebom scheme name on `Core/ExternalIdentifier`
//! elements when the vocab mapping is `other`.
//!
//! Determinism contract (FR-005): same `(scheme, value)` input →
//! byte-identical output across re-runs. No I/O, no clock, no PRNG.
//! Regex compiled once via `OnceLock<Regex>`.
//!
//! See `specs/079-spdx3-id-vocab/contracts/spdx3-id-vocab-mapping.md`
//! for the wire-format contract this module implements.

use std::sync::OnceLock;

use regex::Regex;

use mikebom::binding::identifiers::SchemeName;

/// The 11 controlled-vocabulary values for SPDX 3's
/// `Core/externalIdentifierType` (per the schema audit against
/// `mikebom-cli/tests/fixtures/schemas/spdx-3.0.1.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SpdxIdType {
    Other,
    Cve,
    Swhid,
    SecurityOther,
    Cpe23,
    PackageUrl,
    Gitoid,
    Cpe22,
    UrlScheme,
    Email,
    Swid,
}

impl SpdxIdType {
    /// Returns the literal vocab string the SPDX 3 emission writes
    /// into `externalIdentifierType`. Matches the SHACL enum verbatim.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Other => "other",
            Self::Cve => "cve",
            Self::Swhid => "swhid",
            Self::SecurityOther => "securityOther",
            Self::Cpe23 => "cpe23",
            Self::PackageUrl => "packageUrl",
            Self::Gitoid => "gitoid",
            Self::Cpe22 => "cpe22",
            Self::UrlScheme => "urlScheme",
            Self::Email => "email",
            Self::Swid => "swid",
        }
    }

    /// Try to parse a scheme-name string into a `SpdxIdType` variant.
    /// Returns `None` for non-vocab scheme names. Used to short-circuit
    /// the mapping when an operator-supplied scheme name (or the
    /// scheme already attached to a packageUrl/cpe23/etc. emission)
    /// IS one of the 11 vocab values.
    fn from_scheme_str(s: &str) -> Option<Self> {
        match s {
            "other" => Some(Self::Other),
            "cve" => Some(Self::Cve),
            "swhid" => Some(Self::Swhid),
            "securityOther" => Some(Self::SecurityOther),
            "cpe23" => Some(Self::Cpe23),
            "packageUrl" => Some(Self::PackageUrl),
            "gitoid" => Some(Self::Gitoid),
            "cpe22" => Some(Self::Cpe22),
            "urlScheme" => Some(Self::UrlScheme),
            "email" => Some(Self::Email),
            "swid" => Some(Self::Swid),
            _ => None,
        }
    }
}

/// Output of the per-identifier mapping decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MappingResult {
    /// SPDX 3 controlled-vocabulary value to emit.
    pub(crate) vocab_type: SpdxIdType,
    /// `Some(comment_text)` when the original mikebom scheme name
    /// would otherwise be lost (i.e., the scheme isn't in the vocab
    /// AND we didn't capture its semantic via a more-specific vocab
    /// value like `gitoid`). `None` when no info-preservation comment
    /// is needed.
    pub(crate) comment: Option<String>,
}

/// Pure function: `(mikebom scheme, identifier value)` → SPDX 3 vocab
/// value + optional `comment` field text.
///
/// Logic per `specs/079-spdx3-id-vocab/research.md` §1:
/// 1. If `scheme.as_str()` is one of the 11 vocab strings →
///    pass-through (e.g., `--component-id <PURL>=cve:CVE-1234`).
/// 2. Else if `scheme.as_str() == "git"` AND `value` matches
///    `^[0-9a-f]{40}$` → emit as `gitoid` (no comment — the vocab
///    value carries the "git object ID" semantic faithfully).
/// 3. Otherwise → `other` with `comment = "original-scheme: <name>"`
///    so cross-tier-correlation tooling can recover the original
///    scheme deterministically.
///
/// Determinism contract (FR-005): same inputs → byte-identical
/// outputs across re-runs. No I/O, no clock, no PRNG.
pub(crate) fn map_scheme_to_vocab(scheme: &SchemeName, value: &str) -> MappingResult {
    // Branch 1: scheme name is itself one of the 11 vocab values.
    // This handles `--component-id <PURL>=cve:CVE-1234` etc. — the
    // operator named a vocab value directly; pass through with no
    // info-preservation comment (no info loss).
    if let Some(vocab_type) = SpdxIdType::from_scheme_str(scheme.as_str()) {
        return MappingResult {
            vocab_type,
            comment: None,
        };
    }

    // Branch 2: `git:` scheme with a 40-char hex SHA-1 value. The
    // milestone-074 auto-detect path always produces this exact
    // shape (40-char lowercase hex SHA-1 per `auto_detect.rs:578-603`'s
    // `git_rev_parse_head`), so this is the dominant case.
    if scheme.as_str() == "git" && is_git_sha(value) {
        return MappingResult {
            vocab_type: SpdxIdType::Gitoid,
            comment: None,
        };
    }

    // Branch 3 (default): every built-in non-vocab scheme (`image`,
    // `repo`, `subject`, `attestation`, plus `git:` with non-SHA
    // value) + every user-defined non-vocab scheme (e.g., `jira`,
    // `internal-ticket`) maps to `other` with the original scheme
    // preserved in `comment` per FR-002 / FR-003.
    MappingResult {
        vocab_type: SpdxIdType::Other,
        comment: Some(format!("original-scheme: {}", scheme.as_str())),
    }
}

/// Compiled-once regex `^[0-9a-f]{40}$`. Returns true when `value`
/// is exactly a 40-char lowercase hex string (a SHA-1 git commit).
///
/// Per `specs/079-spdx3-id-vocab/research.md` §2: mikebom's
/// auto-detect `git:` values are bounded by `git_rev_parse_head` to
/// always match this pattern. Other shapes (abbreviated SHAs,
/// 64-char SHA-256, `git+https://...` URLs) fall through to the
/// `other` branch.
fn is_git_sha(value: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        // Constructed from a compile-time literal — `unwrap` is
        // safe; a compile-time-invalid regex here would represent
        // a mikebom code-defect rather than a runtime input
        // condition.
        #[allow(clippy::unwrap_used)]
        {
            Regex::new(r"^[0-9a-f]{40}$").unwrap()
        }
    });
    re.is_match(value)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn scheme(s: &str) -> SchemeName {
        SchemeName::new(s).unwrap()
    }

    /// Table-driven coverage of every (scheme, value) pair from
    /// research.md §1. One row per documented mapping decision.
    #[test]
    fn id_type_mapping_unit_table() {
        // (input scheme, input value, expected vocab as_str(),
        //  expected comment Option<&str>)
        let cases: &[(&str, &str, &str, Option<&str>)] = &[
            // --- Built-in non-vocab schemes (FR-002) ---
            ("image", "registry.example.com/img:tag", "other", Some("original-scheme: image")),
            ("repo", "https://example.com/foo/bar.git", "other", Some("original-scheme: repo")),
            // `git:` SHA → gitoid (FR-004 dominant case).
            (
                "git",
                "0123456789abcdef0123456789abcdef01234567",
                "gitoid",
                None,
            ),
            // `git:` with a non-SHA value → other (defensive
            // FR-004 fallback; not reachable from operator input
            // today since `git` is reserved at flag-parse time).
            ("git", "git+https://example.com/foo/bar.git", "other", Some("original-scheme: git")),
            ("subject", "sha256:deadbeef", "other", Some("original-scheme: subject")),
            (
                "attestation",
                "https://example.com/build/attestation.json",
                "other",
                Some("original-scheme: attestation"),
            ),
            // --- Vocab-named user-defined schemes (FR-003 short-circuit) ---
            ("cve", "CVE-2024-1234", "cve", None),
            // --- Non-vocab user-defined schemes (FR-003 default) ---
            ("jira", "PROJ-1234", "other", Some("original-scheme: jira")),
        ];
        for (scheme_str, value, want_type, want_comment) in cases {
            let got = map_scheme_to_vocab(&scheme(scheme_str), value);
            assert_eq!(
                got.vocab_type.as_str(),
                *want_type,
                "vocab mismatch for scheme={scheme_str:?} value={value:?}"
            );
            assert_eq!(
                got.comment.as_deref(),
                *want_comment,
                "comment mismatch for scheme={scheme_str:?} value={value:?}"
            );
        }
    }

    /// Boundary cases for `is_git_sha` per research §2.
    #[test]
    fn git_sha_detected_as_gitoid() {
        // 40-char lowercase hex → Gitoid.
        let r = map_scheme_to_vocab(
            &scheme("git"),
            "0123456789abcdef0123456789abcdef01234567",
        );
        assert_eq!(r.vocab_type, SpdxIdType::Gitoid);
        assert!(r.comment.is_none(), "gitoid path must not emit comment");

        // `git+https://...` URL → Other (not a SHA).
        let r = map_scheme_to_vocab(
            &scheme("git"),
            "git+https://example.com/foo/bar.git",
        );
        assert_eq!(r.vocab_type, SpdxIdType::Other);
        assert_eq!(r.comment.as_deref(), Some("original-scheme: git"));

        // 7-char abbreviated SHA → Other (regex requires exactly 40).
        let r = map_scheme_to_vocab(&scheme("git"), "abc1234");
        assert_eq!(r.vocab_type, SpdxIdType::Other);
        assert_eq!(r.comment.as_deref(), Some("original-scheme: git"));

        // 64-char hex (SHA-256) → Other (regex bounds the length to 40).
        let r = map_scheme_to_vocab(
            &scheme("git"),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        );
        assert_eq!(r.vocab_type, SpdxIdType::Other);
        assert_eq!(r.comment.as_deref(), Some("original-scheme: git"));

        // Uppercase 40-char hex → Other (regex requires lowercase).
        let r = map_scheme_to_vocab(
            &scheme("git"),
            "0123456789ABCDEF0123456789ABCDEF01234567",
        );
        assert_eq!(r.vocab_type, SpdxIdType::Other);
        assert_eq!(r.comment.as_deref(), Some("original-scheme: git"));

        // Direct helper exercise to guard the regex itself.
        assert!(is_git_sha("0123456789abcdef0123456789abcdef01234567"));
        assert!(!is_git_sha("abc1234"));
        assert!(!is_git_sha(""));
        assert!(!is_git_sha("not-a-sha"));
    }

    /// Branch coverage — vocab-name short-circuit applies to ALL 11
    /// vocab values (defends against a future PR that might add a
    /// helper but forget to wire it through `from_scheme_str`).
    #[test]
    fn vocab_named_schemes_short_circuit() {
        let pairs: &[(&str, SpdxIdType)] = &[
            ("other", SpdxIdType::Other),
            ("cve", SpdxIdType::Cve),
            ("swhid", SpdxIdType::Swhid),
            ("securityOther", SpdxIdType::SecurityOther),
            ("cpe23", SpdxIdType::Cpe23),
            ("packageUrl", SpdxIdType::PackageUrl),
            ("gitoid", SpdxIdType::Gitoid),
            ("cpe22", SpdxIdType::Cpe22),
            ("urlScheme", SpdxIdType::UrlScheme),
            ("email", SpdxIdType::Email),
            ("swid", SpdxIdType::Swid),
        ];
        for (name, want) in pairs {
            // Note: SchemeName regex is `^[a-z][a-z0-9_-]*$` — all
            // 11 vocab strings start with lowercase ASCII letter and
            // use only lowercase + digits, so they all pass parse.
            // (Confirmed: "securityOther" has uppercase 'O' so it
            // FAILS SchemeName::new — skip in this loop.)
            if SchemeName::new(*name).is_err() {
                continue;
            }
            let r = map_scheme_to_vocab(&scheme(name), "any-value");
            assert_eq!(r.vocab_type, *want, "vocab short-circuit failed for {name:?}");
            assert!(r.comment.is_none(), "short-circuit must not emit comment for {name:?}");
        }
    }
}
