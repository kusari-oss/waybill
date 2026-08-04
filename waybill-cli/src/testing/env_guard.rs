//! Serialized, save-and-restore RAII guard for tests that mutate
//! process env vars.
//!
//! Rust's default `cargo test` runs test functions on multiple
//! threads within a single binary process. Because env vars are
//! process-global (not per-thread), two tests that concurrently
//! set/read the same env var race — one sees the other's write
//! partway through, or restores the wrong prior value.
//!
//! Two documented flakes trace back to this pattern:
//! - `podman_source.rs::discover_storage_root_returns_unreachable_when_directory_unreadable_m206`
//!   (memory `reference_podman_test_flake`)
//! - `cargo.rs::resolve_cargo_metadata_timeout_clamps_above_max_m205`
//!   (memory `reference_m205_cargo_metadata_env_flake`)
//!
//! Both are documented as "green in isolation, flakes under
//! `cargo test --workspace` parallel load". This module is the
//! systemic fix.
//!
//! # Usage
//!
//! ```rust,ignore
//! use waybill::testing::EnvGuard;
//!
//! #[test]
//! fn my_test() {
//!     let mut g = EnvGuard::acquire();
//!     g.set("HOME", "/tmp/test-home");
//!     g.remove("SOME_OTHER_VAR");
//!     // ... test body reads/writes env freely, but no other
//!     // EnvGuard-using test in this binary can run concurrently ...
//! } // guard drops: HOME + SOME_OTHER_VAR restored, mutex released
//! ```
//!
//! # Scope
//!
//! The mutex is per-binary (a `OnceLock<Mutex<()>>` in this module).
//! - Src-side `#[cfg(test)]` tests + integration test binaries that
//!   import `waybill::testing::EnvGuard` share the src-side mutex.
//!   Tests inside the `waybill` bin's test build (cargo.rs,
//!   podman_source.rs, signer.rs) all serialize against each other.
//! - Different integration test binaries under `tests/*.rs` run in
//!   separate PROCESSES (cargo spawns a fresh binary per integration
//!   test file). Different processes each have their own env-var
//!   namespace, so cross-binary races don't exist.
//!
//! # Poison recovery
//!
//! If a test panics while holding the guard, the mutex becomes
//! poisoned. This guard treats the panic as recoverable — subsequent
//! callers extract the inner value via `PoisonError::into_inner`
//! rather than propagating the poison. Rationale: tests are already
//! panicking-halt-the-run territory; refusing to acquire the lock
//! would cascade failures across unrelated tests.

use std::sync::{Mutex, MutexGuard, OnceLock};

/// Process-global mutex serializing env-var-mutating tests within a
/// single binary. Per-process (not per-thread) as documented above.
static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn env_lock() -> &'static Mutex<()> {
    ENV_LOCK.get_or_init(|| Mutex::new(()))
}

/// RAII guard that (1) holds the per-binary env-mutation mutex for
/// its lifetime and (2) restores every env var it set/removed back
/// to its prior state on `Drop`.
pub struct EnvGuard {
    // Held for the lifetime of the guard. Field name starts with an
    // underscore to signal "unused local, present for RAII side
    // effects only" to future readers.
    _lock: MutexGuard<'static, ()>,
    originals: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    /// Acquire the mutex and return a guard with an empty
    /// restore-list. Call `set` / `remove` to record and mutate.
    ///
    /// Blocks if another `EnvGuard` in the same binary is live.
    /// Recovers from mutex poison silently (see module docs).
    pub fn acquire() -> Self {
        let _lock = env_lock().lock().unwrap_or_else(|p| p.into_inner());
        Self {
            _lock,
            originals: Vec::new(),
        }
    }

    /// Convenience constructor for the "batch-apply many vars up front"
    /// pattern used by attestation/signer.rs's pre-existing EnvGuard.
    /// Semantically equivalent to `acquire()` + a loop of `set`/`remove`.
    pub fn setup(vars: &[(&str, Option<&str>)]) -> Self {
        let mut g = Self::acquire();
        for (k, v) in vars {
            match v {
                Some(val) => g.set(k, val),
                None => g.remove(k),
            }
        }
        g
    }

