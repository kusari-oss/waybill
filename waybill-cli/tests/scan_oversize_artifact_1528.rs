//! #1528 — `--max-file-size` bounds hashing, not package identification.
//!
//! A wheel larger than the limit used to be dropped by the artifact
//! walker before its filename was ever resolved, so the scan emitted no
//! component for it. The limit is exercised here with a small value
//! against small synthetic wheels rather than with large files.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::io::Write;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

const MAX_FILE_SIZE: u64 = 4096;

/// Write a minimal wheel: a zip holding `<dist>.dist-info/METADATA` and
/// `WHEEL`, padded with a stored filler member to reach `min_len` bytes.
fn write_wheel(path: &Path, name: &str, version: &str, min_len: usize) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);
    let dist_info = format!("{}-{version}.dist-info", name.replace('-', "_"));
    zip.start_file(format!("{dist_info}/METADATA"), opts).unwrap();
    write!(zip, "Metadata-Version: 2.1\nName: {name}\nVersion: {version}\n").unwrap();
    zip.start_file(format!("{dist_info}/WHEEL"), opts).unwrap();
    write!(zip, "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n").unwrap();
    zip.start_file("filler.bin", opts).unwrap();
    zip.write_all(&vec![0u8; min_len]).unwrap();
    zip.finish().unwrap();
}

fn run_scan(root: &Path) -> (Value, String) {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .env("RUST_LOG", "warn")
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--max-file-size")
        .arg(MAX_FILE_SIZE.to_string())
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .output()
        .unwrap();
    let stderr = strip_ansi(&String::from_utf8_lossy(&result.stderr));
    assert!(result.status.success(), "scan failed: stderr={stderr}");
    let doc = serde_json::from_slice(&std::fs::read(&out_path).unwrap()).unwrap();
    (doc, stderr)
}

/// The log formatter colours field names, which splits `size=` apart.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn component<'a>(doc: &'a Value, purl: &str) -> Option<&'a Value> {
    doc["components"]
        .as_array()?
        .iter()
        .find(|c| c["purl"].as_str() == Some(purl))
}

fn hash_count(c: &Value) -> usize {
    c.get("hashes").and_then(Value::as_array).map_or(0, Vec::len)
}

#[test]
fn wheel_over_max_file_size_is_still_identified() {
    let dir = tempfile::tempdir().unwrap();
    let big = dir.path().join("waybill_fixture_big-1.0.0-py3-none-any.whl");
    let small = dir.path().join("waybill_fixture_small-2.0.0-py3-none-any.whl");
    write_wheel(&big, "waybill-fixture-big", "1.0.0", 2 * MAX_FILE_SIZE as usize);
    write_wheel(&small, "waybill-fixture-small", "2.0.0", 16);
    let big_len = std::fs::metadata(&big).unwrap().len();
    assert!(big_len > MAX_FILE_SIZE, "fixture must exceed the limit: {big_len}");
    assert!(std::fs::metadata(&small).unwrap().len() < MAX_FILE_SIZE);

    let (doc, stderr) = run_scan(dir.path());

    // Positive control: the under-limit wheel went through the same
    // walk and was hashed, so the walker path ran with this limit.
    let small_c = component(&doc, "pkg:pypi/waybill-fixture-small@2.0.0")
        .unwrap_or_else(|| panic!("control wheel missing: {doc:#}"));
    assert!(hash_count(small_c) > 0, "control wheel must carry a hash: {small_c:#}");

    let big_c = component(&doc, "pkg:pypi/waybill-fixture-big@1.0.0")
        .unwrap_or_else(|| panic!("oversized wheel produced no component: {doc:#}"));
    assert_eq!(hash_count(big_c), 0, "oversized wheel must not be hashed: {big_c:#}");

    // The skipped hash is named: path and size in a WARN line.
    let warn = stderr
        .lines()
        .find(|l| l.contains("WARN") && l.contains("waybill_fixture_big-1.0.0-py3-none-any.whl"))
        .unwrap_or_else(|| panic!("no WARN naming the oversized wheel: {stderr}"));
    assert!(warn.contains(&format!("size={big_len}")), "WARN must carry the size: {warn}");
    assert!(
        !stderr.contains("waybill_fixture_small-2.0.0"),
        "the under-limit wheel must not be warned about: {stderr}"
    );
}
