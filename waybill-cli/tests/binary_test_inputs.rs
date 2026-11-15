//! Issue #1524: a directory scan does not report the binaries under a
//! test-input directory (`test`, `tests`, `unittests`, `Inputs`), and says
//! how many it skipped (C223). An image scan reports them, and
//! `--include-test-binaries` turns the skip off.
//!
//! The fixtures are hand-built: a minimal ELF64 header, and a Mach-O whose
//! one undefined symbol binds to an `LC_LOAD_DYLIB` install name, which is
//! the shape that produced `pkg:generic/%2Fusr%2Flib%2FlibSystem.B.dylib`
//! from test objects in #1524.

#![allow(clippy::unwrap_used)]

mod common;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::normalize::apply_fake_home_env;
use serde_json::Value;

const C223: &str = "waybill:binary-scan-skipped-test-inputs";

/// Minimal ELF64 little-endian header padded past the binary walker's
/// minimum size. `tag` makes the bytes, and so the hash, distinct per file:
/// identical bytes would let one file's hash cover another in the file tier.
fn elf(tag: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; 2048];
    b[..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
    b[4] = 2; // ELFCLASS64
    b[5] = 1; // little-endian
    b[6] = 1; // EV_CURRENT
    b[16] = 2; // ET_EXEC
    b[18] = 0x3E; // EM_X86_64
    b[20] = 1;
    b[52] = 64; // e_ehsize
    b[100..100 + tag.len()].copy_from_slice(tag);
    b
}

/// A 64-bit little-endian Mach-O executable that loads `dylib`: one
/// `LC_LOAD_DYLIB`, and an `LC_SYMTAB` + `LC_DYSYMTAB` holding one undefined
/// symbol bound to library ordinal 1, which is what the linkage reader
/// follows to the install name.
fn macho(dylib: &str, tag: &[u8]) -> Vec<u8> {
    let le = |v: u32| v.to_le_bytes();
    let mut name = dylib.as_bytes().to_vec();
    name.push(0);
    let dylib_size = (24 + name.len() as u32 + 7) & !7;
    let mut load_dylib = Vec::new();
    for v in [0x0c, dylib_size, 24, 2, 0x1_0000, 0x1_0000] {
        load_dylib.extend_from_slice(&le(v));
    }
    load_dylib.extend_from_slice(&name);
    load_dylib.resize(dylib_size as usize, 0);

    let sizeofcmds = dylib_size + 24 + 80;
    let symoff = (32 + sizeofcmds + 7) & !7;
    let mut nlist = Vec::new();
    nlist.extend_from_slice(&le(1)); // n_strx
    nlist.push(0x01); // N_UNDF | N_EXT
    nlist.push(0); // n_sect
    nlist.extend_from_slice(&(1u16 << 8).to_le_bytes()); // library ordinal 1
    nlist.extend_from_slice(&0u64.to_le_bytes());
    let strtab = b"\0_waybill_fixture_sym\0";
    let stroff = symoff + nlist.len() as u32;

    let mut b = Vec::new();
    // mach_header_64: MH_MAGIC_64, x86_64, MH_EXECUTE, 3 commands,
    // MH_NOUNDEFS | MH_DYLDLINK | MH_TWOLEVEL.
    for v in [0xfeed_facf, 0x0100_0007, 3, 2, 3, sizeofcmds, 0x85, 0] {
        b.extend_from_slice(&le(v));
    }
    b.extend_from_slice(&load_dylib);
    for v in [0x2, 24, symoff, 1, stroff, strtab.len() as u32] {
        b.extend_from_slice(&le(v));
    }
    // LC_DYSYMTAB: iundefsym 0, nundefsym 1.
    let mut dysymtab = [0u32; 20];
    dysymtab[0] = 0x0b;
    dysymtab[1] = 80;
    dysymtab[7] = 1;
    for v in dysymtab {
        b.extend_from_slice(&le(v));
    }
    b.resize(symoff as usize, 0);
    b.extend_from_slice(&nlist);
    b.extend_from_slice(strtab);
    b.resize(2048, 0);
    b[1500..1500 + tag.len()].copy_from_slice(tag);
    b
}