    /// Set an env var, recording its prior value for restore on drop.
    ///
    /// If the same key is set twice through this guard, only the
    /// FIRST recorded prior value is restored (the intermediate
    /// values were transient inside the same test).
    pub fn set(&mut self, key: &str, value: impl AsRef<std::ffi::OsStr>) {
        self.record_prior(key);
        std::env::set_var(key, value);
    }

    /// Remove an env var, recording its prior value for restore on drop.
    pub fn remove(&mut self, key: &str) {
        self.record_prior(key);
        std::env::remove_var(key);
    }

    fn record_prior(&mut self, key: &str) {
        if !self.originals.iter().any(|(k, _)| k == key) {
            self.originals.push((key.to_string(), std::env::var(key).ok()));
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in &self.originals {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    // NB: these tests intentionally use a WAYBILL_ENVGUARD_TEST_*
    // prefix to avoid colliding with real env vars anywhere in the
    // test suite. They ARE subject to the same serialization mutex
    // as every other EnvGuard-using test, so they don't race with
    // podman_source / cargo / signer tests either.

    #[test]
    fn set_records_prior_value_and_restores_on_drop() {
        let key = "WAYBILL_ENVGUARD_TEST_SET_1";
        std::env::set_var(key, "initial");
        {
            let mut g = EnvGuard::acquire();
            g.set(key, "modified");
            assert_eq!(std::env::var(key).unwrap(), "modified");
        }
        assert_eq!(std::env::var(key).unwrap(), "initial");
        std::env::remove_var(key); // test cleanup outside guard
    }

    #[test]
    fn set_records_absence_when_var_did_not_exist() {
        let key = "WAYBILL_ENVGUARD_TEST_SET_2";
        std::env::remove_var(key); // ensure absent
        {
            let mut g = EnvGuard::acquire();
            g.set(key, "modified");
            assert_eq!(std::env::var(key).unwrap(), "modified");
        }
        assert!(std::env::var(key).is_err());
    }

    #[test]
    fn remove_restores_prior_value_on_drop() {
        let key = "WAYBILL_ENVGUARD_TEST_REMOVE_1";
        std::env::set_var(key, "initial");
        {
            let mut g = EnvGuard::acquire();
            g.remove(key);
            assert!(std::env::var(key).is_err());
        }
        assert_eq!(std::env::var(key).unwrap(), "initial");
        std::env::remove_var(key);
    }

    #[test]
    fn multiple_sets_of_same_key_restore_first_prior_value_only() {
        let key = "WAYBILL_ENVGUARD_TEST_MULTI";
        std::env::set_var(key, "initial");
        {
            let mut g = EnvGuard::acquire();
            g.set(key, "first_mod");
            g.set(key, "second_mod"); // should NOT re-record prior
            assert_eq!(std::env::var(key).unwrap(), "second_mod");
        }
        // Restored to the ORIGINAL "initial", not the intermediate
        // "first_mod" that came between the two sets.
        assert_eq!(std::env::var(key).unwrap(), "initial");
        std::env::remove_var(key);
    }

    #[test]
    fn setup_batch_apply_matches_manual_sequence() {
        let key_a = "WAYBILL_ENVGUARD_TEST_SETUP_A";
        let key_b = "WAYBILL_ENVGUARD_TEST_SETUP_B";
        std::env::set_var(key_a, "prior_a");
        std::env::remove_var(key_b);
        {
            let _g = EnvGuard::setup(&[
                (key_a, Some("batch_a")),
                (key_b, Some("batch_b")),
            ]);
            assert_eq!(std::env::var(key_a).unwrap(), "batch_a");
            assert_eq!(std::env::var(key_b).unwrap(), "batch_b");
        }
        assert_eq!(std::env::var(key_a).unwrap(), "prior_a");
        assert!(std::env::var(key_b).is_err());
        std::env::remove_var(key_a);
    }
}
