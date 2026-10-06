//! Milestone 1071 US3 — the derivation record, the native "derived from"
//! links, and `waybill sbom verify-chain` over signed originals (SC-004,
//! SC-005).

mod common;
mod sbom_edit_support;

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};
use sigstore::crypto::signing_key::SigStoreKeyPair;
use sigstore::crypto::SigningScheme;

use common::normalize::apply_fake_home_env;
use sbom_edit_support::*;

/// (format flag, file name) for a scan.
const SCAN: [(&str, &str); 3] =
    [("cyclonedx-json", "orig.cdx.json"), ("spdx-2.3-json", "orig.spdx.json"), ("spdx-3-json", "orig.spdx3.json")];

struct Keys {
    private: PathBuf,
    public: PathBuf,
}

fn keypair(dir: &Path, name: &str) -> Keys {
    let signer = SigningScheme::ECDSA_P256_SHA256_ASN1.create_signer().unwrap();
    let pair: SigStoreKeyPair = signer.to_sigstore_keypair().unwrap();
    let private = dir.join(format!("{name}.pem"));
    let public = dir.join(format!("{name}.pub.pem"));
    std::fs::write(&private, pair.private_key_to_pem().unwrap()).unwrap();
    std::fs::write(&public, pair.public_key_to_pem().unwrap()).unwrap();
    Keys { private, public }
}

/// Scan the fixture project, optionally signed. Returns the output path.
fn scan(dir: &Path, format: &str, file: &str, key: Option<&Path>) -> PathBuf {
    scan_repo(dir, format, file, key, "https://git.corp.acme.example/acme/shop.git")
}

fn scan_repo(dir: &Path, format: &str, file: &str, key: Option<&Path>, repo: &str) -> PathBuf {
    let out = dir.join(file);
    let home = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, home.path());
    cmd.args(["--offline", "sbom", "scan", "--path"])
        .arg(fixture("project"))
        .args(["--no-deep-hash", "--repo", repo, "--format", format])
        .arg("--output")
        .arg(format!("{format}={}", out.display()))
        .env("RUST_LOG", "warn");
    if let Some(k) = key {
        cmd.arg("--sign-key").arg(k);
    }
    let status = cmd.status().unwrap();
    assert!(status.success(), "scan failed");
    out
}

fn verify(derived: &Path, originals: &[&Path], keys: &[&Path], extra: &[&str]) -> (bool, Value) {
    let mut cmd = Command::new(bin());
    cmd.args(["sbom", "verify-chain"]).arg(derived).arg("--json").env("RUST_LOG", "warn");
    for o in originals {
        cmd.arg("--original").arg(o);
    }
    for k in keys {
        cmd.arg("--key").arg(k);
    }
    cmd.args(extra);
    let out = cmd.output().unwrap();
    let report: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("verify-chain output is not JSON ({e}): {}", String::from_utf8_lossy(&out.stderr)));
    assert_eq!(out.status.success(), report["ok"] == json!(true), "exit status disagrees with `ok`");
    (out.status.success(), report)
}

fn status(report: &Value, step: usize, field: &str) -> String {
    report["steps"][step][field]["status"].as_str().unwrap_or("").to_string()
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    data_encoding::HEXLOWER.encode(&sha2::Sha256::digest(bytes))
}

fn signed_edit(dir: &Path, input: &Path, out_name: &str, key: &Path, args: &[&str]) -> PathBuf {
    let out = dir.join(out_name);
    let mut all = args.to_vec();
    let k = key.to_str().unwrap();
    all.extend(["--sign-key", k]);
    let e = edit_to(input, &out, &all, tempfile::tempdir().unwrap());
    e.assert_ok();
    out
}

/// Flip one character inside a string value, keeping the JSON valid.
fn tamper(path: &Path, needle: &str, replacement: &str) {
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains(needle), "tamper target `{needle}` not in {}", path.display());
    std::fs::write(path, text.replacen(needle, replacement, 1)).unwrap();
}

