//! Git repositories built at test time for the git-submodule reader
//! (spec 1530). Everything is local: `file://` submodules over
//! `protocol.file.allow=always`, an isolated `HOME`, and no system config.
//! Nothing is fetched.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Run `git` in `dir` with an isolated environment; panic on failure.
pub fn git(home: &Path, dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .env("HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", home.join(".gitconfig"))
        .env("GIT_AUTHOR_NAME", "waybill-fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "waybill-fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .args(["-c", "protocol.file.allow=always", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} in {} failed: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A tempdir holding `home/` and the repositories a test builds.
pub struct GitFixture {
    pub tmp: tempfile::TempDir,
}

impl GitFixture {
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("home")).expect("home");
        Self { tmp }
    }

    pub fn home(&self) -> PathBuf {
        self.tmp.path().join("home")
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.tmp.path().join(rel)
    }

    pub fn git(&self, dir: &Path, args: &[&str]) -> String {
        git(&self.home(), dir, args)
    }

    /// `git init` a repository with one empty commit; returns its commit.
    pub fn repo(&self, rel: &str) -> String {
        let dir = self.path(rel);
        std::fs::create_dir_all(&dir).expect("mkdir");
        self.git(&dir, &["init", "-q"]);
        self.git(&dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
        self.git(&dir, &["rev-parse", "HEAD"])
    }

    /// Add `dep` (a repository under the fixture) to `super_rel` as a
    /// submodule at `at`, by relative URL, and commit.
    pub fn add_submodule(&self, super_rel: &str, dep_rel: &str, at: &str) {
        let sup = self.path(super_rel);
        let dep = self.path(dep_rel);
        self.git(&sup, &["submodule", "-q", "add", dep.to_str().expect("utf8"), at]);
        self.git(&sup, &["commit", "-q", "-m", &format!("add {at}")]);
    }

    /// `git clone` without initialising submodules.
    pub fn fresh_clone(&self, from_rel: &str, to_rel: &str) -> PathBuf {
        let to = self.path(to_rel);
        self.git(self.tmp.path(), &["clone", "-q", self.path(from_rel).to_str().expect("utf8"), to.to_str().expect("utf8")]);
        to
    }

    /// Record a gitlink at `path` pinned to `commit` without any submodule
    /// repository behind it, then commit.
    pub fn add_gitlink(&self, repo_rel: &str, path: &str, commit: &str) {
        let repo = self.path(repo_rel);
        self.git(&repo, &["update-index", "--add", "--cacheinfo", &format!("160000,{commit},{path}")]);
        self.git(&repo, &["commit", "-q", "-m", &format!("gitlink {path}")]);
    }

    /// Write a file relative to a repository (not committed).
    pub fn write(&self, rel: &str, contents: &str) {
        let p = self.path(rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).expect("mkdir");
        }
        std::fs::write(p, contents).expect("write");
    }
}

impl Default for GitFixture {
    fn default() -> Self {
        Self::new()
    }
}
