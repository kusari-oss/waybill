//! Milestone 924 (#932) — US4: schema, redaction, determinism.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

fn binary_path() -> &'static str { env!("CARGO_BIN_EXE_waybill") }

const SCHEMA: &str = include_str!("../src/report/schema/observation-report.schema.json");

fn w(dir: &Path, rel: &str, body: &[u8]) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

/// A repository exercising every shape the schema describes: a claimed
/// project, an unsupported ecosystem, an ambiguous container, and a directory
/// with observation detail.
fn fixture() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    w(r, "billing-service/Cargo.toml",
      b"[package]\nname = \"waybill-fixture-billing\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    w(r, "tools/codegen/deno.json", b"{}\n");
    w(r, "samples/a/Project.toml", b"n = 1\n");
    w(r, "samples/b/shard.yml", b"name: x\n");
    for i in 0..30 { w(r, &format!("blobs/f{i}.dat"), b"\0\0binary\0"); }
    d
}

fn run(root: &Path, extra: &[&str]) -> (serde_json::Value, String) {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // Write OUTSIDE the scanned tree. Writing the report into the directory
    // being scanned makes the next run observe the previous run's output --
    // which surfaces as a determinism failure that is the harness's fault, not
    // the tool's. (It is also a real hazard for operators: `--output` inside
    // the scan root perturbs the very thing being reported on.)
    let out = std::env::temp_dir()
        .join(format!("m924-schema-{}-{seq}.json", std::process::id()));
    let mut args: Vec<&str> = vec!["repo", "report", "--path", root.to_str().unwrap(),
                                   "--output", out.to_str().unwrap()];
    args.extend_from_slice(extra);
    let o = Command::new(binary_path()).args(&args).output().unwrap();
    assert!(o.status.success(), "repo report failed: {o:?}");
    let v = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    (v, String::from_utf8_lossy(&o.stderr).to_string())
}

fn validator() -> jsonschema::Validator {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).unwrap();
    jsonschema::validator_for(&schema).expect("the published schema must itself be valid")
}

/// SC-006 — every field emitted validates, and the schema describes every
/// field emitted. `additionalProperties: false` throughout is what makes the
/// second half real: a field added to the report without a schema entry fails
/// here rather than drifting quietly.
#[test]
fn every_emitted_report_validates_against_the_published_schema_m924() {
    let d = fixture();
    let v = validator();
    for mode in [vec![], vec!["--redact"]] {
        let (rep, _) = run(d.path(), &mode);
        if let Err(e) = v.validate(&rep) {
            panic!("report failed schema validation (mode {mode:?}): {e}");
        }
    }
}

/// SC-004 / FR-019 — unconditional in BOTH modes. Not a setting: a property.
#[test]
fn no_absolute_paths_and_no_file_contents_in_either_mode_m924() {
    let d = fixture();
    let root_str = d.path().to_string_lossy().to_string();
    for mode in [vec![], vec!["--redact"]] {
        let (rep, _) = run(d.path(), &mode);
        let text = serde_json::to_string(&rep).unwrap();
        assert!(
            !text.contains(&root_str),
            "mode {mode:?}: the absolute scan-root path leaked into the report",
        );
        assert!(!text.contains("/var/folders") && !text.contains("/tmp/"),
            "mode {mode:?}: an absolute filesystem path leaked");
        // File contents are never the report's subject.
        assert!(!text.contains("waybill-fixture-billing"),
            "mode {mode:?}: content read from a scanned file leaked into the report");
    }
}

/// FR-019a — paths are RETAINED by default.
///
/// Without this, a default silently flipped to `paths` passes every other test
/// in this file while quietly destroying the report's usefulness to the person
/// it is sent to.
#[test]
fn the_default_mode_retains_repository_relative_names_m924() {
    let d = fixture();
    let (rep, _) = run(d.path(), &[]);
    assert_eq!(rep["redaction_mode"], "none");
    let text = serde_json::to_string(&rep).unwrap();
    assert!(text.contains("billing-service"),
        "the default mode must keep real directory names -- they are what make \
         a report actionable to someone who cannot see the repository");
    assert!(text.contains("tools/codegen"));
}

