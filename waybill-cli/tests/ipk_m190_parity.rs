//! Milestone 190 (ipk emission parity with rpm reader) — integration
//! tests for #550 (CDX license operator normalization), #551 (SPDX 3
//! license emission), and #552 (ipk PURL epoch qualifier).
//!
//! Fixtures are synthesized at test time via a small ar-format writer.
//! The `.ipk` file format is a BSD ar archive containing three members:
//! `debian-binary` (`2.0\n`), `control.tar.gz` (holds `./control`), and
//! `data.tar.gz` (holds package payload; empty here).
//!
//! Every test scans a directory containing one synthetic .ipk with a
//! specific control-file shape and asserts the emitted CDX/SPDX 2.3/
//! SPDX 3 output matches the m190 emission contracts (see
//! `specs/190-ipk-emission-parity/contracts/emission-shape.md`).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::Value;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;
use common::workspace_root;

// ---------------------------------------------------------------------
// Fixture builder — construct a synthetic .ipk from a control-file body.
// ---------------------------------------------------------------------

/// Build a `.ipk` file at `dest_dir/<filename>` with the given control
/// body. The control text is wrapped in a `./control` tar entry,
/// gzipped, and packaged into a BSD ar archive alongside a `2.0\n`
/// debian-binary member and an empty `data.tar.gz`.
fn build_synthetic_ipk(dest_dir: &Path, filename: &str, control_body: &str) -> PathBuf {
    // Inner tar containing `./control` file.
    let control_tar_bytes = build_control_tar(control_body);
    let control_gz = gzip(&control_tar_bytes);

    // Empty data.tar.gz — a valid empty tar (2 * 512 zero bytes).
    let empty_tar = vec![0u8; 1024];
    let data_gz = gzip(&empty_tar);

    let ar_bytes = build_ar(&[
        ("debian-binary", b"2.0\n"),
        ("control.tar.gz", &control_gz),
        ("data.tar.gz", &data_gz),
    ]);
    let path = dest_dir.join(filename);
    std::fs::write(&path, ar_bytes).expect("write ipk fixture");
    path
}

fn build_control_tar(body: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut tar = tar::Builder::new(&mut buf);
        let mut header = tar::Header::new_gnu();
        header.set_path("./control").expect("set control path");
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append(&header, body.as_bytes())
            .expect("append control");
        tar.finish().expect("finish control tar");
    }
    buf
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).expect("gzip write");
    encoder.finish().expect("gzip finish")
}

fn build_ar(members: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = b"!<arch>\n".to_vec();
    for (name, data) in members {
        out.extend_from_slice(&ar_header(name, data.len() as u64));
        out.extend_from_slice(data);
        if !data.len().is_multiple_of(2) {
            out.push(b'\n');
        }
    }
    out
}

fn ar_header(name: &str, size: u64) -> [u8; 60] {
    let mut h = [b' '; 60];
    // Name field: 16 bytes, space-padded.
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len().min(16);
    h[..name_len].copy_from_slice(&name_bytes[..name_len]);
    // mtime: 12 bytes; zero.
    let mtime = "0           ".as_bytes();
    h[16..28].copy_from_slice(mtime);
    let uid_gid_mode = "0     0     0       ";
    h[28..48].copy_from_slice(uid_gid_mode.as_bytes());
    let size_str = format!("{size:<10}");
    h[48..58].copy_from_slice(size_str.as_bytes());
    h[58..60].copy_from_slice(b"`\n");
    h
}

// ---------------------------------------------------------------------
// Scan helper — invoke waybill against a fixture directory + return
// parsed JSON for the requested format.
// ---------------------------------------------------------------------

fn scan(dir: &Path, format: &str) -> Value {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_ext = match format {
        "cyclonedx-json" => "cdx.json",
        "spdx-2.3-json" => "spdx.json",
        "spdx-3-json" => "spdx3.json",
        _ => panic!("unknown format {format}"),
    };
    let out_path = workdir.path().join(format!("out.{out_ext}"));

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        dir.to_str().unwrap(),
        "--format",
        format,
        "--output",
        out_path.to_str().unwrap(),
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "scan failed: format={format} stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&out_path).expect("read output");
    serde_json::from_slice(&bytes).expect("parse output json")
}

