// milestone 770 — T011: shallow fetch at a pinned SHA.
//
// Contract xtask-quality-cli.md § C-3. Deliberately NOT `git clone`:
// m195's cache does a full clone then checkout, which is fine for cobra
// and ruinous for kubernetes/pytorch/mongo. GitHub serves arbitrary SHAs,
// so a depth-1 fetch retrieves one commit's tree and no history.
//
// C-3.1: no --recurse-submodules. pytorch's third_party/ stays empty by
// design (research R6) — deterministic, therefore rangeable.
//
// C-3.3: Git LFS smudging is disabled for the same reason. Whether a
// checkout contains real content or 131-byte pointer files otherwise
// depends on whether the host happens to have git-lfs installed, which
// makes the fixture — and therefore every bound authored against it —
// non-reproducible across machines.
//
// This was not theoretical. `pants-backend-ai` stores *.bin and *.so
// under LFS. GitHub runners ship git-lfs, so CI smudged them into real
// ELF binaries and waybill's binary tier emitted 46 extra components
// (the libraries plus their DT_NEEDED entries and an embedded openssl
// version), while 14 files moved out of file-tier orphan into
// binary-tier claimed. Hosts without git-lfs saw pointer files and
// emitted none of it. The corpus read 317 pkgs / 45 files where its
// author had measured 271 / 59, and the lane failed every night from
// 2026-09-09 (issue #832).
//
// Pinning smudge OFF matches the authored bounds, so no re-baseline is
// needed. Verified on ubuntu-latest: GIT_LFS_SKIP_SMUDGE=1 reproduces
// 271 / 59 exactly.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::quality::config::Target;

/// Marker file written on a successful checkout; its presence is the
/// cache-hit test (C-3.2).
const MARKER: &str = ".waybill-quality-ok";

pub struct FetchOutcome {
    pub path: PathBuf,
    pub cache_hit: bool,
}

/// `<cache_root>/<name>/<pin-short>` — keyed by pin so re-pinning a
/// target does not clobber the previous checkout.
pub fn target_dir(cache_root: &Path, target: &Target) -> PathBuf {
    cache_root.join(target.name.as_str()).join(target.pin.short())
}

pub fn fetch(cache_root: &Path, target: &Target, refresh: bool) -> Result<FetchOutcome, String> {
    let dir = target_dir(cache_root, target);
    if dir.join(MARKER).exists() {
        if !refresh {
            return Ok(FetchOutcome { path: dir, cache_hit: true });
        }
        std::fs::remove_dir_all(&dir).map_err(|e| format!("cannot clear cache dir: {e}"))?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create cache dir: {e}"))?;

    git(&dir, &["init", "-q"])?;
    git(&dir, &["remote", "add", "origin", &target.url])?;
    git(&dir, &["fetch", "-q", "--depth", "1", "origin", target.pin.as_fetch_spec()])?;
    git(&dir, &["checkout", "-q", "FETCH_HEAD"])?;

    std::fs::write(dir.join(MARKER), target.pin.as_fetch_spec())
        .map_err(|e| format!("cannot write cache marker: {e}"))?;
    Ok(FetchOutcome { path: dir, cache_hit: false })
}

fn git(cwd: &Path, args: &[&str]) -> Result<(), String> {
    let out = Command::new("git")
        .args(args)
        // C-3.3: never smudge LFS pointers. Set on every git invocation
        // rather than just the checkout, so a future step that also
        // materialises content cannot silently reintroduce the split.
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git {}: spawn failed: {e}", args.join(" ")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let first = stderr.lines().next().unwrap_or("(no stderr)");
        return Err(format!("git {}: {first}", args.join(" ")));
    }
    Ok(())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::quality::config::CorpusConfig;

    fn target() -> Target {
        let c = CorpusConfig::parse(
            r#"
sbomqs_version = "v2.0.6"
[[targets]]
name = "go-cobra"
url = "https://github.com/spf13/cobra"
sha = "a655097faf7d54f78933a815984b9919d51a05d2"
"#,
        )
        .unwrap();
        c.targets[0].clone()
    }

    #[test]
    fn target_dir_is_keyed_by_name_and_pin() {
        let d = target_dir(Path::new("/cache"), &target());
        assert_eq!(d, PathBuf::from("/cache/go-cobra/a655097faf7d"));
    }

    #[test]
    fn missing_marker_is_a_cache_miss() {
        let tmp = tempfile::tempdir().unwrap();
        let d = target_dir(tmp.path(), &target());
        std::fs::create_dir_all(&d).unwrap();
        assert!(!d.join(MARKER).exists());
    }

    /// A bad remote must surface as an Err carrying git's own message,
    /// which the caller maps to UnmeasurableReason::FetchFailed (FR-007).
    #[test]
    fn unreachable_remote_returns_error_not_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let mut t = target();
        t.url = format!("file://{}/definitely-not-a-repo", tmp.path().display());
        let r = fetch(tmp.path(), &t, false);
        assert!(r.is_err());
    }
}
