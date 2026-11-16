//! Helpers for the installed-library reader tests (spec 1531, #1531):
//! synthetic install prefixes built in a tempdir and scanned in all three
//! formats. Nothing is fetched.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// The three documents of one scan, plus its log.
pub struct Docs {
    pub cdx: Value,
    pub spdx23: Value,
    pub spdx3: Value,
    pub stderr: String,
}

/// Write `content` to `root/rel`, creating parent directories.
pub fn write(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
    std::fs::write(p, content).expect("write");
}

/// Scan `target` with `mode` (`--path` or `--image`) in all three formats.
pub fn scan_with(mode: &str, target: &Path, extra: &[&str]) -> Docs {
    let out_dir = tempfile::tempdir().expect("tempdir");
    let (c, s2, s3) = (
        out_dir.path().join("o.cdx.json"),
        out_dir.path().join("o.spdx.json"),
        out_dir.path().join("o.spdx3.json"),
    );
    let home = tempfile::tempdir().expect("home");
    let out = Command::new(super::bin())
        .env("HOME", home.path())
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .args(["--offline", "sbom", "scan", mode])
        .arg(target)
        .args(["--no-deep-hash", "--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", c.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", s2.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", s3.display()))
        .args(extra)
        .output()
        .expect("spawn waybill");
    assert!(out.status.success(), "scan failed: {}", String::from_utf8_lossy(&out.stderr));
    let read = |p: &Path| -> Value {
        serde_json::from_slice(&std::fs::read(p).expect("read output")).expect("parse output")
    };
    Docs {
        cdx: read(&c),
        spdx23: read(&s2),
        spdx3: read(&s3),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

pub fn scan(target: &Path) -> Docs {
    scan_with("--path", target, &[])
}

/// A CycloneDX property value.
pub fn prop<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c["properties"].as_array()?.iter().find(|p| p["name"] == name)?["value"].as_str()
}

/// Every CycloneDX component, nested ones included.
pub fn components(cdx: &Value) -> Vec<&Value> {
    fn walk<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
        for c in v["components"].as_array().into_iter().flatten() {
            out.push(c);
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    walk(cdx, &mut out);
    out
}

pub fn component<'a>(cdx: &'a Value, purl: &str) -> Option<&'a Value> {
    components(cdx).into_iter().find(|c| c["purl"] == purl)
}

/// Sorted PURLs of components starting with `prefix`.
pub fn purls_with_prefix(cdx: &Value, prefix: &str) -> Vec<String> {
    let mut v: Vec<String> = components(cdx)
        .into_iter()
        .filter_map(|c| c["purl"].as_str())
        .filter(|p| p.starts_with(prefix))
        .map(str::to_string)
        .collect();
    v.sort();
    v.dedup();
    v
}

/// The `dependsOn` list of the component with `purl`, sorted.
pub fn depends_on(cdx: &Value, purl: &str) -> Vec<String> {
    let Some(c) = component(cdx, purl) else { return Vec::new() };
    let r = c["bom-ref"].as_str().unwrap_or("");
    let mut v: Vec<String> = cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["ref"] == r)
        .and_then(|d| d["dependsOn"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str())
        .filter_map(|r| {
            components(cdx).into_iter().find(|c| c["bom-ref"] == r).and_then(|c| c["purl"].as_str())
        })
        .map(str::to_string)
        .collect();
    v.sort();
    v
}

/// A document-scope waybill field from CycloneDX metadata properties,
/// parsed as JSON.
pub fn cdx_doc_field(cdx: &Value, field: &str) -> Option<Value> {
    let v = cdx["metadata"]["properties"].as_array()?.iter().find(|p| p["name"] == field)?["value"].as_str()?;
    serde_json::from_str(v).ok()
}

