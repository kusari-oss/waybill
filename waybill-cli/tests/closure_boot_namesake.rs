//! #1466 — a declared Haskell dependency never resolves to a nameless closure
//! member that happens to carry the same name.
//!
//! `nix-closure-shellcheck` had 24 `pkg:hackage` libraries depending on
//! `pkg:generic/time-<hash>`: a live-bootstrap C fragment (C206 `fragment`)
//! named `time`, which is also the name of the GHC boot library they declare.
//! `nix-closure-moat` and `nix-closure-slacklinker` showed the same collision
//! one step earlier: the fragment merged into the manifest's versionless
//! `pkg:hackage/time`, which then carried `waybill:closure-member-role:
//! fragment`. Both are a name standing in for an identity.
//!
//! The fixture reproduces both shapes with one closure: `closure_only/` has no
//! manifest (shellcheck), `declared/` declares the boot library (moat).
//! `waybill-fixture-clock` is the boot library (its compiler configuration
//! nulls it); `waybill-fixture-gone` is not, and nothing in the closure but a
//! nameless member carries its name.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use common::nix_stub::{NixStub, Scan};
use serde_json::Value;

const MEMBER_ROLE: &str = "waybill:closure-member-role";
const CLOSURE_ROLE: &str = "waybill:closure-role";
const LIB: &str = "pkg:hackage/waybill-fixture-lib@1.0.0";
const CLOCK: &str = "pkg:hackage/waybill-fixture-clock";
const CLOCK_FRAGMENT_PREFIX: &str = "pkg:generic/waybill-fixture-clock-";
const GONE_FRAGMENT_PREFIX: &str = "pkg:generic/waybill-fixture-gone-";

/// `waybill-fixture-clock` is nulled, which is how milestone 926 identifies a
/// GHC boot library from the revision's own configuration.
const BOOT_CONFIG: &str = "self: super: {\n  waybill-fixture-clock = null;\n}\n";

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix_closure_boot_namesake")
}