/// Write `files` (root-relative path, bytes) under `root`.
fn plant(root: &Path, files: &[(&str, Vec<u8>)]) {
    for (rel, bytes) in files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
}

/// A toolchain-shaped source tree. Four binaries sit under test-input
/// directories; two that only resemble them (`testing/`, lowercase
/// `inputs/`) and one product binary do not.
fn source_tree(root: &Path) {
    plant(
        root,
        &[
            ("bin/waybill-fixture-app", macho("/usr/lib/libwaybill-fixture-a.dylib", b"app")),
            (
                "test/Inputs/waybill-fixture-input.macho",
                macho("/usr/lib/libwaybill-fixture-b.dylib", b"input"),
            ),
            ("tests/waybill-fixture-core", elf(b"core")),
            ("llvm/unittests/x/waybill-fixture-obj", elf(b"obj")),
            ("lldb/test/API/waybill-fixture-api", elf(b"api")),
            ("testing/waybill-fixture-testing-tool", elf(b"testing")),
            ("docs/inputs/waybill-fixture-lower", elf(b"lower")),
        ],
    );
}

struct Docs {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
}

fn run_scan(target: &[&str], subject: &Path, extra: &[&str]) -> Docs {
    let out_dir = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    let (c, s2, s3) = (
        out_dir.path().join("o.cdx.json"),
        out_dir.path().join("o.spdx.json"),
        out_dir.path().join("o.spdx3.json"),
    );
    let mut cmd = Command::new(common::bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env_remove("WAYBILL_INCLUDE_TEST_BINARIES")
        .args(["--offline", "sbom", "scan"])
        .args(target)
        .arg(subject)
        .args(["--no-deep-hash", "--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", c.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", s2.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", s3.display()))
        .args(extra);
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "scan failed: {}", String::from_utf8_lossy(&out.stderr));
    let read = |p: &Path| serde_json::from_slice::<Value>(&std::fs::read(p).unwrap()).unwrap();
    Docs { cdx: read(&c), spdx23: read(&s2), spdx3: read(&s3) }
}

/// Every component name and PURL in the CycloneDX document, recursively,
/// so a binary nested under another component or emitted by the file tier
/// (which carries no PURL) is still seen.
fn names_and_purls(cdx: &Value) -> Vec<String> {
    fn walk(v: &Value, out: &mut Vec<String>) {
        for c in v.as_array().into_iter().flatten() {
            for key in ["name", "purl"] {
                if let Some(s) = c[key].as_str() {
                    out.push(s.to_string());
                }
            }
            walk(&c["components"], out);
        }
    }
    let mut out = Vec::new();
    walk(&cdx["components"], &mut out);
    out.sort();
    out
}

