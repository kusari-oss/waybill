//! Milestone 111 US1 — `--pkg-alias` end-to-end emission tests
//! (T012 + T022).
//!
//! Binding annotations only attach on `--image` scans (milestone-072
//! contract: source-tier SBOMs stay byte-identical). To exercise the
//! alias-rewrite path hermetically we synthesize a docker-save
//! tarball containing one unclaimed ELF binary at `usr/local/bin/baz`
//! (same pattern as `scan_image.rs`), which the binary walker emits
//! as `pkg:generic/baz?file-sha256=<sha>` — the alias LHS.
//!
//! Test A (T012): scanning with
//!   `--bind-to-source source-baz.cdx.json
//!    --pkg-alias "pkg:generic/baz?file-sha256=<sha>=pkg:cargo/baz@1.0.0"`
//! must look the RHS up in the source SBOM, copy its verified
//! binding, and stamp `alias_from` / `alias_to` (FR-001, FR-005).
//!
//! Test B (T022 / SC-004): the same scan with NO `--pkg-alias` must
//! produce the pre-feature envelope (`strength: unknown`,
//! `reason: source-not-found-in-bind-target`, no alias keys) and the
//! full normalized document must be byte-identical to the pinned
//! golden at `tests/fixtures/pkg_alias_binding/image-baz.cdx.json`.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::Digest as _;

mod common;
use common::normalize::{apply_fake_home_env, normalize_cdx_for_golden};
use common::{bin, workspace_root};

/// Path to an in-repo milestone-111 fixture under
/// `mikebom-cli/tests/fixtures/pkg_alias_binding/`.
fn alias_fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pkg_alias_binding")
        .join(rel)
}

/// Minimal parseable ELF64 little-endian header (no program/section
/// headers), padded past the walker's minimum-size gate. Identical
/// recipe to `scan_fs/binary/mod.rs::tests::minimal_elf64_bytes` —
/// fixed bytes so the emitted `file-sha256` PURL qualifier is
/// deterministic across hosts.
fn minimal_elf64_bytes() -> Vec<u8> {
    let mut b = vec![0u8; 2048];
    b[..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
    b[4] = 2; // ELFCLASS64
    b[5] = 1; // little-endian
    b[6] = 1; // EV_CURRENT
    b[16] = 2; // e_type = ET_EXEC
    b[18] = 0x3E; // e_machine = EM_X86_64
    b[20] = 1; // e_version
    b[52] = 64; // e_ehsize
    b
}

/// The canonical PURL the binary walker emits for the planted ELF —
/// also the alias LHS.
fn baz_generic_purl() -> String {
    let sha = sha2::Sha256::digest(minimal_elf64_bytes());
    format!("pkg:generic/baz?file-sha256={sha:x}")
}

/// Build a docker-save tarball whose single layer contains `files`.
/// Same shape as `scan_image.rs::build_synthetic_image`.
fn build_synthetic_image(files: &[(&str, Vec<u8>)]) -> PathBuf {
    let mut layer_bytes = Vec::new();
    {
        let mut layer_tar = tar::Builder::new(&mut layer_bytes);
        for (path, content) in files {
            let mut header = tar::Header::new_ustar();
            header.set_path(path).unwrap();
            header.set_size(content.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            layer_tar.append(&header, content.as_slice()).unwrap();
        }
        layer_tar.finish().unwrap();
    }

    let manifest = r#"[{"Config":"config.json","RepoTags":["mikebom-test:latest"],"Layers":["layer0/layer.tar"]}]"#;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();
    let file = tmp.reopen().unwrap();
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
    tmp.persist(&path).unwrap();
    path
}

/// Scan `tarball` as an image with `--bind-to-source` plus any
/// `extra_args`, returning the raw CDX JSON string.
fn scan_image_bound(tarball: &Path, extra_args: &[&str]) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home");
    let cdx_out = dir.path().join("image.cdx.json");
    let source = alias_fixture("source-baz.cdx.json");

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    let out = cmd
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--image")
        .arg(tarball)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_out.to_string_lossy()))
        .arg("--no-deep-hash")
        .arg("--bind-to-source")
        .arg(&source)
        .args(extra_args)
        .output()
        .expect("scan runs");
    assert!(
        out.status.success(),
        "image scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read_to_string(&cdx_out).expect("read emitted CDX")
}

