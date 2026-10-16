//! FR-014c — a declaration entry must be shaped so that an unrelated
//! change does not move it.
//!
//! This is the requirement milestone 117 had to retrofit onto
//! `walk.audit-allowlist.txt`, whose `<file>:<line>:<content>` shape
//! made every entry below an edit churn. A reviewer reading this file's
//! diff should see exactly which cells were accepted, released or
//! re-accepted — and nothing else.

use super::declarations::{self, Declaration, DeclarationsFile};

fn render(file: &DeclarationsFile) -> String {
    file.declaration
        .iter()
        .map(|d| {
            format!(
                "[[declaration]]\ninvariant = \"{}\"\ntarget = \"{}\"\nkind = \"{}\"\n\
                 count_at_declaration = {}\nreason = \"\"\"\n{}\n\"\"\"\n{}\n",
                d.invariant,
                d.target,
                d.kind,
                d.count_at_declaration,
                d.reason.trim(),
                d.issue.map(|i| format!("issue = {i}")).unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn sorted_insert(mut file: DeclarationsFile, d: Declaration) -> DeclarationsFile {
    file.declaration.push(d);
    file.declaration.sort_by(|a, b| {
        (a.invariant.as_str(), a.target.as_str()).cmp(&(b.invariant.as_str(), b.target.as_str()))
    });
    file
}

/// Adding an entry inserts at its sorted position and moves no other
/// entry's content; removing it restores the file exactly.
#[test]
fn adding_then_removing_a_declaration_leaves_every_other_entry_unchanged() {
    let original = declarations::load();
    let before = render(&original);

    let added = sorted_insert(
        DeclarationsFile { declaration: original.declaration.clone() },
        Declaration {
            // Sorts into the middle of the file, not at either end —
            // appending at the end would pass this test vacuously.
            invariant: "I3".to_string(),
            target: "go-cobra".to_string(),
            kind: "unmarked-non-runtime-reachable".to_string(),
            count_at_declaration: 1,
            reason: "line-stability probe".to_string(),
            issue: None,
        },
    );
    let with_extra = render(&added);
    assert_ne!(before, with_extra, "the probe entry must actually have been inserted");

    // Every original entry's rendered block must still appear verbatim.
    for d in &original.declaration {
        let block = render(&DeclarationsFile { declaration: vec![d.clone()] });
        assert!(
            with_extra.contains(block.trim()),
            "inserting a declaration moved or rewrote {}/{}'s content",
            d.invariant,
            d.target
        );
    }

    // And removing it restores the original byte-for-byte.
    let mut removed = added;
    removed
        .declaration
        .retain(|d| !(d.invariant == "I3" && d.target == "go-cobra" && d.issue.is_none()));
    assert_eq!(
        before,
        render(&removed),
        "removing a declaration must restore the file exactly"
    );
}

/// Adding a corpus target touches only that target's own entries.
#[test]
fn adding_a_target_does_not_rewrite_unrelated_entries() {
    let original = declarations::load();
    let before = render(&original);
    let added = sorted_insert(
        DeclarationsFile { declaration: original.declaration.clone() },
        Declaration {
            invariant: "I1".to_string(),
            target: "aaa-brand-new-target".to_string(),
            kind: "file-tier-dependency-edge".to_string(),
            count_at_declaration: 3,
            reason: "a new target's first declared cell".to_string(),
            issue: Some(1206),
        },
    );
    let after = render(&added);
    for d in &original.declaration {
        let block = render(&DeclarationsFile { declaration: vec![d.clone()] });
        assert!(after.contains(block.trim()), "{}/{} moved", d.invariant, d.target);
    }
    assert!(after.len() > before.len());
}
