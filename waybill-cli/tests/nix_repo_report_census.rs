//! Milestone 925 SC-004 — a Nix repository's files are attributable.
//!
//! Note what this does NOT assert: that `files_unclaimed` drops. That count
//! uses the milestone-924 sole-claimant rule, and `go_binary` registers `**/*`,
//! so every file has at least two claimants and the count is unmoved by any
//! reader. SC-004 was originally written against that number and was amended
//! once measurement showed it could never move.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

fn binary_path() -> &'static str { env!("CARGO_BIN_EXE_waybill") }

fn report(root: &Path) -> serde_json::Value {
    let out = std::env::temp_dir().join(format!("nix-rep-{}.json", std::process::id()));
    let st = Command::new(binary_path())
        .args(["repo", "report", "--path", root.to_str().unwrap(),
               "--output", out.to_str().unwrap()])
        .status().unwrap();
    assert!(st.success(), "repo report failed: {st:?}");
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

fn nix_tree() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix/single-github/flake.lock");
    std::fs::copy(&src, d.path().join("flake.lock")).unwrap();
    // The expression files this feature deliberately does not read.
    for f in ["flake.nix", "default.nix", "shell.nix"] {
        std::fs::write(d.path().join(f), "{ }\n").unwrap();
    }
    d
}

/// FR-014 — the reader claims `flake.lock` during the walk.
#[test]
fn the_nix_reader_matches_the_lockfile() {
    let d = nix_tree();
    let r = report(d.path());
    let matched = r["readers"].as_array().unwrap().iter()
        .find(|x| x["reader_id"].as_str() == Some("nix"))
        .and_then(|x| x["files_matched"].as_u64());
    assert_eq!(
        matched, Some(1),
        "the nix reader must appear in the census having matched the lockfile; \
         absence means the registration did not take. readers={:?}",
        r["readers"]
    );
}

/// SC-004 — the Nix files this feature does NOT read are still attributed to a
/// named ecosystem, so they read as "recognised, not parsed" rather than as
/// files waybill has never heard of.
#[test]
fn unread_nix_expression_files_are_attributed_to_the_nix_ecosystem() {
    let d = nix_tree();
    let r = report(d.path());
    let nix_markers = r["directories"].as_array().unwrap().iter()
        .flat_map(|dir| dir["ecosystems"].as_array().map(|a| a.to_vec()).unwrap_or_default())
        .filter(|e| e["ecosystem"].as_str() == Some("nix"))
        .count();
    assert!(
        nix_markers >= 3,
        "flake.nix, default.nix and shell.nix are Nix expressions this milestone \
         does not evaluate — being unread is fine, being unrecognisable is not. \
         got {nix_markers} nix markers"
    );
}
