//! Spec 1491 (#1413) — operator-supplied CPEs: `--root-cpe`, `--cpe-map`,
//! and `--supplement-cdx` winning over a synthesized CPE.
//!
//! Every assertion is on the emitted documents, in all three formats. The
//! closure cases reuse milestone 1377's stub-`nix` fixture, whose `zlib`
//! carries a nixpkgs-declared CPE and whose `openssl` a curated one
//! (`fixtures/nix_closure_declared_cpe/README.md`).

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::nix_stub::{NixStub, Rows, Scan, DEFAULT_ATTR};
use serde_json::Value;

const C212: &str = "waybill:cpe-provenance";
const VENDOR: &str = "waybill-fixture-vendor";
const ROOT_COMMENT: &str = "CPE supplied by the operator with --root-cpe";
const SUPPLEMENT_COMMENT: &str = "CPE supplied by the operator with --supplement-cdx";
const MAP_COMMENT: &str = "CPE supplied by the operator with --cpe-map";

fn op_cpe(product: &str, version: &str) -> String {
    format!("cpe:2.3:a:{VENDOR}:{product}:{version}:*:*:*:*:*:*:*")
}

// ---------------------------------------------------------------- harness

/// The three documents of one scan, or the failure.
struct Docs {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
    stderr: String,
}

impl From<Scan> for Docs {
    fn from(s: Scan) -> Self {
        Docs { cdx: s.cdx, spdx23: s.spdx23, spdx3: s.spdx3, stderr: s.stderr }
    }
}

fn command(root: &Path, out: &Path, extra: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_waybill"));
    cmd.args(["sbom", "scan", "--path"])
        .arg(root)
        .args(["--offline", "--no-deep-hash"])
        .args(["--format", "cyclonedx-json", "--format", "spdx-2.3-json", "--format", "spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out.join("o.cdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", out.join("o.spdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", out.join("o.spdx3.json").display()))
        .args(extra)
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .env("RUST_LOG", "info")
        .env_remove("WAYBILL_EXCLUDE_PATH");
    cmd
}

fn scan(root: &Path, extra: &[&str]) -> Docs {
    let out = tempfile::tempdir().unwrap();
    let o = command(root, out.path(), extra).output().unwrap();
    assert!(
        o.status.success(),
        "scan failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    let read = |n: &str| -> Value {
        serde_json::from_slice(&std::fs::read(out.path().join(n)).unwrap()).unwrap()
    };
    Docs {
        cdx: read("o.cdx.json"),
        spdx23: read("o.spdx.json"),
        spdx3: read("o.spdx3.json"),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

/// A scan that must fail before writing anything.
fn refused(root: &Path, extra: &[&str]) -> String {
    let out = tempfile::tempdir().unwrap();
    let o: Output = command(root, out.path(), extra).output().unwrap();
    assert!(!o.status.success(), "the scan was expected to fail, and succeeded");
    let written: Vec<_> = std::fs::read_dir(out.path()).unwrap().collect();
    assert!(written.is_empty(), "a refused scan wrote {written:?}");
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// A dpkg root with one package, and its os-release, so the PURL is stable.
fn dpkg_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("var/lib/dpkg")).unwrap();
    std::fs::create_dir_all(dir.path().join("etc")).unwrap();
    std::fs::write(dir.path().join("etc/os-release"), "ID=debian\nVERSION_ID=\"12\"\n").unwrap();
    std::fs::write(
        dir.path().join("var/lib/dpkg/status"),
        "Package: waybill-fixture-osdep\nStatus: install ok installed\nVersion: 1.0.0\n\
         Architecture: amd64\nMaintainer: Fixture\nDescription: fixture\n\n",
    )
    .unwrap();
    dir
}

const OSDEP_PURL: &str = "pkg:deb/debian/waybill-fixture-osdep@1.0.0?arch=amd64&distro=debian-12";
const OSDEP_GUESS: &str = "cpe:2.3:a:debian:waybill-fixture-osdep:1.0.0:*:*:*:*:*:*:*";

/// A Cargo project whose main module is the subject, with one dependency
/// locked at two versions.
fn cargo_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"waybill-fixture-app\"\nversion = \"2.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
    std::fs::write(
        dir.path().join("Cargo.lock"),
        "version = 3\n\n\
         [[package]]\nname = \"waybill-fixture-app\"\nversion = \"2.1.0\"\n\
         dependencies = [\n \"waybill-fixture-dep 1.0.0\",\n \"waybill-fixture-dep 2.0.0\",\n]\n\n\
         [[package]]\nname = \"waybill-fixture-dep\"\nversion = \"1.0.0\"\n\
         source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n\
         [[package]]\nname = \"waybill-fixture-dep\"\nversion = \"2.0.0\"\n\
         source = \"registry+https://github.com/rust-lang/crates.io-index\"\n",
    )
    .unwrap();
    dir
}

fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    p
}

/// A supplement declaring one component. Its name is the scanned
/// component's: the supplement wins on `name`, and the closure merge joins
/// by name and version.
fn supplement(dir: &Path, name: &str, purl: &str, cpe: &str) -> PathBuf {
    write(
        dir,
        "supplement.cdx.json",
        &serde_json::json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "version": 1,
            "components": [{ "type": "library", "name": name, "purl": purl, "cpe": cpe }],
        })
        .to_string(),
    )
}

