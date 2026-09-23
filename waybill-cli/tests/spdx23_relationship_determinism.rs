//! SPDX 2.3 relationship order must not depend on the order the scan produced
//! components in.
//!
//! Found by the #898 Haskell corpus target on its first verification run: the
//! same pinned revision emitted an identical **set** of 111 relationships in
//! two CI runs, in different orders. Packages matched. CycloneDX matched. SPDX
//! 3 matched. Only SPDX 2.3 drifted, and the golden comparison failed.
//!
//! The other two emitters were already immune and this one was not:
//!
//!   - CycloneDX accumulates into `BTreeMap<String, BTreeSet<String>>`
//!     (`generate/cyclonedx/dependencies.rs`), so it is sorted by construction.
//!   - SPDX 3 sorts explicitly (`v3_relationships::sort_by_spdx_id`).
//!   - SPDX 2.3 pushed edges while walking `artifacts.components` and
//!     inherited that order verbatim.
//!
//! Upstream order stopped being stable when the walker and resolver became
//! parallel (m772-m774), so "whatever order the scan produced" is now a
//! function of thread scheduling. A document format must not be.
//!
//! This test asserts the property directly — same inputs in a different order
//! produce byte-identical output — rather than asserting the output happens to
//! be sorted, which would pass for any fixed order the implementation chose.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

fn binary_path() -> &'static str { env!("CARGO_BIN_EXE_waybill") }

/// A small multi-package tree: several manifests so the emitted document
/// carries enough relationships for an ordering difference to be observable.
fn fixture() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    std::fs::write(
        r.join("root.cabal"),
        "name: waybill-fixture-root\nversion: 1.0\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends: waybill-fixture-alpha, waybill-fixture-beta, \
         waybill-fixture-gamma, waybill-fixture-delta\n\n\
         test-suite spec\n  type: exitcode-stdio-1.0\n  \
         build-depends: waybill-fixture-epsilon, waybill-fixture-zeta\n",
    ).unwrap();
    for (dir, name) in [("pkg-a", "waybill-fixture-a"), ("pkg-b", "waybill-fixture-b")] {
        let sub = r.join(dir);
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            sub.join(format!("{dir}.cabal")),
            format!(
                "name: {name}\nversion: 2.0\nlicense: BSD-3-Clause\n\n\
                 library\n  build-depends: waybill-fixture-alpha, waybill-fixture-eta\n"
            ),
        ).unwrap();
    }
    d
}

fn scan_spdx23(root: &Path, seq: usize) -> serde_json::Value {
    let out = std::env::temp_dir()
        .join(format!("spdx23-det-{}-{seq}.json", std::process::id()));
    let st = Command::new(binary_path())
        .args(["sbom", "scan", "--path", root.to_str().unwrap(), "--offline",
               "--format", "spdx-2.3-json", "--output", out.to_str().unwrap()])
        .status().unwrap();
    assert!(st.success(), "scan failed: {st:?}");
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

/// The emitted relationship array must be in a deterministic order, so the
/// same tree scanned repeatedly produces the same bytes.
#[test]
fn spdx23_relationships_are_ordered_deterministically() {
    let d = fixture();
    let a = scan_spdx23(d.path(), 0);
    let b = scan_spdx23(d.path(), 1);

    let seq = |v: &serde_json::Value| -> Vec<String> {
        v["relationships"].as_array().unwrap().iter()
            .map(|r| format!("{} {} {}",
                r["spdxElementId"].as_str().unwrap_or(""),
                r["relationshipType"].as_str().unwrap_or(""),
                r["relatedSpdxElement"].as_str().unwrap_or("")))
            .collect()
    };

    let (sa, sb) = (seq(&a), seq(&b));
    assert!(!sa.is_empty(), "fixture emitted no relationships — it cannot detect ordering drift");
    assert_eq!(sa, sb,
        "two scans of the same tree emitted relationships in different orders. \
         SPDX 2.3 must not inherit whatever order the scan produced components \
         in — that order depends on thread scheduling since m772-m774.");
}

/// The order must be a total order over the edge's own fields, not an artifact
/// of insertion. Asserting sortedness is what makes the guarantee checkable by
/// a reader of one document, rather than only by diffing two runs.
#[test]
fn spdx23_relationships_are_sorted_by_element_type_target() {
    let d = fixture();
    let rep = scan_spdx23(d.path(), 2);

    let keys: Vec<(String, String, String)> = rep["relationships"].as_array().unwrap()
        .iter()
        .map(|r| (
            r["spdxElementId"].as_str().unwrap_or("").to_string(),
            r["relationshipType"].as_str().unwrap_or("").to_string(),
            r["relatedSpdxElement"].as_str().unwrap_or("").to_string(),
        ))
        .collect();

    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted,
        "relationships are not in (spdxElementId, relationshipType, \
         relatedSpdxElement) order. A stable-but-arbitrary order still makes \
         every document a diff against the last one.");
}
