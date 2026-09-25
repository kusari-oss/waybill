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
      libraryHaskellDepends = [ waybill-fixture-mid waybill-fixture-twoparents ];
      executableHaskellDepends = [ waybill-fixture-cyc-a ];
      testHaskellDepends = [ waybill-fixture-testonly ];
  }) { };
  waybill-fixture-libb = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-libb";
      version = "0.4.1";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-twoparents ];
  }) { };
  waybill-fixture-boot = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-boot";
      version = "9.9.9";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-nevervisited ];
  }) { };
  waybill-fixture-mid = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-mid";
      version = "0.5.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-leaf waybill-fixture-boot ];
  }) { };
  waybill-fixture-leaf = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-leaf";
      version = "0.9.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-missing ];
  }) { };
  waybill-fixture-testonly = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-testonly";
      version = "6.6.6";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
  }) { };
  waybill-fixture-nevervisited = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-nevervisited";
      version = "7.7.7";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
  }) { };
  waybill-fixture-cyc-a = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-cyc-a";
      version = "1.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-cyc-b ];
  }) { };
  waybill-fixture-cyc-b = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-cyc-b";
      version = "2.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      libraryHaskellDepends = [ waybill-fixture-cyc-a ];
  }) { };
  waybill-fixture-twoparents = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-twoparents";
      version = "3.3.3";
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

/// #975: `--offline` must read the local cache, not refuse it.
///
/// A cache read is the local filesystem, which is what `--offline`'s own help
/// promises to fall back to. The key is the pinned revision, which is
/// immutable, so a hit cannot be stale. Before the fix this resolved 0 with
/// every byte already on disk.
#[test]
fn m975_offline_resolves_from_a_hydrated_cache() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &["--offline"]);

    assert!(
        mentions(&d.cdx, "waybill:nixpkgs-resolved-via"),
        "a hydrated cache must serve an offline scan; nothing here needs the network"
    );
    assert!(
        !mentions(&d.cdx, "\"offline\""),
        "no component should be blamed on being offline when the cache had it"
    );
}

/// #975, the dangerous case: a PARTIAL cache must degrade, not half-resolve.
///
/// `boot_set` swallows every retrieval error (`if let Ok(text)`), so a cache
/// holding the package set but not the `configuration-ghc-*.nix` files would
/// produce an EMPTY boot set. A boot library would then take a version from
/// the package set instead of being classified `compiler-supplied`: a version
/// the build never uses, asserted with a source hash and full provenance.
///
/// The fixture makes this concrete — `waybill-fixture-boot` is present in the
/// package set at `9.9.9` AND nulled by the compiler configuration, so losing
/// the configuration silently turns it into a resolved `9.9.9`.
///
/// That is the same under-inclusion direction that produced two wrong boot
/// rules during #947, and offline it would be silent. Principle III says fail
/// closed, so a cache miss while offline degrades the whole pass.
#[test]
fn m975_a_partial_cache_degrades_rather_than_misclassifying_boot_libraries() {
    // Package set present, NO configuration-ghc-*.nix.
    let cache = seed_cache(&[]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &["--offline"]);

    assert_eq!(
        cdx_prop(&d.cdx, "waybill-fixture-boot", "waybill:haskell-version-unresolved-reason")
            .as_deref(),
        Some("offline"),
        "a boot library must not be resolved from a package set whose boot \
         configuration was unavailable"
    );
    assert!(
        !mentions(&d.cdx, "waybill:nixpkgs-resolved-via"),
        "a partial cache must resolve nothing at all, not a subset"
    );
}

/// #975: a cold cache offline still degrades exactly as it did before.
#[test]
fn m975_offline_with_no_cache_still_degrades() {
    let empty = tempfile::tempdir().unwrap();
    let d = scan(&fixture("resolvable"), Some(empty.path()), &["--offline"]);
    assert_eq!(
        cdx_prop(&d.cdx, "waybill-fixture-liba", "waybill:haskell-version-unresolved-reason")
            .as_deref(),
        Some("offline"),
        "an empty cache offline is still offline"
    );
}

