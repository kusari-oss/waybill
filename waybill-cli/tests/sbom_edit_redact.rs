//! Milestone 1071 US2 — `waybill sbom edit --redact`, against the
//! `tests/fixtures/sbom_edit/` scan outputs. The original values are the
//! ones the fixture README lists.

mod common;
mod sbom_edit_support;

use std::collections::BTreeSet;

use regex::Regex;
use serde_json::Value;

use sbom_edit_support::*;

/// Every form the fixture's internal values take in the three outputs.
const ORIGINALS: &[&str] = &[
    "corp.acme.example",
    "@acme/internal-utils",
    "%40acme/internal-utils",
    "internal-utils",
    "src/index.js",
    "lib/pricing.js",
    "index.js",
    "pricing.js",
];

fn redact_all(file: &str, key: &std::path::Path) -> Edited {
    let key = key.to_str().unwrap();
    edit(
        &fixture(file),
        &[
            "--redact", "paths",
            "--redact", "hosts:pseudonymise=*.corp.acme.example",
            "--redact", "names:pseudonymise=@acme/*",
            "--redact-key-file", key,
        ],
    )
}

fn tokens(text: &str) -> BTreeSet<String> {
    let re = Regex::new(r"redacted-[a-z2-7]{16}").unwrap();
    re.find_iter(text).map(|m| m.as_str().to_string()).collect()
}

fn purls(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                match (k.as_str(), x) {
                    ("purl" | "software_packageUrl", Value::String(s)) => out.push(s.clone()),
                    ("referenceLocator", Value::String(s)) if s.starts_with("pkg:") => out.push(s.clone()),
                    _ => purls(x, out),
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| purls(x, out)),
        _ => {}
    }
}

/// SC-003: no original value survives, in any form.
#[test]
fn redaction_leaves_no_original_value() {
    let dir = tempfile::tempdir().unwrap();
    let key = write_key(dir.path(), "k", "fixture-key-one");
    for (label, file) in FORMATS {
        let input_text = std::fs::read_to_string(fixture(file)).unwrap();
        assert!(ORIGINALS.iter().all(|o| input_text.contains(o)), "{label}: fixture lacks an original value");
        let e = redact_all(file, &key);
        e.assert_ok();
        let text = e.text();
        for o in ORIGINALS {
            assert!(!text.contains(o), "{label}: `{o}` survived redaction");
        }
        let input = read_json(&fixture(file));
        let out = e.json();
        assert_conforms_like(&input, &out, label);
        if label == "spdx3" {
            spdx3_validate_or_skip(&e.path);
        }
        // Rewritten PURLs still parse, and identities stay distinct.
        let mut ps = Vec::new();
        purls(&out, &mut ps);
        assert!(!ps.is_empty(), "{label}");
        for p in &ps {
            assert!(waybill_common::types::purl::Purl::new(p).is_ok(), "{label}: `{p}` no longer parses as a PURL");
        }
        assert_eq!(components(&out).len(), components(&input).len(), "{label}: component count changed");
        let ids: BTreeSet<String> = components(&out).into_iter().map(|(i, _)| i).collect();
        assert_eq!(ids.len(), components(&out).len(), "{label}: identities collided");
    }
}

/// SC-008: the same key gives the same tokens in every document, a
/// different key gives none of them, and no token is an original.
#[test]
fn pseudonyms_are_deterministic_per_key() {
    let dir = tempfile::tempdir().unwrap();
    let k1 = write_key(dir.path(), "k1", "fixture-key-one\n");
    let k2 = write_key(dir.path(), "k2", "fixture-key-two");
    let a = tokens(&redact_all("full.cdx.json", &k1).assert_ok().text());
    let b = tokens(&redact_all("full.spdx.json", &k1).assert_ok().text());
    let c = tokens(&redact_all("full.spdx3.json", &k1).assert_ok().text());
    let other = tokens(&redact_all("full.cdx.json", &k2).assert_ok().text());
    assert!(!a.is_empty());
    assert_eq!(a, b, "CycloneDX and SPDX 2.3 tokens differ under one key");
    assert_eq!(a, c, "CycloneDX and SPDX 3 tokens differ under one key");
    assert!(a.is_disjoint(&other), "a different key reproduced tokens");
    assert!(a.iter().all(|t| !ORIGINALS.iter().any(|o| t.contains(o))));
}

#[test]
fn names_remove_gives_distinct_ordinals() {
    for (label, file) in FORMATS {
        let out = edit(&fixture(file), &["--redact", "names:remove=re:^(@acme/internal-utils|lodash)$"]).assert_ok().json();
        let ns = names(&out);
        assert!(ns.contains("redacted-1") && ns.contains("redacted-2"), "{label}: {ns:?}");
        assert!(!ns.contains("lodash") && !ns.contains("@acme/internal-utils"), "{label}");
    }
}

#[test]
fn hosts_remove_uses_an_invalid_tld() {
    let text = edit(&fixture("full.cdx.json"), &["--redact", "hosts:remove=*.corp.acme.example"]).assert_ok().text();
    assert!(text.contains("redacted-host-1.invalid"), "no `.invalid` host marker");
}

#[test]
fn pseudonymising_without_a_key_is_an_error_and_writes_nothing() {
    for (label, file) in FORMATS {
        let e = edit(&fixture(file), &["--redact", "hosts=*.corp.acme.example"]);
        assert!(!e.status.success(), "{label}: succeeded without a key");
        assert!(e.stderr.contains("--redact-key-file"), "{label}: {}", e.stderr);
        assert!(!e.path.exists(), "{label}: output written");
    }
}

/// Redacted values never reach the derivation record: it records counts.
#[test]
fn the_derivation_record_carries_counts_not_values() {
    let dir = tempfile::tempdir().unwrap();
    let key = write_key(dir.path(), "k", "fixture-key-one");
    for (label, file) in FORMATS {
        let out = redact_all(file, &key).assert_ok().json();
        let record = derivation(&out).unwrap_or_else(|| panic!("{label}: no derivation record"));
        let categories: Vec<&str> = record["operations"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|o| o["category"].as_str())
            .collect();
        assert_eq!(categories, ["redact-paths", "redact-hosts", "redact-names"], "{label}");
        let text = record.to_string();
        assert!(ORIGINALS.iter().all(|o| !text.contains(o)), "{label}: a value in the record");
    }
}

/// Paths carried as evidence survive `--drop-annotations waybill:` (in
/// CycloneDX natively, in SPDX as an `evidence.occurrences` annotation), so
/// a later `--redact paths` must find them in every format, and the three
/// derivation records must count the same.
#[test]
fn paths_in_evidence_are_redacted_alike_in_every_format() {
    let mut counts = Vec::new();
    for (label, file) in FORMATS {
        let e = edit(&fixture(file), &["--drop-annotations", "waybill:", "--redact", "paths"]);
        e.assert_ok();
        let text = e.text();
        for p in ["package-lock.json", "src/index.js", "lib/pricing.js"] {
            assert!(!text.contains(p), "{label}: `{p}` survived");
        }
        let line = e.stderr.lines().find(|l| l.starts_with("redact-paths")).unwrap_or_default().to_string();
        counts.push(line);
    }
    assert!(counts.iter().all(|c| *c == counts[0] && !c.ends_with("matched 0, changed 0")), "{counts:?}");
}
