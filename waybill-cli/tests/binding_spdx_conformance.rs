//! #1147 — a `--bind-to-source` SPDX document must conform.
//!
//! Milestone 072 linked the bound document to its source SBOM with
//! `BUILT_FROM` (SPDX 2.3) and `built_from` (SPDX 3), neither of which the
//! formats define, so every bound document failed the SPDX 2.3 schema and
//! `spdx3-validate`. No gate ran on a bound document, because binding is
//! emitted only for `--image` scans. This test scans a synthetic image
//! tarball, hermetically, and runs both gates.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::normalize::apply_fake_home_env;
use common::{bin, workspace_root};

/// A docker-save tarball with one layer: os-release and one dpkg package.
fn build_image_tarball() -> (PathBuf, tempfile::TempDir) {
    let mut layer_bytes = Vec::new();
    {
        let mut layer = tar::Builder::new(&mut layer_bytes);
        let files: [(&str, &[u8]); 2] = [
            ("etc/os-release", b"NAME=\"Debian\"\nID=debian\nVERSION_ID=\"12\"\nVERSION_CODENAME=bookworm\n"),
            (
                "var/lib/dpkg/status",
                b"Package: foo\nStatus: install ok installed\nVersion: 1.0\nArchitecture: amd64\nMaintainer: Debian <debian@example.org>\n\n",
            ),
        ];
        for (path, body) in files {
            let mut h = tar::Header::new_ustar();
            h.set_path(path).unwrap();
            h.set_size(body.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            layer.append(&h, body).unwrap();
        }
        layer.finish().unwrap();
    }
    let manifest = r#"[{"Config":"config.json","RepoTags":["docker.io/test/foo:v1"],"Layers":["layer0/layer.tar"]}]"#;
    let td = tempfile::tempdir().unwrap();
    let tarball = td.path().join("img.tar");
    {
        let mut outer = tar::Builder::new(std::fs::File::create(&tarball).unwrap());
        for (path, body) in [("manifest.json", manifest.as_bytes()), ("layer0/layer.tar", layer_bytes.as_slice())] {
            let mut h = tar::Header::new_ustar();
            h.set_path(path).unwrap();
            h.set_size(body.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            outer.append(&h, body).unwrap();
        }
        outer.into_inner().unwrap().flush().unwrap();
    }
    (tarball, td)
}

fn scan(tarball: &Path, home: &Path, args: &[&str]) {
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, home);
    let out = cmd
        .args(["--offline", "sbom", "scan", "--image"])
        .arg(tarball)
        .args(["--no-deep-hash"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "scan failed: {}", String::from_utf8_lossy(&out.stderr));
}

/// Scan once for a source SBOM, then again bound to it, in both SPDX formats.
fn bound_documents() -> (serde_json::Value, PathBuf, PathBuf, tempfile::TempDir, String) {
    let (tarball, _img) = build_image_tarball();
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.cdx.json");
    scan(&tarball, home.path(), &["--format", "cyclonedx-json", "--output", &format!("cyclonedx-json={}", source.display())]);
    let source_sha = {
        use sha2::Digest;
        data_encoding::HEXLOWER.encode(&sha2::Sha256::digest(std::fs::read(&source).unwrap()))
    };
    let spdx23 = dir.path().join("bound.spdx.json");
    let spdx3 = dir.path().join("bound.spdx3.json");
    scan(
        &tarball,
        home.path(),
        &[
            "--bind-to-source",
            source.to_str().unwrap(),
            "--format",
            "spdx-2.3-json,spdx-3-json",
            "--output",
            &format!("spdx-2.3-json={}", spdx23.display()),
            "--output",
            &format!("spdx-3-json={}", spdx3.display()),
        ],
    );
    // The source is named by its own identifier, never by the path it was
    // read from: that is not an IRI, and it leaks the scanning host's
    // filesystem into the document.
    for out in [&spdx23, &spdx3] {
        let text = std::fs::read_to_string(out).unwrap();
        assert!(!text.contains(source.to_str().unwrap()), "{} names the source SBOM by its local path", out.display());
    }
    let doc23: serde_json::Value = serde_json::from_slice(&std::fs::read(&spdx23).unwrap()).unwrap();
    (doc23, spdx23, spdx3, dir, source_sha)
}

#[test]
fn spdx23_binding_is_descendant_of_and_schema_valid() {
    let (doc, _, _, _dir, source_sha) = bound_documents();
    let refs = doc["externalDocumentRefs"].as_array().unwrap();
    assert!(
        refs.iter().any(|r| r["externalDocumentId"] == "DocumentRef-source-sbom"
            && r["checksum"]["checksumValue"] == source_sha.as_str()),
        "no externalDocumentRefs entry for the source SBOM: {refs:#?}"
    );
    let rels = doc["relationships"].as_array().unwrap();
    assert!(
        rels.iter().any(|r| r["spdxElementId"] == "SPDXRef-DOCUMENT"
            && r["relationshipType"] == "DESCENDANT_OF"
            && r["relatedSpdxElement"] == "DocumentRef-source-sbom:SPDXRef-DOCUMENT"),
        "no DESCENDANT_OF edge to the source document"
    );
    assert!(!rels.iter().any(|r| r["relationshipType"] == "BUILT_FROM"));

    let schema: serde_json::Value = serde_json::from_slice(
        &std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/schemas/spdx-2.3.json")).unwrap(),
    )
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator.iter_errors(&doc).map(|e| format!("{}: {e}", e.instance_path())).collect();
    assert!(errors.is_empty(), "bound SPDX 2.3 document fails the schema: {errors:#?}");
}

#[test]
fn spdx3_binding_is_descendant_of_and_conformant() {
    let (_, _, spdx3_path, _dir, source_sha) = bound_documents();
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(&spdx3_path).unwrap()).unwrap();
    let graph = doc["@graph"].as_array().unwrap();
    let document = graph.iter().find(|e| e["type"] == "SpdxDocument").unwrap();
    let import = &document["import"][0];
    assert_eq!(import["verifiedUsing"][0]["hashValue"], source_sha.as_str());
    let source_iri = import["externalSpdxId"].clone();
    assert!(
        graph.iter().any(|e| e["relationshipType"] == "descendantOf"
            && e["from"] == document["spdxId"]
            && e["to"] == serde_json::json!([source_iri])),
        "no descendantOf edge to the imported source document"
    );
    assert!(!graph.iter().any(|e| e["relationshipType"] == "built_from"));

    // spdx3-validate, when installed (milestone 078 convention).
    let validator = workspace_root().join(".venv/spdx3-validate/bin/spdx3-validate");
    if !validator.exists() {
        assert!(
            std::env::var("WAYBILL_REQUIRE_SPDX3_VALIDATOR").ok().as_deref() != Some("1"),
            "spdx3-validate not found at {} and WAYBILL_REQUIRE_SPDX3_VALIDATOR=1 is set",
            validator.display()
        );
        eprintln!("WARN: spdx3-validate not found; skipping its conformance gate");
        return;
    }
    let out = Command::new(&validator).args(["--quiet", "-j"]).arg(&spdx3_path).output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(
        out.status.success() && !text.contains("Violation of type"),
        "spdx3-validate rejected the bound SPDX 3 document:\n{text}"
    );
}
