//! Shared helpers for the `waybill sbom edit` integration tests
//! (milestone 1071). Format-aware readers of the fixture outputs, so each
//! test states its property once for all three formats.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::OnceLock;

use serde_json::Value;

/// (label, fixture file name) for the three formats.
pub const FORMATS: [(&str, &str); 3] =
    [("cdx", "full.cdx.json"), ("spdx23", "full.spdx.json"), ("spdx3", "full.spdx3.json")];

pub const ROOT: &str = "acme-shop";

pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sbom_edit").join(name)
}

pub fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

pub fn read_json(path: &Path) -> Value {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

pub struct Edited {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
    pub path: PathBuf,
    pub dir: tempfile::TempDir,
}

impl Edited {
    pub fn json(&self) -> Value {
        read_json(&self.path)
    }

    pub fn text(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap_or_default()
    }

    pub fn assert_ok(&self) -> &Self {
        assert!(self.status.success(), "edit failed: {}", self.stderr);
        self
    }
}

/// Run `waybill sbom edit <input> -o <tmp>/<input name> <args>`.
pub fn edit(input: &Path, args: &[&str]) -> Edited {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let path = dir.path().join(input.file_name().unwrap_or_else(|| panic!("input has no file name")));
    edit_to(input, &path, args, dir)
}

pub fn edit_to(input: &Path, output: &Path, args: &[&str], dir: tempfile::TempDir) -> Edited {
    let out = Command::new(bin())
        .args(["sbom", "edit"])
        .arg(input)
        .arg("-o")
        .arg(output)
        .args(args)
        .env("RUST_LOG", "warn")
        .output()
        .unwrap_or_else(|e| panic!("running waybill: {e}"));
    Edited {
        status: out.status,
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        path: output.to_path_buf(),
        dir,
    }
}

pub fn format_of(doc: &Value) -> &'static str {
    if doc.get("bomFormat").is_some() {
        "cdx"
    } else if doc.get("spdxVersion").is_some() {
        "spdx23"
    } else {
        "spdx3"
    }
}

/// Every component as (id, name).
pub fn components(doc: &Value) -> Vec<(String, String)> {
    match format_of(doc) {
        "cdx" => {
            let mut out = Vec::new();
            fn walk(cs: &[Value], out: &mut Vec<(String, String)>) {
                for c in cs {
                    if let (Some(id), Some(n)) = (c.get("bom-ref").and_then(Value::as_str), c.get("name").and_then(Value::as_str)) {
                        out.push((id.to_string(), n.to_string()));
                    }
                    if let Some(sub) = c.get("components").and_then(Value::as_array) {
                        walk(sub, out);
                    }
                }
            }
            if let Some(root) = doc.pointer("/metadata/component") {
                walk(std::slice::from_ref(root), &mut out);
            }
            walk(doc.get("components").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]), &mut out);
            out
        }
        "spdx23" => ["packages", "files"]
            .iter()
            .flat_map(|k| doc.get(*k).and_then(Value::as_array).cloned().unwrap_or_default())
            .filter_map(|p| {
                let id = p.get("SPDXID")?.as_str()?.to_string();
                let name = p.get("name").or_else(|| p.get("fileName"))?.as_str()?.to_string();
                Some((id, name))
            })
            .collect(),
        _ => graph(doc)
            .iter()
            .filter(|e| matches!(e.get("type").and_then(Value::as_str), Some("software_Package" | "software_File")))
            .filter_map(|e| Some((e.get("spdxId")?.as_str()?.to_string(), e.get("name")?.as_str()?.to_string())))
            .collect(),
    }
}

pub fn names(doc: &Value) -> BTreeSet<String> {
    components(doc).into_iter().map(|(_, n)| n).collect()
}

fn graph(doc: &Value) -> Vec<Value> {
    doc.get("@graph").and_then(Value::as_array).cloned().unwrap_or_default()
}

