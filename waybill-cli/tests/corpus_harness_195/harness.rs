//! Corpus harness — data-model.md Entities 2, 3, 5 + research §R2.
//!
//! Spawns the released `mikebom` binary against each pinned corpus
//! target, captures the emitted CDX + SPDX 2.3 + SPDX 3 SBOMs, and
//! surfaces structured `AssertionFailure` vs `CorpusInfraError`
//! diagnostics per FR-009 / FR-012.

use std::path::PathBuf;

use super::cache::{CorpusCacheDir, CorpusCacheKey};
use super::manifest::{CorpusTarget, PinnedRef, SourceKind};

// -----------------------------------------------------------------------
// Data-model Entity 2 — EmittedSboms
// -----------------------------------------------------------------------

pub struct EmittedSboms {
    pub cdx: serde_json::Value,
    pub spdx_2_3: serde_json::Value,
    pub spdx_3: serde_json::Value,
    pub paths: EmittedPaths,
}

pub struct EmittedPaths {
    pub cdx: PathBuf,
    pub spdx_2_3: PathBuf,
    pub spdx_3: PathBuf,
}

// -----------------------------------------------------------------------
// Data-model Entity 3 — AssertionFailure
// -----------------------------------------------------------------------

#[derive(Debug)]
pub struct AssertionFailure {
    pub invariant_name: &'static str,
    pub format: FailureFormat,
    pub observed: String,
    pub expected: String,
    pub suggested_action: &'static str,
}

#[derive(Debug)]
pub enum FailureFormat {
    Cdx,
    Spdx23,
    Spdx3,
    All,
}

impl std::fmt::Display for FailureFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            FailureFormat::Cdx => "cdx",
            FailureFormat::Spdx23 => "spdx-2.3",
            FailureFormat::Spdx3 => "spdx-3",
            FailureFormat::All => "all",
        })
    }
}

impl std::fmt::Display for AssertionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "================================================================================")?;
        writeln!(f, "✗ corpus target FAILED")?;
        writeln!(f, "--------------------------------------------------------------------------------")?;
        writeln!(f, "class:     mikebom-regression")?;
        writeln!(f, "invariant: {}", self.invariant_name)?;
        writeln!(f, "format:    {}", self.format)?;
        writeln!(f, "observed:  {}", self.observed)?;
        writeln!(f, "expected:  {}", self.expected)?;
        writeln!(f, "next:      {}", self.suggested_action)?;
        write!(f,   "================================================================================")
    }
}

// -----------------------------------------------------------------------
// Data-model Entity 5 — CorpusInfraError (with Display per m195 T009 fix)
// -----------------------------------------------------------------------

