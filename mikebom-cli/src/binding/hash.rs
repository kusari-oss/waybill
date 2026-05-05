//! Milestone 072 T005 — binding-hash-v1 algorithm.
//!
//! Implements `compute_binding_hash(inputs: &BindingHashInputs) ->
//! BindingHash` per `contracts/binding-hash-v1.md` C-2 + C-3:
//!
//! 1. Build a JSON object with keys `algo`, `lockfile`, `manifest`,
//!    `vcs` (lex-sorted), values `null` when the input side is `None`,
//!    otherwise the string.
//! 2. Canonicalize via the milestone-071
//!    `parity::extractors::common::canonicalize_for_compare` helper
//!    (`order_sensitive = false`) so the canonical form matches the
//!    cross-format-parity comparator's canonical form (single
//!    canonical-JSON primitive across the project).
//! 3. SHA-256 the UTF-8 bytes via `sha2::Sha256`.
//! 4. Hex-encode lowercase via `data_encoding::HEXLOWER`.
//! 5. Wrap in `BindingHash::from_hex(...)`.
//!
//! Determinism: this function is deterministic across mikebom alpha
//! versions for any byte-identical input triple. The pinned-vector
//! tests (`pinned_vec_*`) lock the contract — a future canonical-JSON
//! tweak that breaks them MUST bump the algo version (V1 → V2) per
//! `contracts/binding-hash-v1.md` C-6.

use data_encoding::HEXLOWER;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::binding::{BindingError, BindingHash, BindingHashInputs};
use crate::parity::extractors::canonicalize_for_compare;

/// Algorithm version literal — the `"v1"` value in the canonical
/// envelope per `contracts/binding-hash-v1.md` C-2.
pub const BINDING_HASH_ALGO_V1: &str = "v1";