/// Extract the CDX 1.6 license tokens for a component. Handles both
/// CDX schema forms:
/// - single `{expression: "..."}` → returns `Expression(string)`
/// - list of `{license: {id or name: "..."}}` entries → returns
///   `Ids(vec![...])` preserving original ordering
enum CdxLicense {
    Expression(String),
    Ids(Vec<String>),
    Absent,
}

fn cdx_license_of(doc: &Value, name: &str) -> CdxLicense {
    let comps = doc["components"].as_array().expect("components array");
    let c = match comps.iter().find(|c| c["name"].as_str() == Some(name)) {
        Some(c) => c,
        None => return CdxLicense::Absent,
    };
    let entries = match c["licenses"].as_array() {
        Some(a) if !a.is_empty() => a,
        _ => return CdxLicense::Absent,
    };
    if entries.len() == 1 {
        if let Some(e) = entries[0]["expression"].as_str() {
            return CdxLicense::Expression(e.to_string());
        }
    }
    let mut tokens = Vec::new();
    for entry in entries {
        let l = &entry["license"];
        if let Some(id) = l["id"].as_str() {
            tokens.push(id.to_string());
        } else if let Some(nm) = l["name"].as_str() {
            tokens.push(nm.to_string());
        }
    }
    CdxLicense::Ids(tokens)
}

/// Assert that the CDX license entry for `name` — regardless of which
/// shape the builder picked — represents the token set `expected` with
/// the given operator (`"AND"` or `"OR"`). Also asserts NO raw BitBake
/// operators (`&`, `|`) appear anywhere in the emitted licenses block.
fn assert_cdx_license_tokens(doc: &Value, name: &str, expected_tokens: &[&str], operator: &str) {
    let observed = cdx_license_of(doc, name);
    match observed {
        CdxLicense::Absent => panic!(
            "CDX component `{name}` has no .licenses entry; expected {operator}-joined {expected_tokens:?}"
        ),
        CdxLicense::Expression(e) => {
            assert!(!e.contains('&'), "raw & in CDX expression: {e}");
            assert!(!e.contains('|'), "raw | in CDX expression: {e}");
            // Compare against the canonical joined form.
            let canonical = expected_tokens.join(&format!(" {operator} "));
            assert_eq!(
                e, canonical,
                "expected CDX expression `{canonical}`; got `{e}`"
            );
        }
        CdxLicense::Ids(ids) => {
            for id in &ids {
                assert!(!id.contains('&'), "raw & in CDX id: {id}");
                assert!(!id.contains('|'), "raw | in CDX id: {id}");
            }
            let mut ids_sorted = ids.clone();
            ids_sorted.sort();
            let mut expected_sorted: Vec<String> =
                expected_tokens.iter().map(|s| s.to_string()).collect();
            expected_sorted.sort();
            assert_eq!(
                ids_sorted, expected_sorted,
                "CDX id set mismatch (operator {operator}): got {ids:?}, expected {expected_tokens:?}"
            );
        }
    }
}

fn cdx_purl(doc: &Value, name: &str) -> String {
    let comps = doc["components"].as_array().expect("components array");
    comps
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
        .and_then(|c| c["purl"].as_str())
        .expect("purl present")
        .to_string()
}

fn cdx_version(doc: &Value, name: &str) -> String {
    let comps = doc["components"].as_array().expect("components array");
    comps
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
        .and_then(|c| c["version"].as_str())
        .expect("version present")
        .to_string()
}

fn spdx23_license_declared(doc: &Value, name: &str) -> Option<String> {
    let pkgs = doc["packages"].as_array().expect("packages array");
    pkgs.iter()
        .find(|p| p["name"].as_str() == Some(name))
        .and_then(|p| p["licenseDeclared"].as_str().map(str::to_string))
}

fn spdx23_external_ref_purl(doc: &Value, name: &str) -> String {
    let pkgs = doc["packages"].as_array().expect("packages array");
    let pkg = pkgs
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
        .expect("package present");
    let refs = pkg["externalRefs"].as_array().expect("externalRefs");
    refs.iter()
        .find(|r| r["referenceType"].as_str() == Some("purl"))
        .and_then(|r| r["referenceLocator"].as_str())
        .expect("purl externalRef")
        .to_string()
}