/// A document-scope waybill field from an SPDX 2.3 or SPDX 3 document's
/// annotation envelopes, parsed as JSON.
pub fn spdx_doc_field(doc: &Value, field: &str) -> Option<Value> {
    let anns: Vec<&Value> = doc["annotations"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(doc["@graph"].as_array().into_iter().flatten().filter(|e| e["type"] == "Annotation"))
        .collect();
    anns.iter().find_map(|a| {
        let env = a["comment"].as_str().or_else(|| a["statement"].as_str())?;
        let env: Value = serde_json::from_str(env).ok()?;
        if env["field"] != field {
            return None;
        }
        serde_json::from_str(env["value"].as_str()?).ok()
    })
}

/// The same document-scope field from all three formats; panics if they
/// disagree. `None` when absent from all three.
pub fn doc_field_all_formats(docs: &Docs, field: &str) -> Option<Value> {
    let c = cdx_doc_field(&docs.cdx, field);
    let s2 = spdx_doc_field(&docs.spdx23, field);
    let s3 = spdx_doc_field(&docs.spdx3, field);
    assert_eq!(c, s2, "{field}: CycloneDX and SPDX 2.3 disagree");
    assert_eq!(c, s3, "{field}: CycloneDX and SPDX 3 disagree");
    c
}

/// Source files (C18) of a component, sorted.
pub fn source_files(c: &Value) -> Vec<String> {
    let Some(v) = prop(c, "waybill:source-files") else { return Vec::new() };
    let mut out: Vec<String> = serde_json::from_str::<Vec<String>>(v)
        .unwrap_or_else(|_| v.split(',').map(str::to_string).collect());
    out.sort();
    out
}

/// Catalog row `row`'s values from all three formats, through the parity
/// extractors; panics if the formats disagree.
pub fn row_all_formats(docs: &Docs, row: &str) -> std::collections::BTreeSet<String> {
    let e = waybill::parity::extractors::EXTRACTORS
        .iter()
        .find(|e| e.row_id == row)
        .unwrap_or_else(|| panic!("no extractor for {row}"));
    let (a, b, c) = ((e.cdx)(&docs.cdx), (e.spdx23)(&docs.spdx23), (e.spdx3)(&docs.spdx3));
    assert_eq!(a, b, "{row}: CycloneDX and SPDX 2.3 disagree");
    assert_eq!(a, c, "{row}: CycloneDX and SPDX 3 disagree");
    a
}

/// A dpkg database at `root` owning `owned` (paths relative to the root,
/// without a leading `/`) through package `pkg`.
pub fn dpkg_owning(root: &Path, pkg: &str, owned: &[&str]) {
    write(
        root,
        "var/lib/dpkg/status",
        &format!("Package: {pkg}\nStatus: install ok installed\nVersion: 1.0\nArchitecture: amd64\n"),
    );
    let list: String = owned.iter().map(|p| format!("/{p}\n")).collect();
    write(root, &format!("var/lib/dpkg/info/{pkg}:amd64.list"), &list);
}

/// The `waybill:installed-library-coverage` record, agreeing across formats.
pub fn coverage(docs: &Docs) -> Option<Value> {
    doc_field_all_formats(docs, "waybill:installed-library-coverage")
}

pub fn skipped(cov: &Value, reason: &str) -> u64 {
    cov["skipped"][reason].as_u64().unwrap_or(u64::MAX)
}

/// A minimal ELF64 shared library (`ET_DYN`, no program headers, so no
/// `PT_INTERP`) whose `.rodata` section holds `rodata`. Adapted from the
/// cargo-auditable fixture in `closure_binary_contents.rs`.
pub fn elf_shared_lib(rodata: &[u8]) -> Vec<u8> {
    let mut shstrtab = vec![0u8];
    let name_off = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".rodata\0");
    let shstr_off = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".shstrtab\0");
    let data_off = 64u64;
    let pad = 1024usize.saturating_sub(rodata.len());
    let strtab_off = data_off + rodata.len() as u64 + pad as u64;
    let shoff = strtab_off + shstrtab.len() as u64;
    let mut f = Vec::new();
    f.extend_from_slice(b"\x7fELF");
    f.extend_from_slice(&[2, 1, 1, 0]);
    f.extend_from_slice(&[0u8; 8]);
    f.extend_from_slice(&3u16.to_le_bytes()); // ET_DYN
    f.extend_from_slice(&62u16.to_le_bytes()); // x86_64
    f.extend_from_slice(&1u32.to_le_bytes());
    f.extend_from_slice(&0u64.to_le_bytes());
    f.extend_from_slice(&0u64.to_le_bytes());
    f.extend_from_slice(&shoff.to_le_bytes());
    f.extend_from_slice(&0u32.to_le_bytes());
    f.extend_from_slice(&64u16.to_le_bytes());
    f.extend_from_slice(&0u16.to_le_bytes());
    f.extend_from_slice(&0u16.to_le_bytes());
    f.extend_from_slice(&64u16.to_le_bytes());
    f.extend_from_slice(&3u16.to_le_bytes());
    f.extend_from_slice(&2u16.to_le_bytes());
    f.extend_from_slice(rodata);
    f.extend(std::iter::repeat_n(0u8, pad));
    f.extend_from_slice(&shstrtab);
    let mut sh = |name: u32, ty: u32, flags: u64, off: u64, size: u64| {
        f.extend_from_slice(&name.to_le_bytes());
        f.extend_from_slice(&ty.to_le_bytes());
        f.extend_from_slice(&flags.to_le_bytes());
        f.extend_from_slice(&0u64.to_le_bytes());
        f.extend_from_slice(&off.to_le_bytes());
        f.extend_from_slice(&size.to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes());
        f.extend_from_slice(&1u64.to_le_bytes());
        f.extend_from_slice(&0u64.to_le_bytes());
    };
    sh(0, 0, 0, 0, 0);
    sh(name_off, 1, 2, data_off, rodata.len() as u64); // PROGBITS, SHF_ALLOC
    sh(shstr_off, 3, 0, strtab_off, shstrtab.len() as u64);
    f
}

