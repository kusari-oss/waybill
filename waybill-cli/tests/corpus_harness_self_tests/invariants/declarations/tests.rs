#[allow(unused_imports)]
use std::collections::BTreeSet;
#[allow(unused_imports)]
use std::path::PathBuf;
use crate::corpus_harness_195::invariants::declarations::*;

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
