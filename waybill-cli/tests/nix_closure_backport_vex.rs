//! #1436 — closure-backport VEX that scanners honour, without overclaiming.
//!
//! A closure component whose patches resolve a CVE carries a `nix_drv` PURL
//! qualifier naming every patching build it stands for, and each CVE that
//! every one of those builds resolves gets one `fixed` statement on that
//! qualified PURL. Every assertion is on the emitted documents, through the
//! stub `nix`, against `tests/fixtures/nix_closure_backport_vex/closure.json`:
//!
//! - `waybill-fixture-zip 6.0`: two variants, both patching `CVE-2099-0001`;
//! - `waybill-fixture-ssh 1.0`: one variant, `CVE-2099-0002` and `-0003`;
//! - `waybill-fixture-split 2.0`: two patching variants, only one of which
//!   resolves `CVE-2099-0004`;
//! - `waybill-fixture-plain 1.0`: a patch naming no CVE;
//! - `waybill-fixture-zip 6.1`: unpatched, the leak case;
//! - `waybill-fixture-app 1.0`: the evaluated package, consuming them all.
//!
//! The project is a wrapper flake (no language manifest), so the document has
//! no main module.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::nix_stub::{NixStub, Rows, Scan, DEFAULT_ATTR};
use serde_json::Value;

const ZIP_Q: &str = "pkg:generic/waybill-fixture-zip@6.0?nix_drv=\
                     zipa0000000000000000000000000001,zipb0000000000000000000000000002";
const SSH_Q: &str = "pkg:generic/waybill-fixture-ssh@1.0?nix_drv=ssh00000000000000000000000000003";
const SPLIT_Q: &str = "pkg:generic/waybill-fixture-split@2.0?nix_drv=\
                       splda000000000000000000000000004,spldb000000000000000000000000005";
const PLAIN: &str = "pkg:generic/waybill-fixture-plain@1.0";
const ZIP_61: &str = "pkg:generic/waybill-fixture-zip@6.1";
const SSH_OUT: &str = "/nix/store/vsh00000000000000000000000000003-waybill-fixture-ssh-1.0";

fn closure() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_closure_backport_vex/closure.json");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The closure without `waybill-fixture-split`: nothing is unscoped.
fn closure_without_split() -> String {
    let mut c: Value = serde_json::from_str(&closure()).unwrap();
    let drvs = c["derivations"].as_object_mut().unwrap();
    drvs.retain(|k, _| !k.contains("waybill-fixture-split"));
    c.to_string()
}

/// A wrapper flake: the 1377 fixture project's flake, with no language
/// manifest beside it, so no main module.
fn wrapper_flake() -> tempfile::TempDir {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure_role_scope/project");
    let dir = tempfile::tempdir().unwrap();
    for f in ["flake.nix", "flake.lock"] {
        std::fs::copy(src.join(f), dir.path().join(f)).unwrap();
    }
    dir
}

struct VexScan {
    scan: Scan,
    openvex: Option<Value>,
    _project: tempfile::TempDir,
    _out: tempfile::TempDir,
}

fn scan_with(closure: &str, declarations: Option<&str>, extra: &[&str]) -> VexScan {
    let project = wrapper_flake();
    let out = tempfile::tempdir().unwrap();
    let sidecar = out.path().join("waybill.openvex.json");
    let sidecar_arg = format!("openvex={}", sidecar.display());
    let mut args = vec!["--nix-closure", "--output", &sidecar_arg];
    args.extend_from_slice(extra);
    let scan = NixStub {
        project: project.path().to_path_buf(),
        closure,
        rows: Rows::Json("[]"),
        attr: DEFAULT_ATTR,
        declarations,
        ..Default::default()
    }
    .scan(&args);
    let openvex = std::fs::read(&sidecar).ok().map(|b| serde_json::from_slice(&b).unwrap());
    VexScan { scan, openvex, _project: project, _out: out }
}

fn scan() -> VexScan {
    scan_with(&closure(), None, &[])
}

