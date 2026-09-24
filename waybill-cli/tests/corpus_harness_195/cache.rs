//! Corpus cache — data-model.md Entity 4, research §R3.
//!
//! Cache layout mirrors milestone-090 fixture cache exactly:
//! `~/.cache/waybill/corpus/<source-id-short>/<pin>/` where
//! `source-id-short` is `hex(sha256(url))[..16]` and `<pin>` is the
//! raw SHA (40 hex) or digest algo:hex.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::harness::CorpusInfraError;
use super::manifest::{CorpusTarget, PinnedRef, SourceKind};

/// GHC series waybill may ask for, mirroring `KNOWN_GHC_SERIES` in
/// `scan_fs::package_db::nix::haskell_packages`. Hydration fetches all of
/// them so any candidate subset is a cache hit.
const KNOWN_GHC_SERIES: &[&str] =
    &["9.16.x", "9.14.x", "9.12.x", "9.10.x", "9.8.x", "9.6.x", "9.4.x", "9.0.x"];

/// Read the nixpkgs input a `flake.lock` pins, as `(owner, repo, rev)`.
///
/// `None` when the file is absent, unparseable, names no nixpkgs-shaped
/// GitHub input, or pins no exact revision — in which case there is nothing
/// to hydrate and the target simply exercises other readers.
fn nixpkgs_pin(lock_path: &Path) -> Option<(String, String, String)> {
    let text = std::fs::read_to_string(lock_path).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    let nodes = doc.get("nodes")?.as_object()?;
    for (name, node) in nodes {
        // The `root` node carries only `inputs`, so every lookup below must
        // skip rather than abort. Using `?` here would return from the whole
        // function on the first node without a `locked` block, find no
        // nixpkgs, and silently hydrate nothing -- leaving every downstream
        // assertion vacuously green.
        let Some(locked) = node.get("locked") else { continue };
        if locked.get("type").and_then(|v| v.as_str()) != Some("github") {
            continue;
        }
        let Some(repo) = locked.get("repo").and_then(|v| v.as_str()) else { continue };
        if name != "nixpkgs" && repo != "nixpkgs" {
            continue;
        }
        let Some(owner) = locked.get("owner").and_then(|v| v.as_str()) else { continue };
        let Some(rev) = locked.get("rev").and_then(|v| v.as_str()) else { continue };
        if rev.is_empty() {
            continue;
        }
        return Some((owner.to_string(), repo.to_string(), rev.to_string()));
    }
    None
}



pub struct CorpusCacheKey {
    pub source_id_short: String,
    pub pin: String,
}

impl CorpusCacheKey {
    pub fn for_target(target: &CorpusTarget) -> Self {
        let source_str = match &target.source {
            SourceKind::Git { clone_url } => *clone_url,
            SourceKind::OciImage { image_ref } => *image_ref,
        };
        let mut h = Sha256::new();
        h.update(source_str.as_bytes());
        let digest = h.finalize();
        let source_id_short: String = digest
            .iter()
            .take(8)
            .map(|b| format!("{b:02x}"))
            .collect();
        let pin = match &target.pinned {
            PinnedRef::Sha { hex } => (*hex).to_string(),
            PinnedRef::Digest { algo_hex } => (*algo_hex).replace(':', "-"),
        };
        Self { source_id_short, pin }
    }

    pub fn dir(&self, cache_root: &Path) -> PathBuf {
        cache_root
            .join("corpus")
            .join(&self.source_id_short)
            .join(&self.pin)
    }
}

pub struct CorpusCacheDir {
    pub root: PathBuf,
}

