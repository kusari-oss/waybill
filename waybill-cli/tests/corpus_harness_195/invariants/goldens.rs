//! Loading the committed golden triples as `EmittedSboms`.
//!
//! The plan assumed the matrix could only run in the dispatched corpus
//! lane, because that is where documents get emitted. It turns out the
//! 17 golden triples under
//! `waybill-cli/tests/fixtures/public_corpus/<target>/` are complete —
//! they carry `dependencies[]`, every `waybill:*` property and both
//! subject pointers — and the masking layer 2 applies only touches
//! hashes, serial numbers, timestamps and the workspace path, none of
//! which any invariant reads.
//!
//! So the matrix is measurable **hermetically**, in the default
//! `cargo test` lane, with no network and no container pulls. That is
//! strictly better than the plan assumed: it means the gate, the
//! declarations file and the mutation check all run for every developer
//! on every commit, rather than only in a dispatched lane.
//!
//! The lane still evaluates freshly emitted documents (see
//! `public_corpus.rs`). The two are complementary: goldens catch a
//! refresh that imports a defect, the lane catches a defect before a
//! refresh hides it.

use std::path::PathBuf;

use super::super::harness::{EmittedPaths, EmittedSboms};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("public_corpus")
}

/// Load one target's golden triple. `None` when the target has no
/// committed goldens — a legitimate state for a newly added target
/// before its first refresh.
pub fn load(target: &str) -> Option<EmittedSboms> {
    let dir = fixtures_root().join(target);
    let cdx_path = dir.join("cdx.json");
    let spdx23_path = dir.join("spdx-2.3.json");
    let spdx3_path = dir.join("spdx-3.json");
    let read = |p: &PathBuf| -> Option<serde_json::Value> {
        let bytes = std::fs::read(p).ok()?;
        serde_json::from_slice(&bytes).ok()
    };
    Some(EmittedSboms {
        cdx: read(&cdx_path)?,
        spdx_2_3: read(&spdx23_path)?,
        spdx_3: read(&spdx3_path)?,
        paths: EmittedPaths { cdx: cdx_path, spdx_2_3: spdx23_path, spdx_3: spdx3_path },
    })
}

/// Every manifest target that has committed goldens, in manifest order.
pub fn load_all() -> Vec<(String, EmittedSboms)> {
    super::super::manifest::TARGETS
        .iter()
        .filter_map(|t| load(t.name).map(|s| (t.name.to_string(), s)))
        .collect()
}