fn statements(s: &VexScan) -> Vec<Value> {
    let doc = s.openvex.as_ref().unwrap_or_else(|| {
        panic!("no OpenVEX sidecar was written.\nstderr:\n{}", s.scan.stderr)
    });
    doc["statements"].as_array().unwrap().clone()
}

/// (product `@id`, CVE, status) for every statement.
fn claims(s: &VexScan) -> BTreeSet<(String, String, String)> {
    statements(s)
        .iter()
        .flat_map(|st| {
            st["products"].as_array().unwrap().iter().map(move |p| {
                (
                    p["@id"].as_str().unwrap().to_string(),
                    st["vulnerability"]["name"].as_str().unwrap().to_string(),
                    st["status"].as_str().unwrap().to_string(),
                )
            })
        })
        .collect()
}

fn fixed(purl: &str, cve: &str) -> (String, String, String) {
    (purl.to_string(), cve.to_string(), "fixed".to_string())
}

fn cdx_purls(doc: &Value) -> BTreeSet<String> {
    fn walk(v: &Value, out: &mut BTreeSet<String>) {
        for c in v.as_array().into_iter().flatten() {
            if let Some(p) = c["purl"].as_str() {
                out.insert(p.to_string());
            }
            walk(&c["components"], out);
        }
    }
    let mut out = BTreeSet::new();
    walk(&doc["components"], &mut out);
    out
}

fn doc_property(doc: &Value, name: &str) -> Option<String> {
    doc["metadata"]["properties"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"] == name)
        .and_then(|p| p["value"].as_str())
        .map(str::to_string)
}

/// A document-scope annotation's value in an SPDX document, by field name.
fn spdx_doc_annotation(doc: &Value, field: &str) -> Option<String> {
    let text = doc.to_string();
    let needle = format!(r#"\"field\":\"{field}\""#);
    if !text.contains(&needle) {
        return None;
    }
    // The envelope is a JSON string inside a comment; find it and decode.
    let comments: Vec<String> = match doc["@graph"].as_array() {
        Some(graph) => graph
            .iter()
            .filter(|e| e["type"] == "Annotation")
            .filter_map(|e| e["statement"].as_str().map(str::to_string))
            .collect(),
        None => doc["annotations"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a["comment"].as_str().map(str::to_string))
            .collect(),
    };
    comments.iter().find_map(|c| {
        let v: Value = serde_json::from_str(c).ok()?;
        (v["field"] == field).then(|| v["value"].as_str().map(str::to_string)).flatten()
    })
}

// ------------------------------------------------------------------------
// US1 — the OpenVEX claim
// ------------------------------------------------------------------------

#[test]
fn one_fixed_statement_per_resolved_pair() {
    let s = scan();
    let got = claims(&s);
    let want: BTreeSet<_> = [
        fixed(ZIP_Q, "CVE-2099-0001"),
        fixed(SSH_Q, "CVE-2099-0002"),
        fixed(SSH_Q, "CVE-2099-0003"),
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want, "{:#?}", statements(&s));
    // One statement per pair, each with one product and no subcomponent.
    let st = statements(&s);
    assert_eq!(st.len(), 3);
    for x in &st {
        assert_eq!(x["products"].as_array().map(Vec::len), Some(1), "{x:#}");
        assert!(x["products"][0]["subcomponents"].is_null(), "{x:#}");
        assert_eq!(x["products"][0]["identifiers"]["purl"], x["products"][0]["@id"], "{x:#}");
    }
}

#[test]
fn no_affected_statement_for_a_backport() {
    // FR-003. No declaration is served here, so every statement is a
    // backport's, and none may be `affected` (or anything but `fixed`).
    let s = scan();
    let st = statements(&s);
    assert!(!st.is_empty(), "control");
    assert!(st.iter().all(|x| x["status"] == "fixed"), "{st:#?}");
}

#[test]
fn a_cve_only_some_variants_resolve_gets_no_statement() {
    let s = scan();
    // CONTROL: the split component is in the document, qualified, and its
    // pedigree records the CVE -- so the absence below is the rule, not a
    // missing component.
    assert!(cdx_purls(&s.scan.cdx).contains(SPLIT_Q), "{:?}", cdx_purls(&s.scan.cdx));
    assert!(s.scan.cdx.to_string().contains("CVE-2099-0004"));
    assert!(
        !claims(&s).iter().any(|(p, c, _)| p == SPLIT_Q || c == "CVE-2099-0004"),
        "{:#?}",
        statements(&s)
    );
}

#[test]
fn statements_need_no_document_subject() {
    // The wrapper flake has no main module. #1436 part 1 was this scan
    // emitting no statement at all.
    let s = scan();
    assert!(
        !s.scan.cdx["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["properties"].to_string().contains("main-module")),
        "control: the fixture must have no main module"
    );
    assert!(!statements(&s).is_empty());
}

#[test]
fn the_grade_note_is_kept() {
    // FR-004.
    for st in statements(&scan()) {
        let note = st["impact_statement"].as_str().unwrap_or_default();
        assert!(note.contains("filename-derived"), "{st:#}");
    }
}

#[test]
fn a_declaration_withholds_the_fixed_claim() {
    // Research R10: nixpkgs declaring (ssh, CVE-2099-0002) outranks the
    // filename-derived claim for that pair, and only that pair.
    let decl = format!(
        r#"{{"waybill-fixture-ssh":[{{"setId":"top-level","out":"{SSH_OUT}",
            "kv":["CVE-2099-0002: synthetic declaration for the waybill fixture"]}}]}}"#
    );
    let s = scan_with(&closure(), Some(&decl), &[]);
    let got = claims(&s);
    // CONTROL: the declaration reached the document.
    assert!(
        got.iter().any(|(_, c, st)| c == "CVE-2099-0002" && st == "affected"),
        "the stub's declaration was not read: {got:#?}\nstderr:\n{}",
        s.scan.stderr
    );
    assert!(!got.contains(&fixed(SSH_Q, "CVE-2099-0002")), "{got:#?}");
    assert!(got.contains(&fixed(SSH_Q, "CVE-2099-0003")), "{got:#?}");
    assert!(got.contains(&fixed(ZIP_Q, "CVE-2099-0001")), "{got:#?}");
}