#[derive(Debug)]
pub enum CorpusInfraError {
    GitClone { target: &'static str, stderr: String },
    OciPull { target: &'static str, stderr: String },
    SbomEmission { target: &'static str, stderr: String, missing_files: Vec<PathBuf> },
    CacheIo { path: PathBuf, kind: std::io::ErrorKind },
    OciToolMissing,
}

impl std::fmt::Display for CorpusInfraError {
    /// Renders the corpus-infra failure block per contracts/
    /// corpus-harness.md "Diagnostic Output Format", including the
    /// `underlying error: <stderr excerpt, capped at 500 chars>` line.
    /// The `class:` value is `corpus-infra` (distinct from
    /// `AssertionFailure`'s `mikebom-regression`) per FR-012.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "================================================================================")?;
        writeln!(f, "✗ corpus target FAILED")?;
        writeln!(f, "--------------------------------------------------------------------------------")?;
        writeln!(f, "class:     corpus-infra")?;
        match self {
            CorpusInfraError::GitClone { target, stderr } => {
                writeln!(f, "invariant: git-clone")?;
                writeln!(f, "target:    {target}")?;
                writeln!(f, "next:      check network / verify pinned URL still resolves publicly")?;
                writeln!(f, "underlying error: {}", truncate_stderr(stderr))?;
            }
            CorpusInfraError::OciPull { target, stderr } => {
                writeln!(f, "invariant: oci-pull")?;
                writeln!(f, "target:    {target}")?;
                writeln!(f, "next:      check network / verify pinned image digest still exists in registry")?;
                writeln!(f, "underlying error: {}", truncate_stderr(stderr))?;
            }
            CorpusInfraError::SbomEmission { target, stderr, missing_files } => {
                writeln!(f, "invariant: sbom-emission")?;
                writeln!(f, "target:    {target}")?;
                writeln!(f, "missing:   {missing_files:?}")?;
                writeln!(f, "next:      investigate mikebom regression in the emission path for {target}")?;
                writeln!(f, "underlying error: {}", truncate_stderr(stderr))?;
            }
            CorpusInfraError::CacheIo { path, kind } => {
                writeln!(f, "invariant: cache-io")?;
                writeln!(f, "path:      {path:?}")?;
                writeln!(f, "kind:      {kind:?}")?;
                writeln!(f, "next:      check disk space / permissions on {path:?}")?;
            }
            CorpusInfraError::OciToolMissing => {
                writeln!(f, "invariant: oci-tool-missing")?;
                writeln!(f, "next:      install `docker` (or set MIKEBOM_CORPUS_SKIP_OCI=1 to skip image targets on this host)")?;
            }
        }
        write!(f, "================================================================================")
    }
}

fn truncate_stderr(s: &str) -> String {
    if s.len() <= 500 {
        s.to_string()
    } else {
        format!("{}... [truncated, {} total bytes]", &s[..497], s.len())
    }
}

// -----------------------------------------------------------------------
// Env gates (FR-006, contracts/corpus-harness.md env-var table)
// -----------------------------------------------------------------------

/// FR-006 opt-in gate. `#[test]` functions early-return when this
/// returns false — matches the milestone-101 windows-smoke pattern.
pub fn env_gate() -> bool {
    std::env::var("MIKEBOM_RUN_PUBLIC_CORPUS").as_deref() == Ok("1")
}

pub fn skip_oci_gate() -> bool {
    std::env::var("MIKEBOM_CORPUS_SKIP_OCI").as_deref() == Ok("1")
}

pub fn update_goldens_gate() -> bool {
    std::env::var("MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS").as_deref() == Ok("1")
}

// -----------------------------------------------------------------------
// scan_target — invoke released mikebom binary, capture emitted SBOMs
// -----------------------------------------------------------------------

/// Ensures the pinned artifact is hydrated (clone or pull), invokes
/// the released `mikebom` binary via `env!("CARGO_BIN_EXE_mikebom")`
/// (matches milestone-101 windows-smoke pattern), reads back the
/// emitted CDX + SPDX 2.3 + SPDX 3 files, and returns them parsed.
pub fn scan_target(target: &CorpusTarget) -> Result<EmittedSboms, CorpusInfraError> {
    let cache = CorpusCacheDir::default()?;
    let cache_dir = cache.ensure_hydrated(target)?;

    let out = tempfile::tempdir().map_err(|e| CorpusInfraError::CacheIo {
        path: PathBuf::from("<tmpdir>"),
        kind: e.kind(),
    })?;
    let cdx_path = out.path().join(format!("{}.cdx.json", target.name));
    let spdx23_path = out.path().join(format!("{}.spdx.json", target.name));
    let spdx3_path = out.path().join(format!("{}.spdx3.json", target.name));

    let bin = env!("CARGO_BIN_EXE_mikebom");
    let mut cmd = std::process::Command::new(bin);
    cmd.arg("--offline"); // Corpus scans MUST NOT hit the network
                          // from the mikebom side — network activity
                          // is confined to the cache-hydration step.
    cmd.arg("sbom").arg("scan");
    match &target.source {
        SourceKind::Git { .. } => {
            cmd.arg("--path").arg(&cache_dir);
        }
        SourceKind::OciImage { image_ref } => {
            // Reference by digest so the pull is deterministic even
            // if the tag has moved since cache-hydration.
            let PinnedRef::Digest { algo_hex } = &target.pinned else {
                return Err(CorpusInfraError::SbomEmission {
                    target: target.name,
                    stderr: "OciImage target must use PinnedRef::Digest".to_string(),
                    missing_files: vec![],
                });
            };
            // image_ref is `docker.io/library/postgres:16` — for
            // digest-pinned pulls, strip the tag and append `@<digest>`.
            let base = image_ref.rsplit_once(':').map(|(b, _)| b).unwrap_or(image_ref);
            cmd.arg("--image").arg(format!("{base}@{algo_hex}"));
        }
    }
    cmd.arg("--format").arg("cyclonedx-json,spdx-2.3-json,spdx-3-json");
    cmd.arg("--output").arg(format!("cyclonedx-json={}", cdx_path.display()));
    cmd.arg("--output").arg(format!("spdx-2.3-json={}", spdx23_path.display()));
    cmd.arg("--output").arg(format!("spdx-3-json={}", spdx3_path.display()));
    // Operator override — treat the target's `name` as the SBOM
    // subject so goldens don't hard-code the tempdir path.
    cmd.arg("--root-name").arg(target.name);
    cmd.arg("--root-version").arg(pinned_ref_short(&target.pinned));
    let output = cmd
        .output()
        .map_err(|e| CorpusInfraError::SbomEmission {
            target: target.name,
            stderr: format!("spawn failed: {e}"),
            missing_files: vec![],
        })?;
    if !output.status.success() {
        return Err(CorpusInfraError::SbomEmission {
            target: target.name,
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            missing_files: [&cdx_path, &spdx23_path, &spdx3_path]
                .iter()
                .filter(|p| !p.exists())
                .map(|p| (*p).clone())
                .collect(),
        });
    }
    let cdx = parse_file(&cdx_path, target.name)?;
    let spdx_2_3 = parse_file(&spdx23_path, target.name)?;
    let spdx_3 = parse_file(&spdx3_path, target.name)?;

    // Persist the tempdir beyond the enclosing `out` scope by
    // renaming the emitted files under the target's cache dir; the
    // Layer 2 golden diff step needs the paths after `scan_target`
    // returns.
    let persist_dir = cache_dir.join("emitted");
    std::fs::create_dir_all(&persist_dir).ok();
    let final_cdx = persist_dir.join(format!("{}.cdx.json", target.name));
    let final_spdx23 = persist_dir.join(format!("{}.spdx.json", target.name));
    let final_spdx3 = persist_dir.join(format!("{}.spdx3.json", target.name));
    std::fs::copy(&cdx_path, &final_cdx).map_err(|e| CorpusInfraError::CacheIo {
        path: final_cdx.clone(),
        kind: e.kind(),
    })?;
    std::fs::copy(&spdx23_path, &final_spdx23).map_err(|e| CorpusInfraError::CacheIo {
        path: final_spdx23.clone(),
        kind: e.kind(),
    })?;
    std::fs::copy(&spdx3_path, &final_spdx3).map_err(|e| CorpusInfraError::CacheIo {
        path: final_spdx3.clone(),
        kind: e.kind(),
    })?;

    Ok(EmittedSboms {
        cdx,
        spdx_2_3,
        spdx_3,
        paths: EmittedPaths {
            cdx: final_cdx,
            spdx_2_3: final_spdx23,
            spdx_3: final_spdx3,
        },
    })
}

fn parse_file(path: &std::path::Path, target: &'static str) -> Result<serde_json::Value, CorpusInfraError> {
    let text = std::fs::read_to_string(path).map_err(|e| CorpusInfraError::SbomEmission {
        target,
        stderr: format!("read failed for {}: {e}", path.display()),
        missing_files: vec![path.to_path_buf()],
    })?;
    serde_json::from_str(&text).map_err(|e| CorpusInfraError::SbomEmission {
        target,
        stderr: format!("json parse failed for {}: {e}", path.display()),
        missing_files: vec![],
    })
}

fn pinned_ref_short(p: &PinnedRef) -> String {
    match p {
        PinnedRef::Sha { hex } => hex[..hex.len().min(7)].to_string(),
        PinnedRef::Digest { algo_hex } => {
            algo_hex
                .rsplit_once(':')
                .map(|(_, h)| h[..h.len().min(12)].to_string())
                .unwrap_or_else(|| algo_hex.to_string())
        }
    }
}
