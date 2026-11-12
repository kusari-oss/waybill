//! #1282 — a closure member the pinned package set holds only under a
//! versioned alias is identified as a Hackage package.
//!
//! nixpkgs keeps alternative versions as `<name>_<version with . -> _>`
//! attributes. The compiler-supplied packages have no default attribute at
//! all, so on haskell-language-server's closure `Cabal 3.14.2.0` stayed
//! `pkg:generic` beside its own `Cabal_3_14_2_0`, and HSEC-2026-0006 could not
//! match it. Measured over four pinned revisions, every attribute is
//! `<pname>` or `<pname>_<version>` and no `(pname, version)` is held twice,
//! so the alias for an exact pair is a function of the pair: no index, and
//! nothing to choose between (`specs/1275-closure-hackage-identity/research.md`
//! R4, superseded 2026-11-12).
//!
//! | member | set | outcome |
//! |---|---|---|
//! | `cabal 3.14.2.0` | `cabal_3_12_1_0`, `cabal_3_14_2_0`; no default | confirmed via the alias; declares `syntax` |
//! | `syntax 3.14.2.0` | `syntax_3_14_2_0` only | confirmed; the in-document edge target |
//! | `hashable 1.5.1.0` | default 1.5.1.0, alias `_1_4_7_0` | confirmed via the default (the control) |
//! | `ext 0.1.1.0` | default 0.1.0.2, alias `_0_1_1_0` | confirmed via the alias |
//! | `stale 9.9.9` | `stale_1_0_0` only | `not-in-package-set` |
//! | `cnative 2.0.0` | `cnative_2_0_0`, not GHC-built | `signals-disagree` |

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use common::nix_stub::{NixStub, Scan};
use serde_json::Value;

const IDENTITY: &str = "waybill:closure-identity";

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix_closure_hackage_alias")
}

fn read(name: &str) -> String {
    let p = dir().join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

fn scan() -> Scan {
    let closure = read("closure.json");
    let packages = read("hackage-packages.nix");
    NixStub {
        project: dir().join("closure_only"),
        closure: &closure,
        package_set: Some((&packages, "{ }\n")),
        ..Default::default()
    }
    .scan(&["--nix-closure", "--no-deps-dev", "--no-clearly-defined"])
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

/// The one component named `waybill-fixture-<short>`.
fn member<'a>(cdx: &'a Value, short: &str) -> &'a Value {
    let name = format!("waybill-fixture-{short}");
    let found: Vec<&Value> = components(cdx).into_iter().filter(|c| c["name"] == name).collect();
    assert_eq!(found.len(), 1, "one component named {name}");
    found[0]
}

fn prop(c: &Value, name: &str) -> Option<String> {
    c["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
        .and_then(|p| p["value"].as_str())
        .map(str::to_string)
}

fn license_ids(c: &Value) -> Vec<String> {
    c["licenses"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["license"]["id"].as_str().or_else(|| l["expression"].as_str()))
        .map(str::to_string)
        .collect()
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

/// `Cabal`'s shape: no default attribute, the closure's version held by
/// `<name>_<version>` beside another alias. The identity, license, hash and
/// declared edges all come from the alias that holds the version.
#[test]
fn an_alias_only_exact_pair_is_a_hackage_package() {
    let s = scan();
    let cabal = member(&s.cdx, "cabal");
    assert_eq!(cabal["purl"], "pkg:hackage/waybill-fixture-cabal@3.14.2.0");
    assert_eq!(prop(cabal, IDENTITY).as_deref(), Some("confirmed"));
    // `cabal_3_14_2_0` declares bsd3, `cabal_3_12_1_0` mit: the license says
    // which attribute was read.
    assert_eq!(license_ids(cabal), vec!["BSD-3-Clause"]);
    assert!(
        cabal["hashes"].as_array().is_some_and(|h| !h.is_empty()),
        "the alias's source hash is carried: {cabal}"
    );
    assert_eq!(
        depends_on(&s.cdx, "pkg:hackage/waybill-fixture-cabal@3.14.2.0"),
        vec!["pkg:hackage/waybill-fixture-syntax@3.14.2.0"],
        "declared edges come from the alias's relations"
    );
    let syntax = member(&s.cdx, "syntax");
    assert_eq!(syntax["purl"], "pkg:hackage/waybill-fixture-syntax@3.14.2.0");
}

/// `extensions`' shape: the default attribute holds another version and an
/// alias holds the closure's.
#[test]
fn an_alias_beside_a_default_at_another_version_is_used() {
    let s = scan();
    let ext = member(&s.cdx, "ext");
    assert_eq!(ext["purl"], "pkg:hackage/waybill-fixture-ext@0.1.1.0");
    assert_eq!(prop(ext, IDENTITY).as_deref(), Some("confirmed"));
    assert_eq!(license_ids(ext), vec!["BSD-3-Clause"], "the alias's, not the default's mit");
    assert_eq!(
        depends_on(&s.cdx, "pkg:hackage/waybill-fixture-ext@0.1.1.0"),
        vec!["pkg:hackage/waybill-fixture-text@2.0.2"]
    );
}

/// CONTROL: where the default attribute holds the closure's version, the
/// default is what is read, and an alias at another version contributes
/// nothing.
#[test]
fn the_default_attribute_wins_when_it_holds_the_version() {
    let s = scan();
    let h = member(&s.cdx, "hashable");
    assert_eq!(h["purl"], "pkg:hackage/waybill-fixture-hashable@1.5.1.0");
    assert_eq!(license_ids(h), vec!["BSD-3-Clause"], "the default's, not the alias's mit");
    assert_eq!(
        depends_on(&s.cdx, "pkg:hackage/waybill-fixture-hashable@1.5.1.0"),
        vec!["pkg:hackage/waybill-fixture-text@2.0.2"],
        "no edge from the 1.4.7.0 alias's relations"
    );
}

/// An alias at another version is not an exact pair (FR-004), and an exact
/// alias pair without the GHC signal is not an identity (FR-005).
#[test]
fn an_alias_never_relaxes_the_version_or_ghc_rule() {
    let s = scan();
    let stale = member(&s.cdx, "stale");
    assert_eq!(stale["purl"], "pkg:generic/waybill-fixture-stale@9.9.9");
    assert_eq!(prop(stale, IDENTITY).as_deref(), Some("not-in-package-set"));
    let c = member(&s.cdx, "cnative");
    assert_eq!(c["purl"], "pkg:generic/waybill-fixture-cnative@2.0.0");
    assert_eq!(prop(c, IDENTITY).as_deref(), Some("signals-disagree"));
}
