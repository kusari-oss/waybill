//! Divergent-PURL collision detection types (milestone 134).
//!
//! When the per-ecosystem main-module dedup at scan time finds
//! 2+ manifest files claiming the same `pkg:<ecosystem>/<name>@<version>`
//! identity but with different declared direct-dep sets OR different
//! deep-hashes, the scan-side code constructs a [`DivergenceRecord`]
//! and forwards it through the SBOM emission pipeline.
//!
//! Both this type and [`CollisionsSummary`] (the document-scope
//! aggregate) are written to the wire as JSON envelopes per the
//! `contracts/per-component-property.md` and
//! `contracts/document-scope-annotation.md` specs at
//! `specs/134-divergent-purl-detection/contracts/`.
//!
//! Design intent (FR-010): ecosystem-agnostic. The cargo reader is
//! the first caller (milestone 134), but the structure carries no
//! cargo-specific fields. Future npm / maven / pip / gem / go-binary
//! follow-ups populate the same shape at their own dedup sites.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::types::purl::Purl;

/// Wire-format schema version. Bumped only on incompatible payload
/// changes. Pinned to `1` for milestone 134.
pub const DIVERGENCE_SCHEMA_VERSION: u32 = 1;

/// Reason a same-PURL collision was classified as divergent.
///
/// Two orthogonal axes — declared-dep-set divergence (always
/// checked) and deep-hash divergence (only checked under
/// `--deep-hash`). `Both` reports the combined verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DivergenceReason {
    /// Declared direct dep sets differ across colliding manifests.
    DepsDiffer,
    /// Deep hashes differ across colliding manifests (`--deep-hash`).
    HashesDiffer,
    /// Both declared dep sets AND deep hashes differ.
    Both,
}

/// One detected divergent-PURL collision.
///
/// Constructed at the per-ecosystem dedup site and forwarded
/// through the SBOM emission pipeline. Lives on the deduped root
/// component as a `mikebom:duplicate-purl-divergent` property AND
/// inside the document-scope `mikebom:purl-collisions-detected`
/// summary's `collisions[]` array.
///
/// Validation invariants (enforced via [`DivergenceRecord::validate`]):
///
/// 1. `v == DIVERGENCE_SCHEMA_VERSION` for this milestone.
/// 2. `paths.len() >= 2` (a collision requires at least 2 manifests).
/// 3. `dep_sets_by_path.is_some()` iff `reason ∈ { DepsDiffer, Both }`.
/// 4. `hashes_by_path.is_some()` iff `reason ∈ { HashesDiffer, Both }`.
/// 5. When `dep_sets_by_path` is `Some`, its key set equals
///    `paths` as a set.
/// 6. Same for `hashes_by_path`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DivergenceRecord {
    pub v: u32,
    pub purl: Purl,
    pub reason: DivergenceReason,
    /// Every manifest path that participated in the collision, in
    /// filesystem-walk discovery order (deterministic — sorted entries
    /// per the walker's invariant). Always 2+ entries.
    pub paths: Vec<String>,
    /// Per-path declared direct dep names. Sorted lexicographically.
    /// Populated when `reason ∈ { DepsDiffer, Both }`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dep_sets_by_path: Option<BTreeMap<String, Vec<String>>>,
    /// Per-path deep-hash hex strings. Populated when
    /// `reason ∈ { HashesDiffer, Both }` AND `--deep-hash` is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashes_by_path: Option<BTreeMap<String, String>>,
}