/// A 64-bit little-endian Mach-O dylib (`MH_DYLIB`) that loads each of
/// `loads` and whose `__TEXT,__cstring` section holds `cstring`.
///
/// Each `LC_LOAD_DYLIB` gets one undefined symbol bound to its library
/// ordinal, because the `object` crate reports a Mach-O import through a
/// symbol, not through the load command alone (the shape of the `macho`
/// builder in `binary_test_inputs.rs`). `cstring` is where the binary scan
/// reads embedded version strings on Mach-O. Spec 1536 uses this rather than
/// an ELF: an ELF import names its library only through symbol versioning.
pub fn macho_dylib(loads: &[&str], cstring: &[u8]) -> Vec<u8> {
    let le = |v: u32| v.to_le_bytes();
    let mut cmds: Vec<u8> = Vec::new();
    let mut ncmds = 0u32;

    // Data laid out after the header and commands, at fixed offsets.
    const CSTRING_OFF: u32 = 2048;
    const SYM_OFF: u32 = 3072;

    // LC_SEGMENT_64 __TEXT with one section, __cstring.
    let mut seg = Vec::new();
    seg.extend_from_slice(&le(0x19));
    seg.extend_from_slice(&le(72 + 80));
    let name16 = |s: &str, v: &mut Vec<u8>| {
        let mut n = [0u8; 16];
        n[..s.len()].copy_from_slice(s.as_bytes());
        v.extend_from_slice(&n);
    };
    name16("__TEXT", &mut seg);
    let total = 4096u64;
    for v in [0u64, total, 0, total] {
        seg.extend_from_slice(&v.to_le_bytes()); // vmaddr, vmsize, fileoff, filesize
    }
    for v in [5u32, 5, 1, 0] {
        seg.extend_from_slice(&le(v)); // maxprot, initprot, nsects, flags
    }
    name16("__cstring", &mut seg);
    name16("__TEXT", &mut seg);
    seg.extend_from_slice(&u64::from(CSTRING_OFF).to_le_bytes()); // addr
    seg.extend_from_slice(&(cstring.len() as u64).to_le_bytes()); // size
    for v in [CSTRING_OFF, 0, 0, 0, 0x2, 0, 0, 0] {
        seg.extend_from_slice(&le(v)); // offset, align, reloff, nreloc, S_CSTRING_LITERALS, reserved1-3
    }
    cmds.extend_from_slice(&seg);
    ncmds += 1;

    for dylib in loads {
        let mut name = dylib.as_bytes().to_vec();
        name.push(0);
        let size = (24 + name.len() as u32 + 7) & !7;
        for v in [0x0c, size, 24, 2, 0x1_0000, 0x1_0000] {
            cmds.extend_from_slice(&le(v));
        }
        let start = cmds.len();
        cmds.extend_from_slice(&name);
        cmds.resize(start + size as usize - 24, 0);
        ncmds += 1;
    }

    // One undefined symbol per load, bound to its ordinal.
    let mut strtab = vec![0u8];
    let mut nlist = Vec::new();
    for (i, _) in loads.iter().enumerate() {
        let strx = strtab.len() as u32;
        strtab.extend_from_slice(format!("_waybill_fixture_sym{i}\0").as_bytes());
        nlist.extend_from_slice(&le(strx));
        nlist.push(0x01); // N_UNDF | N_EXT
        nlist.push(0);
        nlist.extend_from_slice(&(((i as u16) + 1) << 8).to_le_bytes());
        nlist.extend_from_slice(&0u64.to_le_bytes());
    }
    let stroff = SYM_OFF + nlist.len() as u32;
    for v in [0x2, 24, SYM_OFF, loads.len() as u32, stroff, strtab.len() as u32] {
        cmds.extend_from_slice(&le(v));
    }
    ncmds += 1;
    let mut dysymtab = [0u32; 20];
    dysymtab[0] = 0x0b;
    dysymtab[1] = 80;
    dysymtab[7] = loads.len() as u32; // nundefsym
    for v in dysymtab {
        cmds.extend_from_slice(&le(v));
    }
    ncmds += 1;

    let mut b = Vec::new();
    // mach_header_64: MH_MAGIC_64, x86_64, MH_DYLIB, MH_NOUNDEFS | MH_DYLDLINK | MH_TWOLEVEL.
    for v in [0xfeed_facf, 0x0100_0007, 3, 6, ncmds, cmds.len() as u32, 0x85, 0] {
        b.extend_from_slice(&le(v));
    }
    b.extend_from_slice(&cmds);
    assert!(b.len() <= CSTRING_OFF as usize && cstring.len() <= (SYM_OFF - CSTRING_OFF) as usize);
    b.resize(CSTRING_OFF as usize, 0);
    b.extend_from_slice(cstring);
    b.resize(SYM_OFF as usize, 0);
    b.extend_from_slice(&nlist);
    b.extend_from_slice(&strtab);
    b.resize(total as usize, 0);
    b
}
