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
    use std::collections::BTreeMap;

    /// A package set with two known versions, built through the public parse
    /// path so the test cannot drift from the real entry shape.
    fn two_package_set() -> PackageSet {
        parse(
            r#"
  "waybill-fixture-alpha" = callPackage ({ mkDerivation }: mkDerivation {
     pname = "waybill-fixture-alpha"; version = "1.0.0";
   }) {};
  "waybill-fixture-beta" = callPackage ({ mkDerivation }: mkDerivation {
     pname = "waybill-fixture-beta"; version = "2.0.0";
   }) {};
"#,
        )
    }

    #[test]
    fn evaluation_supersedes_a_file_parsed_version_and_reports_the_loser() {
        let mut set = two_package_set();
        // Control: the fixture really does start where we think it does. A
        // parse that silently produced nothing would make every assertion
        // below vacuous.
        assert_eq!(set.get("waybill-fixture-alpha").map(|e| e.version.as_str()), Some("1.0.0"));

        let mut evaluated = BTreeMap::new();
        evaluated.insert("waybill-fixture-alpha".to_string(), Some("1.5.0".to_string()));

        let diverged = set.apply_evaluated_versions(&evaluated);

        assert_eq!(diverged.len(), 1, "one component disagreed");
        assert_eq!(diverged[0].name, "waybill-fixture-alpha");
        assert_eq!(diverged[0].file_parsed, "1.0.0", "the loser is retained, not dropped");
        assert_eq!(diverged[0].evaluated, "1.5.0");
        assert_eq!(
            set.get("waybill-fixture-alpha").map(|e| e.version.as_str()),
            Some("1.5.0"),
            "evaluation wins: in nix, evaluation is what is real"
        );
        assert_eq!(
            set.get("waybill-fixture-beta").map(|e| e.version.as_str()),
            Some("2.0.0"),
            "an untouched package must not move"
        );
    }

    #[test]
    fn agreement_produces_no_divergence_record() {
        let mut set = two_package_set();
        let mut evaluated = BTreeMap::new();
        evaluated.insert("waybill-fixture-alpha".to_string(), Some("1.0.0".to_string()));
        assert!(set.apply_evaluated_versions(&evaluated).is_empty());
    }

    #[test]
    fn a_null_evaluation_result_never_disturbs_a_parsed_version() {
        // `null` means the attribute was absent or `tryEval` caught it. It is
        // not a version, and treating it as one would erase good data.
        let mut set = two_package_set();
        let mut evaluated = BTreeMap::new();
        evaluated.insert("waybill-fixture-alpha".to_string(), None);
        assert!(set.apply_evaluated_versions(&evaluated).is_empty());
        assert_eq!(set.get("waybill-fixture-alpha").map(|e| e.version.as_str()), Some("1.0.0"));
    }

    #[test]
    fn an_evaluated_name_absent_from_the_parsed_set_is_ignored_here() {
        // Principle XII constraint 1 is about component introduction, which
        // is the caller's concern; this function only reconciles versions for
        // entries the set already has.
        let mut set = two_package_set();
        let mut evaluated = BTreeMap::new();
        evaluated.insert("waybill-fixture-unknown".to_string(), Some("9.9.9".to_string()));
        assert!(set.apply_evaluated_versions(&evaluated).is_empty());
        assert_eq!(set.len(), 2);
    }

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