// ---------------------------------------------------------------------------
// Milestone 985 (#962) — the transitive runtime closure
// ---------------------------------------------------------------------------

/// Every `pkg:hackage/*` component in a document, with its properties.
fn hackage(d: &serde_json::Value) -> Vec<(&serde_json::Value, std::collections::BTreeMap<String, String>)> {
    d["components"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with("pkg:hackage/")))
                .map(|c| {
                    let props = c["properties"]
                        .as_array()
                        .map(|ps| {
                            ps.iter()
                                .filter_map(|p| {
                                    Some((
                                        p["name"].as_str()?.to_string(),
                                        p["value"].as_str()?.to_string(),
                                    ))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    (c, props)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// US1 / FR-001. A package the project never declares appears because
/// something it declares depends on it.
#[test]
fn m985_a_transitive_dependency_is_emitted_with_a_version() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    let hs = hackage(&d.cdx);
    let mid = hs
        .iter()
        .find(|(c, _)| c["name"].as_str() == Some("waybill-fixture-mid"))
        .expect("a package reached only transitively must be emitted");
    assert_eq!(mid.0["version"].as_str(), Some("0.5.0"));
    assert_eq!(
        mid.1.get("waybill:nixpkgs-component-origin").map(String::as_str),
        Some("transitive")
    );
}

/// US1 / FR-004. Same treatment as a declared dependency: a native hash, not
/// a `waybill:` annotation.
#[test]
fn m985_a_transitive_dependency_carries_a_native_source_hash() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    let hs = hackage(&d.cdx);
    let mid = hs
        .iter()
        .find(|(c, _)| c["name"].as_str() == Some("waybill-fixture-mid"))
        .expect("mid");
    let has_sha = mid.0["hashes"]
        .as_array()
        .is_some_and(|a| a.iter().any(|h| h["alg"].as_str() == Some("SHA-256")));
    assert!(has_sha, "got {:?}", mid.0["hashes"]);
}

/// US1 / FR-010. The fixture contains a mutually recursive pair. If the walk
/// did not terminate this test would hang rather than fail, which is why the
/// assertion is on the result.
#[test]
fn m985_the_walk_terminates_on_a_cyclic_package_set() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    let names: Vec<&str> = hackage(&d.cdx)
        .iter()
        .filter_map(|(c, _)| c["name"].as_str())
        .collect();
    assert!(names.contains(&"waybill-fixture-cyc-a"));
    assert!(names.contains(&"waybill-fixture-cyc-b"));
}

/// US1 / FR-005a. **SC-005 is a universal**, so this counts violations across
/// the whole document rather than checking one instance.
#[test]
fn m985_every_unresolvable_name_carries_a_reason() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    let offenders: Vec<&str> = hackage(&d.cdx)
        .iter()
        .filter(|(c, p)| {
            c["version"].as_str().unwrap_or("").is_empty()
                && !p.contains_key("waybill:haskell-version-unresolved-reason")
        })
        .filter_map(|(c, _)| c["name"].as_str())
        .collect();
    assert!(offenders.is_empty(), "versionless with no reason: {offenders:?}");
    // ...and the transitively-reached missing name is one of them.
    let missing = hackage(&d.cdx)
        .into_iter()
        .find(|(c, _)| c["name"].as_str() == Some("waybill-fixture-missing"))
        .expect("a name absent from the package set must still be emitted (FR-005a)");
    assert_eq!(
        missing.1.get("waybill:haskell-version-unresolved-reason").map(String::as_str),
        Some("absent-from-package-set")
    );
}

/// US1 / FR-011. A boot library is recorded but never walked THROUGH: its
/// relations belong to the compiler, not to the nixpkgs entry. The fixture's
/// `nevervisited` is reachable only that way.
#[test]
fn m985_the_closure_does_not_walk_through_a_boot_library() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    let names: Vec<&str> = hackage(&d.cdx)
        .iter()
        .filter_map(|(c, _)| c["name"].as_str())
        .collect();
    assert!(names.contains(&"waybill-fixture-boot"), "boot lib is still recorded");
    assert!(
        !names.contains(&"waybill-fixture-nevervisited"),
        "reachable only through a boot library, so must be absent; got {names:?}"
    );
}

/// US1 / FR-002. Test relations are out of scope, and the fixture reaches
/// `testonly` ONLY through `testHaskellDepends`. Deferred to issue #985.
#[test]
fn m985_test_only_relations_do_not_enter_the_runtime_closure() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    let names: Vec<&str> = hackage(&d.cdx)
        .iter()
        .filter_map(|(c, _)| c["name"].as_str())
        .collect();
    assert!(
        !names.contains(&"waybill-fixture-testonly"),
        "a test-only relation must not appear in the RUNTIME closure; got {names:?}"
    );
}

