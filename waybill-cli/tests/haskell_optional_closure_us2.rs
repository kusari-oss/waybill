//! Milestone 1273 US2 (#1281) — an optional mark is withdrawn only on
//! evidence.
//!
//! The `.cabal` reader marks a dependency `Optional` when the only stanza
//! declaring it is excluded under the manifest's declared flag defaults
//! (milestone 955 / PR #1030). Measured at 0.14.0, that mark was withdrawn
//! whenever a `flake.lock` pinned nixpkgs — which for a Nix-based Haskell
//! project is always — because the runtime closure used to justify the
//! withdrawal is **seeded from the declared dependency set**, and that set
//! contains the very components whose marks are under review. Every one of
//! them was therefore "reached" by construction, and the closure could only
//! confirm the marks it was handed.
//!
//! # Cache isolation (mandatory)
//!
//! **No test here reads the host's `~/.cache/waybill/nixpkgs/`.** A test that
//! did would pass on a developer's machine and fail on CI, which has no such
//! cache, and Principle VII forbids it. Each test seeds a
//! `tempfile::tempdir()` with the synthetic package set below and points
//! `WAYBILL_NIXPKGS_CACHE` at it — the pattern established by
//! `nix_haskell_resolution_m926.rs`. For the "no cache" case the variable is
//! pointed at a non-existent directory rather than unset, because an unset
//! value falls back to the real `$HOME`.
//!
//! No test here touches the network, and none is `#[ignore]`d: an ignored
//! test is one nobody runs, and this project has already been bitten by a
//! suite that was green because it was not looking (#949).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// The revision the fixtures' `flake.lock` pins. It is also the directory
/// name the seeded cache uses, so the two cannot drift apart.
const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";

/// The US2 graph, as nixpkgs would record it. Synthetic names throughout.
///
/// ```text
///   kept      -> contested          (built: declared by the library)
///   marked-a  -> fanout -> deep     (marked: disabled stanza only)
///   marked-b                        (marked: disabled stanza only)
/// ```
///
/// `contested` is declared by BOTH the library (through `kept`'s relations)
/// and the disabled stanza, so a closure seeded without the marked set still
/// reaches it — the one case where a withdrawal is justified.
///
/// `fanout` and `deep` are reachable only through `marked-a`. They carry no
/// mark of their own, so nothing withdraws them; they are here because Probe
/// D measured that restoring one mark confers runtime reachability on
/// everything below it, and a test that only checked the directly-marked
/// components would pass with that blast radius intact.
const PACKAGES: &str = r#"
  waybill-fixture-kept = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-kept";
      version = "1.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-contested ];
  }) { };
  waybill-fixture-marked-a = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-marked-a";
      version = "2.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-fanout ];
  }) { };
  waybill-fixture-marked-b = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-marked-b";
      version = "3.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
  }) { };
  waybill-fixture-contested = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-contested";
      version = "4.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
  }) { };
  waybill-fixture-fanout = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-fanout";
      version = "5.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-deep ];
  }) { };
  waybill-fixture-deep = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-deep";
      version = "6.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
  }) { };
"#;

const CONFIG: &str = "self: super: {\n}\n";

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/haskell_hpack")
        .join(name)
}

/// Seed a per-revision cache in a tempdir, so the resolved path runs with no
/// network and no dependency on host state.
fn seed_cache() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let rev_dir = dir.path().join(REV);
    std::fs::create_dir_all(&rev_dir).unwrap();
    std::fs::write(rev_dir.join("hackage-packages.nix"), PACKAGES).unwrap();
    // The override file (#1033). Absent, the pass logs
    // `configuration-common.nix unavailable` and resolves on the generated
    // set alone -- which still works, but seeding it keeps the fixture a
    // faithful miniature of a real revision.
    std::fs::write(rev_dir.join("configuration-common.nix"), CONFIG).unwrap();
    // Exactly the series the fixtures' `flake.nix` names. `candidate_series`
    // returns every KNOWN_GHC_SERIES when no `flake.nix` is present, and an
    // offline miss on ANY candidate degrades the whole pass (#975) -- so a
    // fixture without that file makes every assertion here vacuous, which is
    // how the first version of this suite reported T032 green before the fix
    // existed.
    std::fs::write(rev_dir.join("configuration-ghc-9.6.x.nix"), CONFIG).unwrap();
    dir
}

struct Scan {
    doc: Value,
    stderr: String,
}

