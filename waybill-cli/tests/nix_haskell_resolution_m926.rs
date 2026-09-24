//! Milestone 926 (#947) — nixpkgs-resolved Haskell versions, end to end.
//!
//! These run the real binary against fixture trees. That is a different
//! guarantee from the unit tests in
//! `scan_fs::package_db::nix::haskell_packages`, which exercise retrieval,
//! parsing and classification directly: these also cover CLI wiring, the
//! enrichment call site, annotation registration and the emission pipeline in
//! all three formats — any of which can be broken while the module's own
//! tests stay green.
//!
//! **No test here touches the network.** The resolved path is exercised by
//! pre-populating the per-revision cache, which `cached_or_fetch` consults
//! before retrieving. That is deliberately not an `#[ignore]`d online test:
//! an ignored test is one nobody runs, and this project has already been bitten
//! by a suite that was green because it was not looking (#949).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_haskell")
        .join(name)
}

/// The revision the fixtures' `flake.lock` pins.
const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";

/// A package set carrying one resolvable dependency and one the compiler
/// supplies. Synthetic names throughout; the source hash is the research-R2
/// vector, so the emitted SHA-256 is one verified against a real tarball.
const PACKAGES: &str = r#"
  waybill-fixture-liba = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-liba";
      version = "1.2.3";
      sha256 = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly";
  }) { };
  waybill-fixture-libb = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-libb";
      version = "0.4.1";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
  }) { };
  waybill-fixture-boot = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-boot";
      version = "9.9.9";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
  }) { };
"#;

const CONFIG: &str = "self: super: { waybill-fixture-boot = null; }\n";

/// Seed the per-revision cache so the resolved path runs with no network.
fn seed_cache(series: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let rev_dir = dir.path().join(REV);
    std::fs::create_dir_all(&rev_dir).unwrap();
    std::fs::write(rev_dir.join("hackage-packages.nix"), PACKAGES).unwrap();
    for s in series {
        std::fs::write(rev_dir.join(format!("configuration-ghc-{s}.nix")), CONFIG).unwrap();
    }
    dir
}

struct Docs {
    cdx: serde_json::Value,
    spdx2: serde_json::Value,
    spdx3: serde_json::Value,
}

/// Scan a fixture, emitting all three formats.
fn scan(root: &Path, cache: Option<&Path>, extra: &[&str]) -> Docs {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!("m926-{}-{seq}", std::process::id()));
    let (c, s2, s3) = (
        base.with_extension("cdx.json"),
        base.with_extension("spdx.json"),
        base.with_extension("spdx3.json"),
    );

    let mut cmd = Command::new(binary_path());
    cmd.args([
        "sbom", "scan", "--path", root.to_str().unwrap(),
        // Suppress every OTHER network enrichment. `--offline` cannot be used
        // for this: it also disables the feature under test. Without these the
        // suite makes real deps.dev and ClearlyDefined calls, which is both
        // non-hermetic and slow — ClearlyDefined alone is 97-98% of a cold
        // scan's wall clock (#930).
        "--no-deps-dev", "--no-clearly-defined",
        "--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json",
        "--output", &format!("cyclonedx-json={}", c.display()),
        "--output", &format!("spdx-2.3-json={}", s2.display()),
        "--output", &format!("spdx-3-json={}", s3.display()),
    ]);
    cmd.args(extra);
    match cache {
        Some(p) => {
            cmd.env("WAYBILL_NIXPKGS_CACHE", p);
        }
        None => {
            // An unset cache would fall back to the real $HOME. Point it at a
            // path that cannot exist so a test never reads or writes the
            // developer's cache.
            cmd.env("WAYBILL_NIXPKGS_CACHE", root.join("__no_cache__"));
        }
    }
    let st = cmd.status().unwrap();
    assert!(st.success(), "scan failed: {st:?}");

    let read = |p: &PathBuf| -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
    };
    Docs { cdx: read(&c), spdx2: read(&s2), spdx3: read(&s3) }
}