/// FR-003. The closure resolves offline from a hydrated cache.
///
/// Not hypothetical. Milestone 975 found `--offline` refusing a cache it
/// already had — 0 of 97 resolved with every byte on disk — because the
/// offline check sat above the cache read rather than inside it. The closure
/// reads the same package set through the same retrieval path, so it must be
/// shown to inherit the fix rather than assumed to.
#[test]
fn m985_the_closure_resolves_offline_from_a_hydrated_cache() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &["--offline"]);
    let names: Vec<&str> = hackage(&d.cdx)
        .iter()
        .filter_map(|(c, _)| c["name"].as_str())
        .collect();
    assert!(
        names.contains(&"waybill-fixture-mid"),
        "a hydrated cache must serve the closure offline; nothing here needs \
         the network. got {names:?}"
    );
    assert!(mentions(&d.cdx, "waybill:nixpkgs-haskell-closure"), "the closure ran");
}

/// FR-003 / Principle III. A PARTIAL cache must degrade, not half-resolve.
///
/// `boot_set` tolerates a missing compiler configuration, because a GHC series
/// genuinely absent from nixpkgs must not abort the pass. An offline cache
/// miss is the opposite situation, and tolerating it yields an EMPTY boot set
/// — under which every boot library takes a version from the package set.
///
/// At closure scale that is worse than on the declared path: an empty boot set
/// also means the walk descends THROUGH boot libraries, pulling in their
/// nixpkgs dependency lists, which the build does not have. The whole pass
/// degrades instead.
#[test]
fn m985_a_partial_cache_does_not_half_resolve_the_closure() {
    // Package set present, NO configuration-ghc-*.nix.
    let cache = seed_cache(&[]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &["--offline"]);
    assert!(
        !mentions(&d.cdx, "waybill:nixpkgs-resolved-via"),
        "a partial cache must resolve nothing at all, not a subset"
    );
    let names: Vec<&str> = hackage(&d.cdx)
        .iter()
        .filter_map(|(c, _)| c["name"].as_str())
        .collect();
    assert!(
        !names.contains(&"waybill-fixture-nevervisited"),
        "an empty boot set would let the walk descend THROUGH a boot library; \
         got {names:?}"
    );
}

/// US2 / FR-006a. **SC-003-adjacent universal**: every Haskell component,
/// declared ones included.
#[test]
fn m985_every_haskell_component_carries_an_origin() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    let missing: Vec<&str> = hackage(&d.cdx)
        .iter()
        .filter(|(_, p)| !p.contains_key("waybill:nixpkgs-component-origin"))
        .filter_map(|(c, _)| c["name"].as_str())
        .collect();
    assert!(missing.is_empty(), "no origin on: {missing:?}");
}

/// US2 / FR-007. Reachable both ways is declared; the stronger claim wins.
#[test]
fn m985_a_declared_package_stays_declared() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    let liba = hackage(&d.cdx)
        .into_iter()
        .find(|(c, _)| c["name"].as_str() == Some("waybill-fixture-liba"))
        .expect("liba");
    assert_eq!(
        liba.1.get("waybill:nixpkgs-component-origin").map(String::as_str),
        Some("declared")
    );
}

