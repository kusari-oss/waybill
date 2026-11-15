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