fn cdx_component<'a>(d: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    d["components"]
        .as_array()?
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
}

fn cdx_prop(d: &serde_json::Value, name: &str, key: &str) -> Option<String> {
    cdx_component(d, name)?["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(key))
        .and_then(|p| p["value"].as_str())
        .map(str::to_string)
}

/// A document with the two fields that legitimately differ between runs
/// removed. Structural rather than textual: an earlier textual version of
/// this looped forever, because replacing a match with the search key finds
/// the same match again.
fn canonical(v: &serde_json::Value) -> String {
    let mut m = v.clone();
    if let Some(o) = m.as_object_mut() {
        o.remove("serialNumber");
        if let Some(md) = o.get_mut("metadata").and_then(|m| m.as_object_mut()) {
            md.remove("timestamp");
        }
    }
    serde_json::to_string(&m).unwrap()
}

/// Does `key` appear anywhere in a serialised document? Annotation envelopes
/// differ in shape per format, so presence of the key is the portable test.
fn mentions(d: &serde_json::Value, key: &str) -> bool {
    serde_json::to_string(d).unwrap().contains(key)
}

// ---------------------------------------------------------------------------
// US1 — versions and native hashes
// ---------------------------------------------------------------------------

/// T021: a ranged dependency resolves to an exact version with a **native**
/// SHA-256, in all three formats, with no flags passed (FR-015 default-on).
#[test]
fn m926_resolves_with_a_native_hash_in_all_three_formats() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);

    let c = cdx_component(&d.cdx, "waybill-fixture-liba")
        .expect("the dependency must be present");
    assert_eq!(c["version"].as_str(), Some("1.2.3"));

    // Native carrier, not an annotation (contract C3).
    let hashes = c["hashes"].as_array().expect("a native hashes[] entry");
    let sha = hashes
        .iter()
        .find(|h| h["alg"].as_str() == Some("SHA-256"))
        .expect("SHA-256 among the native hashes");
    assert_eq!(
        sha["content"].as_str(),
        Some("9e26f12230d38ae56dcf94f8c139799dc3b7376f3434d35ce74847a0a24fd5ff")
    );

    // SPDX 2.3 native checksums[].
    let pkg = d.spdx2["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill-fixture-liba"))
        .expect("present in SPDX 2.3");
    assert_eq!(pkg["versionInfo"].as_str(), Some("1.2.3"));
    let sums = pkg["checksums"].as_array().expect("native checksums[]");
    assert!(
        sums.iter().any(|s| s["algorithm"].as_str() == Some("SHA256")
            && s["checksumValue"].as_str()
                == Some("9e26f12230d38ae56dcf94f8c139799dc3b7376f3434d35ce74847a0a24fd5ff")),
        "SPDX 2.3 must carry the hash natively, got {sums:?}"
    );

    // SPDX 3 carries the version too.
    assert!(
        serde_json::to_string(&d.spdx3).unwrap().contains("1.2.3"),
        "SPDX 3 must carry the resolved version"
    );
}

/// T042 / SC-001: the declared set partitions exactly into resolved and
/// unresolved-with-a-reason. Asserted over the whole set rather than a
/// sampled dependency, so a regression resolving only the first entry fails.
#[test]
fn m926_every_declared_dependency_is_resolved_or_reasoned() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);

    let declared = [
        "waybill-fixture-liba",
        "waybill-fixture-libb",
        "waybill-fixture-boot",
        "waybill-fixture-absent",
    ];
    for name in declared {
        let Some(c) = cdx_component(&d.cdx, name) else {
            panic!("{name} is declared and must appear as a component");
        };
        let versioned = c["version"].as_str().is_some_and(|v| !v.is_empty());
        let reasoned =
            cdx_prop(&d.cdx, name, "waybill:haskell-version-unresolved-reason").is_some();
        assert!(
            versioned ^ reasoned,
            "{name}: must be exactly one of resolved or reasoned \
             (versioned={versioned}, reasoned={reasoned})"
        );
    }
}

