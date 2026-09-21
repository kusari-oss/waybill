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

fn assemble(
    root: &Path,
    census: &census::Census,
    entries: &[crate::scan_fs::package_db::PackageDbEntry],
    redaction: RedactionMode,
) -> ObservationReport {
    let threshold = DEFAULT_SIGNIFICANCE_THRESHOLD;
    let recorded = significance::partition(census, root, threshold);
    let per_dir_components = components_by_dir(entries);

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
            DirectoryObservation {
                path: rel(root, &r.path, redaction),
                claim_status,
                claimed_by,
                ecosystems: Vec::new(),
                ambiguity: None,
                files_direct: r.files_direct,
                files_aggregated: r.files_aggregated,
                components_emitted: *per_dir_components.get(&r.path).unwrap_or(&0),
                observation: None,
            }
        })
        .chain(excluded)
        .collect();

    // FR-004 — every reader carries BOTH counts. A reader absent from the
    // census matched nothing; one present with no components engaged and
    // produced nothing. Different diagnoses, so both must be expressible.
    let emitted_per_reader: BTreeMap<String, u64> = census::take_emitted()
        .into_iter()
        .map(|(id, n)| (id.as_str().to_string(), n))
        .collect();
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
            components_emitted: *emitted_per_reader.get(&id).unwrap_or(&0),
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