#[test]
fn the_record_and_native_link_are_present_in_each_format() {
    for (label, file) in FORMATS {
        let input = std::fs::read(fixture(file)).unwrap();
        let out = edit(&fixture(file), &["--drop", "scope=development"]).assert_ok().json();
        let record = derivation(&out).unwrap_or_else(|| panic!("{label}: no record"));
        assert_eq!(record["schema"], "waybill-derivation/v1", "{label}");
        assert_eq!(record["original"]["sha256"], json!(sha256_hex(&input)), "{label}");
        assert_eq!(record["original"]["signature"], json!({"kind": "none"}), "{label}");
        assert_eq!(record["operations"][0]["category"], "drop-components", "{label}");
        let text = record.to_string();
        assert!(!text.contains("jest-lite"), "{label}: a dropped value in the record");
        let link = match label {
            "cdx" => out["externalReferences"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["type"] == "bom" && r["url"] == json!(format!("urn:sha256:{}", sha256_hex(&input)))),
            "spdx23" => {
                out["externalDocumentRefs"].as_array().is_some_and(|a| {
                    a.iter().any(|r| r["checksum"]["checksumValue"] == json!(sha256_hex(&input)))
                }) && out["relationships"].as_array().unwrap().iter().any(|r| r["relationshipType"] == "AMENDS")
            }
            _ => out["@graph"].as_array().unwrap().iter().any(|e| e["relationshipType"] == "amendedBy"),
        };
        assert!(link, "{label}: native derivation link missing");
        let input_doc = read_json(&fixture(file));
        assert_conforms_like(&input_doc, &out, label);
    }
}

