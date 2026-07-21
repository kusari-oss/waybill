//! Corpus cache — data-model.md Entity 4, research §R3.
//!
//! Cache layout mirrors milestone-090 fixture cache exactly:
//! `~/.cache/mikebom/corpus/<source-id-short>/<pin>/` where
//! `source-id-short` is `hex(sha256(url))[..16]` and `<pin>` is the
//! raw SHA (40 hex) or digest algo:hex.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::harness::CorpusInfraError;
use super::manifest::{CorpusTarget, PinnedRef, SourceKind};

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
    /// Honors `MIKEBOM_CORPUS_CACHE_DIR`, then `$XDG_CACHE_HOME/mikebom`,
    /// then `$HOME/.cache/mikebom` per contracts/corpus-harness.md.
    pub fn default() -> Result<Self, CorpusInfraError> {
        let root = if let Ok(explicit) = std::env::var("MIKEBOM_CORPUS_CACHE_DIR") {
            PathBuf::from(explicit)
        } else if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
            PathBuf::from(xdg).join("mikebom")
        } else {
            let home = std::env::var("HOME").map_err(|_| CorpusInfraError::CacheIo {
                path: PathBuf::from("<no-home>"),
                kind: std::io::ErrorKind::NotFound,
            })?;
            PathBuf::from(home).join(".cache").join("mikebom")
        };
        Ok(Self { root })
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
            return Ok(work_dir_for(&dir, target));
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
                // the cache dir itself (mikebom is invoked with
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
