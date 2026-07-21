//! Milestone 072 T006 — per-ecosystem source-input extractors.
//!
//! Owns the dispatch from a (scan-root path, ecosystem) pair to the
//! `BindingHashInputs` triple `(vcs, lockfile, manifest)` per
//! `contracts/binding-hash-v1.md` C-7 + research.md §1.
//!
//! Each ecosystem's inputs are listed in research.md §1's table.
//! Inputs that don't exist on disk are tolerated as `None` rather
//! than errors — that's how the binding-strength derivation
//! (`weak` / `unknown`) surfaces missing evidence to consumers.
//!
//! Extraction strategy:
//!
//! - **VCS commit**: `git rev-parse HEAD` shell-out from the scan
//!   root. Mirrors milestone 053's `git describe` pattern at
//!   `mikebom-cli/src/scan_fs/package_db/golang.rs:733`. Returns
//!   `None` when git is absent OR the path isn't inside a git
//!   working tree (e.g., extracted tarball, container rootfs).
//! - **Lockfile**: SHA-256 of the on-disk bytes per
//!   `contracts/binding-hash-v1.md` C-1. No re-parsing — the bytes
//!   are the contract.
//! - **Manifest**: same as lockfile — SHA-256 of the bytes as on
//!   disk.

use std::path::{Path, PathBuf};
use std::process::Command;

use data_encoding::HEXLOWER;
use sha2::{Digest, Sha256};

use crate::binding::BindingHashInputs;

/// Recognized source-tier ecosystems for binding-input extraction.
/// Mirrors the 6-row table in research.md §1 + contracts/binding-hash-v1.md
/// C-7. PURL ecosystem strings (`cargo`, `npm`, `pypi`, `gem`, `maven`,
/// `golang`) map to these variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingEcosystem {
    Cargo,
    Npm,
    Pip,
    Gem,
    Maven,
    Golang,
}

impl BindingEcosystem {
    /// Decode a PURL ecosystem string to a `BindingEcosystem`.
    /// Returns `None` for ecosystems mikebom doesn't bind today
    /// (deb / apk / rpm / generic — base-layer system packages,
    /// which carry `binding: unknown { reason: "base-layer-system-package" }`).
    pub fn from_purl_ecosystem(eco: &str) -> Option<Self> {
        match eco {
            "cargo" => Some(Self::Cargo),
            "npm" => Some(Self::Npm),
            "pypi" => Some(Self::Pip),
            "gem" => Some(Self::Gem),
            "maven" => Some(Self::Maven),
            "golang" => Some(Self::Golang),
            _ => None,
        }
    }
}

/// Extract the `BindingHashInputs` triple for an ecosystem from a
/// project's source root directory. Tolerates missing inputs by
/// leaving the corresponding side `None`.
///
/// The `vcs` side is sourced from `git rev-parse HEAD` from the
/// scan root. The `lockfile` and `manifest` sides are SHA-256 over
/// the on-disk bytes per contract C-1.
pub fn extract_source_inputs(
    eco: BindingEcosystem,
    scan_root: &Path,
) -> BindingHashInputs {
    let vcs = git_rev_parse_head(scan_root);
    let (lockfile_path, manifest_path) = ecosystem_paths(eco, scan_root);

    let lockfile = lockfile_path.as_ref().and_then(|p| sha256_file(p));
    let manifest = manifest_path.as_ref().and_then(|p| sha256_file(p));

    BindingHashInputs {
        vcs,
        lockfile,
        manifest,
    }
}

