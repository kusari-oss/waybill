//! Issue #928 — a Go module path keeps its canonical casing all the way to
//! the emitted PURL.
//!
//! This is load-bearing for advisory matching and nothing pinned it. OSV's Go
//! matching is case-sensitive on the canonical module path, measured against
//! the live API:
//!
//! ```text
//! POST https://api.osv.dev/v1/query  {"package":{"name":"...","ecosystem":"Go"}}
//!
//!   github.com/Masterminds/goutils  ->  2 vulns
//!   github.com/masterminds/goutils  ->  0 vulns
//!   github.com/Masterminds/vcs      ->  2 vulns
//!   github.com/masterminds/vcs      ->  0 vulns
//! ```
//!
//! So lowercasing a Go module path does not produce a slightly-worse match, it
//! produces **no match at all** — and the symptom is a scanner reporting zero
//! findings rather than an error. Fewer vulnerabilities reads as good news.
//!
//! waybill already preserves the path from `go.sum`. The risk this test exists
//! to cover is a plausible future change removing that by accident: a
//! "normalize PURLs for consistency" pass, a PURL library that case-folds by
//! default, or extending the deliberate lowercasing on the `pkg:generic`/CMake
//! path (C103) to Go by analogy. Each of those would pass every other test and
//! emit a valid-looking document.
//!
//! The fixture is written here rather than vendored because no fixture in the
//! tree had a mixed-case Go module path — which is why the property went
//! unpinned. Module names are synthetic.

use std::path::Path;
use std::process::Command;

/// A module path with capitals in both the owner and the repository segment,
/// plus an all-lowercase one as a control.
const MIXED_CASE_MODULE: &str = "github.com/WaybillFixture/MixedCase";
const LOWER_CASE_MODULE: &str = "github.com/waybillfixture/lowercase";

fn write_fixture(dir: &Path) {
    std::fs::write(
        dir.join("go.mod"),
        format!(
            "module example.com/waybill-fixture-casing\n\
             \n\
             go 1.24\n\
             \n\
             require (\n\
             \t{MIXED_CASE_MODULE} v1.2.3\n\
             \t{LOWER_CASE_MODULE} v0.4.0\n\
             )\n"
        ),
    )
    .expect("write go.mod");
    // `h1:` lines only; waybill reads the module/version pair, and a synthetic
    // hash keeps the fixture offline and self-contained.
    std::fs::write(
        dir.join("go.sum"),
        format!(
            "{MIXED_CASE_MODULE} v1.2.3 h1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\n\
             {MIXED_CASE_MODULE} v1.2.3/go.mod h1:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=\n\
             {LOWER_CASE_MODULE} v0.4.0 h1:CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC=\n\
             {LOWER_CASE_MODULE} v0.4.0/go.mod h1:DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD=\n"
        ),
    )
    .expect("write go.sum");
}

fn scan(dir: &Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let out_path = dir.join("sbom.cdx.json");
    let status = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(dir)
        .arg("--output")
        .arg(&out_path)
        .arg("--file-inventory=off")
        .arg("--no-deep-hash")
        .status()
        .expect("waybill should run");
    assert!(status.success(), "scan failed");
    let raw = std::fs::read(&out_path).expect("read sbom");
    serde_json::from_slice(&raw).expect("valid JSON")
}

fn purls(sbom: &serde_json::Value) -> Vec<String> {
    sbom["components"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| c["purl"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn go_module_path_keeps_its_canonical_casing_in_the_emitted_purl() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_fixture(tmp.path());
    let sbom = scan(tmp.path());
    let purls = purls(&sbom);

    // Control: the scan found the Go modules at all. Without this the
    // assertions below pass on an empty document.
    assert!(
        purls.iter().any(|p| p.starts_with("pkg:golang/")),
        "no pkg:golang/* component emitted, so this test proves nothing; got {purls:#?}"
    );

    let expected = format!("pkg:golang/{MIXED_CASE_MODULE}@v1.2.3");
    assert!(
        purls.contains(&expected),
        "the mixed-case module path must survive to the PURL exactly.\n\
         expected: {expected}\n\
         got:      {purls:#?}\n\
         Lowercasing it matches zero OSV advisories — see this file's header."
    );

    // State the failure mode directly, so a case-folding change names itself.
    let folded = expected.to_lowercase();
    assert!(
        !purls.contains(&folded),
        "the module path was case-folded to {folded}, which matches no OSV advisory"
    );

    // The all-lowercase control must be untouched too — a fix that "preserves
    // case" by upper-casing something would be just as wrong.
    let lower = format!("pkg:golang/{LOWER_CASE_MODULE}@v0.4.0");
    assert!(
        purls.contains(&lower),
        "an already-lowercase module path must pass through unchanged; got {purls:#?}"
    );
}
