//! Issue #943 — Hackage package names are case-sensitive, and the reader was
//! lowercasing every one of them.
//!
//! Measured on `haskell/aeson` at v2.3.2.0, three declared dependencies were
//! affected, and they fail in two different ways:
//!
//! | declared | emitted was | `hackage.haskell.org/package/…` |
//! |---|---|---|
//! | `QuickCheck` | `pkg:hackage/quickcheck` | `QuickCheck` 200, `quickcheck` **404** |
//! | `OneTuple`   | `pkg:hackage/onetuple`   | `OneTuple` 200, `onetuple` **404** |
//! | `Diff`       | `pkg:hackage/diff`       | `Diff` 200, `diff` **200** |
//!
//! The third row is the dangerous one. `Diff` and `diff` are both real and
//! DISTINCT packages, so lowercasing did not fail loudly there — it emitted a
//! valid-looking identifier for the wrong package. A consumer matching
//! advisories against `pkg:hackage/diff` gets a confident wrong answer, which
//! is worse than the 404 the other two produce.
//!
//! These tests assert the emitted identifier is byte-equal to the declared
//! spelling. Asserting only that a component EXISTS would pass under both the
//! old and new behaviour and prove nothing.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

fn binary_path() -> &'static str { env!("CARGO_BIN_EXE_waybill") }

fn scan(root: &Path) -> serde_json::Value {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!("m943-{}-{seq}.json", std::process::id()));
    let st = Command::new(binary_path())
        .args(["sbom", "scan", "--path", root.to_str().unwrap(), "--offline",
               "--format", "cyclonedx-json", "--output", out.to_str().unwrap()])
        .status().unwrap();
    assert!(st.success(), "scan failed: {st:?}");
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

fn purls(v: &serde_json::Value) -> BTreeSet<String> {
    v["components"].as_array().unwrap().iter()
        .filter_map(|c| c.get("purl").and_then(|p| p.as_str()).map(str::to_string))
        .collect()
}

/// The declaration path: a capitalised `build-depends:` entry keeps its
/// spelling in the emitted identifier.
#[test]
fn a_capitalised_dependency_keeps_its_spelling_in_the_purl_m943() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("app.cabal"),
        "name: waybill-fixture-app\nversion: 1.0\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends: QuickCheck, OneTuple, Diff, waybill-fixture-lower\n"
    ).unwrap();

    let p = purls(&scan(d.path()));

    for expected in ["pkg:hackage/QuickCheck", "pkg:hackage/OneTuple", "pkg:hackage/Diff"] {
        assert!(p.contains(expected),
            "expected `{expected}` byte-for-byte. Hackage names are \
             case-sensitive: `quickcheck` and `onetuple` resolve to nothing, and \
             `diff` resolves to a DIFFERENT package than `Diff`. purls={p:?}");
    }
    assert!(p.contains("pkg:hackage/waybill-fixture-lower"),
        "an already-lowercase name was altered. purls={p:?}");
}

/// The lockfile path: a freeze pinning a capitalised name must emit it
/// capitalised too. The freeze parser lowercased independently of the
/// declaration path, so fixing only one would leave the other wrong.
#[test]
fn a_capitalised_freeze_pin_keeps_its_spelling_in_the_purl_m943() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("app.cabal"),
        "name: waybill-fixture-app\nversion: 1.0\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends: QuickCheck\n").unwrap();
    std::fs::write(d.path().join("cabal.project.freeze"),
        "constraints: any.QuickCheck ==2.14.3\n").unwrap();

    let p = purls(&scan(d.path()));
    assert!(p.contains("pkg:hackage/QuickCheck@2.14.3"),
        "the freeze path lowercased a pinned package name. \
         purls={p:?}");
    assert!(!p.iter().any(|s| s.starts_with("pkg:hackage/quickcheck")),
        "a lowercased identifier is still present. purls={p:?}");
}

/// Identity preserves case; MATCHING still folds it. A freeze that spells a
/// pin differently from the manifest must still suppress the declaration,
/// rather than emitting the dependency twice.
#[test]
fn matching_against_a_freeze_pin_remains_case_insensitive_m943() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("app.cabal"),
        "name: waybill-fixture-app\nversion: 1.0\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends: QuickCheck >=2.14\n").unwrap();
    std::fs::write(d.path().join("cabal.project.freeze"),
        "constraints: any.quickcheck ==2.14.3\n").unwrap();

    let rep = scan(d.path());
    let matching: Vec<_> = rep["components"].as_array().unwrap().iter()
        .filter(|c| c["name"].as_str()
            .is_some_and(|n| n.eq_ignore_ascii_case("quickcheck")))
        .collect();

    assert_eq!(matching.len(), 1,
        "one dependency produced {} components. Preserving case in the \
         IDENTIFIER must not stop the pin from MATCHING the declaration — \
         those are separate concerns. components={matching:?}",
        matching.len());
}

/// The self-reference / local-package filter (#936) also compares names, and
/// must keep working when two manifests spell the same package differently.
#[test]
fn the_local_package_filter_is_case_insensitive_m943() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    std::fs::write(r.join("lib.cabal"),
        "name: Waybill-Fixture-Lib\nversion: 0.1\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends: waybill-fixture-dep\n").unwrap();
    let ex = r.join("example");
    std::fs::create_dir_all(&ex).unwrap();
    std::fs::write(ex.join("example.cabal"),
        "name: waybill-fixture-example\nversion: 0.1\nlicense: BSD-3-Clause\n\n\
         executable example\n  build-depends: waybill-fixture-lib\n").unwrap();

    let rep = scan(r);
    let libs: Vec<_> = rep["components"].as_array().unwrap().iter()
        .filter(|c| c["name"].as_str()
            .is_some_and(|n| n.eq_ignore_ascii_case("waybill-fixture-lib")))
        .collect();

    assert_eq!(libs.len(), 1,
        "the example's `build-depends:` on the library — spelled in a different \
         case than the library declares itself — minted a second, versionless \
         component. The #936 filter must fold case. components={libs:?}");
}