// ---------------------------------------------------------------------------
// US2 — honest reporting
// ---------------------------------------------------------------------------

/// T025: a boot library stays versionless and says why, carrying no hash and
/// no versioned identifier.
#[test]
fn m926_a_boot_library_is_versionless_with_a_reason() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);

    let c = cdx_component(&d.cdx, "waybill-fixture-boot").expect("present");
    assert!(
        c["version"].as_str().unwrap_or("").is_empty(),
        "no version may be invented for a compiler-supplied package"
    );
    assert!(c.get("hashes").is_none() || c["hashes"].as_array().unwrap().is_empty());
    assert!(
        !c["purl"].as_str().unwrap_or("").contains('@'),
        "the PURL must not carry a version either"
    );
    assert_eq!(
        cdx_prop(&d.cdx, "waybill-fixture-boot", "waybill:haskell-version-unresolved-reason")
            .as_deref(),
        Some("compiler-supplied")
    );
}

/// T041 / FR-014c: the specific Principle IX trap. `waybill-fixture-boot` is
/// present in the package set **with a version**, and must still not resolve.
#[test]
fn m926_a_boot_library_never_takes_the_package_sets_version() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);

    let doc = serde_json::to_string(&d.cdx).unwrap();
    assert!(
        !doc.contains("9.9.9"),
        "9.9.9 is the package set's version for a compiler-supplied package; \
         emitting it would be an invented version the build never uses"
    );
}

/// T041 / FR-014c: no per-compiler component variants, even when the flake
/// names three compilers.
#[test]
fn m926_multiple_candidate_compilers_do_not_multiply_components() {
    let cache = seed_cache(&["9.4.x", "9.6.x", "9.10.x"]);
    let d = scan(&fixture("multi_compiler"), Some(cache.path()), &[]);

    let names: Vec<&str> = d.cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["name"].as_str())
        .filter(|n| n.starts_with("waybill-fixture-lib"))
        .collect();
    let unique: std::collections::BTreeSet<_> = names.iter().collect();
    assert_eq!(
        names.len(),
        unique.len(),
        "one component per dependency, not one per candidate compiler: {names:?}"
    );
}

/// T027 / SC-002: no dependency is versionless without a reason.
#[test]
fn m926_no_versionless_dependency_lacks_a_reason() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);

    let mut offenders = Vec::new();
    for c in d.cdx["components"].as_array().unwrap() {
        let Some(name) = c["name"].as_str() else { continue };
        if !c["purl"].as_str().unwrap_or("").starts_with("pkg:hackage/") {
            continue;
        }
        if !c["version"].as_str().unwrap_or("").is_empty() {
            continue;
        }
        if cdx_prop(&d.cdx, name, "waybill:haskell-version-unresolved-reason").is_none() {
            offenders.push(name.to_string());
        }
    }
    assert!(offenders.is_empty(), "versionless with no reason: {offenders:?}");
}

// ---------------------------------------------------------------------------
// US3 — provenance, and cross-format parity of the new rows
// ---------------------------------------------------------------------------

/// T033: a nixpkgs-resolved version records where it came from, and the row
/// reaches **all three** formats — the thing registration alone cannot prove.
#[test]
fn m926_provenance_reaches_every_format() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);

    let key = "waybill:nixpkgs-resolved-via";
    assert!(mentions(&d.cdx, key), "C169 missing from CycloneDX");
    assert!(mentions(&d.spdx2, key), "C169 missing from SPDX 2.3");
    assert!(mentions(&d.spdx3, key), "C169 missing from SPDX 3");

    let v = cdx_prop(&d.cdx, "waybill-fixture-liba", key).expect("on the resolved component");
    assert!(v.contains(REV), "provenance must name the revision: {v}");
    assert!(v.contains("nixpkgs"));
}

