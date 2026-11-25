//! Spec 1651 — `waybill sbom edit --retier FROM=TO`.
//!
//! Contract `specs/1651-edit-retier/contracts/cli.md`. Every case scans the
//! synthetic tree `fixtures/sbom_edit_retier/project` at run time, so the scan
//! carries both `source` and `design` (one of them `scope: excluded`):
//! `measurements/fixture.md`. Edits are applied to each of the scan's three
//! documents, and `waybill sbom parity-check` runs over the edited set. Rows
//! C5 (`waybill:sbom-tier`) and C243 (`waybill:retiered-from`) are
//! `SymmetricEqual`, so a tier or marker that differs between formats is a
//! parity gap.

mod common;
mod sbom_edit_support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};
use sigstore::crypto::signing_key::SigStoreKeyPair;
use sigstore::crypto::SigningScheme;

use sbom_edit_support::{bin, edit_to, read_json};

/// The three documents of one scan, named as `parity-check --scan-dir` reads them.
const NAMES: [(&str, &str); 3] =
    [("cyclonedx-json", "waybill.cdx.json"), ("spdx-2.3-json", "waybill.spdx.json"), ("spdx-3-json", "waybill.spdx3.json")];

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sbom_edit_retier/project")
}

/// Scan the fixture into `dir` in all three formats.
fn scan(dir: &Path, extra: &[&str]) -> [PathBuf; 3] {
    let home = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(bin());
    cmd.env("HOME", home.path()).env("RUST_LOG", "warn");
    cmd.args(["--offline", "sbom", "scan", "--path"]).arg(project()).args(["--no-deep-hash", "--format"]);
    cmd.arg(NAMES.map(|(f, _)| f).join(","));
    for (f, n) in NAMES {
        cmd.arg("--output").arg(format!("{f}={}", dir.join(n).display()));
    }
    cmd.args(extra);
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "scan failed: {}", String::from_utf8_lossy(&out.stderr));
    NAMES.map(|(_, n)| dir.join(n))
}

/// Edit each document into `out`, returning the three outputs and stderr.
fn edit3(inputs: &[PathBuf; 3], out: &Path, args: &[&str]) -> ([PathBuf; 3], Vec<String>) {
    std::fs::create_dir_all(out).unwrap();
    let mut errs = Vec::new();
    let outputs = NAMES.map(|(_, n)| out.join(n));
    for (i, o) in inputs.iter().zip(&outputs) {
        let e = edit_to(i, o, args, tempfile::tempdir().unwrap());
        e.assert_ok();
        errs.push(e.stderr);
    }
    common::parity::assert_parity(out);
    (outputs, errs)
}

fn prop(c: &Value, name: &str) -> Option<String> {
    c["properties"].as_array()?.iter().find(|p| p["name"] == name)?["value"].as_str().map(str::to_string)
}

/// Per component name: (tier, C243).
fn tiers(cdx: &Value) -> BTreeMap<String, (Option<String>, Option<String>)> {
    cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (c["name"].as_str().unwrap().to_string(), (prop(c, "waybill:sbom-tier"), prop(c, "waybill:retiered-from")))
        })
        .collect()
}

fn t(tier: &str, marker: Option<&str>) -> (Option<String>, Option<String>) {
    (Some(tier.to_string()), marker.map(str::to_string))
}

/// The derivation record's operations (C194).
fn ops(cdx: &Value) -> Value {
    let raw = prop(&cdx["metadata"], "waybill:derivation").unwrap();
    serde_json::from_str::<Value>(&raw).unwrap()["operations"].clone()
}

fn phases(cdx: &Value) -> Vec<String> {
    cdx["metadata"]["lifecycles"].as_array().into_iter().flatten().map(|l| l["phase"].as_str().unwrap().to_string()).collect()
}

/// The components with the relabel's own fields removed, and the edges.
fn without_relabel(cdx: &Value) -> (Value, Value) {
    let mut comps = cdx["components"].clone();
    for c in comps.as_array_mut().unwrap() {
        if let Some(ps) = c["properties"].as_array_mut() {
            ps.retain(|p| p["name"] != "waybill:sbom-tier" && p["name"] != "waybill:retiered-from");
        }
    }
    (comps, cdx["dependencies"].clone())
}

const DECLARED: &str = "waybill-fixture-declared-dep";
const DEV: &str = "waybill-fixture-dev-dep";
const LOCKED: &str = "waybill-fixture-locked";
const LOCKED_DEP: &str = "waybill-fixture-locked-dep";
const DECLARED_PKG: &str = "waybill-fixture-declared";

