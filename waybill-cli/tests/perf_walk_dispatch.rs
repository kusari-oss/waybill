//! T025 scaffolding (superseded by T066) + T066 full SC-005 test + T034
//! US1 ansible wall-time gate.
//!
//! SC-005 microbenchmark: p95 per-file dispatch overhead ≤ 100 µs across
//! a 10,000-file synthetic tree with realistic manifest/noise ratio.
//!
//! Runs unprivileged on macOS + Linux CI lanes (SC-005 constraint).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::process::Command;
use std::time::{Duration, Instant};

/// T066 (US3, SC-005): full 10,000-file synthetic tree with ~5%
/// manifest-matching filenames. Warm-cache methodology (first pass
/// primes OS page cache; subsequent passes are the measurement).
/// Take p95 across 5 warm samples, divide by file count → assert per-
/// file dispatch overhead ≤ 100 µs.
///
/// Env-gated on `WAYBILL_PERF_TEST_ENABLED=1` to keep default `cargo
/// test` runs fast (tree construction + subprocess spawn is ~seconds).
/// CI can opt-in once release-mode timing anchors are set in a
/// follow-up.
///
/// Run locally:
/// ```sh
/// WAYBILL_PERF_TEST_ENABLED=1 cargo test --release --test perf_walk_dispatch -- sc005
/// ```
#[test]
fn sc005_synthetic_10k_file_tree_p95_dispatch_overhead() {
    if std::env::var_os("WAYBILL_PERF_TEST_ENABLED").is_none() {
        eprintln!(
            "sc005_synthetic_10k_file_tree_p95_dispatch_overhead: skipping — \
             set WAYBILL_PERF_TEST_ENABLED=1 to enable.",
        );
        return;
    }

    // Warn (not fail) if this appears to be a debug build. The 100 µs
    // per-file bound is anchored on release-mode.
    let bin_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_waybill"));
    let is_release = bin_path
        .components()
        .any(|c| c.as_os_str() == "release");
    if !is_release {
        eprintln!(
            "sc005: WARNING — binary at {:?} appears to be a debug build. \
             p95 target ≤ 100 µs/file is anchored on release-mode. \
             Re-run with `cargo test --release ...` for a meaningful measurement.",
            bin_path,
        );
    }

    let tmpdir = tempfile::tempdir().unwrap();
    let root = tmpdir.path();

    // ---- Tree construction ---------------------------------------
    // 10,000 files across 100 subdirs × 100 files each. ~5%
    // manifest-matching (5 per subdir) spread across multiple
    // ecosystems so the shared walker's dispatch table exercises a
    // realistic mix of registrations.
    const SUBDIRS: usize = 100;
    const FILES_PER_SUBDIR: usize = 100;
    const TOTAL_FILES: usize = SUBDIRS * FILES_PER_SUBDIR; // 10,000
    // ~5% manifest-matching: 5 per subdir (files 0-4). Rotate across
    // 5 ecosystems so each reader's on_file callback gets exercised.
    const MANIFEST_NAMES: &[&str] = &[
        "Cargo.toml",
        "go.mod",
        "pom.xml",
        "package.json",
        "requirements.txt",
    ];
    for subdir_ix in 0..SUBDIRS {
        let sub = root.join(format!("sub{:03}", subdir_ix));
        std::fs::create_dir(&sub).unwrap();
        for (file_ix, name) in (0..FILES_PER_SUBDIR).map(|i| {
            if i < MANIFEST_NAMES.len() {
                (i, MANIFEST_NAMES[i].to_string())
            } else {
                (i, format!("noise{:03}.txt", i))
            }
        }) {
            let _ = file_ix;
            std::fs::write(sub.join(name), b"x").unwrap();
        }
    }
    let manifest_count = SUBDIRS * MANIFEST_NAMES.len();
    let manifest_ratio = (manifest_count as f64) / (TOTAL_FILES as f64) * 100.0;
    eprintln!(
        "sc005: built {}-file synthetic tree ({} manifests, {:.1}% ratio) at {:?}",
        TOTAL_FILES, manifest_count, manifest_ratio, root,
    );

    // ---- Warm-cache methodology + p95 sample loop ----------------
    // Run the scan once to warm the OS page cache, then 5 more times
    // as measurement samples. p95 across the 5 samples = the 5th
    // (largest) — 5-sample p95 collapses to `max` per the standard
    // percentile convention. This is deliberate: we want the WORST-
    // case warm-cache overhead as our gate, not the median.
    let run_scan = |sample_ix: usize| -> Duration {
        let out_json = tmpdir.path().join(format!("out.sample{}.cdx.json", sample_ix));
        let start = Instant::now();
        let status = Command::new(&bin_path)
            .args([
                "sbom",
                "scan",
                "--offline",
                "--file-inventory=off",
                "--path",
            ])
            .arg(root)
            .arg("--format")
            .arg("cyclonedx-json")
            .arg("--output")
            .arg(&out_json)
            .status()
            .unwrap();
        let elapsed = start.elapsed();
        assert!(
            status.success(),
            "waybill scan failed on sample {} of synthetic 10k-file tree",
            sample_ix,
        );
        elapsed
    };

    let cold_elapsed = run_scan(0); // discard (warms the cache)
    let mut warm_samples: Vec<Duration> = Vec::with_capacity(5);
    for i in 1..=5 {
        warm_samples.push(run_scan(i));
    }
    warm_samples.sort();
    // 5-sample p95 = max sample (ceiling on the empirical warm-cache
    // distribution; matches SC-005's "worst-case" spirit).
    let p95 = *warm_samples.last().unwrap();
    let p50 = warm_samples[2];

    let per_file_p95_ns = p95.as_nanos() / (TOTAL_FILES as u128);
    let per_file_p95_us = (per_file_p95_ns as f64) / 1000.0;

    eprintln!(
        "sc005: cold={:?} warm-samples={:?} warm-p50={:?} warm-p95={:?} \
         per-file-p95={:.1} µs (target ≤ 100 µs)",
        cold_elapsed, warm_samples, p50, p95, per_file_p95_us,
    );

    // ---- Assertion ------------------------------------------------
    // 100 µs × 10k files = 1 second total warm-cache scan budget.
    let target_us = 100.0;
    assert!(
        per_file_p95_us <= target_us,
        "SC-005 regression: p95 per-file dispatch overhead is {:.1} µs, \
         target ≤ {:.1} µs. Warm p95 wall-time was {:?} across {} files. \
         Check whether: (a) a new reader's on_file/on_dir callback is \
         doing too much work (should be < 10 µs per invocation); \
         (b) a duplicate walker is running (was `run_shared_walker_pilot` \
         called twice?); (c) the shared walker's dir_index is doing \
         extra syscalls (see contract C6 in specs/664/contracts/registry-api.md).",
        per_file_p95_us, target_us, p95, TOTAL_FILES,
    );
}