/// US2 / C-2. The origin reaches all three formats (`SymmetricEqual`).
#[test]
fn m985_origin_reaches_every_format() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    assert!(mentions(&d.cdx, "waybill:nixpkgs-component-origin"), "CDX");
    assert!(mentions(&d.spdx2, "waybill:nixpkgs-component-origin"), "SPDX 2.3");
    assert!(mentions(&d.spdx3, "waybill:nixpkgs-component-origin"), "SPDX 3");
}

/// US3 / FR-008, **SC-004**. Counts violations across the document.
///
/// This is milestone 980 at closure scale: there, version resolution rewrote
/// component PURLs after the edges were built and disconnected everything it
/// resolved. The closure multiplies both counts.
#[test]
fn m985_no_closure_edge_dangles() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    let mut refs: std::collections::HashSet<String> = d.cdx["components"]
        .as_array()
        .expect("components")
        .iter()
        .filter_map(|c| c["bom-ref"].as_str().map(str::to_string))
        .collect();
    if let Some(r) = d.cdx["metadata"]["component"]["bom-ref"].as_str() {
        refs.insert(r.to_string());
    }
    let dangling: Vec<String> = d.cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|e| {
            let from = e["ref"].as_str().unwrap_or("?").to_string();
            e["dependsOn"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(move |t| t.as_str().map(|t| (from.clone(), t.to_string())))
                .collect::<Vec<_>>()
        })
        .filter(|(_, t)| !refs.contains(t))
        .map(|(f, t)| format!("{f} -> {t}"))
        .collect();
    assert!(dangling.is_empty(), "invariant I2 violated: {dangling:?}");
}

/// US3 / FR-009. The edge names the actual parent, not the root.
#[test]
fn m985_an_edge_comes_from_the_actual_parent_not_the_root() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    let by_ref: std::collections::BTreeMap<&str, &str> = d.cdx["components"]
        .as_array()
        .expect("components")
        .iter()
        .filter_map(|c| Some((c["bom-ref"].as_str()?, c["name"].as_str()?)))
        .collect();
    let mut into_leaf: Vec<&str> = Vec::new();
    for e in d.cdx["dependencies"].as_array().into_iter().flatten() {
        let from = e["ref"].as_str().unwrap_or("");
        for t in e["dependsOn"].as_array().into_iter().flatten() {
            if by_ref.get(t.as_str().unwrap_or("")) == Some(&"waybill-fixture-leaf") {
                into_leaf.push(by_ref.get(from).copied().unwrap_or(from));
            }
        }
    }
    assert_eq!(
        into_leaf,
        vec!["waybill-fixture-mid"],
        "leaf is reached through mid, so the edge must come from mid"
    );
}

/// US3 / FR-012, E2.2. Two parents, two edges, one component.
#[test]
fn m985_a_package_reached_twice_is_one_component_with_two_edges() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    let hits: Vec<_> = hackage(&d.cdx)
        .into_iter()
        .filter(|(c, _)| c["name"].as_str() == Some("waybill-fixture-twoparents"))
        .collect();
    assert_eq!(hits.len(), 1, "must appear exactly once");
    let target = hits[0].0["bom-ref"].as_str().expect("bom-ref");
    let parents = d.cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| {
            e["dependsOn"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|t| t.as_str() == Some(target))
        })
        .count();
    assert_eq!(parents, 2, "reached from liba AND libb");
}

/// US4 / FR-014. The counts reach the DOCUMENT, not a log line — which is the
/// defect milestone 973 was filed for.
#[test]
fn m985_the_closure_records_its_counts_at_document_scope() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    let raw = d.cdx["metadata"]["properties"]
        .as_array()
        .expect("document properties")
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill:nixpkgs-haskell-closure"))
        .and_then(|p| p["value"].as_str())
        .expect("C176 must be present when the closure ran");
    let v: serde_json::Value = serde_json::from_str(raw).expect("C176 is JSON");
    assert!(v["declared"].as_u64().is_some_and(|n| n > 0), "{v}");
    assert!(v["transitive"].as_u64().is_some_and(|n| n > 0), "{v}");
    assert!(v["relations-walked"].as_u64().is_some_and(|n| n > 0), "{v}");
    assert!(mentions(&d.spdx2, "waybill:nixpkgs-haskell-closure"), "SPDX 2.3");
    assert!(mentions(&d.spdx3, "waybill:nixpkgs-haskell-closure"), "SPDX 3");
}

