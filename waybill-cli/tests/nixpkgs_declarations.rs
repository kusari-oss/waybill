//! Milestone 1050 — the paths that do not need a declaration.
//!
//! Statement and annotation *shapes* are unit-tested next to the code that
//! builds them, because no fixture can drive a declaration: waybill resolves
//! them against plain nixpkgs at the pinned revision, so a fixture's own
//! packages resolve to nothing (research R8). What a fixture *can* prove is
//! that a scan completes, degrades with a reason, emits nothing when the
//! flag is absent, and never reaches for an advisory database.

use std::path::PathBuf;
use std::process::Command;

use tempfile::tempdir;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix_declarations")
}

fn waybill_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_waybill"))
}

/// `nix` is not present on every runner, and this milestone's paths need it.
/// Skipping is honest; pretending to have tested them is not.
fn nix_available() -> bool {
    Command::new("nix")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Returns (emitted CDX bytes, combined stderr+stdout).
fn run_scan(extra: &[&str]) -> (Vec<u8>, String) {
    let out_dir = tempdir().expect("output tempdir");
    let home = tempdir().expect("home tempdir");
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(waybill_bin());
    cmd.env_remove("HOME")
        .env_remove("XDG_CACHE_HOME")
        .env("HOME", home.path())
        .env("WAYBILL_FIXTURES_DIR", env!("WAYBILL_FIXTURES_DIR"))
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture())
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&out_path)
        // The only external enrichers. Disabled so a completed scan proves
        // the feature needs no advisory or registry lookup of its own.
        .arg("--no-deps-dev")
        .arg("--no-clearly-defined")
        .args(extra);
    let output = cmd.output().expect("waybill invokes");
    assert!(
        output.status.success(),
        "waybill failed (extra={extra:?}): {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&out_path).expect("read emitted CDX");
    let mut combined = String::from_utf8_lossy(&output.stderr).to_string();
    combined.push('\n');
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    (bytes, combined)
}

fn parse(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).expect("emitted CDX parses")
}

