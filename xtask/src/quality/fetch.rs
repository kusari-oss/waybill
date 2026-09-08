// milestone 770 — T011: shallow fetch at a pinned SHA.
//
// Contract xtask-quality-cli.md § C-3. Deliberately NOT `git clone`:
// m195's cache does a full clone then checkout, which is fine for cobra
// and ruinous for kubernetes/pytorch/mongo. GitHub serves arbitrary SHAs,
// so a depth-1 fetch retrieves one commit's tree and no history.
//
// C-3.1: no --recurse-submodules. pytorch's third_party/ stays empty by
// design (research R6) — deterministic, therefore rangeable.

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
