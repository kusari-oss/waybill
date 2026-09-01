//! Milestone 671 — integration tests for the opt-in `--file-inventory=source-tree`
//! file-tier mode (SC-003 follow-up).
//!
//! These tests validate m671 acceptance criteria against synthetic
//! fixtures created via `tempfile::tempdir()` (per m670 T007
//! precedent — no new files under `waybill-cli/tests/fixtures/`).
//!
//! Test coverage (populated by T012, T015, T016, T017, T018):
//! - US1: opt-in mode surfaces .py/.c/.h files as file-tier components
//!   (SC-001, SC-002); C156 annotation present (SC-007)
//! - US2: default mode emits 0 file-tier components on the same fixture
//!   (FR-007 byte-identity)
//! - US3: restriction subset filters correctly (SC-006); fail-loud on
//!   unknown extensions (FR-009); C156 restriction is lex-sorted
//!
//! Cross-linked: `specs/671-file-tier-cpython/{spec,plan,data-model}.md`.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// Create a 10-file synthetic fixture matching T012's spec: 3 `.py`,
/// 3 `.c`, 3 `.h`, and 1 `README.md` (excluded-extension control).
/// Files sit under `<root>/src/` with unique-per-shape content so
/// SHA-256 hashes are non-degenerate.
fn write_mixed_source_fixture(root: &Path) {
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    for (i, name) in ["alpha.py", "beta.py", "gamma.py"].iter().enumerate() {
        std::fs::write(
            src.join(name),
            format!("# waybill-fixture-py {}\nprint(\"{}\")\n", i, name),
        )
        .unwrap();
    }
    for (i, name) in ["one.c", "two.c", "three.c"].iter().enumerate() {
        std::fs::write(
            src.join(name),
            format!(
                "/* waybill-fixture-c {} */\nint main() {{ return {}; }}\n",
                i, i
            ),
        )
        .unwrap();
    }
    for (i, name) in ["one.h", "two.h", "three.h"].iter().enumerate() {
        std::fs::write(
            src.join(name),
            format!("/* waybill-fixture-h {} */\n#define WAYBILL_{} 1\n", i, i),
        )
        .unwrap();
    }
    std::fs::write(
        root.join("README.md"),
        "# waybill-fixture-readme\n\nControl file — should never appear as file-tier.\n",
    )
    .unwrap();
}

fn run_scan(root: &Path, extra_args: &[&str]) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(binary_path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()));
    for a in extra_args {
        cmd.arg(a);
    }
    let result = cmd.output().unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Run the CLI expecting it to FAIL. Returns `(exit_code, stderr)`.
/// Used for the T017 US3 fail-loud tests.
fn run_scan_expecting_failure(root: &Path, extra_args: &[&str]) -> (i32, String) {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(binary_path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()));
    for a in extra_args {
        cmd.arg(a);
    }
    let result = cmd.output().unwrap();
    assert!(
        !result.status.success(),
        "scan was expected to fail but succeeded; stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );
    let code = result.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    (code, stderr)
}

/// Count components whose `type` field equals `"file"`.
fn file_component_count(doc: &Value) -> usize {
    doc.get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("file"))
                .count()
        })
        .unwrap_or(0)
}

/// Extract the C156 doc-scope annotation value (parsed inner object) or `None`.
fn c156_value(doc: &Value) -> Option<Value> {
    let props = doc
        .get("metadata")
        .and_then(|m| m.get("properties"))
        .and_then(|p| p.as_array())?;
    for prop in props {
        let name = prop.get("name").and_then(|n| n.as_str())?;
        if name == "waybill:file-inventory-source-shapes-active" {
            let raw = prop.get("value").and_then(|v| v.as_str())?;
            return serde_json::from_str::<Value>(raw).ok();
        }
    }
    None
}