/// The reason row likewise reaches all three formats.
#[test]
fn m926_the_unresolved_reason_reaches_every_format() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);

    let key = "waybill:haskell-version-unresolved-reason";
    assert!(mentions(&d.cdx, key), "C170 missing from CycloneDX");
    assert!(mentions(&d.spdx2, key), "C170 missing from SPDX 2.3");
    assert!(mentions(&d.spdx3, key), "C170 missing from SPDX 3");
}

// ---------------------------------------------------------------------------
// Degradation, determinism, and the no-op paths
// ---------------------------------------------------------------------------

/// T022 / SC-007: a repository with no `flake.lock` is untouched.
#[test]
fn m926_no_flake_lock_leaves_the_document_unchanged() {
    let with = scan(&fixture("no_flake"), None, &[]);
    let without = scan(&fixture("no_flake"), None, &["--no-nixpkgs-haskell"]);

    assert_eq!(
        canonical(&with.cdx),
        canonical(&without.cdx),
        "the feature must be a no-op when there is no flake.lock"
    );
    assert!(!mentions(&with.cdx, "waybill:nixpkgs-resolved-via"));
    assert!(!mentions(&with.cdx, "waybill:haskell-version-unresolved-reason"));
}

/// T044 / contract C7: a degraded pass records it at **document scope**, not
/// only per dependency. An operator checks the document first.
#[test]
fn m926_degradation_is_recorded_at_document_scope() {
    let d = scan(&fixture("resolvable"), None, &["--offline"]);

    let props = d.cdx["metadata"]["properties"]
        .as_array()
        .expect("document-scope properties");
    let rec = props
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill:nixpkgs-haskell-degraded"))
        .expect("C173 must be present when the pass degraded");
    assert_eq!(rec["value"].as_str(), Some("offline"));

    // And it must reach the other two formats (C173 is SymmetricEqual).
    assert!(mentions(&d.spdx2, "waybill:nixpkgs-haskell-degraded"), "C173 missing from SPDX 2.3");
    assert!(mentions(&d.spdx3, "waybill:nixpkgs-haskell-degraded"), "C173 missing from SPDX 3");
}

/// A clean pass records no degradation — the row must not appear just because
/// the feature ran.
#[test]
fn m926_a_clean_pass_records_no_degradation() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    assert!(
        !mentions(&d.cdx, "waybill:nixpkgs-haskell-degraded"),
        "a successful resolution must not claim it degraded"
    );
}

/// T034 / FR-012: a lock pinning a moving reference has no reproducible
/// revision, so every dependency is reasoned rather than resolved.
#[test]
fn m926_a_moving_reference_degrades_with_its_own_reason() {
    let d = scan(&fixture("moving_ref"), None, &[]);
    assert_eq!(
        cdx_prop(&d.cdx, "waybill-fixture-liba", "waybill:haskell-version-unresolved-reason")
            .as_deref(),
        Some("no-exact-revision")
    );
}

/// T034 / FR-009: `--offline` degrades with its own reason and invents nothing.
#[test]
fn m926_offline_degrades_with_its_own_reason() {
    let d = scan(&fixture("resolvable"), None, &["--offline"]);
    assert_eq!(
        cdx_prop(&d.cdx, "waybill-fixture-liba", "waybill:haskell-version-unresolved-reason")
            .as_deref(),
        Some("offline")
    );
    let c = cdx_component(&d.cdx, "waybill-fixture-liba").unwrap();
    assert!(c["version"].as_str().unwrap_or("").is_empty());
}

/// T035 / SC-004: two scans of the same repository at the same revision
/// produce byte-identical documents.
#[test]
fn m926_two_scans_of_one_revision_are_byte_identical() {
    let cache = seed_cache(&["9.6.x"]);
    let a = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    let b = scan(&fixture("resolvable"), Some(cache.path()), &[]);

    assert_eq!(canonical(&a.cdx), canonical(&b.cdx));
}