// ---------------------------------------------------------------------
// User Story 1 — the same graph under another tier
// ---------------------------------------------------------------------

/// R1: every `design` component becomes `source` with C243 `design`; nothing
/// else about any component changes.
#[test]
fn r1_design_becomes_source_and_nothing_else_changes() {
    let d = tempfile::tempdir().unwrap();
    let orig = scan(&d.path().join("orig"), &[]);
    let (out, _) = edit3(&orig, &d.path().join("r1"), &["--retier", "design=source"]);
    let (a, b) = (read_json(&orig[0]), read_json(&out[0]));
    let got = tiers(&b);
    assert_eq!(got[DECLARED], t("source", Some("design")));
    assert_eq!(got[DEV], t("source", Some("design")));
    for n in [LOCKED, LOCKED_DEP, DECLARED_PKG] {
        assert_eq!(got[n], t("source", None), "{n}");
    }
    assert_eq!(without_relabel(&a), without_relabel(&b), "components or edges changed");
    assert_eq!(
        ops(&b),
        json!([{"category": "retier", "matched": 2, "changed": 2, "from": "design", "to": "source"}])
    );
}

/// R2: mappings apply to the input's tiers, each component at most once.
#[test]
fn r2_each_component_is_relabelled_at_most_once() {
    let d = tempfile::tempdir().unwrap();
    let orig = scan(&d.path().join("orig"), &[]);
    let (out, _) = edit3(&orig, &d.path().join("r2"), &["--retier", "design=source", "--retier", "source=build"]);
    let b = read_json(&out[0]);
    let got = tiers(&b);
    assert_eq!(got[DECLARED], t("source", Some("design")), "an original design moved twice");
    for n in [LOCKED, LOCKED_DEP, DECLARED_PKG] {
        assert_eq!(got[n], t("build", Some("source")), "{n}");
    }
    assert_eq!(
        ops(&b),
        json!([
            {"category": "retier", "matched": 2, "changed": 2, "from": "design", "to": "source"},
            {"category": "retier", "matched": 5, "changed": 3, "from": "source", "to": "build"}
        ])
    );
}

/// R3: a mapping that matches nothing, and the same mapping twice.
#[test]
fn r3_no_match_and_a_repeated_mapping() {
    let d = tempfile::tempdir().unwrap();
    let orig = scan(&d.path().join("orig"), &[]);
    let (out, _) = edit3(&orig, &d.path().join("none"), &["--retier", "analyzed=build"]);
    let (a, b) = (read_json(&orig[0]), read_json(&out[0]));
    assert_eq!(tiers(&a), tiers(&b));
    assert_eq!(phases(&a), phases(&b), "nothing relabelled, so the type is untouched");
    assert_eq!(ops(&b), json!([{"category": "retier", "matched": 0, "changed": 0, "from": "analyzed", "to": "build"}]));

    let (out, _) = edit3(&orig, &d.path().join("twice"), &["--retier", "design=source", "--retier", "design=source"]);
    let ops = ops(&read_json(&out[0]));
    assert_eq!(ops[1], json!({"category": "retier", "matched": 0, "changed": 0, "from": "design", "to": "source"}));
}

/// R7: a non-CISA `FROM` (`image`, as the NuGet readers emit) is relabelled.
#[test]
fn r7_non_cisa_from_is_accepted() {
    let d = tempfile::tempdir().unwrap();
    let orig = scan(&d.path().join("orig"), &[]);
    let mut doc = read_json(&orig[0]);
    for c in doc["components"].as_array_mut().unwrap() {
        if c["name"] == LOCKED_DEP {
            for p in c["properties"].as_array_mut().unwrap() {
                if p["name"] == "waybill:sbom-tier" {
                    p["value"] = json!("image");
                }
            }
        }
    }
    let input = d.path().join("image.cdx.json");
    std::fs::write(&input, serde_json::to_vec(&doc).unwrap()).unwrap();
    let e = edit_to(&input, &d.path().join("out.cdx.json"), &["--retier", "image=build"], tempfile::tempdir().unwrap());
    e.assert_ok();
    assert_eq!(tiers(&e.json())[LOCKED_DEP], t("build", Some("image")));
}

// ---------------------------------------------------------------------
// User Story 2 — the relabel cannot be mistaken for a scan
// ---------------------------------------------------------------------

