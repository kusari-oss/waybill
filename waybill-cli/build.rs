// build.rs combines:
//   - eBPF bytecode-build coordination (existing)
//   - Milestone 090 — fixture-repo fetch (existing)
//   - Milestone 108 — fingerprint-corpus SHA pin (new)
//
// The eBPF side is mostly a no-op: bytecode is compiled via
// `cargo xtask ebpf` and included via include_bytes_aligned! in
// loader.rs.
//
// The fixture-fetch side reads the pinned `mikebom-test-fixtures`
// Git SHA from `<workspace>/tests/fixtures.rev`, ensures the fixture
// repo is cloned at that SHA into a per-host cache, and exposes the
// cache path to test code via the `MIKEBOM_FIXTURES_DIR` compile-time
// env var. Cache-warm builds skip the network entirely. See
// specs/090-split-test-fixtures-repo/contracts/fixture-path-helper.md.
//
// The fingerprint-corpus SHA pin reads `<workspace>/tests/fingerprints.rev`
// and emits it as the `MIKEBOM_FINGERPRINTS_CORPUS_SHA` compile-time
// env var consumed by the runtime corpus loader. Unlike fetch_fixtures,
// this build step does NOT touch the network — the runtime loader
// handles cache-miss fetch when `--fingerprints-corpus` is set at
// scan time. See specs/108-fingerprint-corpus/contracts/cache-layout.md.

use std::path::PathBuf;
use std::process::Command;

const FIXTURE_REPO_URL: &str = "https://github.com/kusari-sandbox/mikebom-test-fixtures.git";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    fetch_fixtures();
    emit_fingerprints_corpus_sha();
}

fn fetch_fixtures() {
    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by cargo"),
    );
    let workspace_root = manifest_dir
        .parent()
        .expect("CARGO_MANIFEST_DIR must have a parent (the workspace root)")
        .to_path_buf();
    let pin_path = workspace_root.join("tests").join("fixtures.rev");

    println!("cargo:rerun-if-changed={}", pin_path.display());
    println!("cargo:rerun-if-env-changed=MIKEBOM_FIXTURE_CACHE");

    let sha_raw = std::fs::read_to_string(&pin_path).unwrap_or_else(|e| {
        panic!(
            "\nfailed to read fixture-repo pin at {}: {}\n\nThis commit predates the milestone-090 fixture split, OR the pin\nfile is missing. Either:\n  1. Check out a post-090 mikebom revision that has tests/fixtures.rev, OR\n  2. Manually create the file with a 40-char hex SHA from\n     {url}\n",
            pin_path.display(),
            e,
            url = FIXTURE_REPO_URL,
        )
    });
    let sha = sha_raw.trim().to_string();
    let valid = sha.len() == 40
        && sha
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    if !valid {
        panic!("\ntests/fixtures.rev MUST be a 40-char lowercase hex SHA; got {sha:?}\n");
    }

    let cache_parent = resolve_cache_parent();
    let cache_target = cache_parent.join(&sha);

    if cache_target.exists()
        && std::fs::read_dir(&cache_target)
            .map(|d| d.count())
            .unwrap_or(0)
            > 0
    {
        // Cache hit — skip fetch.
        println!(
            "cargo:rustc-env=MIKEBOM_FIXTURES_DIR={}",
            cache_target.display()
        );
        return;
    }

    // Cache miss — clone + pin to exact SHA. `resolve_cache_parent`
    // already guarantees the parent directory exists.

    println!("cargo:warning=fetching mikebom-test-fixtures @ {sha} (one-time per pin)");

    let cache_target_str = cache_target
        .to_str()
        .expect("cache target path must be UTF-8");

    let clone_status = Command::new("git")
        .args(["clone", FIXTURE_REPO_URL, cache_target_str])
        .status();
    if !matches!(clone_status, Ok(s) if s.success()) {
        let _ = std::fs::remove_dir_all(&cache_target);
        panic!(
            "\nFailed to fetch mikebom-test-fixtures revision {sha}:\n    URL:   {url}\n    Cache: {cache}\n    Cause: git clone failed (status {clone_status:?})\n\nWorkaround:\n    1. Verify network access to github.com.\n    2. Manually clone:\n         git clone {url} {cache}\n         git -C {cache} reset --hard {sha}\n    3. Re-run cargo build.\n",
            url = FIXTURE_REPO_URL,
            cache = cache_target.display(),
        );
    }

    // Pin to exact SHA — clone defaults to default-branch HEAD; we
    // want the specific SHA from tests/fixtures.rev for reproducibility
    // across mikebom commits.
    let reset_status = Command::new("git")
        .args(["-C", cache_target_str, "reset", "--hard", &sha])
        .status();
    if !matches!(reset_status, Ok(s) if s.success()) {
        let _ = std::fs::remove_dir_all(&cache_target);
        panic!(
            "\nFailed to pin {cache} to revision {sha}:\n    Cause: git reset --hard failed (status {reset_status:?})\n\nThis usually means the SHA isn't reachable from the default branch.\nFix: verify the SHA exists in {url} and is on the\ndefault branch's history.\n",
            cache = cache_target.display(),
            url = FIXTURE_REPO_URL,
        );
    }

    println!(
        "cargo:rustc-env=MIKEBOM_FIXTURES_DIR={}",
        cache_target.display()
    );
}