fn cdx_count(cdx: &Value) -> Option<String> {
    cdx["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == C223)
        .and_then(|p| p["value"].as_str().map(str::to_string))
}

/// C223 from an SPDX 2.3 or SPDX 3 document's annotations.
fn spdx_count(doc: &Value) -> Option<String> {
    doc["annotations"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(doc["@graph"].as_array().into_iter().flatten().filter(|e| e["type"] == "Annotation"))
        .find_map(|a| {
            let env = a["comment"].as_str().or_else(|| a["statement"].as_str())?;
            let env: Value = serde_json::from_str(env).ok()?;
            (env["field"] == C223).then(|| env["value"].as_str().map(str::to_string))?
        })
}

fn counts(d: &Docs) -> [Option<String>; 3] {
    [cdx_count(&d.cdx), spdx_count(&d.spdx23), spdx_count(&d.spdx3)]
}

fn has(names: &[String], needle: &str) -> bool {
    names.iter().any(|n| n.contains(needle))
}

const SKIPPED: [&str; 4] = [
    "waybill-fixture-input.macho",
    "waybill-fixture-core",
    "waybill-fixture-obj",
    "waybill-fixture-api",
];
const KEPT: [&str; 3] = [
    "waybill-fixture-app",
    "waybill-fixture-testing-tool",
    "waybill-fixture-lower",
];

/// The fix. Binaries under test-input directories are neither binary-tier
/// nor file-tier components, the load command of the Mach-O test object
/// contributes no dependency, and all three formats carry the count.
#[test]
fn directory_scan_skips_test_input_binaries_and_counts_them() {
    let tree = tempfile::tempdir().unwrap();
    source_tree(tree.path());
    let docs = run_scan(&["--path"], tree.path(), &[]);
    let names = names_and_purls(&docs.cdx);

    for n in SKIPPED {
        assert!(!has(&names, n), "{n} is a test input and must not be a component: {names:?}");
    }
    assert!(
        !has(&names, "libwaybill-fixture-b.dylib"),
        "a test object's load command is not a dependency: {names:?}"
    );
    // Positive control: the same kinds of binary outside a test-input
    // directory, and directories whose names merely resemble one.
    for n in KEPT {
        assert!(has(&names, n), "{n} is not under a test-input directory: {names:?}");
    }
    assert!(
        has(&names, "pkg:generic/%2Fusr%2Flib%2Flibwaybill-fixture-a.dylib"),
        "control: the product binary's load command is still read: {names:?}"
    );

    let want = Some(SKIPPED.len().to_string());
    assert_eq!(counts(&docs), [want.clone(), want.clone(), want], "C223 in CDX, SPDX 2.3, SPDX 3");
}

/// The opt-in. Everything is read, and with nothing skipped C223 is absent.
#[test]
fn include_test_binaries_reads_them_and_omits_the_count() {
    let tree = tempfile::tempdir().unwrap();
    source_tree(tree.path());
    let docs = run_scan(&["--path"], tree.path(), &["--include-test-binaries"]);
    let names = names_and_purls(&docs.cdx);

    for n in SKIPPED.iter().chain(KEPT.iter()) {
        assert!(has(&names, n), "{n} must be read under --include-test-binaries: {names:?}");
    }
    assert!(has(&names, "libwaybill-fixture-b.dylib"), "{names:?}");
    assert_eq!(counts(&docs), [None, None, None]);
}

/// A docker-save tarball whose one layer holds `files`; the shape
/// `pkg_alias_binding_us1.rs` builds.
fn build_synthetic_image(files: &[(&str, Vec<u8>)]) -> (tempfile::TempDir, PathBuf) {
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
    let manifest = r#"[{"Config":"config.json","RepoTags":["waybill-fixture:latest"],"Layers":["layer0/layer.tar"]}]"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.tar");
    let mut outer = tar::Builder::new(std::fs::File::create(&path).unwrap());
    for (name, bytes) in [("manifest.json", manifest.as_bytes()), ("layer0/layer.tar", &layer_bytes[..])] {
        let mut h = tar::Header::new_ustar();
        h.set_path(name).unwrap();
        h.set_size(bytes.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        outer.append(&h, bytes).unwrap();
    }
    outer.into_inner().unwrap().flush().unwrap();
    (dir, path)
}

/// An installed system's `tests/` is not a fixture: an image scan reads
/// the binary under it, and records no count.
#[test]
fn image_scan_reads_binaries_under_tests_directories() {
    let (_dir, image) = build_synthetic_image(&[
        ("usr/bin/waybill-fixture-tool", elf(b"tool")),
        ("usr/lib/waybill-fixture/tests/waybill-fixture-check", elf(b"check")),
    ]);
    let docs = run_scan(&["--image"], &image, &[]);
    let names = names_and_purls(&docs.cdx);

    assert!(has(&names, "waybill-fixture-tool"), "control: {names:?}");
    assert!(
        has(&names, "waybill-fixture-check"),
        "an image scan must read binaries under tests/: {names:?}"
    );
    assert_eq!(counts(&docs), [None, None, None]);
}