fn closure_scan(extra: &[&str]) -> Docs {
    let fixture = |name: &str| {
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/nix_closure_declared_cpe")
                .join(name),
        )
        .unwrap()
    };
    let rows = fixture("rows.json");
    let mut args = vec!["--nix-closure"];
    args.extend_from_slice(extra);
    NixStub {
        project: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/closure_role_scope/project"),
        closure: &fixture("closure.json"),
        rows: Rows::Json(&rows),
        attr: DEFAULT_ATTR,
        ..Default::default()
    }
    .scan(&args)
    .into()
}

// ---------------------------------------------------------------- readers

/// The CycloneDX component with this name, the subject included.
fn cdx_component(d: &Docs, pred: impl Fn(&Value) -> bool) -> &Value {
    d.cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .chain(std::iter::once(&d.cdx["metadata"]["component"]))
        .find(|c| pred(c))
        .unwrap_or_else(|| panic!("no such CycloneDX component"))
}

fn by_name(name: &str) -> impl Fn(&Value) -> bool + '_ {
    move |c: &Value| c["name"].as_str() == Some(name)
}

fn by_purl(purl: &str) -> impl Fn(&Value) -> bool + '_ {
    move |c: &Value| c["purl"].as_str() == Some(purl)
}

fn cdx_cpe_method(c: &Value) -> Option<String> {
    c["evidence"]["identity"]
        .as_array()?
        .iter()
        .find(|i| i["field"] == "cpe")
        .and_then(|i| i["methods"][0]["value"].as_str().map(str::to_string))
}

/// `(cpe, comment)` for every CPE the SPDX 2.3 package named `name` carries.
fn spdx23_cpes(d: &Docs, name: &str) -> Vec<(String, Option<String>)> {
    d.spdx23["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"].as_str() == Some(name))
        .flat_map(|p| p["externalRefs"].as_array().cloned().unwrap_or_default())
        .filter(|r| r["referenceType"].as_str().is_some_and(|t| t.starts_with("cpe")))
        .map(|r| {
            (
                r["referenceLocator"].as_str().unwrap().to_string(),
                r["comment"].as_str().map(str::to_string),
            )
        })
        .collect()
}

