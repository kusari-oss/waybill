//! Per-file dispatch loop + panic isolation. Private module (no public
//! API surface). Invoked by `SharedWalker::run()` (walker.rs) once per
//! matched file/dir.
//!
//! Contract C1 — dispatch order is REGISTRATION ORDER. `dispatch_file`
//! and `dispatch_dir` both iterate `registrations` in slice order.
//!
//! Contract C4 — every callback is wrapped in `catch_unwind` +
//! `AssertUnwindSafe`. A panic in one reader's callback is logged via
//! `tracing::warn!` and the dispatch loop continues to remaining readers.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use std::collections::HashSet;

use super::{ReaderId, ReaderRegistration, SharedWalkerContext};

/// Dispatch a file-visit event to every reader whose registered patterns
/// match the file's basename. Returns the list of reader IDs actually
/// dispatched (used by `WalkerMetrics::tick_file`).
///
/// Contract C10 (m664 post-2026-08-23 API extension): when `scope` is
/// `Some(set)`, dispatch is restricted to readers whose `reader_id` is
/// in `set`. This scoping engages when the walker descended into a
/// normally-skipped directory via one or more readers' `descend_into`
/// override — non-requesting readers must NOT see files under that
/// subtree (byte-identity preservation for the 21 already-migrated
/// readers). `None` = all registrations active (root + normal descent).
pub(super) fn dispatch_file(
    path: &Path,
    registrations: &[ReaderRegistration],
    ctx: &SharedWalkerContext<'_>,
    scope: Option<&HashSet<ReaderId>>,
) -> Vec<ReaderId> {
    let Some(basename) = path.file_name() else {
        return Vec::new();
    };
    let mut dispatched_to = Vec::new();
    for reg in registrations {
        // C1: iterate in registration order. Guarantees dispatch determinism.
        // C10: honor the descend-into scope restriction.
        if let Some(set) = scope {
            if !set.contains(&reg.reader_id) {
                continue;
            }
        }
        let Some(on_file) = reg.on_file else { continue };
        if !reg.patterns.is_match(basename) {
            continue;
        }
        dispatched_to.push(reg.reader_id);
        // C4: isolate the reader's callback so a panic in one reader
        // does not corrupt scan output for other readers or crash the
        // whole scan.
        let reader_id = reg.reader_id;
        let path_ref = path;
        let ctx_ref = ctx;
        let result = catch_unwind(AssertUnwindSafe(|| {
            on_file(path_ref, ctx_ref);
        }));
        if let Err(payload) = result {
            let payload_str = describe_panic(&payload);
            tracing::warn!(
                target: "waybill::scan_fs::walk_registry",
                reader = reader_id.as_str(),
                path = %path.display(),
                payload = %payload_str,
                "reader callback panicked during file dispatch; continuing with remaining readers",
            );
        }
    }
    dispatched_to
}

/// Dispatch a directory-completion event to every reader that registered
/// an `on_dir` callback. Same C1 + C4 + C10 semantics as `dispatch_file`.
pub(super) fn dispatch_dir(
    dir: &Path,
    registrations: &[ReaderRegistration],
    ctx: &SharedWalkerContext<'_>,
    scope: Option<&HashSet<ReaderId>>,
) -> Vec<ReaderId> {
    let mut dispatched_to = Vec::new();
    for reg in registrations {
        // C10: honor the descend-into scope restriction.
        if let Some(set) = scope {
            if !set.contains(&reg.reader_id) {
                continue;
            }
        }
        let Some(on_dir) = reg.on_dir else { continue };
        dispatched_to.push(reg.reader_id);
        let reader_id = reg.reader_id;
        let dir_ref = dir;
        let ctx_ref = ctx;
        let result = catch_unwind(AssertUnwindSafe(|| {
            on_dir(dir_ref, ctx_ref);
        }));
        if let Err(payload) = result {
            let payload_str = describe_panic(&payload);
            tracing::warn!(
                target: "waybill::scan_fs::walk_registry",
                reader = reader_id.as_str(),
                dir = %dir.display(),
                payload = %payload_str,
                "reader callback panicked during dir dispatch; continuing with remaining readers",
            );
        }
    }
    dispatched_to
}

fn describe_panic(payload: &Box<dyn std::any::Any + Send + 'static>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use std::fs;
    use std::sync::Mutex;

    use crate::scan_fs::package_db::exclude_path::ExclusionSet;
    use crate::scan_fs::walk_registry::{
        globset_from_patterns, ReaderId, ReaderRegistration, ReaderRegistryBuilder, SharedWalker,
        SharedWalkerContext,
    };

    /// Per-test dispatch-order log. Written to by the test's two
    /// callbacks; asserted at end of test. Only used by
    /// `registry_dispatches_in_registration_order` — no other test
    /// touches this static.
    static ORDER_LOG: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

    fn cb_reader_alpha(_path: &std::path::Path, _ctx: &SharedWalkerContext<'_>) {
        ORDER_LOG.lock().unwrap().push("alpha");
    }

    fn cb_reader_bravo(_path: &std::path::Path, _ctx: &SharedWalkerContext<'_>) {
        ORDER_LOG.lock().unwrap().push("bravo");
    }

    /// T011 — contract C1: dispatch order matches registration order.
    /// Two readers with an overlapping pattern both match the same file;
    /// the reader registered FIRST is dispatched first.
    #[test]
    fn registry_dispatches_in_registration_order() {
        // Reset log for this test invocation. Serialize via the Mutex
        // itself — hold the lock long enough that no concurrent test
        // can interleave (there's only one such test in this module,
        // so this is defensive).
        ORDER_LOG.lock().unwrap().clear();

        let tmpdir = tempfile::tempdir().unwrap();
        let overlap_path = tmpdir.path().join("overlap.marker");
        fs::write(&overlap_path, b"payload").unwrap();

        let registry = ReaderRegistryBuilder::new()
            .register(ReaderRegistration {
                reader_id: ReaderId::new("alpha"),
                state: None,
                patterns: globset_from_patterns(&["**/overlap.marker"]).unwrap(),
                on_file: Some(cb_reader_alpha),
                on_dir: None,
                descend_into: None,
            })
            .register(ReaderRegistration {
                reader_id: ReaderId::new("bravo"),
                state: None,
                patterns: globset_from_patterns(&["**/overlap.marker"]).unwrap(),
                on_file: Some(cb_reader_bravo),
                on_dir: None,
                descend_into: None,
            })
            .build()
            .unwrap();

        let excludes = ExclusionSet::new_empty();
        let mut walker = SharedWalker::new(tmpdir.path(), &registry, &excludes);
        walker.run();
        let _ = walker.finish();

        let log = ORDER_LOG.lock().unwrap();
        assert_eq!(
            *log,
            vec!["alpha", "bravo"],
            "dispatch order must match registration order (contract C1); got {:?}",
            *log,
        );
    }
}
