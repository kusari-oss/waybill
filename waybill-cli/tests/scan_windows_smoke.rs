//! Milestone 101: Windows-host integration smoke test. Validates that
//! the locally-built `waybill.exe` (a) exits 0 against two cross-
//! platform fixtures, (b) emits well-formed CycloneDX 1.6 JSON,
//! (c) emits >= 1 component per expected ecosystem, (d) forward-slash-
//! normalizes path-shaped fields per milestone 100 Contract 3, and
//! (e) completes within 60 seconds per scan (hang regression guard).
//!
//! `#[cfg(windows)]`-gated at the file level: on Linux/macOS this
//! integration-test binary compiles to empty. The existing goldens
//! regression suite covers Unix forward-slash behavior; this file
//! is the Windows-specific gate per milestone 101's design.
//!
//! Failure-diagnostic policy (FR-012): on assertion failure, print
//! the first 10 emitted component PURLs + up to 5 offending
//! path-field name/value pairs inline, AND write the full emitted
//! SBOM to a per-test tempdir as `actual.cdx.json` with the absolute
//! path printed.

#![cfg(windows)]
#![allow(clippy::unwrap_used)]

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SCAN_TIMEOUT_SECS: u64 = 60;

/// Canonical list of path-shaped property/field names emitted by
/// milestone-100's normalization chokepoint + the 3 defensive
/// emission sites. Used by `walk_for_backslash_in_path_fields`
/// to scope the backslash check (FR-003) — broader scoping would
/// false-positive on CPE 2.3 escape sequences (the iteration-6
/// lesson from milestone 100).
const PATH_FIELD_NAMES: &[&str] = &[
    "waybill:source-files",
    "waybill:source-path",
    "location",
];

/// Run waybill.exe sbom scan with a hard 60-second timeout. Returns
/// (exit_status, elapsed). On timeout, kills the subprocess and panics with
/// the tail of its stderr, so the log says where the scan was (#1098: a
/// timeout that reported only "likely hang regression" could not be told
/// apart from slow network enrichment).
///
/// Polls rather than sleeping out the full timeout: the previous helper
/// joined a 60 s sleeper thread, so every case took a minute however fast
/// the scan was.
fn run_scan_with_timeout(
    input_path: &Path,
    output_path: &Path,
) -> (std::process::ExitStatus, Duration) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args([
            "sbom",
            "scan",
            "--path",
            input_path.to_str().expect("path utf-8"),
            "--output",
            output_path.to_str().expect("output utf-8"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn waybill.exe");

    // Drained on its own thread so a chatty scan can never block on a full
    // pipe while this thread waits for it to exit.
    // Collected line by line rather than read to EOF: a subprocess the scan
    // spawned (git, go) can outlive a killed scan while holding the inherited
    // stderr, so EOF may never come, and what was written before the timeout
    // is exactly the part worth printing.
    let stderr = child.stderr.take().expect("stderr is piped");
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = std::sync::Arc::clone(&log);
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stderr).lines().map_while(Result::ok) {
            sink.lock().unwrap().push(line);
        }
    });

    let start = Instant::now();
    let deadline = Duration::from_secs(SCAN_TIMEOUT_SECS);
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait waybill.exe") {
            break Some(status);
        }
        if start.elapsed() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let elapsed = start.elapsed();

    let Some(status) = status else {
        // Give the reader a moment to take lines already in the pipe.
        std::thread::sleep(Duration::from_millis(500));
        let lines = log.lock().unwrap().clone();
        let tail = &lines[lines.len().saturating_sub(40)..];
        panic!(
            "waybill.exe sbom scan timed out — likely hang regression \
             (elapsed: {elapsed:?}, fixture: {})\n--- last {} stderr lines ---\n{}",
            input_path.display(),
            tail.len(),
            tail.join("\n")
        );
    };
    (status, elapsed)
}