/// Return the canonical (lockfile, manifest) path tuple per
/// ecosystem. Falls back through the documented alternates per
/// research.md §1 (e.g., npm: `package-lock.json` →
/// `yarn.lock` → `pnpm-lock.yaml`; pip: `poetry.lock` → `pdm.lock`).
fn ecosystem_paths(
    eco: BindingEcosystem,
    scan_root: &Path,
) -> (Option<PathBuf>, Option<PathBuf>) {
    let pick = |relatives: &[&str]| -> Option<PathBuf> {
        relatives
            .iter()
            .map(|r| scan_root.join(r))
            .find(|p| p.is_file())
    };
    match eco {
        BindingEcosystem::Cargo => (pick(&["Cargo.lock"]), pick(&["Cargo.toml"])),
        BindingEcosystem::Npm => (
            pick(&["package-lock.json", "yarn.lock", "pnpm-lock.yaml"]),
            pick(&["package.json"]),
        ),
        BindingEcosystem::Pip => {
            // Lockfile: prefer poetry.lock, then pdm.lock, then
            // requirements*.txt (when none of the modern lockfiles
            // are present, fall back to any well-known requirements
            // file). The contract for requirements*.txt is "SHA-256
            // of concatenated --hash= lines"; for PR-A we
            // simplify to "SHA-256 of the file bytes" — same on
            // both emit + verify side, so determinism still holds
            // (contracts/binding-hash-v1.md C-5).
            let lock = pick(&[
                "poetry.lock",
                "pdm.lock",
                "requirements.txt",
                "requirements-prod.txt",
                "requirements-runtime.txt",
            ]);
            (lock, pick(&["pyproject.toml"]))
        }
        BindingEcosystem::Gem => {
            // Manifest = the project's own `.gemspec` (NOT vendored).
            // Search the repo root for a single .gemspec; ambiguous
            // multi-gemspec layouts return None.
            let gemspec = std::fs::read_dir(scan_root).ok().and_then(|rd| {
                let mut found: Option<PathBuf> = None;
                for entry in rd.flatten() {
                    let p = entry.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("gemspec") {
                        if found.is_some() {
                            // More than one .gemspec: ambiguous,
                            // skip rather than guess.
                            return None;
                        }
                        found = Some(p);
                    }
                }
                found
            });
            (pick(&["Gemfile.lock"]), gemspec)
        }
        BindingEcosystem::Maven => {
            // Maven has no canonical lockfile per research.md §1.
            // Strength caps at `weak` for maven.
            (None, pick(&["pom.xml"]))
        }
        BindingEcosystem::Golang => (pick(&["go.sum"]), pick(&["go.mod"])),
    }
}

/// SHA-256 the file bytes at `path`. Returns `None` on read error.
/// The hex-encoded lowercase output is what the C-1 contract
/// specifies as the lockfile / manifest input.
fn sha256_file(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Some(HEXLOWER.encode(&hasher.finalize()))
}

