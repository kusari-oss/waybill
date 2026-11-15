//! Spec 1531 (#1531) FR-014 / T043: an image scan reads these files too,
//! and skips the ones its dpkg database owns.

#![allow(clippy::unwrap_used)]

mod common;

use std::io::Write;

use common::installed_lib::{component, coverage, scan_with, skipped};

#[test]
fn an_image_scan_reads_unowned_files_and_skips_dpkg_owned_ones() {
    let owned = "usr/lib/x86_64-linux-gnu/pkgconfig/waybill-fixture-z.pc";
    let files: Vec<(String, Vec<u8>)> = vec![
        ("etc/os-release".into(), b"NAME=\"Debian\"\nID=debian\nVERSION_ID=\"12\"\n".to_vec()),
        (
            "var/lib/dpkg/status".into(),
            b"Package: waybill-fixture-z-dev\nStatus: install ok installed\nVersion: 1.0\nArchitecture: amd64\n\n".to_vec(),
        ),
        ("var/lib/dpkg/info/waybill-fixture-z-dev:amd64.list".into(), format!("/{owned}\n").into_bytes()),
        (owned.into(), b"Version: 6.0\n".to_vec()),
        ("opt/sdk/lib/pkgconfig/waybill-fixture-a.pc".into(), b"Version: 1.2.3\n".to_vec()),
    ];
    let mut layer_bytes = Vec::new();
    {
        let mut layer = tar::Builder::new(&mut layer_bytes);
        for (path, body) in &files {
            let mut h = tar::Header::new_ustar();
            h.set_path(path).unwrap();
            h.set_size(body.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            layer.append(&h, body.as_slice()).unwrap();
        }
        layer.finish().unwrap();
    }
    let manifest = r#"[{"Config":"config.json","RepoTags":["docker.io/test/waybill-fixture:v1"],"Layers":["layer0/layer.tar"]}]"#;
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

    let docs = scan_with("--image", &tarball, &[]);
    assert!(component(&docs.cdx, "pkg:generic/waybill-fixture-a@1.2.3").is_some(), "the unowned library is read");
    assert!(component(&docs.cdx, "pkg:generic/waybill-fixture-z@6.0").is_none(), "the dpkg-owned file is skipped");
    let cov = coverage(&docs).expect("C225 on an image scan");
    assert_eq!(skipped(&cov, "owned-by-os-package"), 1, "{cov}");
}
