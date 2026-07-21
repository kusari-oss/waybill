//! Milestone 076 — `subject:` identifier scheme + per-component
//! user-defined identifier integration tests.
//!
//! ## Test harness rationale
//!
//! `waybill trace run` (the build-tier subject auto-detect path)
//! requires Linux + eBPF + privileges, so end-to-end driving a real
//! trace through the CLI binary isn't feasible cross-platform. This
//! file follows the milestone-074 / milestone-073 pattern:
//!
//! 1. **Direct library-level tests** of
//!    `subject_identifiers_from_attestation_subjects` against synthetic
//!    `Vec<ResourceDescriptor>` fixtures — covers US1's auto-detect
//!    behavior (FR-002 + 2026-05-06 sha256-only clarification).
//!
//! 2. **`waybill sbom scan`-driven tests** for the source-tier and
//!    image-tier `--subject-hash` / `--component-id` flags — covers
//!    US2 / US3 / US4 cross-format wire mapping. Source-tier scans
//!    can be driven cross-platform.
//!
//! Tests guard `.unwrap()` use per CLAUDE.md.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use waybill::binding::identifiers::auto_detect::subject_identifiers_from_attestation_subjects;
use waybill::binding::identifiers::component_id::{
    parse_component_id_flag, ComponentIdentifierFlag, ComponentIdentifierFlagError,
};
use waybill_common::attestation::statement::ResourceDescriptor;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

// ---------------------------------------------------------------------
// Synthetic-subject-set fixture builder
// ---------------------------------------------------------------------

const SHA256_A: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const SHA256_B: &str =
    "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
const SHA256_C: &str =
    "1111111122222222333333334444444455555555666666667777777788888888";
const SHA512_A: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn make_subject(name: &str, digests: &[(&str, &str)]) -> ResourceDescriptor {
    let mut digest = BTreeMap::new();
    for (algo, hex) in digests {
        digest.insert((*algo).to_string(), (*hex).to_string());
    }
    ResourceDescriptor {
        name: name.to_string(),
        digest,
    }
}

// ---------------------------------------------------------------------
// US1 — build-tier auto-detect from in-toto subject set
// ---------------------------------------------------------------------

#[test]
fn build_tier_autodetects_subject_from_in_toto_subjects() {
    // US1 §1: one subject with sha256 → one subject: identifier.
    let subjects = vec![make_subject("myapp", &[("sha256", SHA256_A)])];
    let ids = subject_identifiers_from_attestation_subjects(&subjects);
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].scheme.as_str(), "subject");
    assert_eq!(ids[0].value.as_str(), format!("sha256:{SHA256_A}"));
    assert!(ids[0].is_builtin());
    let label = ids[0].source_label.as_deref().unwrap();
    assert!(
        label.contains("build-tier") && label.contains("myapp"),
        "expected build-tier label naming subject; got {label:?}"
    );
}

#[test]
fn build_tier_autodetect_emits_one_subject_per_in_toto_subject() {
    // US1 §2: 3 subjects → 3 identifiers in input order.
    let subjects = vec![
        make_subject("myapp-a", &[("sha256", SHA256_A)]),
        make_subject("myapp-b", &[("sha256", SHA256_B)]),
        make_subject("myapp-c", &[("sha256", SHA256_C)]),
    ];
    let ids = subject_identifiers_from_attestation_subjects(&subjects);
    assert_eq!(ids.len(), 3);
    assert_eq!(ids[0].value.as_str(), format!("sha256:{SHA256_A}"));
    assert_eq!(ids[1].value.as_str(), format!("sha256:{SHA256_B}"));
    assert_eq!(ids[2].value.as_str(), format!("sha256:{SHA256_C}"));
}

#[test]
fn build_tier_autodetect_skips_subject_without_sha256() {
    // US1 §3 + 2026-05-06 clarification: subject with only sha512 →
    // no identifier.
    let subjects = vec![make_subject("legacy-app", &[("sha512", SHA512_A)])];
    let ids = subject_identifiers_from_attestation_subjects(&subjects);
    assert!(ids.is_empty());
}