fn spdx3_license_expression_strings(doc: &Value) -> Vec<String> {
    doc["@graph"]
        .as_array()
        .expect("graph array")
        .iter()
        .filter(|e| e["type"].as_str() == Some("simplelicensing_LicenseExpression"))
        .filter_map(|e| {
            e["simplelicensing_licenseExpression"]
                .as_str()
                .map(str::to_string)
        })
        .collect()
}

fn spdx3_custom_license_ids(doc: &Value) -> Vec<String> {
    doc["@graph"]
        .as_array()
        .expect("graph array")
        .iter()
        .filter(|e| e["type"].as_str() == Some("simplelicensing_CustomLicense"))
        .filter_map(|e| e["spdxId"].as_str().map(str::to_string))
        .collect()
}

fn spdx3_software_package_iri(doc: &Value, name: &str) -> Option<String> {
    doc["@graph"]
        .as_array()?
        .iter()
        .find(|e| {
            e["type"].as_str() == Some("software_Package") && e["name"].as_str() == Some(name)
        })
        .and_then(|e| e["spdxId"].as_str().map(str::to_string))
}

fn spdx3_software_package_purl(doc: &Value, name: &str) -> String {
    doc["@graph"]
        .as_array()
        .expect("graph array")
        .iter()
        .find(|e| {
            e["type"].as_str() == Some("software_Package") && e["name"].as_str() == Some(name)
        })
        .and_then(|e| e["software_packageUrl"].as_str())
        .expect("software_packageUrl")
        .to_string()
}

