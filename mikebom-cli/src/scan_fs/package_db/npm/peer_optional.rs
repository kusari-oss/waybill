//! Milestone 180 T003 — shared reader-time peer-precedence guard for
//! JavaScript-ecosystem optional-dependency classification.
//!
//! **The FR-006 contract** (from `specs/180-npm-optional-dep-reader/
//! contracts/peer-precedence-guard.md`): when a dep is declared BOTH as
//! `peerDependencies.<name>` AND as `peerDependenciesMeta.<name>.optional
//! = true`, the m178 peer classification MUST win over m180's optional
//! classification. This means:
//!
//! - `LifecycleScope::Optional` MUST NOT be set on the target.
//! - `mikebom:optional-derivation` annotation MUST NOT be emitted on the
//!   target.
//! - m178's `PROVIDED_DEPENDENCY_OF` emission (via the m147 peer-edge-
//!   targets state on the SOURCE component) continues to fire.
//!
//! This helper is consumed by every m180 reader touch: US1 npm, US2
//! pnpm, US3 yarn, and (contingent) US5 bun. Centralizing the predicate
//! here avoids per-reader drift.

use serde_json::Value;

/// Return true iff the named entry is BOTH a peer-declared dep AND
/// flagged as optional-peer via `peerDependenciesMeta.<name>.optional
/// = true` in the given parent package.json. When true, callers MUST
/// short-circuit the m180 optional classification — the peer semantic
/// wins.
///
/// The predicate operates on ANY `Value` that has the shape of a
/// package.json (parsed JSON object) — no schema enforcement beyond the
/// two field-existence checks. Missing / null / non-object fields all
/// safely return false.
///
/// Milestone 180 T003. Contract: see
/// `specs/180-npm-optional-dep-reader/contracts/peer-precedence-guard.md`.
///
/// **Reader usage note** (m180 + m181):
///
/// - **npm** (`package_lock.rs`) and **pnpm** (`pnpm_lock.rs`) do NOT
///   call this helper. They short-circuit the peer-optional case using
///   the per-entry `peer: true` lockfile flag — npm/pnpm set that flag
///   on entries installed to satisfy a `peerDependencies` declaration,
///   so their lockfiles already reflect the parent's manifest.
///
/// - **yarn v1 + Berry** (`yarn_lock.rs`) DO call this helper. Yarn's
///   Plug'n'Play resolver moves peer metadata into `.pnp.cjs` or into
///   the source package.json declaration — `yarn.lock` does NOT carry
///   `peer: true` on lockfile entries the way npm/pnpm do. So yarn's
///   parsers cross-reference the root `package.json` via this helper
///   to identify peer-optional deps at reader time.
///
/// Milestone 180 introduced the helper; milestone 181 wired yarn to
/// consume it (removing the earlier `#[allow(dead_code)]` marker).
pub(crate) fn is_peer_optional(entry_name: &str, parent_pkg_json: &Value) -> bool {
    let has_peer_dep = parent_pkg_json
        .get("peerDependencies")
        .and_then(|v| v.as_object())
        .map(|obj| obj.contains_key(entry_name))
        .unwrap_or(false);
    let is_optional_peer = parent_pkg_json
        .get("peerDependenciesMeta")
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get(entry_name))
        .and_then(|meta| meta.get("optional"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    has_peer_dep && is_optional_peer
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn is_peer_optional_true_when_both_present() {
        // The canonical case: react declared as both peer AND peer-
        // optional. Guard fires → m180 optional classification MUST
        // be short-circuited.
        let pkg_json = json!({
            "peerDependencies": {"react": "^18"},
            "peerDependenciesMeta": {"react": {"optional": true}}
        });
        assert!(is_peer_optional("react", &pkg_json));
    }

    #[test]
    fn is_peer_optional_false_when_peer_dep_missing() {
        // peerDependenciesMeta.react.optional = true is present but
        // there's no matching peerDependencies.react. This is an
        // authoring anomaly per npm's own rules — `peerDependenciesMeta`
        // is meta ABOUT peer deps; without the underlying peer entry
        // the meta is orphaned. Guard MUST NOT fire.
        let pkg_json = json!({
            "peerDependenciesMeta": {"react": {"optional": true}}
        });
        assert!(!is_peer_optional("react", &pkg_json));
    }

    #[test]
    fn is_peer_optional_false_when_meta_missing() {
        // The dep IS peer-declared but no peerDependenciesMeta entry
        // marks it optional. It's a plain (required) peer dep. Guard
        // MUST NOT fire — m178's `PROVIDED_DEPENDENCY_OF` still emits
        // (via m147's separate flow), but that's not this predicate's
        // concern.
        let pkg_json = json!({
            "peerDependencies": {"react": "^18"}
        });
        assert!(!is_peer_optional("react", &pkg_json));
    }

    #[test]
    fn is_peer_optional_false_when_optional_flag_false() {
        // Peer AND peerDependenciesMeta both present, but `optional`
        // is explicitly false. Guard MUST NOT fire — the dep is a
        // plain (required) peer.
        let pkg_json = json!({
            "peerDependencies": {"react": "^18"},
            "peerDependenciesMeta": {"react": {"optional": false}}
        });
        assert!(!is_peer_optional("react", &pkg_json));
    }

    #[test]
    fn is_peer_optional_false_for_unrelated_entry_name() {
        // Peer + peer-optional both present but named for a DIFFERENT
        // dep. Guard MUST NOT fire when we ask about `lodash` — the
        // predicate is name-scoped.
        let pkg_json = json!({
            "peerDependencies": {"react": "^18"},
            "peerDependenciesMeta": {"react": {"optional": true}}
        });
        assert!(!is_peer_optional("lodash", &pkg_json));
    }

    #[test]
    fn is_peer_optional_false_when_package_json_is_empty() {
        // Defensive check: an empty object (or an entirely non-object
        // Value) MUST NOT trip the predicate.
        let empty = json!({});
        assert!(!is_peer_optional("react", &empty));
        let null = Value::Null;
        assert!(!is_peer_optional("react", &null));
    }
}