/// Dependency edges as (from id, to id).
pub fn edges(doc: &Value) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    match format_of(doc) {
        "cdx" => {
            for d in doc.get("dependencies").and_then(Value::as_array).cloned().unwrap_or_default() {
                let Some(from) = d.get("ref").and_then(Value::as_str) else { continue };
                for t in d.get("dependsOn").and_then(Value::as_array).cloned().unwrap_or_default() {
                    if let Some(t) = t.as_str() {
                        out.insert((from.to_string(), t.to_string()));
                    }
                }
            }
        }
        "spdx23" => {
            for r in doc.get("relationships").and_then(Value::as_array).cloned().unwrap_or_default() {
                let (Some(a), Some(t), Some(b)) = (
                    r.get("spdxElementId").and_then(Value::as_str),
                    r.get("relationshipType").and_then(Value::as_str),
                    r.get("relatedSpdxElement").and_then(Value::as_str),
                ) else {
                    continue;
                };
                if t == "DEPENDS_ON" {
                    out.insert((a.to_string(), b.to_string()));
                } else if t.ends_with("DEPENDENCY_OF") {
                    out.insert((b.to_string(), a.to_string()));
                }
            }
        }
        _ => {
            for e in graph(doc) {
                if e.get("relationshipType").and_then(Value::as_str) != Some("dependsOn") {
                    continue;
                }
                let Some(from) = e.get("from").and_then(Value::as_str) else { continue };
                for t in e.get("to").and_then(Value::as_array).cloned().unwrap_or_default() {
                    if let Some(t) = t.as_str() {
                        out.insert((from.to_string(), t.to_string()));
                    }
                }
            }
        }
    }
    out
}

fn ids_named(doc: &Value, name: &str) -> BTreeSet<String> {
    components(doc).into_iter().filter(|(_, n)| n == name).map(|(i, _)| i).collect()
}

pub fn depends(doc: &Value, from: &str, to: &str) -> bool {
    let (f, t) = (ids_named(doc, from), ids_named(doc, to));
    edges(doc).iter().any(|(a, b)| f.contains(a) && t.contains(b))
}

/// Names reachable from the named component over dependency edges.
pub fn reachable(doc: &Value, from: &str) -> BTreeSet<String> {
    let by_id: BTreeMap<String, String> = components(doc).into_iter().collect();
    let mut adj: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (a, b) in edges(doc) {
        adj.entry(a).or_default().push(b);
    }
    let mut seen: BTreeSet<String> = ids_named(doc, from);
    let mut queue: VecDeque<String> = seen.iter().cloned().collect();
    while let Some(n) = queue.pop_front() {
        for m in adj.get(&n).cloned().unwrap_or_default() {
            if seen.insert(m.clone()) {
                queue.push_back(m);
            }
        }
    }
    seen.iter().filter_map(|i| by_id.get(i).cloned()).filter(|n| n != from).collect()
}

