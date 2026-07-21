//! Milestone 187 (#542 + #543) — ipk reader Yocto fixes integration tests.
//!
//! US1 (#543) — ar-format is now the PRIMARY parse path; mikebom extracts
//! `License:` / `Depends:` / `Architecture:` / other control fields for every
//! modern Yocto ipk, emitting `mikebom:source-mechanism =
//! "ipk-file-archive-extraction"` and `mikebom:arch-source = "control-file"`.
//! Pre-2015 `gzip(tar)` ipks are still parsed via the legacy path with
//! `mikebom:source-mechanism = "ipk-file"` and NO arch-source property
//! (FR-014 / SC-005 byte-identity guarantee).
//!
//! US2 (#542) — the filename-fallback path consults the parent-directory
//! name for the authoritative `?arch=` source, correctly handling Yocto arches
//! with `_` (`qemux86_64`, `powerpc_e500v2`, etc.).
//!
//! Test fixtures are fabricated at test time via `tar` + `flate2` — no
//! vendored fixtures needed.

use std::io::Write;
use std::process::Command;

// ─────────────────────────────────────────────────────────────────
// ar + tar fixture builders
// ─────────────────────────────────────────────────────────────────

/// Build a BSD ar 60-byte member header for `name` + `size`.
fn ar_header(name: &str, size: u64) -> Vec<u8> {
    let mut h = vec![b' '; 60];
    let name_bytes = name.as_bytes();
    h[..name_bytes.len()].copy_from_slice(name_bytes);
    let mtime = "0           ".as_bytes();
    h[16..28].copy_from_slice(mtime);
    let uid_gid_mode = "0     0     0       ";
    h[28..48].copy_from_slice(uid_gid_mode.as_bytes());
    let size_str = format!("{size:<10}");
    h[48..58].copy_from_slice(size_str.as_bytes());
    h[58..60].copy_from_slice(b"`\n");
    h
}

/// Assemble a full ar archive (magic + n members).
fn ar_archive(members: &[(&str, &[u8])]) -> Vec<u8> {
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

/// Build a `control.tar.gz` blob wrapping a single `./control` file whose
/// content is `control_body`.
fn build_control_tar_gz(control_body: &str) -> Vec<u8> {
    let uncompressed = {
        let mut builder = tar::Builder::new(Vec::<u8>::new());
        let mut header = tar::Header::new_gnu();
        header.set_path("./control").unwrap();
        header.set_size(control_body.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, control_body.as_bytes()).unwrap();
        builder.into_inner().unwrap()
    };
    let mut encoder =
        flate2::write::GzEncoder::new(Vec::<u8>::new(), flate2::Compression::default());
    encoder.write_all(&uncompressed).unwrap();
    encoder.finish().unwrap()
}

/// Build a `data.tar.gz` blob with a single stub file entry so the file-list
/// walk has content but downstream processing is trivially cheap.
fn build_data_tar_gz() -> Vec<u8> {
    let uncompressed = {
        let mut builder = tar::Builder::new(Vec::<u8>::new());
        let body = b"stub payload\n";
        let mut header = tar::Header::new_gnu();
        header.set_path("./usr/bin/stub").unwrap();
        header.set_size(body.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append(&header, body.as_ref()).unwrap();
        builder.into_inner().unwrap()
    };
    let mut encoder =
        flate2::write::GzEncoder::new(Vec::<u8>::new(), flate2::Compression::default());
    encoder.write_all(&uncompressed).unwrap();
    encoder.finish().unwrap()
}

/// Build a full modern ar-format ipk with debian-binary + control.tar.gz +
/// data.tar.gz members. Returns the bytes that should be written to
/// `<name>_<version>_<arch>.ipk`.
fn build_ar_ipk(control_body: &str) -> Vec<u8> {
    let control = build_control_tar_gz(control_body);
    let data = build_data_tar_gz();
    ar_archive(&[
        ("debian-binary", b"2.0\n"),
        ("control.tar.gz", &control),
        ("data.tar.gz", &data),
    ])
}

/// Build a pre-2015 gzip(tar)-outer ipk. Same 3 members, wrapped in a
/// gzipped-tar outer envelope (the OLD format).
fn build_legacy_gzip_tar_ipk(control_body: &str) -> Vec<u8> {
    let control = build_control_tar_gz(control_body);
    let data = build_data_tar_gz();
    // Outer tar containing the three sibling members.
    let outer_tar = {
        let mut builder = tar::Builder::new(Vec::<u8>::new());
        for (name, bytes) in &[
            ("debian-binary", &b"2.0\n"[..]),
            ("control.tar.gz", &control[..]),
            ("data.tar.gz", &data[..]),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_path(name).unwrap();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, *bytes).unwrap();
        }
        builder.into_inner().unwrap()
    };
    let mut encoder =
        flate2::write::GzEncoder::new(Vec::<u8>::new(), flate2::Compression::default());
    encoder.write_all(&outer_tar).unwrap();
    encoder.finish().unwrap()
}

fn mikebom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

/// Scan a directory and return the emitted CycloneDX 1.6 JSON as a
/// `serde_json::Value`.
fn scan_dir(scan_root: &std::path::Path) -> (serde_json::Value, String) {
    let tempdir = tempfile::tempdir().unwrap();
    let out = tempdir.path().join("out.cdx.json");
    let cmd_out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--path",
            scan_root.to_str().unwrap(),
            "--offline",
            "--format",
            "cyclonedx-json",
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("spawn mikebom binary");
    let stderr = String::from_utf8_lossy(&cmd_out.stderr).to_string();
    assert!(
        cmd_out.status.success(),
        "mikebom exit={:?} stderr:\n{stderr}",
        cmd_out.status.code(),
    );
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out).unwrap()).expect("output is valid JSON");
    (json, stderr)
}

