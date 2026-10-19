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
//! Per-scan `tracing::info!` fires from [`classify_orphans_pre_emit`] in
//! THIS module, just before it returns, using [`OrphanReasonCounts`] fields
//! to populate the FR-008 grep-friendly log line. It fires unconditionally,
//! so all-zero counters mean "nothing matched" and are distinguishable from
//! the classifier not having run at all.
//!
//! (This paragraph previously said the log fired from the call site. It does
//! not, and never did — the line is at `classify_orphans_pre_emit`. The
//! `let _m167_orphan_reason_counts = …` binding at `cli/scan_cmd.rs` is a
//! genuinely unused return value, not a dropped log. Corrected by feature
//! 1218, whose research notes record the half-day the wrong sentence cost.)
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

/// Milestone 866 — module identity for the same-name sibling check.
///
/// `Purl::name()` returns the LAST path segment, which for a Go module
/// with a major-version suffix is the suffix itself: both
/// `github.com/russross/blackfriday/v2` and
/// `github.com/cpuguy83/go-md2man/v2` report `name == "v2"`. Keying the
/// sibling index on that made every `/vN` module a "version" of every
/// other, so one reachable `/vN` module marked all the rest
/// `stale-go-sum-entry` — measured on `go-cobra`, where reachable
/// `go-md2man/v2` mislabelled orphaned `blackfriday/v2`.
///
/// The sibling check means "same module, different version", so the key
/// is the PURL with the version stripped.
fn versionless_identity(purl: &str) -> &str {
    match purl.split_once('@') {
        Some((base, _version)) => base,
        None => purl,
    }
}

/// Feature 1218 — lock files, whose entries record a *resolution*.
///
/// The `dead-lockfile-entry` arm requires membership here, so that a
/// manifest-declared coordinate can never reach it: a `requirements.txt`
/// entry (issue #1234) and a `pom.xml` entry (issue #1235) both state what
/// was *asked for*, not what was resolved, and neither is a resolution fork.
///
/// Deliberately an allowlist rather than a manifest denylist. A new lock
/// format is then invisible to this arm until someone adds it — the
/// component stays unexplained and keeps failing I7, which is the loud
/// failure. A denylist would silently admit every format nobody thought of.
const LOCKFILE_NAMES: &[&str] = &[
    "uv.lock",
    "poetry.lock",
    "Pipfile.lock",
    "pdm.lock",
    "Cargo.lock",
    "Gemfile.lock",
    "composer.lock",
    "pubspec.lock",
    "Podfile.lock",
    "mix.lock",
    "packages.lock.json",
];

/// Feature 1218 — lock files whose nodes reference each other, so that "no
/// other node references this one" is a meaningful statement about them.
///
/// Deliberately disjoint from [`LOCKFILE_NAMES`]: `flake.lock` is NOT in
/// that list, because mechanism D owns it. Both lists appear in one ordered
/// `match`, and B's arm precedes D's, so a `flake.lock` entry in both lists
/// would be claimed by B whenever it happened to have a reachable sibling —
/// giving one population two codes depending on an unrelated coincidence.
const NODE_GRAPH_LOCKFILE_NAMES: &[&str] = &["flake.lock"];

/// Feature 1218 — build-system manifests: files that *declare* what a build
/// wants, as opposed to lock files, which record what it resolved.
///
/// Mechanism C requires membership here, so that a component with no
/// provenance at all cannot be called a foreign build system. Note that
/// `requirements.txt` appears in BOTH this list and nothing else — it is a
/// manifest, never a lock file, which is the distinction issue #1234 turns
/// on.
const MANIFEST_NAMES: &[&str] = &[
    "WORKSPACE",
    "WORKSPACE.bazel",
    "MODULE.bazel",
    "BUILD",
    "BUILD.bazel",
    "requirements.txt",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "Cargo.toml",
    "package.json",
    "go.mod",
    "pyproject.toml",
    "CMakeLists.txt",
    "conanfile.txt",
    "vcpkg.json",
];

