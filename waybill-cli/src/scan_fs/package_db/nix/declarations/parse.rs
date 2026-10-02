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
    /// Advisory identifiers named by `text`: CVE, GHSA, or a vendor's
    /// `<Vendor>-YYYY-N` (#1051). Usually one, often none, occasionally
    /// several. Extraction does not consume the text.
    pub identifiers: Vec<String>,
}

impl Declaration {
    pub fn parse(text: &str) -> Self {
        Self {
            text: text.to_string(),
            identifiers: identifiers_in(text),
        }
    }

    /// Whether this entry becomes VEX (it names an advisory) or an
    /// annotation (it does not).
    pub fn names_an_identifier(&self) -> bool {
        !self.identifiers.is_empty()
    }

    /// The CVE subset of [`Self::identifiers`]. Feeds C188's CVE-only
    /// fields, which keep their meaning under #1051 (FR-011a).
    pub fn cves(&self) -> impl Iterator<Item = &str> {
        self.identifiers.iter().map(String::as_str).filter(|id| id.starts_with("CVE-"))
    }

    /// Whether this declaration's subject is a **party** rather than the
    /// software.
    ///
    /// nixpkgs occasionally records why a package is distrusted in terms of
    /// who now controls it. Those are a different kind of statement from the
    /// rest: they attribute conduct or standing to an organisation, where
    /// every other entry describes the artifact -- what it bundles, what it
    /// vendors, whether it is still maintained. The spec deferred emitting
    /// them, and an SBOM that reproduces a judgement about a company into
    /// every consumer's document is not the place to settle it.
    ///
    /// Withheld, not reclassified: the declaration is still read, still
    /// counted, and the withholding is logged, so an operator can tell this
    /// apart from nixpkgs having said nothing.
    ///
    /// **This is a phrase match over free text, not comprehension.**
    /// Measured against every `knownVulnerabilities` entry in one nixpkgs
    /// revision: 27 distinct prose declarations, of which exactly two are
    /// party claims, and both are caught --
    ///
    /// * "... was acquired by <company>, a company distrusted by the
    ///   community"
    /// * "Please nag <company> to update to OpenSSL 3 for Darwin."
    ///
    /// New phrasing upstream will not match, and will be emitted. That is
    /// the intended failure direction: this withholds a narrow, named class
    /// rather than guessing at a broad one, because over-withholding would
    /// silently drop the bundled-component declarations that are the whole
    /// reason prose is carried at all.
    pub fn is_about_a_party(&self) -> bool {
        let t = self.text.to_ascii_lowercase();
        // Ownership and standing claims.
        if t.contains("acquired by") || t.contains("distrusted by") {
            return true;
        }
        // Directed at a vendor rather than describing the artifact.
        if t.contains("please nag") {
            return true;
        }
        false
    }
}

/// The advisory-identifier shapes nixpkgs actually uses (#1051).
///
/// Chosen from a whole-tree census rather than from what might occur
/// (`specs/1050-nixpkgs-security-vex/measurements/kv-identifier-census.py`,
/// README Q5). At nixpkgs `a799d3e3`, across 159 entries: CVE, GHSA (three,
/// in one entry) and `Sonatype-YYYY-NNNN` (two) cover every identifier, and
/// no prose entry contains any other `<word>-<digits>` token. The vendor
/// arm requires a capitalised name, a 19xx/20xx year and at least three
/// digits, so a version string ("Electron 38.8.4") or a bare year cannot
/// reach it.
fn identifier_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\bCVE-\d{4}-\d+\b|\bGHSA(?:-[23456789cfghjmpqrvwx]{4}){3}\b|\b[A-Z][A-Za-z]{1,15}-(?:19|20)\d{2}-\d{3,}\b",
        )
        .expect("static identifier pattern is valid")
    })
}