/// Extract components matching a name from the CDX doc.
fn components_by_name<'a>(cdx: &'a serde_json::Value, name: &str) -> Vec<&'a serde_json::Value> {
    cdx.get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| c.get("name").and_then(|n| n.as_str()) == Some(name))
                .collect()
        })
        .unwrap_or_default()
}

/// Get a specific `mikebom:*` property value from a component.
fn get_property(component: &serde_json::Value, key: &str) -> Option<String> {
    component
        .get("properties")
        .and_then(|p| p.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(key))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()).map(String::from))
}

// ─────────────────────────────────────────────────────────────────
// US1 (#543) — ar-format primary path integration tests
// ─────────────────────────────────────────────────────────────────

#[test]
fn us1_ar_format_extracts_control_metadata() {
    let tempdir = tempfile::tempdir().unwrap();
    let arch_dir = tempdir.path().join("core2-64");
    std::fs::create_dir(&arch_dir).unwrap();
    let control = "Package: busybox\n\
                   Version: 1.36.1-r0\n\
                   Architecture: core2-64\n\
                   License: GPL-2.0-only & bzip2-1.0.4\n\
                   Depends: libc6 (>= 2.39), update-alternatives-opkg\n\
                   Maintainer: Poky Maintainers <poky@lists.yoctoproject.org>\n\
                   Description: Test busybox stub\n";
    let ipk = build_ar_ipk(control);
    std::fs::write(arch_dir.join("busybox_1.36.1-r0_core2-64.ipk"), &ipk).unwrap();

    // Also emit a stub libc6 ipk so busybox's Depends edge can
    // resolve (mikebom's edge emission graph-resolves — phantom edges
    // are dropped when the depended-on component isn't in the SBOM).
    let libc6_control = "Package: libc6\n\
                         Version: 2.39\n\
                         Architecture: core2-64\n\
                         License: LGPL-2.1-or-later\n\
                         Description: libc6 stub for m187 test\n";
    std::fs::write(
        arch_dir.join("libc6_2.39_core2-64.ipk"),
        build_ar_ipk(libc6_control),
    )
    .unwrap();

    let (cdx, _stderr) = scan_dir(tempdir.path());
    let comps = components_by_name(&cdx, "busybox");
    assert_eq!(comps.len(), 1, "expected exactly one busybox component");
    let comp = comps[0];

    let purl = comp.get("purl").and_then(|p| p.as_str()).unwrap();
    assert!(
        purl.contains("busybox@1.36.1-r0"),
        "PURL should carry name + version: {purl}"
    );
    assert!(
        purl.contains("arch=core2-64"),
        "PURL should carry ?arch=core2-64 from control file: {purl}"
    );

    let licenses = comp.get("licenses").and_then(|l| l.as_array()).unwrap();
    assert!(
        !licenses.is_empty(),
        "licenses[] MUST be non-empty (License field extracted). components={comp:#}"
    );

    assert_eq!(
        get_property(comp, "mikebom:source-mechanism").as_deref(),
        Some("ipk-file-archive-extraction"),
        "ar-format path MUST emit ipk-file-archive-extraction"
    );
    assert_eq!(
        get_property(comp, "mikebom:arch-source").as_deref(),
        Some("control-file"),
        "ar-format path MUST emit arch-source = control-file"
    );

    // Depends edges — busybox declares libc6 + update-alternatives-opkg.
    let deps = cdx.get("dependencies").and_then(|d| d.as_array()).unwrap();
    let busybox_ref = comp.get("bom-ref").and_then(|b| b.as_str()).unwrap();
    let busybox_dep = deps
        .iter()
        .find(|d| d.get("ref").and_then(|r| r.as_str()) == Some(busybox_ref))
        .expect("busybox dependency entry present");
    let depends_on = busybox_dep
        .get("dependsOn")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        depends_on.iter().any(|d| d.contains("libc6")),
        "Depends edges should include libc6; got {depends_on:?}"
    );
    // (Recommends handling deferred per FR-006 — no assertion.)
}

