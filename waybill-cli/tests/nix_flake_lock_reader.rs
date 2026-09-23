//! Milestone 925 — `flake.lock` reader, end to end.
//!
//! These run the real binary against fixture trees. That is a different
//! guarantee from the unit tests in `scan_fs::package_db::nix`, which exercise
//! the parser and emission logic directly: these also cover registration,
//! walker dispatch, and the emission pipeline, any of which can be broken while
//! the module's own tests stay green.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> &'static str { env!("CARGO_BIN_EXE_waybill") }

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix").join(name)
}

fn scan(root: &Path) -> serde_json::Value {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!("nix-rd-{}-{seq}.json", std::process::id()));
    let st = Command::new(binary_path())
        .args(["sbom", "scan", "--path", root.to_str().unwrap(), "--offline",
               "--format", "cyclonedx-json", "--output", out.to_str().unwrap()])
        .status().unwrap();
    assert!(st.success(), "scan failed: {st:?}");
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

fn purls(v: &serde_json::Value) -> BTreeSet<String> {
    v["components"].as_array().unwrap().iter()
        .filter_map(|c| c.get("purl").and_then(|p| p.as_str()).map(str::to_string))
        .collect()
}

fn props(v: &serde_json::Value, purl_prefix: &str, name: &str) -> Vec<String> {
    v["components"].as_array().unwrap().iter()
        .filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with(purl_prefix)))
        .flat_map(|c| c["properties"].as_array().map(|a| a.to_vec()).unwrap_or_default())
        .filter(|p| p["name"].as_str() == Some(name))
        .filter_map(|p| p["value"].as_str().map(str::to_string))
        .collect()
}

/// C-1 — one component per identifiable locked input, and none for the root.
#[test]
fn every_pinned_input_becomes_exactly_one_component() {
    let p = purls(&scan(&fixture("single-github")));
    let nix: Vec<_> = p.iter().filter(|s| s.starts_with("pkg:github/")).collect();
    assert_eq!(nix.len(), 1, "one input is pinned; got {nix:?}");
    assert!(
        !p.iter().any(|s| s.contains("/root@") || s.ends_with("/root")),
        "the root node is the project, not an input — it must not emit. purls={p:?}"
    );
}

/// C-2 — a github input is host-typed, and canonical.
///
/// Asserts the identifier byte-for-byte. `pkg:github` namespace and name are
/// `case_sensitive: false` in the published type definition and shall be
/// lowercased; a lockfile writing `NixOS` must not produce an identifier that
/// fails to join with one writing `nixos`.
#[test]
fn a_github_input_emits_a_canonical_host_typed_purl() {
    let p = purls(&scan(&fixture("single-github")));
    assert!(
        p.contains("pkg:github/nixos/nixpkgs@a799d3e3886da994fa307f817a6bc705ae538eeb"),
        "expected the canonical lowercased identifier; the fixture writes `NixOS`. purls={p:?}"
    );
}

/// C-2 second arm — a `tarball` input has no owner/repo but does have a rev, so
/// it is identifiable rather than skippable. Measured on 2 of 5 real lockfiles.
#[test]
fn a_tarball_input_is_identified_via_generic_not_dropped() {
    let p = purls(&scan(&fixture("tarball-input")));
    assert!(
        p.contains("pkg:generic/nixpkgs@a32edd7654519351e48e80372a928df336394670"),
        "a tarball input carries a rev and must still be identified. purls={p:?}"
    );
}

/// C-3 — the component's version IS the locked revision, not `lastModified`
/// and not a truncation. A different slot from the identifier.
#[test]
fn the_component_version_is_the_locked_revision_verbatim() {
    let rep = scan(&fixture("single-github"));
    let v: Vec<_> = rep["components"].as_array().unwrap().iter()
        .filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with("pkg:github/")))
        .filter_map(|c| c["version"].as_str())
        .collect();
    assert_eq!(v, vec!["a799d3e3886da994fa307f817a6bc705ae538eeb"]);
}

