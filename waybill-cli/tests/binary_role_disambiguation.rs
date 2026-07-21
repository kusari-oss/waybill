//! Milestone 104 US3 — edge-case disambiguation integration tests.
//!
//! Covers cases that the unit tests in `scan_fs/binary/role.rs`
//! can't reach (because they require the full scan pipeline rather
//! than a direct `classify` call):
//!
//! * ELF PIE executable (ET_DYN + PT_INTERP) — emitted as
//!   `application` via the role module's PIE disambiguation.
//! * Fat Mach-O — classified from the first slice's filetype per
//!   FR-006. The fat-container parse happens in
//!   `scan_fs::binary::scan::scan_fat_macho`, not in `classify`
//!   itself, so this needs to be tested at the integration level.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

/// ELF64 + 1024 padding + PT_INTERP program-header. Same layout
/// shape as `binary_role_parity.rs::mk_elf64` plus a phdr entry.
fn mk_elf64_pie() -> Vec<u8> {
    let mut out = vec![0u8; 1024];
    out[0] = 0x7f; out[1] = b'E'; out[2] = b'L'; out[3] = b'F';
    out[4] = 2; out[5] = 1; out[6] = 1;
    out[16] = object::elf::ET_DYN as u8;
    out[17] = (object::elf::ET_DYN >> 8) as u8;
    out[18] = 62; // EM_X86_64
    out[20] = 1; // e_version
    out[52] = 64; // e_ehsize
    out[32..40].copy_from_slice(&64_u64.to_le_bytes()); // e_phoff
    out[54] = 56; // e_phentsize
    out[56] = 1; // e_phnum
    out[58] = 64; // e_shentsize
    // phdr[0]: PT_INTERP at offset 64
    let pt_interp = object::elf::PT_INTERP;
    out[64..68].copy_from_slice(&pt_interp.to_le_bytes());
    out
}

/// ELF64 + 1024 padding + NO PT_INTERP. Same shape as the PIE
/// fixture but with `e_phnum = 0`.
fn mk_elf64_shared_lib() -> Vec<u8> {
    let mut out = vec![0u8; 1024];
    out[0] = 0x7f; out[1] = b'E'; out[2] = b'L'; out[3] = b'F';
    out[4] = 2; out[5] = 1; out[6] = 1;
    out[16] = object::elf::ET_DYN as u8;
    out[17] = (object::elf::ET_DYN >> 8) as u8;
    out[18] = 62;
    out[20] = 1;
    out[52] = 64;
    out[54] = 56;
    out[58] = 64;
    out
}

