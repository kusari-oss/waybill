//! Milestone 1272 T058–T060 — the optional axis's error and edge paths,
//! in all four dialects this feature touched.
//!
//! Contracts C8 and C9, FR-011, FR-012, and the spec's Edge Cases
//! section. Three questions, asked of every dialect:
//!
//! - **T058 / FR-012 / C8** — an optional declaration naming a package
//!   the lockfile does not lock. The axis must stay silent and no
//!   component may be fabricated. A classifier that synthesised one
//!   would publish a coordinate that does not exist, which is worse
//!   than omitting it: a consumer cannot tell an invented package from
//!   a real one.
//! - **T059 / FR-011 / C9** — a lockfile malformed in the
//!   axis-bearing region specifically. The axis must be left unset, the
//!   parse must not abort, and the reader must not guess. Note the
//!   shape of this test: it asserts the *other* components still
//!   arrive, because a reader that bailed on the whole file would also
//!   leave the axis unset and would otherwise look identical.
//! - **T060** — an optional declaration naming a non-registry target
//!   (`link:`, `file:`, `workspace:`). No registry identity may be
//!   invented, matching what the development axis already does.
//!
//! Fixtures are written at test time rather than committed. These are
//! deliberately malformed or degenerate inputs; inline is where they
//! are legible, and a committed directory of twelve broken lockfiles
//! invites being "fixed" by someone who does not know they are the
//! point.
//!
//! **Where these tests get their positive control.** T058 and T060 both
//! assert an ABSENCE, and an absence is also what you get from a
//! classifier that never read the optional declaration at all. The
//! control is not in this file: it is
//! `optional_dep_cross_dialect.rs`, which establishes on the same four
//! dialects that a root optional declaration DOES classify. Read
//! together, that pair says the machinery is live and declines on
//! purpose here. Read alone, this file cannot tell the difference, and
//! neither could a reader of it.
//!
//! All package names are synthetic per FR-015.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::process::Command;

/// Write a project into a fresh directory and scan it.
///
/// Returns `None` when the scan itself failed, which is a distinct
/// outcome from "scanned and said nothing" and must not be conflated
/// with it — FR-011 requires the parse to survive, so a non-zero exit
/// is a failure of the requirement, not a way of satisfying it.
fn scan_project(files: &[(&str, &str)]) -> Option<Value> {
    let dir = tempfile::tempdir().unwrap();
    for (name, body) in files {
        std::fs::write(dir.path().join(name), body).unwrap();
    }
    let out = dir.path().join("out.cdx.json");
    let result = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(dir.path())
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out.display()))
        .output()
        .unwrap();
    if !result.status.success() {
        eprintln!(
            "scan exited {:?}: {}",
            result.status.code(),
            String::from_utf8_lossy(&result.stderr)
        );
        return None;
    }
    Some(serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap())
}

fn names(cdx: &Value) -> Vec<String> {
    let mut v: Vec<String> = cdx
        .get("components")
        .and_then(|c| c.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// The `waybill:source-files` value for a named component, if any.
///
/// This exists because T059's central assertion -- that the parse
/// survived a malformed axis region -- is satisfiable VACUOUSLY: every
/// one of these fixtures also has a `package.json`, and the manifest
/// readers would happily emit the same component name if the lockfile
/// reader had bailed on the whole file. Checking which file the
/// surviving component was read FROM is what makes the test about the
/// lockfile reader rather than about its neighbour.
fn source_files_of(cdx: &Value, name: &str) -> Option<String> {
    cdx.get("components")
        .and_then(|c| c.as_array())?
        .iter()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(name))?
        .get("properties")
        .and_then(|p| p.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("waybill:source-files"))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
        .map(str::to_string)
}

/// The lockfile filename each dialect's malformed fixture is built on.
fn lockfile_name(dialect: &str) -> &'static str {
    match dialect {
        "pnpm" => "pnpm-lock.yaml",
        "bun" => "bun.lock",
        "yarn_v1" | "yarn_berry" => "yarn.lock",
        other => panic!("unknown dialect {other}"),
    }
}