/// R4: a second relabel keeps C243's first value, and the chain records both.
#[test]
fn r4_second_edit_keeps_the_original_tier() {
    let d = tempfile::tempdir().unwrap();
    let orig = scan(&d.path().join("orig"), &[]);
    let (first, _) = edit3(&orig, &d.path().join("first"), &["--retier", "design=source"]);
    let (second, _) = edit3(&first, &d.path().join("second"), &["--retier", "source=build"]);
    let b = read_json(&second[0]);
    assert_eq!(tiers(&b)[DECLARED], t("build", Some("design")), "C243 must keep the scan's tier");
    let raw = prop(&b["metadata"], "waybill:derivation").unwrap();
    let record: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(record["ancestors"][0]["operations"][0]["from"], "design");
    let (ok, report) = verify(&second[0], &[&first[0], &orig[0]], &[]);
    assert!(ok, "{report:#}");
}

/// R5: with the tiers dropped first, there is nothing to relabel.
#[test]
fn r5_dropped_tiers_leave_nothing_to_relabel() {
    let d = tempfile::tempdir().unwrap();
    let orig = scan(&d.path().join("orig"), &[]);
    let e = edit_to(
        &orig[0],
        &d.path().join("out.cdx.json"),
        &["--drop-annotations", "waybill:sbom-tier", "--retier", "design=source"],
        tempfile::tempdir().unwrap(),
    );
    e.assert_ok();
    let b = e.json();
    assert_eq!(ops(&b)[1]["matched"], 0);
    assert!(tiers(&b).values().all(|(_, m)| m.is_none()), "C243 written with no tier to relabel");
}

/// R6: C243 is protected from `--drop-annotations`.
#[test]
fn r6_the_marker_is_protected() {
    let d = tempfile::tempdir().unwrap();
    let orig = scan(&d.path().join("orig"), &[]);
    let (first, _) = edit3(&orig, &d.path().join("first"), &["--retier", "design=source"]);
    let (second, _) = edit3(&first, &d.path().join("second"), &["--drop-annotations", "waybill:"]);
    let b = read_json(&second[0]);
    assert_eq!(tiers(&b)[DECLARED].1.as_deref(), Some("design"));
}

fn keypair(dir: &Path) -> (PathBuf, PathBuf) {
    let signer = SigningScheme::ECDSA_P256_SHA256_ASN1.create_signer().unwrap();
    let pair: SigStoreKeyPair = signer.to_sigstore_keypair().unwrap();
    let (private, public) = (dir.join("k.pem"), dir.join("k.pub.pem"));
    std::fs::write(&private, pair.private_key_to_pem().unwrap()).unwrap();
    std::fs::write(&public, pair.public_key_to_pem().unwrap()).unwrap();
    (private, public)
}

fn verify(derived: &Path, originals: &[&Path], keys: &[&Path]) -> (bool, Value) {
    let mut cmd = Command::new(bin());
    cmd.args(["sbom", "verify-chain"]).arg(derived).arg("--json").env("RUST_LOG", "warn");
    for o in originals {
        cmd.arg("--original").arg(o);
    }
    for k in keys {
        cmd.arg("--key").arg(k);
    }
    let out = cmd.output().unwrap();
    let report: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("verify-chain output is not JSON ({e}): {}", String::from_utf8_lossy(&out.stderr)));
    (out.status.success(), report)
}

/// V1: `verify-chain` validates a relabelled document, unsigned and signed.
#[test]
fn v1_verify_chain_passes_unsigned_and_signed() {
    let d = tempfile::tempdir().unwrap();
    let orig = scan(&d.path().join("orig"), &[]);
    let (out, _) = edit3(&orig, &d.path().join("r1"), &["--retier", "design=source"]);
    for (o, i) in out.iter().zip(&orig) {
        let (ok, report) = verify(o, &[i], &[]);
        assert!(ok, "unsigned {}: {report:#}", o.display());
    }
    let (private, public) = keypair(d.path());
    let signed = d.path().join("signed.cdx.json");
    let e = edit_to(&orig[0], &signed, &["--retier", "design=source", "--sign-key", private.to_str().unwrap()], tempfile::tempdir().unwrap());
    e.assert_ok();
    let (ok, report) = verify(&signed, &[&orig[0]], &[&public]);
    assert!(ok, "signed: {report:#}");
}

// ---------------------------------------------------------------------
// User Story 3 — the document stays self-consistent
// ---------------------------------------------------------------------