impl DivergenceRecord {
    /// Verify the validation invariants documented on the struct.
    /// Returns `Err` with a short diagnostic when any invariant is
    /// violated.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.v != DIVERGENCE_SCHEMA_VERSION {
            return Err("DivergenceRecord.v != DIVERGENCE_SCHEMA_VERSION");
        }
        if self.paths.len() < 2 {
            return Err("DivergenceRecord.paths.len() < 2");
        }
        let deps_present = self.dep_sets_by_path.is_some();
        let hashes_present = self.hashes_by_path.is_some();
        match self.reason {
            DivergenceReason::DepsDiffer => {
                if !deps_present {
                    return Err("reason=DepsDiffer but dep_sets_by_path is None");
                }
                if hashes_present {
                    return Err("reason=DepsDiffer but hashes_by_path is Some");
                }
            }
            DivergenceReason::HashesDiffer => {
                if !hashes_present {
                    return Err("reason=HashesDiffer but hashes_by_path is None");
                }
                if deps_present {
                    return Err("reason=HashesDiffer but dep_sets_by_path is Some");
                }
            }
            DivergenceReason::Both => {
                if !deps_present {
                    return Err("reason=Both but dep_sets_by_path is None");
                }
                if !hashes_present {
                    return Err("reason=Both but hashes_by_path is None");
                }
            }
        }
        if let Some(deps) = &self.dep_sets_by_path {
            for path in &self.paths {
                if !deps.contains_key(path) {
                    return Err("dep_sets_by_path missing entry for a path");
                }
            }
            for key in deps.keys() {
                if !self.paths.contains(key) {
                    return Err("dep_sets_by_path has key not in paths");
                }
            }
        }
        if let Some(hashes) = &self.hashes_by_path {
            for path in &self.paths {
                if !hashes.contains_key(path) {
                    return Err("hashes_by_path missing entry for a path");
                }
            }
            for key in hashes.keys() {
                if !self.paths.contains(key) {
                    return Err("hashes_by_path has key not in paths");
                }
            }
        }
        Ok(())
    }
}

/// Document-scope aggregate of every divergent collision detected in
/// the scan. Emitted ONLY when `collisions` is non-empty (FR-009 —
/// the absence of this annotation IS the no-collision signal).
///
/// `collisions` is sorted lexically by `record.purl.as_str()` so
/// the wire output is deterministic across runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollisionsSummary {
    pub v: u32,
    pub collisions: Vec<DivergenceRecord>,
}

impl CollisionsSummary {
    /// Construct from an unordered iterator of records. Sorts the
    /// `collisions` vec deterministically by PURL string.
    pub fn from_records(records: impl IntoIterator<Item = DivergenceRecord>) -> Self {
        let mut collisions: Vec<DivergenceRecord> = records.into_iter().collect();
        collisions.sort_by(|a, b| a.purl.as_str().cmp(b.purl.as_str()));
        Self {
            v: DIVERGENCE_SCHEMA_VERSION,
            collisions,
        }
    }

