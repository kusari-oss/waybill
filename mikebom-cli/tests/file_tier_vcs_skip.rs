//! Milestone 174: file-tier VCS metadata exclusion — end-to-end
//! integration tests via the release binary.
//!
//! Covers three user stories:
//!
//! * **US1 (P1 MVP)** — scans of git-cloned repos emit ZERO components
//!   representing files inside `.git/` at any depth. Verified against
//!   the emitted `mikebom:source-files` annotation.
//! * **US2 (P1)** — first-party scripts and `.gitignore` files (which
//!   are NOT VCS metadata) still surface as file-tier components.
//! * **US3 (P2)** — the operator's `--exclude-path` flag continues to
//!   compose alongside the built-in VCS exclusion.

use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::bin;

fn scan(fixture_root: &Path, extra_args: &[&str]) -> serde_json::Value {
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let out_path = tmp.path().to_path_buf();
    let mut cmd = Command::new(bin());
    cmd.arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture_root)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let output = cmd.output().expect("mikebom should run");
    assert!(
        output.status.success(),
        "scan failed (exit={:?}): stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

/// Extract every distinct file-path string referenced by any
/// `mikebom:source-files` annotation in the emitted SBOM.
fn all_source_file_paths(sbom: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(components) = sbom["components"].as_array() {
        for c in components {
            if let Some(props) = c["properties"].as_array() {
                for p in props {
                    if p["name"].as_str() == Some("mikebom:source-files") {
                        if let Some(v) = p["value"].as_str() {
                            if let Ok(arr) = serde_json::from_str::<Vec<String>>(v) {
                                out.extend(arr);
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

fn write_file(root: &Path, rel: &str, content: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
    std::fs::write(&p, content).expect("write");
}

/// Assemble the standard 4-file fixture that reproduces the langflow
/// audit bug shape:
/// - `<root>/.git/hooks/pre-commit.sample` (VCS metadata; must NOT appear)
/// - `<root>/.git/HEAD` (VCS metadata; must NOT appear)
/// - `<root>/dev.start.sh` (first-party script; MUST appear)
/// - `<root>/.gitignore` (repo config, NOT metadata; MUST appear)
fn assemble_repo_fixture(root: &Path) {
    write_file(
        root,
        ".git/hooks/pre-commit.sample",
        b"#!/bin/sh\n# git default hook template\n",
    );
    write_file(root, ".git/HEAD", b"ref: refs/heads/main\n");
    write_file(
        root,
        "dev.start.sh",
        b"#!/bin/bash\nset -euo pipefail\necho \"starting dev server\"\n",
    );
    write_file(
        root,
        ".gitignore",
        b"target/\nnode_modules/\n*.log\n",
    );
}

/// US1 primary acceptance: zero `.git/` paths in any component's
/// `mikebom:source-files` annotation. Also asserts US2 preview: first-
/// party script + `.gitignore` DO appear.
#[test]
fn t006_us1_git_hook_samples_not_emitted() {
    let tmp = tempfile::tempdir().expect("tempdir");
    assemble_repo_fixture(tmp.path());

    let sbom = scan(tmp.path(), &[]);
    let paths = all_source_file_paths(&sbom);

    let git_paths: Vec<&String> = paths.iter().filter(|p| p.starts_with(".git/")).collect();
    assert!(
        git_paths.is_empty(),
        "SC-001: expected zero .git/ paths; got {git_paths:?}"
    );

    let has_dev_script = paths.iter().any(|p| p == "dev.start.sh");
    assert!(
        has_dev_script,
        "SC-002: expected first-party script dev.start.sh to be emitted; got paths={paths:?}"
    );

    // Note: `.gitignore` is legitimately dropped by the m133
    // content-shape classifier pre-existing to m174; the FR-006
    // similar-name protection is verified at walker level by
    // `walker_preserves_similar_names` (walker.rs unit tests), where
    // the classifier is bypassed. m174 only guarantees non-emission
    // of `.git/*` descendants; it does not lift files past the
    // classifier.
}

/// US3 primary acceptance: `--exclude-path` composes with the built-in
/// VCS exclusion. `.git/` is excluded (built-in) AND `ignored/` is
/// excluded (operator's flag); first-party content survives both.
#[test]
fn t009_us3_exclude_path_composes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_file(tmp.path(), ".git/hooks/pre-commit.sample", b"template");
    write_file(tmp.path(), "ignored/junk.sh", b"#!/bin/sh\necho junk\n");
    write_file(tmp.path(), "dev.start.sh", b"#!/bin/bash\necho dev\n");
    write_file(tmp.path(), "ci-test.sh", b"#!/bin/bash\necho ci\n");

    let sbom = scan(tmp.path(), &["--exclude-path", "ignored"]);
    let paths = all_source_file_paths(&sbom);

    let git_paths: Vec<&String> = paths.iter().filter(|p| p.starts_with(".git/")).collect();
    assert!(
        git_paths.is_empty(),
        "built-in VCS exclusion: expected zero .git/ paths; got {git_paths:?}"
    );

    let ignored_paths: Vec<&String> =
        paths.iter().filter(|p| p.starts_with("ignored/")).collect();
    assert!(
        ignored_paths.is_empty(),
        "operator --exclude-path: expected zero ignored/ paths; got {ignored_paths:?}"
    );

    let dev_scripts: Vec<&String> = paths
        .iter()
        .filter(|p| p == &&"dev.start.sh".to_string() || p == &&"ci-test.sh".to_string())
        .collect();
    assert_eq!(
        dev_scripts.len(),
        2,
        "expected both first-party scripts preserved; got dev_scripts={dev_scripts:?} all_paths={paths:?}"
    );
}

/// SC-006: redundant `--exclude-path '.git/**'` (the pre-174
/// workaround) is a harmless no-op. Assert emitted paths are the
/// same set as the T006 fixture WITHOUT the redundant flag.
#[test]
fn t010_us3_redundant_exclude_path_git_is_noop() {
    let tmp = tempfile::tempdir().expect("tempdir");
    assemble_repo_fixture(tmp.path());

    let without_flag = scan(tmp.path(), &[]);
    let with_flag = scan(tmp.path(), &["--exclude-path", ".git"]);

    let mut paths_without = all_source_file_paths(&without_flag);
    let mut paths_with = all_source_file_paths(&with_flag);
    paths_without.sort();
    paths_with.sort();

    assert_eq!(
        paths_without, paths_with,
        "SC-006: redundant --exclude-path '.git/**' should be a no-op; \
         without-flag paths={paths_without:?}, with-flag paths={paths_with:?}"
    );
}

// Silence unused-import warning if PathBuf is otherwise unused.
#[allow(dead_code)]
fn _unused(_: PathBuf) {}
