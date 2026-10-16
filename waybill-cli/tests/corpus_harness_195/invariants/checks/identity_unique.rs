//! I2 — `(name, version, type)` identifies at most one component.
//!
//! The same package emitted twice under one identity double-counts it in
//! any consumer that aggregates by name and version, and makes "how many
//! dependencies does this have" wrong.
//!
//! Kinds split on component **type** deliberately: a duplicated file and
//! a duplicated library are different defects with different fixes, and
//! one kind covering both would let a declaration accepting the first
//! silently absorb the second (FR-012).

use std::collections::BTreeMap;

use super::super::{CellOutcome, Violation};
use super::cdx_components;

pub const KIND_LIBRARY: &str = "duplicate-library-identity";
pub const KIND_FILE: &str = "duplicate-file-identity";
pub const KIND_APPLICATION: &str = "duplicate-application-identity";
/// A component type the corpus has never emitted. Present so the
/// checker cannot produce a kind the catalog row does not list — the
/// gate fails on that, and it would fail at evaluation time on some
/// future target rather than here.
pub const KIND_OTHER: &str = "duplicate-other-identity";
pub const KINDS: &[&str] = &[KIND_LIBRARY, KIND_FILE, KIND_APPLICATION, KIND_OTHER];

fn kind_for_type(ty: &str) -> &'static str {
    match ty {
        "library" => KIND_LIBRARY,
        "file" => KIND_FILE,
        "application" => KIND_APPLICATION,
        _ => KIND_OTHER,
    }
}

pub fn check(sboms: &super::EmittedSboms) -> CellOutcome {
    let components = cdx_components(&sboms.cdx);
    if components.len() < 2 {
        return CellOutcome::NotApplicable {
            reason: format!("{} component(s); identity collision needs at least 2", components.len()),
        };
    }

    let mut seen: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    for c in components {
        let name = c.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
        let version = c.get("version").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let ty = c.get("type").and_then(|t| t.as_str()).unwrap_or("").to_string();
        *seen.entry((name, version, ty)).or_insert(0) += 1;
    }

    let mut per_kind: BTreeMap<&'static str, (usize, Option<String>)> = BTreeMap::new();
    for ((name, version, ty), n) in seen {
        if n < 2 {
            continue;
        }
        let kind = kind_for_type(&ty);
        let entry = per_kind.entry(kind).or_insert((0, None));
        entry.0 += 1;
        if entry.1.is_none() {
            entry.1 = Some(format!("{ty} {name}@{version} x{n}"));
        }
    }
    if per_kind.is_empty() {
        return CellOutcome::Pass;
    }
    CellOutcome::Fail(
        per_kind
            .into_iter()
            .map(|(kind, (count, example))| Violation { kind, count, example })
            .collect(),
    )
}
