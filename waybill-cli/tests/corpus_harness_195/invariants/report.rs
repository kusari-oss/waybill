//! The matrix report — console grid (FR-014) and the per-run JSON
//! artifact (FR-014a), plus the ecosystem coverage statement (FR-015).
//!
//! The artifact is what makes reading a fix's reach a diff rather than
//! an investigation (SC-003), so its `cells` are sorted and its shape is
//! stable.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::super::manifest::{EcosystemId, TARGETS};
use super::declarations::DeclarationsFile;
use super::{applicability, Cell, CellOutcome, Verdict};

// -----------------------------------------------------------------------
// Ecosystem coverage — FR-015
// -----------------------------------------------------------------------

fn ecosystems_doc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("docs").join("ecosystems.md")
}

/// The coverage matrix's row labels, with the markdown link wrapper
/// stripped: `[golang](#golang)` → `golang`.
///
/// Parses the **first** table whose header starts with
/// `| Ecosystem | Detection source |`. The document carries a second
/// table whose first column is also `Ecosystem` (the design-tier
/// fallback matrix), so keying on the word alone reads the wrong rows.
pub fn parse_ecosystem_universe(markdown: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut in_matrix = false;
    for line in markdown.lines() {
        let line = line.trim();
        if line.starts_with("| Ecosystem |") {
            in_matrix = line.contains("Detection source");
            continue;
        }
        if !line.starts_with('|') {
            if in_matrix && !line.is_empty() {
                in_matrix = false;
            }
            continue;
        }
        if !in_matrix || line.starts_with("|---") {
            continue;
        }
        let first = line.trim_matches('|').split('|').next().unwrap_or("").trim();
        let label = strip_md_link(first);
        if !label.is_empty() {
            out.insert(label);
        }
    }
    out
}

/// `[golang](#golang)` → `golang`; `pants (Python)` → `pants (Python)`.
fn strip_md_link(cell: &str) -> String {
    let c = cell.trim();
    if let Some(rest) = c.strip_prefix('[') {
        if let Some(end) = rest.find("](") {
            return rest[..end].to_string();
        }
    }
    c.to_string()
}

pub fn load_ecosystem_universe() -> BTreeSet<String> {
    let p = ecosystems_doc_path();
    let text =
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    parse_ecosystem_universe(&text)
}

/// Ecosystems at least one corpus target exercises, and those none does.
pub fn coverage() -> (BTreeSet<String>, BTreeSet<String>) {
    let universe = load_ecosystem_universe();
    let covered: BTreeSet<String> = TARGETS
        .iter()
        .flat_map(|t| t.reader_ecosystems.iter())
        .map(|e| e.doc_row().to_string())
        .collect();
    let uncovered: BTreeSet<String> = universe.difference(&covered).cloned().collect();
    (covered, uncovered)
}

// -----------------------------------------------------------------------
// The report
// -----------------------------------------------------------------------

pub struct MatrixReport {
    pub cells: Vec<Cell>,
    pub verdicts: BTreeMap<(String, String), Verdict>,
    pub applicability: BTreeMap<&'static str, usize>,
    pub ecosystems_covered: BTreeSet<String>,
    pub ecosystems_uncovered: BTreeSet<String>,
    pub unfalsifiable: Vec<&'static str>,
}

pub fn build(cells: Vec<Cell>, declarations: &DeclarationsFile) -> MatrixReport {
    let verdicts = cells
        .iter()
        .map(|c| {
            let d = declarations.find(c.invariant, &c.target);
            ((c.invariant.to_string(), c.target.clone()), super::verdict(&c.outcome, d))
        })
        .collect();
    let (covered, uncovered) = coverage();
    MatrixReport {
        applicability: applicability(&cells),
        unfalsifiable: super::unfalsifiable_invariants(&cells),
        ecosystems_covered: covered,
        ecosystems_uncovered: uncovered,
        verdicts,
        cells,
    }
}

impl MatrixReport {
    pub fn lane_failures(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .verdicts
            .iter()
            .filter(|(_, v)| v.fails_lane())
            .map(|((inv, target), v)| {
                let observation = self
                    .cells
                    .iter()
                    .find(|c| c.invariant == inv && &c.target == target)
                    .map(|c| describe(&c.outcome))
                    .unwrap_or_default();
                format!("{inv} / {target}: {v:?} — {observation}")
            })
            .collect();
        for id in &self.unfalsifiable {
            out.push(format!(
                "{id}: not applicable on every corpus target — unfalsifiable here, so its row \
                 of dashes cannot be told from a working check (FR-006a)"
            ));
        }
        out.sort();
        out
    }