/// SC-004: the tamper matrix, per format, with static-key signatures.
#[test]
fn verify_chain_catches_every_tampering() {
    let dir = tempfile::tempdir().unwrap();
    let keys = keypair(dir.path(), "k");
    let stranger = keypair(dir.path(), "stranger");
    for (format, file) in SCAN {
        let d = dir.path().join(format);
        std::fs::create_dir_all(&d).unwrap();
        let original = scan(&d, format, file, Some(&keys.private));
        let derived = signed_edit(&d, &original, &format!("derived.{file}"), &keys.private, &["--drop", "scope=development"]);
        let pk = keys.public.as_path();

        let (ok, report) = verify(&derived, &[&original], &[pk], &[]);
        assert!(ok, "{format}: clean chain failed: {report:#}");
        assert_eq!(status(&report, 0, "own_signature"), "verified", "{format}");
        assert_eq!(status(&report, 0, "original_hash"), "matched", "{format}");
        assert_eq!(status(&report, 0, "original_signature"), "verified", "{format}");

        // A key that didn't sign either document.
        let (ok, _) = verify(&derived, &[&original], &[stranger.public.as_path()], &[]);
        assert!(!ok, "{format}: verified under the wrong key");

        // (a) An original byte changed.
        let a = d.join(format!("a.{file}"));
        std::fs::copy(&original, &a).unwrap();
        tamper(&a, "acme-shop", "acme-shoq");
        let (ok, report) = verify(&derived, &[&a], &[pk], &[]);
        assert!(!ok && status(&report, 0, "original_hash") == "mismatched", "{format}: (a)");

        // (b) A derivative byte changed.
        let b = d.join(format!("b.{file}"));
        std::fs::copy(&derived, &b).unwrap();
        let side = PathBuf::from(format!("{}.sig.json", derived.display()));
        if side.exists() {
            std::fs::copy(&side, format!("{}.sig.json", b.display())).unwrap();
        }
        tamper(&b, "express", "exprest");
        let (ok, report) = verify(&b, &[&original], &[pk], &[]);
        assert!(!ok && status(&report, 0, "own_signature") == "failed", "{format}: (b) {report:#}");

        // (c) A different original (scans are deterministic under a fixed
        // timestamp, so it differs in what it describes).
        let other = scan_repo(&d, format, &format!("other.{file}"), Some(&keys.private), "https://example.org/other.git");
        let (ok, _) = verify(&derived, &[&other], &[pk], &[]);
        assert!(!ok, "{format}: (c)");

        // (d) The derivative's signature removed.
        let unsigned = d.join(format!("d.{file}"));
        let mut doc = read_json(&derived);
        if let Some(o) = doc.as_object_mut() {
            o.remove("signature");
        }
        std::fs::write(&unsigned, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        let (ok, report) = verify(&unsigned, &[&original], &[pk], &[]);
        assert!(!ok && status(&report, 0, "own_signature") == "failed", "{format}: (d) {report:#}");

        // (e) The embedded original signature altered: the derivative's own
        // signature covers the record, and the altered material no longer
        // verifies against the original.
        let e = d.join(format!("e.{file}"));
        let text = std::fs::read_to_string(&derived).unwrap();
        let record = derivation(&read_json(&derived)).unwrap();
        let sig_value = record["original"]["signature"]["material"]["value"]
            .as_str()
            .or_else(|| record["original"]["signature"]["material"]["signatures"][0]["sig"].as_str())
            .unwrap()
            .to_string();
        let flipped: String = sig_value.chars().rev().collect();
        std::fs::write(&e, text.replace(&sig_value, &flipped)).unwrap();
        if let Ok(side) = std::fs::read(format!("{}.sig.json", derived.display())) {
            std::fs::write(format!("{}.sig.json", e.display()), side).unwrap();
        }
        let (ok, report) = verify(&e, &[&original], &[pk], &[]);
        assert!(!ok, "{format}: (e) {report:#}");
    }
}

#[test]
fn a_two_step_chain_verifies_with_two_originals() {
    let dir = tempfile::tempdir().unwrap();
    let keys = keypair(dir.path(), "k");
    let original = scan(dir.path(), "cyclonedx-json", "orig.cdx.json", Some(&keys.private));
    let first = signed_edit(dir.path(), &original, "first.cdx.json", &keys.private, &["--drop", "scope=development"]);
    let second = signed_edit(dir.path(), &first, "second.cdx.json", &keys.private, &["--drop", "tier=file"]);
    let record = derivation(&read_json(&second)).unwrap();
    assert_eq!(record["ancestors"].as_array().map(Vec::len), Some(1));
    assert_eq!(record["ancestors"][0]["operations"][0]["category"], "drop-components");
    let (ok, report) = verify(&second, &[&first, &original], &[&keys.public], &[]);
    assert!(ok, "{report:#}");
    assert_eq!(report["steps"].as_array().map(Vec::len), Some(2));
    // One original short: the second step is reported, not failed.
    let (ok, report) = verify(&second, &[&first], &[&keys.public], &[]);
    assert!(ok, "{report:#}");
    assert_eq!(status(&report, 1, "original_hash"), "original-not-supplied");
}

#[test]
fn an_unsigned_original_is_reported_unsigned_not_verified() {
    let dir = tempfile::tempdir().unwrap();
    let original = scan(dir.path(), "spdx-2.3-json", "orig.spdx.json", None);
    let derived = dir.path().join("derived.spdx.json");
    edit_to(&original, &derived, &["--drop", "scope=development"], tempfile::tempdir().unwrap()).assert_ok();
    let (ok, report) = verify(&derived, &[&original], &[], &[]);
    assert!(ok, "{report:#}");
    assert_eq!(status(&report, 0, "own_signature"), "unsigned");
    assert_eq!(status(&report, 0, "original_signature"), "unsigned");
}

/// Analysis H2: signature material that contains a value the edit redacts
/// is referenced by digest, never embedded, and verifies from the sidecar.
#[test]
fn signature_material_with_a_redacted_value_is_referenced_by_digest() {
    let dir = tempfile::tempdir().unwrap();
    let keys = keypair(dir.path(), "k");
    let original = scan(dir.path(), "spdx-2.3-json", "orig.spdx.json", Some(&keys.private));
    // A key id naming the internal host: still a valid DSSE envelope.
    let sidecar = PathBuf::from(format!("{}.sig.json", original.display()));
    let mut env = read_json(&sidecar);
    env["signatures"][0]["keyid"] = json!("signer@git.corp.acme.example");
    std::fs::write(&sidecar, serde_json::to_string_pretty(&env).unwrap()).unwrap();

    let derived = dir.path().join("derived.spdx.json");
    let k = keys.private.to_str().unwrap();
    edit_to(&original, &derived, &["--redact", "hosts:remove=*.corp.acme.example", "--sign-key", k], tempfile::tempdir().unwrap())
        .assert_ok();
    let text = std::fs::read_to_string(&derived).unwrap();
    assert!(!text.contains("corp.acme.example"), "the host leaked");
    let sig = &derivation(&read_json(&derived)).unwrap()["original"]["signature"];
    assert_eq!(sig["embedded"], json!(false));
    assert_eq!(sig["reason"], json!("contains-redacted-values"));
    assert!(sig.get("material").is_none());

    // Found next to the original.
    let (ok, report) = verify(&derived, &[&original], &[&keys.public], &[]);
    assert!(ok, "{report:#}");
    // Moved away: needs --original-signature.
    let moved = dir.path().join("moved.sig.json");
    std::fs::rename(&sidecar, &moved).unwrap();
    let (ok, _) = verify(&derived, &[&original], &[&keys.public], &[]);
    assert!(!ok, "verified without the signature material");
    let (ok, report) = verify(&derived, &[&original], &[&keys.public], &["--original-signature", moved.to_str().unwrap()]);
    assert!(ok, "{report:#}");
}