#[test]
fn us1_ar_format_tolerates_missing_debian_binary() {
    let tempdir = tempfile::tempdir().unwrap();
    let arch_dir = tempdir.path().join("all");
    std::fs::create_dir(&arch_dir).unwrap();
    let control = "Package: nodeb\n\
                   Version: 1.0\n\
                   Architecture: all\n\
                   License: MIT\n\
                   Description: no debian-binary member\n";
    let ctar = build_control_tar_gz(control);
    let dtar = build_data_tar_gz();
    let ipk = ar_archive(&[
        ("control.tar.gz", &ctar),
        ("data.tar.gz", &dtar),
    ]);
    std::fs::write(arch_dir.join("nodeb_1.0_all.ipk"), &ipk).unwrap();

    let (cdx, stderr) = scan_dir(tempdir.path());
    let comps = components_by_name(&cdx, "nodeb");
    assert_eq!(comps.len(), 1);
    let comp = comps[0];
    assert_eq!(
        get_property(comp, "mikebom:source-mechanism").as_deref(),
        Some("ipk-file-archive-extraction")
    );
    assert!(
        stderr.contains("missing debian-binary"),
        "stderr should contain WARN about missing debian-binary. stderr:\n{stderr}"
    );
}

#[test]
fn us1_pre_2015_gzip_tar_still_works() {
    let tempdir = tempfile::tempdir().unwrap();
    let arch_dir = tempdir.path().join("all");
    std::fs::create_dir(&arch_dir).unwrap();
    let control = "Package: legacy\n\
                   Version: 0.9\n\
                   Architecture: all\n\
                   License: Apache-2.0\n\
                   Description: pre-2015 gzip(tar) format\n";
    let ipk = build_legacy_gzip_tar_ipk(control);
    std::fs::write(arch_dir.join("legacy_0.9_all.ipk"), &ipk).unwrap();

    let (cdx, _stderr) = scan_dir(tempdir.path());
    let comps = components_by_name(&cdx, "legacy");
    assert_eq!(comps.len(), 1);
    let comp = comps[0];
    // Legacy path: source-mechanism = "ipk-file" (unchanged from pre-m187).
    assert_eq!(
        get_property(comp, "mikebom:source-mechanism").as_deref(),
        Some("ipk-file"),
        "legacy gzip-tar path MUST emit source-mechanism = ipk-file (byte-identity)"
    );
    // F9 byte-identity guarantee — legacy path does NOT emit arch-source.
    assert_eq!(
        get_property(comp, "mikebom:arch-source"),
        None,
        "legacy path MUST NOT emit arch-source property (SC-005 byte-identity)"
    );
    // License extracted normally.
    let licenses = comp.get("licenses").and_then(|l| l.as_array()).unwrap();
    assert!(!licenses.is_empty(), "legacy path should still extract licenses");
}

