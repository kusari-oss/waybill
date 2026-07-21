//! Milestone 167 — emit-time `waybill:orphan-reason` classifier for
//! Go + npm orphans.
//!
//! Extends the milestone-061 C45 vocabulary from 2 codes
//! (`unresolved-indirect-require`, `flat-attached-fallback` — both
//! Go-only, both emitted at Go-reader-time in
//! `scan_fs/package_db/golang/legacy.rs:2091,2118`) to 5 codes covering
//! Go + npm orphans per milestone-165 audit classifications.
//!
//! Runs AFTER `compute_graph_completeness` (m158) so the
//! BFS-reachability set is already available. Iterates each Go/npm
//! component; components NOT in the reachable set are BFS-unreachable
//! per the milestone-167 orphan definition (Q1 clarification 2026-07-06).
//! Emits the most-specific reason code per FR-005 priority. Preserves
//! `flat-attached-fallback` set by the Go reader — never overwrites it
//! (backward-compat guard).
//!
//! Per-scan `tracing::info!` fires from the CALL SITE (not this module)
//! after the classifier returns, using [`OrphanReasonCounts`] fields to
//! populate the FR-008 grep-friendly log line.
//!
//! See:
//!   - `specs/167-orphan-reason-expand/spec.md` FR-001 through FR-010
//!   - `specs/167-orphan-reason-expand/data-model.md` entities E2/E3/E4
//!   - `docs/reference/sbom-format-mapping.md` C45 row

use std::collections::{HashMap, HashSet};

use waybill_common::resolution::{Relationship, ResolvedComponent};

use crate::generate::graph_completeness::{
    build_workspace_peer_edges, compute_graph_completeness,
};
use crate::generate::root_selector::{select_root, ResolvedRootSubject};
use crate::generate::RootComponentOverride;
use crate::scan_fs::package_db::maven::ScanTargetCoord;

const ANNOTATION_KEY: &str = "waybill:orphan-reason";

/// The C45 `waybill:orphan-reason` vocabulary after milestone 167 lands.
/// Total 5 codes: 2 preserved from m061 + 3 new.
///
/// Priority order (most-specific to least-specific per FR-005):
///   1. `stale-go-sum-entry` — Go multi-version case
///   2. `dead-lockfile-entry` — npm multi-version case
///   3. `hoisted-unused` — npm no-sibling case
///   4. `unresolved-indirect-require` — Go no-sibling case (m061)
///   5. `flat-attached-fallback` — Go reader-time backfill (m061,
///      NEVER overwritten)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrphanReasonCode {
    StaleGoSumEntry,
    DeadLockfileEntry,
    HoistedUnused,
    UnresolvedIndirectRequire,
    FlatAttachedFallback,
}

impl OrphanReasonCode {
    /// Wire value for the `waybill:orphan-reason` annotation. These
    /// literal strings are frozen — SBOM consumers key on them.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StaleGoSumEntry => "stale-go-sum-entry",
            Self::DeadLockfileEntry => "dead-lockfile-entry",
            Self::HoistedUnused => "hoisted-unused",
            Self::UnresolvedIndirectRequire => "unresolved-indirect-require",
            Self::FlatAttachedFallback => "flat-attached-fallback",
        }
    }
}

/// Per-code drop count returned by [`classify_orphans`]. The call site
/// uses these fields to populate FR-008's `tracing::info!` line with
/// grep-friendly per-code counters.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct OrphanReasonCounts {
    pub stale_go_sum_entry: usize,
    pub dead_lockfile_entry: usize,
    pub hoisted_unused: usize,
    pub unresolved_indirect_require: usize,
    pub flat_attached_fallback: usize,
}

impl OrphanReasonCounts {
    /// Increment the counter for the given code.
    pub fn tally(&mut self, code: OrphanReasonCode) {
        match code {
            OrphanReasonCode::StaleGoSumEntry => self.stale_go_sum_entry += 1,
            OrphanReasonCode::DeadLockfileEntry => self.dead_lockfile_entry += 1,
            OrphanReasonCode::HoistedUnused => self.hoisted_unused += 1,
            OrphanReasonCode::UnresolvedIndirectRequire => {
                self.unresolved_indirect_require += 1
            }
            OrphanReasonCode::FlatAttachedFallback => {
                self.flat_attached_fallback += 1
            }
        }
    }
}

