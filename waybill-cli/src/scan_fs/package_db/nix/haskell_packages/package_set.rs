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
    /// Runtime dependency relations this attribute declares (milestone 985).
    ///
    /// `libraryHaskellDepends` + `executableHaskellDepends`, merged and
    /// deduplicated. Test and benchmark relations are deliberately absent —
    /// see `relations_re` and issue #985.
    pub(crate) runtime_relations: Vec<String>,
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

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A top-level attribute binding: `  th-compat = callPackage (` or
/// `  "3d-graphics-examples" = callPackage (`.
///
/// The attribute name is the key, NOT `pname`. See [`parse`].
fn attribute_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^  (?:"([^"]+)"|([A-Za-z0-9][A-Za-z0-9_.'-]*))\s*=\s*callPackage"#)
            .expect("static attribute regex")
    })
}

fn version_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"version\s*=\s*"([^"]+)"\s*;"#).expect("static version regex")
    })
}

fn sha256_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"sha256\s*=\s*"([^"]+)"\s*;"#).expect("static sha256 regex")
    })
}

/// Parse `hackage-packages.nix`, keyed by **attribute name**.
///
/// # Why the attribute name and not `pname` (#970)
///
/// nixpkgs exposes alternative versions of a package as separate attributes
/// that share one `pname`:
///
/// ```text
///   unordered-containers          -> version 0.2.20.1   <- what a build uses
///   unordered-containers_0_2_21   -> version 0.2.21     <- pinned alternative
/// ```
///
/// Keying on `pname` collapses the two, and any tie-break between them is a
/// guess. The original parser took last-wins and therefore emitted 0.2.21 for
/// a project whose build uses 0.2.20.1 — a version the build never sees,
/// carrying a native SHA-256 belonging to the *other* tarball, and provenance
/// asserting it came from the pinned revision. Measured at that revision:
/// 19,429 attributes against 19,058 distinct `pname` values, so 371
/// attributes share a name with another.
///
/// Attribute names are unique (19,429 of 19,429), and a `.cabal` file names
/// the package, which is the unsuffixed attribute. So an exact attribute
/// lookup selects the right one with no suffix heuristic: nothing asks for
/// `unordered-containers_0_2_21`, and if something did, that is what it would
/// get.
///
/// Keying on `pname` was a deliberate earlier choice, because the attribute
/// name is unquoted unless the package name is not a valid Nix identifier and
/// a parser keyed on `"name" =` finds almost nothing. That problem is real;
/// the answer is to parse both spellings, which this does.
/// `libraryHaskellDepends = [ a b c ];` and the executable twin.
///
/// # Why only these two fields (#962, FR-002)
///
/// These two are the **runtime** closure: parsing them reproduces what
/// `nix eval` reports for `propagatedBuildInputs`, exactly, at 167 components
/// on one measured project. That external agreement is what makes the closure
/// checkable against something outside waybill's own assumptions.
///
/// `testHaskellDepends` and `benchmarkHaskellDepends` are **deliberately not
/// extracted**. They are not a runtime concern, they have no equivalent
/// oracle, and they are far larger: measured multipliers are 1.5–3.8× for the
/// runtime closure against 7.3–9.8× including them. Deferred to issue #985 —
/// this omission is a decision, not an oversight.
///
/// The list body is matched non-greedily up to the first `]`, which is safe
/// because these lists contain only identifiers — no nested brackets.
fn relations_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?s)(?:library|executable)HaskellDepends\s*=\s*\[([^\]]*)\]")
            .expect("static runtime-relations regex")
    })
}

/// Every runtime relation named in one attribute's body, deduplicated and
/// sorted so the closure walk is deterministic (FR-013).
fn runtime_relations_of(body: &str) -> Vec<String> {
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for c in relations_re().captures_iter(body) {
        let Some(list) = c.get(1) else { continue };
        for name in list.as_str().split_whitespace() {
            // Nix identifiers only; anything else is not a package reference.
            if !name.is_empty() {
                out.insert(name.to_string());
            }
        }
    }
    out.into_iter().collect()
}