#[test]
fn us1_malformed_ar_falls_through_to_filename() {
    let tempdir = tempfile::tempdir().unwrap();
    let arch_dir = tempdir.path().join("all");
    std::fs::create_dir(&arch_dir).unwrap();
    // Ar magic + 20 bytes = truncated first header.
    let mut broken_ar = b"!<arch>\n".to_vec();
    broken_ar.extend_from_slice(&[b' '; 20]);
    std::fs::write(arch_dir.join("broken_1.0_all.ipk"), &broken_ar).unwrap();

    let (cdx, stderr) = scan_dir(tempdir.path());
    let comps = components_by_name(&cdx, "broken");
    assert_eq!(
        comps.len(),
        1,
        "filename fallback should produce exactly one component"
    );
    let comp = comps[0];
    assert_eq!(
        get_property(comp, "mikebom:source-mechanism").as_deref(),
        Some("ipk-file-filename-fallback")
    );
    assert!(
        stderr.contains("ar-format archive malformed")
            || stderr.contains("ar-format:"),
        "stderr should name ar-format failure reason (NOT the deleted `legacy ar-format` string). stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("legacy ar-format"),
        "the pre-m187 misleading `legacy ar-format` message MUST be gone. stderr:\n{stderr}"
    );
}

#[test]
fn us1_sc004_invariant_extraction_implies_license() {
    // SC-004 invariant: every ar-extracted component with a non-empty
    // control-file License MUST have non-empty licenses[]. An ipk with
    // empty License field MUST have empty licenses[].
    let tempdir = tempfile::tempdir().unwrap();
    let arch_dir = tempdir.path().join("all");
    std::fs::create_dir(&arch_dir).unwrap();

    let fixtures = [
        ("mit-pkg", "MIT"),
        ("apache-pkg", "Apache-2.0"),
        ("nolic-pkg", ""),
    ];
    for (name, license) in &fixtures {
        let control = format!(
            "Package: {name}\n\
             Version: 1.0\n\
             Architecture: all\n\
             License: {license}\n\
             Description: SC-004 invariant fixture\n"
        );
        let ipk = build_ar_ipk(&control);
        std::fs::write(arch_dir.join(format!("{name}_1.0_all.ipk")), &ipk).unwrap();
    }

    let (cdx, _stderr) = scan_dir(tempdir.path());
    for (name, license) in &fixtures {
        let comps = components_by_name(&cdx, name);
        assert_eq!(comps.len(), 1, "expected one component for {name}");
        let comp = comps[0];
        assert_eq!(
            get_property(comp, "mikebom:source-mechanism").as_deref(),
            Some("ipk-file-archive-extraction"),
            "{name} should take ar-extraction path"
        );
        let licenses = comp
            .get("licenses")
            .and_then(|l| l.as_array())
            .cloned()
            .unwrap_or_default();
        if license.is_empty() {
            assert!(
                licenses.is_empty(),
                "{name} has empty control-file License → licenses[] MUST be empty. got: {licenses:?}"
            );
        } else {
            assert!(
                !licenses.is_empty(),
                "{name} has non-empty control-file License={license} → licenses[] MUST be non-empty. component={comp:#}"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// US2 (#542) — parent-dir arch source integration tests
// ─────────────────────────────────────────────────────────────────

#[test]
fn us2_qemux86_64_arch_extracted_from_parent_dir() {
    // Malformed ar body → filename fallback. Parent dir is qemux86_64.
    // Verify ?arch=qemux86_64, version=1.0-r0 (no _qemux86 gluing),
    // mikebom:arch-source = parent-directory.
    let tempdir = tempfile::tempdir().unwrap();
    let arch_dir = tempdir.path().join("qemux86_64");
    std::fs::create_dir(&arch_dir).unwrap();
    let mut broken_ar = b"!<arch>\n".to_vec();
    broken_ar.extend_from_slice(&[b' '; 20]);
    std::fs::write(arch_dir.join("kernel_1.0-r0_qemux86_64.ipk"), &broken_ar).unwrap();

    let (cdx, _stderr) = scan_dir(tempdir.path());
    let comps = components_by_name(&cdx, "kernel");
    assert_eq!(comps.len(), 1);
    let comp = comps[0];

    let purl = comp.get("purl").and_then(|p| p.as_str()).unwrap();
    assert!(
        purl.contains("kernel@1.0-r0"),
        "PURL version MUST NOT contain `_qemux86` — got: {purl}"
    );
    assert!(
        purl.contains("arch=qemux86_64"),
        "PURL MUST carry ?arch=qemux86_64 from parent dir — got: {purl}"
    );
    assert!(
        !purl.contains("arch=64"),
        "PURL MUST NOT carry the pre-m187 buggy ?arch=64 — got: {purl}"
    );

    assert_eq!(
        get_property(comp, "mikebom:arch-source").as_deref(),
        Some("parent-directory")
    );
    assert_eq!(
        get_property(comp, "mikebom:source-mechanism").as_deref(),
        Some("ipk-file-filename-fallback")
    );
}

#[test]
fn us2_powerpc_e500v2_arch_extracted_from_parent_dir() {
    let tempdir = tempfile::tempdir().unwrap();
    let arch_dir = tempdir.path().join("powerpc_e500v2");
    std::fs::create_dir(&arch_dir).unwrap();
    let mut broken_ar = b"!<arch>\n".to_vec();
    broken_ar.extend_from_slice(&[b' '; 20]);
    std::fs::write(
        arch_dir.join("libfoo_2.5_powerpc_e500v2.ipk"),
        &broken_ar,
    )
    .unwrap();

    let (cdx, _stderr) = scan_dir(tempdir.path());
    let comps = components_by_name(&cdx, "libfoo");
    assert_eq!(comps.len(), 1);
    let comp = comps[0];
    let purl = comp.get("purl").and_then(|p| p.as_str()).unwrap();
    assert!(purl.contains("libfoo@2.5"), "PURL version: {purl}");
    assert!(
        purl.contains("arch=powerpc_e500v2"),
        "multi-underscore arch MUST be preserved verbatim — got: {purl}"
    );
    assert_eq!(
        get_property(comp, "mikebom:arch-source").as_deref(),
        Some("parent-directory")
    );
}

#[test]
fn us2_no_parent_dir_match_falls_back_to_filename_heuristic() {
    // Parent dir "downloads" doesn't match filename's arch suffix "_all"
    // → filename rsplit heuristic → mikebom:arch-source = filename-heuristic.
    let tempdir = tempfile::tempdir().unwrap();
    let loose_dir = tempdir.path().join("downloads");
    std::fs::create_dir(&loose_dir).unwrap();
    let mut broken_ar = b"!<arch>\n".to_vec();
    broken_ar.extend_from_slice(&[b' '; 20]);
    std::fs::write(loose_dir.join("stray_1.0_all.ipk"), &broken_ar).unwrap();

    let (cdx, _stderr) = scan_dir(tempdir.path());
    let comps = components_by_name(&cdx, "stray");
    assert_eq!(comps.len(), 1);
    let comp = comps[0];
    let purl = comp.get("purl").and_then(|p| p.as_str()).unwrap();
    assert!(
        purl.contains("arch=all"),
        "filename rsplit heuristic should emit ?arch=all — got: {purl}"
    );
    assert_eq!(
        get_property(comp, "mikebom:arch-source").as_deref(),
        Some("filename-heuristic")
    );
}

#[test]
fn us2_arch_source_control_file_when_ar_succeeds() {
    // Well-formed ar with control-file Architecture = qemux86_64
    // inside a directory named `wrongname/` — control-file wins per
    // FR-005. Emits mikebom:arch-source = "control-file", NOT
    // "parent-directory".
    let tempdir = tempfile::tempdir().unwrap();
    let wrong_dir = tempdir.path().join("wrongname");
    std::fs::create_dir(&wrong_dir).unwrap();
    let control = "Package: authoritative\n\
                   Version: 3.0\n\
                   Architecture: qemux86_64\n\
                   License: MIT\n\
                   Description: control file wins over parent dir\n";
    let ipk = build_ar_ipk(control);
    std::fs::write(
        wrong_dir.join("authoritative_3.0_wrongname.ipk"),
        &ipk,
    )
    .unwrap();

    let (cdx, _stderr) = scan_dir(tempdir.path());
    let comps = components_by_name(&cdx, "authoritative");
    assert_eq!(comps.len(), 1);
    let comp = comps[0];
    let purl = comp.get("purl").and_then(|p| p.as_str()).unwrap();
    assert!(
        purl.contains("arch=qemux86_64"),
        "control-file Architecture MUST win over parent dir — got: {purl}"
    );
    assert_eq!(
        get_property(comp, "mikebom:arch-source").as_deref(),
        Some("control-file")
    );
    assert_eq!(
        get_property(comp, "mikebom:source-mechanism").as_deref(),
        Some("ipk-file-archive-extraction")
    );
}

#[test]
fn regression_us1_us2_combined_yocto_scan() {
    // Combined regression: (a) ar+qemux86_64 (US1 ar-path), (b)
    // ar+core2-64 (US1 ar-path), (c) legacy gzip-tar+core2-64 (legacy
    // path). Verify all three take their expected code paths + emit
    // correct arch + source-mechanism values.
    let tempdir = tempfile::tempdir().unwrap();
    let qemu_dir = tempdir.path().join("qemux86_64");
    let core_dir = tempdir.path().join("core2-64");
    std::fs::create_dir(&qemu_dir).unwrap();
    std::fs::create_dir(&core_dir).unwrap();

    // (a) ar-format + qemux86_64.
    let ctl_a = "Package: pkg-a\n\
                 Version: 1.0\n\
                 Architecture: qemux86_64\n\
                 License: MIT\n\
                 Description: a\n";
    std::fs::write(qemu_dir.join("pkg-a_1.0_qemux86_64.ipk"), build_ar_ipk(ctl_a)).unwrap();

    // (b) ar-format + core2-64.
    let ctl_b = "Package: pkg-b\n\
                 Version: 2.0\n\
                 Architecture: core2-64\n\
                 License: Apache-2.0\n\
                 Description: b\n";
    std::fs::write(core_dir.join("pkg-b_2.0_core2-64.ipk"), build_ar_ipk(ctl_b)).unwrap();

    // (c) legacy gzip-tar + core2-64.
    let ctl_c = "Package: pkg-c\n\
                 Version: 3.0\n\
                 Architecture: core2-64\n\
                 License: BSD-3-Clause\n\
                 Description: c\n";
    std::fs::write(
        core_dir.join("pkg-c_3.0_core2-64.ipk"),
        build_legacy_gzip_tar_ipk(ctl_c),
    )
    .unwrap();

    let (cdx, _stderr) = scan_dir(tempdir.path());

    let a = components_by_name(&cdx, "pkg-a");
    let b = components_by_name(&cdx, "pkg-b");
    let c = components_by_name(&cdx, "pkg-c");
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    assert_eq!(c.len(), 1);

    // (a) + (b) — ar path.
    for comp in [a[0], b[0]] {
        assert_eq!(
            get_property(comp, "mikebom:source-mechanism").as_deref(),
            Some("ipk-file-archive-extraction")
        );
        assert_eq!(
            get_property(comp, "mikebom:arch-source").as_deref(),
            Some("control-file")
        );
    }
    // (c) — legacy path, NO arch-source.
    assert_eq!(
        get_property(c[0], "mikebom:source-mechanism").as_deref(),
        Some("ipk-file")
    );
    assert_eq!(get_property(c[0], "mikebom:arch-source"), None);
}