/// US4 / FR-015. A scan where the closure did not run records nothing.
#[test]
fn m985_a_scan_without_the_closure_records_nothing() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(
        &fixture("resolvable"),
        Some(cache.path()),
        &["--no-nixpkgs-haskell-closure"],
    );
    assert!(!mentions(&d.cdx, "waybill:nixpkgs-haskell-closure"), "CDX");
    assert!(!mentions(&d.cdx, "waybill:nixpkgs-component-origin"), "no origins either");
}

/// FR-016 / FR-017. The opt-out suppresses the closure WITHOUT surrendering
/// declared-dependency resolution — the two are separately valuable.
#[test]
fn m985_the_opt_out_keeps_declared_resolution() {
    let cache = seed_cache(&["9.6.x"]);
    let on = scan(&fixture("resolvable"), Some(cache.path()), &[]);
    let off = scan(
        &fixture("resolvable"),
        Some(cache.path()),
        &["--no-nixpkgs-haskell-closure"],
    );
    let names = |d: &serde_json::Value| -> std::collections::BTreeSet<String> {
        hackage(d).iter().filter_map(|(c, _)| c["name"].as_str().map(str::to_string)).collect()
    };
    let (n_on, n_off) = (names(&on.cdx), names(&off.cdx));
    assert!(n_on.len() > n_off.len(), "the closure must add components");
    assert!(
        n_off.is_subset(&n_on),
        "the opt-out must only REMOVE closure additions, never change what was already there"
    );
    // Declared resolution survives the opt-out.
    let liba = hackage(&off.cdx)
        .into_iter()
        .find(|(c, _)| c["name"].as_str() == Some("waybill-fixture-liba"))
        .expect("liba");
    assert_eq!(liba.0["version"].as_str(), Some("1.2.3"));
}

/// #980 / invariant I2: every edge endpoint must resolve to a component
/// present in the document.
///
/// I2 is not a new idea — it is named in
/// `generate::graph_completeness` (m860, FR-001, C-3.3) and asserted there
/// against a hand-built three-element fixture. It had never been checked on
/// a real emitted document, which is how #980 shipped: the nixpkgs pass
/// assigns a version and rewrites the component's PURL, but the PURL is the
/// identity the dependency graph keys on and the edges were already built.
/// Every component the feature successfully resolved became unreachable.
///
/// The symptom is format-specific and neither format errors:
///   CycloneDX keeps the edge pointing at the pre-resolution PURL
///   SPDX drops the relationship entirely — schema-valid and wrong
///
/// So this asserts the invariant directly rather than trusting either
/// format's own validity.
#[test]
fn m980_no_dependency_edge_points_at_a_component_that_does_not_exist() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);

    let mut refs: std::collections::HashSet<String> = d.cdx["components"]
        .as_array()
        .expect("components")
        .iter()
        .filter_map(|c| c["bom-ref"].as_str().map(str::to_string))
        .collect();
    if let Some(root) = d.cdx["metadata"]["component"]["bom-ref"].as_str() {
        refs.insert(root.to_string());
    }

    let mut dangling: Vec<String> = Vec::new();
    if let Some(deps) = d.cdx["dependencies"].as_array() {
        for e in deps {
            let from = e["ref"].as_str().unwrap_or("?");
            for t in e["dependsOn"].as_array().into_iter().flatten() {
                let t = t.as_str().unwrap_or("?");
                if !refs.contains(t) {
                    dangling.push(format!("{from} -> {t}"));
                }
            }
        }
    }
    assert!(
        dangling.is_empty(),
        "invariant I2 violated: {} edge(s) point at a bom-ref no component has.\n  {}\n\
         A component whose version was resolved must keep its inbound edges; \
         rewriting its PURL without rewriting the endpoints orphans it (#980).",
        dangling.len(),
        dangling.join("\n  ")
    );
}

