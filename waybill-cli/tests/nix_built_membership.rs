//! Spec 1664 (#1731): a Haskell package a Nix file builds is a project member.
//!
//! Positive control (FR-008). `cabal.project` says `packages: */*.cabal`, which
//! reaches one level. Under `tools/`, two levels down:
//! - (i) `waybill-fixture-built` — `callCabal2nix` on a literal path, from two
//!   `.nix` files: a member, with C18 naming both and C248.
//! - (ii) `waybill-fixture-attr` — only a `src =` in a non-build attribute.
//! - (iii) `waybill-fixture-interp` — only an interpolated path.
//! - (v) `empty` — a `callCabal2nix` names it, but it holds no manifest.
//!
//! And (iv) `lib/`, which the glob matches and Nix also names: a member with no
//! C248, because Nix made no difference.

#![allow(clippy::unwrap_used)]

use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::process::Command;

use serde_json::Value;

const ROOT: &str = "waybill-fixture-root";

fn cabal(name: &str, deps: &str) -> String {
    format!("cabal-version: 2.4\nname: {name}\nversion: 0.1.0\n\nlibrary\n  build-depends: base{deps}\n")
}

fn write(root: &Path, files: &[(&str, String)]) {
    for (rel, text) in files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
}

fn base_files() -> Vec<(&'static str, String)> {
    vec![
        ("cabal.project", "packages: */*.cabal\n".into()),
        ("lib/waybill-fixture-lib.cabal", cabal("waybill-fixture-lib", "")),
        ("tools/waybill-fixture-built/waybill-fixture-built.cabal", cabal("waybill-fixture-built", ", waybill-fixture-dep")),
        ("tools/waybill-fixture-attr/waybill-fixture-attr.cabal", cabal("waybill-fixture-attr", "")),
        ("tools/waybill-fixture-interp/waybill-fixture-interp.cabal", cabal("waybill-fixture-interp", "")),
        ("tools/empty/README", "no manifest here\n".into()),
    ]
}

fn nix_files() -> Vec<(&'static str, String)> {
    vec![
        (
            "nix/pkgs/built.nix",
            "{ haskellPackages }: haskellPackages.callCabal2nix \"waybill-fixture-built\" ../../tools/waybill-fixture-built { }\n".into(),
        ),
        (
            "nix/overlay.nix",
            "root: final: prev: {\n\
             \x20 built = prev.callCabal2nix \"waybill-fixture-built\" ../tools/waybill-fixture-built { };\n\
             \x20 lib = prev.callCabal2nix \"waybill-fixture-lib\" ../lib { };\n\
             \x20 empty = prev.callCabal2nix \"empty\" ../tools/empty { };\n\
             \x20 attr = final.stdenv.mkDerivation { name = \"a\"; src = ../tools/waybill-fixture-attr; };\n\
             \x20 interp = prev.callCabal2nix \"waybill-fixture-interp\" \"${root}/tools/waybill-fixture-interp\" { };\n\
             \x20 # old = prev.callCabal2nix \"waybill-fixture-attr\" ../tools/waybill-fixture-attr { };\n\
             }\n"
                .into(),
        ),
    ]
}

fn scan(root: &Path) -> Value {
    let out = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let cdx = out.path().join("w.cdx.json");
    let o = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .env("HOME", home.path())
        .env("PATH", "")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(root)
        .args(["--no-deep-hash", "--format", "cyclonedx-json", "--output"])
        .arg(format!("cyclonedx-json={}", cdx.display()))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&std::fs::read(cdx).unwrap()).unwrap()
}

fn components(v: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    let mut stack = vec![v];
    while let Some(v) = stack.pop() {
        match v {
            Value::Object(m) => {
                if m.contains_key("purl") {
                    out.push(v);
                }
                stack.extend(m.values());
            }
            Value::Array(a) => stack.extend(a.iter()),
            _ => {}
        }
    }
    out
}

fn prop<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c["properties"].as_array()?.iter().find(|p| p["name"] == name)?["value"].as_str()
}

