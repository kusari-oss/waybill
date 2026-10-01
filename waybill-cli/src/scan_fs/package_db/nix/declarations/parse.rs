//! One `meta.knownVulnerabilities` entry, and the identifiers inside it.
//!
//! Measured across a 46-attribute sample: 72% of entries name a CVE, 28% do
//! not. The two halves go to different places — CVE-bearing entries become
//! VEX, prose entries become a per-component annotation — but the split is a
//! property of the text, not of two different attributes, so both come
//! through here.

use std::sync::OnceLock;

use regex::Regex;

/// One entry, with whatever identifiers its text names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// Verbatim. The maintainer's words are the value of the prose half —
    /// "Includes vulnerable versions of bundled libraries: openssl, ffmpeg,
    /// gdal, and proj" says something no identifier could, and reducing it to
    /// a flag would discard exactly what makes it worth emitting.
    pub text: String,
    /// Identifiers named by `text`. Usually one, often none, occasionally
    /// several. Extraction does not consume the text.
    pub cves: Vec<String>,
}

impl Declaration {
    pub fn parse(text: &str) -> Self {
        Self {
            text: text.to_string(),
            cves: cves_in(text),
        }
    }

    /// Whether this entry becomes VEX (it names something) or an annotation
    /// (it does not).
    pub fn names_a_cve(&self) -> bool {
        !self.cves.is_empty()
    }
}

fn cve_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"CVE-\d{4}-\d+").expect("static CVE pattern is valid"))
}

/// Identifiers named anywhere in the text, deduplicated, in order of first
/// appearance.
///
/// Most real entries embed the identifier in a sentence — "CVE-2019-9501:
/// heap buffer overflow, potentially allowing remote code execution" — so
/// this scans rather than anchoring, and the caller keeps the sentence.
pub fn cves_in(text: &str) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    cve_pattern()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn an_identifier_inside_prose_is_extracted_without_consuming_the_prose() {
        // The common shape: 72% of measured entries look like this, and the
        // description is the half a consumer can act on.
        let d = Declaration::parse(
            "CVE-2019-9501: heap buffer overflow, potentially allowing remote \
             code execution by sending specially-crafted WiFi packets",
        );
        assert_eq!(d.cves, vec!["CVE-2019-9501"]);
        assert!(
            d.text.contains("heap buffer overflow"),
            "the description was consumed by extraction: {:?}",
            d.text
        );
        assert!(d.names_a_cve());
    }

    #[test]
    fn an_entry_naming_several_identifiers_yields_each_of_them() {
        // FR-004: each becomes its own statement, rather than one statement
        // carrying a concatenated subject.
        let d = Declaration::parse("fixed by CVE-2014-8139, CVE-2014-8140 and CVE-2014-8141");
        assert_eq!(d.cves, vec!["CVE-2014-8139", "CVE-2014-8140", "CVE-2014-8141"]);
    }

    #[test]
    fn a_repeated_identifier_is_named_once() {
        let d = Declaration::parse("CVE-2021-4217 — see CVE-2021-4217 upstream");
        assert_eq!(d.cves, vec!["CVE-2021-4217"]);
    }

    #[test]
    fn prose_naming_nothing_still_parses_and_keeps_its_text() {
        // 28% of measured entries. These are the ones no feed carries:
        // bundled components and upstream abandonment.
        let d = Declaration::parse(
            "Includes vulnerable versions of bundled libraries: openssl, ffmpeg, gdal, and proj.",
        );
        assert!(d.cves.is_empty());
        assert!(!d.names_a_cve());
        assert!(d.text.contains("openssl"), "the text must survive verbatim");
    }

    #[test]
    fn a_version_like_string_is_not_mistaken_for_an_identifier() {
        // Real entries sit beside version literals ("2.92", "5.2.4"); the
        // pattern must not fire on them.
        let d = Declaration::parse("Garage version 2.92 is EOL");
        assert!(d.cves.is_empty(), "matched: {:?}", d.cves);
    }
}
