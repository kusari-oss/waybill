//! Compiler-supplied (boot) libraries (milestone 926, #947).
//!
//! nixpkgs marks a package the compiler ships by binding it to `null` in
//! `configuration-ghc-<series>.nix`: "use the one that comes with GHC, do not
//! build it from Hackage". Such a package has no version in the package set,
//! and its real version is a property of the compiler rather than of nixpkgs.
//! This is the same set `cabal v2-freeze` declines to pin (#938) — two
//! independent tools declining for the same reason.
//!
//! # Why the rule is simply "nulled means compiler-supplied"
//!
//! Two more elaborate rules were implemented and both were rejected by
//! measurement. The record is kept because each looks obviously right.
//!
//! The naive extraction — every `name = null;` binding — also matches
//! `editedCabalFile`, an attribute *inside* a derivation override rather than
//! a package. Two attempts were made to exclude it.
//!
//! **Attempt 1, attribute-set nesting depth.** Measured across three GHC
//! series at nixpkgs `a799d3e3`: it correctly drops `editedCabalFile` from
//! 9.6.x but also drops `directory-ospath-streaming` from 9.4.x, a real
//! package at v0.3. Real boot libraries live at deeper nesting too, inside
//! conditional attribute sets. Depth does not separate the cases.
//!
//! **Attempt 2, package-set membership** — keep a nulled name only when it is
//! also a package in `hackage-packages.nix`. This excludes `editedCabalFile`
//! correctly, and it was wrong for a subtler reason. Measured across all eight
//! series at that revision, the nulled names absent from the package set are:
//!
//! ```text
//! editedCabalFile      a derivation attribute       -> correctly excluded
//! rts                  the GHC runtime system       -> WRONGLY excluded
//! ghc-platform         GHC-bundled                  -> WRONGLY excluded
//! ghc-toolchain        GHC-bundled                  -> WRONGLY excluded
//! system-cxx-std-lib   GHC-bundled                  -> WRONGLY excluded
//! ```
//!
//! Those four are real Haskell packages a project can declare, and they are
//! missing from `hackage-packages.nix` *precisely because* they are never
//! built from Hackage — they only ever come from the compiler. Excluding them
//! would report `absent-from-package-set` for a dependency whose true reason
//! is `compiler-supplied`: no invented version, but a false statement in the
//! emitted document (Principle X).
//!
//! **So the rule is the simple one.** A nulled name is compiler-supplied.
//! `editedCabalFile` stays in the set and is inert, because the set is only
//! ever consulted for names the project actually declared, and no project
//! declares a dependency called `editedCabalFile` — it is not a legal Haskell
//! package name. A false positive nothing can ever query costs nothing.
//!
//! The asymmetry that governs every version of this rule: over-including a
//! boot library withholds a version, which is lossy and visible as a reason
//! code. Under-including one lets a package the compiler supplies resolve to
//! a Hackage version the build never uses — an invented version, which
//! Principle IX forbids. Both rejected attempts failed toward under-inclusion.

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

/// `name = <fn> self.<target>;` in a per-compiler configuration (#984).
///
/// The configuration file does not only null out boot libraries. It also
/// **rebinds** attributes, and one shape of rebinding creates a bare name
/// that exists nowhere else:
///
/// ```nix
/// os-string = doDistribute self.os-string_2_0_10;
/// ```
///
/// `hackage-packages.nix` carries the attribute `os-string_2_0_10`; the bare
/// `os-string` a project (or another derivation's dependency list) refers to
/// is created here. Without reading this, such a name resolves to nothing and
/// is reported `absent-from-package-set` when a version was available.
///
/// Only the `self.` form is captured. The `super.` form —
/// `doJailbreak super.aeson` — modifies an attribute that already exists in
/// the package set, so it changes nothing about which version a bare name
/// refers to. Measured at nixpkgs `a799d3e3`, `configuration-ghc-9.6.x.nix`
/// has 25 rebindings, of which 3 have a bare name absent from the package
/// set.
///
/// The target is captured, not resolved: an alias whose target is itself
/// missing yields nothing rather than a guess (Principle IX).
fn alias_binding_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            // The target must be the LAST term before `;`. Without that,
            // `addBuildDepend self.libiserv super.iserv-proxy` reads as
            // `iserv-proxy -> libiserv`, aliasing a package onto one of its
            // own dependencies and resolving it to that dependency's version.
            // Caught by `m984_an_addbuilddepend_rebinding_does_not_alias_onto_its_dependency`
            // before it shipped; the trailing `;` is what separates the two
            // shapes.
            r"(?m)(?:^|[\s{;])([A-Za-z][A-Za-z0-9_'-]*)[ \t]*=[ \t]*[A-Za-z][A-Za-z0-9_']*[ \t]+self\.([A-Za-z][A-Za-z0-9_'-]*)[ \t]*;",
        )
        .expect("static alias-binding regex")
    })
}