fn optional_names(cdx: &Value) -> Vec<String> {
    let mut v: Vec<String> = cdx
        .get("components")
        .and_then(|c| c.as_array())
        .map(|a| {
            a.iter()
                .filter(|c| c.get("scope").and_then(|s| s.as_str()) == Some("excluded"))
                .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// The four dialects' lockfile shapes, parameterised over one optional
/// declaration name, so the three questions below can be asked of each
/// dialect from one table.
fn dialect_files(dialect: &str, declared_optional: &str, lock_valid: bool) -> Vec<(String, String)> {
    let manifest = format!(
        r#"{{
  "name": "waybill-fixture-errpath-{dialect}",
  "version": "1.0.0",
  "private": true,
  "dependencies": {{ "waybill-fixture-present": "1.0.0" }},
  "optionalDependencies": {{ "{declared_optional}": "9.9.9" }}
}}"#
    );
    let mut files = vec![("package.json".to_string(), manifest)];
    match dialect {
        "pnpm" => {
            let axis = if lock_valid {
                "  waybill-fixture-present@1.0.0: {}"
            } else {
                // Malformed in the axis-bearing region: `optional`
                // carries a mapping where a boolean belongs.
                "  waybill-fixture-present@1.0.0:\n    optional:\n      not: a-boolean"
            };
            files.push((
                "pnpm-lock.yaml".to_string(),
                format!(
                    "lockfileVersion: '9.0'\n\nimporters:\n\n  .:\n    dependencies:\n      \
                     waybill-fixture-present:\n        specifier: 1.0.0\n        version: 1.0.0\n\n\
                     packages:\n\n  waybill-fixture-present@1.0.0:\n    resolution: \
                     {{integrity: sha512-cHJlc2VudDEwMDBwcmVzZW50MTAwMHByZXNlbnQxMDAwcHJlc2VudDEwMDBwcmVzZW50MTAwMHByZXNlbnQxMDAwcHI=}}\n\n\
                     snapshots:\n\n{axis}\n"
                ),
            ));
        }
        "bun" => {
            let axis = if lock_valid {
                r#"{ "dependencies": {} }"#
            } else {
                // `dependencies` as an array where an object belongs.
                r#"{ "dependencies": ["not", "an", "object"] }"#
            };
            files.push((
                "bun.lock".to_string(),
                format!(
                    r#"{{
  "lockfileVersion": 2,
  "configVersion": 1,
  "workspaces": {{
    "": {{
      "name": "waybill-fixture-errpath-bun",
      "dependencies": {{ "waybill-fixture-present": "1.0.0" }},
      "optionalDependencies": {{ "{declared_optional}": "9.9.9" }}
    }}
  }},
  "packages": {{
    "waybill-fixture-present": ["waybill-fixture-present@1.0.0", "", {axis}, "sha512-cHJlc2VudDEwMDBwcmVzZW50MTAwMHByZXNlbnQxMDAwcHJlc2VudDEwMDBwcmVzZW50MTAwMHByZXNlbnQxMDAwcHJlc2U"]
  }}
}}"#
                ),
            ));
        }
        "yarn_v1" => {
            let axis = if lock_valid {
                ""
            } else {
                // A sub-block header with no children under it.
                "  optionalDependencies:\n"
            };
            files.push((
                "yarn.lock".to_string(),
                format!(
                    "# THIS IS AN AUTOGENERATED FILE. DO NOT EDIT THIS FILE DIRECTLY.\n\
                     # yarn lockfile v1\n\n\n\
                     \"waybill-fixture-present@1.0.0\":\n  version \"1.0.0\"\n{axis}"
                ),
            ));
        }
        "yarn_berry" => {
            let axis = if lock_valid {
                "  dependenciesMeta:\n    waybill-fixture-present:\n      optional: false\n"
            } else {
                // `dependenciesMeta` as a scalar where a mapping belongs.
                "  dependenciesMeta: \"not-a-mapping\"\n"
            };
            files.push((
                "yarn.lock".to_string(),
                format!(
                    "__metadata:\n  version: 8\n  cacheKey: 10c0\n\n\
                     \"waybill-fixture-present@npm:1.0.0\":\n  version: 1.0.0\n  \
                     resolution: \"waybill-fixture-present@npm:1.0.0\"\n  \
                     checksum: 10c0/1111000000000000000000000000000000000000000000000000000000000000\n  \
                     languageName: node\n  linkType: hard\n\n\
                     \"waybill-fixture-errpath-yarn_berry@workspace:.\":\n  \
                     version: 0.0.0-use.local\n  resolution: \
                     \"waybill-fixture-errpath-yarn_berry@workspace:.\"\n  \
                     dependencies:\n    waybill-fixture-present: \"npm:1.0.0\"\n{axis}  \
                     languageName: unknown\n  linkType: soft\n"
                ),
            ));
        }
        other => panic!("unknown dialect {other}"),
    }
    files
}

const DIALECTS: [&str; 4] = ["pnpm", "bun", "yarn_v1", "yarn_berry"];

fn as_pairs(owned: &[(String, String)]) -> Vec<(&str, &str)> {
    owned
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect()
}

