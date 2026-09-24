//! Parse a pinned revision's generated Haskell package set (milestone 926).
//!
//! `pkgs/development/haskell-modules/hackage-packages.nix` maps a package
//! name to the exact version and source hash nixpkgs builds it from. At
//! revision `a799d3e3` it is 16,634,427 bytes holding 19,437 derivation
//! blocks that resolve to 19,058 unique names.
//!
//! Shape, verbatim from that revision:
//!
//! ```text
//!   th-compat = callPackage (
//!     { mkDerivation, base, hspec, ... }:
//!     mkDerivation {
//!       pname = "th-compat";
//!       version = "0.1.7";
//!       sha256 = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly";
//! ```
//!
//! Two details a parser gets wrong on the first try:
//!
//! - The attribute name is **unquoted** unless the package name is not a
//!   valid Nix identifier. A parser keyed on `"name" =` finds almost nothing.
//!   This parser reads `pname` from inside the derivation instead, which is
//!   quoted and always present.
//! - A name may be defined more than once (19,437 blocks, 19,058 names).
//!   Later bindings shadow earlier ones in a Nix attribute set, so
//!   construction is last-wins.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

use super::nix_base32;

/// One package as the pinned revision records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackageSetEntry {
    /// Exact version, e.g. `0.1.7`.
    pub(crate) version: String,
    /// SHA-256 of the source tarball, **hex**, converted from Nix base32 at
    /// parse time. `None` when the revision records no hash, or records one
    /// that is not 32 bytes — never a malformed value (Principle IX).
    pub(crate) source_hash: Option<String>,
}

/// The name → (version, hash) mapping carried by one pinned revision.
#[derive(Debug, Clone, Default)]
pub(crate) struct PackageSet {
    entries: HashMap<String, PackageSetEntry>,
}

impl PackageSet {
    pub(crate) fn get(&self, name: &str) -> Option<&PackageSetEntry> {
        self.entries.get(name)
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `pname = "x"; version = "y"; ... sha256 = "z";` within one derivation.
///
/// The bounded gap between `version` and `sha256` keeps the match inside a
/// single derivation: an unbounded `.*?` would happily pair one package's
/// `pname` with a later package's `sha256`.
fn derivation_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?s)pname\s*=\s*"([^"]+)"\s*;\s*version\s*=\s*"([^"]+)"\s*;(.{0,400}?)sha256\s*=\s*"([^"]+)"\s*;"#,
        )
        .expect("static derivation regex")
    })
}

/// A derivation with a `pname`/`version` but no nearby `sha256`.
fn versioned_only_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"pname\s*=\s*"([^"]+)"\s*;\s*version\s*=\s*"([^"]+)"\s*;"#)
            .expect("static versioned-only regex")
    })
}

/// Parse `hackage-packages.nix`.
///
/// Last definition wins, matching Nix attribute-set semantics.
pub(crate) fn parse(text: &str) -> PackageSet {
    let mut entries: HashMap<String, PackageSetEntry> = HashMap::new();

    // Pass 1: every package that also carries a source hash.
    for c in derivation_re().captures_iter(text) {
        let (Some(name), Some(version), Some(sha)) = (c.get(1), c.get(2), c.get(4)) else {
            continue;
        };
        entries.insert(
            name.as_str().to_string(),
            PackageSetEntry {
                version: version.as_str().to_string(),
                // A hash that does not decode to 32 bytes yields None rather
                // than a value that would be emitted in a field labelled
                // SHA-256 (Principle IX).
                source_hash: nix_base32::sha256_to_hex(sha.as_str()),
            },
        );
    }

    // Pass 2: packages with a version but no hash nearby. A version alone is
    // still worth having — it is the difference between design tier and
    // source tier — so it is not discarded for want of a hash.
    for c in versioned_only_re().captures_iter(text) {
        let (Some(name), Some(version)) = (c.get(1), c.get(2)) else {
            continue;
        };
        entries
            .entry(name.as_str().to_string())
            .or_insert_with(|| PackageSetEntry {
                version: version.as_str().to_string(),
                source_hash: None,
            });
    }

    PackageSet { entries }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Shaped exactly like the real file, including the unquoted attribute
    /// name that a `"name" =` parser would miss.
    const SAMPLE: &str = r#"
  th-compat = callPackage (
    {
      mkDerivation,
      base,
      template-haskell,
    }:
    mkDerivation {
      pname = "th-compat";
      version = "0.1.7";
      sha256 = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly";
      libraryHaskellDepends = [ base template-haskell ];
      description = "Backward- and forward-compatible Quote and Code types";
    }
  ) { };

  "3d-graphics-examples" = callPackage (
    { mkDerivation }:
    mkDerivation {
      pname = "3d-graphics-examples";
      version = "0.0.0.2";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
    }
  ) { };
"#;

    #[test]
    fn m926_parses_unquoted_and_quoted_attribute_names_alike() {
        let ps = parse(SAMPLE);
        assert_eq!(ps.len(), 2);
        assert_eq!(ps.get("th-compat").unwrap().version, "0.1.7");
        assert_eq!(ps.get("3d-graphics-examples").unwrap().version, "0.0.0.2");
    }

    /// The hash arrives as Nix base32 and must be stored as hex, because that
    /// is what the native SHA-256 field in every format expects (R2 / C3).
    #[test]
    fn m926_source_hash_is_stored_as_hex() {
        let ps = parse(SAMPLE);
        assert_eq!(
            ps.get("th-compat").unwrap().source_hash.as_deref(),
            Some("9e26f12230d38ae56dcf94f8c139799dc3b7376f3434d35ce74847a0a24fd5ff")
        );
    }

    /// Nix attrset semantics: a later binding shadows an earlier one.
    #[test]
    fn m926_last_definition_wins() {
        let dup = r#"
      mkDerivation { pname = "dup"; version = "1.0"; sha256 = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly"; }
      mkDerivation { pname = "dup"; version = "2.0"; sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r"; }
    "#;
        assert_eq!(parse(dup).get("dup").unwrap().version, "2.0");
    }

    /// A version without a hash is still worth having: it is what moves a
    /// component from design tier to source tier.
    #[test]
    fn m926_version_without_hash_is_kept_with_no_hash() {
        let no_hash = r#"mkDerivation { pname = "hashless"; version = "3.1"; }"#;
        let e = parse(no_hash).get("hashless").cloned().unwrap();
        assert_eq!(e.version, "3.1");
        assert_eq!(e.source_hash, None);
    }

    /// A malformed hash must yield no hash, never a malformed one.
    #[test]
    fn m926_undecodable_hash_yields_no_hash_not_a_bad_one() {
        let bad = r#"mkDerivation { pname = "bad"; version = "1.0"; sha256 = "not-valid-base32-eout"; }"#;
        let e = parse(bad).get("bad").cloned().unwrap();
        assert_eq!(e.version, "1.0");
        assert_eq!(e.source_hash, None);
    }

    #[test]
    fn m926_empty_input_yields_empty_set() {
        assert!(parse("").is_empty());
    }
}
