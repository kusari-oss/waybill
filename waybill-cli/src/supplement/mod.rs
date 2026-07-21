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
    }
}

pub(crate) fn install(
    provenance: SupplementProvenance,
    services: Vec<SupplementService>,
) -> InstalledSupplementGuard {
    ACTIVE_PROVENANCE.with(|cell| {
        *cell.borrow_mut() = Some(provenance);
    });
    ACTIVE_SERVICES.with(|cell| {
        *cell.borrow_mut() = Some(services);
    });
    InstalledSupplementGuard
}

pub(crate) fn current_provenance() -> Option<SupplementProvenance> {
    ACTIVE_PROVENANCE.with(|cell| cell.borrow().clone())
}

pub(crate) fn current_services() -> Option<Vec<SupplementService>> {
    ACTIVE_SERVICES.with(|cell| cell.borrow().clone())
}
