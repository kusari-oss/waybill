//! Milestone 1377 (#1398) — the subject of a `--nix-closure` scan is the
//! package the evaluated attribute built.
//!
//! Measured before this milestone: the openssh wrapper flake's subject was
//! `filesystem-scan 0.0.0` with a `waybill:` placeholder CPE, and the
//! `openssh 10.3p1` derivation was an ordinary component, so grype reported
//! 0 findings on the scanned package where sbomnix's document gave 28.
//!
//! Every assertion is on the emitted documents, through a stub `nix`.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use common::nix_stub::{NixStub, Rows, Scan, DEFAULT_ATTR};
use serde_json::Value;

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure_role_scope/project")
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_closure_subject")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn scan_closure(closure: &str, attr: &str, extra: &[&str]) -> Scan {
    let rows = fixture("rows.json");
    let mut args = vec!["--nix-closure"];
    args.extend_from_slice(extra);
    NixStub {
        project: project(),
        closure,
        rows: Rows::Json(&rows),
        attr,
        ..Default::default()
    }
    .scan(&args)
}

fn scan(closure_fixture: &str) -> Scan {
    scan_closure(&fixture(closure_fixture), DEFAULT_ATTR, &[])
}

/// The subject as each format names it: `(name, version, purl)`.
fn subjects(s: &Scan) -> [(String, String, String); 3] {
    let str_of = |v: &Value| v.as_str().unwrap_or_default().to_string();

    let m = &s.cdx["metadata"]["component"];
    let cdx = (str_of(&m["name"]), str_of(&m["version"]), str_of(&m["purl"]));

    let described = str_of(&s.spdx23["documentDescribes"][0]);
    let pkg = s.spdx23["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["SPDXID"].as_str() == Some(described.as_str()))
        .unwrap_or_else(|| panic!("SPDX 2.3 describes {described}, which is not a package"));
    let purl23 = pkg["externalRefs"]
        .as_array()
        .and_then(|refs| refs.iter().find(|r| r["referenceType"] == "purl"))
        .map(|r| str_of(&r["referenceLocator"]))
        .unwrap_or_default();
    let spdx23 = (str_of(&pkg["name"]), str_of(&pkg["versionInfo"]), purl23);

    let graph = s.spdx3["@graph"].as_array().unwrap();
    let doc = graph
        .iter()
        .find(|e| e["type"] == "SpdxDocument")
        .expect("an SpdxDocument");
    let root_id = str_of(&doc["rootElement"][0]);
    let root = graph
        .iter()
        .find(|e| e["spdxId"].as_str() == Some(root_id.as_str()))
        .unwrap_or_else(|| panic!("SPDX 3 rootElement {root_id} is not in the graph"));
    let purl3 = root["software_packageUrl"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_default();
    let spdx3 = (
        str_of(&root["name"]),
        str_of(&root["software_packageVersion"]),
        purl3,
    );
    [cdx, spdx23, spdx3]
}

fn heuristic(s: &Scan) -> Option<String> {
    s.cdx["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == "waybill:root-selection-heuristic")
        .and_then(|p| p["value"].as_str())
        .and_then(|v| serde_json::from_str::<Value>(v).ok())
        .and_then(|v| v["heuristic"].as_str().map(str::to_string))
}

/// US1 scenario 1, FR-001, FR-002 — one package, three formats.
#[test]
fn the_evaluated_package_is_the_subject_in_every_format() {
    let s = scan("closure.json");
    for (format, (name, version, purl)) in ["cdx", "spdx-2.3", "spdx-3"].iter().zip(subjects(&s)) {
        assert_eq!(name, "waybill-fixture-app", "{format} subject name");
        assert_eq!(version, "2.1", "{format} subject version");
        assert_eq!(
            purl, "pkg:generic/waybill-fixture-app@2.1",
            "{format} subject identity is the one the closure tier assigns"
        );
    }
}

/// Clarification 2026-11-09 — the metadata lookup's coverage counts the
/// evaluated package. The fixture root is `unreferenced` and applies no
/// patch, so before milestone 1377 it was neither emitted nor counted.
#[test]
fn the_evaluated_package_is_counted_in_metadata_coverage() {
    let s = scan("closure.json");
    let c184: Value = s.cdx["metadata"]["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "waybill:nix-closure")
        .and_then(|p| p["value"].as_str())
        .and_then(|v| serde_json::from_str(v).ok())
        .expect("a C184 record");
    // lib, tool and the root app; all three have a row.
    assert_eq!(c184["licenses"]["wanted"], 3, "{}", c184["licenses"]);
    assert_eq!(c184["licenses"]["received"], 3, "{}", c184["licenses"]);
    assert_eq!(
        s.cdx["metadata"]["component"]["licenses"][0]["license"]["id"], "MIT",
        "the subject carries the license its row declares"
    );
}

/// FR-012 — no CPE from either source means no CPE, not a placeholder.
///
/// The gh shape: the row reaches the subject and declares no CPE, and the
/// curated table has no row for the name.
#[test]
fn a_subject_without_a_cpe_carries_none_and_no_placeholder() {
    let mut rows: Value = serde_json::from_str(&fixture("rows.json")).unwrap();
    for r in rows.as_array_mut().unwrap() {
        r["cpe"] = Value::Null;
    }
    let rows = rows.to_string();
    let s = NixStub {
        project: project(),
        closure: &fixture("closure.json"),
        rows: Rows::Json(&rows),
        attr: DEFAULT_ATTR,
        ..Default::default()
    }
    .scan(&["--nix-closure"]);
    assert!(
        s.cdx["metadata"]["component"].get("cpe").is_none(),
        "metadata.component carries {:?}",
        s.cdx["metadata"]["component"]["cpe"]
    );
    for (format, doc) in [("spdx-2.3", &s.spdx23), ("spdx-3", &s.spdx3)] {
        let text = doc.to_string();
        assert!(
            !text.contains("cpe:2.3:a:waybill:"),
            "{format} carries a waybill-vendor placeholder CPE"
        );
    }
}

/// The fixture project plus a second main module at its root, a Cargo
/// workspace-root package. With two workspace-root main-modules the ladder picks by ecosystem
/// priority, which ranks `cargo` and not `hackage`, so the cabal package can
/// be the subject only through the evaluated-package tag.
fn two_main_module_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for entry in std::fs::read_dir(project()).unwrap() {
        let entry = entry.unwrap();
        let to = dir.path().join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&to).unwrap();
            for inner in std::fs::read_dir(entry.path()).unwrap() {
                let inner = inner.unwrap();
                std::fs::copy(inner.path(), to.join(inner.file_name())).unwrap();
            }
        } else {
            std::fs::copy(entry.path(), &to).unwrap();
        }
    }
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"waybill-fixture-other\"\nversion = \"0.2.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    dir
}

/// FR-003, US1 scenario 3 — the moat shape. The root's `(name, version)` is
/// the project's cabal package, so the closure component merges into the
/// reader's. One component, and it is the subject, which is only possible
/// if the tag survived `merge_closure_components`.
#[test]
fn an_evaluated_package_that_is_also_the_main_module_is_one_subject() {
    let dir = two_main_module_project();
    let rows = fixture("rows.json");
    let s = NixStub {
        project: dir.path().to_path_buf(),
        closure: &fixture("closure-mainmodule.json"),
        rows: Rows::Json(&rows),
        attr: DEFAULT_ATTR,
        ..Default::default()
    }
    .scan(&["--nix-closure"]);
    let [cdx, _, _] = subjects(&s);
    assert_eq!(cdx.0, "waybill-fixture-roleproj");
    assert_eq!(cdx.1, "0.1.0");
    assert_eq!(
        cdx.2, "pkg:hackage/waybill-fixture-roleproj@0.1.0",
        "the reader's identity is kept"
    );
    assert_eq!(heuristic(&s).as_deref(), Some("nix-evaluated-package"));
    // FR-003's "no duplicate": the closure root merged into the reader's
    // component instead of adding a `pkg:generic` twin. A subject chosen
    // from several main-modules is also listed in `components[]`; that is
    // today's shape for every such scan, closure or not, so it is one entry
    // here and not zero.
    let purls: Vec<&str> = s.cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"] == "waybill-fixture-roleproj")
        .filter_map(|c| c["purl"].as_str())
        .collect();
    assert_eq!(purls, vec!["pkg:hackage/waybill-fixture-roleproj@0.1.0"]);
}

/// The control for the test above: without `--nix-closure` the same project
/// does NOT make the cabal package the subject, so the test above passes
/// because of the closure, not because the fixture already did.
#[test]
fn control_without_a_closure_the_ladder_picks_the_other_main_module() {
    let dir = two_main_module_project();
    let s = NixStub {
        project: dir.path().to_path_buf(),
        closure: &fixture("closure-mainmodule.json"),
        rows: Rows::Fail,
        attr: DEFAULT_ATTR,
        ..Default::default()
    }
    .scan(&[]);
    let [cdx, _, _] = subjects(&s);
    assert_ne!(cdx.2, "pkg:hackage/waybill-fixture-roleproj@0.1.0", "{cdx:?}");
    assert_ne!(heuristic(&s).as_deref(), Some("nix-evaluated-package"));
}

/// Edge case — a degraded closure names no package, so the subject is
/// exactly today's.
#[test]
fn a_degraded_closure_leaves_todays_subject() {
    let degraded = NixStub {
        project: project(),
        closure: &fixture("closure.json"),
        rows: Rows::Fail,
        // No attribute matches, so `derivation show` fails and the tier
        // degrades with "no evaluable attribute".
        attr: "packages.aarch64-darwin.waybill-fixture-absent",
        ..Default::default()
    }
    .scan(&["--nix-closure"]);
    let without = NixStub {
        project: project(),
        closure: &fixture("closure.json"),
        rows: Rows::Fail,
        attr: DEFAULT_ATTR,
        ..Default::default()
    }
    .scan(&[]);
    assert_eq!(subjects(&degraded), subjects(&without));
    assert_ne!(heuristic(&degraded).as_deref(), Some("nix-evaluated-package"));
}

/// Principle III — with two unconsumed derivations the closure does not say
/// which was evaluated, so no evaluated package is named and the subject is
/// today's. The first `.drv` key in sort order is a hash, not an answer.
#[test]
fn a_closure_with_two_roots_names_no_evaluated_package() {
    let two = scan("closure-two-roots.json");
    let without = NixStub {
        project: project(),
        closure: &fixture("closure-two-roots.json"),
        rows: Rows::Fail,
        attr: DEFAULT_ATTR,
        ..Default::default()
    }
    .scan(&[]);
    assert_eq!(subjects(&two)[0].2, subjects(&without)[0].2);
    assert_ne!(subjects(&two)[0].0, "waybill-fixture-app");
    assert_ne!(heuristic(&two).as_deref(), Some("nix-evaluated-package"));
}

/// Edge case — a NixOS configuration's subject is its toplevel derivation,
/// under the identity the closure tier assigns a member with no `pname`.
#[test]
fn a_nixos_configurations_subject_is_its_toplevel() {
    let attr = "nixosConfigurations.waybill-fixture-host.config.system.build.toplevel";
    let s = scan_closure(&fixture("closure-nixos.json"), attr, &["--nix-closure-attr", attr]);
    let all = subjects(&s);
    for (format, (name, _, purl)) in ["cdx", "spdx-2.3", "spdx-3"].iter().zip(all.iter()) {
        assert_eq!(name, "nixos-system-waybill-fixture-host-1.0", "{format} subject");
        assert!(
            purl.starts_with("pkg:generic/nixos-system-waybill-fixture-host-1.0"),
            "{format} subject identity: {purl}"
        );
    }
    assert_eq!(all[0].2, all[1].2);
    assert_eq!(all[0].2, all[2].2);
}

/// FR-001 — `--root-name` still decides.
#[test]
fn an_operator_override_still_decides_the_subject() {
    let s = scan_closure(
        &fixture("closure.json"),
        DEFAULT_ATTR,
        &["--root-name", "waybill-fixture-override", "--root-version", "9.9"],
    );
    let [cdx, _, _] = subjects(&s);
    assert_eq!(cdx.0, "waybill-fixture-override");
    assert_eq!(cdx.1, "9.9");
}

/// The selection tag drives the ladder and never reaches a document.
#[test]
fn the_internal_tag_appears_in_no_output() {
    for f in ["closure.json", "closure-mainmodule.json"] {
        let s = scan(f);
        for (format, doc) in [("cdx", &s.cdx), ("spdx-2.3", &s.spdx23), ("spdx-3", &s.spdx3)] {
            assert!(
                !doc.to_string().contains("waybill:nix-evaluated-package"),
                "{f}: {format} leaks the internal tag"
            );
        }
    }
}