/// The same for SPDX 3.
fn spdx3_cpes(d: &Docs, name: &str) -> Vec<(String, Option<String>)> {
    let graph = d.spdx3["@graph"].as_array().unwrap();
    graph
        .iter()
        .filter(|e| e["type"] == "software_Package" && e["name"].as_str() == Some(name))
        .flat_map(|e| e["externalIdentifier"].as_array().cloned().unwrap_or_default())
        .filter_map(|id| {
            let id = match id.as_str() {
                Some(iri) => graph.iter().find(|e| e["spdxId"].as_str() == Some(iri))?.clone(),
                None => id,
            };
            (id["externalIdentifierType"] == "cpe23").then(|| {
                (
                    id["identifier"].as_str().unwrap().to_string(),
                    id["comment"].as_str().map(str::to_string),
                )
            })
        })
        .collect()
}

fn envelope_value(comment: &str) -> Option<String> {
    let v: Value = serde_json::from_str(comment).ok()?;
    (v.get("field")?.as_str()? == C212).then(|| v["value"].as_str().unwrap().to_string())
}

/// Every C212 value in each format, the CycloneDX subject included, sorted
/// and deduplicated as the parity extractors compare them.
fn c212_values(d: &Docs) -> [Vec<String>; 3] {
    let mut cdx: Vec<String> = d.cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .chain(std::iter::once(&d.cdx["metadata"]["component"]))
        .flat_map(|c| c["properties"].as_array().cloned().unwrap_or_default())
        .filter(|p| p["name"] == C212)
        .filter_map(|p| p["value"].as_str().map(str::to_string))
        .collect();
    let mut spdx23: Vec<String> = d.spdx23["packages"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["annotations"].as_array().cloned().unwrap_or_default())
        .filter_map(|a| envelope_value(a["comment"].as_str()?))
        .collect();
    let mut spdx3: Vec<String> = d.spdx3["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "Annotation")
        .filter_map(|a| envelope_value(a["statement"].as_str()?))
        .collect();
    for v in [&mut cdx, &mut spdx23, &mut spdx3] {
        v.sort();
        v.dedup();
    }
    [cdx, spdx23, spdx3]
}

fn assert_c212_parity(d: &Docs) -> Vec<String> {
    let [cdx, spdx23, spdx3] = c212_values(d);
    assert_eq!(cdx, spdx23, "C212: CycloneDX and SPDX 2.3 disagree");
    assert_eq!(cdx, spdx3, "C212: CycloneDX and SPDX 3 disagree");
    cdx
}

fn c212_json(basis: &str, overrode: Option<&str>) -> String {
    let mut v = serde_json::json!({ "basis": basis });
    if let Some(o) = overrode {
        v["overrode"] = Value::String(o.to_string());
    }
    v.to_string()
}

/// A component's CPE, source and C212 in every format: the shape every
/// operator case asserts.
fn assert_operator_cpe(
    d: &Docs,
    cdx: &Value,
    spdx_name: &str,
    want: &str,
    comment: &str,
    method: &str,
    c212: Option<String>,
) {
    assert_eq!(cdx["cpe"].as_str(), Some(want), "CycloneDX cpe");
    assert_eq!(cdx_cpe_method(cdx).as_deref(), Some(method), "CycloneDX evidence.identity");
    let spdx23 = spdx23_cpes(d, spdx_name);
    assert_eq!(spdx23.first(), Some(&(want.to_string(), Some(comment.to_string()))), "SPDX 2.3");
    let spdx3 = spdx3_cpes(d, spdx_name);
    assert!(spdx3.contains(&(want.to_string(), Some(comment.to_string()))), "SPDX 3: {spdx3:?}");
    let props = cdx["properties"].as_array().cloned().unwrap_or_default();
    let got: Vec<String> = props
        .iter()
        .filter(|p| p["name"] == C212)
        .filter_map(|p| p["value"].as_str().map(str::to_string))
        .collect();
    assert_eq!(got, c212.into_iter().collect::<Vec<_>>(), "CycloneDX C212");
    assert_c212_parity(d);
}

// ------------------------------------------------- US2: supplement wins

/// SC-002 control: without a supplement the deb component carries the
/// heuristic guess, so "replaced" below is a change and not a coincidence.
#[test]
fn control_a_deb_component_carries_a_synthesized_guess() {
    let root = dpkg_root();
    let d = scan(root.path(), &[]);
    assert_eq!(cdx_component(&d, by_purl(OSDEP_PURL))["cpe"].as_str(), Some(OSDEP_GUESS));
}

/// US2 scenario 1, FR-003, SC-002 — the operator's CPE replaces the guess in
/// every format, its source is recorded natively, and C212 names the guess.
#[test]
fn s1491_us2_a_supplement_cpe_replaces_a_synthesized_guess() {
    let root = dpkg_root();
    let want = op_cpe("waybill_fixture_osdep", "1.0.0");
    let supp = supplement(root.path(), "waybill-fixture-osdep", OSDEP_PURL, &want);
    let d = scan(root.path(), &["--supplement-cdx", supp.to_str().unwrap()]);
    let c = cdx_component(&d, by_purl(OSDEP_PURL));
    assert_operator_cpe(
        &d,
        c,
        "waybill-fixture-osdep",
        &want,
        SUPPLEMENT_COMMENT,
        "operator --supplement-cdx",
        Some(c212_json("purl", Some(OSDEP_GUESS))),
    );
    // The guess is no longer emitted as a second identifier.
    assert_eq!(spdx23_cpes(&d, "waybill-fixture-osdep").len(), 1);
}

/// Spec edge case — a CPE whose version differs from the component's is
/// emitted as given, with a WARN naming the component.
#[test]
fn s1491_us2_a_version_mismatch_is_emitted_with_a_warning() {
    let root = dpkg_root();
    let other = op_cpe("waybill_fixture_osdep", "9.9");
    let supp = supplement(root.path(), "waybill-fixture-osdep", OSDEP_PURL, &other);
    let d = scan(root.path(), &["--supplement-cdx", supp.to_str().unwrap()]);
    assert_eq!(cdx_component(&d, by_purl(OSDEP_PURL))["cpe"].as_str(), Some(other.as_str()));
    assert!(d.stderr.contains("differs from the component's"), "{}", d.stderr);
}

/// US2 scenario 1 on a curated `pkg:generic` closure member: the operator
/// beats the curated table, and C212 names the curated CPE.
#[test]
fn s1491_us2_a_supplement_cpe_replaces_a_curated_one() {
    let plain = closure_scan(&[]);
    let openssl = cdx_component(&plain, by_name("openssl"));
    let purl = openssl["purl"].as_str().unwrap().to_string();
    let curated = openssl["cpe"].as_str().unwrap().to_string();
    // CONTROL: the curated table gave it the CPE.
    assert_eq!(curated, "cpe:2.3:a:openssl:openssl:9.9.9:*:*:*:*:*:*:*");

    let dir = tempfile::tempdir().unwrap();
    let want = op_cpe("openssl_fork", "9.9.9");
    let supp = supplement(dir.path(), "openssl", &purl, &want);
    let d = closure_scan(&["--supplement-cdx", supp.to_str().unwrap()]);
    let c = cdx_component(&d, by_name("openssl"));
    assert_operator_cpe(
        &d,
        c,
        "openssl",
        &want,
        SUPPLEMENT_COMMENT,
        "operator --supplement-cdx",
        Some(c212_json("purl", Some(&curated))),
    );
}

/// US2 scenario 2 (T023) — on a member with a nixpkgs-declared CPE, the
/// operator wins and C212 names the declared CPE.
#[test]
fn s1491_us2_a_supplement_cpe_replaces_a_nixpkgs_declared_one() {
    let plain = closure_scan(&[]);
    let zlib = cdx_component(&plain, by_name("zlib"));
    let purl = zlib["purl"].as_str().unwrap().to_string();
    let declared = zlib["cpe"].as_str().unwrap().to_string();
    // CONTROL: nixpkgs' CPE is the one emitted without the supplement.
    assert_eq!(declared, op_cpe("zlib", "9.9.9"));

    let dir = tempfile::tempdir().unwrap();
    let want = op_cpe("zlib_operator", "9.9.9");
    let supp = supplement(dir.path(), "zlib", &purl, &want);
    let d = closure_scan(&["--supplement-cdx", supp.to_str().unwrap()]);
    assert_operator_cpe(
        &d,
        cdx_component(&d, by_name("zlib")),
        "zlib",
        &want,
        SUPPLEMENT_COMMENT,
        "operator --supplement-cdx",
        Some(c212_json("purl", Some(&declared))),
    );
}

// ------------------------------------------------------ US1: --root-cpe

/// US1 scenarios 1 and 4 — a subject named by `--root-name` gets the
/// operator's CPEs: the first is primary, the second secondary, both with
/// the operator source. A synthetic subject has no component scope and
/// carries no C212 (FR-009).
#[test]
fn s1491_us1_root_cpe_names_a_synthetic_subject() {
    let root = dpkg_root();
    let primary = op_cpe("waybill_fixture_app", "2.1.0");
    let secondary = op_cpe("waybill_fixture_app_alt", "2.1.0");
    let d = scan(
        root.path(),
        &[
            "--root-name",
            "waybill-fixture-app",
            "--root-version",
            "2.1.0",
            "--root-cpe",
            &primary,
            "--root-cpe",
            &secondary,
        ],
    );
    let subject = &d.cdx["metadata"]["component"];
    assert_eq!(subject["cpe"].as_str(), Some(primary.as_str()));
    assert_eq!(cdx_cpe_method(subject).as_deref(), Some("operator --root-cpe"));
    let candidates = subject["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "waybill:cpe-candidates")
        .and_then(|p| p["value"].as_str())
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<String>>(candidates).unwrap(),
        vec![primary.clone(), secondary.clone()]
    );
    let both = vec![
        (primary.clone(), Some(ROOT_COMMENT.to_string())),
        (secondary.clone(), Some(ROOT_COMMENT.to_string())),
    ];
    assert_eq!(spdx23_cpes(&d, "waybill-fixture-app"), both);
    assert_eq!(spdx3_cpes(&d, "waybill-fixture-app"), both);
    assert!(
        !c212_values(&d)[0].iter().any(|v| v.contains("root-cpe")),
        "a synthetic subject carries no C212"
    );
}

/// US1 scenario 2 — a scanned main-module subject: the operator's CPE
/// replaces its synthesized one, C212 names the displaced CPE, and the three
/// formats agree, CycloneDX's promoted `metadata.component` included
/// (research R7).
#[test]
fn s1491_us1_root_cpe_replaces_a_main_modules_cpe() {
    let root = cargo_root();
    let control = scan(root.path(), &[]);
    let before = control.cdx["metadata"]["component"]["cpe"].as_str().unwrap().to_string();

    let want = op_cpe("waybill_fixture_app", "2.1.0");
    let d = scan(root.path(), &["--root-cpe", &want]);
    let subject = &d.cdx["metadata"]["component"];
    assert_eq!(subject["purl"].as_str(), Some("pkg:cargo/waybill-fixture-app@2.1.0"));
    assert_operator_cpe(
        &d,
        subject,
        "waybill-fixture-app",
        &want,
        ROOT_COMMENT,
        "operator --root-cpe",
        Some(c212_json("root-cpe", Some(&before))),
    );
}

/// US1 scenario 3, FR-006 — a malformed or wildcard `--root-cpe` stops the
/// scan before it starts.
#[test]
fn s1491_us1_a_malformed_root_cpe_writes_nothing() {
    let root = dpkg_root();
    for bad in ["cpe:2.3:a:*:waybill_fixture_app:2.1.0:*:*:*:*:*:*:*", "cpe:/a:v:p:1", "not-a-cpe"] {
        let stderr = refused(root.path(), &["--root-name", "x", "--root-cpe", bad]);
        assert!(stderr.contains("--root-cpe"), "{bad}: {stderr}");
    }
}

/// Research R8 and the placeholder rule — `--root-cpe` needs a subject it
/// can name: not with `--split`, and not on a placeholder named after the
/// scan directory.
#[test]
fn s1491_us1_root_cpe_needs_a_named_subject() {
    let root = dpkg_root();
    let cpe = op_cpe("waybill_fixture_app", "2.1.0");
    let stderr = refused(root.path(), &["--root-cpe", &cpe]);
    assert!(stderr.contains("no named subject"), "{stderr}");
    // CONTROL: the same scan with a named subject succeeds.
    scan(root.path(), &["--root-name", "waybill-fixture-app", "--root-cpe", &cpe]);

    let o = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args(["sbom", "scan", "--path"])
        .arg(root.path())
        .args(["--split", "--root-cpe", &cpe])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("cannot be used with"));
}

