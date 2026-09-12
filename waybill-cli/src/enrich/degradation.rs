//! Milestone 839 (#766) — what went wrong during enrichment, if
//! anything.
//!
//! Constitution Principles XI and XII.3 both require a transparency
//! annotation when an enrichment source degrades. Without one, a
//! degraded run and a clean run produce SBOMs that differ only in
//! component counts — and nobody checks a component count against an
//! expected number, so silent under-enrichment looks exactly like a
//! package having no licence upstream.
//!
//! The record is document-scope: it says one thing about the run, not
//! one thing about each of potentially thousands of components.

use std::collections::BTreeSet;

/// A way the enrichment phase can fall short of what was asked of it.
///
/// Closed enum rather than free text: these values are emitted into
/// SBOMs that downstream tooling matches on, so the vocabulary has to
/// be stable and enumerable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DegradationMode {
    // `BatchUnavailable` and `Throttled` are part of this closed
    // vocabulary (catalog row C158) but are only *produced* once the
    // batch path exists — T026/T028. They are added there rather than
    // carried here unconstructed, because the workspace denies dead
    // code and a standing `#[allow]` outlives the reason for it.
    /// Enrichment could not run at all — the service was unreachable
    /// for the whole phase.
    WhollyUnavailable,
}

impl DegradationMode {
    /// Stable wire string. Changing one of these is a breaking change
    /// for any consumer matching on it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WhollyUnavailable => "wholly-unavailable",
        }
    }
}

/// Everything that degraded during one enrichment phase.
///
/// `BTreeSet` so the emitted order is deterministic and a mode
/// recorded twice appears once — the annotation answers "which modes
/// occurred", not "how many times each".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DegradationRecord {
    modes: BTreeSet<DegradationMode>,
    unenriched: usize,
}

impl DegradationRecord {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a mode. Idempotent.
    pub fn record(&mut self, mode: DegradationMode) {
        self.modes.insert(mode);
    }

    /// Add to the count of components left unenriched by degradation.
    ///
    /// Deliberately separate from [`record`]: a run can degrade in a
    /// way that costs speed but not coverage (`BatchUnavailable`
    /// falling back successfully), and reporting those as unenriched
    /// components would overstate the harm.
    pub fn add_unenriched(&mut self, n: usize) {
        self.unenriched = self.unenriched.saturating_add(n);
    }

    /// True when nothing degraded. A clean run emits no annotation at
    /// all rather than an explicit "no degradation" marker, which
    /// would be noise on the overwhelming majority of scans.
    pub fn is_clean(&self) -> bool {
        self.modes.is_empty()
    }



    /// The annotation value: every mode that occurred, comma-joined,
    /// plus the unenriched count.
    ///
    /// Every mode is listed, not just the first — a run that was
    /// throttled *and* lost the batch endpoint is not adequately
    /// described by either alone (FR-017c).
    pub fn annotation_value(&self) -> Option<String> {
        if self.is_clean() {
            return None;
        }
        let modes: Vec<&str> = self.modes.iter().map(|m| m.as_str()).collect();
        Some(format!(
            "{};unenriched={}",
            modes.join(","),
            self.unenriched
        ))
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn clean_run_emits_nothing() {
        let r = DegradationRecord::new();
        assert!(r.is_clean());
        assert_eq!(r.annotation_value(), None);
    }

    #[test]
    fn a_mode_is_reported_with_its_count() {
        let mut r = DegradationRecord::new();
        r.record(DegradationMode::WhollyUnavailable);
        r.add_unenriched(12);
        assert_eq!(
            r.annotation_value().unwrap(),
            "wholly-unavailable;unenriched=12",
        );
    }





    #[test]
    fn wire_strings_are_stable() {
        // These are matched on by downstream tooling; changing one is
        // a breaking change, so pin them.
        assert_eq!(DegradationMode::WhollyUnavailable.as_str(), "wholly-unavailable");
    }
}