fn spdx3_has_declared_license_targets(doc: &Value, from_iri: &str) -> Vec<String> {
    doc["@graph"]
        .as_array()
        .expect("graph array")
        .iter()
        .filter(|e| {
            e["type"].as_str() == Some("Relationship")
                && e["relationshipType"].as_str() == Some("hasDeclaredLicense")
                && e["from"].as_str() == Some(from_iri)
        })
        .flat_map(|e| {
            e["to"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .collect()
}

// ---------------------------------------------------------------------
// US1 — CDX license operator normalization (#550)
// ---------------------------------------------------------------------

fn build_ipk_with_license(dir: &Path, name: &str, license: &str) -> PathBuf {
    let filename = format!("{name}_1.0-r0_all.ipk");
    let control = format!(
        "Package: {name}\n\
         Version: 1.0-r0\n\
         Description: m190 fixture\n\
         Section: base\n\
         Priority: optional\n\
         Maintainer: nobody\n\
         License: {license}\n\
         Architecture: all\n"
    );
    build_synthetic_ipk(dir, &filename, &control)
}

#[test]
fn us1_cdx_bitbake_and_becomes_spdx_and() {
    let dir = tempfile::tempdir().unwrap();
    build_ipk_with_license(dir.path(), "bitbake-and-fixture", "GPL-2.0-only & MIT");
    let doc = scan(dir.path(), "cyclonedx-json");
    assert_cdx_license_tokens(&doc, "bitbake-and-fixture", &["GPL-2.0-only", "MIT"], "AND");
}

#[test]
fn us1_cdx_bitbake_or_becomes_spdx_or() {
    let dir = tempfile::tempdir().unwrap();
    build_ipk_with_license(dir.path(), "bitbake-or-fixture", "MIT | Apache-2.0");
    let doc = scan(dir.path(), "cyclonedx-json");
    assert_cdx_license_tokens(&doc, "bitbake-or-fixture", &["MIT", "Apache-2.0"], "OR");
}

#[test]
fn us1_cdx_double_operators_canonicalize() {
    let dir = tempfile::tempdir().unwrap();
    build_ipk_with_license(dir.path(), "double-ops-fixture", "MIT && Apache-2.0");
    let doc = scan(dir.path(), "cyclonedx-json");
    assert_cdx_license_tokens(&doc, "double-ops-fixture", &["MIT", "Apache-2.0"], "AND");
}

#[test]
fn us1_cdx_no_raw_operators_ever_leak() {
    // Broader safety-net: scan all four operator forms + a grouped
    // expression + verify NONE of them emit raw BitBake operators.
    let dir = tempfile::tempdir().unwrap();
    build_ipk_with_license(dir.path(), "s-and", "MIT & Apache-2.0");
    build_ipk_with_license(dir.path(), "s-or", "MIT | Apache-2.0");
    build_ipk_with_license(dir.path(), "d-and", "MIT && Apache-2.0");
    build_ipk_with_license(dir.path(), "d-or", "MIT || Apache-2.0");
    let doc = scan(dir.path(), "cyclonedx-json");
    for c in doc["components"].as_array().unwrap() {
        for l in c["licenses"].as_array().into_iter().flatten() {
            let e = l["expression"].as_str().unwrap_or("");
            assert!(!e.contains('&'), "raw & in component {}: {}", c["name"], e);
            assert!(!e.contains('|'), "raw | in component {}: {}", c["name"], e);
        }
    }
}

#[test]
fn us1_cross_format_license_expression_equality() {
    // FR-013: CDX, SPDX 2.3, SPDX 3 licenses all encode the same
    // canonical value `GPL-2.0-only AND MIT` for the same input.
    // CDX chooses the split-id form; SPDX 2.3 + SPDX 3 use the single
    // expression string. Both are equivalent.
    let dir = tempfile::tempdir().unwrap();
    build_ipk_with_license(dir.path(), "parity-fixture", "GPL-2.0-only & MIT");

    let cdx = scan(dir.path(), "cyclonedx-json");
    let spdx23 = scan(dir.path(), "spdx-2.3-json");
    let spdx3 = scan(dir.path(), "spdx-3-json");

    assert_cdx_license_tokens(&cdx, "parity-fixture", &["GPL-2.0-only", "MIT"], "AND");
    let spdx23_e = spdx23_license_declared(&spdx23, "parity-fixture").expect("licenseDeclared");
    assert_eq!(spdx23_e, "GPL-2.0-only AND MIT");
    let spdx3_es = spdx3_license_expression_strings(&spdx3);
    assert!(
        spdx3_es.contains(&"GPL-2.0-only AND MIT".to_string()),
        "SPDX 3 license expressions did not include the canonical form; got: {spdx3_es:?}"
    );
}

// ---------------------------------------------------------------------
// US2 — SPDX 3 license emission (#551)
// ---------------------------------------------------------------------

#[test]
fn us2_spdx3_emits_license_expression_for_compound_license() {
    // Direct #551 regression gate. Pre-m190: the SPDX 3 graph
    // contained ZERO simplelicensing_LicenseExpression elements when
    // an ipk carried a compound BitBake license.
    let dir = tempfile::tempdir().unwrap();
    build_ipk_with_license(dir.path(), "bitbake-and-fixture", "GPL-2.0-only & MIT");
    let doc = scan(dir.path(), "spdx-3-json");

    let exprs = spdx3_license_expression_strings(&doc);
    assert!(
        !exprs.is_empty(),
        "SPDX 3 graph must contain at least one simplelicensing_LicenseExpression; got zero"
    );
    assert!(
        exprs.iter().any(|e| e == "GPL-2.0-only AND MIT"),
        "expected 'GPL-2.0-only AND MIT' among license expressions; got: {exprs:?}"
    );

    // FR-006 relationship coverage per C1 remediation: the ipk's
    // software_Package MUST have a hasDeclaredLicense relationship
    // linking to the LicenseExpression element.
    let pkg_iri = spdx3_software_package_iri(&doc, "bitbake-and-fixture")
        .expect("software_Package IRI for the fixture");
    let targets = spdx3_has_declared_license_targets(&doc, &pkg_iri);
    assert!(
        !targets.is_empty(),
        "hasDeclaredLicense relationship missing for package {pkg_iri}"
    );
    // At least one relationship target must be a LicenseExpression
    // element with the canonical value.
    let license_iris: Vec<String> = doc["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"].as_str() == Some("simplelicensing_LicenseExpression"))
        .filter(|e| {
            e["simplelicensing_licenseExpression"].as_str() == Some("GPL-2.0-only AND MIT")
        })
        .filter_map(|e| e["spdxId"].as_str().map(str::to_string))
        .collect();
    assert!(
        license_iris.iter().any(|iri| targets.contains(iri)),
        "hasDeclaredLicense targets {targets:?} do not match any canonical LicenseExpression IRI {license_iris:?}"
    );
}

#[test]
fn us2_spdx3_emits_custom_license_for_vendor_operand() {
    // FR-005 / US2 acceptance #3: vendor-license operand → the
    // existing m154 CustomLicense sweep must emit at least one
    // simplelicensing_CustomLicense element.
    let dir = tempfile::tempdir().unwrap();
    build_ipk_with_license(dir.path(), "vendor-license-fixture", "SomeVendorLicense");
    let doc = scan(dir.path(), "spdx-3-json");
    let ids = spdx3_custom_license_ids(&doc);
    assert!(
        !ids.is_empty(),
        "SPDX 3 must emit a simplelicensing_CustomLicense for vendor operand; got zero"
    );
}

#[test]
fn us2_empty_license_omits_declared_license_relationship() {
    // Q3 answer B — SPDX 3 omits the hasDeclaredLicense relationship
    // AND the LicenseExpression element when License field is empty
    // or missing.
    let dir = tempfile::tempdir().unwrap();
    // Build an ipk with the License line entirely absent.
    let control = "Package: empty-license\n\
                   Version: 1.0-r0\n\
                   Description: m190 empty-license fixture\n\
                   Section: base\n\
                   Priority: optional\n\
                   Maintainer: nobody\n\
                   Architecture: all\n";
    build_synthetic_ipk(dir.path(), "empty-license_1.0-r0_all.ipk", control);
    let doc = scan(dir.path(), "spdx-3-json");
    let pkg_iri =
        spdx3_software_package_iri(&doc, "empty-license").expect("software_Package IRI");
    let targets = spdx3_has_declared_license_targets(&doc, &pkg_iri);
    assert!(
        targets.is_empty(),
        "empty License field must NOT emit hasDeclaredLicense; got: {targets:?}"
    );
}

// ---------------------------------------------------------------------
// US3 — ipk PURL epoch qualifier (#552)
// ---------------------------------------------------------------------

fn build_ipk_with_version(dir: &Path, name: &str, version: &str) -> PathBuf {
    let filename = format!("{name}_{version}_all.ipk");
    let control = format!(
        "Package: {name}\n\
         Version: {version}\n\
         Description: m190 fixture\n\
         Section: base\n\
         Priority: optional\n\
         Maintainer: nobody\n\
         License: MIT\n\
         Architecture: all\n"
    );
    build_synthetic_ipk(dir, &filename, &control)
}

#[test]
fn us3_epoch_positive_emits_qualifier_and_strips_prefix() {
    let dir = tempfile::tempdir().unwrap();
    build_ipk_with_version(dir.path(), "epoch-pos", "1:6.4-r0");
    let doc = scan(dir.path(), "cyclonedx-json");
    let purl = cdx_purl(&doc, "epoch-pos");
    assert_eq!(purl, "pkg:opkg/epoch-pos@6.4-r0?arch=all&epoch=1");
    let version = cdx_version(&doc, "epoch-pos");
    assert_eq!(version, "6.4-r0");
}

#[test]
fn us3_epoch_zero_omits_qualifier() {
    let dir = tempfile::tempdir().unwrap();
    build_ipk_with_version(dir.path(), "epoch-zero", "0:1.0-r0");
    let doc = scan(dir.path(), "cyclonedx-json");
    let purl = cdx_purl(&doc, "epoch-zero");
    assert_eq!(
        purl, "pkg:opkg/epoch-zero@1.0-r0?arch=all",
        "epoch=0 must NOT emit &epoch=0"
    );
    let version = cdx_version(&doc, "epoch-zero");
    assert_eq!(version, "1.0-r0");
}

#[test]
fn us3_no_epoch_preserves_byte_identity() {
    // FR-011 / SC-006 — no-epoch input MUST produce the same PURL as
    // the pre-m190 build_opkg_purl(name, version, arch, None).
    let dir = tempfile::tempdir().unwrap();
    build_ipk_with_version(dir.path(), "no-epoch", "2.0-r0");
    let doc = scan(dir.path(), "cyclonedx-json");
    let purl = cdx_purl(&doc, "no-epoch");
    assert_eq!(purl, "pkg:opkg/no-epoch@2.0-r0?arch=all");
    let version = cdx_version(&doc, "no-epoch");
    assert_eq!(version, "2.0-r0");
}

#[test]
fn us3_filename_only_epoch_extracted() {
    // FR-012 filename-source branch (C2 remediation): control body
    // absent (parse skips to filename fallback); filename encodes the
    // epoch. Must yield &epoch=5 in the emitted PURL.
    let dir = tempfile::tempdir().unwrap();
    // Filename encodes epoch:5, version 1.0-r0, arch all.
    let filename = "legacy_5:1.0-r0_all.ipk";
    // Write empty file (no valid ipk archive) → the reader's
    // filename-fallback path takes over.
    std::fs::write(dir.path().join(filename), b"").expect("write empty ipk");
    let doc = scan(dir.path(), "cyclonedx-json");
    let purl = cdx_purl(&doc, "legacy");
    assert_eq!(
        purl, "pkg:opkg/legacy@1.0-r0?arch=all&epoch=5",
        "filename-source epoch must reach the PURL builder"
    );
    let version = cdx_version(&doc, "legacy");
    assert_eq!(version, "1.0-r0");
}

// ---------------------------------------------------------------------
// FR-007 / SC-004 — spdx3-validate conformance gate for m190 fixtures
// (mirror of the sbom_user_metadata.rs pattern).
// ---------------------------------------------------------------------

fn spdx3_validate_or_skip(spdx3_path: &Path) {
    let bin_path = workspace_root().join(".venv/spdx3-validate/bin/spdx3-validate");
    if !bin_path.exists() {
        let require =
            std::env::var("WAYBILL_REQUIRE_SPDX3_VALIDATOR").ok().as_deref() == Some("1");
        if require {
            panic!(
                "spdx3-validate not found at {} and WAYBILL_REQUIRE_SPDX3_VALIDATOR=1 is set; \
                 run scripts/install-spdx3-validate.sh on this host before re-running CI.",
                bin_path.display()
            );
        }
        eprintln!(
            "[ipk_m190_parity] WARN: spdx3-validate not found at {}; skipping conformance gate \
             (set WAYBILL_REQUIRE_SPDX3_VALIDATOR=1 to fail hard in CI).",
            bin_path.display()
        );
        return;
    }
    let output = Command::new(&bin_path)
        .arg("--quiet")
        .arg("-j")
        .arg(spdx3_path)
        .output()
        .expect("spdx3-validate must run when binary exists");
    let combined_text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success() && !combined_text.contains("Violation of type"),
        "spdx3-validate reported violations for {}:\n{}",
        spdx3_path.display(),
        combined_text
    );
}

#[test]
fn us2_spdx3_validate_accepts_compound_license_ipk() {
    // FR-007 / SC-004 — SPDX 3 output for a compound-license ipk MUST
    // validate cleanly against spdx3-validate==0.0.5 with zero
    // conformance errors.
    let dir = tempfile::tempdir().unwrap();
    build_ipk_with_license(dir.path(), "conformance-fixture", "GPL-2.0-only & MIT");

    // Emit SPDX 3 to a stable path so the validator can chew on it.
    let workdir = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    let out_path = workdir.path().join("out.spdx3.json");

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "spdx-3-json",
        "--output",
        out_path.to_str().unwrap(),
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "spdx-3 emission failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    spdx3_validate_or_skip(&out_path);
}

#[test]
fn us3_cross_format_purl_parity() {
    // FR-013 — CDX .purl == SPDX 2.3 externalRefs[purl] == SPDX 3
    // software_packageUrl for every fixture, including epoch cases.
    let dir = tempfile::tempdir().unwrap();
    build_ipk_with_version(dir.path(), "parity-epoch", "1:6.4-r0");

    let cdx = scan(dir.path(), "cyclonedx-json");
    let spdx23 = scan(dir.path(), "spdx-2.3-json");
    let spdx3 = scan(dir.path(), "spdx-3-json");

    let cdx_p = cdx_purl(&cdx, "parity-epoch");
    let spdx23_p = spdx23_external_ref_purl(&spdx23, "parity-epoch");
    let spdx3_p = spdx3_software_package_purl(&spdx3, "parity-epoch");

    assert_eq!(cdx_p, "pkg:opkg/parity-epoch@6.4-r0?arch=all&epoch=1");
    assert_eq!(cdx_p, spdx23_p, "CDX/SPDX 2.3 PURL mismatch");
    assert_eq!(cdx_p, spdx3_p, "CDX/SPDX 3 PURL mismatch");
}
