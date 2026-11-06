//! `waybill debug closure-dump` — the reporter must not drift from the reader.
//!
//! This command exists because four wrong findings in one session all came
//! from answering "what does waybill see for this derivation?" with a model
//! of the reader rather than the reader. The tests below pin the two
//! distinctions those models got wrong; if the command ever stops reporting
//! them truthfully it is worse than not existing, because it would carry the
//! authority of the real code while giving a model's answer.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix_closure_debug/closure.json")
}

fn dump(extra: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args(["debug", "closure-dump", "--closure-json"])
        .arg(fixture())
        .args(extra)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "closure-dump failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// **The distinction that produced a phantom defect.** `pname` is read from
/// the flat `env` map *or* the typed `structuredAttrs` carrier. A probe that
/// consulted `env` alone called 66 of one real closure's 1,275 derivations
/// nameless and reported ten of them as a milestone-1282 gap; all ten declare
/// a `pname` in the typed carrier.
///
/// Asserting the per-carrier split rather than only the total is the point: a
/// reporter that read one carrier and called the other "none" would still
/// total correctly.
#[test]
fn the_pname_carrier_is_reported_per_derivation() {
    let d = dump(&["--summary-only"]);
    let c = &d["pname_carrier"];
    assert_eq!(c["env"], 2, "{c}");
    assert_eq!(c["structuredAttrs"], 2, "{c}");
    assert_eq!(c["none"], 1, "{c}");
}

/// A derivation declaring `pname` only in the typed carrier is **not** a
/// milestone-1282 member, and the dump must say so on the same row that
/// names the carrier — the two facts together are what settle "is this a
/// coverage gap?".
#[test]
fn a_typed_carrier_pname_is_not_a_1282_member() {
    let d = dump(&["--filter", "typed"]);
    let rows = d["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    let r = &rows[0];
    assert_eq!(r["declared_pname"], "waybill-fixture-typed");
    assert_eq!(r["pname_carrier"], "structuredAttrs");
    assert_eq!(r["member_role"], Value::Null);
    assert_eq!(r["identity_hash"], Value::Null);
}

/// **The distinction that made a 107-name gap evaporate.** nix names a
/// derivation `python3.13-urllib3`; waybill names the component by its
/// `pname`, `urllib3`. Comparing raw derivation names against another tool's
/// component names counts every such package as missing. The dump reports
/// both spellings on one row so the comparison cannot be made blind.
#[test]
fn the_derivation_name_and_the_component_name_are_both_reported() {
    let d = dump(&["--filter", "flat"]);
    let r = &d["rows"].as_array().unwrap()[0];
    assert_eq!(r["name"], "waybill-fixture-flat-2.0");
    assert_eq!(r["pname_with_fallback"], "waybill-fixture-flat");
    assert_ne!(
        r["name"], r["pname_with_fallback"],
        "the fixture must keep these different, or this asserts nothing"
    );
}

/// Emission is decided by `emits_as_component`, not by a `pname` test — the
/// fallback in `pname()` makes a `pname` test true for every member. The
/// dump reports the real predicate's answer.
#[test]
fn emission_is_reported_from_the_real_predicate() {
    let d = dump(&[]);
    let rows = d["rows"].as_array().unwrap();
    let src = rows
        .iter()
        .find(|r| r["name"] == "source")
        .expect("the source member");
    // pname-less, so milestone 1282 emits it and it carries an identity.
    assert_eq!(src["declared_pname"], Value::Null);
    assert_eq!(src["pname_carrier"], "none");
    assert_eq!(src["member_role"], "source");
    assert_eq!(src["emits_as_component"], true);
    assert!(src["identity_hash"].as_str().is_some_and(|h| !h.is_empty()));

    // A pname-bearing `unreferenced` derivation is suppressed, which is the
    // documented milestone-1282 exclusion rather than a gap.
    // `waybill-fixture-orphan` declares a pname in the typed carrier and
    // nothing references it, so it is suppressed. That is milestone 1282's
    // documented exclusion rather than a gap, and telling the two apart is
    // what this command is for.
    let orphan = rows
        .iter()
        .find(|r| r["name"] == "waybill-fixture-orphan-4.0")
        .unwrap();
    assert_eq!(orphan["declared_pname"], "waybill-fixture-orphan");
    assert_eq!(orphan["pname_carrier"], "structuredAttrs");
    assert_eq!(orphan["c182_role"], "unreferenced");
    assert_eq!(orphan["emits_as_component"], false);
    assert_eq!(orphan["member_role"], Value::Null);
}

/// The counts and the rows must describe the same run: a summary that
/// disagreed with its own rows would be the drift this command exists to
/// prevent, reintroduced inside it.
#[test]
fn the_summary_agrees_with_the_rows() {
    let d = dump(&[]);
    let rows = d["rows"].as_array().unwrap();
    let emitted = rows
        .iter()
        .filter(|r| r["emits_as_component"] == true)
        .count();
    assert_eq!(d["emitted_as_component"], emitted);
    let members = rows.iter().filter(|r| !r["member_role"].is_null()).count();
    assert_eq!(d["members_1282"], members);
    assert_eq!(d["derivations"], rows.len());
}