/// Identifiers named anywhere in the text, deduplicated, in order of first
/// appearance.
///
/// Most real entries embed the identifier in a sentence — "CVE-2019-9501:
/// heap buffer overflow, potentially allowing remote code execution" — so
/// this scans rather than anchoring, and the caller keeps the sentence.
pub fn identifiers_in(text: &str) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    identifier_pattern()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {

    /// Pinned to the real corpus, not to invented sentences.
    ///
    /// Every distinct prose `knownVulnerabilities` entry in one nixpkgs
    /// revision (27 of them). The two party claims must be withheld and the
    /// rest kept -- in particular the bundled-component and lifecycle
    /// entries, which are the reason prose is carried at all.
    #[test]
    fn only_party_claims_are_withheld_across_the_measured_corpus() {
        let withhold = [
            "Alist was acquired by Bugotech, a company distrusted by the community",
            "Please nag Broadcom to update to OpenSSL 3 for Darwin.",
        ];
        let keep = [
            // Composition: a component inside, which no SBOM of this package lists.
            "Includes vulnerable versions of bundled libraries: openssl, ffmpeg, gdal, and proj.",
            "Vendors Electron 2.0 (end-of-life)",
            "The bundled version of openssl 1.0.2zk in ovftool for Darwin has open vulnerabilities (maximum severity: Moderate)",
            "resholve depends on python27 (EOL). While it's safe to run on trusted input in the build sandbox, you should avoid running it on untrusted input.",
            "Ventoy uses binary blobs which can't be trusted to be free of malware or compliant to their licenses.",
            // Lifecycle of the software itself.
            "Electron version 38.8.4 is EOL",
            "minio has been abandoned by upstream and security issues won't be fixed.",
            "youtube-dl is unmaintained, migrate to yt-dlp, if possible",
            "librewolf-bin lacks maintenance in nixpkgs, consider using an alternative",
            "NexusMods.App has been discontinued upstream",
            "Gradle 7 no longer receives security updates with the release of Gradle 9 on 31 July 2025.",
            "This NodeJS release has reached its end of life.",
            "Unmaintained. Probable XSS/code injection vulnerability.",
        ];
        for t in withhold {
            assert!(
                Declaration::parse(t).is_about_a_party(),
                "must be withheld: {t}"
            );
        }
        for t in keep {
            assert!(
                !Declaration::parse(t).is_about_a_party(),
                "must be kept -- this describes the software, not a party: {t}"
            );
        }
    }
    use super::*;

    #[test]
    fn an_identifier_inside_prose_is_extracted_without_consuming_the_prose() {
        // The common shape: 72% of measured entries look like this, and the
        // description is the half a consumer can act on.
        let d = Declaration::parse(
            "CVE-2019-9501: heap buffer overflow, potentially allowing remote \
             code execution by sending specially-crafted WiFi packets",
        );
        assert_eq!(d.identifiers, vec!["CVE-2019-9501"]);
        assert!(
            d.text.contains("heap buffer overflow"),
            "the description was consumed by extraction: {:?}",
            d.text
        );
        assert!(d.names_an_identifier());
    }

    #[test]
    fn an_entry_naming_several_identifiers_yields_each_of_them() {
        // FR-004: each becomes its own statement, rather than one statement
        // carrying a concatenated subject.
        let d = Declaration::parse("fixed by CVE-2014-8139, CVE-2014-8140 and CVE-2014-8141");
        assert_eq!(d.identifiers, vec!["CVE-2014-8139", "CVE-2014-8140", "CVE-2014-8141"]);
    }

    /// #1051: the two non-CVE shapes the census found, from the real
    /// entries. Both used to fall through to prose.
    #[test]
    fn vendor_and_ghsa_advisories_are_identifiers() {
        assert_eq!(Declaration::parse("Sonatype-2015-0286").identifiers, vec!["Sonatype-2015-0286"]);
        assert_eq!(Declaration::parse("Sonatype-2022-6438").identifiers, vec!["Sonatype-2022-6438"]);

        // django-ckeditor at nixpkgs a799d3e3, abridged but with every token
        // the pattern could plausibly catch: a repeated GHSA, a package name
        // ending in a digit, a version suffix, a year.
        let d = Declaration::parse(
            "django-ckeditor bundles CKEditor 4.22.1 which isn't supported anmyore \
             … such as CKEditor 5 (django-ckeditor-5) … LTS package until December \
             2028. … List of vulnerabilites fixed in CKEditor 4.24.0-lts: \
             * GHSA-fq6h-4g8v-qqvm * GHSA-fq6h-4g8v-qqvm * GHSA-mw2c-vx6j-mg76",
        );
        assert_eq!(d.identifiers, vec!["GHSA-fq6h-4g8v-qqvm", "GHSA-mw2c-vx6j-mg76"]);
        assert!(d.names_an_identifier());
        assert_eq!(d.cves().count(), 0, "no CVE, so the CVE-only C188 fields are unaffected");
    }

    #[test]
    fn cves_is_the_cve_subset_of_identifiers() {
        let d = Declaration::parse("CVE-2020-1234 and GHSA-fq6h-4g8v-qqvm and Sonatype-2015-0286");
        assert_eq!(d.identifiers.len(), 3);
        assert_eq!(d.cves().collect::<Vec<_>>(), vec!["CVE-2020-1234"]);
    }

    /// The false-positive surface the vendor arm must not reach. Lifecycle
    /// prose with versions and years, as nixpkgs writes it.
    #[test]
    fn lifecycle_prose_with_versions_and_years_names_no_identifier() {
        for text in [
            "Electron version 38.8.4 is EOL",
            "Uses Electron 37.6.0, EOL on October 4, 2025, Several CVEs known.",
            "Garage version 2.92 is EOL",
            "The Xen Project Hypervisor version 4.17 is no longer supported",
        ] {
            assert!(Declaration::parse(text).identifiers.is_empty(), "matched in {text:?}");
        }
    }

    #[test]
    fn a_repeated_identifier_is_named_once() {
        let d = Declaration::parse("CVE-2021-4217 — see CVE-2021-4217 upstream");
        assert_eq!(d.identifiers, vec!["CVE-2021-4217"]);
    }

    #[test]
    fn prose_naming_nothing_still_parses_and_keeps_its_text() {
        // 28% of measured entries. These are the ones no feed carries:
        // bundled components and upstream abandonment.
        let d = Declaration::parse(
            "Includes vulnerable versions of bundled libraries: openssl, ffmpeg, gdal, and proj.",
        );
        assert!(d.identifiers.is_empty());
        assert!(!d.names_an_identifier());
        assert!(d.text.contains("openssl"), "the text must survive verbatim");
    }

    #[test]
    fn a_version_like_string_is_not_mistaken_for_an_identifier() {
        // Real entries sit beside version literals ("2.92", "5.2.4"); the
        // pattern must not fire on them.
        let d = Declaration::parse("Garage version 2.92 is EOL");
        assert!(d.identifiers.is_empty(), "matched: {:?}", d.identifiers);
    }
}