/// Recursive walk over the SBOM JSON. Returns every (field-name,
/// offending-value) pair where a path-shaped field value contains
/// a backslash. Scoped to CDX `properties[]` shape (`{"name": ...,
/// "value": ...}`) AND direct-field shapes (`location` on
/// `evidence.occurrences[]`). Per FR-003 + research §4 + §7.
fn walk_for_backslash_in_path_fields(val: &serde_json::Value) -> Vec<(String, String)> {
    fn inner(val: &serde_json::Value, hits: &mut Vec<(String, String)>) {
        match val {
            serde_json::Value::Object(map) => {
                // CDX `properties[]` shape:
                //   { "name": "waybill:source-files", "value": "..." }
                if let (Some(name_str), Some(value)) =
                    (map.get("name").and_then(|n| n.as_str()), map.get("value"))
                {
                    if PATH_FIELD_NAMES.contains(&name_str) {
                        if let Some(value_str) = value.as_str() {
                            if value_str.contains('\\') {
                                hits.push((name_str.to_string(), value_str.to_string()));
                            }
                        }
                    }
                }
                // Direct-field shapes (e.g., "location" on CDX
                // evidence.occurrences[]).
                for (k, v) in map {
                    if PATH_FIELD_NAMES.contains(&k.as_str()) {
                        if let Some(value_str) = v.as_str() {
                            if value_str.contains('\\') {
                                hits.push((k.clone(), value_str.to_string()));
                            }
                        }
                    }
                    inner(v, hits);
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr {
                    inner(v, hits);
                }
            }
            _ => {}
        }
    }
    let mut hits = Vec::new();
    inner(val, &mut hits);
    hits
}

/// On assertion failure, write the emitted SBOM to a per-test tempdir
/// as `actual.cdx.json` and print first 10 component PURLs + offending
/// fields inline. FR-012.
fn diagnose_and_panic(
    label: &str,
    sbom: &serde_json::Value,
    raw: &str,
    msg: String,
) -> ! {
    let tmp = std::env::temp_dir().join(format!(
        "waybill-smoke-{label}-{}.cdx.json",
        std::process::id()
    ));
    let _ = std::fs::write(&tmp, raw);
    eprintln!("\n--- SMOKE FAILURE [{label}] ---");
    eprintln!("{msg}");
    if let Some(comps) = sbom.get("components").and_then(|c| c.as_array()) {
        eprintln!("First 10 component PURLs:");
        for c in comps.iter().take(10) {
            let p = c
                .get("purl")
                .and_then(|p| p.as_str())
                .unwrap_or("<missing>");
            eprintln!("  {p}");
        }
    }
    eprintln!("Full SBOM written to: {}", tmp.display());
    eprintln!("--- end smoke failure ---\n");
    panic!("smoke test [{label}] failed");
}

fn run_smoke_case(label: &str, fixture_subpath: &str, expected_purl_prefixes: &[&str]) {
    let fixtures_root = PathBuf::from(env!("WAYBILL_FIXTURES_DIR"));
    let input = fixtures_root.join(fixture_subpath);
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("out.cdx.json");

    let (status, elapsed) = run_scan_with_timeout(&input, &output);
    eprintln!("[smoke:{label}] scan completed in {elapsed:?}");
    assert!(
        status.success(),
        "smoke [{label}]: waybill.exe exited non-zero ({status:?})"
    );

    let raw = std::fs::read_to_string(&output).expect("read emitted SBOM");
    let sbom: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => diagnose_and_panic(
            label,
            &serde_json::Value::Null,
            &raw,
            format!("malformed JSON: {e}"),
        ),
    };

    // Envelope: CycloneDX 1.6.
    if sbom.get("bomFormat").and_then(|v| v.as_str()) != Some("CycloneDX") {
        diagnose_and_panic(label, &sbom, &raw, "bomFormat != CycloneDX".to_string());
    }
    if sbom.get("specVersion").and_then(|v| v.as_str()) != Some("1.6") {
        diagnose_and_panic(label, &sbom, &raw, "specVersion != 1.6".to_string());
    }

    // >= 1 component per expected prefix.
    let comps = sbom
        .get("components")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    if comps.is_empty() {
        diagnose_and_panic(label, &sbom, &raw, "components[] empty".to_string());
    }
    for prefix in expected_purl_prefixes {
        let matched = comps.iter().any(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .map(|p| p.starts_with(prefix))
                .unwrap_or(false)
        });
        if !matched {
            diagnose_and_panic(
                label,
                &sbom,
                &raw,
                format!("no component with PURL prefix {prefix}"),
            );
        }
    }

    // FR-003: no backslashes in path-shaped fields.
    let bs_hits = walk_for_backslash_in_path_fields(&sbom);
    if !bs_hits.is_empty() {
        let summary = bs_hits
            .iter()
            .take(5)
            .map(|(name, value)| format!("    {name} = {value}"))
            .collect::<Vec<_>>()
            .join("\n");
        diagnose_and_panic(
            label,
            &sbom,
            &raw,
            format!(
                "found {} path-shaped field value(s) with backslash separators \
                 (milestone-100 normalization regression):\n{summary}",
                bs_hits.len()
            ),
        );
    }
}

#[test]
fn smoke_cargo_fixture() {
    run_smoke_case("cargo", "cargo/lockfile-v3", &["pkg:cargo/"]);
}

#[test]
fn smoke_polyglot_monorepo() {
    run_smoke_case(
        "polyglot",
        "polyglot-monorepo",
        &["pkg:pypi/", "pkg:npm/"],
    );
}
