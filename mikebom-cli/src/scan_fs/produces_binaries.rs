//! Shared helper for normalizing `mikebom:produces-binaries` declarations
//! (milestone 116, Option B of issue #225).
//!
//! Every per-ecosystem main-module extractor (Cargo, npm, pip, gem, maven, Go)
//! collects candidate binary names from its ecosystem's manifest or filesystem
//! layout, then passes the candidates through [`normalize_produces_binaries`]
//! to produce the canonical form per `specs/116-produces-binaries/contracts/property.md`:
//!
//! - Lowercase ASCII (non-ASCII characters dropped per the `^[a-z0-9][a-z0-9_-]*$` invariant)
//! - Trailing `.exe` / `.jar` suffixes stripped (case-insensitive)
//! - Sorted lex (byte-deterministic across hosts)
//! - Deduped
//! - Empty entries removed
//!
//! The binder owns ALL platform-suffix translation (see
//! `binding::verify::SourceSbomContext::binding_for_purl`); extractors never
//! emit `.exe` or `.jar` suffixes.

use std::collections::BTreeSet;

/// Canonical-form normalization for `mikebom:produces-binaries` declarations.
///
/// Returns the input names lowercased, with `.exe` / `.jar` suffixes stripped,
/// filtered to the `[a-z0-9][a-z0-9_-]*` shape invariant, deduped, and lex-sorted.
/// Empty input or all-invalid input produces an empty `Vec`; callers MUST omit
/// the SBOM property entirely when the result is empty (FR-001).
pub(crate) fn normalize_produces_binaries<I, S>(names: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut set: BTreeSet<String> = BTreeSet::new();
    for raw in names {
        let lower = raw.as_ref().to_ascii_lowercase();
        let stripped = strip_known_suffix(&lower);
        let cleaned = filter_to_shape(stripped);
        if !cleaned.is_empty() {
            set.insert(cleaned);
        }
    }
    set.into_iter().collect()
}

fn strip_known_suffix(name: &str) -> &str {
    if let Some(stem) = name.strip_suffix(".exe") {
        return stem;
    }
    if let Some(stem) = name.strip_suffix(".jar") {
        return stem;
    }
    name
}

fn filter_to_shape(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for (i, c) in name.chars().enumerate() {
        let keep = match c {
            'a'..='z' | '0'..='9' => true,
            '_' | '-' => i > 0,
            _ => false,
        };
        if keep {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_empty() {
        let out: Vec<String> = normalize_produces_binaries::<_, &str>(std::iter::empty());
        assert!(out.is_empty());
    }

    #[test]
    fn lowercases_mixed_case() {
        let out = normalize_produces_binaries(["Baz", "BAZ-CLI"]);
        assert_eq!(out, vec!["baz".to_string(), "baz-cli".to_string()]);
    }

    #[test]
    fn strips_known_suffixes() {
        let out = normalize_produces_binaries(["baz.exe", "baz.JAR", "baz"]);
        assert_eq!(out, vec!["baz".to_string()]);
    }

    #[test]
    fn dedupes_after_normalization() {
        let out = normalize_produces_binaries(["baz", "BAZ", "baz.exe", "baz.jar"]);
        assert_eq!(out, vec!["baz".to_string()]);
    }

    #[test]
    fn sorts_lexicographically() {
        let out = normalize_produces_binaries(["zeta", "alpha", "mid"]);
        assert_eq!(
            out,
            vec!["alpha".to_string(), "mid".to_string(), "zeta".to_string()]
        );
    }

    #[test]
    fn drops_invalid_leading_chars() {
        // Leading `-` or `_` are not allowed by the shape invariant.
        let out = normalize_produces_binaries(["-baz", "_baz", "baz"]);
        assert_eq!(out, vec!["baz".to_string()]);
    }

    #[test]
    fn strips_non_ascii() {
        let out = normalize_produces_binaries(["bäz", "café"]);
        // bäz → bz, café → caf — both valid shapes after stripping.
        assert_eq!(out, vec!["bz".to_string(), "caf".to_string()]);
    }

    #[test]
    fn preserves_dash_and_underscore() {
        let out = normalize_produces_binaries(["baz-cli", "baz_lib"]);
        assert_eq!(out, vec!["baz-cli".to_string(), "baz_lib".to_string()]);
    }

    #[test]
    fn empty_string_filtered_out() {
        let out = normalize_produces_binaries(["", "baz"]);
        assert_eq!(out, vec!["baz".to_string()]);
    }
}
