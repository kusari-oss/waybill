//! Parsing `docs/reference/sbom-invariants.md` and gating it against
//! `REGISTRY`, both directions.
//!
//! Mirrors `every_catalog_row_has_an_extractor`
//! (`waybill-cli/src/parity/extractors/mod.rs:922`), which already
//! asserts both directions. FR-008 requires rows-without-checkers; the
//! other direction is adopted because a checker with no catalog row is
//! an invariant nobody can discover or review.

use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRow {
    pub id: String,
    pub statement: String,
    pub precondition: String,
    pub kinds: BTreeSet<String>,
    pub retired: bool,
}

pub fn catalog_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("docs")
        .join("reference")
        .join("sbom-invariants.md")
}

pub(crate) fn strip_backticks(s: &str) -> String {
    s.trim().trim_matches('`').trim().to_string()
}

/// Parse the catalog table. Rows are recognised by an `I<n>` first cell,
/// so the surrounding prose and the terminology table cannot be mistaken
/// for catalog rows.
pub fn parse(markdown: &str) -> Vec<CatalogRow> {
    let mut rows = Vec::new();
    for line in markdown.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 5 {
            continue;
        }
        let id = cells[0];
        let is_id = id.len() >= 2
            && id.starts_with('I')
            && id[1..].chars().all(|c| c.is_ascii_digit());
        if !is_id {
            continue;
        }
        let status = cells[4];
        rows.push(CatalogRow {
            id: id.to_string(),
            statement: cells[1].to_string(),
            precondition: cells[2].to_string(),
            kinds: cells[3]
                .split(',')
                .map(strip_backticks)
                .filter(|k| !k.is_empty() && k != "—")
                .collect(),
            retired: status.eq_ignore_ascii_case("retired"),
        });
    }
    rows
}

/// `### I<n>` diagnostic sections present in the document (FR-005).
pub fn diagnostic_sections(markdown: &str) -> BTreeSet<String> {
    markdown
        .lines()
        .filter_map(|l| l.trim().strip_prefix("### "))
        .map(|s| s.trim().to_string())
        .filter(|s| {
            s.len() >= 2 && s.starts_with('I') && s[1..].chars().all(|c| c.is_ascii_digit())
        })
        .collect()
}

pub fn load() -> String {
    std::fs::read_to_string(catalog_path())
        .unwrap_or_else(|e| panic!("read {}: {e}", catalog_path().display()))
}