/// Mach-O 64-bit LE header padded to 1024 bytes.
fn mk_macho64(filetype: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(1024);
    out.extend_from_slice(&0xfeedfacf_u32.to_le_bytes());
    out.extend_from_slice(&0x01000007_u32.to_le_bytes());
    out.extend_from_slice(&3_u32.to_le_bytes());
    out.extend_from_slice(&filetype.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.resize(1024, 0);
    out
}

/// Fat Mach-O with 2 slices. First slice carries the filetype
/// `slice_0_type`; second slice is intentionally a dylib so that
/// any code path that picks slice 1 (instead of slice 0) emits
/// the wrong role and the test fails.
fn mk_fat_macho(slice_0_type: u32) -> Vec<u8> {
    let first = mk_macho64(slice_0_type);
    let second = mk_macho64(object::macho::MH_DYLIB);
    let header_size: u32 = 4 + 4 + 2 * 20; // fat magic + nfat + 2 fat_arch entries
    let first_off = header_size;
    let first_sz = first.len() as u32;
    let second_off = first_off + first_sz;
    let second_sz = second.len() as u32;
    let mut out = Vec::new();
    out.extend_from_slice(&0xcafebabe_u32.to_be_bytes()); // FAT_MAGIC (BE)
    out.extend_from_slice(&2_u32.to_be_bytes()); // nfat_arch
    // arch 0 — x86_64
    out.extend_from_slice(&0x01000007_u32.to_be_bytes()); // cputype
    out.extend_from_slice(&3_u32.to_be_bytes()); // cpusubtype
    out.extend_from_slice(&first_off.to_be_bytes());
    out.extend_from_slice(&first_sz.to_be_bytes());
    out.extend_from_slice(&0_u32.to_be_bytes()); // align
    // arch 1 — arm64
    out.extend_from_slice(&0x0100000c_u32.to_be_bytes()); // CPU_TYPE_ARM64
    out.extend_from_slice(&0_u32.to_be_bytes());
    out.extend_from_slice(&second_off.to_be_bytes());
    out.extend_from_slice(&second_sz.to_be_bytes());
    out.extend_from_slice(&0_u32.to_be_bytes());
    out.extend_from_slice(&first);
    out.extend_from_slice(&second);
    out
}

fn scan_cdx(fake_home: &std::path::Path, target: &std::path::Path) -> serde_json::Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(target)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash");
    let out = cmd.output().expect("scan runs");
    assert!(out.status.success(), "scan failed: stderr={}", String::from_utf8_lossy(&out.stderr));
    let bytes = std::fs::read(&out_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    drop(out_dir);
    parsed
}

fn type_for_name(cdx: &serde_json::Value, name: &str) -> Option<String> {
    cdx["components"]
        .as_array()?
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
        .and_then(|c| c["type"].as_str().map(String::from))
}

#[test]
fn elf_pie_and_shared_lib_disambiguate() {
    // US3 acceptance scenario 1+2: ET_DYN + PT_INTERP → Application;
    // ET_DYN without PT_INTERP → SharedLibrary.
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = tempfile::tempdir().unwrap();
    std::fs::write(scan_target.path().join("pie-exec"), mk_elf64_pie()).unwrap();
    std::fs::write(scan_target.path().join("libthing.so"), mk_elf64_shared_lib()).unwrap();
    let cdx = scan_cdx(fake_home.path(), scan_target.path());
    assert_eq!(
        type_for_name(&cdx, "pie-exec").as_deref(),
        Some("application"),
        "ELF PIE exec (ET_DYN+PT_INTERP) → CDX type=application"
    );
    assert_eq!(
        type_for_name(&cdx, "libthing.so").as_deref(),
        Some("library"),
        "ELF shared lib (ET_DYN no PT_INTERP) → CDX type=library"
    );
}

#[test]
fn fat_macho_first_slice_drives_classification() {
    // FR-006: universal/fat Mach-O classification is taken from the
    // first slice. Fixture's slice-0 is MH_EXECUTE (Application);
    // slice-1 is MH_DYLIB. If anything read slice-1 the result
    // would be `library`. Locking in first-slice behavior with a
    // direct end-to-end assertion.
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = tempfile::tempdir().unwrap();
    std::fs::write(scan_target.path().join("fat-exec"), mk_fat_macho(object::macho::MH_EXECUTE)).unwrap();
    let cdx = scan_cdx(fake_home.path(), scan_target.path());
    assert_eq!(
        type_for_name(&cdx, "fat-exec").as_deref(),
        Some("application"),
        "Fat Mach-O with MH_EXECUTE in slice 0 → CDX type=application (FR-006)"
    );
}

#[test]
fn macho_bundle_falls_back_to_library() {
    // US3 acceptance scenario 3: MH_BUNDLE → Other → CDX library
    // (historic default for unclassifiable binary-reader components).
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = tempfile::tempdir().unwrap();
    std::fs::write(
        scan_target.path().join("plugin.bundle"),
        mk_macho64(object::macho::MH_BUNDLE),
    )
    .unwrap();
    let cdx = scan_cdx(fake_home.path(), scan_target.path());
    assert_eq!(
        type_for_name(&cdx, "plugin.bundle").as_deref(),
        Some("library"),
        "Mach-O MH_BUNDLE → CDX type=library (Other-bucket fallback)"
    );
}