/// Resolve a writable directory to host the fixture-repo cache.
///
/// Resolution order:
/// 1. `MIKEBOM_FIXTURE_CACHE` env var (explicit operator override).
/// 2. `$HOME/.cache/mikebom/fixtures/` on Unix /
///    `$USERPROFILE/.cache/mikebom/fixtures/` on Windows.
/// 3. `$OUT_DIR/mikebom-fixtures/` as a defensive fallback when (2)
///    isn't writable — cargo always sets `OUT_DIR` and guarantees it
///    is writable. Triggered in `cross` Docker containers where
///    `HOME=""` produces an unusable path like `/.cache/mikebom/...`
///    that the container's root filesystem rejects.
///
/// Returns a path whose parent directory already exists and is
/// writable, so callers can `clone`/`reset` into a subdirectory of it
/// without panicking on permission errors.
fn resolve_cache_parent() -> PathBuf {
    let preferred = std::env::var("MIKEBOM_FIXTURE_CACHE")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            let home = std::env::var("HOME")
                .ok()
                .or_else(|| std::env::var("USERPROFILE").ok())
                .filter(|s| !s.is_empty())?;
            Some(
                PathBuf::from(home)
                    .join(".cache")
                    .join("mikebom")
                    .join("fixtures"),
            )
        });

    if let Some(path) = preferred {
        if std::fs::create_dir_all(&path).is_ok() {
            return path;
        }
        println!(
            "cargo:warning=fixture cache parent {} not writable; falling back to $OUT_DIR/mikebom-fixtures/",
            path.display()
        );
    }

    // Fallback: $OUT_DIR is always set by cargo and is writable.
    let out_dir = std::env::var("OUT_DIR")
        .expect("cargo must set OUT_DIR in build.rs");
    let fallback = PathBuf::from(out_dir).join("mikebom-fixtures");
    std::fs::create_dir_all(&fallback)
        .expect("OUT_DIR-based fixture cache fallback must be writable");
    fallback
}

/// Milestone 108 — read the pinned `kusari-sandbox/mikebom-fingerprints`
/// SHA from `<workspace>/tests/fingerprints.rev` and emit it as the
/// `MIKEBOM_FINGERPRINTS_CORPUS_SHA` compile-time env var.
///
/// Unlike `fetch_fixtures`, this step does NOT touch the network. The
/// runtime corpus loader handles cache-miss fetch when an operator
/// passes `--fingerprints-corpus` at scan time. The build only emits
/// the pin so `env!("MIKEBOM_FINGERPRINTS_CORPUS_SHA")` resolves to
/// a known value in production code.
///
/// Validation: the file must contain a single 40-char lowercase hex
/// SHA (with optional trailing newline). Anything else panics the
/// build — the pin is reproducibility-critical and a malformed pin
/// would silently produce broken binaries.
fn emit_fingerprints_corpus_sha() {
    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by cargo"),
    );
    let workspace_root = manifest_dir
        .parent()
        .expect("CARGO_MANIFEST_DIR must have a parent (the workspace root)")
        .to_path_buf();
    let pin_path = workspace_root.join("tests").join("fingerprints.rev");

    println!("cargo:rerun-if-changed={}", pin_path.display());

    let sha_raw = std::fs::read_to_string(&pin_path).unwrap_or_else(|e| {
        panic!(
            "\nfailed to read fingerprint-corpus pin at {}: {}\n\nThis commit predates the milestone-108 fingerprint-corpus split,\nOR the pin file is missing. Either:\n  1. Check out a post-108 mikebom revision that has tests/fingerprints.rev, OR\n  2. Manually create the file with a 40-char hex SHA from\n     https://github.com/kusari-sandbox/mikebom-fingerprints/commits/main\n",
            pin_path.display(),
            e,
        )
    });
    let sha = sha_raw.trim().to_string();
    let valid = sha.len() == 40
        && sha
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    if !valid {
        panic!("\ntests/fingerprints.rev MUST be a 40-char lowercase hex SHA; got {sha:?}\n");
    }

    println!("cargo:rustc-env=MIKEBOM_FINGERPRINTS_CORPUS_SHA={sha}");
}