/// Emit-time classifier — stamps `waybill:orphan-reason` on BFS-
/// unreachable Go/npm components per FR-005 priority. Returns
/// per-code counters for observability (FR-008).
///
/// Called from `scan_fs::mod.rs` after
/// `compute_graph_completeness` populates
/// `GraphCompletenessResult.reachable_set` (milestone-158's BFS output).
///
/// Algorithm (per data-model.md E3):
///
/// 1. Build a `by_name` index — `(ecosystem, name) → Vec<PURL string>`
///    over all Go/npm components. Used for O(1) same-name sibling
///    lookup.
/// 2. Iterate every Go/npm component. For each: (a) skip if PURL is in
///    `reachable_set` (not orphan); (b) skip if the component already
///    carries `waybill:orphan-reason=flat-attached-fallback` (set by
///    the Go reader) — preserve + tally FlatAttachedFallback;
///    (c) compute `has_reachable_sibling` via the index; (d) pattern-
///    match on `(ecosystem, has_reachable_sibling)` — `(golang, true)`
///    → `StaleGoSumEntry`, `(npm, true)` → `DeadLockfileEntry`,
///    `(npm, false)` → `HoistedUnused`, `(golang, false)` →
///    `UnresolvedIndirectRequire`; (e) insert into `extra_annotations`
///    (overwrites any existing Go-reader-time
///    `unresolved-indirect-require` when a same-name sibling is present
///    — FR-005 priority refinement).
///
/// Complexity: O(N) build + O(N) iterate + O(S) sibling check =
/// O(N × S) where S = max same-name group size. On real fixtures
/// N ≤ 3000, S ≤ 5, so sub-millisecond per scan.
pub fn classify_orphans(
    components: &mut [ResolvedComponent],
    reachable_set: &HashSet<String>,
) -> OrphanReasonCounts {
    // Step 1: build (ecosystem, name) → Vec<PURL> index over all Go/npm
    // components. This lets us check "same-name reachable sibling
    // exists" in O(S) where S = same-name group size.
    let mut by_name: HashMap<(String, String), Vec<String>> = HashMap::new();
    for c in components.iter() {
        let ecosystem = c.purl.ecosystem();
        if ecosystem != "npm" && ecosystem != "golang" {
            continue;
        }
        let key = (ecosystem.to_string(), c.purl.name().to_string());
        by_name.entry(key).or_default().push(c.purl.as_str().to_string());
    }

    let mut counts = OrphanReasonCounts::default();

    // Step 2: iterate every Go/npm component; classify orphans.
    for c in components.iter_mut() {
        let ecosystem = c.purl.ecosystem().to_string();
        if ecosystem != "npm" && ecosystem != "golang" {
            continue;
        }
        let purl_str = c.purl.as_str().to_string();

        // 2a. Non-orphan (reachable via BFS) → skip.
        if reachable_set.contains(&purl_str) {
            continue;
        }

        // 2b. Preserve `flat-attached-fallback` set by the Go reader —
        // never overwrite (m061 backward-compat guard).
        //
        // Note: components carrying `flat-attached-fallback` typically
        // HAVE an incoming backfill edge from the Go main-module (see
        // `golang/legacy.rs:2101-2107` — the annotation semantic
        // widened to "incoming edge attribution unknown / synthesized"),
        // so they'll usually be BFS-reachable and short-circuit at 2a
        // above, never reaching this arm. In that case the annotation
        // value is preserved on the wire (no insert runs) but the
        // `counts.flat_attached_fallback` counter under-reports — the
        // counter tallies only the (BFS-unreachable ∩ Go-reader-tagged)
        // intersection, which is the correct FR-008 semantic.
        if let Some(existing) = c
            .extra_annotations
            .get(ANNOTATION_KEY)
            .and_then(|v| v.as_str())
        {
            if existing == "flat-attached-fallback" {
                counts.tally(OrphanReasonCode::FlatAttachedFallback);
                continue;
            }
        }

        // 2c. Same-name reachable sibling check.
        let key = (ecosystem.clone(), c.purl.name().to_string());
        let has_reachable_sibling = by_name
            .get(&key)
            .map(|siblings| {
                siblings
                    .iter()
                    .any(|s| s != &purl_str && reachable_set.contains(s))
            })
            .unwrap_or(false);

        // 2d. FR-005 priority via ecosystem-partitioned pattern match.
        let code = match (ecosystem.as_str(), has_reachable_sibling) {
            ("golang", true) => OrphanReasonCode::StaleGoSumEntry,
            ("npm", true) => OrphanReasonCode::DeadLockfileEntry,
            ("npm", false) => OrphanReasonCode::HoistedUnused,
            ("golang", false) => OrphanReasonCode::UnresolvedIndirectRequire,
            // Unreachable — outer filter guarantees ecosystem is one of
            // {golang, npm}. If a new ecosystem is added to the outer
            // filter later without extending this match, the compiler
            // won't help us — so degrade gracefully by leaving the
            // component un-annotated rather than panicking.
            _ => continue,
        };

        counts.tally(code);
        c.extra_annotations.insert(
            ANNOTATION_KEY.to_string(),
            serde_json::Value::String(code.as_str().to_string()),
        );
    }

    counts
}