/// `waybill:` fields present, with their values, across all three carriers:
/// CycloneDX properties, SPDX 2.3 annotation comments, SPDX 3 statements.
pub fn waybill_fields(doc: &Value) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    fn walk(v: &Value, out: &mut BTreeMap<String, Vec<String>>) {
        match v {
            Value::Object(m) => {
                if let (Some(Value::String(n)), Some(val)) = (m.get("name"), m.get("value")) {
                    if n.starts_with("waybill:") && m.len() == 2 {
                        out.entry(n.clone()).or_default().push(val.as_str().map(str::to_string).unwrap_or_else(|| val.to_string()));
                    }
                }
                m.values().for_each(|x| walk(x, out));
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::String(s) => {
                if let Ok(env) = serde_json::from_str::<Value>(s) {
                    if env.get("schema").and_then(Value::as_str) == Some("waybill-annotation/v1") {
                        if let Some(f) = env.get("field").and_then(Value::as_str) {
                            let val = env.get("value").cloned().unwrap_or(Value::Null);
                            out.entry(f.to_string())
                                .or_default()
                                .push(val.as_str().map(str::to_string).unwrap_or_else(|| val.to_string()));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    walk(doc, &mut out);
    out
}

/// The `waybill:derivation` record, parsed.
pub fn derivation(doc: &Value) -> Option<Value> {
    waybill_fields(doc)
        .get("waybill:derivation")
        .and_then(|v| v.first())
        .and_then(|s| serde_json::from_str(s).ok())
}

fn validator(schema: &str) -> jsonschema::Validator {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/schemas").join(schema);
    let schema: Value = read_json(&path);
    jsonschema::validator_for(&schema).unwrap_or_else(|e| panic!("compiling {}: {e}", path.display()))
}

/// Schema errors, as instance-path-free messages with any listed property
/// names removed, so the same defect in an input and its edit compares
/// equal. (The masked corpus goldens carry placeholder IRIs, which fail the
/// SPDX 3 schema before any edit; an element that already fails lists every
/// property it has, including one an edit added.)
pub fn schema_errors(doc: &Value) -> BTreeSet<String> {
    static SPDX23: OnceLock<jsonschema::Validator> = OnceLock::new();
    static SPDX3: OnceLock<jsonschema::Validator> = OnceLock::new();
    let errors: Vec<String> = match format_of(doc) {
        "cdx" => crate::common::cdx_schema::cdx_validator().iter_errors(doc).map(|e| e.to_string()).collect(),
        "spdx23" => SPDX23.get_or_init(|| validator("spdx-2.3.json")).iter_errors(doc).map(|e| e.to_string()).collect(),
        _ => SPDX3.get_or_init(|| validator("spdx-3.0.1.json")).iter_errors(doc).map(|e| e.to_string()).collect(),
    };
    errors
        .into_iter()
        .map(|e| match e.find(" (") {
            Some(i) if e.ends_with("unexpected)") => e[..i].to_string(),
            _ => e,
        })
        .collect()
}

/// An edit introduces no schema error its input didn't have; an input
/// with none gives an output with none.
pub fn assert_conforms_like(input: &Value, output: &Value, label: &str) {
    let before = schema_errors(input);
    let after = schema_errors(output);
    let new: Vec<&String> = after.difference(&before).collect();
    assert!(new.is_empty(), "{label}: the edit introduced schema errors: {new:#?}");
    if before.is_empty() {
        assert!(after.is_empty(), "{label}: {after:#?}");
    }
}

/// Run `spdx3-validate` when it is installed (milestone 078 convention).
pub fn spdx3_validate_or_skip(path: &Path) {
    let validator = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join(".venv/spdx3-validate/bin/spdx3-validate"))
        .unwrap_or_default();
    if !validator.exists() {
        assert!(
            std::env::var("WAYBILL_REQUIRE_SPDX3_VALIDATOR").ok().as_deref() != Some("1"),
            "spdx3-validate not found at {} and WAYBILL_REQUIRE_SPDX3_VALIDATOR=1 is set",
            validator.display()
        );
        eprintln!("WARN: spdx3-validate not found; skipping conformance gate");
        return;
    }
    let out = Command::new(&validator)
        .args(["--quiet", "-j"])
        .arg(path)
        .output()
        .unwrap_or_else(|e| panic!("running spdx3-validate: {e}"));
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(
        out.status.success() && !text.contains("Violation of type"),
        "spdx3-validate rejected {}:\n{text}",
        path.display()
    );
}

pub fn write_key(dir: &Path, name: &str, key: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, key).unwrap_or_else(|e| panic!("writing key: {e}"));
    p
}

/// Replace the corpus harness's masks (`<masked>`, `<masked-sha256>`, …)
/// with valid placeholders, so a masked golden can be schema-checked.
pub fn unmask(doc: &mut Value) {
    let spdx23 = format_of(doc) == "spdx23";
    fn walk(key: &str, v: &mut Value, spdx23: bool) {
        match v {
            Value::String(s) if s.contains("<masked") => {
                *v = match key {
                    "annotationDate" | "created" | "timestamp" | "builtTime" | "releaseTime" => {
                        Value::String("2026-01-01T00:00:00Z".into())
                    }
                    // SPDX 2.3 masks the whole object; SPDX 3 a reference.
                    "creationInfo" if spdx23 => serde_json::json!({
                        "created": "2026-01-01T00:00:00Z",
                        "creators": ["Tool: waybill-0.0.0"],
                    }),
                    "creationInfo" => Value::String("_:creation-info".into()),
                    "serialNumber" => Value::String("urn:uuid:00000000-0000-0000-0000-000000000000".into()),
                    _ => Value::String(
                        s.replace("<masked-sha256>", &"0".repeat(64))
                            .replace("<masked-tool-version>", "0.0.0")
                            .replace("<masked>", "masked"),
                    ),
                };
            }
            Value::Array(a) => a.iter_mut().for_each(|x| walk(key, x, spdx23)),
            Value::Object(m) => m.iter_mut().for_each(|(k, x)| walk(k, x, spdx23)),
            _ => {}
        }
    }
    walk("", doc, spdx23);
}