/// Issue #1033 — the top-level version rebinds in `configuration-common.nix`.
///
/// `hackage-packages.nix` is generated from Hackage and carries, under the
/// plain attribute name, whatever version the generator chose. nixpkgs then
/// **rebinds** some of those names to a pinned alternative:
///
/// ```nix
/// ghc-typelits-natnormalise = doDistribute self.ghc-typelits-natnormalise_0_7_12;
/// ```
///
/// The composed package set therefore has `0.7.12` where the generated file
/// still says `0.7.10` under that name. waybill read only the generated file,
/// so it emitted the superseded version and said nothing — measured on
/// `haskell-language-server`, where both of the revision's top-level rebinds
/// appeared in the dependency set and both were wrong.
///
/// # Why only the top level
///
/// The same `<name> = self.<name>_<version>;` shape appears **20 more times**
/// in that file at deeper indentation, inside `overrideScope` blocks and
/// per-package overlays. Those are one package's private view, not a rebind of
/// the shared set. Applying them globally would introduce twenty new wrong
/// versions — strictly worse than the bug this fixes.
///
/// So the rule is: after the file's top-level `in`, at an indentation of
/// exactly two spaces. Measured at revision `cbb5cf35…`: 22 matches of the
/// shape, of which exactly 2 satisfy this and they are precisely the two that
/// were wrong.
///
/// # This is a heuristic, and its failure direction is deliberate
///
/// Indentation is a proxy for "direct member of the override set" and a
/// reformatting of nixpkgs would break it. It breaks toward matching
/// **nothing**, which is exactly today's behaviour, rather than toward
/// applying an override that does not apply. A parser that tracked brace depth
/// would be sturdier; the rest of this module is regex over Nix source, and a
/// rule whose failure mode is "no change" is the one to prefer while that
/// stays true.
///
/// Returns plain-name → alias-attribute-name.
pub(crate) fn parse_top_level_version_rebinds(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut seen_in = false;
    for line in text.lines() {
        // The `let ... in` that opens the override set. Bindings before it are
        // `let` bindings and are not members of the set.
        if !seen_in {
            if line == "in" || line.starts_with("in ") {
                seen_in = true;
            }
            continue;
        }
        // Exactly two spaces of indent, and not three.
        let Some(rest) = line.strip_prefix("  ") else {
            continue;
        };
        if rest.starts_with(' ') {
            continue;
        }
        let Some((lhs, rhs)) = rest.split_once(" = ") else {
            continue;
        };
        let name = lhs.trim().trim_matches('"');
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.'".contains(c))
        {
            continue;
        }
        // `self.<alias>;` optionally wrapped in `doDistribute`.
        let rhs = rhs.trim().trim_end_matches(';').trim();
        let rhs = rhs.strip_prefix("doDistribute ").unwrap_or(rhs).trim();
        let Some(alias) = rhs.strip_prefix("self.") else {
            continue;
        };
        // Only a versioned alias OF THE SAME PACKAGE. `foo = self.bar_1_2` is
        // an aliasing decision, not a version pin, and is left alone.
        let Some(suffix) = alias.strip_prefix(name) else {
            continue;
        };
        if !suffix.starts_with('_') || !suffix[1..].chars().all(|c| c.is_ascii_digit() || c == '_') {
            continue;
        }
        out.insert(name.to_string(), alias.to_string());
    }
    out
}

impl PackageSet {
    /// Apply issue #1033's rebinds: a rebound name takes the alias's version.
    ///
    /// A rebind naming an alias this revision does not carry is ignored rather
    /// than guessed at — the generated file is the authority on what exists.
    /// Returns the number of entries whose version changed.
    pub(crate) fn apply_version_rebinds(&mut self, rebinds: &HashMap<String, String>) -> usize {
        let mut changed = 0usize;
        for (name, alias) in rebinds {
            let Some(alias_entry) = self.entries.get(alias).cloned() else {
                continue;
            };
            if let Some(target) = self.entries.get_mut(name) {
                if target.version != alias_entry.version {
                    target.version = alias_entry.version;
                    // The alias's own relations and hash describe the alias's
                    // source, which is what the rebound name now resolves to.
                    target.runtime_relations = alias_entry.runtime_relations;
                    target.source_hash = alias_entry.source_hash;
                    changed += 1;
                }
            }
        }
        changed
    }

    /// Apply versions obtained by evaluating Nix, superseding what parsing the
    /// package-set files produced.
    ///
    /// Evaluation wins. In Nix, evaluation is what is real: the files are an
    /// approximation of it, and issue #1033 is the proof that the
    /// approximation can be wrong -- two components shipped wrong versions
    /// because a file that supersedes the generated set was not read.
    ///
    /// Returns one record per component where the two disagreed. The losing
    /// value is returned rather than dropped so the caller can keep it in the
    /// document: that divergence is how #1033 was found, and hiding it would
    /// remove the signal that catches the next one.
    pub(crate) fn apply_evaluated_versions(
        &mut self,
        evaluated: &std::collections::BTreeMap<String, Option<String>>,
    ) -> Vec<EvaluatedDivergence> {
        let mut diverged = Vec::new();
        for (name, version) in evaluated {
            // `None` means the attribute was absent or `tryEval` caught it --
            // not a version, and not a reason to disturb what parsing found.
            let Some(evaluated_version) = version else {
                continue;
            };
            let Some(target) = self.entries.get_mut(name) else {
                continue;
            };
            if &target.version != evaluated_version {
                diverged.push(EvaluatedDivergence {
                    name: name.clone(),
                    file_parsed: target.version.clone(),
                    evaluated: evaluated_version.clone(),
                });
                target.version = evaluated_version.clone();
            }
        }
        diverged
    }
}