/// T1: a derived type is recomputed in all three formats.
#[test]
fn t1_derived_type_is_recomputed_everywhere() {
    let d = tempfile::tempdir().unwrap();
    let orig = scan(&d.path().join("orig"), &[]);
    assert_eq!(phases(&read_json(&orig[0])), ["design", "pre-build"]);
    let (out, _) = edit3(&orig, &d.path().join("r1"), &["--retier", "design=source"]);
    assert_eq!(phases(&read_json(&out[0])), ["pre-build"]);
    let comment = read_json(&out[1])["creationInfo"]["comment"].as_str().unwrap().to_string();
    assert!(comment.contains("Observed lifecycle phases: pre-build."), "{comment}");
    let spdx3 = read_json(&out[2]);
    let graph = spdx3["@graph"].as_array().unwrap();
    let sbom = graph.iter().find(|e| e["type"] == "software_Sbom").unwrap();
    assert_eq!(sbom["software_sbomType"], json!(["source"]));
    let doc = graph.iter().find(|e| e["type"] == "SpdxDocument").unwrap();
    assert!(doc["comment"].as_str().unwrap().contains("Observed lifecycle phases: pre-build."));
}

/// T2: a type asserted with `--sbom-type` is kept, and the edit says so.
#[test]
fn t2_asserted_type_is_kept() {
    let d = tempfile::tempdir().unwrap();
    let orig = scan(&d.path().join("orig"), &["--sbom-type", "build"]);
    let (out, errs) = edit3(&orig, &d.path().join("r1"), &["--retier", "design=source"]);
    assert_eq!(phases(&read_json(&out[0])), ["build"]);
    assert!(errs[0].contains("document type kept"), "{}", errs[0]);
    assert_eq!(tiers(&read_json(&out[0]))[DECLARED], t("source", Some("design")), "the relabel itself still applies");
}

/// C1: completeness and its reason are untouched; a relabel resolves nothing.
#[test]
fn c1_completeness_is_untouched() {
    let d = tempfile::tempdir().unwrap();
    let orig = scan(&d.path().join("orig"), &[]);
    let (out, _) = edit3(&orig, &d.path().join("r1"), &["--retier", "design=source"]);
    let (a, b) = (read_json(&orig[0]), read_json(&out[0]));
    for f in ["waybill:graph-completeness", "waybill:graph-completeness-reason"] {
        assert_eq!(prop(&a["metadata"], f), prop(&b["metadata"], f), "{f}");
    }
    assert_eq!(prop(&b["metadata"], "waybill:graph-completeness-reason").as_deref(), Some("transitive-edges-unresolvable: npm"));
}

/// D1: with `--drop`, the type is recomputed from the output's tiers; `--drop`
/// alone keeps today's behaviour.
#[test]
fn d1_drop_with_retier_and_drop_alone() {
    let d = tempfile::tempdir().unwrap();
    let orig = scan(&d.path().join("orig"), &[]);
    let (both, _) = edit3(&orig, &d.path().join("both"), &["--drop", "tier=design", "--retier", "source=build"]);
    assert_eq!(phases(&read_json(&both[0])), ["build"]);
    let (alone, _) = edit3(&orig, &d.path().join("alone"), &["--drop", "tier=design"]);
    assert_eq!(phases(&read_json(&alone[0])), ["design", "pre-build"], "--drop alone must not change the type");
}

// ---------------------------------------------------------------------
// User Story 4 — misuse is refused clearly
// ---------------------------------------------------------------------

#[test]
fn refusals_name_the_problem_and_write_nothing() {
    let d = tempfile::tempdir().unwrap();
    let orig = scan(&d.path().join("orig"), &[]);
    let cases: &[(&[&str], &str)] = &[
        (&["--retier", "design"], "expected FROM=TO"),
        (&["--retier", "=source"], "both sides"),
        (&["--retier", "design="], "both sides"),
        (&["--retier", "design=sources"], "design, source, build, analyzed, deployed, runtime"),
        (&["--retier", "design=design"], "the same tier"),
        (&["--retier", "design=source", "--retier", "design=build"], "maps `design` to both"),
    ];
    for (i, (args, needle)) in cases.iter().enumerate() {
        let out = d.path().join(format!("refused-{i}.cdx.json"));
        let e = edit_to(&orig[0], &out, args, tempfile::tempdir().unwrap());
        assert!(!e.status.success(), "{args:?} was accepted");
        assert!(e.stderr.contains(needle), "{args:?}: {}", e.stderr);
        assert!(!out.exists(), "{args:?} wrote output");
    }
}