/// Shell out to `git rev-parse HEAD`. Mirrors milestone 053's
/// `git describe` pattern at golang.rs:733.
///
/// Returns `None` when:
///   - git is not on $PATH;
///   - `scan_root` is not inside a git working tree (the command
///     exits non-zero, e.g., "fatal: not a git repository");
///   - the command produces empty stdout.
fn git_rev_parse_head(scan_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("HEAD")
        .current_dir(scan_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let commit = stdout.trim();
    if commit.is_empty() {
        return None;
    }
    Some(commit.to_string())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    use std::fs;

    /// Cargo: synthesize Cargo.toml + Cargo.lock; verify both
    /// `lockfile` and `manifest` populate with stable hex.
    #[test]
    fn cargo_lockfile_and_manifest_populate() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            b"[package]\nname = \"foo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("Cargo.lock"),
            b"# This file is automatically @generated by Cargo.\n",
        )
        .unwrap();

        let inputs = extract_source_inputs(BindingEcosystem::Cargo, dir.path());
        assert!(inputs.lockfile.is_some(), "lockfile should be populated");
        assert!(inputs.manifest.is_some(), "manifest should be populated");
        assert_eq!(inputs.lockfile.as_ref().unwrap().len(), 64);
        assert_eq!(inputs.manifest.as_ref().unwrap().len(), 64);
    }

    /// Cargo: missing Cargo.lock → only manifest populates,
    /// strength would be `weak`.
    #[test]
    fn cargo_missing_lockfile_yields_none_for_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            b"[package]\nname = \"foo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let inputs = extract_source_inputs(BindingEcosystem::Cargo, dir.path());
        assert!(inputs.lockfile.is_none());
        assert!(inputs.manifest.is_some());
    }

    /// npm: package-lock.json wins over yarn.lock when both exist
    /// (pick-order in `ecosystem_paths`).
    #[test]
    fn npm_package_lock_preferred_over_yarn_lock() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), b"{\"name\":\"foo\"}").unwrap();
        fs::write(dir.path().join("yarn.lock"), b"# yarn lockfile\n").unwrap();
        fs::write(
            dir.path().join("package-lock.json"),
            b"{\"name\":\"foo\",\"lockfileVersion\":3}",
        )
        .unwrap();

        let inputs = extract_source_inputs(BindingEcosystem::Npm, dir.path());

        // The lockfile hash must be SHA-256 of the package-lock.json
        // bytes, NOT yarn.lock bytes.
        let expected = sha256_file(&dir.path().join("package-lock.json")).unwrap();
        assert_eq!(inputs.lockfile.as_deref(), Some(expected.as_str()));
    }

    /// npm: only yarn.lock present → falls through to it.
    #[test]
    fn npm_falls_back_to_yarn_lock() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), b"{\"name\":\"foo\"}").unwrap();
        fs::write(dir.path().join("yarn.lock"), b"# yarn lockfile\n").unwrap();
        let inputs = extract_source_inputs(BindingEcosystem::Npm, dir.path());
        assert!(inputs.lockfile.is_some());
    }

    /// golang: go.mod + go.sum populate the triple.
    #[test]
    fn golang_go_mod_and_go_sum_populate() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("go.mod"),
            b"module example.com/foo\n\ngo 1.22\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("go.sum"),
            b"github.com/x/y v0.1.0/go.mod h1:abc...\n",
        )
        .unwrap();
        let inputs = extract_source_inputs(BindingEcosystem::Golang, dir.path());
        assert!(inputs.lockfile.is_some());
        assert!(inputs.manifest.is_some());
    }

    /// maven: only pom.xml; lockfile stays None per the
    /// "strength capped at weak" contract.
    #[test]
    fn maven_lockfile_always_none() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("pom.xml"),
            b"<?xml version=\"1.0\"?><project></project>",
        )
        .unwrap();
        let inputs = extract_source_inputs(BindingEcosystem::Maven, dir.path());
        assert!(inputs.lockfile.is_none());
        assert!(inputs.manifest.is_some());
    }

    /// VCS: outside-of-git directory yields None for vcs (no
    /// false-positive hash; binding-strength derivation handles it).
    #[test]
    fn vcs_outside_git_repo_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        // The default tempdir is NOT a git checkout. `git rev-parse
        // HEAD` exits non-zero and we surface that as `None`.
        let result = git_rev_parse_head(dir.path());
        assert!(result.is_none(), "expected None outside a git repo");
    }

    /// PURL ecosystem decode: known strings map; unknowns return None.
    #[test]
    fn purl_ecosystem_decode_round_trip() {
        assert_eq!(
            BindingEcosystem::from_purl_ecosystem("cargo"),
            Some(BindingEcosystem::Cargo),
        );
        assert_eq!(
            BindingEcosystem::from_purl_ecosystem("npm"),
            Some(BindingEcosystem::Npm),
        );
        assert_eq!(
            BindingEcosystem::from_purl_ecosystem("pypi"),
            Some(BindingEcosystem::Pip),
        );
        assert_eq!(
            BindingEcosystem::from_purl_ecosystem("gem"),
            Some(BindingEcosystem::Gem),
        );
        assert_eq!(
            BindingEcosystem::from_purl_ecosystem("maven"),
            Some(BindingEcosystem::Maven),
        );
        assert_eq!(
            BindingEcosystem::from_purl_ecosystem("golang"),
            Some(BindingEcosystem::Golang),
        );
        // Out-of-scope ecosystems (deb, apk, rpm, generic) return None
        // and the caller emits binding: unknown.
        assert_eq!(BindingEcosystem::from_purl_ecosystem("deb"), None);
        assert_eq!(BindingEcosystem::from_purl_ecosystem("apk"), None);
        assert_eq!(BindingEcosystem::from_purl_ecosystem("rpm"), None);
        assert_eq!(BindingEcosystem::from_purl_ecosystem("generic"), None);
    }
}