// ------------------------------------------------------- US3: --cpe-map

fn map(dir: &Path, rows: &[(&str, &str, &str)]) -> PathBuf {
    let body: String = rows
        .iter()
        .map(|(key, vendor, product)| {
            format!(
                "[[row]]\nkey = \"{key}\"\npair = [ {{ vendor = \"{vendor}\", product = \"{product}\" }} ]\n\n"
            )
        })
        .collect();
    write(dir, "map.toml", &body)
}

/// US3 scenario 1, SC-003 — one row, every version: each component gets
/// the mapped vendor and product at its own version.
#[test]
fn s1491_us3_a_map_row_applies_at_every_version() {
    let root = cargo_root();
    let m = map(root.path(), &[("pkg:cargo/waybill-fixture-dep", VENDOR, "dep")]);
    let d = scan(root.path(), &["--cpe-map", m.to_str().unwrap()]);
    for v in ["1.0.0", "2.0.0"] {
        let purl = format!("pkg:cargo/waybill-fixture-dep@{v}");
        let c = cdx_component(&d, by_purl(&purl));
        assert_eq!(c["cpe"].as_str(), Some(op_cpe("dep", v).as_str()), "{purl}");
        assert_eq!(cdx_cpe_method(c).as_deref(), Some("operator --cpe-map"), "{purl}");
    }
    let spdx = spdx23_cpes(&d, "waybill-fixture-dep");
    assert_eq!(spdx.len(), 2, "{spdx:?}");
    assert!(spdx.iter().all(|(_, c)| c.as_deref() == Some(MAP_COMMENT)));
    assert_c212_parity(&d);
}