fn main_modules(doc: &Value) -> Vec<&Value> {
    components(doc)
        .into_iter()
        .filter(|c| prop(c, "waybill:source-type") == Some("hackage-main-module"))
        .collect()
}

fn named<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    main_modules(doc).into_iter().find(|c| c["name"] == name)
}

#[test]
fn a_package_nix_builds_is_a_member_and_nothing_else_is() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(ROOT);
    let mut files = base_files();
    files.extend(nix_files());
    write(&root, &files);
    let doc = scan(&root);

    // (i) admitted, with both .nix files and C248.
    let built = named(&doc, "waybill-fixture-built").expect("(i) is a member");
    assert_eq!(prop(built, "waybill:membership-source"), Some("nix-callcabal2nix"));
    let files: Vec<String> = serde_json::from_str(prop(built, "waybill:source-files").unwrap()).unwrap();
    for f in ["nix/overlay.nix", "nix/pkgs/built.nix", "tools/waybill-fixture-built/waybill-fixture-built.cabal"] {
        assert!(files.iter().any(|p| p == f), "{f} missing from {files:?}");
    }
    let mut sorted = files.clone();
    sorted.sort();
    assert_eq!(files, sorted, "source-files sorted");

    // (i)'s dependency is emitted and reached from it.
    let dep = components(&doc)
        .into_iter()
        .find(|c| c["purl"].as_str().is_some_and(|p| p.starts_with("pkg:hackage/waybill-fixture-dep")))
        .expect("(i)'s dependency is emitted");
    let built_ref = built["bom-ref"].as_str().unwrap();
    let dep_ref = dep["bom-ref"].as_str().unwrap();
    let edges: HashSet<&str> = doc["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["ref"] == built_ref)
        .flat_map(|d| d["dependsOn"].as_array().unwrap().iter().filter_map(Value::as_str))
        .collect();
    assert!(edges.contains(dep_ref), "(i) -> its dependency");
    assert_eq!(prop(dep, "waybill:membership-source"), None, "a dependency was not admitted by anything");

    // (iv) admitted by cabal.project: no C248.
    let lib = named(&doc, "waybill-fixture-lib").expect("(iv) is a member");
    assert_eq!(prop(lib, "waybill:membership-source"), None);

    // (ii) and (iii) excluded; exactly one of the three two-level candidates.
    let two_level: BTreeSet<&str> = main_modules(&doc)
        .into_iter()
        .filter_map(|c| c["name"].as_str())
        .filter(|n| ["waybill-fixture-built", "waybill-fixture-attr", "waybill-fixture-interp"].contains(n))
        .collect();
    assert_eq!(two_level, BTreeSet::from(["waybill-fixture-built"]));

    // (v) admits nothing.
    assert!(
        !components(&doc).into_iter().any(|c| prop(c, "waybill:source-files").is_some_and(|f| f.contains("tools/empty"))),
        "a Nix path with no manifest admitted something"
    );
}

/// Research R6: where the rule rejects nothing, the `.nix` files change
/// nothing.
#[test]
fn nix_files_change_nothing_when_nothing_is_rejected() {
    let key = |doc: &Value| -> (Vec<String>, String) {
        let mut cs: Vec<String> = components(doc).iter().map(|c| {
            let mut c = (*c).clone();
            c.as_object_mut().unwrap().remove("bom-ref");
            c.to_string()
        }).collect();
        cs.sort();
        (cs, doc["dependencies"].to_string())
    };
    let base = vec![
        ("cabal.project", "packages: */*.cabal\n".to_string()),
        ("lib/waybill-fixture-lib.cabal", cabal("waybill-fixture-lib", "")),
    ];
    let a = tempfile::tempdir().unwrap();
    write(&a.path().join(ROOT), &base);
    let b = tempfile::tempdir().unwrap();
    let mut with_nix = base.clone();
    with_nix.push(("nix/lib.nix", "p: p.callCabal2nix \"waybill-fixture-lib\" ../lib { }\n".to_string()));
    write(&b.path().join(ROOT), &with_nix);
    assert_eq!(key(&scan(&a.path().join(ROOT))), key(&scan(&b.path().join(ROOT))));
}
