// milestone 770 — T015/T016: sbomqs invocation.
//
// Deliberately UNLIKE waybill-cli/tests/sbomqs_parity.rs, which skips
// cleanly when sbomqs is absent. That test may skip because it is one
// signal among many; this command's entire purpose is the score, so a
// missing scorer fails the run (FR-016, Constitution III — a missing
// signal is never a passing signal).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Locate sbomqs: `WAYBILL_SBOMQS_BIN` then `$PATH`, mirroring
/// `sbomqs_parity.rs:33`.
pub fn locate() -> Option<PathBuf> {
    if let Ok(env) = std::env::var("WAYBILL_SBOMQS_BIN") {
        let p = PathBuf::from(&env);
        if p.exists() {
            return Some(p);
        }
    }
    Command::new("sbomqs")
        .arg("version")
        .output()
        .ok()
        .map(|_| PathBuf::from("sbomqs"))
}

/// Version string reported by the binary, e.g. `v2.0.6`. Best-effort: a
/// version we cannot parse is reported as unknown rather than fatal, since
/// the score itself is still usable.
pub fn version(bin: &Path) -> Option<String> {
    let out = Command::new(bin).arg("version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find_map(|l| l.trim().strip_prefix("GitVersion:"))
        .map(|v| v.trim().to_string())
}

/// Overall 0–10 score from `files[0].sbom_quality_score`.
///
/// NOTE: sbomqs counts the document's root component; the independent
/// package/file split in `analyze` does not. Both are correct; the report
/// records them as distinct fields and they must not be reconciled.
pub fn score(bin: &Path, document: &Path) -> Result<f64, String> {
    let out = Command::new(bin)
        .arg("score")
        .arg("--json")
        .arg(document)
        .output()
        .map_err(|e| format!("sbomqs spawn failed: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "sbomqs exited non-zero: {}",
            stderr.trim().chars().take(160).collect::<String>()
        ));
    }
    parse_score(&out.stdout)
}

/// Pull `files[0].sbom_quality_score` out of an `sbomqs score --json` body.
///
/// Split out from [`score`] so it is testable without spawning anything —
/// the subprocess is thin, but this key-walk is where the real hazard lives.
/// The trial run initially read a non-existent `avg_score` key and silently
/// produced `None` for all 18 targets, so a missing key MUST be an error and
/// never a default.
pub fn parse_score(stdout: &[u8]) -> Result<f64, String> {
    let v: serde_json::Value =
        serde_json::from_slice(stdout).map_err(|e| format!("sbomqs JSON parse failed: {e}"))?;
    v.get("files")
        .and_then(|f| f.as_array())
        .and_then(|a| a.first())
        .and_then(|f| f.get("sbom_quality_score"))
        .and_then(|s| s.as_f64())
        .ok_or_else(|| "sbomqs output has no files[0].sbom_quality_score".to_string())
}

/// Scores keyed by format name. Only `cyclonedx` this milestone; the map
/// shape is what makes adding SPDX additive (FR-030).
pub fn score_map(bin: &Path, cdx: &Path) -> Result<BTreeMap<String, f64>, String> {
    let mut m = BTreeMap::new();
    m.insert("cyclonedx".to_string(), score(bin, cdx)?);
    Ok(m)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::parse_score;

    /// The exact bug the trial run hit: a plausible-looking body with the
    /// WRONG key name must fail loudly rather than silently yielding nothing.
    #[test]
    fn errors_on_missing_key_rather_than_defaulting() {
        let e = parse_score(br#"{"files":[{"avg_score":7.5}]}"#).unwrap_err();
        assert!(e.contains("sbom_quality_score"), "{e}");
    }

    #[test]
    fn reads_the_documented_key() {
        assert_eq!(
            parse_score(br#"{"files":[{"sbom_quality_score":6.39}]}"#).unwrap(),
            6.39
        );
    }

    #[test]
    fn errors_on_empty_files_array() {
        assert!(parse_score(br#"{"files":[]}"#).is_err());
    }

    #[test]
    fn errors_on_absent_files_key() {
        assert!(parse_score(br#"{}"#).is_err());
    }

    #[test]
    fn errors_on_malformed_json() {
        let e = parse_score(b"not json at all").unwrap_err();
        assert!(e.contains("JSON parse failed"), "{e}");
    }

    #[test]
    fn errors_when_score_is_not_a_number() {
        assert!(parse_score(br#"{"files":[{"sbom_quality_score":"7.5"}]}"#).is_err());
    }

    /// sbomqs reports every scanned file; we deliberately read only the
    /// first, since the command scores exactly one document per invocation.
    #[test]
    fn reads_only_the_first_file_entry() {
        let v = parse_score(
            br#"{"files":[{"sbom_quality_score":1.5},{"sbom_quality_score":9.9}]}"#,
        )
        .unwrap();
        assert_eq!(v, 1.5);
    }
}
