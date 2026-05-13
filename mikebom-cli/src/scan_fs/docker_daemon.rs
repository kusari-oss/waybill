//! Local Docker-daemon image source (milestone 044 commit 1).
//!
//! Trivy and syft both default to checking the local docker daemon
//! before reaching for a registry pull. mikebom adopts the same
//! convention via the `--image-src docker,remote` flag (see
//! [`crate::cli::scan_cmd::ImageSource`]).
//!
//! Implementation: shell out to the `docker` CLI. We could talk to
//! `/var/run/docker.sock` directly, but `docker save` / `docker image
//! inspect` is what users already have configured (DOCKER_HOST,
//! DOCKER_TLS_VERIFY, contexts) and avoids adding a daemon-API client.
//! If `docker` is not on `$PATH`, callers fall through to the next
//! configured source.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

/// Outcome of probing the local docker daemon for a given image
/// reference.
#[derive(Debug, PartialEq, Eq)]
pub enum InspectOutcome {
    /// `docker image inspect <ref>` succeeded — the image is cached
    /// locally and can be exported via [`save`].
    Present,
    /// `docker` ran but the image is not in the local cache (exit !=
    /// 0 from `docker image inspect`).
    Absent,
    /// `docker` is not available — either the binary isn't on `$PATH`
    /// or the daemon is unreachable. Callers treat this the same as
    /// `Absent` for routing purposes; the variant exists so
    /// diagnostic logs can distinguish "daemon down" from "image not
    /// cached".
    DockerUnavailable,
}

/// Check whether `image_ref` is present in the local docker daemon's
/// cache. Pure read; never modifies state.
///
/// Returns:
/// - `Present` if `docker image inspect <ref>` exits 0
/// - `Absent` if the command runs but exits non-zero (e.g.
///   "No such image: ...")
/// - `DockerUnavailable` if the `docker` binary can't be spawned (not
///   installed, or `$DOCKER_HOST` points at a dead daemon and the
///   command fails before exec).
pub fn inspect(image_ref: &str) -> InspectOutcome {
    let mut cmd = Command::new("docker");
    cmd.args(["image", "inspect", image_ref])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match cmd.status() {
        Ok(status) if status.success() => InspectOutcome::Present,
        Ok(_) => InspectOutcome::Absent,
        Err(e) => {
            tracing::debug!(
                image_ref,
                error = %e,
                "docker image inspect could not be spawned; skipping local-daemon source"
            );
            InspectOutcome::DockerUnavailable
        }
    }
}

/// Export `image_ref` from the local docker daemon to a `docker
/// save`-format tarball at `dest`. Caller is responsible for the
/// destination path's lifetime (typically a `tempfile::TempDir`).
///
/// Errors: `docker save` writes to stderr on failure; we capture and
/// surface it. If `docker` isn't available the error mentions both
/// possibilities (not installed / daemon unreachable) so the user
/// can diagnose.
pub fn save(image_ref: &str, dest: &Path) -> Result<()> {
    let dest_arg = dest
        .to_str()
        .context("docker save destination path is not valid UTF-8")?;
    let output = Command::new("docker")
        .args(["save", image_ref, "-o", dest_arg])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| {
            format!(
                "spawning `docker save {image_ref} -o {dest_arg}` (is the `docker` \
                 CLI installed and is the daemon reachable?)"
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        bail!(
            "docker save {image_ref} failed with status {}: {}",
            output.status,
            stderr.trim()
        );
    }
    Ok(())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Whether the test host has `docker` available AND the daemon
    /// is responsive. Tests that need a running daemon skip
    /// themselves when this is false so CI lanes without docker
    /// (the default lane) don't fail.
    ///
    /// Uses `docker info` (not `docker --version`) so an installed-
    /// CLI-but-dead-daemon configuration — observed on the
    /// `--features ebpf-tracing` lane intermittently — is detected
    /// and the dependent tests skip. `docker info` returns non-zero
    /// when the daemon socket is unreachable; `docker --version`
    /// only checks the CLI binary.
    fn docker_available() -> bool {
        Command::new("docker")
            .arg("info")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    // Milestone 100: `#[cfg(unix)]` — the PATH-override mechanism is
    // POSIX-flavored (`/this/path/does/not/exist` doesn't behave the
    // same on Windows; the daemon `inspect` flow itself is Linux-only
    // anyway since Docker on Windows uses Windows-named-pipes / WSL2).
    #[cfg(unix)]
    #[test]
    fn inspect_returns_unavailable_when_docker_not_on_path() {
        // We can't easily simulate "docker missing" because the
        // current process's $PATH may very well contain it. Instead,
        // exercise the spawn-error path by overriding $PATH for this
        // test only.
        //
        // SAFETY: Rust `set_var` is single-threaded-only as of edition
        // 2024; in our test crate the runner spawns one thread per
        // test. Cargo serializes env mutations within a single
        // process otherwise. This test only mutates $PATH and
        // restores it, so concurrent tests reading $PATH could see a
        // transient empty value. None of the other tests in this
        // file read $PATH, so the race window is unobservable in
        // practice.
        let saved_path = std::env::var_os("PATH");
        // SAFETY: see comment above.
        unsafe {
            std::env::set_var("PATH", "/this/path/does/not/exist");
        }
        let outcome = inspect("alpine:3.19");
        // SAFETY: see comment above.
        unsafe {
            match saved_path {
                Some(p) => std::env::set_var("PATH", p),
                None => std::env::remove_var("PATH"),
            }
        }
        assert_eq!(outcome, InspectOutcome::DockerUnavailable);
    }

    #[test]
    fn inspect_returns_absent_for_image_not_in_local_cache() {
        if !docker_available() {
            eprintln!("skipping: `docker --version` failed; daemon not available");
            return;
        }
        // A reference that cannot exist anywhere — random suffix in
        // the tag space.
        let outcome =
            inspect("registry.invalid.mikebom-test.example/no-such-image:nope-d9f4b2");
        assert_eq!(outcome, InspectOutcome::Absent);
    }

    #[test]
    fn save_propagates_stderr_on_unknown_image() {
        if !docker_available() {
            eprintln!("skipping: `docker --version` failed; daemon not available");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("image.tar");
        let err = save(
            "registry.invalid.mikebom-test.example/no-such-image:nope-d9f4b2",
            &dest,
        )
        .unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("docker save") && msg.contains("failed"),
            "expected error to mention `docker save` failure, got: {msg}"
        );
    }
}
