// Milestone 119 (#326) — operator-supplied CDX 1.6 JSON supplement merge.
//
// `waybill sbom scan --supplement-cdx <PATH>` accepts a hand-authored
// CDX 1.6 (or 1.4 / 1.5) JSON document declaring ground truth the
// scanner cannot observe:
//
// - SaaS dependencies with no on-disk footprint (Stripe, Twilio, …)
// - Vendored libraries shipped without a recognizable manifest
// - License / supplier / copyright metadata on otherwise-known coords
// - Dependency edges into the above
//
// Pipeline position: a sibling input source to the filesystem scanner.
// At scan startup the supplement is parsed and structurally validated;
// after the scanner finishes, `supplement::merge::merge()` combines the
// two streams into a single `MergeOutcome` consumed by the CDX/SPDX
// builders.
//
// Conflict resolution (per spec FR-006 / FR-007 hard/soft split):
//
// - Scanner wins on bytes-derived facts: hashes[], cpe, canonical
//   purl, version, binary-role.
// - Developer wins on operator-domain metadata: licenses[],
//   concluded_licenses[], supplier, copyright, name (display),
//   description, externalReferences[] (all types).
// - Catch-all default: scanner wins (FR-015 safety property).
//
// Every disagreement is recorded as a `waybill:assertion-conflict`
// annotation on the merged component so consumers can audit. The
// scanner CANNOT be silenced — a supplement asserting "no openssl"
// against a fingerprint-detected openssl still emits the openssl
// component; the assertion appears as an annotated conflict.
//
// Three new waybill annotation keys (research.md § Decision 8):
//
// - **C65**: `waybill:source-tier = "declared"` (value extension on
//   the existing per-component key — supplement-only entries).
// - **C66**: `waybill:supplement-cdx = "<path>@sha256:<hex>"`
//   (document-scope provenance, emitted iff the flag is in effect).
// - **C67**: `waybill:assertion-conflict = <JSON-encoded array of
//   conflict records>` (per-component, repeatable conflicts stored
//   as a single JSON-array property).
//
// Constitution Principles preserved: I (pure Rust, zero new crates),
// III (parse/IO/schema failures fail closed before any walker begins),
// V (every new annotation has a Principle V audit row in
// docs/reference/sbom-format-mapping.md citing CDX/SPDX native-field
// gaps), X (operator-supplement provenance is visible to consumers),
// XII (operator-supplied supplements are an enrichment source per the
// External Data Source carve-out — they enrich the scanner's
// discovery, never substitute for it).

pub(crate) mod annotation;
pub(crate) mod conflict;
pub(crate) mod merge;
pub(crate) mod parser;

pub(crate) use merge::{merge, SupplementProvenance};
pub(crate) use parser::{load, SupplementService};

// Thread-local install pattern mirrors milestone-113's
// `scan_fs::package_db::exclude_path` annotation channel. The CLI
// boundary installs the active scan's supplement provenance + services
// list; emitters deep below (CDX metadata.rs, the new CDX
// services.rs, SPDX 2.3 / SPDX 3 packagers) read via the snapshot
// helpers. The RAII guards clear thread-local state at scan-exit so
// successive in-process scans (e.g., inside integration tests) don't
// leak state.

thread_local! {
    static ACTIVE_PROVENANCE: std::cell::RefCell<Option<SupplementProvenance>> =
        const { std::cell::RefCell::new(None) };
    static ACTIVE_SERVICES: std::cell::RefCell<Option<Vec<SupplementService>>> =
        const { std::cell::RefCell::new(None) };
    /// #1009 — PURLs the supplement's subject declared as direct
    /// dependencies. Each emitter anchors them to the root it selects.
    static ACTIVE_ROOT_ANCHORS: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
    /// Spec 1530 (#1515), research R5 — PURLs a *reader* needs anchored to
    /// the chosen root whatever other edges it has: top-level git
    /// submodules. Separate from the supplement's cell so either can be
    /// active without the other.
    static READER_ROOT_ANCHORS: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
}

/// Clears the reader root anchors on drop (spec 1530).
pub(crate) struct ReaderAnchorGuard;

