//! Compiler-supplied (boot) libraries (milestone 926, #947).
//!
//! nixpkgs marks a package the compiler ships by binding it to `null` in
//! `configuration-ghc-<series>.nix`: "use the one that comes with GHC, do not
//! build it from Hackage". Such a package has no version in the package set,
//! and its real version is a property of the compiler rather than of nixpkgs.
//! This is the same set `cabal v2-freeze` declines to pin (#938) — two
//! independent tools declining for the same reason.
//!
//! # Why a nulled name is not automatically a boot library
//!
//! The obvious extraction — every `name = null;` binding — also matches
//! `editedCabalFile`, an attribute *inside* a derivation override rather than
//! a package.
//!
//! Scoping by attribute-set nesting depth looks like the fix and is not.
//! Measured across three GHC series at nixpkgs `a799d3e3`, a depth rule
//! correctly drops `editedCabalFile` from 9.6.x but **also** drops
//! `directory-ospath-streaming` from 9.4.x — a real package at v0.3 that the
//! compiler genuinely supplies. Real boot libraries live at deeper nesting
//! too, inside conditional attribute sets.
//!
//! What separates the cases is package-set membership: `editedCabalFile` is
//! not a package at all, and `directory-ospath-streaming` is.
//!
//! The asymmetry is what makes this the right rule rather than merely a
//! working one. Over-including a boot library withholds a version — lossy,
//! and visible to the operator as a reason code. Under-including one lets a
//! package the compiler supplies resolve to a Hackage version the build never
//! uses: an invented version, which Principle IX forbids. The rule must fail
//! toward over-inclusion, and package-set membership cannot under-include a
//! real package, because a real package is by definition in the set.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;

/// `name = null;` in a per-compiler configuration.
///
/// Deliberately not anchored to the start of a line. Real configurations put
/// each binding on its own line, but that is a formatting convention rather
/// than a guarantee, and a line-anchored pattern silently misses an inline
/// `{ alpha = null; }`. Missing one is the dangerous direction: it would let
/// a package the compiler supplies resolve to a Hackage version. The leading
/// boundary keeps `x.y = null;` and `foo_bar = null;` from being split at an
/// arbitrary point.
fn null_binding_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)(?:^|[\s{;])([A-Za-z][A-Za-z0-9_'-]*)[ \t]*=[ \t]*null[ \t]*;")
            .expect("static null-binding regex")
    })
}

/// Every attribute bound to `null` in one compiler configuration.
///
/// These are *candidates*, not conclusions: see the module docs. Membership
/// in the package set decides which are actually boot libraries, and that
/// test lives at classification time so this function needs no package set.
pub(crate) fn nulled_names(config_text: &str) -> BTreeSet<String> {
    null_binding_re()
        .captures_iter(config_text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

/// Union the nulled names of every candidate compiler configuration.
///
/// Union, not intersection, is the fail-closed reading of FR-014a: a package
/// nulled in *any* candidate is treated as compiler-supplied, because when
/// several compilers are possible and they disagree, waybill cannot know
/// which one built the project. Measured cost at nixpkgs `a799d3e3`: 5
/// genuine packages when a flake names three series, 18 when no flake signal
/// is found and every series at the revision is a candidate.
pub(crate) fn union_nulled<'a, I>(configs: I) -> BTreeSet<String>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut out = BTreeSet::new();
    for text in configs {
        out.extend(nulled_names(text));
    }
    out
}

/// Is `name` a boot library — nulled by a candidate compiler AND a real
/// package?
///
/// `is_package` answers "does the pinned package set contain this name",
/// passed in rather than imported so this module stays independent of the
/// package-set parser.
pub(crate) fn is_boot_library(
    name: &str,
    nulled: &BTreeSet<String>,
    is_package: impl Fn(&str) -> bool,
) -> bool {
    nulled.contains(name) && is_package(name)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Shaped like a real `configuration-ghc-<series>.nix`: top-level package
    /// overrides, plus a nested derivation override that also binds `null`.
    const CONFIG: &str = r#"
{ pkgs, haskellLib }:
self: super: {
  base = null;
  text = null;

  some-package = overrideCabal (drv: {
    editedCabalFile = null;
  }) super.some-package;
} // lib.optionalAttrs (versionAtLeast "9.4") {
  directory-ospath-streaming = null;
}
"#;

    fn package_set_contains(name: &str) -> bool {
        // `editedCabalFile` is deliberately absent: it is a derivation
        // attribute, not a Haskell package.
        matches!(name, "base" | "text" | "directory-ospath-streaming")
    }

    #[test]
    fn m926_collects_every_null_binding_as_a_candidate() {
        let n = nulled_names(CONFIG);
        assert!(n.contains("base"));
        assert!(n.contains("text"));
        assert!(n.contains("editedCabalFile"));
        assert!(n.contains("directory-ospath-streaming"));
    }

    #[test]
    fn m926_a_nulled_name_that_is_a_package_is_a_boot_library() {
        let n = nulled_names(CONFIG);
        assert!(is_boot_library("base", &n, package_set_contains));
        assert!(is_boot_library("text", &n, package_set_contains));
    }

    /// The false positive the depth rule was invented to remove.
    #[test]
    fn m926_a_nulled_name_that_is_not_a_package_is_excluded() {
        let n = nulled_names(CONFIG);
        assert!(
            !is_boot_library("editedCabalFile", &n, package_set_contains),
            "a derivation attribute is not a boot library"
        );
    }

    /// The case that **rejected** the attrset-depth rule. This binding sits
    /// deeper than the top-level overrides, inside a conditional attrset, yet
    /// names a real package the compiler supplies. A depth-scoped extractor
    /// drops it, and dropping it would let it resolve to a Hackage version
    /// the build never uses.
    #[test]
    fn m926_a_nulled_package_at_deeper_nesting_is_still_a_boot_library() {
        let n = nulled_names(CONFIG);
        assert!(
            is_boot_library("directory-ospath-streaming", &n, package_set_contains),
            "nesting depth must not decide this — measurement rejected that rule"
        );
    }

    /// FR-014a: nulled in ANY candidate means boot. Fail closed.
    #[test]
    fn m926_union_marks_a_package_nulled_in_only_one_candidate_as_boot() {
        let only_in_a = "self: super: { alpha = null; }";
        let only_in_b = "self: super: { beta = null; }";
        let u = union_nulled([only_in_a, only_in_b]);
        assert!(u.contains("alpha"));
        assert!(u.contains("beta"));
        assert!(is_boot_library("alpha", &u, |_| true));
        assert!(is_boot_library("beta", &u, |_| true));
    }

    #[test]
    fn m926_a_package_no_candidate_nulls_is_not_boot() {
        let n = nulled_names(CONFIG);
        assert!(!is_boot_library("aeson", &n, |_| true));
    }
}
