//! I6 — a package does not appear under several different
//! unknown-version spellings.
//!
//! One package appearing as both absent-version and `v0.0.0-unknown` is
//! counted twice and neither entry is matchable against an advisory
//! feed.
//!
//! **What is not a sentinel matters as much as what is.**
//! `v0.0.0-20260928230214-8a89bd6388cc` is a legitimate Go
//! pseudo-version and `0.0.0.0` is a legitimate .NET version. A
//! substring match on `0.0.0` flags roughly forty innocent components
//! across the committed corpus, so the comparison is against an exact
//! set. A gate that reddens on innocent data is the gate that gets
//! switched off.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{CellOutcome, Violation};
use super::cdx_components;

pub const KIND_MIXED_UNKNOWN_VERSION_SPELLING: &str = "mixed-unknown-version-spelling";
pub const KINDS: &[&str] = &[KIND_MIXED_UNKNOWN_VERSION_SPELLING];

/// Exact spellings that mean "we could not resolve a version". Measured
/// across the 17 committed goldens.
pub const UNKNOWN_SENTINELS: &[&str] = &["", "unknown", "v0.0.0-unknown", "UNKNOWN", "NOASSERTION"];

fn sentinel_of(component: &serde_json::Value) -> Option<&'static str> {
    match component.get("version").and_then(|v| v.as_str()) {
        None => Some("<absent>"),
        Some(v) => UNKNOWN_SENTINELS.iter().find(|s| **s == v.trim()).map(|s| {
            if s.is_empty() {
                "<empty>"
            } else {
                *s
            }
        }),
    }
}

pub fn check(sboms: &super::EmittedSboms) -> CellOutcome {
    let components = cdx_components(&sboms.cdx);

    // name -> the distinct sentinel spellings seen for it
    let mut per_name: BTreeMap<&str, BTreeSet<&'static str>> = BTreeMap::new();
    let mut any = false;
    for c in components {
        let Some(s) = sentinel_of(c) else { continue };
        any = true;
        let name = c.get("name").and_then(|n| n.as_str()).unwrap_or("");
        per_name.entry(name).or_default().insert(s);
    }
    if !any {
        return CellOutcome::NotApplicable {
            reason: "no component carries an unknown-version sentinel".to_string(),
        };
    }

    let mut offenders: Vec<String> = per_name
        .into_iter()
        .filter(|(_, spellings)| spellings.len() > 1)
        .map(|(name, spellings)| {
            let list: Vec<&str> = spellings.into_iter().collect();
            format!("{name}: {}", list.join(", "))
        })
        .collect();
    if offenders.is_empty() {
        return CellOutcome::Pass;
    }
    offenders.sort();
    let count = offenders.len();
    CellOutcome::Fail(vec![Violation {
        kind: KIND_MIXED_UNKNOWN_VERSION_SPELLING,
        count,
        example: offenders.into_iter().next(),
    }])
}