/// T034 — US1 acceptance scenario 2 (audit-revised threshold).
///
/// Empirically validates the ≤ 3.5s partial-improvement target on the
/// ansible baseline (m664 diagnostic measured 4.10s pre-milestone).
/// The audit-revised target reflects the coexistence-window tax
/// (FR-004) — the headline SC-001 (≤ 1.2s) is verified in T060a
/// post-US2 once every walker-using reader has migrated.
///
/// Env-gated on `WAYBILL_PERF_ANSIBLE_DIR` so this only runs when the
/// fixture is available. CI skip if unset (matches m196 corpus-goldens
/// skip pattern per project memory `reference_public_corpus_fixtures`).
///
/// Run locally:
/// ```sh
/// git clone --depth=1 https://github.com/ansible/ansible.git /tmp/ansible
/// WAYBILL_PERF_ANSIBLE_DIR=/tmp/ansible cargo test --release --test perf_walk_dispatch -- us1_ansible_wall_time
/// ```
///
/// Note the `--release` — the wall-clock target is anchored on release-
/// mode measurements per spec.md Assumptions ("reference dev environment
/// is macOS APFS release-mode with warm caches"). A debug build will
/// substantially exceed the 3.5s bound; the test warns + skips if it
/// detects a debug-mode binary invocation.
#[test]
fn us1_ansible_wall_time() {
    let Some(ansible_dir) = std::env::var_os("WAYBILL_PERF_ANSIBLE_DIR") else {
        eprintln!(
            "us1_ansible_wall_time: skipping — set WAYBILL_PERF_ANSIBLE_DIR to \
             an ansible checkout (git clone --depth=1 ansible/ansible) to enable.",
        );
        return;
    };
    let ansible_path = std::path::PathBuf::from(&ansible_dir);
    if !ansible_path.is_dir() {
        panic!(
            "WAYBILL_PERF_ANSIBLE_DIR is set to {:?} but that path is not a directory",
            ansible_dir,
        );
    }

    let bin_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_waybill"));
    // Detect debug-mode build via path suffix. Not authoritative but
    // catches the common case of forgetting `--release`.
    let is_release = bin_path
        .components()
        .any(|c| c.as_os_str() == "release");
    if !is_release {
        eprintln!(
            "us1_ansible_wall_time: skipping — binary at {:?} appears to be a \
             debug build. Re-run with `cargo test --release --test perf_walk_dispatch \
             -- us1_ansible_wall_time` to measure the audit-anchored ≤ 3.5s target.",
            bin_path,
        );
        return;
    }

    let tmpdir = tempfile::tempdir().unwrap();
    let out_json = tmpdir.path().join("out.cdx.json");

    // Warm-cache methodology: run the scan TWICE and measure the second.
    // The first run primes the OS page cache with the full tree traversal
    // (much more thorough than any pre-pass could be). Per spec.md
    // Assumptions, US1 perf targets are anchored on warm-cache
    // measurements. Cold-cache first-run timing is recorded for
    // diagnostic purposes but not asserted against.
    let run_scan = || -> std::time::Duration {
        let start = Instant::now();
        let status = Command::new(&bin_path)
            .args([
                "sbom",
                "scan",
                "--offline",
                "--file-inventory=off",
                "--path",
            ])
            .arg(&ansible_path)
            .arg("--format")
            .arg("cyclonedx-json")
            .arg("--output")
            .arg(&out_json)
            .status()
            .unwrap();
        let elapsed = start.elapsed();
        assert!(
            status.success(),
            "waybill sbom scan failed against ansible checkout at {:?}",
            ansible_path,
        );
        elapsed
    };
    let cold_elapsed = run_scan();
    let warm_elapsed = run_scan();

    eprintln!(
        "us1_ansible_wall_time: ansible checkout at {:?} — \
         cold={:?} warm={:?} (target ≤ 3.5s on warm run, per audit-revised US1 threshold)",
        ansible_path, cold_elapsed, warm_elapsed,
    );

    // The audit-revised US1 target. Baseline 4.10s; net improvement
    // after ~120ms shared-walker tax with all 5 pilot readers migrated
    // to the consolidated walker (T033) → ≈ 3.43s on warm-cache.
    let target = std::time::Duration::from_millis(3_500);
    assert!(
        warm_elapsed <= target,
        "US1 warm-cache wall-time regression: ansible scan took {:?}, target ≤ 3.5s. \
         Baseline was 4.10s; the T033-consolidated shared walker should have \
         landed at ~3.43s. Check whether: (a) a non-migrated reader has \
         regressed, (b) the shared walker is running twice (someone reintroduced \
         a build_and_run call in read_all?), (c) the OS is under load. \
         Cold-cache reading was {:?} for reference.",
        warm_elapsed, cold_elapsed,
    );
}

