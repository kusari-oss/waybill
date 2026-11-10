//! Milestone 1281 (#1346) — licenses for nix closure components.
//!
//! The closure JSON carries no license data at all, so every closure
//! component was emitted unlicensed. Measured on the pinned target, 96
//! components another tool licenses from the same flake were silent here.
//!
//! **Every assertion is on the emitted document.** The join itself is
//! unit-tested in `closure/licenses.rs`; what this suite exists to catch is
//! the step after it — whether a joined license reaches the component a
//! consumer actually receives, in each format's own license construct. A join
//! test stays green while nothing is emitted.
//!
//! The fixtures are JSON handed to a stub `nix`, **never evaluated by a real
//! one**: on 2026-11-04 evaluating a fixture flake rewrote a committed
//! `flake.lock` and broke 11 tests in an unrelated file.

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
        .join("tests/fixtures/nix_closure_licenses")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The suite's accessors, over the shared stub's [`Scan`].
trait LicenseView {
    fn components(&self) -> &Vec<Value>;
    fn component(&self, name: &str) -> &Value;
    fn cdx_licenses(&self, name: &str) -> Vec<String>;
    fn cdx_license_slots(&self, name: &str) -> Vec<&'static str>;
    fn spdx23_license(&self, name: &str) -> Option<String>;
    fn spdx3_declared(&self, name: &str) -> Vec<String>;
    fn coverage(&self) -> Value;
}

impl LicenseView for Scan {
    fn components(&self) -> &Vec<Value> {
        self.cdx["components"].as_array().unwrap()
    }