/// Extract the parsed `mikebom:source-document-binding` envelope from
/// the component whose PURL starts with `pkg:generic/baz`.
fn baz_binding_envelope(raw: &str) -> serde_json::Value {
    let sbom: serde_json::Value = serde_json::from_str(raw).expect("valid JSON");
    let comp = sbom["components"]
        .as_array()
        .expect("components array")
        .iter()
        .find(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with("pkg:generic/baz"))
        })
        .unwrap_or_else(|| {
            panic!(
                "no pkg:generic/baz component emitted; purls = {:?}",
                sbom["components"]
                    .as_array()
                    .map(|a| a
                        .iter()
                        .filter_map(|c| c["purl"].as_str())
                        .collect::<Vec<_>>())
            )
        });
    let prop = comp["properties"]
        .as_array()
        .expect("properties array")
        .iter()
        .find(|p| p["name"].as_str() == Some("mikebom:source-document-binding"))
        .expect("binding property present");
    serde_json::from_str(prop["value"].as_str().expect("string value"))
        .expect("envelope is valid JSON")
}

/// T012 — aliased image scan binds the generic component to the
/// source-tier PURL's verified binding and stamps alias provenance.
#[test]
fn aliased_scan_binds_generic_component_to_source_purl() {
    let tarball =
        build_synthetic_image(&[("usr/local/bin/baz", minimal_elf64_bytes())]);
    let lhs = baz_generic_purl();
    let alias = format!("{lhs}=pkg:cargo/baz@1.0.0");

    let raw = scan_image_bound(&tarball, &["--pkg-alias", &alias]);
    let envelope = baz_binding_envelope(&raw);

    assert_eq!(
        envelope["alias_from"].as_str(),
        Some(lhs.as_str()),
        "alias_from must record the matched LHS (FR-005); envelope = {envelope}"
    );
    assert_eq!(
        envelope["alias_to"].as_str(),
        Some("pkg:cargo/baz@1.0.0"),
        "alias_to must record the RHS (FR-005); envelope = {envelope}"
    );
    assert_eq!(
        envelope["strength"].as_str(),
        Some("verified"),
        "alias lookup must inherit the source component's verified \
         binding, not unknown; envelope = {envelope}"
    );
    assert_ne!(
        envelope["reason"].as_str(),
        Some("source-not-found-in-bind-target"),
        "reason must not report a source miss when the alias RHS \
         resolved; envelope = {envelope}"
    );
    // The copied binding carries the source SBOM's binding hash.
    assert_eq!(
        envelope["hash"].as_str(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "hash must be copied from the source-tier binding; envelope = {envelope}"
    );
}

/// T022 / SC-004 — without `--pkg-alias` the scan emits the
/// pre-feature unknown envelope with NO alias keys, byte-identical
/// (post-normalization) to the pinned golden.
#[test]
fn no_alias_scan_is_byte_identical_to_pre_feature_golden() {
    let tarball =
        build_synthetic_image(&[("usr/local/bin/baz", minimal_elf64_bytes())]);

    let raw = scan_image_bound(&tarball, &[]);
    let envelope = baz_binding_envelope(&raw);

    assert_eq!(envelope["strength"].as_str(), Some("unknown"));
    assert_eq!(
        envelope["reason"].as_str(),
        Some("source-not-found-in-bind-target")
    );
    assert!(
        envelope.get("alias_from").is_none() && envelope.get("alias_to").is_none(),
        "pre-feature envelope shape must not grow alias keys when no \
         alias is declared (SC-004); envelope = {envelope}"
    );

    // Byte-identity against the pinned golden. Two per-run paths leak
    // into the document and must be masked before the standard
    // normalization pass: the tarball itself, and the per-scan
    // `mikebom-image-<random>` extraction dir that prefixes every
    // component source-path.
    let masked = raw.replace(
        tarball.to_string_lossy().as_ref(),
        "<IMAGE_TARBALL>",
    );
    let extract_dir_re = regex::Regex::new(r#"[^"]*mikebom-image-[^/"]+"#)
        .expect("static regex compiles");
    let masked = extract_dir_re.replace_all(&masked, "<IMAGE_EXTRACT_DIR>");
    let normalized = normalize_cdx_for_golden(&masked, &workspace_root());

    let golden_path = alias_fixture("image-baz.cdx.json");
    let update = std::env::var("MIKEBOM_UPDATE_CDX_GOLDENS")
        .ok()
        .map(|v| v == "1")
        .unwrap_or(false);
    if update || !golden_path.exists() {
        std::fs::write(&golden_path, normalized.as_bytes()).expect("write golden");
        eprintln!(
            "[pkg_alias_binding_us1] updated golden: {}",
            golden_path.display()
        );
        return;
    }
    let golden = std::fs::read_to_string(&golden_path)
        .expect("read pinned golden")
        .replace("\r\n", "\n");
    if golden != normalized {
        let actual = golden_path.with_extension("actual.json");
        std::fs::write(&actual, normalized.as_bytes()).expect("write actual");
        panic!(
            "no-alias image scan drifted from the pre-feature golden \
             (SC-004 byte-identity).\n  golden: {}\n  actual: {}\nTo \
             accept an intentional change, rerun with \
             MIKEBOM_UPDATE_CDX_GOLDENS=1",
            golden_path.display(),
            actual.display()
        );
    }
}