#[test]
fn t058_unlocked_optional_declaration_fabricates_nothing() {
    // FR-012 / contract C8. The manifest declares
    // `waybill-fixture-unlocked` optional; no lockfile locks it.
    for dialect in DIALECTS {
        let owned = dialect_files(dialect, "waybill-fixture-unlocked", true);
        let cdx = scan_project(&as_pairs(&owned))
            .unwrap_or_else(|| panic!("{dialect}: scan MUST succeed (FR-011)"));
        let emitted = names(&cdx);
        assert!(
            !emitted.iter().any(|n| n == "waybill-fixture-unlocked"),
            "{dialect}: FR-012 — an optional declaration the lockfile does not \
             lock MUST NOT fabricate a component. Emitted: {emitted:?}"
        );
        // Control: the locked package IS emitted, so this cannot pass
        // by emitting nothing at all.
        assert!(
            emitted.iter().any(|n| n == "waybill-fixture-present"),
            "{dialect}: control — the locked package MUST still be emitted, \
             otherwise this test passes vacuously. Emitted: {emitted:?}"
        );
    }
}

#[test]
fn t059_malformed_axis_region_leaves_the_axis_unset() {
    // FR-011 / contract C9. Each dialect's lockfile is malformed in the
    // axis-bearing region *specifically* — the optional marker, the
    // optional sub-block, or dependenciesMeta — and nowhere else.
    for dialect in DIALECTS {
        let owned = dialect_files(dialect, "waybill-fixture-unlocked", false);
        let cdx = scan_project(&as_pairs(&owned)).unwrap_or_else(|| {
            panic!("{dialect}: FR-011 — a malformed axis region MUST NOT abort the scan")
        });
        let emitted = names(&cdx);
        // The load-bearing half: a reader that gave up on the whole
        // file would also leave the axis unset. Only the surviving
        // component distinguishes "degraded gracefully" from "bailed".
        assert!(
            emitted.iter().any(|n| n == "waybill-fixture-present"),
            "{dialect}: FR-011 — the parse MUST survive a malformed axis region \
             and still emit the rest of the lockfile. Emitted: {emitted:?}"
        );
        // ...and it must have come from the LOCKFILE. Without this the
        // assertion above passes when the lockfile reader bailed
        // entirely and the sibling package.json supplied the name.
        let lock = lockfile_name(dialect);
        let source = source_files_of(&cdx, "waybill-fixture-present");
        assert!(
            source.as_deref().is_some_and(|s| s.contains(lock)),
            "{dialect}: the surviving component must have been read from {lock}, \
             or this test is about the manifest reader and not the lockfile \
             reader at all. waybill:source-files = {source:?}"
        );
        // And it must not guess: nothing may be classified optional
        // off an unreadable marker.
        let optional = optional_names(&cdx);
        assert!(
            !optional.iter().any(|n| n == "waybill-fixture-present"),
            "{dialect}: FR-011 — the reader MUST NOT guess an axis value from an \
             unreadable region. Classified optional: {optional:?}"
        );
    }
}

#[test]
fn t060_non_registry_optional_target_invents_no_identity() {
    // Spec Edge Cases. An optional declaration whose target is a
    // non-registry protocol has no registry identity to resolve to, and
    // the development axis already declines to synthesise one (m1271
    // FR-012's skipped-declaration count exists for exactly this).
    for dialect in DIALECTS {
        for target in ["link:../sibling", "file:./vendor/thing", "workspace:*"] {
            let owned = dialect_files(dialect, "waybill-fixture-nonregistry", true);
            // Rewrite the optional declaration's SPEC to the
            // non-registry protocol. The name stays, so a reader that
            // keyed off the name alone would still emit it — which is
            // the failure this looks for.
            let patched: Vec<(String, String)> = owned
                .iter()
                .map(|(n, b)| (n.clone(), b.replace("\"9.9.9\"", &format!("\"{target}\""))))
                .collect();
            let cdx = scan_project(&as_pairs(&patched)).unwrap_or_else(|| {
                panic!("{dialect}/{target}: scan MUST succeed")
            });
            let emitted = names(&cdx);
            assert!(
                !emitted.iter().any(|n| n == "waybill-fixture-nonregistry"),
                "{dialect}/{target}: a non-registry optional target MUST NOT be \
                 given a registry identity. Emitted: {emitted:?}"
            );
            assert!(
                emitted.iter().any(|n| n == "waybill-fixture-present"),
                "{dialect}/{target}: control — the locked package MUST still be \
                 emitted. Emitted: {emitted:?}"
            );
        }
    }
}
