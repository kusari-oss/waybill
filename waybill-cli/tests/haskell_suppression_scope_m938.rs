//! Issue #938 — a lockfile is authoritative about what it describes, and says
//! nothing about anything else.
//!
//! Design-tier emission used to be gated on a repository-global flag:
//!
//! ```ignore
//! let any_successful_lockfile =
//!     !successful_freeze_dirs.is_empty() || !stack_lock_dirs.is_empty();
//! if !has_local_successful_lockfile && !any_successful_lockfile { … }
//! ```
//!
//! That was wrong twice over, and the two failures compound.
//!
//! - **Across directories.** A lockfile anywhere in the tree suppressed
//!   design-tier emission everywhere, so a package in an unrelated directory
//!   with no lockfile of its own lost its dependencies outright — suppression
//!   fired with nothing to replace what it silenced. Cabal itself scopes
//!   `cabal.project.freeze` to the project that owns it.
//!
//! - **Within one directory.** Suppression was all-or-nothing, so a dependency
//!   the lockfile does not mention was dropped too: no pin (the lockfile is
//!   silent on it) and no fallback (the lockfile suppressed it).
//!
//! Measured on a public cabal+hpack library with a 3-constraint freeze at the
//! root: 5 components before, 21 after. Sixteen components were being deleted
//! by the act of adding a partial lockfile.
//!
//! The rule both cases share: a dependency keeps its design-tier entry unless
//! the lockfile GOVERNING its manifest supplies a pin FOR THAT DEPENDENCY.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

fn binary_path() -> &'static str { env!("CARGO_BIN_EXE_waybill") }

fn scan(root: &Path) -> serde_json::Value {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!("m938-{}-{seq}.json", std::process::id()));
    let st = Command::new(binary_path())
        .args(["sbom", "scan", "--path", root.to_str().unwrap(), "--offline",
               "--format", "cyclonedx-json", "--output", out.to_str().unwrap()])
        .status().unwrap();
    assert!(st.success(), "scan failed: {st:?}");
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

fn names(v: &serde_json::Value) -> BTreeSet<String> {
    v["components"].as_array().unwrap().iter()
        .map(|c| c["name"].as_str().unwrap_or_default().to_string()).collect()
}

/// Name -> version for components that carry one.
fn versioned(v: &serde_json::Value) -> BTreeSet<(String, String)> {
    v["components"].as_array().unwrap().iter()
        .filter_map(|c| {
            let n = c["name"].as_str()?;
            let ver = c.get("version").and_then(|x| x.as_str()).filter(|s| !s.is_empty())?;
            Some((n.to_string(), ver.to_string()))
        }).collect()
}

/// Case 1 — a freeze beside the ROOT package must not silence a SIBLING
/// package that has no freeze of its own.
#[test]
fn a_freeze_in_one_directory_does_not_silence_a_sibling_package_m938() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    std::fs::write(r.join("root.cabal"),
        "name: waybill-fixture-root\nversion: 1.0\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends: waybill-fixture-alpha\n").unwrap();
    std::fs::write(r.join("cabal.project.freeze"),
        "constraints: any.waybill-fixture-alpha ==1.2.3\n").unwrap();

    // The sibling declares the SAME dependency the root pins. That is what
    // makes this test sensitive to the SCOPING rule specifically: if the
    // root's freeze were treated as governing the whole tree, `alpha` would be
    // considered pinned here and the sibling's declaration suppressed. Were
    // the sibling to declare only names the root does not pin, the
    // per-dependency rule alone would keep them and this test would pass even
    // with repository-global scoping — proving nothing. (It did exactly that
    // in its first draft; the mutation check caught it.)
    let sib = r.join("sibling");
    std::fs::create_dir_all(&sib).unwrap();
    std::fs::write(sib.join("sibling.cabal"),
        "name: waybill-fixture-sibling\nversion: 2.0\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends: waybill-fixture-alpha >=9.0 && <10.0\n").unwrap();

    let rep = scan(r);
    let n = names(&rep);
    assert!(n.contains("waybill-fixture-alpha"), "names={n:?}");

    // The sibling has no freeze, so its declaration must survive as a
    // design-tier entry carrying its own constraint — the root's pin governs
    // the root only.
    let ranges: BTreeSet<String> = rep["components"].as_array().unwrap().iter()
        .filter(|c| c["name"].as_str() == Some("waybill-fixture-alpha"))
        .flat_map(|c| c["properties"].as_array().map(|a| a.to_vec()).unwrap_or_default())
        .filter(|p| p["name"].as_str() == Some("waybill:requirement-ranges"))
        .filter_map(|p| serde_json::from_str::<Vec<String>>(p["value"].as_str()?).ok())
        .flatten().collect();

    assert!(ranges.iter().any(|r| r.contains("9.0")),
        "the sibling package has NO freeze of its own, yet its declaration was \
         suppressed by the ROOT's freeze — which governs the root, not the \
         sibling. Cabal scopes `cabal.project.freeze` to the project that owns \
         it. ranges={ranges:?}");
}

