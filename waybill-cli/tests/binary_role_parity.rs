//! Milestone 104 — cross-format binary role typing parity.
//!
//! US1 / US2: scans a directory containing synthetic Mach-O and ELF
//! binaries representing each `BinaryRole` variant, then asserts that
//! every emitted format (CDX 1.6 + SPDX 2.3 + SPDX 3) carries the
//! role-equivalent value in its native component-type field.
//!
//! Synthetic-binary bytes are hand-built rather than vendored so the
//! fixture cost is zero KB on disk and the test is host-portable
//! (host architecture doesn't matter — we're constructing the file
//! headers ourselves, not invoking a compiler).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

/// Mach-O 64-bit little-endian header padded to 1024 bytes. The
/// binary reader has a `MIN_BINARY_SIZE_BYTES = 1024` floor below
/// which files aren't scanned — synthetic fixtures need to clear it.
/// Only the filetype byte is meaningful for role classification;
/// everything else is zeroed.
fn mk_macho64(filetype: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(1024);
    out.extend_from_slice(&0xfeedfacf_u32.to_le_bytes()); // MH_MAGIC_64
    out.extend_from_slice(&0x01000007_u32.to_le_bytes()); // CPU_TYPE_X86_64
    out.extend_from_slice(&3_u32.to_le_bytes()); // cpusubtype
    out.extend_from_slice(&filetype.to_le_bytes()); // filetype
    out.extend_from_slice(&0_u32.to_le_bytes()); // ncmds
    out.extend_from_slice(&0_u32.to_le_bytes()); // sizeofcmds
    out.extend_from_slice(&0_u32.to_le_bytes()); // flags
    out.extend_from_slice(&0_u32.to_le_bytes()); // reserved
    out.resize(1024, 0);
    out
}

/// ELF64 little-endian header padded to 1024 bytes (matches the
/// binary reader's `MIN_BINARY_SIZE_BYTES` floor). Sets `e_type` at
/// offset 16 — the rest is the standard skeleton enough for
/// `object::read::File::parse` to accept it.
fn mk_elf64(e_type: u16) -> Vec<u8> {
    let mut out = vec![0u8; 1024];
    out[0] = 0x7f;
    out[1] = b'E';
    out[2] = b'L';
    out[3] = b'F';
    out[4] = 2; // ELFCLASS64
    out[5] = 1; // ELFDATA2LSB
    out[6] = 1; // EV_CURRENT
    out[16] = (e_type & 0xff) as u8;
    out[17] = ((e_type >> 8) & 0xff) as u8;
    out[18] = 62; // EM_X86_64
    out[20] = 1; // e_version
    out[52] = 64; // e_ehsize
    out[54] = 56; // e_phentsize
    out[58] = 64; // e_shentsize
    out
}