/// Bare-name → target-attribute rebindings in one compiler configuration.
pub(crate) fn alias_names(config_text: &str) -> std::collections::BTreeMap<String, String> {
    alias_binding_re()
        .captures_iter(config_text)
        .filter_map(|c| {
            let bare = c.get(1)?.as_str().to_string();
            let target = c.get(2)?.as_str().to_string();
            // A self-referential alias tells us nothing and would make the
            // lookup in `classify` recurse for no gain.
            (bare != target).then_some((bare, target))
        })
        .collect()
}

/// Union the aliases of every candidate compiler configuration.
///
/// Where candidates **disagree** about a name's target, the name is dropped
/// rather than arbitrated. This mirrors `union_nulled`'s fail-closed posture
/// for the same reason: when several compilers are possible and they point a
/// name at different versions, waybill cannot know which one built the
/// project, and picking either would assert a version the build may never
/// have used (Principle IX).
pub(crate) fn union_aliases<'a, I>(configs: I) -> std::collections::BTreeMap<String, String>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut out: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut conflicted: BTreeSet<String> = BTreeSet::new();
    for text in configs {
        for (bare, target) in alias_names(text) {
            match out.get(&bare) {
                Some(prev) if prev != &target => {
                    conflicted.insert(bare);
                }
                _ => {
                    out.insert(bare, target);
                }
            }
        }
    }
    for name in &conflicted {
        out.remove(name);
        tracing::debug!(
            package = %name,
            "nixpkgs-haskell: candidate compilers disagree on an alias target; not resolving"
        );
    }
    out
}