#[test]
fn source_tree_unrestricted_emits_nine_file_components_with_hashes_and_c156() {
    let dir = tempfile::tempdir().unwrap();
    write_mixed_source_fixture(dir.path());

    let doc = run_scan(dir.path(), &["--file-inventory=source-tree"]);

    // (a) Nine file-tier components (3 .py + 3 .c + 3 .h) — the .md
    // control is excluded by the FR-005 exclusion list even under
    // source-tree mode (FR-002 allowlist is intentionally narrower
    // than the broader exclusion list of docs / configs / build glue).
    assert_eq!(
        file_component_count(&doc),
        9,
        "expected 9 file-tier components (3 .py + 3 .c + 3 .h); doc={doc:#?}",
    );

    // (b) Each file component must carry a SHA-256 hash + at least
    // one `evidence.occurrences[].location` entry.
    let components = doc["components"].as_array().unwrap();
    let mut file_components: Vec<&Value> = components
        .iter()
        .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("file"))
        .collect();
    file_components.sort_by_key(|c| {
        c.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string()
    });
    for comp in &file_components {
        let name = comp.get("name").and_then(|n| n.as_str()).unwrap_or("<unknown>");
        let hashes = comp
            .get("hashes")
            .and_then(|h| h.as_array())
            .unwrap_or_else(|| panic!("file component {name} missing hashes[]"));
        assert!(
            hashes
                .iter()
                .any(|h| h.get("alg").and_then(|a| a.as_str()) == Some("SHA-256")),
            "file component {name} missing SHA-256 hash entry: {hashes:?}",
        );
        // File-tier components carry the source path via the
        // `waybill:file-paths` property (JSON-array string) rather
        // than CDX-native `evidence.occurrences[].location`. See
        // milestone 133 US3 shape at file_tier/emit.rs — the location
        // signal is preserved but routes through `properties[]` per
        // the m133 design (evidence.identity carries the hash-based
        // matching technique instead of a per-occurrence location).
        let props = comp
            .get("properties")
            .and_then(|p| p.as_array())
            .unwrap_or_else(|| panic!("file component {name} missing properties[]"));
        let file_paths_prop = props.iter().find(|p| {
            p.get("name").and_then(|n| n.as_str())
                == Some("waybill:file-paths")
        });
        assert!(
            file_paths_prop.is_some(),
            "file component {name} missing waybill:file-paths property; props={props:?}",
        );
        let raw = file_paths_prop
            .unwrap()
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap();
        let paths: Vec<String> = serde_json::from_str(raw).unwrap();
        assert!(
            !paths.is_empty(),
            "file component {name}: waybill:file-paths is an empty array",
        );
    }

    // (c) Doc-scope C156 annotation present with mode=source-tree,
    // restriction=null (unrestricted).
    let annotation = c156_value(&doc).expect("C156 annotation must be present");
    assert_eq!(
        annotation.get("mode").and_then(|m| m.as_str()),
        Some("source-tree"),
    );
    assert!(
        annotation.get("restriction").is_some_and(|r| r.is_null()),
        "restriction must be null for unrestricted source-tree mode, got {annotation:?}",
    );
}

#[test]
fn orphan_mode_emits_zero_file_components_on_same_fixture() {
    let dir = tempfile::tempdir().unwrap();
    write_mixed_source_fixture(dir.path());

    let doc = run_scan(dir.path(), &["--file-inventory=orphan"]);

    // Default classifier excludes every FR-002 source-code extension
    // AND the .md — nothing survives the excluded-extension check
    // (FR-007 byte-identity for the default path).
    assert_eq!(
        file_component_count(&doc),
        0,
        "orphan mode must emit 0 file-tier components on a source-only fixture; doc={doc:#?}",
    );

    // T015 (US2): C156 must be absent (SC-005 byte-identity — the
    // annotation only fires under source-tree mode). Locks the
    // FR-007 default-mode byte-identity invariant per-run so any
    // future accidental emission trips this test.
    assert!(
        c156_value(&doc).is_none(),
        "C156 annotation must be absent in orphan mode",
    );
}

// -------------------------------------------------------------------
// User Story 3 — shape-subset restriction (P2)
// -------------------------------------------------------------------

#[test]
fn source_tree_with_py_only_restriction_emits_three_components_and_c156_lists_py() {
    // T016 (US3): reuse the T012 synthetic fixture (3 .py + 3 .c + 3 .h
    // + 1 .md), scan with `--file-inventory-source-shapes=py`, and
    // assert that only the 3 .py files land as file-tier components +
    // the C156 restriction is `["py"]` (SC-006, SC-007).
    let dir = tempfile::tempdir().unwrap();
    write_mixed_source_fixture(dir.path());

    let doc = run_scan(
        dir.path(),
        &[
            "--file-inventory=source-tree",
            "--file-inventory-source-shapes=py",
        ],
    );

    assert_eq!(
        file_component_count(&doc),
        3,
        "expected 3 file-tier components (.py only); doc={doc:#?}",
    );

    // Verify the emitted file components are actually the .py ones.
    let components = doc["components"].as_array().unwrap();
    let file_names: Vec<&str> = components
        .iter()
        .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("file"))
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
        .collect();
    for name in &file_names {
        assert!(
            name.ends_with(".py"),
            "restricted mode leaked a non-.py file: {name} (all: {file_names:?})",
        );
    }

    let annotation = c156_value(&doc).expect("C156 annotation must be present");
    assert_eq!(
        annotation.get("mode").and_then(|m| m.as_str()),
        Some("source-tree"),
    );
    let restriction = annotation
        .get("restriction")
        .and_then(|r| r.as_array())
        .unwrap_or_else(|| panic!("restriction must be an array, got {annotation:?}"));
    let restriction_strs: Vec<&str> =
        restriction.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(
        restriction_strs,
        vec!["py"],
        "restriction must be exactly [\"py\"], got {restriction_strs:?}",
    );
}

