//! Issue #1444 — a relationship two edge sources both produce is emitted once,
//! in every format.
//!
//! On moat (`--nix-closure`) SPDX 2.3 carried 331 exact-duplicate
//! relationships, and the root's out-edges disagreed: CycloneDX 28, SPDX 2.3
//! 40, SPDX 3 28. Every duplicate was a milestone-1280 `nix-build-graph` edge
//! landing on a pair another source had already contributed — the pinned
//! package set's declared edges (m1275), the m985 runtime closure, or the
//! project's own manifest. CycloneDX collapses them into a `dependsOn` set and
//! SPDX 3 groups targets into a set; SPDX 2.3 wrote one row per edge.
//!
//! The fixture reproduces the manifest-overlap case, which is the one that
//! reaches the root: a Cargo project whose `Cargo.lock` records
//! `app -> lib`, `app -> devlib` (a dev-dependency) and `lib -> leaf`, and
//! whose stubbed closure records the same three edges in `inputs.drvs`. The
//! closure members merge into the Cargo components on `(name, version)`, so
//! both sources name the same PURL pairs.
//!
//! Every assertion is on the emitted documents, through a stub `nix`.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::nix_stub::{NixStub, Rows, Scan, DEFAULT_ATTR};
use serde_json::Value;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix_closure_overlapping_edges")
}

fn scan(extra: &[&str]) -> Scan {
    let closure = std::fs::read_to_string(fixture_dir().join("closure.json")).unwrap();
    let mut args = vec!["--no-deps-dev"];
    args.extend_from_slice(extra);
    NixStub {
        project: fixture_dir().join("project"),
        closure: &closure,
        rows: Rows::Json("[]"),
        attr: DEFAULT_ATTR,
        ..Default::default()
    }
    .scan(&args)
}

fn closure_scan() -> Scan {
    scan(&["--nix-closure"])
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The `relationships=N` the scan reports for its internal edge list, which is
/// counted before any emitter deduplicates.
fn internal_relationship_count(s: &Scan) -> usize {
    let text = strip_ansi(&s.stderr);
    let line = text
        .lines()
        .find(|l| l.contains("SBOM written"))
        .unwrap_or_else(|| panic!("no `SBOM written` line in stderr:\n{text}"));
    line.split_whitespace()
        .find_map(|w| w.strip_prefix("relationships="))
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("no relationships= field in: {line}"))
}

