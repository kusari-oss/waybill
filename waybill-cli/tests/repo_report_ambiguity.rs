//! Milestone 924 (#932) — US3: typed uncertainty.
//!
//! The report records observations and ambiguity, never a conclusion the
//! evidence does not support (FR-014).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

fn binary_path() -> &'static str { env!("CARGO_BIN_EXE_waybill") }

fn w(dir: &Path, rel: &str, body: &[u8]) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn run_report(root: &Path, extra: &[&str]) -> serde_json::Value {
    // A per-invocation output path. Keying on the process id alone collides:
    // every test in this binary shares a pid and cargo runs them in parallel,
    // so they overwrite each other's reports and fail in whichever order they
    // happen to finish. Cost me a debugging round on an implementation that
    // was already correct.
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir()
        .join(format!("m924-{}-{seq}.json", std::process::id()));
    let mut args: Vec<&str> = vec!["repo", "report", "--path", root.to_str().unwrap(),
                                   "--output", out.to_str().unwrap()];
    args.extend_from_slice(extra);
    let st = Command::new(binary_path()).args(&args).status().unwrap();
    assert!(st.success(), "repo report failed: {st:?}");
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

/// **SC-001 — the self-test.** This repository is the feature's hardest case.
///
/// `waybill-cli/tests/` holds 89 lockfiles across several ecosystems, none of
/// which are waybill's dependencies. The report must record that ambiguity
/// with its evidence rather than silently treating them as dependencies or
/// omitting them.
#[test]
fn this_repository_reports_its_own_fixture_tree_as_ambiguous_m924() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let rep = run_report(repo, &["--exclude-path", "target"]);

    let tests_dir = rep["directories"].as_array().unwrap().iter()
        .find(|o| o["path"] == "waybill-cli/tests")
        .unwrap_or_else(|| panic!("waybill-cli/tests must appear in the report"));

    let amb = tests_dir.get("ambiguity").and_then(|a| a.as_object()).unwrap_or_else(|| {
        panic!(
            "waybill-cli/tests carries NO ambiguity record. Its subtree holds \
             lockfiles from many ecosystems that are fixtures, not dependencies \
             — recording that is the whole point of this feature. Got: {tests_dir}"
        )
    });

    let interps = amb["interpretations"].as_array().unwrap();
    assert!(interps.len() >= 2,
        "an ambiguity with fewer than two interpretations is a classification, \
         not an ambiguity (FR-014): {interps:?}");
    let ev = amb["evidence"].as_array().unwrap();
    assert!(ev.len() >= 3,
        "the ambiguity must carry the evidence that produced it (FR-015): {ev:?}");
    assert!(
        interps.iter().any(|i| i.as_str().unwrap_or("").contains("fixtures")),
        "'test fixtures' must be among the readings offered: {interps:?}",
    );
}

/// SC-014 — claim status and ambiguity are independent fields (FR-012a/b).
#[test]
fn claim_status_and_ambiguity_are_independent_m924() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    // A claimed, unambiguous project.
    w(root, "app/Cargo.toml",
      b"[package]\nname = \"waybill-fixture-app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    // An unclaimed, ambiguous tree: several ecosystems below a dir with no marker.
    w(root, "samples/a/deno.json", b"{}\n");
    w(root, "samples/b/Project.toml", b"name = \"X\"\n");
    w(root, "samples/c/shard.yml", b"name: x\n");

    let rep = run_report(root, &[]);
    let dirs = rep["directories"].as_array().unwrap();

    for o in dirs {
        let s = o["claim_status"].as_str().unwrap();
        assert!(
            ["claimed", "unclaimed", "excluded_by_policy"].contains(&s),
            "claim_status must be exactly one of the three exclusive values: {s}",
        );
    }
    let samples = dirs.iter().find(|o| o["path"] == "samples")
        .expect("the multi-ecosystem container must be recorded");
    assert!(samples.get("ambiguity").map(|a| !a.is_null()).unwrap_or(false),
        "a directory whose subtree spans three ecosystems must be ambiguous: {samples}");
    let app = dirs.iter().find(|o| o["path"] == "app").expect("app must be recorded");
    assert_eq!(app["claim_status"], "claimed");
    assert!(app.get("ambiguity").map(|a| a.is_null()).unwrap_or(true),
        "a single-ecosystem project root is not ambiguous: {app}");
}

/// FR-011 — the field that makes an unclassified directory actionable.
#[test]
fn binary_and_text_directories_are_distinguishable_m924() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    for i in 0..30 {
        w(root, &format!("blobs/f{i}.dat"), b"\0\0\0binary payload\0");
        w(root, &format!("prose/f{i}.txt"), b"plain readable text\n");
    }
    let rep = run_report(root, &[]);
    let kind = |p: &str| -> String {
        rep["directories"].as_array().unwrap().iter()
            .find(|o| o["path"] == p)
            .and_then(|o| o["observation"]["content_kind"].as_str())
            .unwrap_or("<absent>").to_string()
    };
    assert_eq!(kind("blobs"), "predominantly_binary");
    assert_eq!(kind("prose"), "predominantly_text");
}

/// FR-014 — the report never resolves ambiguity by preference.
#[test]
fn an_ambiguous_directory_never_claims_a_single_authoritative_ecosystem_m924() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    w(root, "mixed/a/deno.json", b"{}\n");
    w(root, "mixed/b/Project.toml", b"name = \"X\"\n");

    let rep = run_report(root, &[]);
    for o in rep["directories"].as_array().unwrap() {
        if o.get("ambiguity").map(|a| !a.is_null()).unwrap_or(false) {
            let ecos = o["ecosystems"].as_array().cloned().unwrap_or_default();
            assert!(
                ecos.len() != 1,
                "a directory carrying an ambiguity must not also assert one \
                 authoritative ecosystem — that resolves the ambiguity by \
                 preference, which FR-014 forbids: {o}",
            );
        }
    }
}

/// FR-010 — this feature must not widen the file-tier source-shape allowlist.
///
/// A **negative** requirement with no other enforcement: widening that
/// allowlist silently changes emitted SBOM content, and nothing else in the
/// suite would fail.
#[test]
fn the_file_tier_source_shape_allowlist_is_unchanged_m924() {
    let src = include_str!("../src/scan_fs/file_tier/source_shape.rs");
    let start = src.find("pub(crate) enum SourceShape {").expect("enum must exist");
    let end = src[start..].find("\n}").expect("enum must close") + start;
    let body = &src[start..end];
    let variants = body.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//") && l.ends_with(','))
        .count();
    assert_eq!(
        variants, 21,
        "the file-tier SourceShape allowlist changed ({variants} variants, \
         expected 21). FR-010 forbids this feature widening it — that allowlist \
         governs SBOM EMISSION, and changing it changes emitted output. If the \
         change is deliberate and unrelated to m924, update this count and say \
         why in the commit.",
    );
}