/// T060 — US2 SC-002 empirical wall-time gate.
///
/// Validates the SC-002 headline target (≤ 1.5s) on the pytorch
/// checkout post-US2 migration. Baseline 4.30s; net improvement ≥
/// 2.8× per spec.md SC-002.
///
/// Env-gated on `WAYBILL_PERF_PYTORCH_DIR` — CI skip if unset (matches
/// the us1_ansible_wall_time pattern + m196 corpus-goldens skip pattern
/// per project memory `reference_public_corpus_fixtures`).
///
/// Run locally:
/// ```sh
/// git clone --depth=1 https://github.com/pytorch/pytorch.git /tmp/pytorch
/// WAYBILL_PERF_PYTORCH_DIR=/tmp/pytorch cargo test --release --test perf_walk_dispatch -- us2_pytorch_wall_time
/// ```
///
/// Note the `--release` — the wall-clock target is anchored on release-
/// mode measurements per spec.md Assumptions. A debug build will
/// substantially exceed the 1.5s bound; the test warns + skips if it
/// detects a debug-mode binary invocation.
///
/// **Placed in `perf_walk_dispatch.rs` (not `walk_registry_integration.rs`
/// as tasks.md T060 originally specified)** per T034's finding: the
/// m664 lib-boundary constraint prevents `walk_registry_integration.rs`
/// from spawning the binary. Same collocation as `us1_ansible_wall_time`.
#[test]
fn us2_pytorch_wall_time() {
    let Some(pytorch_dir) = std::env::var_os("WAYBILL_PERF_PYTORCH_DIR") else {
        eprintln!(
            "us2_pytorch_wall_time: skipping — set WAYBILL_PERF_PYTORCH_DIR to \
             a pytorch checkout (git clone --depth=1 pytorch/pytorch) to enable.",
        );
        return;
    };
    let pytorch_path = std::path::PathBuf::from(&pytorch_dir);
    if !pytorch_path.is_dir() {
        panic!(
            "WAYBILL_PERF_PYTORCH_DIR is set to {:?} but that path is not a directory",
            pytorch_dir,
        );
    }

    let bin_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_waybill"));
    let is_release = bin_path
        .components()
        .any(|c| c.as_os_str() == "release");
    if !is_release {
        eprintln!(
            "us2_pytorch_wall_time: skipping — binary at {:?} appears to be a \
             debug build. Re-run with `cargo test --release --test perf_walk_dispatch \
             -- us2_pytorch_wall_time` to measure the SC-002 ≤ 1.5s target.",
            bin_path,
        );
        return;
    }

    let tmpdir = tempfile::tempdir().unwrap();
    let out_json = tmpdir.path().join("out.cdx.json");

    // Warm-cache methodology — same as us1_ansible_wall_time.
    let run_scan = || -> std::time::Duration {
        let start = Instant::now();
        let status = Command::new(&bin_path)
            .args([
                "sbom",
                "scan",
                "--offline",
                "--file-inventory=off",
                "--path",
            ])
            .arg(&pytorch_path)
            .arg("--format")
            .arg("cyclonedx-json")
            .arg("--output")
            .arg(&out_json)
            .status()
            .unwrap();
        let elapsed = start.elapsed();
        assert!(
            status.success(),
            "waybill sbom scan failed against pytorch checkout at {:?}",
            pytorch_path,
        );
        elapsed
    };
    let cold_elapsed = run_scan();
    let warm_elapsed = run_scan();

    eprintln!(
        "us2_pytorch_wall_time: pytorch checkout at {:?} — \
         cold={:?} warm={:?} (target ≤ 1.5s on warm run, per SC-002)",
        pytorch_path, cold_elapsed, warm_elapsed,
    );

    // SC-002: pytorch baseline 4.30s → target ≤ 1.5s (≥ 2.8× improvement).
    let target = Duration::from_millis(1_500);
    assert!(
        warm_elapsed <= target,
        "SC-002 warm-cache wall-time regression: pytorch scan took {:?}, target ≤ 1.5s. \
         Baseline was 4.30s; every US2-migrated reader should have consolidated \
         into the shared walker → ≈ 1.5s on warm-cache. Check whether: (a) a \
         reader silently reintroduced an independent safe_walk call site, \
         (b) the shared walker is running twice (someone reintroduced a \
         build_and_run call in read_all?), (c) a US2 reader migration got \
         reverted, (d) the OS is under load. \
         Cold-cache reading was {:?} for reference.",
        warm_elapsed, cold_elapsed,
    );
}

