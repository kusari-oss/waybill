// build.rs combines:
//   - eBPF bytecode-build coordination (existing)
//   - Milestone 090 — fixture-repo fetch (new)
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

use std::path::PathBuf;
use std::process::Command;

const FIXTURE_REPO_URL: &str = "https://github.com/kusari-sandbox/mikebom-test-fixtures.git";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    fetch_fixtures();
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

    let cache_parent = std::env::var("MIKEBOM_FIXTURE_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| {
                std::env::var("USERPROFILE").expect("HOME or USERPROFILE must be set")
            });
            PathBuf::from(home)
                .join(".cache")
                .join("mikebom")
                .join("fixtures")
        });
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

    // Cache miss — clone + pin to exact SHA.
    std::fs::create_dir_all(&cache_parent).unwrap_or_else(|e| {
        panic!(
            "\nfailed to create cache parent {}: {}\n",
            cache_parent.display(),
            e,
        )
    });

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