    /// `Ok(())` when every record validates AND the summary
    /// invariants hold (schema version + non-empty when emitted).
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.v != DIVERGENCE_SCHEMA_VERSION {
            return Err("CollisionsSummary.v != DIVERGENCE_SCHEMA_VERSION");
        }
        if self.collisions.is_empty() {
            return Err("CollisionsSummary.collisions must be non-empty when emitted");
        }
        for record in &self.collisions {
            record.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn purl(s: &str) -> Purl {
        Purl::new(s).unwrap()
    }

    fn dep_set(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn deps_differ_record_validates() {
        let mut deps = BTreeMap::new();
        deps.insert("crates/foo/Cargo.toml".to_string(), dep_set(&["serde", "tokio"]));
        deps.insert(
            "vendor/foo/Cargo.toml".to_string(),
            dep_set(&["anyhow", "serde", "tokio"]),
        );
        let record = DivergenceRecord {
            v: DIVERGENCE_SCHEMA_VERSION,
            purl: purl("pkg:cargo/foo@1.2.3"),
            reason: DivergenceReason::DepsDiffer,
            paths: vec![
                "crates/foo/Cargo.toml".to_string(),
                "vendor/foo/Cargo.toml".to_string(),
            ],
            dep_sets_by_path: Some(deps),
            hashes_by_path: None,
        };
        assert!(record.validate().is_ok());
    }

    #[test]
    fn hashes_differ_record_validates() {
        let mut hashes = BTreeMap::new();
        hashes.insert("crates/foo/Cargo.toml".to_string(), "aa".repeat(32));
        hashes.insert("vendor/foo/Cargo.toml".to_string(), "bb".repeat(32));
        let record = DivergenceRecord {
            v: DIVERGENCE_SCHEMA_VERSION,
            purl: purl("pkg:cargo/foo@1.2.3"),
            reason: DivergenceReason::HashesDiffer,
            paths: vec![
                "crates/foo/Cargo.toml".to_string(),
                "vendor/foo/Cargo.toml".to_string(),
            ],
            dep_sets_by_path: None,
            hashes_by_path: Some(hashes),
        };
        assert!(record.validate().is_ok());
    }

    #[test]
    fn both_record_requires_both_maps() {
        let mut deps = BTreeMap::new();
        deps.insert("a".to_string(), dep_set(&["x"]));
        deps.insert("b".to_string(), dep_set(&["x", "y"]));
        let record_missing_hashes = DivergenceRecord {
            v: DIVERGENCE_SCHEMA_VERSION,
            purl: purl("pkg:cargo/foo@1.2.3"),
            reason: DivergenceReason::Both,
            paths: vec!["a".to_string(), "b".to_string()],
            dep_sets_by_path: Some(deps),
            hashes_by_path: None,
        };
        assert!(record_missing_hashes.validate().is_err());
    }

    #[test]
    fn fewer_than_two_paths_invalid() {
        let mut deps = BTreeMap::new();
        deps.insert("only/Cargo.toml".to_string(), dep_set(&["x"]));
        let record = DivergenceRecord {
            v: DIVERGENCE_SCHEMA_VERSION,
            purl: purl("pkg:cargo/foo@1.2.3"),
            reason: DivergenceReason::DepsDiffer,
            paths: vec!["only/Cargo.toml".to_string()],
            dep_sets_by_path: Some(deps),
            hashes_by_path: None,
        };
        assert!(record.validate().is_err());
    }

    #[test]
    fn dep_keys_must_match_paths() {
        let mut deps = BTreeMap::new();
        deps.insert("a".to_string(), dep_set(&["x"]));
        // Missing "b" in deps map even though "b" is in paths.
        let record = DivergenceRecord {
            v: DIVERGENCE_SCHEMA_VERSION,
            purl: purl("pkg:cargo/foo@1.2.3"),
            reason: DivergenceReason::DepsDiffer,
            paths: vec!["a".to_string(), "b".to_string()],
            dep_sets_by_path: Some(deps),
            hashes_by_path: None,
        };
        assert!(record.validate().is_err());
    }

    #[test]
    fn collisions_summary_sorts_by_purl_string() {
        let mut deps_z = BTreeMap::new();
        deps_z.insert("a".to_string(), dep_set(&["x"]));
        deps_z.insert("b".to_string(), dep_set(&["x", "y"]));
        let record_z = DivergenceRecord {
            v: DIVERGENCE_SCHEMA_VERSION,
            purl: purl("pkg:cargo/zzz@1.0.0"),
            reason: DivergenceReason::DepsDiffer,
            paths: vec!["a".to_string(), "b".to_string()],
            dep_sets_by_path: Some(deps_z),
            hashes_by_path: None,
        };
        let mut deps_a = BTreeMap::new();
        deps_a.insert("c".to_string(), dep_set(&["x"]));
        deps_a.insert("d".to_string(), dep_set(&["x", "y"]));
        let record_a = DivergenceRecord {
            v: DIVERGENCE_SCHEMA_VERSION,
            purl: purl("pkg:cargo/aaa@1.0.0"),
            reason: DivergenceReason::DepsDiffer,
            paths: vec!["c".to_string(), "d".to_string()],
            dep_sets_by_path: Some(deps_a),
            hashes_by_path: None,
        };
        // Insert out of order.
        let summary = CollisionsSummary::from_records(vec![record_z, record_a]);
        assert_eq!(summary.collisions.len(), 2);
        // After construction, sorted by PURL string.
        assert_eq!(summary.collisions[0].purl.as_str(), "pkg:cargo/aaa@1.0.0");
        assert_eq!(summary.collisions[1].purl.as_str(), "pkg:cargo/zzz@1.0.0");
        assert!(summary.validate().is_ok());
    }

    #[test]
    fn empty_summary_invalid_when_emitted() {
        let summary = CollisionsSummary {
            v: DIVERGENCE_SCHEMA_VERSION,
            collisions: Vec::new(),
        };
        assert!(summary.validate().is_err());
    }
}