fn read(name: &str) -> String {
    let p = dir().join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

fn scan(project: &str) -> Scan {
    let closure = read("closure.json");
    let packages = read("hackage-packages.nix");
    NixStub {
        project: dir().join(project),
        closure: &closure,
        package_set: Some((&packages, BOOT_CONFIG)),
        ..Default::default()
    }
    .scan(&["--nix-closure", "--no-deps-dev", "--no-clearly-defined"])
}

/// Every CycloneDX component, nested ones included.
fn components(cdx: &Value) -> Vec<&Value> {
    fn walk<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
        if let Some(cs) = v["components"].as_array() {
            for c in cs {
                out.push(c);
                walk(c, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(cdx, &mut out);
    out
}

fn purls(cdx: &Value) -> Vec<String> {
    components(cdx).iter().filter_map(|c| c["purl"].as_str()).map(str::to_string).collect()
}

fn component<'a>(cdx: &'a Value, purl: &str) -> Vec<&'a Value> {
    components(cdx).into_iter().filter(|c| c["purl"] == purl).collect()
}

fn prop(c: &Value, name: &str) -> Option<String> {
    c["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
        .and_then(|p| p["value"].as_str())
        .map(str::to_string)
}

fn depends_on(cdx: &Value, from: &str) -> Vec<String> {
    cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["ref"] == from)
        .filter_map(|d| d["dependsOn"].as_array())
        .flatten()
        .filter_map(|t| t.as_str())
        .map(str::to_string)
        .collect()
}

/// SPDX 2.3 `DEPENDS_ON` targets of the package carrying `from`, as PURLs.
fn spdx23_depends_on(spdx: &Value, from: &str) -> Vec<String> {
    let purl_of: std::collections::HashMap<&str, &str> = spdx["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let id = p["SPDXID"].as_str()?;
            let purl = p["externalRefs"]
                .as_array()?
                .iter()
                .find(|r| r["referenceType"] == "purl")?["referenceLocator"]
                .as_str()?;
            Some((id, purl))
        })
        .collect();
    spdx["relationships"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["relationshipType"] == "DEPENDS_ON")
        .filter(|r| r["spdxElementId"].as_str().and_then(|i| purl_of.get(i)) == Some(&from))
        .filter_map(|r| r["relatedSpdxElement"].as_str().and_then(|i| purl_of.get(i)))
        .map(|p| p.to_string())
        .collect()
}

/// The nameless member is in the document, under its own hash-bearing PURL.
/// Asserted before any absence, so an edge missing because the member was
/// never emitted cannot pass for one the binding declined.
fn require_nameless(scan: &Scan, prefix: &str) -> String {
    let found: Vec<String> =
        purls(&scan.cdx).into_iter().filter(|p| p.starts_with(prefix)).collect();
    assert_eq!(found.len(), 1, "one nameless member under {prefix}; document holds {:?}", purls(&scan.cdx));
    let c = component(&scan.cdx, &found[0]);
    assert_eq!(prop(c[0], MEMBER_ROLE).as_deref(), Some("fragment"), "{}", found[0]);
    found[0].clone()
}

/// The shellcheck shape: no manifest, so nothing but the fragment carries the
/// boot library's name. The edge goes to the compiler-supplied component
/// milestone 1275 FR-012c synthesises for an absent boot library — the same
/// component the 25 other boot targets on that golden already bind to.
#[test]
fn a_boot_target_binds_to_the_compiler_supplied_library_not_a_nameless_namesake() {
    let s = scan("closure_only");
    let fragment = require_nameless(&s, CLOCK_FRAGMENT_PREFIX);

    let deps = depends_on(&s.cdx, LIB);
    assert!(
        !deps.contains(&fragment),
        "{LIB} depends on the nameless fragment {fragment}: {deps:?}"
    );
    assert!(deps.contains(&CLOCK.to_string()), "{LIB} lost its declared boot edge: {deps:?}");

    let clock = component(&s.cdx, CLOCK);
    assert_eq!(clock.len(), 1, "exactly one {CLOCK}");
    assert_eq!(
        prop(clock[0], "waybill:haskell-version-unresolved-reason").as_deref(),
        Some("compiler-supplied")
    );

    // The three formats carry one graph.
    let spdx = spdx23_depends_on(&s.spdx23, LIB);
    assert!(spdx.contains(&CLOCK.to_string()), "SPDX 2.3: {spdx:?}");
    assert!(!spdx.contains(&fragment), "SPDX 2.3: {spdx:?}");
}

/// A target that is not a boot library and has no Haskell component binds to
/// nothing: its edge is omitted (and counted, FR-012c), never pointed at a
/// fragment that shares the name.
#[test]
fn a_non_boot_target_binds_to_nothing_rather_than_a_nameless_namesake() {
    let s = scan("closure_only");
    let fragment = require_nameless(&s, GONE_FRAGMENT_PREFIX);

    let deps = depends_on(&s.cdx, LIB);
    assert!(!deps.contains(&fragment), "{LIB} depends on {fragment}: {deps:?}");
    assert!(
        !purls(&s.cdx).iter().any(|p| p.starts_with("pkg:hackage/waybill-fixture-gone")),
        "no component may be invented for a bare name"
    );
    assert!(!spdx23_depends_on(&s.spdx23, LIB).contains(&fragment));
}

/// The moat shape: the manifest already declares the boot library. The
/// nameless member must stay its own component (milestone 1282 FR-011: the
/// members it adds never deduplicate against another reader's components), so
/// the manifest's component describes the library and nothing else.
#[test]
fn a_nameless_member_never_merges_into_the_manifest_component_of_its_name() {
    let s = scan("declared");
    let fragment = require_nameless(&s, CLOCK_FRAGMENT_PREFIX);

    let clock = component(&s.cdx, CLOCK);
    assert_eq!(clock.len(), 1, "exactly one {CLOCK}");
    for ann in [MEMBER_ROLE, CLOSURE_ROLE] {
        assert_eq!(
            prop(clock[0], ann),
            None,
            "{CLOCK} carries the nameless member's {ann}: {}",
            clock[0]
        );
    }
    // CONTROL: the manifest's own description survived, so the assertions
    // above are about the right component.
    assert_eq!(prop(clock[0], "waybill:sbom-tier").as_deref(), Some("design"));

    let deps = depends_on(&s.cdx, LIB);
    assert!(deps.contains(&CLOCK.to_string()), "{deps:?}");
    assert!(!deps.contains(&fragment), "{deps:?}");
}
