//! FR-009 diagnostic-log aggregator. See `data-model.md`
//! §"WalkerMetrics" and `research.md` R9 for the log-line shape.

use std::collections::BTreeMap;

use super::ReaderId;

pub struct WalkerMetrics {
    passes: u32,
    files_visited: u64,
    dirs_visited: u64,
    per_reader_dispatch_counts: BTreeMap<ReaderId, u64>,
    started_at: std::time::Instant,
}

impl WalkerMetrics {
    /// Initialize with every registered reader mapped to 0 dispatches
    /// (contract requires all-readers-visible even when count is 0, so
    /// operators can spot pilot-set mis-sizing).
    pub fn new(registered_reader_ids: &[ReaderId]) -> Self {
        let mut per_reader_dispatch_counts = BTreeMap::new();
        for id in registered_reader_ids {
            per_reader_dispatch_counts.insert(*id, 0u64);
        }
        Self {
            passes: 1,
            files_visited: 0,
            dirs_visited: 0,
            per_reader_dispatch_counts,
            started_at: std::time::Instant::now(),
        }
    }

    pub fn tick_file(&mut self, dispatched_to: &[ReaderId]) {
        self.files_visited += 1;
        for id in dispatched_to {
            if let Some(v) = self.per_reader_dispatch_counts.get_mut(id) {
                *v += 1;
            }
        }
    }

    pub fn tick_dir(&mut self) {
        self.dirs_visited += 1;
    }

    /// Emit one `tracing::info!` line with the R9-specified shape:
    ///
    ///   passes=1 files_visited=N dirs_visited=M registered_readers=R
    ///   per_reader_dispatch_counts={"...": N, ...} wall_ms=X
    ///
    /// Field order is stable so operator scripts / regression tests can
    /// parse without JSON dependencies.
    pub fn emit(&self) {
        let wall_ms = self.started_at.elapsed().as_millis() as u64;
        let registered_readers = self.per_reader_dispatch_counts.len();
        let per_reader = format_per_reader_counts(&self.per_reader_dispatch_counts);
        tracing::info!(
            target: "waybill::scan_fs::walk_registry",
            "shared walker completed passes={} files_visited={} dirs_visited={} \
             registered_readers={} per_reader_dispatch_counts={} wall_ms={}",
            self.passes,
            self.files_visited,
            self.dirs_visited,
            registered_readers,
            per_reader,
            wall_ms,
        );
    }

    // Accessors for tests.
    #[cfg(test)]
    pub(crate) fn files_visited(&self) -> u64 {
        self.files_visited
    }

    #[cfg(test)]
    pub(crate) fn dirs_visited(&self) -> u64 {
        self.dirs_visited
    }

    #[cfg(test)]
    pub(crate) fn dispatch_count(&self, id: &ReaderId) -> u64 {
        self.per_reader_dispatch_counts.get(id).copied().unwrap_or(0)
    }
}

/// Format the per-reader dispatch counts as `{"id": N, ...}`.
/// BTreeMap iteration is sorted by key, so output is stable.
fn format_per_reader_counts(m: &BTreeMap<ReaderId, u64>) -> String {
    let mut out = String::from("{");
    let mut first = true;
    for (id, count) in m {
        if !first {
            out.push_str(", ");
        }
        first = false;
        // Manually format: `"id": count` — avoids serde_json dep pull-in
        // for this diagnostic-only path.
        out.push('"');
        out.push_str(id.as_str());
        out.push('"');
        out.push_str(": ");
        out.push_str(&count.to_string());
    }
    out.push('}');
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn new_initializes_all_readers_to_zero() {
        let ids = [ReaderId::new("r1"), ReaderId::new("r2"), ReaderId::new("r3")];
        let m = WalkerMetrics::new(&ids);
        for id in &ids {
            assert_eq!(m.dispatch_count(id), 0);
        }
        assert_eq!(m.files_visited(), 0);
        assert_eq!(m.dirs_visited(), 0);
    }

    #[test]
    fn tick_file_increments_files_and_per_reader() {
        let ids = [ReaderId::new("r1"), ReaderId::new("r2")];
        let mut m = WalkerMetrics::new(&ids);
        m.tick_file(&[ReaderId::new("r1")]);
        m.tick_file(&[ReaderId::new("r1"), ReaderId::new("r2")]);
        m.tick_file(&[]);
        assert_eq!(m.files_visited(), 3);
        assert_eq!(m.dispatch_count(&ReaderId::new("r1")), 2);
        assert_eq!(m.dispatch_count(&ReaderId::new("r2")), 1);
    }

    #[test]
    fn tick_dir_increments_dir_count() {
        let mut m = WalkerMetrics::new(&[]);
        m.tick_dir();
        m.tick_dir();
        assert_eq!(m.dirs_visited(), 2);
    }

    #[test]
    fn format_per_reader_counts_is_stable_across_iterations() {
        let mut m: BTreeMap<ReaderId, u64> = BTreeMap::new();
        m.insert(ReaderId::new("beta"), 3);
        m.insert(ReaderId::new("alpha"), 1);
        m.insert(ReaderId::new("gamma"), 0);
        // BTreeMap iterates by key; ReaderId derives Ord.
        let s = format_per_reader_counts(&m);
        assert_eq!(s, r#"{"alpha": 1, "beta": 3, "gamma": 0}"#);
    }
}