#[test]
fn the_unpatched_build_is_never_named() {
    // SC-002's leak case: another build of the name, unpatched, keeps an
    // unqualified PURL, and no statement can cover it.
    let s = scan();
    let purls = cdx_purls(&s.scan.cdx);
    assert!(purls.contains(ZIP_61), "control: {purls:?}");
    assert!(purls.contains(PLAIN), "a non-CVE patch leaves the PURL alone: {purls:?}");
    for (p, _, _) in claims(&s) {
        assert!(p.contains("?nix_drv="), "a claim on an unqualified PURL: {p}");
        assert_ne!(p, ZIP_61);
    }
}

#[test]
fn no_orphan_endpoint() {
    let s = scan();
    // CONTROL: the documents carry references to check.
    assert!(common::endpoints::reference_count(&s.scan.cdx, &s.scan.spdx23, &s.scan.spdx3) > 0);
    let dangling = common::endpoints::dangling(&s.scan.cdx, &s.scan.spdx23, &s.scan.spdx3);
    assert!(dangling.is_empty(), "{dangling:#?}");
    // The edges reach the qualified components: the app depends on zip.
    let edges = s.scan.cdx["dependencies"].to_string();
    assert!(edges.contains(ZIP_Q), "no edge names the qualified zip: {edges}");
}

