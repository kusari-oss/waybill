#[allow(unused_imports)]
use std::collections::BTreeSet;
#[allow(unused_imports)]
use std::path::PathBuf;
use crate::corpus_harness_195::invariants::catalog::*;
use crate::corpus_harness_195::invariants::REGISTRY;

/// FR-008 / FR-001c / FR-012a / FR-005 — the bidirectional gate.
/// Hermetic: no network, no corpus, runs in the default lane.
#[test]
fn catalog_and_registry_agree() {
    let md = load();
    let rows = parse(&md);
    assert!(!rows.is_empty(), "no catalog rows parsed from {}", catalog_path().display());

    let active: Vec<&CatalogRow> = rows.iter().filter(|r| !r.retired).collect();
    let active_ids: BTreeSet<&str> = active.iter().map(|r| r.id.as_str()).collect();
    let retired_ids: BTreeSet<&str> =
        rows.iter().filter(|r| r.retired).map(|r| r.id.as_str()).collect();
    let registry_ids: BTreeSet<&str> = REGISTRY.iter().map(|i| i.id).collect();

    // Direction 1 (FR-008): an active row with no checker.
    let missing: Vec<&&str> = active_ids.difference(&registry_ids).collect();
    assert!(
        missing.is_empty(),
        "catalog rows with no registry entry: {missing:?} — add a checker or retire the row"
    );

    // Direction 2: a checker nobody can review.
    let orphans: Vec<&&str> = registry_ids.difference(&active_ids).collect();
    assert!(
        orphans.is_empty(),
        "registry entries with no active catalog row: {orphans:?} — add a row to {}",
        catalog_path().display()
    );

    // A retired row must have no checker (numbering rule 4).
    let revived: Vec<&&str> = registry_ids.intersection(&retired_ids).collect();
    assert!(revived.is_empty(), "registry entries for retired rows: {revived:?}");

    // IDs ascending and unique.
    let numbers: Vec<u32> =
        rows.iter().filter_map(|r| r.id[1..].parse::<u32>().ok()).collect();
    let mut sorted = numbers.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        numbers, sorted,
        "catalog IDs must be unique and ascending, including retired rows"
    );

    let diagnostics = diagnostic_sections(&md);
    for row in &active {
        assert!(
            !row.kinds.is_empty(),
            "{} is Active with no Kinds (FR-012a) — a checker that can only report a \
             count cannot satisfy FR-012",
            row.id
        );
        assert!(
            !row.precondition.is_empty() && row.precondition != "—",
            "{} is Active with no Precondition (FR-006)",
            row.id
        );
        // FR-005 — the diagnostic is the most valuable part of the
        // existing per-target layer and must not be skipped.
        assert!(
            diagnostics.contains(&row.id),
            "{} has no `### {}` diagnostic section (FR-005)",
            row.id,
            row.id
        );
    }
}

/// A checker may only emit kinds its catalog row lists — in both
/// directions. Without this, a reviewer approving a declaration
/// cannot know what they are accepting.
#[test]
fn kind_sets_agree() {
    let rows = parse(&load());
    for inv in REGISTRY {
        let row = rows
            .iter()
            .find(|r| r.id == inv.id)
            .unwrap_or_else(|| panic!("{} missing from the catalog", inv.id));
        let registry_kinds: BTreeSet<String> =
            inv.kinds.iter().map(|k| (*k).to_string()).collect();
        assert_eq!(
            registry_kinds, row.kinds,
            "{} kind sets disagree — registry {:?} vs catalog {:?}",
            inv.id, registry_kinds, row.kinds
        );
    }
}

#[test]
fn statements_and_preconditions_match_the_catalog() {
    let rows = parse(&load());
    for inv in REGISTRY {
        let row = rows.iter().find(|r| r.id == inv.id).unwrap();
        // Statements are wrapped in the source; compare on collapsed
        // whitespace so a line break is not a mismatch.
        let collapse = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(
            collapse(inv.statement),
            collapse(&row.statement),
            "{} statement drifted from the catalog",
            inv.id
        );
        assert_eq!(
            collapse(inv.precondition),
            collapse(&row.precondition),
            "{} precondition drifted from the catalog",
            inv.id
        );
    }
}

#[test]
fn the_terminology_table_is_not_mistaken_for_catalog_rows() {
    // The doc opens with a two-row | Term | Means | Lives in | table.
    // A parser keying on column count rather than an `I<n>` first
    // cell would read those as invariants.
    let rows = parse(&load());
    assert!(rows.iter().all(|r| r.id.starts_with('I')));
    assert!(rows.iter().all(|r| r.id[1..].parse::<u32>().is_ok()));
}