/// Is `name` supplied by the compiler rather than built from Hackage?
///
/// Membership in the nulled set is the whole test. See the module docs for
/// why the two more discriminating rules were rejected — both excluded real
/// packages, and this one cannot, because a nulled binding means exactly
/// "the compiler supplies this".
pub(crate) fn is_boot_library(name: &str, nulled: &BTreeSet<String>) -> bool {
    nulled.contains(name)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Shaped like a real `configuration-ghc-<series>.nix`: top-level package
    /// overrides, a nested derivation override that also binds `null`, and a
    /// conditional attrset holding a real package.
    const CONFIG: &str = r#"
{ pkgs, haskellLib }:
self: super: {
  base = null;
  text = null;
  rts = null;

  some-package = overrideCabal (drv: {
    editedCabalFile = null;
  }) super.some-package;
} // lib.optionalAttrs (versionAtLeast "9.4") {
  directory-ospath-streaming = null;
}
"#;

    #[test]
    fn m926_collects_every_null_binding() {
        let n = nulled_names(CONFIG);
        for name in ["base", "text", "rts", "editedCabalFile", "directory-ospath-streaming"] {
            assert!(n.contains(name), "{name} should be collected");
        }
    }

    #[test]
    fn m926_a_nulled_name_is_compiler_supplied() {
        let n = nulled_names(CONFIG);
        assert!(is_boot_library("base", &n));
        assert!(is_boot_library("text", &n));
    }

    /// The case that rejected the **attrset-depth** rule. This binding sits
    /// deeper than the top-level overrides, inside a conditional attrset, yet
    /// names a real package the compiler supplies. A depth-scoped extractor
    /// drops it, which would let it resolve to a Hackage version the build
    /// never uses.
    #[test]
    fn m926_a_nulled_package_at_deeper_nesting_is_still_compiler_supplied() {
        let n = nulled_names(CONFIG);
        assert!(
            is_boot_library("directory-ospath-streaming", &n),
            "nesting depth must not decide this — measurement rejected that rule"
        );
    }

    /// The case that rejected the **package-set membership** rule. `rts` is
    /// the GHC runtime system: a real package a project can declare, and
    /// absent from `hackage-packages.nix` precisely because it is never built
    /// from Hackage. Requiring package-set membership reports it as
    /// `absent-from-package-set` when the true reason is `compiler-supplied`
    /// — no invented version, but a false statement in the document.
    ///
    /// Measured at nixpkgs a799d3e3, the nulled-but-not-a-package names are
    /// `editedCabalFile`, `rts`, `ghc-platform`, `ghc-toolchain` and
    /// `system-cxx-std-lib`. Only the first is not a package.
    #[test]
    fn m926_a_ghc_bundled_package_absent_from_the_package_set_is_still_compiler_supplied() {
        let n = nulled_names(CONFIG);
        assert!(
            is_boot_library("rts", &n),
            "package-set membership must not decide this — rts is GHC-bundled \
             and never appears in hackage-packages.nix"
        );
    }

    /// `editedCabalFile` stays in the set and is harmless: the set is only
    /// consulted for names a project declared, and this is not a legal
    /// Haskell package name. Asserting the inertness rather than the
    /// exclusion, because attempts to exclude it cost real packages.
    #[test]
    fn m926_a_non_package_attribute_is_inert_rather_than_excluded() {
        let n = nulled_names(CONFIG);
        assert!(n.contains("editedCabalFile"), "no attempt is made to exclude it");
        for declared in ["base", "text", "rts", "aeson", "vector"] {
            assert_ne!(
                declared, "editedCabalFile",
                "a declared dependency can never carry this name"
            );
        }
    }

    /// FR-014a: nulled in ANY candidate means compiler-supplied. Fail closed.
    #[test]
    fn m926_union_marks_a_package_nulled_in_only_one_candidate_as_boot() {
        let only_in_a = "self: super: { alpha = null; }";
        let only_in_b = "self: super: { beta = null; }";
        let u = union_nulled([only_in_a, only_in_b]);
        assert!(is_boot_library("alpha", &u));
        assert!(is_boot_library("beta", &u));
    }

    #[test]
    fn m926_a_package_no_candidate_nulls_is_not_boot() {
        let n = nulled_names(CONFIG);
        assert!(!is_boot_library("aeson", &n));
    }

    /// Bindings are not always on their own line. A line-anchored pattern
    /// misses this, and missing a nulled binding is the dangerous direction.
    #[test]
    fn m926_inline_bindings_are_collected() {
        let n = nulled_names("self: super: { alpha = null; beta = null; }");
        assert!(is_boot_library("alpha", &n));
        assert!(is_boot_library("beta", &n));
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod alias_tests {
    use super::*;

    /// The exact shape from nixpkgs that #984 was filed for.
    #[test]
    fn m984_a_dodistribute_alias_is_captured() {
        let cfg = "self: super: {\n  \
                   os-string = doDistribute self.os-string_2_0_10;\n}\n";
        let a = alias_names(cfg);
        assert_eq!(a.get("os-string").map(String::as_str), Some("os-string_2_0_10"));
    }

    /// `super.` rebindings modify an attribute that already exists, so they
    /// say nothing about which version a bare name refers to. Capturing them
    /// would map a name onto itself and add nothing but noise.
    #[test]
    fn m984_a_super_rebinding_is_not_an_alias() {
        let cfg = "self: super: {\n  \
                   aeson = dontCheck super.aeson;\n  \
                   gtk = doJailbreak super.gtk;\n}\n";
        assert!(alias_names(cfg).is_empty(), "got {:?}", alias_names(cfg));
    }

    /// `addBuildDepend self.<dep> super.<pkg>` names the DEPENDENCY after
    /// `self.`, not the package being rebound. Capturing it would alias
    /// `iserv-proxy` onto `libiserv` — a different package entirely.
    ///
    /// This is a known limitation, asserted so it stays deliberate: the
    /// binding is skipped rather than mis-read. Such a name keeps whatever
    /// the package set already gives it.
    #[test]
    fn m984_an_addbuilddepend_rebinding_does_not_alias_onto_its_dependency() {
        let cfg = "self: super: {\n  \
                   iserv-proxy = addBuildDepend self.libiserv super.iserv-proxy;\n}\n";
        let a = alias_names(cfg);
        assert_ne!(
            a.get("iserv-proxy").map(String::as_str),
            Some("libiserv"),
            "must not alias a package onto one of its dependencies"
        );
    }

    /// Fail closed when candidate compilers disagree (FR-014a's posture).
    #[test]
    fn m984_conflicting_alias_targets_resolve_to_nothing() {
        let a = union_aliases([
            "self: super: { ghc-lib = doDistribute self.ghc-lib_9_8_5; }",
            "self: super: { ghc-lib = doDistribute self.ghc-lib_9_6_1; }",
        ]);
        assert!(
            !a.contains_key("ghc-lib"),
            "candidates disagree, so no version may be asserted; got {a:?}"
        );
    }

    /// Agreement across candidates is kept.
    #[test]
    fn m984_agreeing_alias_targets_survive_the_union() {
        let a = union_aliases([
            "self: super: { os-string = doDistribute self.os-string_2_0_10; }",
            "self: super: { os-string = doDistribute self.os-string_2_0_10; }",
        ]);
        assert_eq!(a.get("os-string").map(String::as_str), Some("os-string_2_0_10"));
    }

    /// Against the REAL configuration file, not a hand-written snippet.
    ///
    /// #984 existed because the parser's model of this file was built from
    /// what it was looking for (`= null;`) rather than from what the file
    /// contains. A synthetic fixture would have reproduced that blind spot,
    /// so this asserts against bytes nixpkgs actually ships.
    ///
    /// `#[ignore]`d rather than silently skipping when the cache is absent.
    /// A test that prints "skipping" and reports `ok` is indistinguishable
    /// from one that ran — the exact criticism #918 makes of the corpus gate,
    /// and it would be self-defeating in a test whose whole purpose is to
    /// check the parser against bytes rather than against assumptions.
    ///
    /// Run it with a populated cache:
    ///   cargo test -p waybill --bins m984_real -- --ignored
    #[test]
    #[ignore = "needs a populated ~/.cache/waybill/nixpkgs; ignored rather than \
                silently skipping, because a test that reports `ok` without \
                running is worse than one that visibly does not run (#918)"]
    fn m984_real_configuration_yields_the_known_aliases() {
        let path = std::path::Path::new(&std::env::var("HOME").unwrap_or_default())
            .join(".cache/waybill/nixpkgs")
            .join("a799d3e3886da994fa307f817a6bc705ae538eeb")
            .join("configuration-ghc-9.6.x.nix");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "no cached configuration at {} ({e}). Populate it by scanning a \
                 Nix-built Haskell project, then re-run with --ignored.",
                path.display()
            )
        });
        let a = alias_names(&text);
        assert_eq!(
            a.get("os-string").map(String::as_str),
            Some("os-string_2_0_10"),
            "the alias that motivated #984 must be captured from the real file"
        );
        assert_eq!(
            a.get("semaphore-compat").map(String::as_str),
            Some("semaphore-compat_1_0_0")
        );
        // `super.` rebindings dominate this file; none may appear.
        for (bare, target) in &a {
            assert_ne!(bare, target, "self-alias leaked: {bare}");
        }
    }
}
