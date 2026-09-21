//! Milestone 924 (#932) — US1 census tests.
//!
//! The census is what makes the whole document trustworthy: a reader who
//! cannot see the repository can still check FR-003's reconciliation and know
//! whether the report accounts for everything it walked.
//!
//! Fixtures use synthetic package names (`waybill-fixture-*`) throughout —
//! real coordinates trip advisory scanning in CI.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn w(dir: &Path, rel: &str, body: &[u8]) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

/// A repository containing every category the census must account for.
fn fixture() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();

    // (a) claimed — a well-formed cargo manifest
    w(r, "svc/Cargo.toml",
      b"[package]\nname = \"waybill-fixture-svc\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
        [dependencies]\nwaybill-fixture-dep = \"1.0.0\"\n");
    w(r, "svc/src/lib.rs", b"// fixture\n");

    // (b) claimed but yields nothing — a manifest a reader matches and cannot parse (FR-004)
    w(r, "broken/Cargo.toml", b"this is not valid toml at all {{{\n");

    // (c) unclaimed — files no reader has a pattern for
    for i in 0..4 {
        w(r, &format!("notes/note{i}.txt"), b"plain text\n");
    }

    // (d) excluded by policy — named via --exclude-path at invocation (FR-005)
    w(r, "scratch/Cargo.toml",
      b"[package]\nname = \"waybill-fixture-scratch\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");

    d
}

