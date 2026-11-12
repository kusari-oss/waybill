//! Milestone 1276 (#1340) US3 — the document records how many components the
//! lifecycle-scope rule exempted.
//!
//! C10 / FR-007. `PropagationSummary` has always held the figure and only
//! ever logged it; a log line cannot serve an operator comparing two
//! documents of one project across waybill versions, which is the gap C174
//! exists to close for the neighbouring pass.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const ROW: &str = "waybill:scope-exemptions";

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_closure_hackage")
        .join(name)
}

struct Docs {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
}

/// The revision both fixture `flake.lock`s pin.
const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";

/// A package set and a compiler configuration, seeded into a temporary cache.
///
/// Hermetic by necessity rather than tidiness: the `project` fixture has a
/// `.cabal`, so `is_enrichable` is satisfied, `enrich` runs, and `enrich` is
/// **not** offline — pointed at an empty cache it fetches the real 16 MB
/// `hackage-packages.nix` over the network. Milestone 1275's findings F8
/// records that exact trap costing a test its hermeticity, and this suite hit
/// it too: `t020` ran in 3.42 s and logged
/// `revision=Some(a799d3e3…) degraded=None` before the cache was seeded.
fn seed_cache() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let rev = dir.path().join(REV);
    std::fs::create_dir_all(&rev).unwrap();
    std::fs::write(
        rev.join("hackage-packages.nix"),
        concat!(
            "  waybill-fixture-bootlib = callPackage ({ mkDerivation }: mkDerivation {\n",
            "      pname = \"waybill-fixture-bootlib\";\n",
            "      version = \"1.1.1\";\n",
            "      license = lib.licenses.bsd3;\n",
            "  }) { };\n",
        ),
    )
    .unwrap();
    // `waybill-fixture-bootlib` nulled: the compiler supplies it, so the
    // exempt class is non-empty and the recorded count can be nonzero.
    for series in [
        "9.16.x", "9.14.x", "9.12.x", "9.10.x", "9.8.x", "9.6.x", "9.4.x", "9.0.x",
    ] {
        std::fs::write(
            rev.join(format!("configuration-ghc-{series}.nix")),
            "self: super: {\n  waybill-fixture-bootlib = null;\n}\n",
        )
        .unwrap();
    }
    dir
}

/// A cache whose compiler configuration nulls a name **no component has**.
///
/// The exempt class is therefore *known* and *empty in effect*: the rule
/// applied and exempted nothing. That is the state US3 scenario 2 is about,
/// and it is distinct both from "no configuration was read" and from "the
/// rule exempted something".
fn seed_cache_nulling_nothing_present() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let rev = dir.path().join(REV);
    std::fs::create_dir_all(&rev).unwrap();
    // One entry, not an empty file: `enrich` returns early on an empty
    // package set, and the boot set is computed after that point — so an
    // empty file leaves `boot_libraries` unset and the rule inapplicable,
    // which is the opposite of the state this fixture is for.
    std::fs::write(
        rev.join("hackage-packages.nix"),
        concat!(
            "  waybill-fixture-unrelated = callPackage ({ mkDerivation }: mkDerivation {\n",
            "      pname = \"waybill-fixture-unrelated\";\n",
            "      version = \"0.1.0\";\n",
            "      license = lib.licenses.bsd3;\n",
            "  }) { };\n",
        ),
    )
    .unwrap();
    for series in [
        "9.16.x", "9.14.x", "9.12.x", "9.10.x", "9.8.x", "9.6.x", "9.4.x", "9.0.x",
    ] {
        std::fs::write(
            rev.join(format!("configuration-ghc-{series}.nix")),
            "self: super: {\n  waybill-fixture-no-such-boot-library = null;\n}\n",
        )
        .unwrap();
    }
    dir
}