/// Feature 1218 — does any of this component's source paths name a file
/// called `name`?
///
/// Compares `Path::file_name`, never a substring of the whole path. The
/// paths on `ResolutionEvidence.source_file_paths` are **raw**: they may be
/// absolute, or scan-root-relative, or `path+file://<abs>`-shaped. The
/// spellings a reader sees in an emitted SBOM (`"examples/celery/requirements.txt"`)
/// are produced later, at emit time, by
/// `scan_fs::sbom_path::emitted_source_files` relativizing against the scan
/// roots — so a predicate that matched those spellings here would match
/// nothing. The m226 Pants Go reader hit the same asymmetry from the other
/// side ("the Go reader emits scan-root-relative source_paths, the index
/// stores absolute").
fn sourced_from_file_named(component: &ResolvedComponent, name: &str) -> bool {
    component.evidence.source_file_paths.iter().any(|p| {
        std::path::Path::new(p.trim_start_matches("path+file://"))
            .file_name()
            .is_some_and(|f| f == name)
    })
}

/// Feature 1218 — the workspace members this component was derived from,
/// per the `waybill:workspace-member` annotation.
///
/// The stored value is a JSON-array-encoded **string**, not a JSON array —
/// `scan_fs/mod.rs` writes `Value::String(serde_json::to_string(&sorted))`,
/// so the wire shape is `"[\"examples/celery\"]"`. Absent or unparseable
/// yields an empty Vec rather than failing the scan: this annotation is an
/// input to a diagnostic, and a scan must not die because one component's
/// provenance could not be read.
fn workspace_members(component: &ResolvedComponent) -> Vec<String> {
    component
        .extra_annotations
        .get("waybill:workspace-member")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

/// The C45 `waybill:orphan-reason` vocabulary. Total 8 codes: 2
/// preserved from m061, 3 from m167, 3 from feature 1218.
///
/// Priority order, most-specific to least-specific. The three feature-1218
/// codes sit strictly BELOW every m167 code, which is what makes feature
/// 1218's FR-004 ("every component carrying an m167 code still carries it")
/// hold by position rather than by case analysis:
///
///   0. `flat-attached-fallback` — Go reader-time backfill (m061,
///      NEVER overwritten)
///   1. `stale-go-sum-entry` — Go multi-version case
///   2. `dead-lockfile-entry` — lockfile multi-version case (npm since
///      m167, pypi since feature 1218)
///   3. `hoisted-unused` — npm no-sibling case
///   4. `unresolved-indirect-require` — Go no-sibling case (m061)
///   5. `build-toolchain` — a compiler or snapshot pin (1218)
///   6. `foreign-build-system` — a build system the subject does not use
///      (1218)
///   7. `unreferenced-lock-node` — a lockfile node nothing references
///      (1218)
///
/// See `specs/1218-explain-every-orphan/contracts/orphan-reason-vocabulary.md`
/// for the wire contract, including the per-value list of populations each
/// code MUST NOT be emitted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrphanReasonCode {
    StaleGoSumEntry,
    DeadLockfileEntry,
    HoistedUnused,
    UnresolvedIndirectRequire,
    FlatAttachedFallback,
    /// Feature 1218 mechanism E — a compiler, snapshot or toolchain pin.
    /// Build tooling, not a dependency of the subject.
    BuildToolchain,
    /// Feature 1218 mechanism C — declared by a build system the subject
    /// does not build with, co-located in the same repository.
    ForeignBuildSystem,
    /// Feature 1218 mechanism D — a node in a lockfile that no other node
    /// in that lockfile references.
    UnreferencedLockNode,
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
            Self::BuildToolchain => "build-toolchain",
            Self::ForeignBuildSystem => "foreign-build-system",
            Self::UnreferencedLockNode => "unreferenced-lock-node",
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
    pub build_toolchain: usize,
    pub foreign_build_system: usize,
    pub unreferenced_lock_node: usize,
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
            OrphanReasonCode::BuildToolchain => self.build_toolchain += 1,
            OrphanReasonCode::ForeignBuildSystem => {
                self.foreign_build_system += 1
            }
            OrphanReasonCode::UnreferencedLockNode => {
                self.unreferenced_lock_node += 1
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
    // Feature 1218 T011: the m167 guard `if ecosystem != "npm" &&
    // ecosystem != "golang" { continue; }` stood here. It is gone, so the
    // index spans every ecosystem — which mechanism B needs, and which is
    // also why the `dead-lockfile-entry` arm below cannot rely on "has a
    // reachable sibling" alone. See the comment on that arm.
    let mut by_name: HashMap<(String, String), Vec<String>> = HashMap::new();
    for c in components.iter() {
        let ecosystem = c.purl.ecosystem();
        let key = (
            ecosystem.to_string(),
            versionless_identity(c.purl.as_str()).to_string(),
        );
        by_name.entry(key).or_default().push(c.purl.as_str().to_string());
    }

    // Feature 1218 — the ecosystems that have at least one component
    // reachable from the subject. Mechanism C reads "the subject does not
    // build with this" off this set: an ecosystem with no reachable
    // component plays no part in the subject's dependency graph.
    //
    // Computed once here rather than per-component, so the arm stays O(1).
    let ecosystems_with_reachable: HashSet<String> = components
        .iter()
        .filter(|c| reachable_set.contains(c.purl.as_str()))
        .map(|c| c.purl.ecosystem().to_string())
        .collect();

    let mut counts = OrphanReasonCounts::default();

    // Step 2: iterate every component; classify orphans.
    //
    // Feature 1218 T011: the second m167 ecosystem guard stood here.
    for c in components.iter_mut() {
        let ecosystem = c.purl.ecosystem().to_string();
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

        // 2b-bis. Feature 1218 FR-008 — a component already explained by
        // one of invariant I7's other markers must not receive a second,
        // possibly contradictory one.
        //
        // This guard is NEW. Before feature 1218 the classifier only ever
        // looked at Go and npm components, which do not carry these
        // markers, so nothing needed it. Now it does, and the corpus has a
        // live case: `haskell-language-server` holds four unreachable
        // components — `docutils`, `myst-parser`, `sphinx`,
        // `sphinx-rtd-theme` — from `docs/requirements.txt`, already
        // explained by C151 `waybill:unresolved-reason`. Without this
        // guard the `foreign-build-system` arm claims them (pypi has no
        // reachable component in that document, and `requirements.txt` is
        // a build manifest), asserting that a Sphinx documentation
        // dependency is a foreign build system.
        //
        // The failure would be invisible to every count: because those
        // four are already explained they are not I7 offenders, so the
        // offender total stays at 16 and every success criterion still
        // passes. The marker set below mirrors
        // `invariants/checks/orphan_explained.rs::is_explained`.
        let already_explained = c
            .extra_annotations
            .contains_key("waybill:unresolved-reason")
            || c.extra_annotations.contains_key("waybill:closure-role")
            || c.extra_annotations
                .get("waybill:version-absence")
                .and_then(|v| v.as_str())
                == Some("by-construction")
            || c.extra_annotations
                .get("waybill:component-tier")
                .and_then(|v| v.as_str())
                == Some("file");
        if already_explained {
            continue;
        }

        // 2c. Same-name reachable sibling check.
        let key = (
            ecosystem.clone(),
            versionless_identity(&purl_str).to_string(),
        );
        let has_reachable_sibling = by_name
            .get(&key)
            .map(|siblings| {
                siblings
                    .iter()
                    .any(|s| s != &purl_str && reachable_set.contains(s))
            })
            .unwrap_or(false);

        // 2d. Priority order, most-specific first. The m167 arms come
        // first so that feature 1218's FR-004 — every component carrying an
        // m167 code still carries it — holds by POSITION rather than by
        // case analysis.
        let code = match (ecosystem.as_str(), has_reachable_sibling) {
            ("golang", true) => OrphanReasonCode::StaleGoSumEntry,
            ("npm", true) => OrphanReasonCode::DeadLockfileEntry,
            ("npm", false) => OrphanReasonCode::HoistedUnused,
            ("golang", false) => OrphanReasonCode::UnresolvedIndirectRequire,

            // Feature 1218 — every other ecosystem. Before T011 removed the
            // guards, this arm was `_ => continue`.
            _ => {
                // T013 — the lockfile multi-version case, generalizing
                // `dead-lockfile-entry` from npm to any lockfile.
                //
                // "Has a reachable same-name sibling" is NECESSARY BUT NOT
                // SUFFICIENT, and this is the single sharpest edge in the
                // feature. Removing the ecosystem guards widened the
                // sibling index to every ecosystem, so maven components now
                // find siblings too — and `maven-guice` has exactly that:
                // `com.google.inject/guice@5.1.0`, declared by
                // `examples/guice-demo/pom.xml`, sits beside a reachable
                // `guice@7.0.0` that the workspace resolved. Those three
                // components are issue #1235 (one coordinate emitted twice,
                // only the resolved one wired) and marking them would close
                // the I7 cell that reports a real reader defect while
                // leaving the duplicate in the document.
                //
                // So the arm additionally requires a POSITIVE signal: the
                // component must come from a lock file, named by
                // `LOCKFILE_NAMES` below. Stating it positively rather than
                // excluding manifests is deliberate. Measured, none of
                // issue #1234's thirteen `examples/celery/requirements.txt`
                // components happens to share a name with a reachable
                // component, so a sibling-only test excludes them today —
                // by luck, not by design, and a golden refresh could change
                // it. A lockfile allowlist excludes both #1234
                // (requirements.txt) and #1235 (pom.xml) structurally.
                let from_lockfile = LOCKFILE_NAMES
                    .iter()
                    .any(|n| sourced_from_file_named(c, n));
                if has_reachable_sibling && from_lockfile {
                    OrphanReasonCode::DeadLockfileEntry
                } else if c.sbom_tier.as_deref() == Some("design")
                    && c.evidence.source_file_paths
                        .iter()
                        .all(|p| p.trim().is_empty())
                    && workspace_members(c).is_empty()
                {
                    // T025 — mechanism E. A compiler, snapshot or toolchain
                    // pin: build tooling, not a dependency of the subject.
                    //
                    // All THREE conditions are required, not just the tier.
                    // `design` tier is not a synonym for "toolchain pin":
                    // `haskell-language-server` holds six unreachable
                    // `design`-tier components, of which only these two are
                    // pins. The other four are `docs/requirements.txt`
                    // documentation dependencies with source files and a
                    // `["docs"]` workspace member — they are caught by the
                    // FR-008 guard above as well, but the predicate must not
                    // depend on that for correctness.
                    OrphanReasonCode::BuildToolchain
                } else if MANIFEST_NAMES
                    .iter()
                    .any(|n| sourced_from_file_named(c, n))
                    && !ecosystems_with_reachable.contains(&ecosystem)
                {
                    // T018 — mechanism C. Declared by a build system the
                    // subject does not build with, co-located in the repo.
                    //
                    // Two conditions. The component comes from a build
                    // system's manifest, AND no component of its ecosystem
                    // is reachable from the subject — which is what "the
                    // subject does not build with it" means in evidence
                    // this pass can see, without needing the subject's own
                    // ecosystem threaded in.
                    //
                    // The second condition is what excludes the deferred
                    // populations: `python-flask`'s pypi has 108 components
                    // with many reachable, so issue #1234's thirteen cannot
                    // match; `maven-guice`'s maven has 46 with many
                    // reachable, so issue #1235's three cannot either. Both
                    // exclusions are structural rather than coincidental.
                    OrphanReasonCode::ForeignBuildSystem
                } else if NODE_GRAPH_LOCKFILE_NAMES
                    .iter()
                    .any(|n| sourced_from_file_named(c, n))
                {
                    // T019 — mechanism D. A node in a lock file whose nodes
                    // reference each other, which no other node references.
                    //
                    // The claim is deliberately narrow: nothing in a
                    // `flake.lock` points at this input. It does NOT claim
                    // the component is unused by the artifact, which a
                    // flake.lock alone cannot establish — the m925 reader
                    // emits inputs as components but wires edges only
                    // between lockfile nodes, so a dangling input is a
                    // statement about the lockfile, not about the build.
                    OrphanReasonCode::UnreferencedLockNode
                } else {
                    // Matching no mechanism, the component stays
                    // unexplained and keeps failing invariant I7 — FR-011,
                    // deliberately no catch-all. A code that explained
                    // everything would distinguish nothing and would make
                    // I7 unfalsifiable.
                    continue;
                }
            }
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
    target_kind: crate::generate::root_selector::ScanTargetKind,
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
        target_kind,
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
        orphan_reason_build_toolchain = counts.build_toolchain,
        orphan_reason_foreign_build_system = counts.foreign_build_system,
        orphan_reason_unreferenced_lock_node = counts.unreferenced_lock_node,
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

    // ---- feature 1218 ----

    /// Helper: a component that is unreachable, with the given source file.
    fn unreachable_from(purl: &str, source_file: &str) -> ResolvedComponent {
        let mut c = mk(purl);
        c.evidence.source_file_paths = vec![source_file.to_string()];
        c
    }

    /// Feature 1218 T014 — mechanism B: a pypi lockfile fork with a
    /// reachable same-name sibling classifies `dead-lockfile-entry`, the
    /// code m167 already defined for the npm multi-version case.
    #[test]
    fn m1218_us1_pypi_lockfile_fork_with_reachable_sibling() {
        let mut comps = vec![
            unreachable_from("pkg:pypi/alabaster@0.7.16", "uv.lock"),
            unreachable_from("pkg:pypi/alabaster@1.0.0", "uv.lock"),
        ];
        let reachable: HashSet<String> =
            ["pkg:pypi/alabaster@1.0.0".to_string()].into_iter().collect();

        let counts = classify_orphans(&mut comps, &reachable);

        assert_eq!(
            comps[0].extra_annotations.get(ANNOTATION_KEY).and_then(|v| v.as_str()),
            Some("dead-lockfile-entry"),
            "the unused fork says it is unused",
        );
        assert!(
            !comps[1].extra_annotations.contains_key(ANNOTATION_KEY),
            "the reachable copy carries nothing",
        );
        assert_eq!(counts.dead_lockfile_entry, 1);
    }

    /// Feature 1218 T015 — FR-004 at the unit level: the m167 codes keep
    /// their assignments under identical conditions. Verified here, before
    /// any golden exists to check it against.
    #[test]
    fn m1218_us1_m167_assignments_are_unchanged() {
        // npm multi-version -> dead-lockfile-entry (unchanged)
        let mut npm = vec![
            mk("pkg:npm/lodash@4.17.20"),
            mk("pkg:npm/lodash@4.17.21"),
        ];
        let reach: HashSet<String> =
            ["pkg:npm/lodash@4.17.21".to_string()].into_iter().collect();
        classify_orphans(&mut npm, &reach);
        assert_eq!(
            npm[0].extra_annotations.get(ANNOTATION_KEY).and_then(|v| v.as_str()),
            Some("dead-lockfile-entry"),
        );

        // npm no-sibling -> hoisted-unused (unchanged)
        let mut solo = vec![mk("pkg:npm/left-pad@1.3.0")];
        classify_orphans(&mut solo, &HashSet::new());
        assert_eq!(
            solo[0].extra_annotations.get(ANNOTATION_KEY).and_then(|v| v.as_str()),
            Some("hoisted-unused"),
        );

        // Go multi-version -> stale-go-sum-entry (unchanged)
        let mut go = vec![
            mk("pkg:golang/github.com/spf13/cobra@1.7.0"),
            mk("pkg:golang/github.com/spf13/cobra@1.8.0"),
        ];
        let greach: HashSet<String> =
            ["pkg:golang/github.com/spf13/cobra@1.8.0".to_string()]
                .into_iter()
                .collect();
        classify_orphans(&mut go, &greach);
        assert_eq!(
            go[0].extra_annotations.get(ANNOTATION_KEY).and_then(|v| v.as_str()),
            Some("stale-go-sum-entry"),
        );

        // Go no-sibling -> unresolved-indirect-require (unchanged)
        let mut gosolo = vec![mk("pkg:golang/example.com/x@1.0.0")];
        classify_orphans(&mut gosolo, &HashSet::new());
        assert_eq!(
            gosolo[0].extra_annotations.get(ANNOTATION_KEY).and_then(|v| v.as_str()),
            Some("unresolved-indirect-require"),
        );
    }

    /// Feature 1218 T016 — **the most important test in the feature.**
    ///
    /// A manifest-declared coordinate with a reachable same-name sibling
    /// must classify NOTHING. This is issue #1235's three `maven-guice`
    /// components in miniature: `com.google.inject/guice@5.1.0`, declared by
    /// `examples/guice-demo/pom.xml`, beside the reachable `guice@7.0.0`
    /// the workspace resolved.
    ///
    /// Removing the m167 ecosystem guards widened the sibling index to every
    /// ecosystem, so these satisfy "has a reachable sibling". Marking them
    /// would close the I7 cell that reports a real duplicate-coordinate
    /// defect, and the corpus count would read BETTER for it — 13 instead of
    /// 16 — which is why this is asserted rather than measured.
    #[test]
    fn m1218_us1_manifest_declared_coordinate_is_not_a_lockfile_fork() {
        let mut comps = vec![
            unreachable_from(
                "pkg:maven/com.google.inject/guice@5.1.0",
                "examples/guice-demo/pom.xml",
            ),
            unreachable_from(
                "pkg:maven/com.google.inject/guice@7.0.0",
                "pom.xml",
            ),
        ];
        let reachable: HashSet<String> =
            ["pkg:maven/com.google.inject/guice@7.0.0".to_string()]
                .into_iter()
                .collect();

        let counts = classify_orphans(&mut comps, &reachable);

        assert!(
            !comps[0].extra_annotations.contains_key(ANNOTATION_KEY),
            "a pom.xml coordinate is not a lockfile resolution fork; \
             marking it would silently close issue #1235's I7 cell",
        );
        assert_eq!(
            counts.dead_lockfile_entry, 0,
            "no lockfile-fork code may be tallied for a manifest coordinate",
        );
    }

    /// Feature 1218 T016 companion — the `requirements.txt` population of
    /// issue #1234 must also classify nothing, and must do so *structurally*
    /// rather than because none of them happens to have a reachable sibling.
    #[test]
    fn m1218_us1_requirements_txt_entry_is_not_a_lockfile_fork() {
        let mut comps = vec![
            unreachable_from(
                "pkg:pypi/amqp@5.1.1",
                "examples/celery/requirements.txt",
            ),
            unreachable_from("pkg:pypi/amqp@5.2.0", "uv.lock"),
        ];
        let reachable: HashSet<String> =
            ["pkg:pypi/amqp@5.2.0".to_string()].into_iter().collect();

        classify_orphans(&mut comps, &reachable);

        assert!(
            !comps[0].extra_annotations.contains_key(ANNOTATION_KEY),
            "a requirements.txt entry states what was asked for, not what \
             was resolved; issue #1234 must keep failing I7",
        );
    }

    /// Feature 1218 T018/T020 — mechanism C: a component from a foreign
    /// build system's manifest, whose ecosystem has no reachable component.
    /// Modelled on `maven-guice`'s two `pkg:bazel/*` WORKSPACE entries and
    /// `go-opentelemetry`'s root `requirements.txt` pypi entry.
    #[test]
    fn m1218_us2_foreign_build_system_manifest() {
        let mut comps = vec![
            unreachable_from("pkg:bazel/rules_jvm_external@unknown", "WORKSPACE"),
            unreachable_from("pkg:maven/org.example/used@1.0", "pom.xml"),
        ];
        // The maven component is reachable, so maven is "used" by the
        // subject and bazel is not.
        let reachable: HashSet<String> =
            ["pkg:maven/org.example/used@1.0".to_string()].into_iter().collect();

        let counts = classify_orphans(&mut comps, &reachable);

        assert_eq!(
            reason_of(&comps[0]),
            Some("foreign-build-system"),
            "a bazel WORKSPACE entry in a project whose maven components are \
             reachable is a build system the subject does not use",
        );
        assert_eq!(counts.foreign_build_system, 1);
    }

    /// Feature 1218 T019/T020 — mechanism D must be a DIFFERENT value from
    /// mechanism C. "A build system you do not use" and "a lockfile entry
    /// nothing references" call for different consumer actions, so one code
    /// for both would fail FR-001.
    #[test]
    fn m1218_us2_lock_node_and_foreign_build_system_differ() {
        let mut comps = vec![
            unreachable_from("pkg:github/nixos/nixpkgs@abc123", "flake.lock"),
            unreachable_from("pkg:bazel/rules_jvm_external@unknown", "WORKSPACE"),
        ];
        let counts = classify_orphans(&mut comps, &HashSet::new());

        assert_eq!(reason_of(&comps[0]), Some("unreferenced-lock-node"));
        assert_eq!(reason_of(&comps[1]), Some("foreign-build-system"));
        assert_ne!(
            reason_of(&comps[0]),
            reason_of(&comps[1]),
            "the two mechanisms must be distinguishable by value alone",
        );
        assert_eq!(counts.unreferenced_lock_node, 1);
        assert_eq!(counts.foreign_build_system, 1);
    }

    /// Feature 1218 T021 — a component of the SUBJECT'S OWN build system,
    /// declared by a sibling module, must classify nothing. Issue #1235's
    /// three `maven-guice` components satisfy both widened predicates —
    /// "has a reachable sibling" and "is co-located with another build
    /// system" — so this closes the second direction of that trap.
    #[test]
    fn m1218_us2_sibling_module_of_the_same_build_system_is_not_foreign() {
        let mut comps = vec![
            unreachable_from(
                "pkg:maven/com.google.inject/guice@5.1.0",
                "examples/guice-demo/pom.xml",
            ),
            unreachable_from("pkg:maven/com.google.inject/guice@7.0.0", "pom.xml"),
        ];
        let reachable: HashSet<String> =
            ["pkg:maven/com.google.inject/guice@7.0.0".to_string()]
                .into_iter()
                .collect();

        let counts = classify_orphans(&mut comps, &reachable);

        assert_eq!(
            reason_of(&comps[0]), None,
            "maven is reachable in this document, so a maven coordinate is \
             not a foreign build system; issue #1235 must keep failing I7",
        );
        assert_eq!(counts.foreign_build_system, 0);
        assert_eq!(counts.dead_lockfile_entry, 0);
    }

    /// Feature 1218 T026 — mechanism E: a toolchain pin. `design` tier, no
    /// source files, no workspace member. Modelled on
    /// `haskell-language-server`'s `ghc-9.6.5` and `stackage-lts-23.28`.
    #[test]
    fn m1218_us3_toolchain_pin() {
        let mut c = mk("pkg:generic/ghc-9.6.5@unspecified");
        c.sbom_tier = Some("design".to_string());
        c.evidence.source_file_paths = vec![String::new()];
        let mut comps = vec![c];

        let counts = classify_orphans(&mut comps, &HashSet::new());

        assert_eq!(reason_of(&comps[0]), Some("build-toolchain"));
        assert_eq!(counts.build_toolchain, 1);
    }

    /// Feature 1218 T027 — mechanism E must not fire on a `source`-tier
    /// component.
    #[test]
    fn m1218_us3_source_tier_is_not_a_toolchain_pin() {
        let mut c = mk("pkg:generic/something@1.0");
        c.sbom_tier = Some("source".to_string());
        c.evidence.source_file_paths = vec![String::new()];
        let mut comps = vec![c];

        classify_orphans(&mut comps, &HashSet::new());
        assert_eq!(reason_of(&comps[0]), None);
    }

    /// Feature 1218 T027a — **the invisible-failure test.**
    ///
    /// `haskell-language-server` holds six unreachable `design`-tier
    /// components. Only two are toolchain pins. The other four —
    /// `docutils`, `myst-parser`, `sphinx`, `sphinx-rtd-theme` from
    /// `docs/requirements.txt`, workspace member `["docs"]` — are Sphinx
    /// documentation dependencies, already explained by C151
    /// `waybill:unresolved-reason`.
    ///
    /// They are reachable by TWO of this feature's arms if either predicate
    /// is written loosely: mechanism E, because they are `design`-tier, and
    /// mechanism C, because `requirements.txt` is a manifest and pypi has no
    /// reachable component in that document. Both must decline.
    ///
    /// And because they are already explained they are NOT invariant-I7
    /// offenders — so if an arm claims them, the offender count stays at 16,
    /// I7 still passes on 15 of 17 targets, and every one of SC-001 through
    /// SC-004 is still green while the document asserts that Sphinx is
    /// build tooling. No count can catch this. Only this test and SC-018.
    #[test]
    fn m1218_us3_design_tier_docs_dependency_is_neither_toolchain_nor_foreign() {
        let mut comps = Vec::new();
        for name in ["docutils", "myst-parser", "sphinx", "sphinx-rtd-theme"] {
            let mut c = mk(&format!("pkg:pypi/{name}@1.0.0"));
            c.sbom_tier = Some("design".to_string());
            c.evidence.source_file_paths = vec!["docs/requirements.txt".to_string()];
            c.extra_annotations.insert(
                "waybill:workspace-member".to_string(),
                json!("[\"docs\"]"),
            );
            c.extra_annotations.insert(
                "waybill:unresolved-reason".to_string(),
                json!("no version specifier in requirements.txt"),
            );
            comps.push(c);
        }

        let counts = classify_orphans(&mut comps, &HashSet::new());

        for c in &comps {
            assert_eq!(
                reason_of(c), None,
                "a documentation dependency is neither build tooling nor a \
                 foreign build system; component {}",
                c.purl.as_str(),
            );
        }
        assert_eq!(
            counts, OrphanReasonCounts::default(),
            "no counter may move for an already-explained component (FR-008)",
        );
    }

    /// Feature 1218 T008 — `sourced_from_file_named` must be invariant
    /// under path shape.
    ///
    /// The paths on `source_file_paths` are raw: absolute, scan-root-
    /// relative, or `path+file://`-prefixed. The spellings that appear in an
    /// emitted SBOM are produced later by `emitted_source_files`. A
    /// predicate that matched the emitted spelling here would match nothing,
    /// which is how the m226 Pants Go reader's path-normalization defect
    /// presented.
    #[test]
    fn m1218_source_file_predicate_is_invariant_under_path_shape() {
        for raw in [
            "/Users/someone/src/flask/examples/celery/requirements.txt",
            "examples/celery/requirements.txt",
            "path+file:///Users/someone/src/flask/examples/celery/requirements.txt",
            "requirements.txt",
        ] {
            let mut c = mk("pkg:pypi/celery@5.2.7");
            c.evidence.source_file_paths = vec![raw.to_string()];
            assert!(
                sourced_from_file_named(&c, "requirements.txt"),
                "should match regardless of path shape; failed on {raw}",
            );
            assert!(
                !sourced_from_file_named(&c, "uv.lock"),
                "must not match a different file name; failed on {raw}",
            );
        }
    }

    /// Feature 1218 T008 — a substring match would wrongly accept a path
    /// whose *directory* is named like the target file.
    #[test]
    fn m1218_source_file_predicate_matches_the_file_not_the_path() {
        let mut c = mk("pkg:pypi/celery@5.2.7");
        c.evidence.source_file_paths =
            vec!["vendor/requirements.txt.d/pinned.in".to_string()];
        assert!(
            !sourced_from_file_named(&c, "requirements.txt"),
            "a directory named requirements.txt.d is not a requirements.txt",
        );
    }

    /// Feature 1218 T009 — `workspace_members` parses the JSON-array-encoded
    /// string shape `scan_fs/mod.rs` actually writes, and degrades to empty
    /// rather than failing the scan.
    #[test]
    fn m1218_workspace_members_parses_the_encoded_string_shape() {
        let mut c = mk("pkg:pypi/celery@5.2.7");
        assert!(workspace_members(&c).is_empty(), "absent annotation");

        c.extra_annotations.insert(
            "waybill:workspace-member".to_string(),
            json!("[\"examples/celery\"]"),
        );
        assert_eq!(workspace_members(&c), vec!["examples/celery".to_string()]);

        // A real JSON array rather than the encoded string: not the shape
        // the writer produces, so it must degrade, not panic.
        c.extra_annotations.insert(
            "waybill:workspace-member".to_string(),
            json!(["examples/celery"]),
        );
        assert!(
            workspace_members(&c).is_empty(),
            "unexpected shape degrades to empty",
        );

        c.extra_annotations.insert(
            "waybill:workspace-member".to_string(),
            json!("not json at all"),
        );
        assert!(workspace_members(&c).is_empty(), "unparseable degrades to empty");
    }


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
    // m167 T019, re-scoped by feature 1218.
    //
    // This test was written to assert m167's guarantee that non-Go/npm
    // ecosystems are never touched by the classifier. Feature 1218
    // deliberately removes that guarantee: the ecosystem guards are gone
    // and cargo, maven and pypi components are now classified.
    //
    // Its assertions still hold, for a different and better reason, so it
    // is re-scoped rather than deleted. Every fixture below is unreachable
    // and carries NO provenance — no source file, no tier, no workspace
    // member — so it matches none of the eight mechanisms and falls through
    // to `continue`. That is feature 1218's FR-011: there is no catch-all
    // code, and a component matching no mechanism stays unexplained and
    // keeps failing invariant I7. A regression that added a catch-all arm
    // would fail here first.
    //
    // The name is updated so the next reader is not told that non-Go/npm
    // is skipped, which has not been true since feature 1218.
    // ---------------------------------------------------------------

    #[test]
    fn m1218_no_provenance_matches_no_mechanism_and_stays_unexplained() {
        let mut components = vec![
            // Unreachable, and in ecosystems the classifier now DOES look
            // at — but with nothing to match on.
            mk("pkg:cargo/serde@1.0.0"),
            mk("pkg:maven/org.example/foo@1.0"),
            mk("pkg:pypi/requests@2.28.0"),
        ];
        let reachable_set = HashSet::new();

        let counts = classify_orphans(&mut components, &reachable_set);

        // ALL counters must be zero: nothing matched, and nothing was
        // invented to cover it.
        assert_eq!(counts, OrphanReasonCounts::default());
        for c in &components {
            assert_eq!(
                reason_of(c),
                None,
                "FR-011: no catch-all code — a component matching no \
                 mechanism stays unexplained",
            );
        }
    }
}
