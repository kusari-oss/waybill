//! Repo observation report — milestone 924 / issue #932.
//!
//! A versioned, machine-readable account of what waybill saw, claimed,
//! ignored, and **could not determine** while traversing a repository.
//!
//! # The organising principle
//!
//! **This module records observations and typed uncertainty, not
//! conclusions.** "I could not determine what this is" is a valid answer when
//! it carries what *was* observable: a directory of 47 binary files and a
//! directory of 47 UTF-8 text files are both unclassified and mean entirely
//! different things.
//!
//! That is a deliberate departure from how the rest of waybill behaves.
//! Constitution Principle IX requires the SBOM to assert nothing it cannot
//! support; here the uncertainty *is* the payload (spec FR-014).
//!
//! # Why this is nearly free
//!
//! `walk_registry::dispatch::dispatch_file` already returns the set of readers
//! that claimed each file, and the walker already binds that value one line
//! before handing it to a metrics sink. The census is that value, retained —
//! not a second traversal (research R1).
//!
//! # What this module must never do
//!
//! - Change emitted SBOM content in any format (FR-024).
//! - Issue a network request (FR-022).
//! - Transmit a report anywhere (FR-023).
//! - Resolve genuine ambiguity by preference (FR-014).

pub(crate) mod census;
pub(crate) mod content_kind;
pub(crate) mod ecosystems;
pub(crate) mod schema;
pub(crate) mod significance;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use schema::{
    ClaimStatus, DirectoryObservation, ObservationReport, ReaderCoverage, RedactionMode,
    RepositoryTotals, SCHEMA_VERSION,
};
use significance::{Significance, DEFAULT_SIGNIFICANCE_THRESHOLD};

/// Build the report for `root`.
///
/// FR-022a: runs the traversal and the readers. Enrichment and SBOM emission
/// are never invoked — not suppressed by a flag, simply not on this path.
pub(crate) fn build(
    root: &Path,
    exclude_set: &crate::scan_fs::package_db::exclude_path::ExclusionSet,
    redaction: RedactionMode,
) -> anyhow::Result<ObservationReport> {
    census::enable_report_mode();

    // FR-022 / FR-022b — force the Go transitive resolver offline.
    //
    // Measured, not assumed: with a Go module present and no offline signal,
    // the resolver reads $GOPROXY and is prepared to fetch
    // (`graph_resolver.rs` proxy tier). It avoided the network in testing only
    // because the module happened to be in the local module cache, which made
    // an earlier SC-007 test pass for the wrong reason.
    //
    // `read_all` has no offline parameter, so this uses the resolver's own
    // documented gate at `graph_resolver.rs:1147`. Set here rather than
    // exposed as a flag: no operator input can turn it off, which is the
    // sense in which FR-022b's guarantee is structural. Transitive edges are
    // not needed by any requirement in this feature -- components come from
    // readers -- so nothing is lost.
    //
    // Safe in-process because the report command performs one scan and exits;
    // tests drive the binary as a subprocess.
    std::env::set_var("WAYBILL_OFFLINE", "1");

    let scan = crate::scan_fs::package_db::read_all(
        root,
        None,
        /* include_dev */ true,
        /* include_legacy_rpmdb */ false,
        crate::scan_fs::ScanMode::Path,
        /* include_declared_deps */ false,
        None,
        exclude_set,
        None,
    )
    .map_err(|e| anyhow::anyhow!("reader phase failed: {e}"))?;

    let census = census::take_collected().ok_or_else(|| {
        anyhow::anyhow!("no census was collected — the walker did not run in report mode")
    })?;

    Ok(assemble(root, &census, &scan.entries, redaction))
}

/// Attribute each emitted component to the directory its source path sits in.
fn components_by_dir(entries: &[crate::scan_fs::package_db::PackageDbEntry]) -> BTreeMap<PathBuf, u64> {
    let mut m: BTreeMap<PathBuf, u64> = BTreeMap::new();
    for e in entries {
        let p = Path::new(&e.source_path);
        let dir = if p.is_dir() { p.to_path_buf() } else { p.parent().unwrap_or(p).to_path_buf() };
        *m.entry(dir).or_insert(0) += 1;
    }
    m
}

