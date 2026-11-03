//! Milestone 1273 US4 (#1279) — `waybill:unresolved-reason` agrees with the
//! document.
//!
//! An operator filters components by this annotation to find what waybill
//! could not identify. Measured at 0.14.0, and again after US2 and US3
//! landed: **2** components are version-resolved and still carry a reason
//! saying they are unresolved. A consumer acting on the annotation is
//! actively misled rather than merely under-informed.
//!
//! The reason is **stale, not wrong**. The reader stamps it correctly at read
//! time, when the component genuinely has no version and the reason names the
//! missing artifact. The nixpkgs pass later fills the version and leaves the
//! annotation behind. So the fix belongs where the fact changes, not in a
//! sweep at emission time — a sweep would also have to decide what to do
//! about every other reader's C151, which FR-015 forbids.
//!
//! # Cache isolation (mandatory)
//!
//! **No test here reads the host's `~/.cache/waybill/nixpkgs/`.** Each test
//! seeds a `tempfile::tempdir()` and points `WAYBILL_NIXPKGS_CACHE` at it.
//! Principle VII; see `nix_haskell_resolution_m926.rs` for the pattern.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";

/// Two resolvable packages and one the compiler supplies.
///
/// `waybill-fixture-boot-lib` is nulled in the configuration below, which is
/// how nixpkgs marks a package GHC ships ("use the one that comes with GHC,
/// do not build it from Hackage"). It therefore has no version in the set,
/// and FR-014 requires its reason to say *that* rather than "could not be
/// determined" — telling an operator to go and find something that does not
/// exist is a different failure from telling them nothing.
const PACKAGES: &str = r#"
  waybill-fixture-resolvable-a = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-resolvable-a";
      version = "1.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.mit;
  }) { };
  waybill-fixture-resolvable-b = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-resolvable-b";
      version = "2.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
"#;

const CONFIG: &str = "self: super: {\n  waybill-fixture-boot-lib = null;\n}\n";

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/haskell_hpack")
        .join(name)
}

fn seed_cache() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let rev_dir = dir.path().join(REV);
    std::fs::create_dir_all(&rev_dir).unwrap();
    std::fs::write(rev_dir.join("hackage-packages.nix"), PACKAGES).unwrap();
    std::fs::write(rev_dir.join("configuration-common.nix"), "self: super: {\n}\n").unwrap();
    std::fs::write(rev_dir.join("configuration-ghc-9.6.x.nix"), CONFIG).unwrap();
    dir
}

fn scan(root: &Path, cache: &Path) -> Value {
    let out = tempfile::tempdir().unwrap();
    let path = out.path().join("o.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", path.display()))
        .env("WAYBILL_NIXPKGS_CACHE", cache)
        .env("RUST_LOG", "info")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap()
}

struct Comp {
    name: String,
    version: String,
    reason: Option<String>,
}

/// Every hackage component, with the two fields this story is about.
fn hackage(doc: &Value) -> Vec<Comp> {
    doc.get("components")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    c.get("purl")
                        .and_then(|v| v.as_str())
                        .map(|p| p.starts_with("pkg:hackage/"))
                        .unwrap_or(false)
                })
                .map(|c| Comp {
                    name: c
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    version: c
                        .get("version")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    reason: c
                        .get("properties")
                        .and_then(|v| v.as_array())
                        .and_then(|ps| {
                            ps.iter()
                                .find(|p| {
                                    p.get("name").and_then(|v| v.as_str())
                                        == Some("waybill:unresolved-reason")
                                })
                                .and_then(|p| p.get("value"))
                                .and_then(|v| v.as_str())
                                .map(str::to_string)
                        }),
                })
                .collect()
        })
        .unwrap_or_default()
}

// ───────────────────────────────────────────────────────────────────────
// T055 / C19 / SC-005 — the self-contradiction
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us4_resolved_and_unresolved_are_mutually_exclusive() {
    let cache = seed_cache();
    let doc = scan(&fixture("unresolved_reason_project"), cache.path());
    let comps = hackage(&doc);

    // Non-vacuity: the pass must actually have resolved something, or this
    // test passes over a document with no resolved components at all.
    let resolved: Vec<&Comp> = comps.iter().filter(|c| !c.version.is_empty()).collect();
    assert!(
        resolved.len() >= 2,
        "the pass must have resolved components for this assertion to mean \
         anything; got {:?}",
        comps.iter().map(|c| (&c.name, &c.version)).collect::<Vec<_>>()
    );

    let contradictory: Vec<(&str, &str, &str)> = comps
        .iter()
        .filter(|c| !c.version.is_empty())
        .filter_map(|c| {
            c.reason
                .as_deref()
                .map(|r| (c.name.as_str(), c.version.as_str(), r))
        })
        .collect();
    assert!(
        contradictory.is_empty(),
        "no component may carry both a resolved version and an \
         unresolved-reason. Measured at 0.14.0 and again after US2/US3: 2. \
         Still contradictory: {contradictory:?}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T056 / C20 / FR-014 — the two whys are different whys
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us4_unresolved_component_distinguishes_compiler_supplied() {
    let cache = seed_cache();
    let doc = scan(&fixture("unresolved_reason_project"), cache.path());
    let comps = hackage(&doc);

    let boot = comps
        .iter()
        .find(|c| c.name == "waybill-fixture-boot-lib")
        .expect("the compiler-supplied dependency is emitted, not dropped");
    assert!(
        boot.version.is_empty(),
        "nixpkgs nulls it, so it carries no version from the package set"
    );
    let reason = boot
        .reason
        .as_deref()
        .expect("FR-014 — an unresolved component carries a reason");
    // The distinction FR-014 is about. GHC ships this package: there is no
    // Hackage tarball to find and no freeze file that would pin it, so
    // reporting "could not be determined" would send an operator looking for
    // something that does not exist.
    assert!(
        reason.contains("compiler") || reason.contains("boot") || reason.contains("ghc"),
        "the reason must say the compiler supplies it, not that it could not \
         be determined; got {reason:?}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T059 / FR-013 — a resolved component carries NO reason, and that is right
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us4_a_resolved_component_carries_no_reason_by_design() {
    let cache = seed_cache();
    let doc = scan(&fixture("unresolved_reason_project"), cache.path());
    let comps = hackage(&doc);

    // The measured fixture has 62 of 66 components carrying no reason, and
    // that is NOT a gap in C151's "universal across design-tier components"
    // specification. Those components were resolved, so FR-013 says they must
    // not carry one; the specification is satisfied by the components that
    // remain unresolved, and FR-014 is about those.
    //
    // Asserted rather than left implicit, because "most components have no
    // reason" is exactly what a broken annotation looks like, and the only
    // thing distinguishing the two is whether the reasonless ones are
    // resolved.
    for c in comps.iter().filter(|c| c.reason.is_none()) {
        assert!(
            !c.version.is_empty(),
            "{} carries no unresolved-reason, so it must be resolved — \
             otherwise FR-014 is violated rather than FR-013 satisfied",
            c.name
        );
    }
    for c in comps.iter().filter(|c| c.reason.is_some()) {
        assert!(
            c.version.is_empty(),
            "{} carries a reason, so it must be unresolved (FR-013)",
            c.name
        );
    }
}
