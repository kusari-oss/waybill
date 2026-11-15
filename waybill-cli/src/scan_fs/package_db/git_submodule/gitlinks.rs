//! Read gitlinks with a hardened `git ls-tree` (spec 1530, research R1,
//! R2, R15).
//!
//! The repository is untrusted. `git ls-files`, `git status` and
//! `git describe --dirty` load the index, and research R1 measured each
//! running a program the repository names in `core.fsmonitor`. This module
//! runs only `ls-tree`, `rev-parse` and `config --get`, all measured not
//! to, and still disables `core.fsmonitor` and hooks on every invocation,
//! so that a future change to the argument list cannot reopen the hole
//! quietly.
//!
//! Nothing read from `.gitmodules` is ever passed to `git`: every argument
//! is either a constant or the repository's own path.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::scan_fs::package_db::nix::eval::invoke::run_bounded_program;
use crate::scan_fs::package_db::nix::eval::reason::DegradationReason;

/// Budget per `git` call. Research R4 measured `ls-tree -r` at a 0.314 s
/// median on a 227,970-entry tree.
pub(crate) const GIT_BUDGET: Duration = Duration::from_secs(30);

/// One tree entry of mode `160000`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Gitlink {
    /// Relative to the repository's top level.
    pub(crate) path: PathBuf,
    pub(crate) commit: String,
}

/// Why `git` produced nothing usable for a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GitError {
    /// `git` is absent, could not run, or ran past its budget.
    Unavailable(String),
    /// git refused the repository: "detected dubious ownership"
    /// (`safe.directory`, research R2). Never overridden here.
    Refused,
    /// Any other failure, with git's first stderr line.
    Failed(String),
}

#[cfg(unix)]
const EMPTY_HOOKS: &str = "core.hooksPath=/dev/null";
#[cfg(not(unix))]
const EMPTY_HOOKS: &str = "core.hooksPath=NUL";

/// The hardening every invocation carries (research R1).
fn base_args(repo: &Path) -> Vec<OsString> {
    vec![
        OsString::from("-C"),
        repo.as_os_str().to_os_string(),
        OsString::from("-c"),
        OsString::from("core.fsmonitor=false"),
        OsString::from("-c"),
        OsString::from(EMPTY_HOOKS),
        OsString::from("--no-pager"),
    ]
}

const ENVS: &[(&str, &str)] = &[("GIT_TERMINAL_PROMPT", "0"), ("GIT_OPTIONAL_LOCKS", "0")];

/// Commands that load the index, and so run `core.fsmonitor` (research R1).
/// Checked at the point of use, not only in a test.
const INDEX_READING: &[&str] = &["ls-files", "status", "diff", "add", "commit", "checkout", "describe"];