/// Case 2 — within ONE directory, a dependency the freeze does not pin keeps
/// its design-tier entry. Four declared, two pinned, four emitted.
#[test]
fn a_dependency_absent_from_the_freeze_keeps_its_design_tier_entry_m938() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    std::fs::write(r.join("app.cabal"),
        "name: waybill-fixture-app\nversion: 1.0\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends: waybill-fixture-alpha, waybill-fixture-beta, \
         waybill-fixture-gamma, waybill-fixture-delta\n").unwrap();
    std::fs::write(r.join("cabal.project.freeze"),
        "constraints: any.waybill-fixture-alpha ==1.0.0,\n             \
         any.waybill-fixture-beta ==2.0.0\n").unwrap();

    let rep = scan(r);
    let n = names(&rep);

    for dep in ["waybill-fixture-alpha", "waybill-fixture-beta",
                "waybill-fixture-gamma", "waybill-fixture-delta"] {
        assert!(n.contains(dep),
            "`{dep}` is declared in build-depends and vanished: the freeze does \
             not pin it, so it got no version, and the freeze suppressed it, so \
             it got no design-tier entry either. names={n:?}");
    }

    let v = versioned(&rep);
    assert!(v.contains(&("waybill-fixture-alpha".into(), "1.0.0".into())),
        "the pinned dependency lost its pin. versioned={v:?}");
    assert!(v.contains(&("waybill-fixture-beta".into(), "2.0.0".into())),
        "the pinned dependency lost its pin. versioned={v:?}");
    assert!(!v.iter().any(|(n, _)| n == "waybill-fixture-gamma"),
        "an UNPINNED dependency acquired a version. The freeze says nothing \
         about it, so claiming one would be invention. versioned={v:?}");
}

/// The property the issue asks for directly: adding a partial freeze must
/// never REMOVE information. It may add pins; it may not delete components.
#[test]
fn adding_a_partial_freeze_never_removes_components_m938() {
    fn tree(freeze: Option<&str>) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        std::fs::write(r.join("app.cabal"),
            "name: waybill-fixture-app\nversion: 1.0\nlicense: BSD-3-Clause\n\n\
             library\n  build-depends: waybill-fixture-alpha, waybill-fixture-beta, \
             waybill-fixture-gamma\n").unwrap();
        if let Some(f) = freeze { std::fs::write(r.join("cabal.project.freeze"), f).unwrap(); }
        d
    }

    let without = names(&scan(tree(None).path()));
    let with = names(&scan(
        tree(Some("constraints: any.waybill-fixture-alpha ==1.0.0\n")).path()));

    let lost: Vec<_> = without.difference(&with).cloned().collect();
    assert!(lost.is_empty(),
        "adding a freeze DELETED components. Pinning a project must not make \
         its SBOM worse than not pinning it. lost={lost:?}");
    assert_eq!(with.len(), without.len(),
        "component count changed when a freeze was added. with={with:?} \
         without={without:?}");
}

/// A range constraint is not a pin. It carries the same class of information
/// `build-depends:` already does — an acceptable set, not a chosen member — so
/// it must not silence the manifest's own declaration.
#[test]
fn a_range_constraint_in_the_freeze_does_not_suppress_the_declaration_m938() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    // The manifest's range and the freeze's range DIFFER on purpose. If a
    // range constraint were treated as a pin, the manifest's declaration would
    // be suppressed and only the freeze's range would survive — so the
    // manifest's range is the observable that distinguishes the two
    // behaviours. With identical ranges the union looks the same either way
    // and the test proves nothing; that was its first draft, and the mutation
    // check caught it.
    std::fs::write(r.join("app.cabal"),
        "name: waybill-fixture-app\nversion: 1.0\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends: waybill-fixture-alpha >=1.0 && <2.0\n").unwrap();
    std::fs::write(r.join("cabal.project.freeze"),
        "constraints: any.waybill-fixture-alpha >=1.5 && <1.9\n").unwrap();

    let rep = scan(r);
    let n = names(&rep);
    assert!(n.contains("waybill-fixture-alpha"),
        "a range-only constraint suppressed the declaration it merely restates. \
         names={n:?}");

    // Name-presence alone proves nothing here: the freeze path used to emit a
    // component for the range too, sanitising it into the version slot as
    // `pkg:hackage/…@>=1.0_&&_<2.0`. That is syntactically a PURL, semantically
    // meaningless, and resolvable against nothing — the shape m895 removed from
    // the `build-depends:` path. So assert the SHAPE, not just the name.
    // (The first draft of this test asserted only presence and passed under a
    // mutation that made ranges suppress; the mutation check caught it.)
    let refs: BTreeSet<String> = rep["components"].as_array().unwrap().iter()
        .filter(|c| c["name"].as_str() == Some("waybill-fixture-alpha"))
        .map(|c| c["bom-ref"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(refs.len(), 1,
        "one declared dependency produced more than one component. A range \
         constraint and the declaration it restates are the same dependency. \
         refs={refs:?}");
    assert!(!refs.iter().next().unwrap().contains("&&"),
        "the surviving component carries a RANGE in its version slot. A \
         constraint describes an acceptable set; a version names one member of \
         it, and PURLs model only the latter. refs={refs:?}");

    let v = versioned(&rep);
    assert!(!v.iter().any(|(n, _)| n == "waybill-fixture-alpha"),
        "an unpinned dependency acquired a version. versioned={v:?}");

    let ranges: BTreeSet<String> = rep["components"].as_array().unwrap().iter()
        .filter(|c| c["name"].as_str() == Some("waybill-fixture-alpha"))
        .flat_map(|c| c["properties"].as_array().map(|a| a.to_vec()).unwrap_or_default())
        .filter(|p| p["name"].as_str() == Some("waybill:requirement-ranges"))
        .filter_map(|p| serde_json::from_str::<Vec<String>>(p["value"].as_str()?).ok())
        .flatten().collect();
    assert!(ranges.iter().any(|r| r.contains("2.0")),
        "the MANIFEST's own range is gone — the freeze's range constraint \
         suppressed the declaration it merely restates. ranges={ranges:?}");
    assert!(ranges.iter().any(|r| r.contains("1.9")),
        "the FREEZE's range is gone. ranges={ranges:?}");
}