/// #980: the resolved components must still be REACHABLE, not merely
/// referenced. A graph can be free of dangling edges and still have dropped
/// them — which is exactly how SPDX fails here, silently and schema-validly.
#[test]
fn m980_a_resolved_component_is_still_reachable_from_the_root() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);

    // `waybill-fixture-liba` resolves to 1.2.3 from the pinned package set.
    // Before #980 it kept its version and lost its edge.
    let target = d.cdx["components"]
        .as_array()
        .expect("components")
        .iter()
        .find(|c| c["name"].as_str() == Some("waybill-fixture-liba"))
        .and_then(|c| c["bom-ref"].as_str())
        .expect("the fixture's resolvable dependency must be emitted")
        .to_string();

    let referenced = d.cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|e| {
            e["dependsOn"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|t| t.as_str() == Some(target.as_str()))
        });
    assert!(
        referenced,
        "a resolved dependency must remain an edge target; \
         {target} is emitted but nothing depends on it (#980)"
    );
}

/// #973 (C174): a pass that SUCCEEDS must say so at document scope.
///
/// Before this, the document named nixpkgs only on failure — a scan that
/// resolved every dependency emitted no document-scope nixpkgs property at
/// all, so a regression that disabled the pass outright would have left the
/// property set byte-identical to a healthy one. That is the hole this
/// closes, which is why the assertion is on the *successful* path.
#[test]
fn m973_a_successful_pass_is_recorded_at_document_scope() {
    let cache = seed_cache(&["9.6.x"]);
    let d = scan(&fixture("resolvable"), Some(cache.path()), &[]);

    let props = d.cdx["metadata"]["properties"]
        .as_array()
        .expect("document-scope properties");
    let rec = props
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill:nixpkgs-haskell-resolution"))
        .expect("C174 must be present when the pass ran");

    let v: serde_json::Value =
        serde_json::from_str(rec["value"].as_str().expect("C174 value is a string"))
            .expect("C174 value is JSON");

    // The revision is the fact a consumer most needs and the one that was
    // previously recoverable only by iterating every component.
    assert!(
        v["revision"].as_str().is_some_and(|r| !r.is_empty()),
        "C174 must name the revision it resolved through, got {v}"
    );
    assert!(
        v["resolved"].as_u64().is_some_and(|n| n > 0),
        "the fixture resolves dependencies, so the count must be positive: {v}"
    );
    assert!(v["unresolved"].is_object(), "unresolved is a by-reason map: {v}");
    assert!(v["disagreements"].is_u64(), "disagreements is a count: {v}");

    // C174 is SymmetricEqual, so it must reach the other two formats.
    assert!(mentions(&d.spdx2, "waybill:nixpkgs-haskell-resolution"), "C174 missing from SPDX 2.3");
    assert!(mentions(&d.spdx3, "waybill:nixpkgs-haskell-resolution"), "C174 missing from SPDX 3");
}

/// #973: a degraded pass still ran, so it is still recorded — C174 and C173
/// are additive, not alternatives. A consumer must not have to infer "the
/// pass ran" from the absence of a success marker.
#[test]
fn m973_a_degraded_pass_records_both_rows() {
    let d = scan(&fixture("resolvable"), None, &["--offline"]);
    assert!(mentions(&d.cdx, "waybill:nixpkgs-haskell-degraded"), "C173 expected");
    assert!(mentions(&d.cdx, "waybill:nixpkgs-haskell-resolution"), "C174 expected");
}

/// #973: the pass never ran, so it records nothing. Keeps every non-Haskell
/// document byte-identical.
#[test]
fn m973_a_scan_without_the_pass_records_nothing() {
    let d = scan(&fixture("no_flake"), None, &[]);
    assert!(
        !mentions(&d.cdx, "waybill:nixpkgs-haskell-resolution"),
        "C174 must be absent when the pass never ran"
    );
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