/// Scan a fixture. `cache` seeds the per-revision package-set cache; `None`
/// points it at a path that cannot exist, so a test never reads the
/// developer's `$HOME` (Principle VII).
fn scan_with(root: &Path, cache: Option<&Path>) -> Docs {
    let out = tempfile::tempdir().unwrap();
    let f = |n: &str| out.path().join(n);
    let fallback = tempfile::tempdir().unwrap();
    let cache_dir = match cache {
        Some(p) => p.to_path_buf(),
        None => fallback.path().join("__none__"),
    };
    let st = Command::new(binary_path())
        .args(["sbom", "scan", "--no-deep-hash", "--path"])
        .arg(root)
        .args(["--no-deps-dev", "--no-clearly-defined"])
        .args(["--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", f("o.cdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", f("o.spdx.json").display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", f("o.spdx3.json").display()))
        // Principle VII: never the host's cache.
        .env("WAYBILL_NIXPKGS_CACHE", &cache_dir)
        .status()
        .unwrap();
    assert!(st.success(), "scan failed: {st:?}");
    let read = |n: &str| -> Value {
        serde_json::from_slice(&std::fs::read(f(n)).unwrap()).unwrap()
    };
    Docs { cdx: read("o.cdx.json"), spdx23: read("o.spdx.json"), spdx3: read("o.spdx3.json") }
}

fn cdx_doc_prop(d: &Value, name: &str) -> Option<String> {
    d["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
        .and_then(|p| p["value"].as_str())
        .map(str::to_string)
}

fn spdx23_doc_annotation(d: &Value, field: &str) -> Option<String> {
    for a in d["annotations"].as_array()? {
        let c = a["comment"].as_str()?;
        if let Ok(v) = serde_json::from_str::<Value>(c) {
            if v["field"] == field {
                return v["value"].as_str().map(str::to_string);
            }
        }
    }
    None
}

/// C10 / FR-007 / SC-005 — the record is present at document scope, in all
/// three formats, with the same value.
#[test]
fn t020_the_exemption_count_is_recorded_in_all_three_formats() {
    let cache = seed_cache();
    let d = scan_with(&fixture("project"), Some(cache.path()));

    let cdx = cdx_doc_prop(&d.cdx, ROW)
        .unwrap_or_else(|| panic!("no {ROW} in CDX metadata.properties"));
    let spdx = spdx23_doc_annotation(&d.spdx23, ROW)
        .unwrap_or_else(|| panic!("no {ROW} in SPDX 2.3 document annotations"));
    assert_eq!(cdx, spdx, "CycloneDX and SPDX 2.3 disagree about the record");

    // SPDX 3's envelope nests inside an Annotation statement; asserting the
    // row reaches the document at all is what C11 needs here, and the parity
    // extractors are what compare the values across formats.
    assert!(
        serde_json::to_string(&d.spdx3).unwrap().contains(ROW),
        "no {ROW} anywhere in the SPDX 3 document"
    );

    // It is a JSON object, not a bare count: an object leaves room for a
    // second exemption class without a new catalogue row.
    let parsed: Value = serde_json::from_str(&cdx).expect("the record is not JSON");
    assert!(
        parsed["compiler-supplied"].is_number(),
        "expected a compiler-supplied count, got {cdx}"
    );
}

/// US3 scenario 2 — a scan where the rule **applied** and exempted nothing
/// records zero rather than omitting the record, and a scan where the rule was
/// **not applicable** omits it.
///
/// "The rule exempted nothing" and "the rule did not run" are different
/// facts. `PropagationSummary`'s `skipped_unrooted` counter exists because
/// they once looked identical from outside, and this is the same trap one
/// field over.
///
/// The fixture has no package-set cache and no `nix`, so the compiler
/// configuration is unreadable and the exempt class is empty by FR-005 — the
/// exact condition under which a naive implementation omits the field.
#[test]
fn t021_a_scan_that_exempts_nothing_records_zero() {
    // State 2 — the rule applied and exempted nothing. The seeded
    // configuration nulls a name no component has, so the class is known and
    // nothing matches it.
    let cache = seed_cache_nulling_nothing_present();
    let applied = scan_with(&fixture("project"), Some(cache.path()));
    let cdx = cdx_doc_prop(&applied.cdx, ROW).unwrap_or_else(|| {
        panic!(
            "{ROW} is absent where the rule APPLIED and exempted nothing — \
             which is indistinguishable from the rule not having run"
        )
    });
    let parsed: Value = serde_json::from_str(&cdx).expect("the record is not JSON");
    assert_eq!(
        parsed["compiler-supplied"].as_u64(),
        Some(0),
        "expected zero exemptions from a class that matches nothing, got {cdx}"
    );

    // State 1 — the rule was not applicable: no compiler configuration was
    // read, so there was no class to apply. Absence carries that, and it is
    // why the record is not emitted on every document in every ecosystem:
    // measured, that cost 35 golden byte-identity tests for a row that on a
    // Cargo or Python project can only ever say zero (findings F11).
    let inapplicable = scan_with(&fixture("closure_only"), None);
    assert!(
        cdx_doc_prop(&inapplicable.cdx, ROW).is_none(),
        "{ROW} is present where no compiler configuration was read: {:?}",
        cdx_doc_prop(&inapplicable.cdx, ROW)
    );
}

fn cdx_component<'a>(d: &'a Docs, name: &str) -> &'a Value {
    d.cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no CDX component named {name}"))
}

fn cdx_prop<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c["properties"].as_array()?.iter().find(|p| p["name"] == name)?["value"].as_str()
}

/// The SPDX 2.3 relationship types that name `name`'s package as the
/// dependency, sorted.
fn spdx23_types_into(d: &Value, name: &str) -> Vec<String> {
    let id = d["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .and_then(|p| p["SPDXID"].as_str())
        .unwrap_or_else(|| panic!("no SPDX 2.3 package named {name}"))
        .to_string();
    let mut out: Vec<String> = d["relationships"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| {
            let ty = r["relationshipType"].as_str()?;
            let into = (ty == "DEPENDS_ON" && r["relatedSpdxElement"] == id.as_str())
                || (ty.ends_with("_DEPENDENCY_OF") && r["spdxElementId"] == id.as_str());
            into.then(|| ty.to_string())
        })
        .collect();
    out.sort();
    out
}

/// #1283 — invariant I9's direct-mark origin, which milestone 1276 left
/// without a suppression path because its population was zero. On the 1283
/// corpus run it was one: slack-web's test suite declares `template-haskell`,
/// the Haskell reader scoped it `development`, and the document reported a
/// package the compiler ships in every build as absent from the artifact.
///
/// The mark is withdrawn wherever the reader set it, in all three formats,
/// so CycloneDX `scope` and the SPDX 2.3 relationship type agree. The control
/// is a test dependency the compiler does not supply, declared by the same
/// stanza, which keeps its scope.
#[test]
fn t022_a_boot_library_only_a_test_suite_declares_is_not_marked_non_runtime() {
    let cache = seed_cache();
    let d = scan_with(&fixture("test_suite_boot"), Some(cache.path()));

    let boot = cdx_component(&d, "waybill-fixture-bootlib");
    assert_eq!(
        cdx_prop(boot, "waybill:haskell-version-unresolved-reason"),
        Some("compiler-supplied"),
        "the population I9 reads; without it this test checks nothing"
    );
    assert_eq!(cdx_prop(boot, "waybill:lifecycle-scope"), None, "{boot}");
    assert_ne!(boot["scope"], "excluded", "{boot}");
    assert_eq!(spdx23_types_into(&d.spdx23, "waybill-fixture-bootlib"), ["DEPENDS_ON"]);

    // CONTROL: the rule is narrow.
    let test_dep = cdx_component(&d, "waybill-fixture-hspec");
    assert_eq!(cdx_prop(test_dep, "waybill:lifecycle-scope"), Some("development"));
    assert_eq!(test_dep["scope"], "excluded");
    assert_eq!(spdx23_types_into(&d.spdx23, "waybill-fixture-hspec"), ["DEV_DEPENDENCY_OF"]);
}