/// Pre-emission wrapper — runs the milestone-158 graph-completeness
/// setup (root selection + workspace-peer edges + BFS reachability)
/// once, then invokes [`classify_orphans`] on the resulting reachable
/// set. Called from `cli::scan_cmd` immediately before the neutral
/// `ScanArtifacts` bundle is built, so the annotations land on the
/// shared `components` Vec and flow through unchanged into every
/// format emitter (CDX / SPDX 2.3 / SPDX 3).
///
/// Emits the FR-008 grep-friendly `tracing::info!` log with per-code
/// counters. Returns the counters for the caller's use (currently
/// unused beyond the log).
///
/// The per-format emitters continue to invoke `compute_graph_
/// completeness` for their own document-scope `waybill:graph-
/// completeness` annotation (C42). BFS is deterministic — the emitters
/// re-compute the same reachable set — so per-component orphan-reason
/// annotations agree with per-document graph-completeness signals
/// without cross-cutting cache infrastructure.
pub fn classify_orphans_pre_emit(
    components: &mut [ResolvedComponent],
    relationships: &[Relationship],
    root_override: &RootComponentOverride,
    scan_target_coord: Option<&ScanTargetCoord>,
    target_name: &str,
) -> OrphanReasonCounts {
    // Mirror the CDX builder's target-version placeholder (`"0.0.0"`).
    // The graph-completeness pass uses `target_ref` as a fallback seed
    // when no main-module component is present; when a main-module IS
    // present, its PURL is the primary seed and this placeholder is
    // ignored. Format-specific target-ref shapes (SPDX IRIs vs CDX
    // PURLs) don't influence orphan classification because reachability
    // is over PURL keys.
    let target_version = "0.0.0";

    let selection = select_root(
        components,
        root_override,
        scan_target_coord,
        target_name,
        target_version,
    );

    // Mirror the CDX builder's target_ref derivation (`builder.rs:430`):
    // main-module PURL wins; else `<name>@<version>` fallback.
    let target_ref: String = match &selection.subject {
        ResolvedRootSubject::MainModule(idx) => components
            .get(*idx)
            .map(|c| c.purl.as_str().to_string())
            .unwrap_or_else(|| format!("{target_name}@{target_version}")),
        _ => format!("{target_name}@{target_version}"),
    };

    let peer_edges = build_workspace_peer_edges(&selection, components);
    let augmented: Vec<Relationship> = relationships
        .iter()
        .cloned()
        .chain(peer_edges)
        .collect();

    let gc = compute_graph_completeness(
        components,
        &augmented,
        &selection,
        &target_ref,
    );

    let counts = classify_orphans(components, &gc.reachable_set);

    // FR-008 — per-scan info-level log with per-code counters, one line
    // per scan. Grep-friendly per the milestone-157-onwards convention.
    // Zero counters indicate a healthy scan.
    tracing::info!(
        orphan_reason_stale_go_sum_entry = counts.stale_go_sum_entry,
        orphan_reason_dead_lockfile_entry = counts.dead_lockfile_entry,
        orphan_reason_hoisted_unused = counts.hoisted_unused,
        orphan_reason_unresolved_indirect_require = counts.unresolved_indirect_require,
        orphan_reason_flat_attached_fallback = counts.flat_attached_fallback,
        "orphan-reason classification complete"
    );

    counts
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::resolution::{
        ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
    };
    use waybill_common::types::purl::Purl;
    use serde_json::json;

    /// Build a minimal test component from a PURL string.
    fn mk(purl_str: &str) -> ResolvedComponent {
        let purl = Purl::new(purl_str).expect("valid purl");
        ResolvedComponent {
            build_inclusion: None,
            name: purl.name().to_string(),
            version: purl.version().unwrap_or("0.0.0").to_string(),
            purl,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::PackageDatabase,
                confidence: 0.85,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: Vec::new(),
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: Vec::new(),
            extra_annotations: Default::default(),
            binary_role: None,
        }
    }

    fn reason_of(c: &ResolvedComponent) -> Option<&str> {
        c.extra_annotations.get(ANNOTATION_KEY).and_then(|v| v.as_str())
    }

    // ---------------------------------------------------------------
    // Skeleton tests preserved from T004.
    // ---------------------------------------------------------------

    #[test]
    fn as_str_returns_frozen_wire_values() {
        // The C45 wire values are frozen — SBOM consumers key on them.
        // Any change to these strings is a wire-format break.
        assert_eq!(OrphanReasonCode::StaleGoSumEntry.as_str(), "stale-go-sum-entry");
        assert_eq!(OrphanReasonCode::DeadLockfileEntry.as_str(), "dead-lockfile-entry");
        assert_eq!(OrphanReasonCode::HoistedUnused.as_str(), "hoisted-unused");
        assert_eq!(
            OrphanReasonCode::UnresolvedIndirectRequire.as_str(),
            "unresolved-indirect-require"
        );
        assert_eq!(
            OrphanReasonCode::FlatAttachedFallback.as_str(),
            "flat-attached-fallback"
        );
    }

    #[test]
    fn counts_default_is_all_zero() {
        let c = OrphanReasonCounts::default();
        assert_eq!(c.stale_go_sum_entry, 0);
        assert_eq!(c.dead_lockfile_entry, 0);
        assert_eq!(c.hoisted_unused, 0);
        assert_eq!(c.unresolved_indirect_require, 0);
        assert_eq!(c.flat_attached_fallback, 0);
    }

    #[test]
    fn tally_increments_the_right_counter() {
        let mut c = OrphanReasonCounts::default();
        c.tally(OrphanReasonCode::StaleGoSumEntry);
        c.tally(OrphanReasonCode::StaleGoSumEntry);
        c.tally(OrphanReasonCode::HoistedUnused);
        assert_eq!(c.stale_go_sum_entry, 2);
        assert_eq!(c.hoisted_unused, 1);
        assert_eq!(c.dead_lockfile_entry, 0);
        assert_eq!(c.unresolved_indirect_require, 0);
        assert_eq!(c.flat_attached_fallback, 0);
    }

    // ---------------------------------------------------------------
    // T005 [US1] — Go stale-go-sum-entry (same-name sibling reachable).
    // ---------------------------------------------------------------

    #[test]
    fn t005_stale_go_sum_entry_emitted_when_go_orphan_with_reachable_sibling() {
        let mut components = vec![
            mk("pkg:golang/k8s.io/api@v0.30.0"),   // reachable
            mk("pkg:golang/k8s.io/api@v0.28.0"),   // orphan, sibling reachable
        ];
        let mut reachable_set = HashSet::new();
        reachable_set.insert("pkg:golang/k8s.io/api@v0.30.0".to_string());

        let counts = classify_orphans(&mut components, &reachable_set);

        assert_eq!(counts.stale_go_sum_entry, 1);
        assert_eq!(reason_of(&components[0]), None);
        assert_eq!(reason_of(&components[1]), Some("stale-go-sum-entry"));
    }

    // ---------------------------------------------------------------
    // T006 [US1] — npm dead-lockfile-entry (same-name sibling reachable).
    // ---------------------------------------------------------------

    #[test]
    fn t006_dead_lockfile_entry_emitted_when_npm_orphan_with_reachable_sibling() {
        let mut components = vec![
            mk("pkg:npm/lodash@4.17.20"),  // reachable
            mk("pkg:npm/lodash@4.17.15"),  // orphan, sibling reachable
        ];
        let mut reachable_set = HashSet::new();
        reachable_set.insert("pkg:npm/lodash@4.17.20".to_string());

        let counts = classify_orphans(&mut components, &reachable_set);

        assert_eq!(counts.dead_lockfile_entry, 1);
        assert_eq!(reason_of(&components[0]), None);
        assert_eq!(reason_of(&components[1]), Some("dead-lockfile-entry"));
    }

    // ---------------------------------------------------------------
    // T007 [US1] — npm hoisted-unused (no same-name sibling).
    // ---------------------------------------------------------------

    #[test]
    fn t007_hoisted_unused_emitted_when_npm_orphan_no_sibling() {
        let mut components = vec![
            mk("pkg:npm/some-hoisted@1.0.0"), // orphan, no sibling
        ];
        let reachable_set = HashSet::new(); // empty; all orphan

        let counts = classify_orphans(&mut components, &reachable_set);

        assert_eq!(counts.hoisted_unused, 1);
        assert_eq!(reason_of(&components[0]), Some("hoisted-unused"));
    }

    // ---------------------------------------------------------------
    // T008 [US1] — FR-005 priority: multi-version case wins over
    // single-version case for the same ecosystem.
    // ---------------------------------------------------------------

    #[test]
    fn t008_fr005_priority_multi_version_wins_over_single() {
        // A Go component that's orphan AND has a same-name reachable
        // sibling MUST get `stale-go-sum-entry` (multi-version case)
        // — NOT `unresolved-indirect-require` (no-sibling case).
        //
        // Also: an npm component that's orphan AND has a same-name
        // reachable sibling MUST get `dead-lockfile-entry` — NOT
        // `hoisted-unused`.
        let mut components = vec![
            mk("pkg:golang/example.com/foo@v1.2.0"),  // reachable
            mk("pkg:golang/example.com/foo@v1.1.0"),  // orphan → stale-go-sum-entry
            mk("pkg:npm/react@18.2.0"),               // reachable
            mk("pkg:npm/react@17.0.2"),               // orphan → dead-lockfile-entry
        ];
        let mut reachable_set = HashSet::new();
        reachable_set.insert("pkg:golang/example.com/foo@v1.2.0".to_string());
        reachable_set.insert("pkg:npm/react@18.2.0".to_string());

        let counts = classify_orphans(&mut components, &reachable_set);

        assert_eq!(counts.stale_go_sum_entry, 1);
        assert_eq!(counts.dead_lockfile_entry, 1);
        assert_eq!(counts.unresolved_indirect_require, 0);
        assert_eq!(counts.hoisted_unused, 0);
        assert_eq!(reason_of(&components[1]), Some("stale-go-sum-entry"));
        assert_eq!(reason_of(&components[3]), Some("dead-lockfile-entry"));
    }

    // ---------------------------------------------------------------
    // T013 [US2] — flat-attached-fallback (from Go reader) preserved,
    // never overwritten.
    // ---------------------------------------------------------------

    #[test]
    fn t013_preserves_flat_attached_fallback_from_go_reader_time() {
        let mut c = mk("pkg:golang/example.com/backfilled@v1.0.0");
        c.extra_annotations.insert(
            ANNOTATION_KEY.to_string(),
            json!("flat-attached-fallback"),
        );
        let mut components = vec![c];
        let reachable_set = HashSet::new(); // orphan by BFS

        let counts = classify_orphans(&mut components, &reachable_set);

        assert_eq!(counts.flat_attached_fallback, 1);
        assert_eq!(counts.unresolved_indirect_require, 0);
        assert_eq!(counts.stale_go_sum_entry, 0);
        // Value MUST still be `flat-attached-fallback` — not overwritten
        // by the classifier's fall-through arm.
        assert_eq!(reason_of(&components[0]), Some("flat-attached-fallback"));
    }

    // ---------------------------------------------------------------
    // T014 [US2] — m061 `unresolved-indirect-require` semantic preserved
    // when no same-name sibling.
    // ---------------------------------------------------------------

    #[test]
    fn t014_preserves_unresolved_indirect_require_when_no_sibling() {
        // Simulates the m061 Go-reader-time emission: component already
        // carries `unresolved-indirect-require` AND has no same-name
        // reachable sibling. Classifier's `(golang, false)` arm writes
        // the same value (idempotent), so the final annotation is
        // still `unresolved-indirect-require`.
        let mut c = mk("pkg:golang/example.com/only-indirect@v0.1.0");
        c.extra_annotations.insert(
            ANNOTATION_KEY.to_string(),
            json!("unresolved-indirect-require"),
        );
        let mut components = vec![c];
        let reachable_set = HashSet::new();

        let counts = classify_orphans(&mut components, &reachable_set);

        assert_eq!(counts.unresolved_indirect_require, 1);
        assert_eq!(counts.stale_go_sum_entry, 0);
        assert_eq!(reason_of(&components[0]), Some("unresolved-indirect-require"));
    }

    // ---------------------------------------------------------------
    // T015 [US2] — m061 `unresolved-indirect-require` gets refined to
    // `stale-go-sum-entry` when a same-name reachable sibling is present.
    // ---------------------------------------------------------------

    #[test]
    fn t015_overwrites_unresolved_indirect_require_when_sibling_present() {
        // Simulates a component pre-annotated by the Go reader with
        // `unresolved-indirect-require` (m061 semantic) but there's a
        // same-name reachable sibling in this scan. m167 refines to
        // `stale-go-sum-entry` per FR-005.
        let mut c_orphan = mk("pkg:golang/example.com/refined@v0.9.0");
        c_orphan.extra_annotations.insert(
            ANNOTATION_KEY.to_string(),
            json!("unresolved-indirect-require"),
        );
        let mut components = vec![
            mk("pkg:golang/example.com/refined@v1.0.0"), // reachable sibling
            c_orphan,
        ];
        let mut reachable_set = HashSet::new();
        reachable_set.insert("pkg:golang/example.com/refined@v1.0.0".to_string());

        let counts = classify_orphans(&mut components, &reachable_set);

        assert_eq!(counts.stale_go_sum_entry, 1);
        assert_eq!(counts.unresolved_indirect_require, 0);
        assert_eq!(reason_of(&components[1]), Some("stale-go-sum-entry"));
    }

    // ---------------------------------------------------------------
    // T018 [US3] — non-orphan component receives NO annotation
    // (three-state semantics: absent on non-orphans).
    // ---------------------------------------------------------------

    #[test]
    fn t018_non_orphan_receives_no_annotation() {
        let mut components = vec![
            mk("pkg:golang/example.com/reachable@v1.0.0"),
            mk("pkg:npm/reachable-npm@1.0.0"),
        ];
        let mut reachable_set = HashSet::new();
        reachable_set.insert("pkg:golang/example.com/reachable@v1.0.0".to_string());
        reachable_set.insert("pkg:npm/reachable-npm@1.0.0".to_string());

        let counts = classify_orphans(&mut components, &reachable_set);

        assert_eq!(counts.stale_go_sum_entry, 0);
        assert_eq!(counts.dead_lockfile_entry, 0);
        assert_eq!(counts.hoisted_unused, 0);
        assert_eq!(counts.unresolved_indirect_require, 0);
        assert_eq!(counts.flat_attached_fallback, 0);
        assert_eq!(reason_of(&components[0]), None);
        assert_eq!(reason_of(&components[1]), None);
    }

    // ---------------------------------------------------------------
    // T019 [US3] — non-Go/npm ecosystems are NOT touched by the
    // classifier, even when BFS-unreachable.
    // ---------------------------------------------------------------

    #[test]
    fn t019_non_go_npm_ecosystem_unaffected() {
        let mut components = vec![
            mk("pkg:cargo/serde@1.0.0"),          // orphan, but Cargo → skip
            mk("pkg:maven/org.example/foo@1.0"),  // orphan, but Maven → skip
            mk("pkg:pypi/requests@2.28.0"),       // orphan, but PyPI → skip
        ];
        let reachable_set = HashSet::new();

        let counts = classify_orphans(&mut components, &reachable_set);

        // ALL counters must be zero.
        assert_eq!(counts, OrphanReasonCounts::default());
        // None of the non-Go/npm components acquire the annotation.
        for c in &components {
            assert_eq!(reason_of(c), None);
        }
    }
}
