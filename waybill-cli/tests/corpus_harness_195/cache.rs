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
pub(crate) const KNOWN_GHC_SERIES: &[&str] =
    &["9.16.x", "9.14.x", "9.12.x", "9.10.x", "9.8.x", "9.6.x", "9.4.x", "9.0.x"];

/// Read the nixpkgs input a `flake.lock` pins, as `(owner, repo, rev)`.
///
/// `None` when the file is absent, unparseable, names no nixpkgs-shaped
/// GitHub input, or pins no exact revision — in which case there is nothing
/// to hydrate and the target simply exercises other readers.
pub(crate) fn nixpkgs_pin(lock_path: &Path) -> Option<(String, String, String)> {
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

/// #1677: exclusive use of one pinned cache directory, released on drop.
///
/// Targets pinned to one commit share one cache directory (`CorpusCacheKey`
/// ignores the target's name), and the corpus runs its targets in parallel.
/// Without this, a second target cloned or checked out into the directory
/// while the first was still cloning it: the first CI run of the #1677
/// targets failed twice with `fatal: unable to read tree`. Held across
/// hydration and `harness::prepared_dir`, not the scan.
pub struct HydrationLock {
    path: PathBuf,
}

impl Drop for HydrationLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.path);
    }
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

        let mut wanted: Vec<(String, String)> = vec![
            (
                "pkgs/development/haskell-modules/hackage-packages.nix".to_string(),
                "hackage-packages.nix".to_string(),
            ),
            // Issue #1033 — the overrides that supersede versions in the
            // generated set. Without it here the corpus scan runs `--offline`
            // against a cache that lacks the file, waybill's best-effort
            // retrieval degrades to the generated versions, and the lane
            // reports goldens that look exactly as they did before the fix.
            // The first regeneration after the fix did precisely that, which
            // is how this line came to be written.
            (
                "pkgs/development/haskell-modules/configuration-common.nix".to_string(),
                "configuration-common.nix".to_string(),
            ),
        ];
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

    /// Wait for and take [`HydrationLock`] on `target`'s cache directory. The
    /// lock is a directory, created atomically. One left by a run that died
    /// is taken over after two hours; waiting gives up after one.
    pub fn lock(&self, target: &CorpusTarget) -> Result<HydrationLock, CorpusInfraError> {
        let dir = CorpusCacheKey::for_target(target).dir(&self.root);
        std::fs::create_dir_all(&dir).map_err(|e| CorpusInfraError::CacheIo { path: dir.clone(), kind: e.kind() })?;
        let path = dir.join(".hydrate.lock");
        let start = std::time::Instant::now();
        loop {
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(HydrationLock { path }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .is_ok_and(|t| t.elapsed().is_ok_and(|d| d > std::time::Duration::from_secs(7200)));
                    if stale {
                        let _ = std::fs::remove_dir(&path);
                        continue;
                    }
                    if start.elapsed() > std::time::Duration::from_secs(3600) {
                        return Err(CorpusInfraError::CacheIo { path, kind: std::io::ErrorKind::TimedOut });
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                Err(e) => return Err(CorpusInfraError::CacheIo { path, kind: e.kind() }),
            }
        }
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
            // Every run, not once: the nix store is not part of the corpus
            // cache, and CI's starts empty even when this cache is restored.
            prewarm_nix_closure(target, &work)?;
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
                prewarm_nix_closure(target, &repo_dir)?;
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

/// Put a `ScanMode::NixClosure` target's flake inputs in the nix store
/// before its scan, so the scan — which runs without `--offline`, because the
/// tier refuses it — finds everything local and nix has nothing to fetch.
/// No-op for every other target.
///
/// An `allow_ifd` target is **built** instead (`nix build --no-link`), for two
/// measured reasons (#1283, public-corpus run 43109906381):
///
/// - Its output is in no binary cache, so without a local build waybill cannot
///   read the runtime closure and falls back to the reachability walk. The walk
///   marked glibc, binutils and gcc as build tooling, recall fell to 77% and
///   82%, and the truth it is scored against was taken from a built output.
/// - `nix flake archive` fetches every locked input, including ones the output
///   never uses. openai's `all-cabal-hashes` input no longer matches its locked
///   narHash, so archiving failed although building succeeds.
///
/// The build allows import-from-derivation, as the scan does, and keeps
/// refusing flake-supplied configuration. Measured on the recall-truth run
/// (43107760056): 93 s for slack-web, 579 s for openai, 423 s for slacklinker.
///
/// **Bounded since #1704.** This build had no timeout and captured nothing
/// until it exited. Twice in two days (public-corpus runs 45420664778 and
/// 45530419336) slacklinker was still in its test 37 minutes after starting,
/// against 8 to 13 minutes in 12 successful runs, and the 45-minute job
/// timeout cancelled the whole run with no record of where it stopped.
/// Every `nix` call waybill itself makes is already bounded.
///
/// Nix's own `max-silent-time` and `timeout` apply per derivation, so they
/// name the derivation that hung but do not bound the whole build; the
/// wall-clock deadline does. Build logs go to a file whose tail becomes the
/// error, so a failure says what was building.
pub(crate) fn prewarm_nix_closure(target: &CorpusTarget, repo_dir: &Path) -> Result<(), CorpusInfraError> {
    let super::manifest::ScanMode::NixClosure { attr, allow_ifd } = target.scan_mode else {
        return Ok(());
    };
    if allow_ifd {
        let installable = format!(".#{attr}");
        let max_silent = PREWARM_MAX_SILENT_SECS.to_string();
        let per_drv = PREWARM_DERIVATION_TIMEOUT_SECS.to_string();
        let started = std::time::Instant::now();
        eprintln!("corpus: {}: prewarm `nix build {installable}` started", target.name);
        let result = run_with_deadline(
            std::process::Command::new("nix")
                .args([
                    "build",
                    "--no-link",
                    "--print-build-logs",
                    "--option",
                    "allow-import-from-derivation",
                    "true",
                    "--option",
                    "accept-flake-config",
                    "false",
                    "--option",
                    "max-silent-time",
                    &max_silent,
                    "--option",
                    "timeout",
                    &per_drv,
                    &installable,
                ])
                .current_dir(repo_dir),
            std::time::Duration::from_secs(PREWARM_WALL_SECS),
        );
        eprintln!(
            "corpus: {}: prewarm `nix build` {} after {}s",
            target.name,
            match &result {
                Ok(_) => "succeeded",
                Err(_) => "FAILED",
            },
            started.elapsed().as_secs()
        );
        return result.map_err(|stderr| CorpusInfraError::NixRealise { target: target.name, stderr });
    }
    let output = std::process::Command::new("nix")
        .args(["flake", "archive", "--json", "."])
        .current_dir(repo_dir)
        .output()
        .map_err(|e| CorpusInfraError::NixFlakeArchive {
            target: target.name,
            stderr: format!("spawn failed: {e}"),
        })?;
    if !output.status.success() {
        return Err(CorpusInfraError::NixFlakeArchive {
            target: target.name,
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }
    Ok(())
}

/// The prewarm build's limits (#1704). Measured normal: 423 s for
/// slacklinker's whole build on the recall-truth run, so a derivation silent
/// for 10 minutes or building for 20 is not making progress. The wall-clock
/// deadline (25 minutes) fails the test well inside the job's 45, with the
/// corpus step starting about 5 minutes in.
pub(crate) const PREWARM_MAX_SILENT_SECS: u64 = 600;
pub(crate) const PREWARM_DERIVATION_TIMEOUT_SECS: u64 = 1200;
pub(crate) const PREWARM_WALL_SECS: u64 = 1500;

/// Run `cmd` to completion or kill it at `deadline`. `Ok` on a zero exit;
/// otherwise `Err` with the last 40 lines of its output, where nix puts the
/// error and the end of the build log that led to it.
pub(crate) fn run_with_deadline(
    cmd: &mut std::process::Command,
    deadline: std::time::Duration,
) -> Result<(), String> {
    let log = tempfile::NamedTempFile::new().map_err(|e| format!("log file: {e}"))?;
    // One handle, cloned: two `reopen()`s would each keep their own offset,
    // and stderr would overwrite the start of stdout.
    let out = log.reopen().map_err(|e| format!("log file: {e}"))?;
    let err = out.try_clone().map_err(|e| format!("log file: {e}"))?;
    let mut child = cmd
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("spawn failed: {e}"))?;
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_secs(1)),
            Err(e) => return Err(format!("wait failed: {e}")),
        }
    };
    let text = std::fs::read_to_string(log.path()).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let tail = lines[lines.len().saturating_sub(40)..].join("\n");
    match status {
        Some(s) if s.success() => Ok(()),
        Some(s) => Err(format!("exit {s}; last 40 lines:\n{tail}")),
        None => Err(format!(
            "killed after the {}s deadline; last 40 lines:\n{tail}",
            deadline.as_secs()
        )),
    }
}

pub(crate) fn work_dir_for(cache_dir: &Path, target: &CorpusTarget) -> PathBuf {
    match &target.source {
        SourceKind::Git { .. } => cache_dir.join("repo"),
        SourceKind::OciImage { .. } => cache_dir.to_path_buf(),
    }
}