fn run_report(root: &Path, extra: &[&str]) -> serde_json::Value {
    let out = root.join("report.json");
    let mut args: Vec<&str> = vec![
        "repo", "report", "--path", root.to_str().unwrap(),
        "--output", out.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    let st = Command::new(binary_path()).args(&args).status().unwrap();
    assert!(st.success(), "`waybill repo report` failed: {st:?} args={args:?}");
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

/// FR-003 / SC-003 — the invariant that makes the census checkable without
/// access to the repository being described.
///
/// FR-005 rides along: an excluded-by-policy directory must be reported
/// distinctly from an unrecognised one. The two demand opposite responses —
/// "you told me to skip this" versus "I did not understand this" — and nothing
/// else in the suite covers the distinction.
#[test]
fn the_census_reconciles_exactly_m924() {
    let d = fixture();
    let rep = run_report(d.path(), &["--exclude-path", "scratch"]);

    let t = &rep["totals"];
    let walked = t["files_walked"].as_u64().unwrap();
    let claimed = t["files_claimed"].as_u64().unwrap();
    let unclaimed = t["files_unclaimed"].as_u64().unwrap();
    let skipped: u64 = t["files_skipped"].as_object().unwrap()
        .values().map(|v| v.as_u64().unwrap()).sum();

    assert_eq!(
        walked, claimed + unclaimed + skipped,
        "FR-003 reconciliation failed: walked={walked} claimed={claimed} \
         unclaimed={unclaimed} skipped={skipped}. A report whose totals do not \
         reconcile is invalid, not merely imperfect.",
    );
    assert!(walked > 0, "the fixture must actually produce files to account for");

    // FR-005: excluded-by-policy is its own outcome, not folded into unclaimed.
    let has_excluded = rep["directories"].as_array().unwrap().iter()
        .any(|e| e["claim_status"] == "excluded_by_policy");
    let skipped_reasons = t["files_skipped"].as_object().unwrap();
    assert!(
        has_excluded || skipped_reasons.contains_key("excluded_by_policy"),
        "an excluded directory must be reported as excluded_by_policy, \
         distinctly from unrecognised; got directories={:?} skips={:?}",
        rep["directories"], skipped_reasons,
    );
}

/// FR-004 — a reader that matched files and produced nothing is a different
/// diagnosis from a reader that never matched. The first is a parse failure or
/// an unsupported dialect; the second is a coverage gap. A single number
/// conflates them, so the report carries both.
#[test]
fn a_reader_that_matched_but_emitted_nothing_is_distinguishable_m924() {
    let d = fixture();
    let rep = run_report(d.path(), &[]);

    let readers = rep["readers"].as_array().unwrap();
    assert!(!readers.is_empty(), "the fixture must engage at least one reader");

    let engaged_but_empty: Vec<_> = readers.iter()
        .filter(|r| r["files_matched"].as_u64().unwrap() > 0
                 && r["components_emitted"].as_u64().unwrap() == 0)
        .collect();
    let never_matched: Vec<_> = readers.iter()
        .filter(|r| r["files_matched"].as_u64().unwrap() == 0)
        .collect();

    assert!(
        !engaged_but_empty.is_empty() || !never_matched.is_empty(),
        "both FR-004 categories are absent, so the distinction is untested; \
         readers={readers:?}",
    );
    for r in readers {
        assert!(r["files_matched"].is_u64() && r["components_emitted"].is_u64(),
            "every reader must carry BOTH counts — one alone cannot express \
             the distinction: {r:?}");
    }
}

/// SC-013 — aggregation loses records without losing counts (FR-021b).
#[test]
fn aggregated_directories_keep_their_counts_m924() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    w(r, "top/Cargo.toml",
      b"[package]\nname = \"waybill-fixture-top\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    // A deep tree of small, unmarked, unclaimed directories: below the
    // significance threshold at every level, so none earns a record.
    for i in 0..6 {
        w(r, &format!("deep/a/b/c/d/e/f{i}.txt"), b"x\n");
    }

    let rep = run_report(r, &[]);
    let t = &rep["totals"];
    let walked = t["files_walked"].as_u64().unwrap();
    let claimed = t["files_claimed"].as_u64().unwrap();
    let unclaimed = t["files_unclaimed"].as_u64().unwrap();
    let skipped: u64 = t["files_skipped"].as_object().unwrap()
        .values().map(|v| v.as_u64().unwrap()).sum();

    assert_eq!(walked, claimed + unclaimed + skipped,
        "reconciliation must survive aggregation");
    assert!(
        t["directories_recorded"].as_u64().unwrap() < t["directories_walked"].as_u64().unwrap(),
        "the deep unmarked tree should have been aggregated, not recorded: \
         recorded={} walked={}",
        t["directories_recorded"], t["directories_walked"],
    );
    assert!(unclaimed >= 6, "the aggregated files must still be counted: {unclaimed}");
}

/// SC-007 / FR-022b — the report path makes no network request.
///
/// **This fixture is deliberately hostile.** An earlier version of this test
/// used a repository with no Go module at all and passed without exercising
/// anything: the network-capable path is the Go transitive resolver, and it
/// never ran. A later version added a Go module that happened to be in the
/// local module cache, so the resolver short-circuited before the proxy tier
/// and the test passed for the second wrong reason.
///
/// This one names a module that cannot be cached and points `GOMODCACHE` at an
/// empty directory, so the proxy tier is the only remaining option. Measured
/// against a control: `sbom scan` on the same fixture attempts the fetch and
/// fails with `connection refused` after ~5s; the report path reaches the
/// gosum tier with `proxy_count=0` in ~0.04s.
#[test]
fn the_report_makes_no_network_request_m924() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    w(r, "go.mod",
      b"module example.com/waybill-fixture-uncached\n\ngo 1.21\n\n        require github.com/waybill-fixture/definitely-not-cached v1.2.3\n");
    w(r, "go.sum",
      b"github.com/waybill-fixture/definitely-not-cached v1.2.3         h1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\n");
    w(r, "main.go", b"package main\n");

    let empty_cache = d.path().join("empty-modcache");
    std::fs::create_dir_all(&empty_cache).unwrap();
    let out = r.join("offline.json");

    let started = std::time::Instant::now();
    let st = Command::new(binary_path())
        .args(["repo", "report", "--path", r.to_str().unwrap(),
               "--output", out.to_str().unwrap()])
        .env("GOPROXY", "http://127.0.0.1:1")
        .env("GOMODCACHE", empty_cache.to_str().unwrap())
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .status()
        .unwrap();
    let elapsed = started.elapsed();

    assert!(st.success(), "the report path failed with the network unreachable: {st:?}");
    assert!(
        elapsed < std::time::Duration::from_secs(3),
        "the report took {elapsed:?}. A run that long against an unroutable \
         proxy means it attempted a fetch and waited for the failure -- the \
         control (`sbom scan`) takes ~5s on this fixture for exactly that \
         reason, and the report path should take ~0.04s.",
    );
    let rep: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert!(rep["totals"]["files_walked"].as_u64().unwrap() > 0);
}

/// FR-021c — the threshold that governed record-or-aggregate is stated, so two
/// reports are never compared without knowing whether they are comparable.
#[test]
fn the_report_states_its_significance_threshold_m924() {
    let d = fixture();
    let rep = run_report(d.path(), &[]);
    assert!(
        rep["significance_threshold"].as_u64().is_some(),
        "significance_threshold must be present: {rep}",
    );
    assert_eq!(rep["schema_stability"], "alpha");
    assert!(rep["schema_version"]["major"].as_u64().is_some());
    assert!(rep["volatile_fields"].as_array().unwrap().iter().any(|v| v == "generated_at"));
}
