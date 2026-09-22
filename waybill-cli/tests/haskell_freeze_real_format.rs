//! Issue #937 — a real `cabal.project.freeze` must yield MORE information than
//! no freeze file, never less.
//!
//! Before this fix, adding a real freeze file to a Haskell project deleted 19
//! of 22 components from the SBOM. Two defects combined:
//!
//! 1. Real `cabal v2-freeze` output qualifies every constraint by scope
//!    (`any.aeson ==2.2.3.0`, `setup.Cabal ==3.10.3.0`). Every constraint
//!    regex anchored on `^([A-Za-z][A-Za-z0-9-]*)`, which cannot match a name
//!    containing a dot, so the parser extracted nothing.
//! 2. `Ok(vec![])` counted as a *successful* lockfile parse, which suppressed
//!    the design-tier fallback — so zero pins plus suppression meant zero
//!    dependencies.
//!
//! **The fixtures below are the actual point of this file.** The pre-existing
//! unit tests and the m143 integration fixture both used hand-written,
//! unqualified constraint text — a format `cabal` never emits — so both layers
//! passed against output that does not exist. A parser test whose fixture did
//! not come from the tool is testing the fixture.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

fn binary_path() -> &'static str { env!("CARGO_BIN_EXE_waybill") }

/// Verbatim shape of `cabal v2-freeze` output: `active-repositories` header,
/// `any.`/`setup.` qualifiers, multi-line continuation, trailing `index-state`.
const REAL_FREEZE: &str = "\
active-repositories: hackage.haskell.org:merge
constraints: any.aeson ==2.2.3.0,
             any.base ==4.18.2.1,
             any.bytestring ==0.11.5.3,
             any.text ==2.0.2,
             setup.Cabal ==3.10.3.0,
             any.aeson +ordered-keymap
index-state: hackage.haskell.org 2026-09-01T00:00:00Z
";

fn fixture(freeze: Option<&str>) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    std::fs::write(
        r.join("app.cabal"),
        "name: waybill-fixture-app\nversion: 1.2.3\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends: base, text, aeson, bytestring\n",
    ).unwrap();
    if let Some(f) = freeze {
        std::fs::write(r.join("cabal.project.freeze"), f).unwrap();
    }
    d
}

fn scan(root: &Path) -> serde_json::Value {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!("m937-{}-{seq}.json", std::process::id()));
    let st = Command::new(binary_path())
        .args(["sbom", "scan", "--path", root.to_str().unwrap(), "--offline",
               "--format", "cyclonedx-json", "--output", out.to_str().unwrap()])
        .status().unwrap();
    assert!(st.success(), "scan failed: {st:?}");
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

fn names(v: &serde_json::Value) -> Vec<String> {
    v["components"].as_array().unwrap().iter()
        .map(|c| c["name"].as_str().unwrap_or("").to_string()).collect()
}

fn pinned(v: &serde_json::Value) -> Vec<(String, String)> {
    v["components"].as_array().unwrap().iter()
        .filter(|c| c.get("version").and_then(|x| x.as_str()).is_some())
        .filter(|c| c["name"] != "waybill-fixture-app")
        .map(|c| (c["name"].as_str().unwrap().to_string(),
                  c["version"].as_str().unwrap().to_string()))
        .collect()
}

/// #937 row A — the case users hit. Real freeze output must produce pins.
#[test]
fn a_real_cabal_freeze_produces_pinned_versions_m937() {
    let d = fixture(Some(REAL_FREEZE));
    let rep = scan(d.path());
    let p: std::collections::BTreeMap<_, _> = pinned(&rep).into_iter().collect();

    assert!(!p.is_empty(),
        "a real `cabal v2-freeze` file produced ZERO pinned components. \
         Its constraints are scope-qualified (`any.aeson ==…`); a parser that \
         only matches unqualified names extracts nothing from real output. \
         Got components: {:?}", names(&rep));

    assert_eq!(p.get("aeson").map(String::as_str), Some("2.2.3.0"));
    assert_eq!(p.get("text").map(String::as_str), Some("2.0.2"));
    assert_eq!(p.get("base").map(String::as_str), Some("4.18.2.1"));
    assert_eq!(p.get("bytestring").map(String::as_str), Some("0.11.5.3"));
    assert_eq!(p.get("Cabal").or(p.get("cabal")).map(String::as_str), Some("3.10.3.0"),
        "the `setup.` qualifier must be handled too, not just `any.`");
}

/// The invariant the whole issue reduces to: pinning must never lose
/// information. Row A against row C.
#[test]
fn pinning_never_yields_fewer_components_than_not_pinning_m937() {
    let with = scan(fixture(Some(REAL_FREEZE)).path());
    let without = scan(fixture(None).path());

    let (nw, nwo) = (names(&with).len(), names(&without).len());
    assert!(nw >= nwo,
        "adding a real cabal.project.freeze REDUCED the component count from \
         {nwo} to {nw}. A lockfile must add precision, never delete \
         dependencies.\n  with:    {:?}\n  without: {:?}",
        names(&with), names(&without));
    assert!(pinned(&with).len() > pinned(&without).len(),
        "and it must add versions the unpinned scan did not have");
}

/// #937 defect 2 — a freeze that parses but yields nothing must fall back,
/// not silence the dependencies it failed to describe.
#[test]
fn a_freeze_yielding_no_constraints_falls_back_to_design_tier_m937() {
    // Well-formed enough to parse: has the keyword, no usable entries.
    let empty_freeze = "active-repositories: hackage.haskell.org:merge\nconstraints:\n";
    let with = scan(fixture(Some(empty_freeze)).path());
    let without = scan(fixture(None).path());

    assert_eq!(
        names(&with).len(), names(&without).len(),
        "a freeze file yielding no constraints must behave as if absent \
         (FR-009's fallback), not suppress design-tier emission.\n  \
         with: {:?}\n  without: {:?}", names(&with), names(&without),
    );
    for dep in ["base", "text", "aeson", "bytestring"] {
        assert!(names(&with).iter().any(|n| n == dep),
            "{dep} vanished when an empty freeze file was present: {:?}", names(&with));
    }
}