/// FR-013c — no `pkg:nix` identifier is invented while the upstream type is
/// unresolved. A prohibition nothing tests is one that regresses silently.
#[test]
fn no_pkg_nix_identifier_is_emitted() {
    for f in ["single-github", "tarball-input", "follows-alias"] {
        let p = purls(&scan(&fixture(f)));
        assert!(
            !p.iter().any(|s| s.starts_with("pkg:nix")),
            "{f}: purl-spec has no `nix` type and whether a canonical one is \
             expressible is unresolved upstream. purls={p:?}"
        );
    }
}

/// C-4 — the NAR hash is annotated, never a native checksum.
#[test]
fn the_nar_hash_is_annotated_and_no_native_hash_is_emitted() {
    let rep = scan(&fixture("single-github"));
    let native: usize = rep["components"].as_array().unwrap().iter()
        .filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with("pkg:github/")))
        .map(|c| c["hashes"].as_array().map(|a| a.len()).unwrap_or(0))
        .sum();
    assert_eq!(
        native, 0,
        "a narHash is SRI base64 over a NAR serialization of a directory tree, \
         not a hash of the component's bytes, and native checksum fields expect \
         hex — filling one would be a false statement a verifier would act on"
    );
    assert_eq!(
        props(&rep, "pkg:github/", "waybill:nix-nar-hash"),
        vec!["sha256-3av0pIjlOWQ6rDbNOmpUSvbNnJkGORQKKjb4LtCZsIY="],
        "the SRI prefix is preserved so the value stays self-describing"
    );
}

/// FR-004 — a `follows` alias names an existing pin and must not mint a second
/// component. Measured as ordinary rather than exotic: 2 of 6 inputs on a real
/// repository are aliases.
#[test]
fn a_follows_alias_adds_an_edge_not_a_component() {
    let p = purls(&scan(&fixture("follows-alias")));
    let nix: Vec<_> = p.iter().filter(|s| s.starts_with("pkg:github/")).collect();
    assert_eq!(
        nix.len(), 2,
        "two pins are declared and one alias names one of them; a third \
         component means the alias was treated as a pin. got {nix:?}"
    );
}

/// FR-006 / FR-007a companion — pin state is emitted for every moving input,
/// and the ref name only where one was written.
#[test]
fn the_pin_state_says_whether_relocking_would_move_the_input() {
    let rep = scan(&fixture("single-github"));
    assert_eq!(props(&rep, "pkg:github/", "waybill:nix-original-pin-state"), vec!["branch-or-tag"]);
    assert_eq!(props(&rep, "pkg:github/", "waybill:nix-original-ref"), vec!["nixos-unstable"]);
}

/// SC-006 — two scans of one tree emit the same components in the same order.
/// `nodes` is a JSON object and its iteration order is not a guarantee; this is
/// the failure mode #948 had, where two scans of unchanged source differed in
/// bytes because an emitter inherited an unstable order.
#[test]
fn two_scans_of_one_tree_agree() {
    let f = fixture("follows-alias");
    let a = scan(&f); let b = scan(&f);
    let seq = |v: &serde_json::Value| -> Vec<String> {
        v["components"].as_array().unwrap().iter()
            .filter_map(|c| c.get("purl").and_then(|p| p.as_str()).map(str::to_string))
            .collect()
    };
    assert_eq!(seq(&a), seq(&b));
}

/// FR-011 — a lockfile governs its own directory. Two independent flakes in one
/// tree yield two independent input sets; neither speaks for the other.
#[test]
fn two_flakes_in_one_tree_are_scoped_independently() {
    let p = purls(&scan(&fixture("two-flakes")));
    let nix: Vec<_> = p.iter().filter(|s| s.starts_with("pkg:github/")).collect();
    assert_eq!(
        nix.len(), 2,
        "each flake pins nixpkgs at a different revision; both must survive. \
         One winning would be the #938 defect in a new place. got {nix:?}"
    );
}
