#[allow(unused_imports)]
use std::path::{Path, PathBuf};
#[allow(unused_imports)]
use crate::corpus_harness_195::harness::CorpusInfraError;
#[allow(unused_imports)]
use crate::corpus_harness_195::manifest::{CorpusTarget, PinnedRef, SourceKind};
use crate::corpus_harness_195::cache::nixpkgs_pin;

fn write(text: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("flake.lock"), text).unwrap();
    d
}

/// The real shape of `haskell/haskell-language-server`'s lock, trimmed:
/// a `root` node with no `locked` block, then the inputs. An earlier
/// draft used `?` on `node.get("locked")`, which returns from the whole
/// function at `root` -- hydrating nothing and making every corpus
/// assertion pass vacuously.
#[test]
fn root_node_without_a_locked_block_does_not_abort_the_search() {
    let d = write(
        r#"{"nodes":{
             "root":{"inputs":{"nixpkgs":"nixpkgs"}},
             "flake-utils":{"locked":{"type":"github","owner":"numtide",
               "repo":"flake-utils","rev":"1170dc"}},
             "nixpkgs":{"locked":{"type":"github","owner":"NixOS",
               "repo":"nixpkgs","rev":"cbb5cf35"},
               "original":{"owner":"NixOS","repo":"nixpkgs","ref":"nixpkgs-unstable"}}
           },"root":"root","version":7}"#,
    );
    assert_eq!(
        nixpkgs_pin(&d.path().join("flake.lock")),
        Some(("NixOS".into(), "nixpkgs".into(), "cbb5cf35".into()))
    );
}

/// A lock with no nixpkgs input hydrates nothing, rather than erroring.
#[test]
fn a_lock_without_nixpkgs_yields_nothing() {
    let d = write(
        r#"{"nodes":{"root":{"inputs":{}},
             "flake-utils":{"locked":{"type":"github","owner":"numtide",
               "repo":"flake-utils","rev":"1170dc"}}},"root":"root","version":7}"#,
    );
    assert_eq!(nixpkgs_pin(&d.path().join("flake.lock")), None);
}

/// A moving reference pins no revision, so there is nothing to fetch.
#[test]
fn a_lock_with_no_revision_yields_nothing() {
    let d = write(
        r#"{"nodes":{"root":{"inputs":{"nixpkgs":"nixpkgs"}},
             "nixpkgs":{"locked":{"type":"github","owner":"NixOS",
               "repo":"nixpkgs","rev":""}}},"root":"root","version":7}"#,
    );
    assert_eq!(nixpkgs_pin(&d.path().join("flake.lock")), None);
}

/// Absent file: the ordinary case for every non-Nix target.
#[test]
fn a_missing_lock_yields_nothing() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(nixpkgs_pin(&d.path().join("flake.lock")), None);
}
