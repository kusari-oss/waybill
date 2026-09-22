//! Milestone 924 (#932) — US2: naming ecosystems waybill cannot read.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

fn binary_path() -> &'static str { env!("CARGO_BIN_EXE_waybill") }

fn w(dir: &Path, rel: &str, body: &[u8]) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn run_report(root: &Path) -> serde_json::Value {
    // Written OUTSIDE the scanned tree. A report placed inside it becomes a
    // file in the very directory being reported on: it inflates file counts,
    // can tip a directory over the significance threshold, and makes a second
    // run observe the first run's output. Found while investigating a census
    // failure that did not reproduce; the hazard is real whether or not it
    // caused that one.
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir()
        .join(format!("m924-{}-{}-{seq}.json", module_path!().replace("::", "_"), std::process::id()));
    let st = Command::new(binary_path())
        .args(["repo", "report", "--path", root.to_str().unwrap(),
               "--output", out.to_str().unwrap()])
        .status().unwrap();
    assert!(st.success(), "repo report failed: {st:?}");
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

/// Every marker the shipped table claims is unsupported.
fn table_entries() -> Vec<(String, String)> {
    include_str!("../src/report/ecosystems.data")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut p = l.split('\t');
            Some((p.next()?.trim().to_string(), p.next()?.trim().to_string()))
        })
        .collect()
}

/// **The anti-staleness guard** (research R6).
///
/// Every marker the table calls unsupported must actually be unclaimed by
/// every registered reader. When a reader lands for one of these ecosystems,
/// this test fails and the table entry must be deleted.
///
/// Without it the table rots silently: it keeps announcing an ecosystem as a
/// gap after the gap is closed, sending a maintainer to look for work that is
/// already done. This project has shipped that bug class before — an enum
/// allowlist that silently missed variants added later, undetected for roughly
/// two years — which is why this is a test and not a convention.
#[test]
fn no_table_entry_names_a_marker_a_reader_already_claims_m924() {
    let entries = table_entries();
    assert!(!entries.is_empty(), "the table must not be empty");

    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    // Each marker in its own directory, so a claim is attributable.
    for (i, (marker, _eco)) in entries.iter().enumerate() {
        w(root, &format!("e{i}/{marker}"), b"{}\n");
    }
    let rep = run_report(root);

    let mut stale = Vec::new();
    for (i, (marker, eco)) in entries.iter().enumerate() {
        let dir = format!("e{i}");
        if let Some(o) = rep["directories"].as_array().unwrap().iter()
            .find(|o| o["path"] == dir)
        {
            let claimed = o["claimed_by"].as_array().map(|a| !a.is_empty()).unwrap_or(false);
            if claimed {
                stale.push(format!("{marker} ({eco}) is claimed by {}", o["claimed_by"]));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "ecosystems.data is STALE — a reader now claims these markers, so the \
         table is telling maintainers a closed gap is still open. Delete the \
         offending lines:\n  {}",
        stale.join("\n  "),
    );
}

/// SC-010 — the positive case. Three unsupported ecosystems, all named.
///
/// T020's negative test alone would pass an implementation that names nothing
/// at all; this is what makes the pair meaningful.
#[test]
fn three_unsupported_ecosystems_are_all_named_m924() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    w(root, "tools/codegen/deno.json", b"{}\n");
    w(root, "analysis/Project.toml", b"name = \"X\"\n");
    w(root, "native/build.zig", b"// zig\n");

    let rep = run_report(root);
    let named: std::collections::BTreeMap<String, String> = rep["directories"].as_array().unwrap()
        .iter()
        .flat_map(|o| o["ecosystems"].as_array().cloned().unwrap_or_default()
            .into_iter()
            .map(move |e| (e["ecosystem"].as_str().unwrap_or("").to_string(),
                           e["support"].as_str().unwrap_or("").to_string())))
        .collect();

    for eco in ["deno", "julia", "zig"] {
        assert_eq!(
            named.get(eco).map(String::as_str), Some("no_reader"),
            "{eco} must be named with an explicit no-reader status; got {named:?}",
        );
    }
}

/// FR-008 — the negative case. Source files without a marker are never
/// attributed, however suggestive the extensions.
#[test]
fn source_files_without_a_marker_are_not_attributed_m924() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    for i in 0..5 { w(root, &format!("src/mod{i}.zig"), b"// zig source\n"); }

    let rep = run_report(root);
    let attributed: Vec<_> = rep["directories"].as_array().unwrap().iter()
        .flat_map(|o| o["ecosystems"].as_array().cloned().unwrap_or_default())
        .collect();
    assert!(
        attributed.is_empty(),
        "a directory of .zig files with no build.zig must stay unrecognised, \
         not be guessed from extensions (FR-008); got {attributed:?}",
    );
}

/// Spec US2 scenario 2 — a supported ecosystem that yielded nothing is a
/// different problem from one with no reader, and must not be merged with it.
#[test]
fn supported_and_unsupported_are_distinguishable_m924() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    w(root, "app/Cargo.toml",
      b"[package]\nname = \"waybill-fixture-app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    w(root, "tools/deno.json", b"{}\n");

    let rep = run_report(root);
    let mut supports = std::collections::BTreeMap::new();
    for o in rep["directories"].as_array().unwrap() {
        for e in o["ecosystems"].as_array().cloned().unwrap_or_default() {
            supports.insert(
                e["ecosystem"].as_str().unwrap_or("").to_string(),
                e["support"].as_str().unwrap_or("").to_string(),
            );
        }
    }
    assert_eq!(supports.get("cargo").map(String::as_str), Some("supported"),
        "a reader exists for cargo: {supports:?}");
    assert_eq!(supports.get("deno").map(String::as_str), Some("no_reader"),
        "no reader exists for deno: {supports:?}");
}
