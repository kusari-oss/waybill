//! Milestone 925 — what a bad or absent `flake.lock` must NOT do.
//!
//! The property behind all of these: reading a lockfile may add information and
//! may decline to, but must never subtract. #937 and #938 were both violations
//! of exactly that in the Haskell reader — adding a lockfile made the document
//! smaller.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> &'static str { env!("CARGO_BIN_EXE_waybill") }

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix").join(name)
}

fn scan_at(root: &Path, offline: bool) -> serde_json::Value {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!("nix-fm-{}-{seq}.json", std::process::id()));
    let mut cmd = Command::new(binary_path());
    cmd.args(["sbom", "scan", "--path", root.to_str().unwrap(),
              // This suite's subject is the `flake.lock` READER, which takes
              // no network. Milestone 926 added a separate subsystem that
              // resolves Haskell versions through the pinned nixpkgs and does
              // retrieve — so an online scan here would drag 16 MB of an
              // unrelated feature into a test about this one, and make it
              // depend on a forge being reachable. Disabled explicitly rather
              // than by `--offline`, which would also change what the online
              // leg below is testing.
              "--no-nixpkgs-haskell",
              "--format", "cyclonedx-json", "--output", out.to_str().unwrap()]);
    if offline { cmd.arg("--offline"); }
    let st = cmd.status().unwrap();
    assert!(st.success(), "scan failed: {st:?}");
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

fn scan(root: &Path) -> serde_json::Value { scan_at(root, true) }

fn names(v: &serde_json::Value) -> BTreeSet<String> {
    v["components"].as_array().unwrap().iter()
        .map(|c| c["name"].as_str().unwrap_or_default().to_string()).collect()
}

/// A tree with a Haskell project and a flake, built twice: with the lockfile
/// and without it. Used to prove the lockfile only ever adds.
fn tree_with_optional_lock(include_lock: bool) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("app.cabal"),
        "name: waybill-fixture-app\nversion: 1.0\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends: waybill-fixture-alpha, waybill-fixture-beta\n",
    ).unwrap();
    if include_lock {
        std::fs::copy(fixture("single-github").join("flake.lock"), d.path().join("flake.lock")).unwrap();
    }
    d
}

/// C-7 / FR-010 — a malformed lockfile emits nothing from itself and leaves
/// every other ecosystem untouched.
#[test]
fn a_malformed_lockfile_does_not_disturb_other_ecosystems() {
    let with = tempfile::tempdir().unwrap();
    std::fs::write(
        with.path().join("app.cabal"),
        "name: waybill-fixture-app\nversion: 1.0\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends: waybill-fixture-alpha\n",
    ).unwrap();
    let without = names(&scan(with.path()));

    std::fs::copy(fixture("malformed").join("flake.lock"), with.path().join("flake.lock")).unwrap();
    let withbad = names(&scan(with.path()));

    assert_eq!(
        without, withbad,
        "a malformed flake.lock must warn and continue, changing nothing else. \
         lost={:?} gained={:?}",
        without.difference(&withbad).collect::<Vec<_>>(),
        withbad.difference(&without).collect::<Vec<_>>()
    );
}

/// FR-010 — an unrecognised schema version is reported, not parsed on
/// optimistic assumptions. Emits no inputs rather than guessing at a format it
/// does not know.
#[test]
fn an_unrecognised_schema_version_emits_no_inputs() {
    let n = names(&scan(&fixture("unknown-version")));
    assert!(
        !n.contains("nixpkgs"),
        "version 99 is not version 7; parsing it anyway would produce components \
         that look authoritative and may be wrong. names={n:?}"
    );
}

/// A lockfile that pins nothing is a truthful statement, not an error.
#[test]
fn a_lockfile_with_no_inputs_is_not_an_error() {
    let n = names(&scan(&fixture("no-inputs")));
    assert!(n.is_empty() || !n.iter().any(|x| x == "nixpkgs"), "names={n:?}");
}

/// SC-005 — the property #937 and #938 were both violations of: adding a
/// lockfile must never REMOVE components.
#[test]
fn adding_a_flake_lock_never_removes_components() {
    let without = names(&scan(tree_with_optional_lock(false).path()));
    let with = names(&scan(tree_with_optional_lock(true).path()));

    let lost: Vec<_> = without.difference(&with).collect();
    assert!(
        lost.is_empty(),
        "adding a flake.lock deleted components: {lost:?}. Pinning a build must \
         not make its SBOM worse than not pinning it"
    );
    assert!(
        with.contains("nixpkgs"),
        "and it should ADD the pinned input. names={with:?}"
    );
}

/// SC-002 — no network and no Nix installation. Both clauses: the output is
/// identical with and without `--offline`, and the scan succeeds with `nix`
/// absent from PATH.
#[test]
fn the_scan_needs_neither_network_nor_a_nix_installation() {
    let d = tree_with_optional_lock(true);
    let off = names(&scan_at(d.path(), true));
    let on = names(&scan_at(d.path(), false));
    assert_eq!(off, on, "offline and online must agree — the reader takes no network");

    // `nix` absent from PATH. On a developer machine that HAS nix, an
    // accidental dependency on it would pass every other check here.
    let out = std::env::temp_dir().join(format!("nix-nopath-{}.json", std::process::id()));
    let st = Command::new(binary_path())
        .args(["sbom", "scan", "--path", d.path().to_str().unwrap(), "--offline",
               "--no-nixpkgs-haskell",
               "--format", "cyclonedx-json", "--output", out.to_str().unwrap()])
        .env("PATH", "/nonexistent")
        .status().unwrap();
    assert!(st.success(), "the reader must not shell out to nix: {st:?}");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert!(names(&v).contains("nixpkgs"));
}