#[test]
fn source_tree_mixed_order_restriction_emits_lex_sorted_c156_array() {
    // T018 (US3): the `restriction` array in C156 must be lex-sorted
    // regardless of input order. `BTreeSet<SourceShape>` iterates in
    // enum-discriminant order (grouped by language family, NOT lex),
    // so the scan_cmd.rs plumbing explicitly `.sort()`s after
    // `as_str()` conversion — this test locks that invariant.
    let dir = tempfile::tempdir().unwrap();
    write_mixed_source_fixture(dir.path());

    let doc = run_scan(
        dir.path(),
        &[
            "--file-inventory=source-tree",
            // Deliberately supplied in reverse-lex order.
            "--file-inventory-source-shapes=py,h,c",
        ],
    );

    assert_eq!(
        file_component_count(&doc),
        9,
        "expected 9 file-tier components when restriction covers all 3 shapes; doc={doc:#?}",
    );
    let annotation = c156_value(&doc).expect("C156 annotation must be present");
    let restriction = annotation
        .get("restriction")
        .and_then(|r| r.as_array())
        .unwrap_or_else(|| panic!("restriction must be an array, got {annotation:?}"));
    let restriction_strs: Vec<&str> =
        restriction.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(
        restriction_strs,
        vec!["c", "h", "py"],
        "restriction must be lex-sorted, got {restriction_strs:?}",
    );
}

#[test]
fn source_tree_unknown_extension_fails_loudly_with_allowlist_diagnostic() {
    // T017 (US3, part 1): unknown extension via
    // `--file-inventory-source-shapes=md` fails at CLI parse time
    // (exit code 2 per clap's `error::ErrorKind::InvalidValue` default)
    // with a stderr diagnostic listing the FR-002 21-extension
    // allowlist (FR-009).
    let dir = tempfile::tempdir().unwrap();
    write_mixed_source_fixture(dir.path());

    let (code, stderr) = run_scan_expecting_failure(
        dir.path(),
        &[
            "--file-inventory=source-tree",
            "--file-inventory-source-shapes=md",
        ],
    );
    assert_eq!(
        code, 2,
        "expected exit code 2 (clap parse failure), got {code}; stderr={stderr}",
    );
    assert!(
        stderr.contains("unknown source-shape extension"),
        "stderr must name the parse failure kind; got: {stderr}",
    );
    // Diagnostic must list the FR-002 21-extension allowlist so
    // operators can self-correct without reading the docs.
    for ext in ["c", "cpp", "h", "py", "rs", "swift", "ts"] {
        assert!(
            stderr.contains(&format!(" {ext},"))
                || stderr.contains(&format!(" {ext} "))
                || stderr.ends_with(&format!(" {ext}")),
            "stderr must include the {ext:?} entry from the FR-002 allowlist; got: {stderr}",
        );
    }
}

#[test]
fn source_tree_restriction_without_correct_mode_fails_loudly_with_cross_arg_diagnostic() {
    // T017 (US3, part 2): passing `--file-inventory-source-shapes`
    // while `--file-inventory` is NOT `source-tree` must fail with a
    // cross-arg conflict diagnostic (FR-001). The wire-in at
    // scan_cmd.rs constructs this error via `anyhow::anyhow!` after
    // clap parsing, so the exit code is 1 (waybill's own error path),
    // not 2 (clap parse failure).
    let dir = tempfile::tempdir().unwrap();
    write_mixed_source_fixture(dir.path());

    let (code, stderr) = run_scan_expecting_failure(
        dir.path(),
        &[
            "--file-inventory=orphan",
            "--file-inventory-source-shapes=py",
        ],
    );
    assert_ne!(code, 0, "expected non-zero exit; stderr={stderr}");
    assert!(
        stderr.contains("--file-inventory-source-shapes")
            && stderr.contains("source-tree"),
        "cross-arg diagnostic must name both flags; got: {stderr}",
    );
}