/// One component whose evaluated version differed from the file-parsed one.
///
/// Not to be confused with the milestone-926 disagreement record (catalogue
/// row C172), which compares a *locally established* version against the
/// pinned revision's and lets the local value win. This compares two readings
/// of the same revision -- one by evaluation, one by file-parsing -- and the
/// evaluated value wins. Opposite precedence, different pair of sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvaluatedDivergence {
    pub(crate) name: String,
    /// The version parsing produced, superseded but retained.
    pub(crate) file_parsed: String,
    /// The version Nix reported, which is what the component now carries.
    pub(crate) evaluated: String,
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod rebind_tests {
    //! Issue #1033. The fixture is the real shape of
    //! `configuration-common.nix` at nixpkgs `cbb5cf35…`, including the
    //! nested bindings that must NOT be applied — those are the trap: the
    //! same syntax appears 20 more times inside `overrideScope` blocks, and
    //! applying them globally would be worse than the bug.

    use super::*;

    const REAL_SHAPE: &str = r#"# COMMON OVERRIDES FOR THE HASKELL PACKAGE SET IN NIXPKGS
{ pkgs, haskellLib }:

self: super:

let
  inherit (pkgs) fetchpatch lib;
  # A `let` binding of the same shape, before the `in`. Not a member of the
  # override set, and must not be read as one.
  trap-let-binding = self.trap-let-binding_9_9_9;
in
{
  ghc-tcplugins-extra = doDistribute self.ghc-tcplugins-extra_0_5;
  ghc-typelits-natnormalise = doDistribute self.ghc-typelits-natnormalise_0_7_12;

  cabal-install = super.cabal-install.overrideScope (self: super: {
    Cabal = self.Cabal_3_16_1_0;
    Cabal-syntax = self.Cabal-syntax_3_16_1_0;
  });

  some-package = super.some-package.override {
    hnix-store-core = self.hnix-store-core_0_8_0_0;
  };

  # An alias to a DIFFERENT package: a naming decision, not a version pin.
  foo = self.bar_1_2_3;

  # Not a version alias at all.
  plain-override = doJailbreak super.plain-override;
}
"#;

    #[test]
    fn only_top_level_same_package_version_rebinds_are_read() {
        let got = parse_top_level_version_rebinds(REAL_SHAPE);
        let mut keys: Vec<&str> = got.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(
            keys,
            vec!["ghc-tcplugins-extra", "ghc-typelits-natnormalise"],
            "got {got:?}",
        );
        assert_eq!(got["ghc-tcplugins-extra"], "ghc-tcplugins-extra_0_5");
        assert_eq!(
            got["ghc-typelits-natnormalise"],
            "ghc-typelits-natnormalise_0_7_12"
        );
    }

    #[test]
    fn a_nested_rebind_is_not_applied() {
        let got = parse_top_level_version_rebinds(REAL_SHAPE);
        for n in ["Cabal", "Cabal-syntax", "hnix-store-core"] {
            assert!(
                !got.contains_key(n),
                "{n} is scoped to one package's override and must not rebind the shared set",
            );
        }
    }

    #[test]
    fn a_let_binding_before_the_in_is_not_a_member() {
        let got = parse_top_level_version_rebinds(REAL_SHAPE);
        assert!(!got.contains_key("trap-let-binding"));
    }

    #[test]
    fn an_alias_to_another_package_is_left_alone() {
        let got = parse_top_level_version_rebinds(REAL_SHAPE);
        assert!(!got.contains_key("foo"), "foo = self.bar_1_2_3 is not a version pin");
        assert!(!got.contains_key("plain-override"));
    }

    fn entry(v: &str) -> PackageSetEntry {
        PackageSetEntry {
            version: v.to_string(),
            runtime_relations: vec![],
            source_hash: None,
        }
    }

    #[test]
    fn applying_a_rebind_takes_the_alias_version() {
        let mut ps = PackageSet::default();
        ps.entries.insert("ghc-typelits-natnormalise".into(), entry("0.7.10"));
        ps.entries.insert("ghc-typelits-natnormalise_0_7_12".into(), entry("0.7.12"));
        let rebinds = parse_top_level_version_rebinds(REAL_SHAPE);
        let changed = ps.apply_version_rebinds(&rebinds);
        assert_eq!(changed, 1);
        assert_eq!(ps.get("ghc-typelits-natnormalise").unwrap().version, "0.7.12");
        // The alias itself is untouched and still addressable.
        assert_eq!(ps.get("ghc-typelits-natnormalise_0_7_12").unwrap().version, "0.7.12");
    }

    #[test]
    fn a_rebind_naming_an_absent_alias_changes_nothing() {
        let mut ps = PackageSet::default();
        ps.entries.insert("ghc-tcplugins-extra".into(), entry("0.4.6"));
        let rebinds = parse_top_level_version_rebinds(REAL_SHAPE);
        assert_eq!(ps.apply_version_rebinds(&rebinds), 0);
        assert_eq!(ps.get("ghc-tcplugins-extra").unwrap().version, "0.4.6");
    }

    #[test]
    fn a_file_with_no_rebinds_is_a_no_op() {
        assert!(parse_top_level_version_rebinds("self: super: {\n}\n").is_empty());
        assert!(parse_top_level_version_rebinds("").is_empty());
    }
}