/// `cache: None` points `WAYBILL_NIXPKGS_CACHE` at a directory that does not
/// exist. Deliberately not "unset": an unset value falls back to the real
/// `$HOME`, which is the thing this suite must never read.
fn scan(root: &Path, cache: Option<&Path>) -> Scan {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(binary_path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .env("RUST_LOG", "info");
    match cache {
        Some(p) => cmd.env("WAYBILL_NIXPKGS_CACHE", p),
        None => cmd.env("WAYBILL_NIXPKGS_CACHE", out_dir.path().join("__no_cache__")),
    };
    let result = cmd.output().unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(&out_path).unwrap();
    Scan {
        doc: serde_json::from_slice(&bytes).unwrap(),
        stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
    }
}

fn component<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    doc.get("components")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
}

fn prop(doc: &Value, name: &str, key: &str) -> Option<String> {
    component(doc, name)?
        .get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(key))
        .and_then(|p| p.get("value"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// `(lifecycle-scope, optional-derivation)` for one component, with `<absent>`
/// standing in for a missing annotation so a disagreement prints readably.
fn classification(doc: &Value, name: &str) -> (String, String) {
    (
        prop(doc, name, "waybill:lifecycle-scope").unwrap_or_else(|| "<absent>".into()),
        prop(doc, name, "waybill:optional-derivation").unwrap_or_else(|| "<absent>".into()),
    )
}

fn doc_prop(doc: &Value, key: &str) -> Option<String> {
    doc.pointer("/metadata/properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(key))
        .and_then(|p| p.get("value"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// The nixpkgs document-scope resolution record, parsed. Assertions go
/// through this rather than through substring matching on the whole
/// document: US1's mutation M10 survived a substring assertion that was
/// being satisfied by a fixture directory's name.
fn resolution_record(doc: &Value) -> Option<Value> {
    for key in [
        "waybill:nixpkgs-haskell-resolution",
        "waybill:haskell-closure",
        "waybill:nixpkgs-haskell-closure",
    ] {
        if let Some(raw) = doc_prop(doc, key) {
            if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                return Some(v);
            }
        }
    }
    None
}

// ───────────────────────────────────────────────────────────────────────
// T032 / C11 / SC-003 — the mark survives when only the seed reaches it
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us2_mark_survives_when_only_the_seed_reaches_it() {
    let cache = seed_cache();
    let with_lock = scan(&fixture("optional_closure"), Some(cache.path()));

    for name in ["waybill-fixture-marked-a", "waybill-fixture-marked-b"] {
        let (scope, deriv) = classification(&with_lock.doc, name);
        assert_eq!(
            (scope.as_str(), deriv.as_str()),
            ("optional", "cabal-flag-disabled-stanza"),
            "{name} is declared only by a `buildable: False` stanza and is \
             reached by nothing outside the marked set, so its mark must \
             survive the closure. Measured at 0.14.0: both were restored to \
             runtime, because the closure was seeded from the set it was \
             asked to confirm."
        );
    }
}

// ───────────────────────────────────────────────────────────────────────
// T033 / C11 — the positive control: a justified withdrawal still happens
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us2_mark_is_withdrawn_on_genuine_non_seed_reach() {
    let cache = seed_cache();
    let s = scan(&fixture("optional_closure"), Some(cache.path()));

    // `contested` is declared by the disabled stanza AND reached through
    // `kept`, which the library declares and which is built. Without this
    // control the fix could be "never withdraw anything", which would pass
    // T032 and be just as wrong in the other direction.
    let (scope, deriv) = classification(&s.doc, "waybill-fixture-contested");
    assert_eq!(
        scope, "<absent>",
        "contested is genuinely reachable from a built dependency, so the \
         mark is withdrawn and no optional scope remains; got \
         scope={scope}, derivation={deriv}"
    );
    assert_eq!(
        deriv, "<absent>",
        "and the derivation goes with it (FR-009's converse)"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T034 / C12 / FR-007 — a withdrawal names what justified it
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us2_withdrawal_names_its_justification() {
    let cache = seed_cache();
    let s = scan(&fixture("optional_closure"), Some(cache.path()));
    let record = resolution_record(&s.doc)
        .expect("the nixpkgs pass emits a document-scope record");
    let rendered = record.to_string();
    assert!(
        rendered.contains("waybill-fixture-contested"),
        "the withdrawal must name the component it applied to; record was \
         {rendered}"
    );
    assert!(
        rendered.contains("waybill-fixture-kept"),
        "and must name the non-seed parent whose closure edge justified it, \
         in enough detail that an operator can tell which path was taken; \
         record was {rendered}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T035 / C13 / FR-008 — three states, not one
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us2_ran_and_withdrew_nothing_differs_from_did_not_run() {
    // (a) did not run: no flake.lock, so the pass never opens.
    let no_lock = scan(&fixture("optional_closure_nolock"), None);
    // (b) ran and withdrew something: the full fixture.
    let cache = seed_cache();
    let ran = scan(&fixture("optional_closure"), Some(cache.path()));

    let a = resolution_record(&no_lock.doc);
    let b = resolution_record(&ran.doc)
        .expect("the pass ran, so it must leave a record");

    // These are the same output today -- `restored=0`, or no record at all --
    // which is the condition under which a feature that CANNOT fire looks
    // like a feature with nothing to do. Milestone 973 exists because
    // milestone 926 made exactly this mistake with the closure summary.
    assert_ne!(
        a.as_ref().map(|v| v.to_string()),
        Some(b.to_string()),
        "a scan where the reconciliation did not run must be \
         distinguishable in the document from one where it ran"
    );
    assert!(
        b.to_string().contains("reconcil") || b.to_string().contains("withdraw"),
        "and the record that ran must say so in its own terms; got {b}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T036 / C14 / FR-009 — a declined withdrawal keeps BOTH
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us2_declined_withdrawal_keeps_scope_and_derivation() {
    let cache = seed_cache();
    let s = scan(&fixture("optional_closure"), Some(cache.path()));

    // Restoring one without the other publishes a scope with no account of
    // where it came from, so both halves are asserted on the same component.
    let (scope, deriv) = classification(&s.doc, "waybill-fixture-marked-a");
    assert_eq!(scope, "optional", "the scope stands");
    assert_eq!(
        deriv, "cabal-flag-disabled-stanza",
        "and so does the derivation that explains it"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T037 / FR-006 — the blast radius, not just the marked pair
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us2_fanout_below_a_marked_component_is_not_published_as_runtime() {
    let cache = seed_cache();
    let s = scan(&fixture("optional_closure"), Some(cache.path()));

    // Probe D measured 8 non-boot components reachable from the root ONLY
    // through the marked pair on the public library #955 was filed against.
    // Each has a parent that is not itself marked, so a test checking only
    // the directly-marked components would pass with all 8 still published
    // as runtime dependencies of a build that does not contain them.
    for name in ["waybill-fixture-fanout", "waybill-fixture-deep"] {
        if component(&s.doc, name).is_none() {
            // Not emitted at all is an acceptable answer: it is reachable
            // only through something not in the default build.
            continue;
        }
        let (scope, _) = classification(&s.doc, name);
        assert_ne!(
            scope, "<absent>",
            "{name} is reachable only through a component whose optional \
             mark survives, so it must not be published as an unqualified \
             runtime dependency. Scope was {scope}."
        );
    }
}

// ───────────────────────────────────────────────────────────────────────
// FR-020 — the signal is logged as well as emitted
// ───────────────────────────────────────────────────────────────────────

/// FR-018 and FR-020 are two requirements, not one.
///
/// FR-018 puts every signal in the document, and SC-009 insists the document
/// assertions be made *without* consulting the log — which is why every other
/// test in this suite reads the emitted JSON. But FR-020 separately requires
/// the signal be logged, "so an operator can act before the SBOM is published
/// rather than only discover it afterwards", and that half needs its own
/// coverage or it is an unverified requirement.
///
/// Mutation M31 found exactly that hole: demoting the reconciliation's
/// `info!` to `debug!` changed nothing any test observed.
#[test]
fn m1273_us2_reconciliation_is_logged_at_info() {
    let cache = seed_cache();
    let s = scan(&fixture("optional_closure"), Some(cache.path()));
    assert!(
        s.stderr.contains("optional-mark reconciliation"),
        "FR-020 — the reconciliation must report at INFO, which is what \
         `RUST_LOG=info` captures here. stderr:\n{}",
        s.stderr
    );
    // And it must carry the three-state counts, not merely announce itself.
    for field in ["reviewed", "withdrawn", "declined"] {
        assert!(
            s.stderr.contains(field),
            "the log line must carry `{field}`; stderr:\n{}",
            s.stderr
        );
    }
}

// ───────────────────────────────────────────────────────────────────────
// FR-015 / C25 — the dev axis is untouched
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us2_development_axis_is_unchanged() {
    // Milestone 1272 measured that folding an extra section into a runtime
    // seed silently reassigns a component from Optional to Development. This
    // milestone touches an optional seed set, so the same failure shape is
    // available: the re-seeded closure must remove the marked names and
    // change nothing else about the seed.
    let cache = seed_cache();
    let s = scan(&fixture("optional_closure"), Some(cache.path()));
    for name in [
        "waybill-fixture-marked-a",
        "waybill-fixture-marked-b",
        "waybill-fixture-kept",
        "waybill-fixture-contested",
    ] {
        let (scope, _) = classification(&s.doc, name);
        assert_ne!(
            scope, "development",
            "{name} must not acquire a Development scope from this change"
        );
    }
    assert!(
        !s.stderr.contains("panicked"),
        "stderr: {}",
        s.stderr
    );
}