fn spdx23_triples(doc: &Value) -> Vec<(String, String, String)> {
    doc["relationships"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["spdxElementId"].as_str().unwrap().to_string(),
                r["relationshipType"].as_str().unwrap().to_string(),
                r["relatedSpdxElement"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// SPDX 2.3 dependency relationships as `(dependent, dependency, type)`,
/// undoing the m228 reversal of the `*_DEPENDENCY_OF` verbs.
fn spdx23_dependencies(doc: &Value) -> Vec<(String, String, String)> {
    spdx23_triples(doc)
        .into_iter()
        .filter_map(|(source, kind, target)| match kind.as_str() {
            "DEPENDS_ON" => Some((source, target, kind)),
            k if k.ends_with("_DEPENDENCY_OF") => Some((target, source, kind)),
            _ => None,
        })
        .collect()
}

/// Root out-edges as `[cdx, spdx-2.3, spdx-3]`, each a list of target names so
/// the three are comparable and a doubled target shows up twice.
fn root_out_edges(s: &Scan) -> [Vec<String>; 3] {
    // CycloneDX: the root's `dependsOn`, resolved to component names.
    let root_ref = s.cdx["metadata"]["component"]["bom-ref"].as_str().unwrap();
    let cdx_name = |r: &str| -> String {
        s.cdx["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["bom-ref"].as_str() == Some(r))
            .and_then(|c| c["name"].as_str())
            .unwrap_or(r)
            .to_string()
    };
    let mut cdx: Vec<String> = s.cdx["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["ref"].as_str() == Some(root_ref))
        .flat_map(|d| d["dependsOn"].as_array().cloned().unwrap_or_default())
        .map(|t| cdx_name(t.as_str().unwrap()))
        .collect();

    // SPDX 2.3: dependencies whose dependent is the described package.
    let described = s.spdx23["documentDescribes"][0].as_str().unwrap();
    let spdx23_name = |id: &str| -> String {
        s.spdx23["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["SPDXID"].as_str() == Some(id))
            .and_then(|p| p["name"].as_str())
            .unwrap_or(id)
            .to_string()
    };
    let mut spdx23: Vec<String> = spdx23_dependencies(&s.spdx23)
        .into_iter()
        .filter(|(from, _, _)| from == described)
        .map(|(_, to, _)| spdx23_name(&to))
        .collect();

    // SPDX 3: every `dependsOn` target from the root element.
    let graph = s.spdx3["@graph"].as_array().unwrap();
    let root3 = graph
        .iter()
        .find(|e| e["type"] == "SpdxDocument")
        .and_then(|d| d["rootElement"][0].as_str())
        .unwrap();
    let spdx3_name = |iri: &str| -> String {
        graph
            .iter()
            .find(|e| e["spdxId"].as_str() == Some(iri))
            .and_then(|e| e["name"].as_str())
            .unwrap_or(iri)
            .to_string()
    };
    let mut spdx3: Vec<String> = graph
        .iter()
        .filter(|e| e["relationshipType"] == "dependsOn" && e["from"].as_str() == Some(root3))
        .flat_map(|e| e["to"].as_array().cloned().unwrap_or_default())
        .filter_map(|t| t.as_str().map(str::to_string))
        .filter(|t| t != "NoAssertionElement")
        .map(|t| spdx3_name(&t))
        .collect();

    cdx.sort();
    spdx23.sort();
    spdx3.sort();
    [cdx, spdx23, spdx3]
}

/// The control the two tests below rest on: the overlap is real. Without the
/// closure the Cargo reader alone yields three dependency edges; with it the
/// scan's internal list holds six, and the documents still describe the same
/// three pairs. If the closure stopped contributing, or stopped landing on the
/// Cargo PURLs, this fails rather than letting the dedup tests pass vacuously.
#[test]
fn the_closure_contributes_the_same_pairs_the_cargo_reader_does() {
    let without = scan(&[]);
    let with = closure_scan();
    assert_eq!(internal_relationship_count(&without), 3, "{}", without.stderr);
    assert_eq!(
        internal_relationship_count(&with),
        6,
        "the closure's three edges must arrive alongside the Cargo reader's three\n{}",
        with.stderr
    );
    let pairs = |s: &Scan| -> BTreeSet<(String, String)> {
        spdx23_dependencies(&s.spdx23)
            .into_iter()
            .map(|(a, b, _)| (a, b))
            .collect()
    };
    assert_eq!(pairs(&without).len(), 3);
    assert_eq!(pairs(&without), pairs(&with));
}

/// #1444 — no SPDX 2.3 relationship appears twice, including the
/// `DEV_DEPENDENCY_OF` the dev-dependency is typed as after both copies of the
/// edge were retyped from its target's lifecycle scope.
#[test]
fn spdx23_emits_each_relationship_once() {
    let s = closure_scan();
    let triples = spdx23_triples(&s.spdx23);
    let unique: BTreeSet<_> = triples.iter().cloned().collect();
    assert_eq!(
        triples.len(),
        unique.len(),
        "SPDX 2.3 relationships carry duplicates: {triples:#?}"
    );
    let deps = spdx23_dependencies(&s.spdx23);
    assert_eq!(deps.len(), 3, "{deps:#?}");
    assert!(
        deps.iter().any(|(_, _, k)| k == "DEV_DEPENDENCY_OF"),
        "the dev-dependency edge is typed: {deps:#?}"
    );
}

/// #1444 — the cross-format root-edge parity check: the root depends on the
/// same targets, once each, in all three formats.
#[test]
fn root_out_edges_agree_across_formats() {
    let s = closure_scan();
    let [cdx, spdx23, spdx3] = root_out_edges(&s);
    assert_eq!(
        cdx,
        vec!["waybill-fixture-devlib".to_string(), "waybill-fixture-lib".to_string()],
        "CycloneDX root out-edges"
    );
    assert_eq!(spdx23, cdx, "SPDX 2.3 root out-edges disagree with CycloneDX");
    assert_eq!(spdx3, cdx, "SPDX 3 root out-edges disagree with CycloneDX");
}

/// The whole dependency graph, not only the root's slice of it: the same edge
/// count in every format.
#[test]
fn dependency_edge_counts_agree_across_formats() {
    let s = closure_scan();
    let cdx = s.cdx["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["dependsOn"].as_array().map_or(0, Vec::len))
        .sum::<usize>();
    let spdx23 = spdx23_dependencies(&s.spdx23).len();
    let spdx3 = s.spdx3["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["relationshipType"] == "dependsOn")
        .flat_map(|e| e["to"].as_array().cloned().unwrap_or_default())
        .filter(|t| t != "NoAssertionElement")
        .count();
    assert_eq!(cdx, 3);
    assert_eq!(spdx23, cdx, "SPDX 2.3 vs CycloneDX");
    assert_eq!(spdx3, cdx, "SPDX 3 vs CycloneDX");
}
