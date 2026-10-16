//! The committed declarations file — the only committed state in the
//! feature and the only thing the gate compares against (FR-014b).
//!
//! A declaration is a recorded decision to accept a known failing cell.
//! It is how a scoped fix becomes a visible row rather than a sentence
//! in a spec's Edge Cases.
//!
//! Contract: `specs/1155-invariant-major-corpus/contracts/declarations.md`.

use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Declaration {
    pub invariant: String,
    pub target: String,
    /// The pinned part. A different kind, or an additional kind, fails
    /// the lane (FR-012).
    pub kind: String,
    /// Context only — **never** compared. Counts drift for reasons
    /// unrelated to waybill, and a gate that reddens on innocent drift
    /// is the gate that gets switched off (milestone 117).
    pub count_at_declaration: usize,
    pub reason: String,
    pub issue: Option<u32>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct DeclarationsFile {
    #[serde(default)]
    pub declaration: Vec<Declaration>,
}

pub fn declarations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus_harness_195")
        .join("invariant-declarations.toml")
}

pub fn load() -> DeclarationsFile {
    let path = declarations_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Validation that must happen before any comparison (FR-001c, FR-010).
/// An invalid declarations file fails the lane rather than being
/// partially honoured — a skipped declaration is an invisible exemption.
pub fn validate(file: &DeclarationsFile) -> Vec<String> {
    let mut errors = Vec::new();
    let known_targets: BTreeSet<&str> =
        super::super::manifest::TARGETS.iter().map(|t| t.name).collect();
    let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();

    for d in &file.declaration {
        match super::invariant(&d.invariant) {
            None => errors.push(format!(
                "declaration names unknown or retired invariant `{}` (FR-001c)",
                d.invariant
            )),
            Some(inv) => {
                if !inv.kinds.contains(&d.kind.as_str()) {
                    errors.push(format!(
                        "declaration {}/{} names kind `{}`, which is not one of {}'s kinds {:?}",
                        d.invariant, d.target, d.kind, d.invariant, inv.kinds
                    ));
                }
            }
        }
        if !known_targets.contains(d.target.as_str()) {
            errors.push(format!(
                "declaration {}/{} names a target absent from the corpus manifest",
                d.invariant, d.target
            ));
        }
        if d.reason.trim().is_empty() {
            errors.push(format!(
                "declaration {}/{} has an empty reason (FR-010)",
                d.invariant, d.target
            ));
        }
        if !seen.insert((d.invariant.as_str(), d.target.as_str())) {
            errors.push(format!(
                "duplicate declaration for ({}, {}) — a cell has at most one",
                d.invariant, d.target
            ));
        }
    }

    // FR-014c line stability: entries sorted by (invariant, target).
    let keys: Vec<(&str, &str)> =
        file.declaration.iter().map(|d| (d.invariant.as_str(), d.target.as_str())).collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    if keys != sorted {
        errors.push(
            "declarations are not sorted by (invariant, target); sorting is what keeps an \
             unrelated change from moving an entry (FR-014c)"
                .to_string(),
        );
    }
    errors
}

impl DeclarationsFile {
    pub fn find(&self, invariant: &str, target: &str) -> Option<&Declaration> {
        self.declaration.iter().find(|d| d.invariant == invariant && d.target == target)
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn the_committed_declarations_file_is_valid() {
        let errors = validate(&load());
        assert!(errors.is_empty(), "declarations file is invalid:\n  {}", errors.join("\n  "));
    }

    #[test]
    fn an_unknown_invariant_is_rejected_not_skipped() {
        let f: DeclarationsFile = toml::from_str(
            r#"
[[declaration]]
invariant = "I999"
target = "go-cobra"
kind = "whatever"
count_at_declaration = 1
reason = "nonsense"
"#,
        )
        .unwrap();
        let errors = validate(&f);
        assert!(
            errors.iter().any(|e| e.contains("I999")),
            "an unknown invariant must fail the lane, not be ignored: {errors:?}"
        );
    }

    #[test]
    fn an_unknown_target_is_rejected() {
        let f: DeclarationsFile = toml::from_str(
            r#"
[[declaration]]
invariant = "I1"
target = "no-such-target"
kind = "file-tier-dependency-edge"
count_at_declaration = 1
reason = "nonsense"
"#,
        )
        .unwrap();
        assert!(validate(&f).iter().any(|e| e.contains("no-such-target")));
    }

    #[test]
    fn a_kind_outside_the_invariants_kinds_is_rejected() {
        let f: DeclarationsFile = toml::from_str(
            r#"
[[declaration]]
invariant = "I1"
target = "go-cobra"
kind = "duplicate-library-identity"
count_at_declaration = 1
reason = "wrong invariant's kind"
"#,
        )
        .unwrap();
        assert!(validate(&f).iter().any(|e| e.contains("not one of")));
    }

    #[test]
    fn an_empty_reason_is_rejected() {
        let f: DeclarationsFile = toml::from_str(
            r#"
[[declaration]]
invariant = "I1"
target = "go-cobra"
kind = "file-tier-dependency-edge"
count_at_declaration = 1
reason = "   "
"#,
        )
        .unwrap();
        assert!(validate(&f).iter().any(|e| e.contains("empty reason")));
    }

    #[test]
    fn duplicate_cells_are_rejected() {
        let f: DeclarationsFile = toml::from_str(
            r#"
[[declaration]]
invariant = "I1"
target = "go-cobra"
kind = "file-tier-dependency-edge"
count_at_declaration = 1
reason = "first"

[[declaration]]
invariant = "I1"
target = "go-cobra"
kind = "file-tier-dependency-edge"
count_at_declaration = 2
reason = "second"
"#,
        )
        .unwrap();
        assert!(validate(&f).iter().any(|e| e.contains("duplicate declaration")));
    }

    #[test]
    fn unsorted_entries_are_rejected() {
        let f: DeclarationsFile = toml::from_str(
            r#"
[[declaration]]
invariant = "I2"
target = "go-cobra"
kind = "duplicate-library-identity"
count_at_declaration = 1
reason = "second alphabetically, first in file"

[[declaration]]
invariant = "I1"
target = "go-cobra"
kind = "file-tier-dependency-edge"
count_at_declaration = 1
reason = "first alphabetically, second in file"
"#,
        )
        .unwrap();
        assert!(validate(&f).iter().any(|e| e.contains("not sorted")));
    }
}