    /// The grid, failures and declarations ahead of passes (FR-014).
    pub fn console(&self) -> String {
        let mut s = String::new();
        // Rows come from the cells actually evaluated, not from the full
        // manifest: the lane evaluates one target at a time, and a grid
        // listing all seventeen with sixteen rows of `?` would read as
        // sixteen unevaluated targets rather than one evaluated one.
        let mut targets: Vec<&str> = Vec::new();
        for t in TARGETS.iter().map(|t| t.name) {
            if self.cells.iter().any(|c| c.target == t) {
                targets.push(t);
            }
        }
        let width = targets.iter().map(|t| t.len()).max().unwrap_or(10);

        s.push_str("invariant matrix — . pass  X fail  - n/a  E error  (d) declared\n\n");
        s.push_str(&format!("{:width$} ", "target", width = width));
        for inv in super::REGISTRY {
            s.push_str(&format!(" {:>3}", inv.id));
        }
        s.push('\n');
        for t in &targets {
            s.push_str(&format!("{:width$} ", t, width = width));
            for inv in super::REGISTRY {
                let cell = self.cells.iter().find(|c| c.invariant == inv.id && c.target == *t);
                let glyph = match cell {
                    Some(c) => {
                        let declared = self
                            .verdicts
                            .get(&(inv.id.to_string(), (*t).to_string()))
                            .map(|v| matches!(v, Verdict::AcceptedKnownFailure))
                            .unwrap_or(false);
                        if declared { 'd' } else { c.outcome.glyph() }
                    }
                    None => '?',
                };
                s.push_str(&format!("   {glyph}"));
            }
            s.push('\n');
        }

        let failures = self.lane_failures();
        if failures.is_empty() {
            s.push_str("\nno lane failures\n");
        } else {
            s.push_str(&format!("\n{} lane failure(s):\n", failures.len()));
            for f in &failures {
                s.push_str(&format!("  {f}\n"));
            }
        }

        s.push_str("\napplicability (targets whose precondition was met — FR-015a):\n");
        for (id, n) in &self.applicability {
            s.push_str(&format!("  {id}: {n}/{}\n", targets.len()));
        }

        // A green grid always carries its caveat (US4).
        s.push_str(&format!(
            "\necosystem coverage — {} of {} documented ecosystems exercised\n",
            self.ecosystems_covered.len(),
            self.ecosystems_covered.len() + self.ecosystems_uncovered.len()
        ));
        s.push_str(&format!(
            "  no corpus target exercises: {}\n",
            self.ecosystems_uncovered.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
        s.push_str(
            "  a passing matrix says nothing about those; see docs/ecosystems.md\n",
        );
        s
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "cells": self.cells.iter().map(|c| serde_json::json!({
                "invariant": c.invariant,
                "target": c.target,
                "state": state_name(&c.outcome),
                "observation": describe(&c.outcome),
                "kinds": c.outcome.kinds().into_iter().collect::<Vec<_>>(),
                "verdict": format!("{:?}", self.verdicts
                    .get(&(c.invariant.to_string(), c.target.clone()))
                    .unwrap_or(&Verdict::Ok)),
            })).collect::<Vec<_>>(),
            "applicability": self.applicability.iter()
                .map(|(k, v)| (k.to_string(), *v)).collect::<BTreeMap<String, usize>>(),
            "ecosystems_covered": self.ecosystems_covered.iter().cloned().collect::<Vec<_>>(),
            "ecosystems_uncovered": self.ecosystems_uncovered.iter().cloned().collect::<Vec<_>>(),
            "unfalsifiable": self.unfalsifiable,
        })
    }

    /// Write the per-run artifact. Uncommitted by design (FR-014a) —
    /// `target/` is already gitignored.
    pub fn write_artifact(&self, run_id: &str) -> std::io::Result<PathBuf> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("target")
            .join("invariants");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("run-{run_id}.json"));
        std::fs::write(&path, serde_json::to_vec_pretty(&self.to_json())?)?;
        Ok(path)
    }
}

fn state_name(o: &CellOutcome) -> &'static str {
    match o {
        CellOutcome::Pass => "pass",
        CellOutcome::Fail(_) => "fail",
        CellOutcome::NotApplicable { .. } => "not-applicable",
        CellOutcome::Error { .. } => "error",
    }
}

fn describe(o: &CellOutcome) -> String {
    match o {
        CellOutcome::Pass => "holds".to_string(),
        CellOutcome::NotApplicable { reason } => format!("not applicable: {reason}"),
        CellOutcome::Error { message } => format!("checker error: {message}"),
        CellOutcome::Fail(vs) => vs
            .iter()
            .map(|v| {
                let ex = v.example.as_deref().unwrap_or("-");
                format!("{} x{} (e.g. {})", v.kind, v.count, ex)
            })
            .collect::<Vec<_>>()
            .join("; "),
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

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
}