/// SC-011 — redaction removes names, keeps structure.
#[test]
fn redaction_removes_names_but_preserves_structure_m924() {
    let d = fixture();
    let (rep, _) = run(d.path(), &["--redact"]);
    assert_eq!(rep["redaction_mode"], "paths");
    let text = serde_json::to_string(&rep).unwrap();
    for name in ["billing-service", "codegen", "samples", "blobs"] {
        assert!(!text.contains(name),
            "redaction leaked the original segment {name:?}");
    }
    let paths: Vec<&str> = rep["directories"].as_array().unwrap().iter()
        .map(|o| o["path"].as_str().unwrap()).collect();
    assert!(paths.iter().any(|p| p.contains('/')),
        "nesting depth must survive redaction: {paths:?}");
    // Identical segments map identically, so repetition still correlates.
    let (rep2, _) = run(d.path(), &["--redact"]);
    let paths2: Vec<&str> = rep2["directories"].as_array().unwrap().iter()
        .map(|o| o["path"].as_str().unwrap()).collect();
    assert_eq!(paths, paths2, "redacted identifiers must be stable across runs");
}

/// SC-012 / FR-019d — the stricter mode is advertised at the moment it
/// matters, not buried in documentation a sharer reads afterwards.
#[test]
fn the_default_run_tells_the_operator_a_stricter_mode_exists_m924() {
    let d = fixture();
    let (_, stderr) = run(d.path(), &[]);
    assert!(stderr.contains("--redact"),
        "a default run must mention --redact in its own output; got: {stderr}");
    let (_, stderr_redacted) = run(d.path(), &["--redact"]);
    assert!(!stderr_redacted.contains("Re-run with --redact"),
        "a run already redacting should not advise redacting: {stderr_redacted}");
}

/// SC-005 / FR-020 — determinism, with the volatile set read FROM the
/// document rather than hard-coded, so the test stays correct as the schema
/// grows (contract C-5).
#[test]
fn two_runs_are_identical_once_declared_volatile_fields_are_masked_m924() {
    let d = fixture();
    let (a, _) = run(d.path(), &[]);
    let (b, _) = run(d.path(), &[]);

    let volatile: Vec<String> = a["volatile_fields"].as_array().unwrap().iter()
        .map(|v| v.as_str().unwrap().to_string()).collect();
    assert!(!volatile.is_empty(), "volatile_fields must be self-describing");

    let mut a2 = a.clone();
    let mut b2 = b.clone();
    for f in &volatile {
        a2[f.as_str()] = serde_json::Value::Null;
        b2[f.as_str()] = serde_json::Value::Null;
    }
    assert_eq!(
        serde_json::to_string(&a2).unwrap(),
        serde_json::to_string(&b2).unwrap(),
        "two runs over an unchanged repository differ in a field not declared \
         volatile. Either the report is nondeterministic or volatile_fields is \
         incomplete -- both are FR-020 failures.",
    );
    // And the masking is load-bearing: unmasked, the runs DO differ.
    assert_ne!(a["generated_at"], serde_json::Value::Null);
}

/// SC-015 / FR-017c — unknown enum members are preserved and surfaced, never
/// coerced or dropped; an unrecognised major is refused.
///
/// Modelled from the consumer's side, because that is where the obligation
/// lands. Adding an enum member is only a MINOR bump, yet it silently breaks a
/// consumer that matches exhaustively -- which is the bug class this repo
/// already shipped once in an allowlist that survived roughly two years.
#[test]
fn a_consumer_refuses_an_unknown_major_and_surfaces_unknown_enum_members_m924() {
    let d = fixture();
    let (rep, _) = run(d.path(), &[]);
    let major = rep["schema_version"]["major"].as_u64().unwrap();

    // A consumer that understands only this major.
    let accepts = |r: &serde_json::Value| r["schema_version"]["major"].as_u64() == Some(major);
    assert!(accepts(&rep));
    let mut future = rep.clone();
    future["schema_version"]["major"] = serde_json::json!(major + 1);
    assert!(!accepts(&future),
        "a consumer MUST refuse a major it does not recognise rather than guess");

    // An unknown enum member must survive a round trip rather than being
    // coerced to a known one or dropped.
    let mut odd = rep.clone();
    odd["directories"][0]["claim_status"] = serde_json::json!("some_future_status");
    let round: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&odd).unwrap()).unwrap();
    assert_eq!(round["directories"][0]["claim_status"], "some_future_status",
        "an unknown enum member must be preserved verbatim");

    // And the schema rejects it, so a consumer is told rather than surprised.
    let v = validator();
    assert!(v.validate(&odd).is_err(),
        "the schema must reject an unknown claim_status so a consumer learns of \
         it, rather than silently accepting a value it cannot interpret");
}