fn property_names(cdx: &serde_json::Value) -> Vec<String> {
    let doc: Vec<String> = cdx["metadata"]["properties"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| p["name"].as_str())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let per_component = cdx["components"]
        .as_array()
        .map(|a| {
            a.iter()
                .flat_map(|c| c["properties"].as_array().cloned().unwrap_or_default())
                .filter_map(|p| p["name"].as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    doc.into_iter().chain(per_component).collect()
}

/// T043 / FR-020 — without the flag, nothing.
#[test]
fn without_nix_closure_no_declaration_output_appears() {
    let (bytes, logs) = run_scan(&[]);
    let names = property_names(&parse(&bytes));
    for n in &names {
        assert!(
            !n.starts_with("waybill:nixpkgs-declaration")
                && !n.starts_with("waybill:nixpkgs-accepted")
                && !n.starts_with("waybill:nixpkgs-security"),
            "declaration output leaked into a flag-off scan: {n}"
        );
    }
    assert!(
        !logs.contains("nixpkgs-declarations:"),
        "the pass ran without being asked:\n{logs}"
    );
}

/// T042 / FR-019 — with the flag, the scan completes and says what happened.
#[test]
fn with_nix_closure_the_pass_runs_or_degrades_but_never_fails_the_scan() {
    if !nix_available() {
        eprintln!("skipping: no `nix` on PATH");
        return;
    }
    let (bytes, logs) = run_scan(&["--nix-closure"]);
    // CONTROL: a document was produced at all.
    assert!(
        parse(&bytes)["components"].is_array(),
        "no components array in the emitted document"
    );
    // Either it ran or it degraded, and either way it said so. Silence would
    // leave a reader unable to tell "nixpkgs said nothing" from "nobody asked".
    assert!(
        logs.contains("nixpkgs-declarations:"),
        "the pass neither ran nor reported degrading:\n{logs}"
    );
}

/// T027 / FR-017, SC-008 — no vulnerability arrays in any emitted SBOM.
///
/// The golden check cannot catch this: goldens are generated with the flag
/// off, and the requirement is about the flag-on path.
#[test]
fn no_emitted_sbom_gains_a_vulnerabilities_array() {
    if !nix_available() {
        eprintln!("skipping: no `nix` on PATH");
        return;
    }
    let (bytes, _) = run_scan(&["--nix-closure"]);
    let cdx = parse(&bytes);
    // The key itself is part of the CycloneDX shape and predates this
    // milestone; what must stay empty is its contents. An entry here would
    // be a vulnerability claim in a composition snapshot.
    let entries = cdx["vulnerabilities"].as_array().map(Vec::len).unwrap_or(0);
    assert_eq!(
        entries, 0,
        "this milestone added {entries} vulnerability entries to an SBOM; \
         those claims belong in VEX, where they can be superseded without \
         rewriting the composition"
    );
}

/// T048 — the document-scope signals reach every format, or none does.
///
/// Computing a value into the summary does not emit it: per-component
/// annotations ride the `extra_annotations` pass-through, document-scope ones
/// need explicit emission in each of the three emitter files. This asserts
/// the three formats agree with each other about whether the pass ran, which
/// is the property that breaks when one emitter is forgotten.
#[test]
fn the_three_formats_agree_about_whether_the_pass_ran() {
    if !nix_available() {
        eprintln!("skipping: no `nix` on PATH");
        return;
    }
    let out_dir = tempdir().expect("output tempdir");
    let home = tempdir().expect("home tempdir");
    let mut seen = Vec::new();
    for (fmt, name) in [
        ("cyclonedx-json", "cdx.json"),
        ("spdx-2.3-json", "spdx23.json"),
        ("spdx-3-json", "spdx3.json"),
    ] {
        let path = out_dir.path().join(name);
        let out = Command::new(waybill_bin())
            .env_remove("HOME")
            .env("HOME", home.path())
            .env("WAYBILL_FIXTURES_DIR", env!("WAYBILL_FIXTURES_DIR"))
            .env("NO_COLOR", "1")
            .args(["sbom", "scan", "--path"])
            .arg(fixture())
            .args(["--format", fmt, "--output"])
            .arg(&path)
            .args(["--no-deps-dev", "--no-clearly-defined", "--nix-closure"])
            .output()
            .expect("waybill invokes");
        assert!(out.status.success(), "{fmt} scan failed");
        let text = std::fs::read_to_string(&path).expect("read output");
        seen.push((fmt, text.contains("waybill:nixpkgs-security")));
    }
    // CONTROL: all three scans produced a document.
    assert_eq!(seen.len(), 3);
    let first = seen[0].1;
    for (fmt, present) in &seen {
        assert_eq!(
            *present, first,
            "{fmt} disagrees with the others about whether the record was \
             emitted; a document-scope signal added to one emitter and \
             forgotten in another looks like this: {seen:?}"
        );
    }
}

/// T044 / FR-018, SC-009 — no advisory database is consulted.
///
/// Proven by exercising the path with every external enricher disabled and
/// the scan still completing, rather than by reading the source. Inspection
/// cannot prove a negative about code that has not run.
///
/// What this establishes: the feature needs no advisory lookup. It does not
/// establish that no byte ever leaves the host — `nix` itself fetches the
/// pinned nixpkgs, which is the same network the closure query already uses.
#[test]
fn the_feature_needs_no_advisory_database() {
    if !nix_available() {
        eprintln!("skipping: no `nix` on PATH");
        return;
    }
    let (bytes, logs) = run_scan(&["--nix-closure"]);
    assert!(parse(&bytes)["components"].is_array());
    for probe in ["osv.dev", "nvd.nist.gov", "api.osv", "cve.circl"] {
        assert!(
            !logs.contains(probe),
            "an advisory database was contacted ({probe}):\n{logs}"
        );
    }
}
