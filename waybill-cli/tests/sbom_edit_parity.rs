//! Milestone 1071 FR-013 — the same edit applied to the three formats of
//! one scan keeps every universal-parity catalogue row in agreement,
//! including C194 (`waybill:derivation`, by projection).

mod common;
mod sbom_edit_support;

use std::collections::BTreeSet;

use waybill::parity::{catalog, extractors};

use sbom_edit_support::*;

fn assert_parity(label: &str, cdx: &serde_json::Value, spdx23: &serde_json::Value, spdx3: &serde_json::Value) {
    let rows = catalog::parse_mapping_doc(&common::workspace_root().join("docs/reference/sbom-format-mapping.md"));
    let mut failures = Vec::new();
    for row in rows.iter().filter(|r| r.classification().is_universal_parity()) {
        let ex = extractors::EXTRACTORS
            .iter()
            .find(|e| e.row_id == row.id)
            .unwrap_or_else(|| panic!("row {} has no extractor", row.id));
        let (a, b, c): (BTreeSet<String>, _, _) = ((ex.cdx)(cdx), (ex.spdx23)(spdx23), (ex.spdx3)(spdx3));
        let ok = match ex.directional {
            extractors::Directionality::SymmetricEqual => a == b && b == c,
            extractors::Directionality::CdxSubsetOfSpdx => a.is_subset(&b) && a.is_subset(&c),
            extractors::Directionality::PresenceOnly => {
                (a.is_empty() && b.is_empty() && c.is_empty()) || (!a.is_empty() && !b.is_empty() && !c.is_empty())
            }
            extractors::Directionality::CdxOnly => true,
        };
        if !ok {
            failures.push(format!("{} ({})\n  CDX={a:?}\n  SPDX2.3={b:?}\n  SPDX3={c:?}", row.id, row.label));
        }
    }
    assert!(failures.is_empty(), "{label}: parity broken:\n{}", failures.join("\n"));
}

fn edit_all(args: &[&str]) -> Vec<serde_json::Value> {
    FORMATS.iter().map(|(_, f)| edit(&fixture(f), args).assert_ok().json()).collect()
}

#[test]
fn unedited_fixtures_agree() {
    let docs: Vec<_> = FORMATS.iter().map(|(_, f)| read_json(&fixture(f))).collect();
    assert_parity("unedited", &docs[0], &docs[1], &docs[2]);
}

#[test]
fn edited_outputs_agree_and_carry_the_same_derivation_projection() {
    let dir = tempfile::tempdir().unwrap();
    let key = write_key(dir.path(), "k", "parity-key");
    let key = key.to_str().unwrap();
    let docs = edit_all(&[
        "--drop", "scope=development",
        "--drop", "name=@acme/internal-utils",
        "--drop", "tier=file",
        "--drop-annotations", "waybill:cpe",
        "--redact", "paths",
        "--redact", "hosts=*.corp.acme.example",
        "--redact", "names=express",
        "--redact-key-file", key,
    ]);
    assert_parity("edited", &docs[0], &docs[1], &docs[2]);
    let c194 = extractors::EXTRACTORS.iter().find(|e| e.row_id == "C194").unwrap();
    let projection = (c194.cdx)(&docs[0]);
    assert_eq!(projection.len(), 1, "one derivation record");
}