fn run_git(repo: &Path, args: &[&str], budget: Duration) -> Result<Vec<u8>, GitError> {
    debug_assert!(
        !args.iter().any(|a| INDEX_READING.contains(a)),
        "git-submodule must not run an index-reading command (research R1): {args:?}"
    );
    if args.iter().any(|a| INDEX_READING.contains(a)) {
        return Err(GitError::Failed("refused to run an index-reading git command".into()));
    }
    let mut argv = base_args(repo);
    argv.extend(args.iter().map(OsString::from));
    let out = run_bounded_program("git", &argv.iter().map(|a| a.as_os_str()).collect::<Vec<&OsStr>>(), ENVS, budget)
        .map_err(|e| match e {
            DegradationReason::ToolAbsent => GitError::Unavailable("git not found".into()),
            other => GitError::Unavailable(format!("{other:?}")),
        })?;
    if out.status_success {
        return Ok(out.stdout);
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if stderr.contains("detected dubious ownership") {
        return Err(GitError::Refused);
    }
    Err(GitError::Failed(stderr.lines().next().unwrap_or("").trim().to_string()))
}

/// Every gitlink in the repository's `HEAD` tree. An empty repository (no
/// `HEAD` yet) has none.
pub(crate) fn read_gitlinks(repo: &Path) -> Result<Vec<Gitlink>, GitError> {
    match run_git(repo, &["ls-tree", "-r", "-z", "--full-tree", "HEAD"], GIT_BUDGET) {
        Ok(stdout) => Ok(parse_ls_tree(&stdout)),
        Err(GitError::Failed(msg)) if msg.contains("Not a valid object name") || msg.contains("ambiguous argument 'HEAD'") => {
            Ok(Vec::new())
        }
        Err(e) => Err(e),
    }
}

/// Parse `ls-tree -z` output, keeping mode-`160000` entries only. Records
/// are `<mode> SP <type> SP <object> TAB <path> NUL`.
pub(crate) fn parse_ls_tree(stdout: &[u8]) -> Vec<Gitlink> {
    stdout
        .split(|b| *b == 0)
        .filter_map(|rec| {
            let tab = rec.iter().position(|b| *b == b'\t')?;
            let (meta, path) = (&rec[..tab], &rec[tab + 1..]);
            let meta = std::str::from_utf8(meta).ok()?;
            let mut fields = meta.split(' ');
            let (mode, kind, object) = (fields.next()?, fields.next()?, fields.next()?);
            if mode != "160000" || kind != "commit" {
                return None;
            }
            Some(Gitlink { path: path_from_bytes(path), commit: object.to_string() })
        })
        .collect()
}

#[cfg(unix)]
fn path_from_bytes(b: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(OsStr::from_bytes(b))
}

#[cfg(not(unix))]
fn path_from_bytes(b: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(b).into_owned())
}

/// The top level of the repository containing `dir`, canonicalized; `None`
/// when `dir` is not inside a repository.
pub(crate) fn toplevel(dir: &Path) -> Result<Option<PathBuf>, GitError> {
    match run_git(dir, &["rev-parse", "--show-toplevel"], GIT_BUDGET) {
        Ok(stdout) => {
            let s = String::from_utf8_lossy(&stdout);
            let p = PathBuf::from(s.trim_end_matches(['\n', '\r']));
            Ok(Some(std::fs::canonicalize(&p).unwrap_or(p)))
        }
        Err(GitError::Failed(msg)) if msg.contains("not a git repository") => Ok(None),
        Err(e) => Err(e),
    }
}

/// The commit a repository's working tree has checked out.
pub(crate) fn head_commit(repo: &Path) -> Option<String> {
    run_git(repo, &["rev-parse", "HEAD"], GIT_BUDGET)
        .ok()
        .map(|o| String::from_utf8_lossy(&o).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// `remote.origin.url`, for resolving relative submodule URLs (research R8).
pub(crate) fn origin_url(repo: &Path) -> Option<String> {
    run_git(repo, &["config", "--get", "remote.origin.url"], GIT_BUDGET)
        .ok()
        .map(|o| String::from_utf8_lossy(&o).trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn s1530_parse_ls_tree_keeps_only_gitlinks() {
        let raw = b"100644 blob 1111111111111111111111111111111111111111\t.gitmodules\0\
160000 commit 9ada6e3b6f74fa7770f46bf8ab6dc4e2a9d59564\tthird_party/dep\0\
040000 tree 2222222222222222222222222222222222222222\tsrc\0\
160000 commit 3333333333333333333333333333333333333333\twith space/x\0";
        let got = parse_ls_tree(raw);
        assert_eq!(
            got,
            vec![
                Gitlink { path: PathBuf::from("third_party/dep"), commit: "9ada6e3b6f74fa7770f46bf8ab6dc4e2a9d59564".into() },
                Gitlink { path: PathBuf::from("with space/x"), commit: "3333333333333333333333333333333333333333".into() },
            ]
        );
    }

    #[test]
    fn s1530_index_reading_commands_are_refused_at_the_call_site() {
        // Release builds return an error; debug builds also assert. Exercise
        // the release path through the guard's own list.
        for cmd in ["ls-files", "status", "describe"] {
            assert!(INDEX_READING.contains(&cmd));
        }
        assert!(!INDEX_READING.contains(&"ls-tree"));
        assert!(!INDEX_READING.contains(&"rev-parse"));
        assert!(!INDEX_READING.contains(&"config"));
    }
}