/// The nearest enclosing project root, or `None` when nothing encloses it.
///
/// Strictly an ancestor search: a project root is not covered by itself.
fn nearest_project_root(dir: &Path, roots: &BTreeSet<PathBuf>) -> Option<PathBuf> {
    let mut cur = dir.parent();
    while let Some(p) = cur {
        if roots.contains(p) {
            return Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    None
}

/// Render a path repository-relative. **Never absolute, in any mode**
/// (FR-019) — absolute paths leak home directories and usernames and buy
/// nothing.
fn rel(root: &Path, p: &Path, redaction: RedactionMode) -> String {
    let r = p.strip_prefix(root).unwrap_or(p);
    let s = if r.as_os_str().is_empty() { ".".to_string() } else { r.to_string_lossy().into_owned() };
    match redaction {
        RedactionMode::None => s,
        // FR-019b — identical segments map identically within a report, so
        // nesting depth and repetition survive while names do not.
        RedactionMode::Paths => s
            .split('/')
            .map(|seg| if seg == "." { seg.to_string() } else { stable_id(seg) })
            .collect::<Vec<_>>()
            .join("/"),
    }
}

fn stable_id(segment: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(segment.as_bytes());
    format!("{:x}", h.finalize())[..12].to_string()
}

/// Ecosystems whose markers appear anywhere beneath `dir`, from the full
/// census rather than only the recorded directories.
///
/// Subtree-scoped on purpose. A fixtures tree holds one ecosystem per
/// subdirectory, not several in one, so a directory-local check would see
/// nothing ambiguous about `waybill-cli/tests/` — the case SC-001 exists for.
fn subtree_ecosystems(census: &census::Census, dir: &Path) -> BTreeMap<String, String> {
    let table = ecosystems::unsupported_markers();
    let mut found = BTreeMap::new();
    for (d, c) in census.dirs() {
        if !d.starts_with(dir) {
            continue;
        }
        for name in &c.filenames {
            if let Some(eco) = table.get(name.as_str()) {
                found.insert(eco.clone(), name.clone());
            } else if significance::is_marker(name) {
                found.insert(ecosystems::ecosystem_for_marker(name).to_string(), name.clone());
            }
        }
    }
    found
}

/// FR-012b / FR-013 — ambiguity, independent of claim status.
///
/// A directory whose subtree spans several ecosystems and which is not itself
/// a single project root cannot be classified from observation: it may be a
/// polyglot project, a fixtures tree, or vendored examples. The report records
/// the competing readings and the evidence, and ranks nothing (FR-014).
fn detect_ambiguity(
    census: &census::Census,
    dir: &Path,
    own_filenames: &BTreeSet<String>,
) -> Option<schema::AmbiguityRecord> {
    // A directory with its own marker is a project root; its subtree spanning
    // several ecosystems is ordinary, not ambiguous.
    if own_filenames.iter().any(|f| significance::is_marker(f)) {
        return None;
    }
    let found = subtree_ecosystems(census, dir);
    if found.len() < 2 {
        return None;
    }
    let mut evidence: Vec<String> = found
        .iter()
        .map(|(eco, marker)| format!("{marker} ({eco})"))
        .collect();
    evidence.sort();
    Some(schema::AmbiguityRecord {
        kind: "multiple_ecosystem_lockfiles".to_string(),
        interpretations: vec![
            "a polyglot project whose subprojects are separate ecosystems".to_string(),
            "test fixtures or vendored examples that are not this project's dependencies"
                .to_string(),
            "generated or build output containing copies of manifests".to_string(),
        ],
        evidence,
    })
}

/// Deepest nesting below `dir`, in directory levels.
fn subtree_depth(census: &census::Census, dir: &Path) -> u32 {
    census
        .dirs()
        .keys()
        .filter(|d| d.starts_with(dir))
        .map(|d| d.strip_prefix(dir).map(|r| r.components().count()).unwrap_or(0) as u32)
        .max()
        .unwrap_or(0)
}

fn assemble(
    root: &Path,
    census: &census::Census,
    entries: &[crate::scan_fs::package_db::PackageDbEntry],
    redaction: RedactionMode,
) -> ObservationReport {
    let threshold = DEFAULT_SIGNIFICANCE_THRESHOLD;
    // Ambiguity is computed BEFORE significance, because carrying an
    // ambiguity is itself a reason to be recorded (see Significance::Ambiguous).
    let ambiguities: BTreeMap<PathBuf, schema::AmbiguityRecord> = census
        .dirs()
        .iter()
        .filter_map(|(d, c)| detect_ambiguity(census, d, &c.filenames).map(|a| (d.clone(), a)))
        .collect();
    let recorded = significance::partition(census, root, threshold, &|p| {
        ambiguities.contains_key(p)
    });
    let per_dir_components = components_by_dir(entries);

    // Project roots: recorded directories that declare a marker of their own.
    // A directory with no marker inherits coverage from the nearest of these
    // above it — the nearest-enclosing-marker rule that Go, Cargo, npm, Maven
    // and Python all follow.
    let project_roots: BTreeSet<PathBuf> = recorded
        .iter()
        .filter(|r| r.census.filenames.iter().any(|f| significance::is_marker(f)))
        .map(|r| r.path.clone())
        .collect();

    // FR-005 — excluded directories are always recorded, regardless of size
    // or markers. An operator who excluded something needs to see that the
    // report knows it was excluded, not merely that it is absent.
    let excluded: Vec<DirectoryObservation> = census
        .excluded_dirs()
        .iter()
        .map(|(dir, files)| DirectoryObservation {
            path: rel(root, dir, redaction),
            claim_status: ClaimStatus::ExcludedByPolicy,
            claimed_by: Vec::new(),
            ecosystems: Vec::new(),
            ambiguity: None,
            covered_by: nearest_project_root(dir, &project_roots)
                .map(|p| rel(root, &p, redaction)),
            files_direct: *files,
            files_aggregated: 0,
            components_emitted: 0,
            observation: None,
        })
        .collect();

    let directories: Vec<DirectoryObservation> = recorded
        .iter()
        .map(|r| {
            let claim_status = if r.significance == Significance::Claimed
                || !r.census.claimed_by.is_empty()
            {
                ClaimStatus::Claimed
            } else {
                ClaimStatus::Unclaimed
            };
            let claimed_by: Vec<String> =
                r.census.claimed_by.iter().map(|id| id.as_str().to_string()).collect();
            let attributions = ecosystems::attribute(
                &r.census.filenames,
                claim_status == ClaimStatus::Claimed,
            );
            let ambiguity = ambiguities.get(&r.path).cloned();
            // FR-011a — "confidently classified" means at least one ecosystem
            // attribution AND no ambiguity record. Anything else earns the
            // observation detail, because a reader needs something to reason
            // with where the report declines to conclude.
            let confident = !attributions.is_empty() && ambiguity.is_none();
            let observation = (!confident).then(|| schema::DirectoryObservationDetail {
                file_count: r.files_direct + r.files_aggregated,
                max_depth: subtree_depth(census, &r.path),
                extension_histogram: content_kind::extension_histogram(&r.census.filenames),
                content_kind: content_kind::classify_dir(&r.path, &r.census.filenames).as_str()
                    .to_string(),
                content_sample_bytes: content_kind::SAMPLE_BYTES,
            });
            DirectoryObservation {
                path: rel(root, &r.path, redaction),
                claim_status,
                ecosystems: attributions,
                claimed_by,
                ambiguity,
                covered_by: nearest_project_root(&r.path, &project_roots)
                    .map(|p| rel(root, &p, redaction)),
                files_direct: r.files_direct,
                files_aggregated: r.files_aggregated,
                components_emitted: *per_dir_components.get(&r.path).unwrap_or(&0),
                observation,
            }
        })
        .chain(excluded)
        .collect();

    // FR-004 — every reader carries BOTH counts. A reader absent from the
    // census matched nothing; one present with no components engaged and
    // produced nothing. Different diagnoses, so both must be expressible.
    // FR-004 — attribute components to readers via sole directory claim.
    //
    // The walker's own per-reader output map covers only readers that emit
    // *during* the walk; most emit in a later phase, so that map reported 0
    // for 27 of 28 readers while the SBOM held 5,552 components. Zero there
    // is indistinguishable from "not tracked", and emitting it as 0 asserted
    // something false.
    //
    // A component is credited to a reader when that reader is the SOLE
    // claimant of the directory its source path sits in. Ambiguous
    // directories credit nobody, and a reader with nothing attributable gets
    // `None` rather than `0`.
    let _walker_emitted = census::take_emitted();
    let mut attributed: BTreeMap<String, u64> = BTreeMap::new();
    let mut attributable_readers: BTreeSet<String> = BTreeSet::new();
    for (dir, n) in &per_dir_components {
        if let Some(d) = census.dirs().get(dir) {
            if d.claimed_by.len() == 1 {
                if let Some(id) = d.claimed_by.iter().next() {
                    *attributed.entry(id.as_str().to_string()).or_insert(0) += n;
                    attributable_readers.insert(id.as_str().to_string());
                }
            }
        }
    }
    // A reader that solely claimed at least one directory has a determined
    // count, even when that count is zero -- that is a real
    // "matched but produced nothing" signal. A reader that never solely
    // claimed anything has no determination to report.
    for d in census.dirs().values() {
        if d.claimed_by.len() == 1 {
            if let Some(id) = d.claimed_by.iter().next() {
                attributable_readers.insert(id.as_str().to_string());
            }
        }
    }
    let emitted_per_reader = &attributed;
    let mut reader_ids: BTreeSet<String> =
        census.per_reader_files().keys().map(|id| id.as_str().to_string()).collect();
    reader_ids.extend(emitted_per_reader.keys().cloned());
    let readers: Vec<ReaderCoverage> = reader_ids
        .into_iter()
        .map(|id| ReaderCoverage {
            files_matched: census
                .per_reader_files()
                .iter()
                .find(|(k, _)| k.as_str() == id)
                .map(|(_, v)| *v)
                .unwrap_or(0),
            components_emitted: attributable_readers
                .contains(&id)
                .then(|| *emitted_per_reader.get(&id).unwrap_or(&0)),
            reader_id: id,
        })
        .collect();

    let files_skipped: BTreeMap<String, u64> = census
        .skipped()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), *v))
        .collect();

    ObservationReport {
        schema_version: SCHEMA_VERSION,
        schema_stability: "alpha",
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        volatile_fields: schema::volatile_fields(),
        redaction_mode: redaction,
        significance_threshold: threshold,
        totals: RepositoryTotals {
            directories_walked: census.dirs().len() as u64,
            directories_recorded: directories.len() as u64,
            files_walked: census.files_walked(),
            files_claimed: census.files_claimed(),
            files_unclaimed: census.files_unclaimed(),
            files_skipped,
        },
        readers,
        directories,
    }
}

/// FR-003 / contract C-4 — the reconciliation invariant, defined once.
///
/// The emitter refuses to write a report that fails this, and a consumer can
/// check the same property without access to the repository being described.
/// Stated in one place so the producer's check and the contract cannot drift
/// apart.
pub(crate) fn totals_reconcile(t: &RepositoryTotals) -> bool {
    let skipped: u64 = t.files_skipped.values().sum();
    t.files_walked == t.files_claimed + t.files_unclaimed + skipped
}