#[test]
fn projected_documents_keep_statements_with_components() {
    // Under a root override the document's subject changes and the
    // components stay: every claim is still carried, and nothing is counted
    // as omitted.
    let s = scan_with(&closure(), None, &["--root-name", "waybill-fixture-override", "--root-version", "9.9"]);
    let carried: BTreeSet<(String, String)> = s.scan.cdx["vulnerabilities"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|v| {
            v["affects"].as_array().unwrap().iter().map(move |a| {
                (a["ref"].as_str().unwrap().to_string(), v["id"].as_str().unwrap().to_string())
            })
        })
        .collect();
    let sidecar: BTreeSet<(String, String)> =
        claims(&s).into_iter().map(|(p, c, _)| (p, c)).collect();
    assert_eq!(sidecar.len(), 3, "control: {sidecar:?}");
    assert_eq!(carried, sidecar);
    assert_eq!(doc_property(&s.scan.cdx, "waybill:vex-claims-omitted"), None);

    // #1360 — `--tier` now filters the closure tier too, so `source-only`
    // drops the build-tier members, and their backport claims go with them:
    // a claim is derived from its component's pedigree, so no claim can name
    // a component the document lacks, and none is left to count as omitted.
    // A `--split` document is asserted at unit level in
    // `generate/cyclonedx/vex.rs`
    // (`a_split_document_carries_the_backport_claims_of_what_it_holds`),
    // because the stub harness writes per-format `--output`s, which
    // `--split` refuses.
    let s = scan_with(&closure(), None, &["--tier", "source-only"]);
    let purls = cdx_purls(&s.scan.cdx);
    assert!(
        !purls.iter().any(|p| p.contains("?nix_drv=")),
        "the backported closure members are build-tier and must be dropped: {purls:?}"
    );
    let st = s
        .openvex
        .as_ref()
        .and_then(|d| d["statements"].as_array().cloned())
        .unwrap_or_default();
    assert!(st.is_empty(), "no claim may outlive its component: {st:#?}");
    assert_eq!(
        s.scan.cdx["vulnerabilities"].as_array().map_or(0, Vec::len),
        0,
        "CycloneDX carries no claim for a dropped component"
    );
    assert_eq!(doc_property(&s.scan.cdx, "waybill:vex-claims-omitted"), None);
}

#[test]
fn a_patched_member_merged_into_a_manifest_component_is_qualified() {
    // T007 (research R6), end to end. The cabal project's main module is
    // also the closure's evaluated package, so the closure member merges
    // into the manifest's component. Patching it means the merged component
    // must carry the identity, under its own PURL type, and every edge the
    // manifest built to or from it must follow.
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure_role_scope/project");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_closure_subject/closure-mainmodule.json");
    let mut c: Value = serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
    c["derivations"]["r00t-waybill-fixture-roleproj-0.1.0.drv"]["env"]["patches"] =
        Value::String("/nix/store/p1-CVE-2099-0005.patch".to_string());
    let closure = c.to_string();
    let out = tempfile::tempdir().unwrap();
    let sidecar = out.path().join("waybill.openvex.json");
    let sidecar_arg = format!("openvex={}", sidecar.display());
    let scan = NixStub {
        project,
        closure: &closure,
        rows: Rows::Json("[]"),
        attr: DEFAULT_ATTR,
        ..Default::default()
    }
    .scan(&["--nix-closure", "--output", &sidecar_arg]);
    let q = "pkg:hackage/waybill-fixture-roleproj@0.1.0?nix_drv=r00t-waybill-fixture-roleproj-0.1.0";

    // CONTROL: the member merged rather than being appended beside the
    // manifest's component, so the generic spelling is absent.
    let all = scan.cdx.to_string();
    assert!(!all.contains("pkg:generic/waybill-fixture-roleproj"), "appended, not merged");
    assert_eq!(scan.cdx["metadata"]["component"]["purl"], q, "the merged subject carries the identity");

    let ov: Value = serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
    let st = ov["statements"].as_array().unwrap();
    assert_eq!(st.len(), 1, "{st:#?}");
    assert_eq!(st[0]["products"][0]["@id"], q);
    assert_eq!(st[0]["status"], "fixed");

    let dangling = common::endpoints::dangling(&scan.cdx, &scan.spdx23, &scan.spdx3);
    assert!(dangling.is_empty(), "{dangling:#?}");
    assert!(
        !all.contains("\"pkg:hackage/waybill-fixture-roleproj@0.1.0\""),
        "something still names the unqualified PURL"
    );
}

// ------------------------------------------------------------------------
// US2 — CycloneDX `vulnerabilities[]`
// ------------------------------------------------------------------------