fn scan_to_json(
    fake_home: &std::path::Path,
    scan_target: &std::path::Path,
    out_format: &str,
    out_filename: &str,
) -> serde_json::Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join(out_filename);
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target)
        .arg("--format")
        .arg(out_format)
        .arg("--output")
        .arg(format!("{out_format}={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash");
    let out = cmd.output().expect("scan runs");
    assert!(
        out.status.success(),
        "scan failed: stderr={}\nstdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    drop(out_dir);
    parsed
}

#[test]
fn cdx_types_executables_as_application_and_dylibs_as_library() {
    // US1 acceptance: scan a directory containing one Mach-O exec
    // and one Mach-O dylib. CDX components[] reports `type:
    // application` for the exec, `type: library` for the dylib.
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = tempfile::tempdir().unwrap();
    std::fs::write(
        scan_target.path().join("my-exec"),
        mk_macho64(object::macho::MH_EXECUTE),
    )
    .unwrap();
    std::fs::write(
        scan_target.path().join("libthing.dylib"),
        mk_macho64(object::macho::MH_DYLIB),
    )
    .unwrap();

    let cdx = scan_to_json(
        fake_home.path(),
        scan_target.path(),
        "cyclonedx-json",
        "out.cdx.json",
    );
    let components = cdx["components"].as_array().expect("components[]");
    let types: std::collections::BTreeMap<String, String> = components
        .iter()
        .filter_map(|c| {
            let name = c["name"].as_str()?.to_string();
            let ty = c["type"].as_str()?.to_string();
            Some((name, ty))
        })
        .collect();
    assert_eq!(
        types.get("my-exec").map(String::as_str),
        Some("application"),
        "executable should emit CDX type=application; got types={types:#?}"
    );
    assert_eq!(
        types.get("libthing.dylib").map(String::as_str),
        Some("library"),
        "dylib should emit CDX type=library; got types={types:#?}"
    );
}

#[test]
fn cdx_types_elf_executable_as_application() {
    // US1 acceptance scenario 3: ELF executable → CDX type=application.
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = tempfile::tempdir().unwrap();
    std::fs::write(scan_target.path().join("elf-exec"), mk_elf64(object::elf::ET_EXEC)).unwrap();

    let cdx = scan_to_json(
        fake_home.path(),
        scan_target.path(),
        "cyclonedx-json",
        "out.cdx.json",
    );
    let exec = cdx["components"]
        .as_array()
        .expect("components[]")
        .iter()
        .find(|c| c["name"].as_str() == Some("elf-exec"))
        .expect("elf-exec component present");
    assert_eq!(
        exec["type"].as_str(),
        Some("application"),
        "ELF executable should emit CDX type=application; got {exec:#?}"
    );
}

#[test]
fn spdx23_primary_package_purpose_matches_role() {
    // US2 acceptance: SPDX 2.3 Package.primaryPackagePurpose carries
    // APPLICATION for executables, LIBRARY for shared libraries.
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = tempfile::tempdir().unwrap();
    std::fs::write(
        scan_target.path().join("my-exec"),
        mk_macho64(object::macho::MH_EXECUTE),
    )
    .unwrap();
    std::fs::write(
        scan_target.path().join("libthing.dylib"),
        mk_macho64(object::macho::MH_DYLIB),
    )
    .unwrap();
    let spdx = scan_to_json(
        fake_home.path(),
        scan_target.path(),
        "spdx-2.3-json",
        "out.spdx.json",
    );
    let purposes: std::collections::BTreeMap<String, String> = spdx["packages"]
        .as_array()
        .expect("packages[]")
        .iter()
        .filter_map(|p| {
            let name = p["name"].as_str()?.to_string();
            let purpose = p
                .get("primaryPackagePurpose")
                .and_then(|v| v.as_str())
                .unwrap_or("(absent)")
                .to_string();
            Some((name, purpose))
        })
        .collect();
    assert_eq!(
        purposes.get("my-exec").map(String::as_str),
        Some("APPLICATION"),
        "Mach-O executable → SPDX 2.3 primaryPackagePurpose=APPLICATION; got {purposes:#?}"
    );
    assert_eq!(
        purposes.get("libthing.dylib").map(String::as_str),
        Some("LIBRARY"),
        "Mach-O dylib → SPDX 2.3 primaryPackagePurpose=LIBRARY; got {purposes:#?}"
    );
}

#[test]
fn spdx3_software_primary_purpose_matches_role() {
    // US2 acceptance: SPDX 3 software_Package.software_primaryPurpose
    // carries "application" for executables, "library" for shared
    // libraries.
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = tempfile::tempdir().unwrap();
    std::fs::write(
        scan_target.path().join("my-exec"),
        mk_macho64(object::macho::MH_EXECUTE),
    )
    .unwrap();
    std::fs::write(
        scan_target.path().join("libthing.dylib"),
        mk_macho64(object::macho::MH_DYLIB),
    )
    .unwrap();
    let spdx3 = scan_to_json(
        fake_home.path(),
        scan_target.path(),
        "spdx-3-json",
        "out.spdx3.json",
    );
    let graph = spdx3["@graph"].as_array().expect("@graph");
    let purposes: std::collections::BTreeMap<String, String> = graph
        .iter()
        .filter(|el| el["type"].as_str() == Some("software_Package"))
        .filter_map(|el| {
            let name = el["name"].as_str()?.to_string();
            let purpose = el
                .get("software_primaryPurpose")
                .and_then(|v| v.as_str())
                .unwrap_or("(absent)")
                .to_string();
            Some((name, purpose))
        })
        .collect();
    assert_eq!(
        purposes.get("my-exec").map(String::as_str),
        Some("application"),
        "Mach-O executable → SPDX 3 software_primaryPurpose=application; got {purposes:#?}"
    );
    assert_eq!(
        purposes.get("libthing.dylib").map(String::as_str),
        Some("library"),
        "Mach-O dylib → SPDX 3 software_primaryPurpose=library; got {purposes:#?}"
    );
}

#[test]
fn cross_format_role_typing_agrees() {
    // US2 acceptance + FR-008: for every binary component, the role
    // value normalized from CDX type equals the role normalized from
    // SPDX 2.3 primaryPackagePurpose equals the role normalized from
    // SPDX 3 software_primaryPurpose.
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = tempfile::tempdir().unwrap();
    std::fs::write(
        scan_target.path().join("my-exec"),
        mk_macho64(object::macho::MH_EXECUTE),
    )
    .unwrap();
    std::fs::write(
        scan_target.path().join("libthing.dylib"),
        mk_macho64(object::macho::MH_DYLIB),
    )
    .unwrap();

    let cdx = scan_to_json(fake_home.path(), scan_target.path(), "cyclonedx-json", "out.cdx.json");
    let spdx = scan_to_json(fake_home.path(), scan_target.path(), "spdx-2.3-json", "out.spdx.json");
    let spdx3 = scan_to_json(fake_home.path(), scan_target.path(), "spdx-3-json", "out.spdx3.json");

    fn cdx_role(cdx_type: &str) -> &'static str {
        match cdx_type {
            "application" => "application",
            "library" => "library",
            "file" => "file",
            _ => "other",
        }
    }
    fn spdx2_role(p: &str) -> &'static str {
        match p {
            "APPLICATION" => "application",
            "LIBRARY" => "library",
            "FILE" => "file",
            _ => "other",
        }
    }

    // Build the role map per format
    let cdx_roles: std::collections::BTreeMap<String, &'static str> = cdx["components"]
        .as_array()
        .expect("components")
        .iter()
        .filter_map(|c| {
            Some((
                c["name"].as_str()?.to_string(),
                cdx_role(c["type"].as_str()?),
            ))
        })
        .collect();
    let spdx2_roles: std::collections::BTreeMap<String, &'static str> = spdx["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .filter_map(|p| {
            Some((
                p["name"].as_str()?.to_string(),
                spdx2_role(p.get("primaryPackagePurpose")?.as_str()?),
            ))
        })
        .collect();
    let spdx3_roles: std::collections::BTreeMap<String, String> = spdx3["@graph"]
        .as_array()
        .expect("@graph")
        .iter()
        .filter(|e| e["type"].as_str() == Some("software_Package"))
        .filter_map(|e| {
            Some((
                e["name"].as_str()?.to_string(),
                e.get("software_primaryPurpose")?.as_str()?.to_string(),
            ))
        })
        .collect();

    for name in ["my-exec", "libthing.dylib"] {
        let c: &str = cdx_roles
            .get(name)
            .unwrap_or_else(|| panic!("CDX missing {name}: cdx_roles={cdx_roles:#?}"));
        let s2: &str = spdx2_roles
            .get(name)
            .unwrap_or_else(|| panic!("SPDX2 missing {name}: spdx2_roles={spdx2_roles:#?}"));
        let s3: &str = spdx3_roles
            .get(name)
            .map(String::as_str)
            .unwrap_or_else(|| panic!("SPDX3 missing {name}: spdx3_roles={spdx3_roles:#?}"));
        assert_eq!(
            c, s2,
            "cross-format role disagreement for {name}: CDX={c} SPDX2.3={s2}"
        );
        assert_eq!(
            c, s3,
            "cross-format role disagreement for {name}: CDX={c} SPDX3={s3}"
        );
    }
}