/// T061 — US2 SC-003 empirical wall-time gate.
///
/// Validates the SC-003 headline target (≤ 3.0s) on the mongo
/// checkout post-US2 migration. Baseline 15.68s; net improvement ≥
/// 5× per spec.md SC-003. Mongo is the largest fixture in the
/// SC-001/002/003 set (55,186 files) and the most sensitive to any
/// per-file dispatch regression.
///
/// Env-gated on `WAYBILL_PERF_MONGO_DIR` — CI skip if unset.
///
/// Run locally:
/// ```sh
/// git clone --depth=1 https://github.com/mongodb/mongo.git /tmp/mongo
/// WAYBILL_PERF_MONGO_DIR=/tmp/mongo cargo test --release --test perf_walk_dispatch -- us2_mongo_wall_time
/// ```
///
/// **Placed in `perf_walk_dispatch.rs` (not `walk_registry_integration.rs`
/// as tasks.md T061 originally specified)** per T034's finding: the
/// m664 lib-boundary constraint prevents `walk_registry_integration.rs`
/// from spawning the binary.
#[test]
fn us2_mongo_wall_time() {
    let Some(mongo_dir) = std::env::var_os("WAYBILL_PERF_MONGO_DIR") else {
        eprintln!(
            "us2_mongo_wall_time: skipping — set WAYBILL_PERF_MONGO_DIR to \
             a mongo checkout (git clone --depth=1 mongodb/mongo) to enable.",
        );
        return;
    };
    let mongo_path = std::path::PathBuf::from(&mongo_dir);
    if !mongo_path.is_dir() {
        panic!(
            "WAYBILL_PERF_MONGO_DIR is set to {:?} but that path is not a directory",
            mongo_dir,
        );
    }

    let bin_path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_waybill"));
    let is_release = bin_path
        .components()
        .any(|c| c.as_os_str() == "release");
    if !is_release {
        eprintln!(
            "us2_mongo_wall_time: skipping — binary at {:?} appears to be a \
             debug build. Re-run with `cargo test --release --test perf_walk_dispatch \
             -- us2_mongo_wall_time` to measure the SC-003 ≤ 3.0s target.",
            bin_path,
        );
        return;
    }

    let tmpdir = tempfile::tempdir().unwrap();
    let out_json = tmpdir.path().join("out.cdx.json");

    // Warm-cache methodology — same as us1_ansible_wall_time.
    let run_scan = || -> std::time::Duration {
        let start = Instant::now();
        let status = Command::new(&bin_path)
            .args([
                "sbom",
                "scan",
                "--offline",
                "--file-inventory=off",
                "--path",
            ])
            .arg(&mongo_path)
            .arg("--format")
            .arg("cyclonedx-json")
            .arg("--output")
            .arg(&out_json)
            .status()
            .unwrap();
        let elapsed = start.elapsed();
        assert!(
            status.success(),
            "waybill sbom scan failed against mongo checkout at {:?}",
            mongo_path,
        );
        elapsed
    };
    let cold_elapsed = run_scan();
    let warm_elapsed = run_scan();

    eprintln!(
        "us2_mongo_wall_time: mongo checkout at {:?} — \
         cold={:?} warm={:?} (target ≤ 3.0s on warm run, per SC-003)",
        mongo_path, cold_elapsed, warm_elapsed,
    );

    // SC-003: mongo baseline 15.68s → target ≤ 3.0s (≥ 5× improvement).
    // Mongo is the most sensitive to per-file dispatch overhead because it
    // has ~55k files. A 100 µs regression per file adds 5.5s wall-time.
    let target = Duration::from_millis(3_000);
    assert!(
        warm_elapsed <= target,
        "SC-003 warm-cache wall-time regression: mongo scan took {:?}, target ≤ 3.0s. \
         Baseline was 15.68s; every US2-migrated reader should have consolidated \
         into the shared walker → ≤ 3.0s on warm-cache. Mongo (~55k files) is \
         the most sensitive fixture: a per-file dispatch regression of just \
         100 µs adds ~5.5s wall-time. Check whether: (a) a reader's on_file \
         callback started doing extra work (should be < 10 µs per invocation), \
         (b) a reader silently reintroduced an independent safe_walk call site, \
         (c) the shared walker is running twice, (d) contract C6 (zero-extra- \
         syscalls sibling lookup) got violated. Cold-cache reading was {:?} \
         for reference.",
        warm_elapsed, cold_elapsed,
    );
}
