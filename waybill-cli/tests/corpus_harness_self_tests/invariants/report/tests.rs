#[allow(unused_imports)]
use std::collections::{BTreeMap, BTreeSet};
#[allow(unused_imports)]
use std::path::PathBuf;
#[allow(unused_imports)]
use crate::corpus_harness_195::manifest::{EcosystemId, TARGETS};
#[allow(unused_imports)]
use crate::corpus_harness_195::invariants::declarations::DeclarationsFile;
#[allow(unused_imports)]
use crate::corpus_harness_195::invariants::{applicability, Cell, CellOutcome, Verdict};
use crate::corpus_harness_195::invariants::report::*;

/// T056 — the universe is derived from the document, and the enum is
/// gated against it in both directions. Without this, a doc rename
/// quietly shrinks the covered set instead of reddening the lane.
#[test]
fn ecosystem_id_and_the_coverage_matrix_agree() {
    let universe = load_ecosystem_universe();
    assert!(!universe.is_empty(), "parsed no rows from docs/ecosystems.md coverage matrix");

    let variants: BTreeSet<String> =
        EcosystemId::ALL.iter().map(|e| e.doc_row().to_string()).collect();

    let rows_without_variant: Vec<&String> = universe.difference(&variants).collect();
    assert!(
        rows_without_variant.is_empty(),
        "coverage-matrix rows with no EcosystemId variant: {rows_without_variant:?} — \
         add the variants, or the covered/uncovered statement silently omits them"
    );
    let variants_without_row: Vec<&String> = variants.difference(&universe).collect();
    assert!(
        variants_without_row.is_empty(),
        "EcosystemId variants with no coverage-matrix row: {variants_without_row:?}"
    );
}

/// The second table in the document also has `Ecosystem` as its
/// first column; a parser keying on that alone reads it too.
#[test]
fn only_the_coverage_matrix_is_parsed() {
    let md = "\
| Ecosystem | Detection source | X |\n\
|---|---|---|\n\
| [apk](#apk) | a | b |\n\
\n\
| Ecosystem | Design-tier fallback? | Y |\n\
|---|---|---|\n\
| [zzz](#zzz) | a | b |\n";
    let u = parse_ecosystem_universe(md);
    assert!(u.contains("apk"));
    assert!(!u.contains("zzz"), "read rows from the design-tier table: {u:?}");
}

/// SC-005 — the uncovered set is non-empty and names gem, which is
/// why #1211 was unobservable from this corpus.
#[test]
fn uncovered_ecosystems_are_named_and_include_gem() {
    let (covered, uncovered) = coverage();
    assert!(!covered.is_empty());
    assert!(
        !uncovered.is_empty(),
        "a green matrix must carry a caveat; an empty uncovered set would be the claim \
         that the corpus sees everything"
    );
    assert!(uncovered.contains("gem"), "expected gem uncovered, got {uncovered:?}");
}
