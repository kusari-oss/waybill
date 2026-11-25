//! Spec 1654 (#1695) — the one parity assertion integration tests share.
//!
//! Ten files asserted parity with private copies of a helper that read each
//! row's `universal_parity` field as the verdict. That field is the catalog's
//! expectation (should the row appear in every format), so the assertion
//! could not fail: removing a row from one format left it `true`. Since spec
//! 1654 every row carries `verdict` (`pass`, `fail`, `not-checked`), computed
//! by the same rule that counts `summary.parity_gaps`.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// Run `waybill sbom parity-check --format json` over the three documents in
/// `out_dir` (`waybill.cdx.json`, `waybill.spdx.json`, `waybill.spdx3.json`)
/// and assert every format agrees:
///
/// - no row has `verdict == "fail"`;
/// - `summary.parity_gaps == 0`;
/// - the number of `fail` rows equals `summary.parity_gaps`, so the per-row
///   field cannot drift from the count.
///
/// On failure it names each failing row with the three formats' presence and
/// value count.
pub fn assert_parity(out_dir: &Path) {
    let out = Command::new(super::bin())
        .args(["sbom", "parity-check", "--format", "json", "--scan-dir"])
        .arg(out_dir)
        .output()
        .unwrap_or_else(|e| panic!("parity-check did not run: {e}"));
    let report: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "parity-check output is not JSON ({e}); exit {:?}; stderr: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    let rows = report["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("parity-check JSON has no rows: {report}"));
    let failing: Vec<String> = rows
        .iter()
        .filter(|r| r["verdict"] == "fail")
        .map(|r| {
            let fmt = |f: &str| match r[f]["kind"].as_str() {
                Some("present") => format!("{f} {}", r[f]["value_count"]),
                Some(k) => format!("{f} {k}"),
                None => format!("{f} ?"),
            };
            format!(
                "{} ({}): {}, {}, {}",
                r["row_id"].as_str().unwrap_or("?"),
                r["label"].as_str().unwrap_or("?"),
                fmt("cdx"),
                fmt("spdx23"),
                fmt("spdx3")
            )
        })
        .collect();
    let gaps = report["summary"]["parity_gaps"]
        .as_u64()
        .unwrap_or_else(|| panic!("parity-check JSON has no summary.parity_gaps: {}", report["summary"]));
    assert_eq!(
        failing.len() as u64,
        gaps,
        "the per-row verdicts and summary.parity_gaps disagree: {failing:#?} vs {gaps}"
    );
    assert!(
        failing.is_empty(),
        "{} parity gap(s) in {}:\n  {}",
        failing.len(),
        out_dir.display(),
        failing.join("\n  ")
    );
}