/// US3 scenarios 2 and 3, FR-005 — on the closure: a row beats the curated
/// table (openssl) and a nixpkgs-declared CPE (zlib), and applies to a name
/// the built-in table has no row for; a supplement still beats a row.
#[test]
fn s1491_us3_map_precedence_on_a_closure() {
    let plain = closure_scan(&[]);
    let curated = cdx_component(&plain, by_name("openssl"))["cpe"].as_str().unwrap().to_string();
    let declared = cdx_component(&plain, by_name("zlib"))["cpe"].as_str().unwrap().to_string();
    let plain_purl = cdx_component(&plain, by_name("waybill-fixture-plain"))["purl"]
        .as_str()
        .unwrap()
        .to_string();
    // CONTROL: this member has no CPE without the map.
    assert!(cdx_component(&plain, by_name("waybill-fixture-plain"))["cpe"].is_null());

    let dir = tempfile::tempdir().unwrap();
    let m = map(
        dir.path(),
        &[
            ("pkg:generic/openssl", VENDOR, "openssl_mapped"),
            ("pkg:generic/zlib", VENDOR, "zlib_mapped"),
            ("pkg:generic/waybill-fixture-plain", VENDOR, "plain_mapped"),
        ],
    );
    let d = closure_scan(&["--cpe-map", m.to_str().unwrap()]);
    for (name, product, displaced) in [
        ("openssl", "openssl_mapped", Some(curated.as_str())),
        ("zlib", "zlib_mapped", Some(declared.as_str())),
        ("waybill-fixture-plain", "plain_mapped", None),
    ] {
        let c = cdx_component(&d, by_name(name));
        let version = c["version"].as_str().unwrap();
        assert_operator_cpe(
            &d,
            c,
            name,
            &op_cpe(product, version),
            MAP_COMMENT,
            "operator --cpe-map",
            // FR-009: nothing displaced on the plain member, so no C212.
            displaced.map(|o| c212_json("cpe-map", Some(o))),
        );
    }

    // A supplement for zlib's exact PURL beats the zlib row.
    let zlib_purl = cdx_component(&plain, by_name("zlib"))["purl"].as_str().unwrap().to_string();
    let want = op_cpe("zlib_supplied", "9.9.9");
    let supp = supplement(dir.path(), "zlib", &zlib_purl, &want);
    let both = closure_scan(&["--cpe-map", m.to_str().unwrap(), "--supplement-cdx", supp.to_str().unwrap()]);
    let zlib = cdx_component(&both, by_name("zlib"));
    assert_eq!(zlib["cpe"].as_str(), Some(want.as_str()));
    assert_eq!(cdx_cpe_method(zlib).as_deref(), Some("operator --supplement-cdx"));
    let _ = plain_purl;
}