#[test]
fn cdx_has_one_resolved_entry_per_pair_on_the_component() {
    let s = scan();
    let v = s.scan.cdx["vulnerabilities"].as_array().unwrap();
    assert_eq!(v.len(), 3, "{v:#?}");
    for e in v {
        assert_eq!(e["analysis"]["state"], "resolved", "{e:#}");
        let refs: Vec<&str> = e["affects"].as_array().unwrap().iter().filter_map(|a| a["ref"].as_str()).collect();
        assert_eq!(refs.len(), 1, "{e:#}");
        assert!(refs[0] == ZIP_Q || refs[0] == SSH_Q, "{e:#}");
        assert!(
            !e["properties"].to_string().contains("waybill:vex-subcomponent"),
            "{e:#}"
        );
    }
}

#[test]
fn cdx_never_marks_a_patched_component_exploitable() {
    // SC-003 (#1421): no entry marks a component exploitable for a CVE its
    // own patches resolve.
    let s = scan();
    let v = s.scan.cdx["vulnerabilities"].as_array().unwrap();
    assert!(!v.is_empty(), "control");
    assert!(v.iter().all(|e| e["analysis"]["state"] != "exploitable"), "{v:#?}");
}

#[test]
fn sidecar_and_cdx_agree() {
    // FR-006: one source, two carriers.
    let s = scan();
    let cdx: BTreeSet<(String, String, String)> = s.scan.cdx["vulnerabilities"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|v| {
            v["affects"].as_array().unwrap().iter().map(move |a| {
                (
                    a["ref"].as_str().unwrap().to_string(),
                    v["id"].as_str().unwrap().to_string(),
                    v["analysis"]["state"].as_str().unwrap().to_string(),
                )
            })
        })
        .collect();
    let sidecar: BTreeSet<(String, String, String)> = claims(&s)
        .into_iter()
        .map(|(p, c, st)| (p, c, if st == "fixed" { "resolved".to_string() } else { st }))
        .collect();
    assert!(!sidecar.is_empty(), "control");
    assert_eq!(cdx, sidecar);
}

#[test]
fn spdx_formats_carry_the_same_qualified_purl() {
    let s = scan();
    let spdx23: BTreeSet<String> = s.scan.spdx23["packages"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["externalRefs"].as_array().cloned().unwrap_or_default())
        .filter(|r| r["referenceType"] == "purl")
        .filter_map(|r| r["referenceLocator"].as_str().map(str::to_string))
        .collect();
    let spdx3: BTreeSet<String> = s.scan.spdx3["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["software_packageUrl"].as_str().map(str::to_string))
        .collect();
    for q in [ZIP_Q, SSH_Q, SPLIT_Q] {
        assert!(cdx_purls(&s.scan.cdx).contains(q), "CycloneDX lacks {q}");
        assert!(spdx23.contains(q), "SPDX 2.3 lacks {q}: {spdx23:?}");
        assert!(spdx3.contains(q), "SPDX 3 lacks {q}: {spdx3:?}");
    }
}

// ------------------------------------------------------------------------
// US3 — C220
// ------------------------------------------------------------------------

#[test]
fn c220_counts_claims_not_every_variant_resolves() {
    let s = scan();
    assert_eq!(doc_property(&s.scan.cdx, "waybill:vex-claims-unscoped").as_deref(), Some("1"));
    assert_eq!(spdx_doc_annotation(&s.scan.spdx23, "waybill:vex-claims-unscoped").as_deref(), Some("1"));
    assert_eq!(spdx_doc_annotation(&s.scan.spdx3, "waybill:vex-claims-unscoped").as_deref(), Some("1"));
}

#[test]
fn no_c220_when_nothing_is_unscoped() {
    let s = scan_with(&closure_without_split(), None, &[]);
    // CONTROL: the scan still made claims.
    assert!(!statements(&s).is_empty());
    assert_eq!(doc_property(&s.scan.cdx, "waybill:vex-claims-unscoped"), None);
    for doc in [&s.scan.spdx23, &s.scan.spdx3] {
        assert!(!doc.to_string().contains("waybill:vex-claims-unscoped"));
    }
}