impl Drop for ReaderAnchorGuard {
    fn drop(&mut self) {
        READER_ROOT_ANCHORS.with(|cell| {
            *cell.borrow_mut() = None;
        });
    }
}

/// Install the PURLs a reader needs anchored to the document root (spec
/// 1530, research R5). Hold the guard for as long as emission runs.
#[allow(dead_code)] // TEMP(1530): first caller is scan_cmd's submodule anchoring (T023)
pub(crate) fn install_reader_root_anchors(anchors: Vec<String>) -> ReaderAnchorGuard {
    READER_ROOT_ANCHORS.with(|cell| {
        *cell.borrow_mut() = Some(anchors);
    });
    ReaderAnchorGuard
}

pub(crate) struct InstalledSupplementGuard;

impl Drop for InstalledSupplementGuard {
    fn drop(&mut self) {
        ACTIVE_PROVENANCE.with(|cell| {
            *cell.borrow_mut() = None;
        });
        ACTIVE_SERVICES.with(|cell| {
            *cell.borrow_mut() = None;
        });
        ACTIVE_ROOT_ANCHORS.with(|cell| {
            *cell.borrow_mut() = None;
        });
    }
}

pub(crate) fn install(
    provenance: SupplementProvenance,
    services: Vec<SupplementService>,
    root_anchors: Vec<String>,
) -> InstalledSupplementGuard {
    ACTIVE_PROVENANCE.with(|cell| {
        *cell.borrow_mut() = Some(provenance);
    });
    ACTIVE_SERVICES.with(|cell| {
        *cell.borrow_mut() = Some(services);
    });
    ACTIVE_ROOT_ANCHORS.with(|cell| {
        *cell.borrow_mut() = Some(root_anchors);
    });
    InstalledSupplementGuard
}

pub(crate) fn current_provenance() -> Option<SupplementProvenance> {
    ACTIVE_PROVENANCE.with(|cell| cell.borrow().clone())
}

/// #1009 — PURLs the scan root must depend on, contributed by a
/// supplement whose dependency graph is rooted at its own subject.
/// Empty/None when no supplement is active, so the default path is
/// byte-identical.
///
/// Spec 1530: also the reader-contributed anchors (top-level git
/// submodules). The union is sorted and deduplicated; empty when neither a
/// supplement nor a reader installed any, so the default path stays
/// byte-identical.
pub(crate) fn current_root_anchors() -> Vec<String> {
    let mut all: Vec<String> =
        ACTIVE_ROOT_ANCHORS.with(|cell| cell.borrow().clone()).unwrap_or_default();
    all.extend(READER_ROOT_ANCHORS.with(|cell| cell.borrow().clone()).unwrap_or_default());
    all.sort();
    all.dedup();
    all
}

pub(crate) fn current_services() -> Option<Vec<SupplementService>> {
    ACTIVE_SERVICES.with(|cell| cell.borrow().clone())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod s1530_reader_anchor_tests {
    use super::*;

    #[test]
    fn empty_when_nothing_installed() {
        assert!(current_root_anchors().is_empty());
    }

    #[test]
    fn reader_anchors_alone_and_cleared_on_drop() {
        {
            let _g = install_reader_root_anchors(vec!["pkg:github/o/b@1".into(), "pkg:github/o/a@1".into()]);
            assert_eq!(current_root_anchors(), ["pkg:github/o/a@1", "pkg:github/o/b@1"]);
        }
        assert!(current_root_anchors().is_empty(), "the guard clears the cell");
    }

    #[test]
    fn union_with_supplement_anchors_is_sorted_and_deduplicated() {
        let _r = install_reader_root_anchors(vec!["pkg:generic/b@1".into(), "pkg:generic/a@1".into()]);
        ACTIVE_ROOT_ANCHORS.with(|cell| *cell.borrow_mut() = Some(vec!["pkg:generic/a@1".into(), "pkg:generic/c@1".into()]));
        assert_eq!(current_root_anchors(), ["pkg:generic/a@1", "pkg:generic/b@1", "pkg:generic/c@1"]);
        ACTIVE_ROOT_ANCHORS.with(|cell| *cell.borrow_mut() = None);
    }
}