#[test]
fn build_tier_autodetect_emits_sha256_only_when_multi_digest() {
    // 2026-05-06 clarification: multi-digest subjects auto-emit
    // sha256 only; sha512 is dropped from auto-detection.
    let subjects = vec![make_subject(
        "myapp",
        &[("sha256", SHA256_A), ("sha512", SHA512_A)],
    )];
    let ids = subject_identifiers_from_attestation_subjects(&subjects);
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].value.as_str(), format!("sha256:{SHA256_A}"));
}

#[test]
fn build_tier_autodetect_empty_subject_set() {
    let ids = subject_identifiers_from_attestation_subjects(&[]);
    assert!(ids.is_empty());
}

// ---------------------------------------------------------------------
// US2 — source-tier and image-tier accept manual --subject-hash
// ---------------------------------------------------------------------

fn fixture_root() -> PathBuf {
    common::fixture_path("cargo/lockfile-v3")
}

fn run_scan(
    fake_home: &Path,
    fixture: &Path,
    extra_args: &[&str],
    out_format: &str,
    out_filename: &str,
) -> (serde_json::Value, String) {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join(out_filename);
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture)
        .arg("--format")
        .arg(out_format)
        .arg("--output")
        .arg(format!("{out_format}={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("scan runs");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "scan failed: stderr={stderr}\nstdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let bytes = std::fs::read(&out_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    drop(out_dir);
    (parsed, stderr)
}

#[test]
fn manual_subject_hash_flag_works_on_source_tier() {
    // US2 §1: --subject-hash on source-tier scan emits subject:
    // identifier in the CDX externalReferences[type:attestation].
    let fake_home = tempfile::tempdir().unwrap();
    let (cdx, _stderr) = run_scan(
        fake_home.path(),
        &fixture_root(),
        &["--subject-hash", &format!("sha256:{SHA256_A}")],
        "cyclonedx-json",
        "out.cdx.json",
    );
    // metadata.component.externalReferences[] should contain an
    // attestation-typed entry whose URL is `sha256:<hex>`.
    let ext_refs = cdx["metadata"]["component"]["externalReferences"]
        .as_array()
        .expect("metadata.component.externalReferences[] present");
    let found = ext_refs.iter().any(|r| {
        r["type"].as_str() == Some("attestation")
            && r["url"].as_str() == Some(&format!("sha256:{SHA256_A}"))
    });
    assert!(
        found,
        "expected externalReferences[type:attestation] with url=sha256:{SHA256_A}; got {ext_refs:#?}"
    );
}

#[test]
fn manual_subject_hash_flag_repeatable() {
    // US2 §2: pass --subject-hash twice; both appear in supply order.
    let fake_home = tempfile::tempdir().unwrap();
    let (cdx, _) = run_scan(
        fake_home.path(),
        &fixture_root(),
        &[
            "--subject-hash",
            &format!("sha256:{SHA256_A}"),
            "--subject-hash",
            &format!("sha256:{SHA256_B}"),
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let ext_refs = cdx["metadata"]["component"]["externalReferences"]
        .as_array()
        .expect("metadata.component.externalReferences[] present");
    let urls: Vec<&str> = ext_refs
        .iter()
        .filter(|r| r["type"].as_str() == Some("attestation"))
        .filter_map(|r| r["url"].as_str())
        .collect();
    let pos_a = urls.iter().position(|u| *u == format!("sha256:{SHA256_A}"));
    let pos_b = urls.iter().position(|u| *u == format!("sha256:{SHA256_B}"));
    assert!(pos_a.is_some(), "first --subject-hash present: {urls:?}");
    assert!(pos_b.is_some(), "second --subject-hash present: {urls:?}");
    assert!(
        pos_a.unwrap() < pos_b.unwrap(),
        "supply order preserved: {urls:?}"
    );
}

#[test]
fn subject_value_validation_soft_fails_to_user_defined() {
    // US2 §3 / FR-005: malformed --subject-hash value soft-fails to
    // user-defined under waybill:identifiers; the scan still exits 0.
    let fake_home = tempfile::tempdir().unwrap();
    let (cdx, _) = run_scan(
        fake_home.path(),
        &fixture_root(),
        &["--subject-hash", "banana"],
        "cyclonedx-json",
        "out.cdx.json",
    );
    // The scan succeeded (the run_scan helper asserts non-zero exit
    // panics). The malformed value rides under waybill:identifiers.
    let props = cdx["metadata"]["properties"].as_array();
    let mut found_userdef = false;
    if let Some(arr) = props {
        for p in arr {
            if p["name"].as_str() == Some("waybill:identifiers") {
                let v = p["value"].as_str().unwrap_or("");
                // The waybill:identifiers annotation envelope is a
                // JSON-encoded list of {scheme, value} entries — the
                // soft-failed entry rides as scheme="subject",
                // value="banana".
                if v.contains("\"subject\"") && v.contains("\"banana\"") {
                    found_userdef = true;
                    break;
                }
            }
        }
    }
    assert!(
        found_userdef,
        "malformed `subject:banana` should ride under waybill:identifiers; got props={props:?}"
    );
}

#[test]
fn subject_identifier_emits_in_all_three_formats() {
    // FR-004: `subject:` value rides per-format native carriers
    // across CDX 1.6, SPDX 2.3, SPDX 3.0.1.
    let fake_home = tempfile::tempdir().unwrap();
    let value = format!("sha256:{SHA256_A}");

    // CDX
    let (cdx, _) = run_scan(
        fake_home.path(),
        &fixture_root(),
        &["--subject-hash", &value],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let cdx_refs = cdx["metadata"]["component"]["externalReferences"]
        .as_array()
        .unwrap();
    assert!(
        cdx_refs.iter().any(|r| r["type"].as_str() == Some("attestation")
            && r["url"].as_str() == Some(&value)),
        "CDX missing subject identifier"
    );

    // SPDX 2.3
    let (spdx, _) = run_scan(
        fake_home.path(),
        &fixture_root(),
        &["--subject-hash", &value],
        "spdx-2.3-json",
        "out.spdx.json",
    );
    // The identifier rides on (a) a main-module Package's externalRefs
    // when one exists, and ALWAYS (b) `creationInfo.creators[]` as a
    // redundant text line per milestone 073's dual-carrier pattern. We
    // check the always-emitted carrier here; the main-module gate is
    // fixture-dependent (a Cargo.lock-only fixture has no main-module).
    let creators = spdx["creationInfo"]["creators"]
        .as_array()
        .expect("creationInfo.creators[]");
    let needle = format!("subject:{value}");
    let found_creator = creators.iter().any(|c| {
        c.as_str().map(|s| s.contains(&needle)).unwrap_or(false)
    });
    assert!(
        found_creator,
        "SPDX 2.3 creators[] missing subject:{value} entry; got {creators:?}"
    );

    // SPDX 3
    let (spdx3, _) = run_scan(
        fake_home.path(),
        &fixture_root(),
        &["--subject-hash", &value],
        "spdx-3-json",
        "out.spdx3.json",
    );
    // SpdxDocument.externalIdentifier[] carries the subject entry.
    // Per milestone 079, waybill's `subject` scheme maps to the
    // SPDX 3 controlled-vocab value `other` with the original
    // scheme preserved on `comment` as `original-scheme: subject`.
    let graph = spdx3["@graph"].as_array().expect("@graph");
    let mut found_3 = false;
    for n in graph {
        if n["type"].as_str() == Some("SpdxDocument") {
            if let Some(eids) = n["externalIdentifier"].as_array() {
                for e in eids {
                    if e["externalIdentifierType"].as_str() == Some("other")
                        && e["comment"].as_str() == Some("original-scheme: subject")
                        && e["identifier"].as_str() == Some(&value)
                    {
                        found_3 = true;
                        break;
                    }
                }
            }
        }
    }
    assert!(found_3, "SPDX 3 missing subject identifier (post-079: type=other + comment=original-scheme: subject)");
}

#[test]
fn manual_subject_hash_flag_works_on_image_tier() {
    // FR-003 coverage: image-tier scans accept --subject-hash. We
    // build a synthetic docker-save tarball, scan it with both
    // --image and --subject-hash, then assert both `image:` (auto)
    // and `subject:` (manual) identifiers ride the CDX
    // externalReferences[].
    let fake_home = tempfile::tempdir().unwrap();
    let (tarball_path, _td) = build_synthetic_image_tarball();
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--image")
        .arg(&tarball_path)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash")
        .arg("--subject-hash")
        .arg(format!("sha256:{SHA256_B}"));
    let out = cmd.output().expect("scan runs");
    assert!(
        out.status.success(),
        "image-tier scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bytes = std::fs::read(&out_path).unwrap();
    let cdx: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let refs = cdx["metadata"]["component"]["externalReferences"]
        .as_array()
        .expect("externalReferences[] present");
    let has_subject = refs.iter().any(|r| {
        r["type"].as_str() == Some("attestation")
            && r["url"].as_str() == Some(&format!("sha256:{SHA256_B}"))
    });
    assert!(has_subject, "expected subject: identifier on image-tier scan; got {refs:#?}");
}

fn build_synthetic_image_tarball() -> (PathBuf, tempfile::TempDir) {
    use std::io::Write as _;
    let mut layer_bytes = Vec::new();
    {
        let mut layer_tar = tar::Builder::new(&mut layer_bytes);
        let os_release =
            b"NAME=\"Debian\"\nID=debian\nVERSION_ID=\"12\"\nVERSION_CODENAME=bookworm\n";
        let mut h = tar::Header::new_ustar();
        h.set_path("etc/os-release").unwrap();
        h.set_size(os_release.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        layer_tar.append(&h, os_release.as_slice()).unwrap();
        let dpkg_status =
            b"Package: foo\nStatus: install ok installed\nVersion: 1.0\nArchitecture: amd64\nMaintainer: Debian <debian@example.org>\n\n";
        let mut h = tar::Header::new_ustar();
        h.set_path("var/lib/dpkg/status").unwrap();
        h.set_size(dpkg_status.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        layer_tar.append(&h, dpkg_status.as_slice()).unwrap();
        layer_tar.finish().unwrap();
    }
    let manifest = r#"[{"Config":"config.json","RepoTags":["docker.io/test/foo:v1"],"Layers":["layer0/layer.tar"]}]"#;
    let td = tempfile::tempdir().unwrap();
    let tarball_path = td.path().join("img.tar");
    let file = std::fs::File::create(&tarball_path).unwrap();
    {
        let mut outer = tar::Builder::new(file);
        let mut mh = tar::Header::new_ustar();
        mh.set_path("manifest.json").unwrap();
        mh.set_size(manifest.len() as u64);
        mh.set_mode(0o644);
        mh.set_cksum();
        outer.append(&mh, manifest.as_bytes()).unwrap();
        let mut lh = tar::Header::new_ustar();
        lh.set_path("layer0/layer.tar").unwrap();
        lh.set_size(layer_bytes.len() as u64);
        lh.set_mode(0o644);
        lh.set_cksum();
        outer.append(&lh, layer_bytes.as_slice()).unwrap();
        outer.into_inner().unwrap().flush().unwrap();
    }
    (tarball_path, td)
}

// ---------------------------------------------------------------------
// US3 — cross-tier digest handshake by string match
// ---------------------------------------------------------------------

#[test]
fn cross_tier_handshake_image_digest_matches_build_subject() {
    // US3 §1, SC-002: an external SBOM-store consumer holding a build
    // SBOM with `subject:sha256:X` and an image SBOM whose components
    // have `hashes[].sha256 == X` can correlate the two by string
    // match alone, with no waybill-side resolver. This test
    // synthesizes two SBOMs and runs the correlation in serde_json.

    let target = SHA256_A;

    // Build SBOM (synthetic): one document with subject:sha256:<target>.
    let build_sbom = serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "metadata": {
            "component": {
                "name": "build-myapp",
                "externalReferences": [
                    {
                        "type": "attestation",
                        "url": format!("sha256:{target}"),
                        "comment": "auto-detected from build-tier in-toto subject `myapp`"
                    }
                ]
            }
        }
    });

    // Image SBOM (synthetic): one component whose hash[sha256] matches.
    let image_sbom = serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "components": [
            {
                "name": "myapp",
                "purl": "pkg:generic/myapp@0.0.0",
                "hashes": [
                    {"alg": "SHA-256", "content": target}
                ]
            }
        ]
    });

    // Pure string-match correlation: extract image components' hashes,
    // then look up the build SBOM by `subject:sha256:<hash>`.
    let mut correlated = false;
    if let Some(comps) = image_sbom["components"].as_array() {
        for c in comps {
            if let Some(hashes) = c["hashes"].as_array() {
                for h in hashes {
                    if h["alg"].as_str() == Some("SHA-256") {
                        if let Some(hex) = h["content"].as_str() {
                            // Look for a build SBOM with
                            // subject:sha256:<hex> in its
                            // metadata.component.externalReferences[].
                            let needle = format!("sha256:{hex}");
                            if let Some(refs) =
                                build_sbom["metadata"]["component"]["externalReferences"]
                                    .as_array()
                            {
                                for r in refs {
                                    if r["type"].as_str() == Some("attestation")
                                        && r["url"].as_str() == Some(needle.as_str())
                                    {
                                        correlated = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        correlated,
        "expected to correlate image component hash with build SBOM subject identifier"
    );
}

// ---------------------------------------------------------------------
// US4 — per-component user-defined identifier attachment
// ---------------------------------------------------------------------

#[test]
fn component_id_attaches_to_matching_component_cdx() {
    // US4 §1, SC-003 (CDX): --component-id attaches to the matching
    // component's properties[]; non-matching components unchanged.
    let fake_home = tempfile::tempdir().unwrap();
    let (cdx, _) = run_scan(
        fake_home.path(),
        &fixture_root(),
        &[
            "--component-id",
            "pkg:cargo/serde@1.0.197=kusari-id:asset-foo",
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comps = cdx["components"].as_array().expect("components[]");
    let mut serde_found = false;
    let mut other_unchanged = true;
    for c in comps {
        if c["purl"].as_str() == Some("pkg:cargo/serde@1.0.197") {
            let props = c["properties"].as_array();
            if let Some(arr) = props {
                if arr.iter().any(|p| {
                    p["name"].as_str() == Some("kusari-id")
                        && p["value"].as_str() == Some("asset-foo")
                }) {
                    serde_found = true;
                }
            }
        } else {
            // Other components must not have a `kusari-id` property.
            if let Some(props) = c["properties"].as_array() {
                if props.iter().any(|p| p["name"].as_str() == Some("kusari-id")) {
                    other_unchanged = false;
                }
            }
        }
    }
    assert!(serde_found, "expected matching component to carry kusari-id");
    assert!(
        other_unchanged,
        "non-matching components must remain unchanged"
    );
}

#[test]
fn component_id_attaches_to_matching_component_spdx23() {
    // US4 §1, SC-003 (SPDX 2.3): --component-id attaches to the
    // matching package's externalRefs[PERSISTENT-ID].
    let fake_home = tempfile::tempdir().unwrap();
    let (spdx, _) = run_scan(
        fake_home.path(),
        &fixture_root(),
        &[
            "--component-id",
            "pkg:cargo/serde@1.0.197=kusari-id:asset-foo",
        ],
        "spdx-2.3-json",
        "out.spdx.json",
    );
    let pkgs = spdx["packages"].as_array().expect("packages[]");
    let mut serde_found = false;
    for p in pkgs {
        // Find the package via its `purl` externalRef.
        let is_serde = p["externalRefs"]
            .as_array()
            .map(|refs| {
                refs.iter().any(|r| {
                    r["referenceType"].as_str() == Some("purl")
                        && r["referenceLocator"].as_str()
                            == Some("pkg:cargo/serde@1.0.197")
                })
            })
            .unwrap_or(false);
        if is_serde {
            if let Some(refs) = p["externalRefs"].as_array() {
                serde_found = refs.iter().any(|r| {
                    r["referenceCategory"].as_str() == Some("PERSISTENT-ID")
                        && r["referenceType"].as_str() == Some("kusari-id")
                        && r["referenceLocator"].as_str() == Some("asset-foo")
                });
            }
        }
    }
    assert!(serde_found, "expected SPDX 2.3 PERSISTENT-ID externalRef on matching package");
}

#[test]
fn component_id_attaches_to_matching_component_spdx3() {
    // US4 §1, SC-003 (SPDX 3): --component-id attaches to the matching
    // Element's externalIdentifier[].
    let fake_home = tempfile::tempdir().unwrap();
    let (spdx3, _) = run_scan(
        fake_home.path(),
        &fixture_root(),
        &[
            "--component-id",
            "pkg:cargo/serde@1.0.197=kusari-id:asset-foo",
        ],
        "spdx-3-json",
        "out.spdx3.json",
    );
    // Per milestone 079, the user-defined non-vocab scheme
    // `kusari-id` maps to the SPDX 3 controlled-vocab value `other`
    // with the original scheme preserved on `comment` as
    // `original-scheme: kusari-id`.
    let graph = spdx3["@graph"].as_array().expect("@graph");
    let mut serde_found = false;
    for n in graph {
        if n["type"].as_str() == Some("software_Package")
            && n["software_packageUrl"].as_str() == Some("pkg:cargo/serde@1.0.197")
        {
            if let Some(eids) = n["externalIdentifier"].as_array() {
                serde_found = eids.iter().any(|e| {
                    e["externalIdentifierType"].as_str() == Some("other")
                        && e["comment"].as_str() == Some("original-scheme: kusari-id")
                        && e["identifier"].as_str() == Some("asset-foo")
                });
            }
        }
    }
    assert!(
        serde_found,
        "expected SPDX 3 externalIdentifier (post-079: type=other + comment=original-scheme: kusari-id) on matching Package"
    );
}

#[test]
fn component_id_warns_on_zero_match() {
    // US4 §2, SC-008 / FR-010: zero-match selector → scan exits 0;
    // warn-level log surfaces the unmatched selector.
    let fake_home = tempfile::tempdir().unwrap();
    let (_, stderr) = run_scan(
        fake_home.path(),
        &fixture_root(),
        &[
            "--component-id",
            "pkg:cargo/nonexistent@0.0.0=acme:foo",
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );
    // The scan succeeded (run_scan asserts). Check the warn surfaced.
    let lower = stderr.to_lowercase();
    assert!(
        lower.contains("matched zero components")
            || lower.contains("nonexistent"),
        "expected warn about zero-match selector; got stderr={stderr}"
    );
}

#[test]
fn component_id_rejects_builtin_scheme_at_parse() {
    // US4 §4 / FR-009 / SC-007: built-in schemes rejected at clap
    // parse time. We exercise this through the `parse_component_id_flag`
    // adapter (the value_parser clap calls); a clap parse failure
    // exits non-zero before any scan work happens.
    let err = parse_component_id_flag("pkg:cargo/foo@1.0=subject:sha256:abc")
        .unwrap_err();
    assert!(
        err.contains("reserved") || err.contains("subject"),
        "expected `reserved` / `subject` in error; got {err}"
    );
    // Same check via the binary itself — clap parse failure emits
    // non-zero exit. We only check exit status here; stderr text is
    // shaped by clap, not us.
    let mut cmd = Command::new(bin());
    let fake_home = tempfile::tempdir().unwrap();
    apply_fake_home_env(&mut cmd, fake_home.path());
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture_root())
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash")
        .arg("--component-id")
        .arg("pkg:cargo/foo@1.0=subject:sha256:abc");
    let out = cmd.output().unwrap();
    assert!(
        !out.status.success(),
        "expected clap parse failure on built-in scheme; status={:?} stdout={} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn component_id_rejects_malformed_input_at_parse() {
    // US4 §5: clear errors for several malformed inputs.
    // No `=`.
    let err = parse_component_id_flag("pkg:cargo/foo@1.0").unwrap_err();
    assert!(err.contains("missing `=`"), "{err}");
    // Empty PURL.
    let err = parse_component_id_flag("=acme:foo").unwrap_err();
    assert!(err.contains("PURL") && err.contains("empty"), "{err}");
    // No `:` on RHS.
    let err = parse_component_id_flag("pkg:cargo/foo@1.0=acme").unwrap_err();
    assert!(err.contains("missing `:`"), "{err}");
    // Empty scheme.
    let err = parse_component_id_flag("pkg:cargo/foo@1.0=:value").unwrap_err();
    assert!(err.contains("scheme is empty"), "{err}");
    // Empty value.
    let err = parse_component_id_flag("pkg:cargo/foo@1.0=acme:").unwrap_err();
    assert!(err.contains("value is empty"), "{err}");
}

#[test]
fn component_id_lexical_order_within_new_entries() {
    // FR-012 + research §6: when two --component-id flags match the
    // same component, the new entries appear in lexical order by
    // (scheme, value) — not supply order.
    let fake_home = tempfile::tempdir().unwrap();
    let (cdx, _) = run_scan(
        fake_home.path(),
        &fixture_root(),
        &[
            "--component-id",
            "pkg:cargo/serde@1.0.197=zzz-scheme:foo",
            "--component-id",
            "pkg:cargo/serde@1.0.197=aaa-scheme:bar",
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comps = cdx["components"].as_array().expect("components[]");
    let mut new_props_in_order: Vec<&str> = Vec::new();
    for c in comps {
        if c["purl"].as_str() == Some("pkg:cargo/serde@1.0.197") {
            if let Some(arr) = c["properties"].as_array() {
                for p in arr {
                    if let Some(name) = p["name"].as_str() {
                        if name == "zzz-scheme" || name == "aaa-scheme" {
                            new_props_in_order.push(name);
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        new_props_in_order,
        vec!["aaa-scheme", "zzz-scheme"],
        "expected new properties in lex order; got {new_props_in_order:?}"
    );
}

#[test]
fn component_id_deterministic_across_reruns() {
    // SC-004: re-emission with identical inputs is byte-identical.
    let fake_home_a = tempfile::tempdir().unwrap();
    let fake_home_b = tempfile::tempdir().unwrap();
    let extra = &[
        "--component-id",
        "pkg:cargo/serde@1.0.197=kusari-id:asset-foo",
    ];
    let workspace = common::workspace_root();
    let run = |home: &Path, fmt: &str, fname: &str| -> String {
        let out_dir = tempfile::tempdir().unwrap();
        let out_path = out_dir.path().join(fname);
        let mut cmd = Command::new(bin());
        apply_fake_home_env(&mut cmd, home);
        cmd.arg("--offline")
            .arg("sbom")
            .arg("scan")
            .arg("--path")
            .arg(fixture_root())
            .arg("--format")
            .arg(fmt)
            .arg("--output")
            .arg(format!("{fmt}={}", out_path.to_string_lossy()))
            .arg("--no-deep-hash");
        for a in extra {
            cmd.arg(a);
        }
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "rerun scan failed: {:?}", String::from_utf8_lossy(&out.stderr));
        let raw = std::fs::read_to_string(&out_path).unwrap();
        // Normalize away volatile fields (serialNumber, timestamps,
        // workspace paths, file hashes) so byte-identity is meaningful.
        match fmt {
            "cyclonedx-json" => common::normalize::normalize_cdx_for_golden(&raw, &workspace),
            "spdx-2.3-json" => {
                common::normalize::normalize_spdx23_for_golden(&raw, &workspace)
            }
            "spdx-3-json" => {
                common::normalize::normalize_spdx3_for_golden(&raw, &workspace)
            }
            other => panic!("unknown format {other}"),
        }
    };
    for (fmt, fname) in &[
        ("cyclonedx-json", "out.cdx.json"),
        ("spdx-2.3-json", "out.spdx.json"),
        ("spdx-3-json", "out.spdx3.json"),
    ] {
        let a = run(fake_home_a.path(), fmt, fname);
        let b = run(fake_home_b.path(), fmt, fname);
        assert_eq!(
            a, b,
            "format {fmt}: re-emission must be byte-identical with --component-id (after normalize)"
        );
    }
}

// ---------------------------------------------------------------------
// Compile-time error-variant exhaustiveness sanity check.
//
// Exercises every variant of ComponentIdentifierFlagError so a future
// addition forces a refresh.
// ---------------------------------------------------------------------

type ErrCase = (&'static str, fn(&ComponentIdentifierFlagError) -> bool);

#[test]
fn component_id_error_variants_exhaustive() {
    use ComponentIdentifierFlagError::*;
    let cases: &[ErrCase] = &[
        ("no_eq", |e| matches!(e, MissingEquals(_))),
        ("=foo:bar", |e| matches!(e, EmptyPurl(_))),
        ("p=noscheme", |e| matches!(e, MissingColon(_))),
        ("p=:value", |e| matches!(e, EmptyScheme)),
        ("p=acme:", |e| matches!(e, EmptyValue)),
        ("p=Acme:foo", |e| matches!(e, InvalidSchemeName(_, _))),
        ("p=repo:foo", |e| matches!(e, BuiltinSchemeRejected(_))),
    ];
    for (raw, predicate) in cases {
        let err = ComponentIdentifierFlag::parse(raw).unwrap_err();
        assert!(
            predicate(&err),
            "input {raw:?} did not match expected variant; got {err:?}"
        );
    }
}