    fn component(&self, name: &str) -> &Value {
        self.components()
            .iter()
            .find(|c| c["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("no component named {name}"))
    }

    /// CycloneDX licenses for a component, as emitted identifiers or names.
    fn cdx_licenses(&self, name: &str) -> Vec<String> {
        self.component(name)["licenses"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|l| {
                        l["license"]["id"]
                            .as_str()
                            .or_else(|| l["license"]["name"].as_str())
                            .or_else(|| l["expression"].as_str())
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether the CDX entry used the `id` slot (a listed SPDX identifier) or
    /// the `name` slot (a preserved declaration).
    fn cdx_license_slots(&self, name: &str) -> Vec<&'static str> {
        self.component(name)["licenses"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|l| {
                        if l["license"]["id"].is_string() {
                            "id"
                        } else if l["license"]["name"].is_string() {
                            "name"
                        } else {
                            "other"
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn spdx23_license(&self, name: &str) -> Option<String> {
        self.spdx23["packages"]
            .as_array()?
            .iter()
            .find(|p| p["name"].as_str() == Some(name))?["licenseDeclared"]
            .as_str()
            .map(str::to_string)
    }

    /// SPDX 3 license expressions reachable from the named package through a
    /// `hasDeclaredLicense` relationship.
    fn spdx3_declared(&self, name: &str) -> Vec<String> {
        let graph = self.spdx3["@graph"].as_array().cloned().unwrap_or_default();
        let Some(pkg) = graph
            .iter()
            .find(|e| e["name"].as_str() == Some(name) && e["type"].as_str() == Some("software_Package"))
        else {
            return Vec::new();
        };
        let id = pkg["spdxId"].as_str().unwrap_or_default().to_string();
        let targets: Vec<String> = graph
            .iter()
            .filter(|e| {
                e["type"].as_str() == Some("Relationship")
                    && e["relationshipType"].as_str() == Some("hasDeclaredLicense")
                    && e["from"].as_str() == Some(id.as_str())
            })
            .flat_map(|e| {
                e["to"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .collect();
        graph
            .iter()
            .filter(|e| {
                e["spdxId"]
                    .as_str()
                    .is_some_and(|s| targets.iter().any(|t| t == s))
            })
            .filter_map(|e| {
                e["simplelicensing_licenseExpression"]
                    .as_str()
                    .or_else(|| e["name"].as_str())
                    .map(str::to_string)
            })
            .collect()
    }

    /// The C184 document record's license coverage block.
    fn coverage(&self) -> Value {
        let props = self.cdx["metadata"]["properties"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let raw = props
            .iter()
            .find(|p| p["name"].as_str() == Some("waybill:nix-closure"))
            .and_then(|p| p["value"].as_str())
            .unwrap_or("{}");
        let v: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
        v["licenses"].clone()
    }
}

fn scan(closure_json: &str, rows: &str) -> Scan {
    scan_with(closure_json, rows, &["--nix-closure"])
}

/// `rows` is what the stub returns for the license evaluation; passing an
/// empty string makes that call fail, which is how US3 is exercised.
fn scan_with(closure_json: &str, rows: &str, extra: &[&str]) -> Scan {
    NixStub {
        project: project(),
        closure: closure_json,
        rows: if rows.is_empty() { Rows::Fail } else { Rows::Json(rows) },
        attr: DEFAULT_ATTR,
    }
    .scan(extra)
}

// ---------------------------------------------------------------- US1

/// T020 / US1 scenario 1 — the feature, at its simplest.
#[test]
fn a_member_declared_mit_carries_the_spdx_identifier() {
    let s = scan(&fixture("closure.json"), &fixture("rows.json"));
    assert_eq!(s.cdx_licenses("waybill-fixture-alpha"), vec!["MIT"]);
}

/// T021 / US1 scenario 2, FR-005 — all of them, not the first.
#[test]
fn several_declared_licenses_are_all_carried() {
    let s = scan(&fixture("closure.json"), &fixture("rows.json"));
    assert_eq!(
        s.cdx_licenses("waybill-fixture-beta"),
        vec!["Apache-2.0", "MIT"],
        "a component declaring two licenses must carry both, in nixpkgs' order"
    );
}

/// T023 / FR-002 — **the** correctness property.
///
/// Two members share a `pname` and differ only by version and store path, and
/// are licensed differently. A name join gives both the same license and
/// nothing in the document says which one is wrong; this is the `autoconf269`
/// case from research R5, reduced to a fixture.
#[test]
fn the_join_is_the_store_path_and_not_the_name() {
    let s = scan(&fixture("closure.json"), &fixture("rows.json"));
    let one = s.cdx_licenses("waybill-fixture-twin");
    assert!(
        one == vec!["BSD-3-Clause"] || one == vec!["GPL-3.0-only"],
        "each twin must take the license of its own store path, got {one:?}"
    );
    // Both versions are emitted as distinct components, and they must not
    // have collapsed onto one license.
    let twins: Vec<Vec<String>> = s
        .components()
        .iter()
        .filter(|c| c["name"].as_str() == Some("waybill-fixture-twin"))
        .map(|c| {
            c["licenses"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|l| l["license"]["id"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(twins.len(), 2, "both twin versions should be emitted");
    assert_ne!(
        twins[0], twins[1],
        "the two store paths declare different licenses; a name join would make them equal"
    );
}

/// T022 / US1 scenario 3, FR-010, SC-004.
#[test]
fn two_scans_of_one_closure_emit_identical_license_sets() {
    let a = scan(&fixture("closure.json"), &fixture("rows.json"));
    let b = scan(&fixture("closure.json"), &fixture("rows.json"));
    let licenses = |s: &Scan| -> Vec<(String, Vec<String>)> {
        let mut v: Vec<(String, Vec<String>)> = s
            .components()
            .iter()
            .map(|c| {
                (
                    format!("{}@{}", c["name"], c["version"]),
                    c["licenses"]
                        .as_array()
                        .map(|a| a.iter().map(|l| l.to_string()).collect())
                        .unwrap_or_default(),
                )
            })
            .collect();
        v.sort();
        v
    };
    assert_eq!(licenses(&a), licenses(&b));
}

/// T049 / FR-009, Constitution Principle V — the native construct in all
/// three formats, and no `waybill:` property carrying a license.
#[test]
fn licenses_land_in_each_formats_native_construct() {
    let s = scan(&fixture("closure.json"), &fixture("rows.json"));

    assert_eq!(s.cdx_licenses("waybill-fixture-alpha"), vec!["MIT"]);
    assert_eq!(
        s.spdx23_license("waybill-fixture-alpha").as_deref(),
        Some("MIT"),
        "SPDX 2.3 must carry it in licenseDeclared"
    );
    assert!(
        s.spdx3_declared("waybill-fixture-alpha")
            .iter()
            .any(|e| e.contains("MIT")),
        "SPDX 3 must carry it through hasDeclaredLicense, got {:?}",
        s.spdx3_declared("waybill-fixture-alpha")
    );

    // Principle V: a native field, never an annotation.
    let props = s.component("waybill-fixture-alpha")["properties"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !props
            .iter()
            .any(|p| p["name"].as_str().is_some_and(|n| n.contains("license"))),
        "a license must not be emitted as a waybill: property: {props:?}"
    );
}

// ---------------------------------------------------------------- US2

/// T029 / US2 scenario 1, FR-003 — `publicDomain` has no SPDX identifier and
/// must not acquire an invented one.
#[test]
fn a_declaration_without_an_spdx_id_is_preserved_not_faked() {
    let s = scan(&fixture("closure.json"), &fixture("rows.json"));
    let got = s.cdx_licenses("waybill-fixture-gamma");
    assert_eq!(got, vec!["Public Domain"], "the declaration must survive");
    assert_eq!(
        s.cdx_license_slots("waybill-fixture-gamma"),
        vec!["name"],
        "an unmappable declaration belongs in the name slot, never the id slot \
         that asserts a listed SPDX identifier"
    );
}

/// T030 / US2 scenario 2 — the mappable case still maps, so the test above
/// is not passing merely because nothing canonicalises.
#[test]
fn a_mappable_declaration_still_reaches_the_id_slot() {
    let s = scan(&fixture("closure.json"), &fixture("rows.json"));
    assert_eq!(s.cdx_license_slots("waybill-fixture-alpha"), vec!["id"]);
}

// ---------------------------------------------------------------- US3

/// T036 / US3 scenario 1, SC-005, FR-006 — the document survives, with its
/// components, and says why the licenses are missing.
#[test]
fn a_failing_evaluation_still_emits_the_document_and_records_why() {
    let s = scan(&fixture("closure.json"), "");
    assert!(
        s.components()
            .iter()
            .any(|c| c["name"].as_str() == Some("waybill-fixture-alpha")),
        "the component set must be unaffected by a license failure"
    );
    assert!(s.cdx_licenses("waybill-fixture-alpha").is_empty());
    let cov = s.coverage();
    assert!(
        cov["degraded"].is_string(),
        "the reason must be in the document, not only in the log: {cov}"
    );
    assert_eq!(cov["received"].as_u64(), Some(0));
    assert!(
        cov["wanted"].as_u64().unwrap_or(0) > 0,
        "wanted must still count the members that were eligible: {cov}"
    );
}

/// T038 / FR-008 — an evaluation that succeeds and returns nothing is a
/// different outcome from one that failed, and from a closure whose members
/// genuinely declare nothing. All three look alike without this record.
#[test]
fn an_empty_result_reports_zero_coverage_explicitly_rather_than_silence() {
    let s = scan(&fixture("closure.json"), "[]");
    let cov = s.coverage();
    assert_eq!(cov["received"].as_u64(), Some(0));
    assert!(
        cov["wanted"].as_u64().unwrap_or(0) > 0,
        "members were offered a license even though none came back: {cov}"
    );
    assert!(
        cov["degraded"].is_null(),
        "an empty result is not a degradation; conflating them hides a real failure: {cov}"
    );
}

/// The successful case records coverage too — otherwise the two tests above
/// would pass against a build that never records anything but zero.
#[test]
fn a_successful_lookup_reports_what_it_licensed() {
    let s = scan(&fixture("closure.json"), &fixture("rows.json"));
    let cov = s.coverage();
    assert_eq!(
        cov["received"].as_u64(),
        Some(5),
        "five fixture members are licensed: {cov}"
    );
    assert!(cov["wanted"].as_u64().unwrap_or(0) >= 5);
    assert_eq!(cov["withheld-ambiguous"].as_u64(), Some(0));
    assert!(cov["degraded"].is_null());
}

/// T037 / US3 scenario 2, FR-007 — without `--nix-closure` the tier must not
/// run at all, so nothing it contributes can reach the document.
#[test]
fn a_scan_without_the_closure_flag_carries_no_closure_licenses() {
    let s = scan_with(&fixture("closure.json"), &fixture("rows.json"), &[]);
    assert!(
        !s.components()
            .iter()
            .any(|c| c["name"].as_str() == Some("waybill-fixture-alpha")),
        "the closure tier must not have run"
    );
}

/// T031 / SC-003 — every emitted identifier traces to an authority.
///
/// SC-003 gates on **provenance**, not on membership of milestone 1273's
/// mapping table: FR-014 makes nixpkgs' own `spdxId` the authority precisely
/// so a second table is not maintained beside it, so an identifier nixpkgs
/// states that the table does not hold is correct and must pass. What must
/// never happen is an identifier waybill invented.
#[test]
fn every_emitted_identifier_came_from_the_declaration() {
    let rows: Value = serde_json::from_str(&fixture("rows.json")).unwrap();
    let declared: std::collections::HashSet<String> = rows
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["licenses"].as_array().cloned().unwrap_or_default())
        .filter_map(|l| {
            l["spdx"]
                .as_str()
                .or_else(|| l["name"].as_str())
                .map(str::to_string)
        })
        .collect();

    let s = scan(&fixture("closure.json"), &fixture("rows.json"));
    for c in s.components() {
        let name = c["name"].as_str().unwrap_or_default();
        if !name.starts_with("waybill-fixture-") {
            continue;
        }
        for emitted in s.cdx_licenses(name) {
            assert!(
                declared.contains(&emitted),
                "{name} carries {emitted:?}, which no declaration states; \
                 an invented identifier is how a license field stops being evidence"
            );
        }
    }
}

/// T039 — a member nixpkgs does not describe carries no license, and that is
/// distinguishable from a lookup that never ran.
///
/// Not every closure member is a nixpkgs package; bootstrap and fetch
/// derivations are not. Absence of a license here is a fact about the member,
/// and the coverage record is what separates it from absence of a lookup.
#[test]
fn a_member_nixpkgs_does_not_describe_is_unlicensed_but_accounted_for() {
    let s = scan(&fixture("closure.json"), &fixture("rows.json"));
    assert!(
        s.cdx_licenses("waybill-fixture-delta").is_empty(),
        "no row describes delta, so it must carry no license"
    );
    let cov = s.coverage();
    assert_eq!(
        cov["received"].as_u64(),
        Some(5),
        "the lookup plainly ran and licensed five members: {cov}"
    );
    assert!(
        cov["wanted"].as_u64().unwrap_or(0) > cov["received"].as_u64().unwrap_or(0),
        "the shortfall must be visible rather than inferred from silence: {cov}"
    );
}

/// T043 / SC-007 and T044 / SC-002 — this feature adds licenses, and nothing
/// else.
///
/// The comparison is between a run whose lookup succeeded and one whose
/// lookup failed, so the only difference between the two documents is the
/// licenses themselves. If the component set or the C182 role census moves,
/// the feature is doing something it was not asked to do; and no component
/// may end up with *fewer* licenses than the run that acquired none.
#[test]
fn the_feature_adds_licenses_and_changes_nothing_else() {
    let with = scan(&fixture("closure.json"), &fixture("rows.json"));
    let without = scan(&fixture("closure.json"), "");

    let identities = |s: &Scan| -> Vec<String> {
        let mut v: Vec<String> = s
            .components()
            .iter()
            .map(|c| format!("{}@{}", c["name"], c["version"]))
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        identities(&with),
        identities(&without),
        "SC-007: the component set must be identical"
    );

    let census = |s: &Scan| -> std::collections::BTreeMap<String, usize> {
        let mut m = std::collections::BTreeMap::new();
        for c in s.components() {
            if let Some(role) = c["properties"].as_array().and_then(|ps| {
                ps.iter()
                    .find(|p| p["name"] == "waybill:closure-role")
                    .and_then(|p| p["value"].as_str())
            }) {
                *m.entry(role.to_string()).or_insert(0) += 1;
            }
        }
        m
    };
    assert_eq!(
        census(&with),
        census(&without),
        "SC-007: the C182 role census must be unchanged"
    );
    assert!(
        !census(&with).is_empty(),
        "a census of nothing would make the assertion above vacuous"
    );

    // SC-002 — nothing loses ground.
    for c in without.components() {
        let name = c["name"].as_str().unwrap_or_default();
        let before = without.cdx_licenses(name).len();
        let after = with.cdx_licenses(name).len();
        assert!(
            after >= before,
            "{name} carried {before} licenses without the lookup and {after} with it"
        );
    }
}