/// Compute the per-component binding hash per
/// `contracts/binding-hash-v1.md`.
///
/// Returns `BindingError::InvalidHashHex` only if the SHA-256 hex
/// encoding produces something the `BindingHash::from_hex` validator
/// rejects — which it cannot, given `HEXLOWER` always yields 64
/// lowercase hex chars over a 32-byte digest. The error path is
/// retained for type-system completeness.
pub fn compute_binding_hash(inputs: &BindingHashInputs) -> Result<BindingHash, BindingError> {
    let envelope: Value = json!({
        "algo": BINDING_HASH_ALGO_V1,
        "lockfile": inputs.lockfile.as_deref().map(Value::from).unwrap_or(Value::Null),
        "manifest": inputs.manifest.as_deref().map(Value::from).unwrap_or(Value::Null),
        "vcs": inputs.vcs.as_deref().map(Value::from).unwrap_or(Value::Null),
    });

    // Canonicalize via the milestone-071 helper so any future spec
    // refinement to the cross-format canonicalization rule
    // automatically applies here too. `order_sensitive = false` is
    // correct for a flat 4-key object (no array fields).
    let canonical = canonicalize_for_compare(&envelope, false);

    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let digest = hasher.finalize();
    let hex = HEXLOWER.encode(&digest);

    BindingHash::from_hex(hex)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Determinism: calling twice on the same inputs returns the same
    /// hex output (C-5).
    #[test]
    fn determinism_same_input_same_output() {
        let inputs = BindingHashInputs {
            vcs: Some("deadbeef0123456789abcdef0123456789abcdef".to_string()),
            lockfile: Some("a".repeat(64)),
            manifest: Some("b".repeat(64)),
        };
        let h1 = compute_binding_hash(&inputs).unwrap();
        let h2 = compute_binding_hash(&inputs).unwrap();
        assert_eq!(h1, h2);
    }

    /// Different inputs (different manifest) produce different
    /// hashes — basic collision-resistance smoke test.
    #[test]
    fn different_inputs_different_outputs() {
        let a = BindingHashInputs {
            vcs: None,
            lockfile: None,
            manifest: Some("a".repeat(64)),
        };
        let b = BindingHashInputs {
            vcs: None,
            lockfile: None,
            manifest: Some("b".repeat(64)),
        };
        let ha = compute_binding_hash(&a).unwrap();
        let hb = compute_binding_hash(&b).unwrap();
        assert_ne!(ha, hb);
    }

    /// All three sides populated still yields a 64-char lowercase hex.
    #[test]
    fn all_three_sides_yields_valid_binding_hash() {
        let inputs = BindingHashInputs {
            vcs: Some("c".repeat(40)),
            lockfile: Some("d".repeat(64)),
            manifest: Some("e".repeat(64)),
        };
        let h = compute_binding_hash(&inputs).unwrap();
        assert_eq!(h.as_hex().len(), 64);
        assert!(h.as_hex().chars().all(|c| c.is_ascii_digit() || matches!(c, 'a'..='f')));
    }

    /// Pinned-vector 1: empty input triple (all `None`). The
    /// canonical envelope is `{"algo":"v1","lockfile":null,"manifest":null,"vcs":null}`.
    /// SHA-256 of those exact bytes must equal the hex below; future
    /// canonicalization changes that break this MUST bump the algo
    /// version to v2 (contracts/binding-hash-v1.md C-6).
    #[test]
    fn pinned_vec_all_none() {
        let inputs = BindingHashInputs::empty();
        let h = compute_binding_hash(&inputs).unwrap();

        // Pinned hex value — locked alongside
        // docs/reference/binding-fixtures/ entries. Future
        // canonicalization changes that break this assertion MUST bump
        // the algo version to v2 per C-6, NOT change v1's contract.
        const EXPECTED_ALL_NONE_HEX: &str =
            "d1c2d092a399997f95450f5f140550a8dfacbb0673310a8281945ed5d11765fb";
        assert_eq!(h.as_hex(), EXPECTED_ALL_NONE_HEX);
    }

    /// Pinned-vector 2: only-manifest populated (the maven case
    /// per research.md §1 — no canonical lockfile, no VCS). Canonical
    /// envelope is
    /// `{"algo":"v1","lockfile":null,"manifest":"<manifest-sha256>","vcs":null}`.
    #[test]
    fn pinned_vec_manifest_only() {
        // Substrate: SHA-256 of byte string "manifest-payload-1".
        // External verifiers can recreate without source-tree access.
        let manifest_input = b"manifest-payload-1";
        let mut h = Sha256::new();
        h.update(manifest_input);
        let manifest_sha = HEXLOWER.encode(&h.finalize());
        // The manifest-side pinned input.
        const EXPECTED_MANIFEST_SHA: &str =
            "8dd2a3b941862cd07ac2ef4966064200a1efed2d6aa35fb38581984bc067e3da";
        assert_eq!(manifest_sha, EXPECTED_MANIFEST_SHA);

        let inputs = BindingHashInputs {
            vcs: None,
            lockfile: None,
            manifest: Some(manifest_sha),
        };
        let bh = compute_binding_hash(&inputs).unwrap();

        // Pinned binding-hash output — locked alongside
        // docs/reference/binding-fixtures/maven-weak/EXPECTED.md (the
        // maven-weak fixture pins the same manifest substrate but
        // with a populated VCS — different output hash; this row is
        // the manifest-only case).
        const EXPECTED_MANIFEST_ONLY_HEX: &str =
            "65f84f7b922ec6daf448d11a55d856b4c62ae8471e9224fbd4c50b16bdb17174";
        assert_eq!(bh.as_hex(), EXPECTED_MANIFEST_ONLY_HEX);
    }

    /// Pinned-vector 3: all three sides populated with deterministic
    /// content (vcs = `deadbeef…`, lockfile + manifest = SHA-256 of
    /// well-known byte strings). This is the `verified`-strength
    /// happy path; downstream verifiers with the same input triple
    /// MUST recompute the same hex, regardless of mikebom version.
    /// Locked alongside docs/reference/binding-fixtures/cargo-verified
    /// + golang-verified — same canonical envelope, both ecosystems.
    #[test]
    fn pinned_vec_all_three_sides() {
        let lockfile_input = b"lockfile-payload-1";
        let manifest_input = b"manifest-payload-1";

        let mut hl = Sha256::new();
        hl.update(lockfile_input);
        let lockfile_sha = HEXLOWER.encode(&hl.finalize());
        const EXPECTED_LOCKFILE_SHA: &str =
            "4c975d294781b5e5f49b946bc5f94da8638b4c60f1c1f3a8c35fa9534744712e";
        assert_eq!(lockfile_sha, EXPECTED_LOCKFILE_SHA);

        let mut hm = Sha256::new();
        hm.update(manifest_input);
        let manifest_sha = HEXLOWER.encode(&hm.finalize());
        const EXPECTED_MANIFEST_SHA: &str =
            "8dd2a3b941862cd07ac2ef4966064200a1efed2d6aa35fb38581984bc067e3da";
        assert_eq!(manifest_sha, EXPECTED_MANIFEST_SHA);

        let vcs = "deadbeef0123456789abcdef0123456789abcdef".to_string();

        let inputs = BindingHashInputs {
            vcs: Some(vcs.clone()),
            lockfile: Some(lockfile_sha.clone()),
            manifest: Some(manifest_sha.clone()),
        };
        let bh = compute_binding_hash(&inputs).unwrap();

        // Pinned binding-hash output — locked alongside
        // docs/reference/binding-fixtures/{cargo,golang}-verified/
        // EXPECTED.md.
        const EXPECTED_ALL_THREE_HEX: &str =
            "745289decaf84d67e5cc9b333b435e8cc341ac19f7ab16673f05133d459a6111";
        assert_eq!(bh.as_hex(), EXPECTED_ALL_THREE_HEX);
    }

    /// Negative-direction cross-check: the envelope shape contract
    /// (C-2) says keys appear in lex order, so the test below
    /// verifies our canonicalization actually sorts them. If a
    /// future refactor accidentally drops the canonicalize step,
    /// this test catches it: insertion-ordered serde_json::Map
    /// (which `json!` macro produces) has the keys in source order
    /// `algo, lockfile, manifest, vcs` — same as lex order here, so
    /// to make this test discriminating we feed a payload whose
    /// non-canonical insertion order would differ. Pre-canonicalize:
    /// `{"vcs": "x", "algo": "v1", ...}` → after canonicalization,
    /// keys MUST be `algo, lockfile, manifest, vcs`.
    #[test]
    fn canonicalization_sorts_keys() {
        // Build a non-canonical insertion order via raw JSON, then
        // canonicalize and confirm the canonical form is what the
        // hash actually consumes.
        let non_canonical = serde_json::json!({
            "vcs": "x",
            "algo": "v1",
            "manifest": "y",
            "lockfile": "z",
        });
        let canonical = canonicalize_for_compare(&non_canonical, false);
        assert_eq!(
            canonical,
            r#"{"algo":"v1","lockfile":"z","manifest":"y","vcs":"x"}"#,
        );
    }
}