impl CorpusCacheDir {
    /// Honors `WAYBILL_CORPUS_CACHE_DIR`, then `$XDG_CACHE_HOME/waybill`,
    /// then `$HOME/.cache/waybill` per contracts/corpus-harness.md.
    pub fn default() -> Result<Self, CorpusInfraError> {
        let root = if let Ok(explicit) = std::env::var("WAYBILL_CORPUS_CACHE_DIR") {
            PathBuf::from(explicit)
        } else if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
            PathBuf::from(xdg).join("waybill")
        } else {
            let home = std::env::var("HOME").map_err(|_| CorpusInfraError::CacheIo {
                path: PathBuf::from("<no-home>"),
                kind: std::io::ErrorKind::NotFound,
            })?;
            PathBuf::from(home).join(".cache").join("waybill")
        };
        Ok(Self { root })
    }

    /// Where waybill should keep its nixpkgs package-set cache for corpus
    /// runs (#969).
    ///
    /// Deliberately under the corpus cache root rather than the developer's
    /// `~/.cache/waybill/nixpkgs`: a corpus run must not read or write the
    /// machine's own cache, both for isolation and so a stale local copy
    /// cannot make a red run look green.
    pub fn nixpkgs_cache_dir(&self) -> PathBuf {
        self.root.join("corpus-nixpkgs")
    }

    /// Fetch the nixpkgs artifacts a target's pinned revision needs.
    ///
    /// No-op unless the checked-out tree has a `flake.lock` naming a nixpkgs
    /// input with an exact `rev`. The revision is pinned by the target's own
    /// committed lockfile, so the bytes are content-fixed and the result is
    /// deterministic across runs.
    ///
    /// Fetches the package set plus **every** known GHC configuration rather
    /// than only the series the target's `flake.nix` names. waybill derives
    /// its candidate series from that file, and a superset guarantees any
    /// subset is a cache hit. It also matters that this over-fetches rather
    /// than under-fetches: since #975 a cache miss while offline degrades the
    /// whole pass, so an under-fetch would turn into a loud corpus failure
    /// rather than a silent misclassification — but a loud failure is still a
    /// failure, and there is no reason to court one over ~25 KB of configs.
    fn hydrate_nixpkgs(
        &self,
        target: &CorpusTarget,
        repo_dir: &Path,
    ) -> Result<(), CorpusInfraError> {
        let Some((owner, repo, rev)) = nixpkgs_pin(&repo_dir.join("flake.lock")) else {
            return Ok(());
        };
        let dest = self.nixpkgs_cache_dir().join(&rev);
        std::fs::create_dir_all(&dest).map_err(|e| CorpusInfraError::CacheIo {
            path: dest.clone(),
            kind: e.kind(),
        })?;

        let mut wanted: Vec<(String, String)> = vec![(
            "pkgs/development/haskell-modules/hackage-packages.nix".to_string(),
            "hackage-packages.nix".to_string(),
        )];
        for series in KNOWN_GHC_SERIES {
            wanted.push((
                format!("pkgs/development/haskell-modules/configuration-ghc-{series}.nix"),
                format!("configuration-ghc-{series}.nix"),
            ));
        }

        for (path, key) in wanted {
            let out = dest.join(&key);
            if out.exists() {
                continue;
            }
            let url = format!("https://raw.githubusercontent.com/{owner}/{repo}/{rev}/{path}");
            let res = std::process::Command::new("curl")
                .args(["-fsSL", "--retry", "3", "-o"])
                .arg(&out)
                .arg(&url)
                .output()
                .map_err(|e| CorpusInfraError::NixpkgsHydration {
                    target: target.name,
                    stderr: format!("curl spawn failed: {e}"),
                })?;
            if !res.status.success() {
                let _ = std::fs::remove_file(&out);
                // The package set is not optional.
                if key == "hackage-packages.nix" {
                    return Err(CorpusInfraError::NixpkgsHydration {
                        target: target.name,
                        stderr: format!(
                            "could not fetch {url}: exit {:?}: {}",
                            res.status.code(),
                            String::from_utf8_lossy(&res.stderr)
                        ),
                    });
                }
                // curl exits 22 for an HTTP error under `-f`. A 404 means
                // nixpkgs genuinely carries no configuration for that series
                // at this revision, which waybill tolerates ONLINE -- the
                // union rule makes a missing series safe.
                //
                // Offline it cannot tell "absent upstream" from "absent from
                // the cache", and since #975 the latter must degrade the
                // whole pass. Caching the absence as an empty file makes the
                // offline run reproduce the online result exactly: the file
                // hits, contributes no nulled names, and the union is
                // unchanged. Without this, any target whose flake.nix names
                // no explicit series would degrade forever, because the
                // candidate list then spans every known series and nixpkgs
                // does not carry all of them at every revision.
                if res.status.code() == Some(22) {
                    std::fs::write(&out, "").map_err(|e| CorpusInfraError::CacheIo {
                        path: out.clone(),
                        kind: e.kind(),
                    })?;
                } else {
                    // Anything else is a transport failure, not an absence.
                    return Err(CorpusInfraError::NixpkgsHydration {
                        target: target.name,
                        stderr: format!(
                            "could not fetch {url}: exit {:?}: {}",
                            res.status.code(),
                            String::from_utf8_lossy(&res.stderr)
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    /// Ensures the target's pinned artifact is present on disk:
    /// - `Git` targets: clone into `<cache-dir>/repo`, `git checkout <sha>`,
    ///   touch `.corpus-pin-verified` marker on success.
    /// - `OciImage` targets: `docker pull <base>@<digest>` (idempotent —
    ///   image lives in the Docker daemon's own storage; the cache-dir
    ///   only holds a marker file recording the pull).
    pub fn ensure_hydrated(&self, target: &CorpusTarget) -> Result<PathBuf, CorpusInfraError> {
        let key = CorpusCacheKey::for_target(target);
        let dir = key.dir(&self.root);
        std::fs::create_dir_all(&dir).map_err(|e| CorpusInfraError::CacheIo {
            path: dir.clone(),
            kind: e.kind(),
        })?;
        let marker = dir.join(".corpus-pin-verified");
        if marker.exists() {
            // #969: re-check nixpkgs hydration even on a marker hit. The
            // marker records that the REPO is at its pin; it says nothing
            // about the nixpkgs package set, which lives in a different
            // directory and can be evicted independently. Gating hydration
            // behind the marker would leave a cleared nixpkgs cache
            // permanently un-hydrated, and the target would then fail on
            // every run until the whole corpus cache was deleted.
            // `hydrate_nixpkgs` checks each file for existence, so this is
            // free once hydrated.
            let work = work_dir_for(&dir, target);
            self.hydrate_nixpkgs(target, &work)?;
            return Ok(work);
        }
        match &target.source {
            SourceKind::Git { clone_url } => {
                let repo_dir = dir.join("repo");
                if !repo_dir.exists() {
                    let output = std::process::Command::new("git")
                        .arg("clone")
                        .arg(*clone_url)
                        .arg(&repo_dir)
                        .output()
                        .map_err(|e| CorpusInfraError::GitClone {
                            target: target.name,
                            stderr: format!("spawn failed: {e}"),
                        })?;
                    if !output.status.success() {
                        return Err(CorpusInfraError::GitClone {
                            target: target.name,
                            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                        });
                    }
                }
                let PinnedRef::Sha { hex } = &target.pinned else {
                    return Err(CorpusInfraError::GitClone {
                        target: target.name,
                        stderr: "Git target must use PinnedRef::Sha".to_string(),
                    });
                };
                let output = std::process::Command::new("git")
                    .arg("-C")
                    .arg(&repo_dir)
                    .arg("checkout")
                    .arg("--detach")
                    .arg(*hex)
                    .output()
                    .map_err(|e| CorpusInfraError::GitClone {
                        target: target.name,
                        stderr: format!("checkout spawn failed: {e}"),
                    })?;
                if !output.status.success() {
                    return Err(CorpusInfraError::GitClone {
                        target: target.name,
                        stderr: format!(
                            "checkout {hex} failed: {}",
                            String::from_utf8_lossy(&output.stderr)
                        ),
                    });
                }
                // #969: a target whose flake.lock pins nixpkgs needs its
                // Haskell package set on disk before the OFFLINE scan, or
                // resolution degrades and every version/hash assertion
                // passes vacuously. Hydration is the sanctioned place for
                // network activity; the scan itself stays offline.
                self.hydrate_nixpkgs(target, &repo_dir)?;
                std::fs::write(&marker, hex).map_err(|e| CorpusInfraError::CacheIo {
                    path: marker.clone(),
                    kind: e.kind(),
                })?;
                Ok(repo_dir)
            }
            SourceKind::OciImage { image_ref } => {
                // Verify docker (or equivalent) is available.
                let which_docker = std::process::Command::new("docker")
                    .arg("--version")
                    .output();
                if which_docker.is_err() || !which_docker.map(|o| o.status.success()).unwrap_or(false) {
                    return Err(CorpusInfraError::OciToolMissing);
                }
                let PinnedRef::Digest { algo_hex } = &target.pinned else {
                    return Err(CorpusInfraError::OciPull {
                        target: target.name,
                        stderr: "OciImage target must use PinnedRef::Digest".to_string(),
                    });
                };
                let base = image_ref.rsplit_once(':').map(|(b, _)| b).unwrap_or(image_ref);
                let pull_ref = format!("{base}@{algo_hex}");
                let output = std::process::Command::new("docker")
                    .arg("pull")
                    .arg(&pull_ref)
                    .output()
                    .map_err(|e| CorpusInfraError::OciPull {
                        target: target.name,
                        stderr: format!("spawn failed: {e}"),
                    })?;
                if !output.status.success() {
                    return Err(CorpusInfraError::OciPull {
                        target: target.name,
                        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    });
                }
                std::fs::write(&marker, &pull_ref).map_err(|e| CorpusInfraError::CacheIo {
                    path: marker.clone(),
                    kind: e.kind(),
                })?;
                // For OCI targets, the "work dir" convention returns
                // the cache dir itself (waybill is invoked with
                // `--image <ref>@<digest>`, not `--path <dir>`).
                Ok(dir)
            }
        }
    }
}

fn work_dir_for(cache_dir: &Path, target: &CorpusTarget) -> PathBuf {
    match &target.source {
        SourceKind::Git { .. } => cache_dir.join("repo"),
        SourceKind::OciImage { .. } => cache_dir.to_path_buf(),
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod nixpkgs_pin_tests {
    use super::nixpkgs_pin;

    fn write(text: &str) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("flake.lock"), text).unwrap();
        d
    }

    /// The real shape of `haskell/haskell-language-server`'s lock, trimmed:
    /// a `root` node with no `locked` block, then the inputs. An earlier
    /// draft used `?` on `node.get("locked")`, which returns from the whole
    /// function at `root` -- hydrating nothing and making every corpus
    /// assertion pass vacuously.
    #[test]
    fn root_node_without_a_locked_block_does_not_abort_the_search() {
        let d = write(
            r#"{"nodes":{
                 "root":{"inputs":{"nixpkgs":"nixpkgs"}},
                 "flake-utils":{"locked":{"type":"github","owner":"numtide",
                   "repo":"flake-utils","rev":"1170dc"}},
                 "nixpkgs":{"locked":{"type":"github","owner":"NixOS",
                   "repo":"nixpkgs","rev":"cbb5cf35"},
                   "original":{"owner":"NixOS","repo":"nixpkgs","ref":"nixpkgs-unstable"}}
               },"root":"root","version":7}"#,
        );
        assert_eq!(
            nixpkgs_pin(&d.path().join("flake.lock")),
            Some(("NixOS".into(), "nixpkgs".into(), "cbb5cf35".into()))
        );
    }

    /// A lock with no nixpkgs input hydrates nothing, rather than erroring.
    #[test]
    fn a_lock_without_nixpkgs_yields_nothing() {
        let d = write(
            r#"{"nodes":{"root":{"inputs":{}},
                 "flake-utils":{"locked":{"type":"github","owner":"numtide",
                   "repo":"flake-utils","rev":"1170dc"}}},"root":"root","version":7}"#,
        );
        assert_eq!(nixpkgs_pin(&d.path().join("flake.lock")), None);
    }

    /// A moving reference pins no revision, so there is nothing to fetch.
    #[test]
    fn a_lock_with_no_revision_yields_nothing() {
        let d = write(
            r#"{"nodes":{"root":{"inputs":{"nixpkgs":"nixpkgs"}},
                 "nixpkgs":{"locked":{"type":"github","owner":"NixOS",
                   "repo":"nixpkgs","rev":""}}},"root":"root","version":7}"#,
        );
        assert_eq!(nixpkgs_pin(&d.path().join("flake.lock")), None);
    }

    /// Absent file: the ordinary case for every non-Nix target.
    #[test]
    fn a_missing_lock_yields_nothing() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(nixpkgs_pin(&d.path().join("flake.lock")), None);
    }
}
