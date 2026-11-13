//! #1046 — a Nix-built Haskell dependency whose overlay pins a version the
//! nixpkgs package set does not carry is one component, not two.
//!
//! | member | declared | pinned set | closure | expected |
//! |---|---|---|---|---|
//! | `overlaid` | `>= 2.0.0.0` | 1.0.0.0 | 2.1.0.0, GHC-built, git source | merged; Hackage decides the identity |
//! | `relaxed` | `>=1.0 && <1.5` | 1.6 | 1.6 (exact pair, `confirmed`) | merged; C221, the build is outside the range |
//! | `namesake` | (no range) | absent | 1.0, **not** GHC-built, tarball source | not merged: not a Haskell package |
//! | `variants` | `>= 3.0` | 2.0 | 3.1 and 3.2, both in range | not merged: two candidates |
//!
//! The closure is JSON handed to a stub `nix`; Hackage is a local stub or a
//! closed port, never the real service (Principle VII).

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use common::nix_stub::{NixStub, Scan};
use serde_json::Value;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure_haskell_overlay")
}

fn read(name: &str) -> String {
    let p = dir().join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

fn scan_with(extra: &[&str]) -> Scan {
    let closure = read("closure.json");
    let packages = read("hackage-packages.nix");
    let mut args = vec!["--nix-closure", "--no-deps-dev", "--no-clearly-defined"];
    args.extend_from_slice(extra);
    NixStub {
        project: dir().join("project"),
        closure: &closure,
        package_set: Some((&packages, "{ }\n")),
        ..Default::default()
    }
    .scan(&args)
}

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

fn named<'a>(cdx: &'a Value, short: &str) -> Vec<&'a Value> {
    let name = format!("waybill-fixture-{short}");
    components(cdx).into_iter().filter(|c| c["name"] == name).collect()
}

fn prop(c: &Value, name: &str) -> Option<String> {
    c["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
        .and_then(|p| p["value"].as_str())
        .map(str::to_string)
}

#[test]
#[ignore = "diagnostic: prints the fixture's current shape"]
fn diagnostic_print_shape() {
    let s = scan_with(&[]);
    for short in ["overlaid", "relaxed", "namesake", "variants"] {
        for c in named(&s.cdx, short) {
            eprintln!(
                "{short}: purl={} tier={:?} identity={:?} ranges={:?} unresolved={:?} refs={}",
                c["purl"],
                prop(c, "waybill:sbom-tier"),
                prop(c, "waybill:closure-identity"),
                prop(c, "waybill:requirement-ranges"),
                prop(c, "waybill:haskell-version-unresolved-reason"),
                c["externalReferences"]
            );
        }
    }
    for c in components(&s.cdx) {
        if prop(c, "waybill:closure-member-role").as_deref() == Some("source") {
            eprintln!("source member: {} refs={}", c["name"], c["externalReferences"]);
        }
    }
}