pub(crate) fn parse(text: &str) -> PackageSet {
    let mut entries: HashMap<String, PackageSetEntry> = HashMap::new();

    let heads: Vec<(usize, String)> = attribute_re()
        .captures_iter(text)
        .filter_map(|c| {
            let m = c.get(0)?;
            let name = c
                .get(1)
                .or_else(|| c.get(2))
                .map(|g| g.as_str().to_string())?;
            Some((m.start(), name))
        })
        .collect();

    for (i, (start, name)) in heads.iter().enumerate() {
        // One attribute's body runs to the next attribute's head. Bounding it
        // this way is what keeps one package's `version` from pairing with a
        // later package's `sha256`.
        let end = heads.get(i + 1).map(|(s, _)| *s).unwrap_or(text.len());
        let body = &text[*start..end];

        let Some(version) = version_re()
            .captures(body)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
        else {
            continue; // no version: nothing worth recording
        };
        let source_hash = sha256_re()
            .captures(body)
            .and_then(|c| c.get(1))
            // A hash that does not decode to 32 bytes yields None rather than
            // a value that would be emitted in a field labelled SHA-256
            // (Principle IX).
            .and_then(|m| nix_base32::sha256_to_hex(m.as_str()));

        entries.insert(
            name.clone(),
            PackageSetEntry {
                version,
                source_hash,
                runtime_relations: runtime_relations_of(body),
            },
        );
    }

    PackageSet { entries }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Shaped exactly like the real file, including the unquoted attribute
    /// name that a `"name" =` parser would miss, and a quoted one.
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

    /// **#970.** nixpkgs exposes alternative versions as separate attributes
    /// sharing one `pname`. Verbatim shape from nixpkgs `a799d3e3`, where the
    /// default attribute is 0.2.20.1 and the pinned alternative is 0.2.21.
    ///
    /// A `pname`-keyed parser collapses these and must guess; last-wins picked
    /// 0.2.21, which is a version the build never uses, carrying the *other*
    /// tarball's hash. 371 attributes at that revision share a name.
    const ALTERNATIVES: &str = r#"
  unordered-containers = callPackage (
    { mkDerivation }:
    mkDerivation {
      pname = "unordered-containers";
      version = "0.2.20.1";
      sha256 = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly";
    }
  ) { };

  unordered-containers_0_2_21 = callPackage (
    { mkDerivation }:
    mkDerivation {
      pname = "unordered-containers";
      version = "0.2.21";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
    }
  ) { };
"#;

    #[test]
    fn m970_the_default_attribute_wins_over_a_pinned_alternative() {
        let ps = parse(ALTERNATIVES);
        let e = ps.get("unordered-containers").unwrap();
        assert_eq!(
            e.version, "0.2.20.1",
            "a .cabal naming `unordered-containers` must get the default \
             attribute, not the pinned alternative that shares its pname"
        );
        // And the hash must be the default attribute's, not the other's.
        assert_eq!(
            e.source_hash.as_deref(),
            Some("9e26f12230d38ae56dcf94f8c139799dc3b7376f3434d35ce74847a0a24fd5ff")
        );
    }

    /// Both attributes survive, keyed separately. Order must not decide which
    /// one an ordinary lookup returns.
    #[test]
    fn m970_both_attributes_are_retained_and_distinct() {
        let ps = parse(ALTERNATIVES);
        assert_eq!(ps.len(), 2);
        assert_eq!(ps.get("unordered-containers_0_2_21").unwrap().version, "0.2.21");
    }

    /// The same two attributes with the alternative written FIRST must give
    /// the same answer. Under `pname` keying with last-wins, reordering
    /// flipped the result — which is what made the bug invisible to a fixture
    /// author, who had no reason to write them in either order.
    #[test]
    fn m970_declaration_order_does_not_change_the_answer() {
        const REVERSED: &str = r#"
  unordered-containers_0_2_21 = callPackage (
    { mkDerivation }:
    mkDerivation {
      pname = "unordered-containers";
      version = "0.2.21";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
    }
  ) { };

  unordered-containers = callPackage (
    { mkDerivation }:
    mkDerivation {
      pname = "unordered-containers";
      version = "0.2.20.1";
      sha256 = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly";
    }
  ) { };
"#;
        assert_eq!(
            parse(REVERSED).get("unordered-containers").unwrap().version,
            "0.2.20.1"
        );
        assert_eq!(
            parse(ALTERNATIVES).get("unordered-containers").unwrap().version,
            parse(REVERSED).get("unordered-containers").unwrap().version
        );
    }

    /// A version without a hash is still worth having: it is what moves a
    /// component from design tier to source tier.
    #[test]
    fn m926_version_without_hash_is_kept_with_no_hash() {
        let no_hash = r#"
  hashless = callPackage ({ mkDerivation }: mkDerivation {
      pname = "hashless"; version = "3.1";
  }) { };
"#;
        let e = parse(no_hash).get("hashless").cloned().unwrap();
        assert_eq!(e.version, "3.1");
        assert_eq!(e.source_hash, None);
    }

    /// A malformed hash must yield no hash, never a malformed one.
    #[test]
    fn m926_undecodable_hash_yields_no_hash_not_a_bad_one() {
        let bad = r#"
  bad = callPackage ({ mkDerivation }: mkDerivation {
      pname = "bad"; version = "1.0"; sha256 = "not-valid-base32-eout";
  }) { };
"#;
        let e = parse(bad).get("bad").cloned().unwrap();
        assert_eq!(e.version, "1.0");
        assert_eq!(e.source_hash, None);
    }

    #[test]
    fn m926_empty_input_yields_empty_set() {
        assert!(parse("").is_empty());
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod relation_tests {
    use super::*;

    const SAMPLE: &str = r#"
  alpha = callPackage ({ mkDerivation }: mkDerivation {
      pname = "alpha";
      version = "1.0.0";
      libraryHaskellDepends = [ base text ];
      executableHaskellDepends = [ optparse-applicative ];
      testHaskellDepends = [ hspec QuickCheck ];
      benchmarkHaskellDepends = [ criterion ];
  }) { };
  beta = callPackage ({ mkDerivation }: mkDerivation {
      pname = "beta";
      version = "2.0.0";
  }) { };
  gamma = callPackage ({ mkDerivation }: mkDerivation {
      pname = "gamma";
      version = "3.0.0";
      libraryHaskellDepends = [
        multi
        line
        list
      ];
  }) { };
  Delta = callPackage ({ mkDerivation }: mkDerivation {
      pname = "Delta";
      version = "4.0.0";
      libraryHaskellDepends = [ Diff ];
  }) { };
"#;

    /// Library and executable relations are both collected, merged, sorted.
    #[test]
    fn m985_library_and_executable_relations_are_collected() {
        let set = parse(SAMPLE);
        let a = set.get("alpha").unwrap();
        assert_eq!(a.runtime_relations, vec!["base", "optparse-applicative", "text"]);
    }

    /// **The scope boundary, enforced by a test rather than by memory.**
    ///
    /// `testHaskellDepends` and `benchmarkHaskellDepends` must not leak into
    /// the runtime closure. On one measured project, test relations alone take
    /// the closure from 32 components to 153 — a 4.8× difference that would
    /// misstate what the project ships. Deferred to issue #985.
    #[test]
    fn m985_test_and_benchmark_relations_are_not_collected() {
        let set = parse(SAMPLE);
        let a = set.get("alpha").unwrap();
        for leaked in ["hspec", "QuickCheck", "criterion"] {
            assert!(
                !a.runtime_relations.iter().any(|r| r == leaked),
                "{leaked} is a test/benchmark relation and must not be in the \
                 runtime closure; got {:?}",
                a.runtime_relations
            );
        }
    }

    /// An attribute with no relations yields an empty list, not an error.
    #[test]
    fn m985_an_attribute_with_no_relations_is_empty_not_absent() {
        let set = parse(SAMPLE);
        assert!(set.get("beta").unwrap().runtime_relations.is_empty());
    }

    /// Real lists span lines; a line-oriented reading would take only the first.
    #[test]
    fn m985_a_multi_line_relation_list_is_read_whole() {
        let set = parse(SAMPLE);
        assert_eq!(
            set.get("gamma").unwrap().runtime_relations,
            vec!["line", "list", "multi"]
        );
    }

    /// Hackage names are case-sensitive: `Diff` and `diff` are different
    /// packages, and folding case here would resolve one to the other's
    /// version (#943).
    #[test]
    fn m985_relation_names_are_case_preserving() {
        let set = parse(SAMPLE);
        assert_eq!(set.get("Delta").unwrap().runtime_relations, vec!["Diff"]);
    }

    /// Relations are sorted and deduplicated, because the walk's output order
    /// reaches the emitted document and two scans must be byte-identical
    /// (FR-013).
    #[test]
    fn m985_relations_are_deterministic() {
        let dup = r#"
  x = callPackage ({ mkDerivation }: mkDerivation {
      pname = "x";
      version = "1.0";
      libraryHaskellDepends = [ zeta alpha zeta ];
      executableHaskellDepends = [ alpha mid ];
  }) { };
"#;
        let set = parse(dup);
        assert_eq!(set.get("x").unwrap().runtime_relations, vec!["alpha", "mid", "zeta"]);
    }
}