/// US3 scenario 5 — a row that matches nothing changes nothing, and is
/// reported.
#[test]
fn s1491_us3_an_unused_row_changes_nothing() {
    let root = dpkg_root();
    let control = scan(root.path(), &[]);
    let m = map(root.path(), &[("pkg:npm/waybill-fixture-unused", VENDOR, "unused")]);
    let d = scan(root.path(), &["--cpe-map", m.to_str().unwrap()]);
    assert_eq!(
        cdx_component(&d, by_purl(OSDEP_PURL))["cpe"],
        cdx_component(&control, by_purl(OSDEP_PURL))["cpe"]
    );
    assert!(d.stderr.contains("rows that matched nothing: pkg:npm/waybill-fixture-unused"), "{}", d.stderr);
}

/// US3 scenario 4, FR-006 — every malformed map stops the scan before it
/// starts, naming the file and the row.
#[test]
fn s1491_us3_a_malformed_map_writes_nothing() {
    let root = dpkg_root();
    let cases = [
        ("not toml [", "not a valid CPE table"),
        ("[[row]]\nkey = \"pkg:deb/debian/x@1.0\"\npair = [ { vendor = \"v\", product = \"p\" } ]\n", "versionless"),
        ("[[row]]\nkey = \"pkg:deb/debian/x\"\nextra = 1\npair = [ { vendor = \"v\", product = \"p\" } ]\n", "not a valid CPE table"),
        (
            "[[row]]\nkey = \"pkg:deb/debian/x\"\npair = [ { vendor = \"v\", product = \"p\" } ]\n\
             [[row]]\nkey = \"pkg:deb/debian/x\"\npair = [ { vendor = \"v\", product = \"p\" } ]\n",
            "more than once",
        ),
        ("[[row]]\nkey = \"pkg:deb/debian/x\"\npair = [ { vendor = \"*\", product = \"p\" } ]\n", "wildcard"),
    ];
    for (body, why) in cases {
        let m = write(root.path(), "bad.toml", body);
        let stderr = refused(root.path(), &["--cpe-map", m.to_str().unwrap()]);
        assert!(stderr.contains("--cpe-map") && stderr.contains(why), "{why}: {stderr}");
    }
    let stderr = refused(root.path(), &["--cpe-map", "/nonexistent/waybill-fixture-map.toml"]);
    assert!(stderr.contains("--cpe-map"), "{stderr}");
}
