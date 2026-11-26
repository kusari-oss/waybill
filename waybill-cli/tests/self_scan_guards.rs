//! Four guards on one scan of waybill's own debug binary (spec 1659, #1716).
//!
//! These used to be four tests, each copying the 125 MB debug binary into a
//! temp dir and scanning it with the same arguments: about 5 s each locally
//! and 20 s each on Linux CI. Under nextest every test is its own process,
//! so an in-process cache cannot share one scan between them; one test that
//! scans once and evaluates all four does. Every guard is evaluated before
//! the test fails, and each failure is named.
//!
//! - **No `openssl:openssl` CPE** (milestone 097, SC-007, was
//!   `cpe_binary_id::mikebom_self_scan_emits_no_spurious_openssl_cpe`).
//!   waybill uses rustls, not OpenSSL. If this fires, either the
//!   milestone-096 version-string scanner now matches an OpenSSL literal
//!   embedded in waybill (a spurious match: fix the scanner's anchor regex),
//!   or waybill genuinely started linking OpenSSL (a real dependency change).
//! - **No spurious version strings** (milestone 096, SC-007, was
//!   `binary_id_enrich::mikebom_itself_does_not_emit_spurious_version_strings`).
//! - **No spurious symbol fingerprints** (milestones 096 and 099, was
//!   `binary_id_enrich::mikebom_itself_does_not_emit_spurious_symbol_fingerprints`).
//!   If this fires, either waybill started linking one of those libraries,
//!   or the fingerprint threshold is too loose.
//! - **Build provenance** (milestone 098, SC-001 to SC-003, was
//!   `binary_build_provenance::mikebom_self_scan_emits_build_provenance_properties`).
//!   The host format's build-tier properties appear on the file-level binary
//!   component. It skips when no file-level binary component is produced
//!   (e.g. a containerized scan where a package-db reader claimed the binary).
//!
//! The first three need this specific binary: their claim is about
//! waybill's own bytes. The fourth needs any host-format binary, and this
//! scan provides one.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

/// `Ok(None)` passed, `Ok(Some(note))` skipped with a reason, `Err` failed.
type GuardResult = Result<Option<String>, String>;

/// A guard's name and the check it makes on the scan output.
type Guard = (&'static str, fn(&Value) -> GuardResult);

fn property_value(component: &Value, name: &str) -> Option<Value> {
    component["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
        .map(|p| p["value"].clone())
}

fn find_file_level(sbom: &Value) -> Option<&Value> {
    sbom["components"]
        .as_array()?
        .iter()
        .find(|c| property_value(c, "waybill:binary-class").is_some())
}

fn component_purls(sbom: &Value) -> Vec<String> {
    sbom["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["purl"].as_str().map(str::to_string))
        .collect()
}

fn guard_no_openssl_cpe(sbom: &Value) -> GuardResult {
    let json_str = serde_json::to_string(sbom).expect("SBOM JSON re-serialize");
    if json_str.contains("cpe:2.3:a:openssl:openssl:") {
        return Err("milestone-097 SC-007 spurious-emission guard: waybill binary \
                    should not emit an openssl:openssl CPE (waybill uses rustls). \
                    If this fires, check (a) milestone-096 version-string scanner \
                    for a false-positive anchor match, OR (b) whether waybill now \
                    genuinely links OpenSSL (a real dependency change)."
            .to_string());
    }
    Ok(None)
}

fn guard_no_spurious_version_strings(sbom: &Value) -> GuardResult {
    let spurious: Vec<String> = component_purls(sbom)
        .into_iter()
        .filter(|purl| {
            purl.starts_with("pkg:generic/openssl@")
                || purl.starts_with("pkg:generic/zlib@")
                || purl.starts_with("pkg:generic/curl@")
                || purl.starts_with("pkg:generic/libcurl@")
                || purl.starts_with("pkg:generic/sqlite@")
                || purl.starts_with("pkg:generic/libxml2@")
        })
        .collect();
    if !spurious.is_empty() {
        return Err(format!(
            "SC-007 false-positive guard: waybill binary should not trip \
             any v1 version-string pattern. Found {} spurious matches: {:?}",
            spurious.len(),
            spurious,
        ));
    }
    Ok(None)
}

fn guard_no_spurious_symbol_fingerprints(sbom: &Value) -> GuardResult {
    // The no-version PURL shape that symbol_match_to_entry emits. A trailing
    // `@` or `?` would be a version-string composite, which the guard above
    // covers.
    let spurious: Vec<String> = component_purls(sbom)
        .into_iter()
        .filter(|purl| {
            matches!(
                purl.as_str(),
                // Milestone-096 v1 fingerprints.
                "pkg:generic/openssl"
                    | "pkg:generic/zlib"
                    | "pkg:generic/libcurl"
                    // Milestone-099 v2 expansion. waybill uses rustls and Rust's
                    // own regex crate, linking none of these.
                    | "pkg:generic/sqlite"
                    | "pkg:generic/pcre"
                    | "pkg:generic/pcre2"
                    | "pkg:generic/gnutls"
            )
        })
        .collect();
    if !spurious.is_empty() {
        return Err(format!(
            "SC-007 false-positive guard: waybill should not trip any v1 \
             symbol fingerprint. Found {} spurious matches: {:?}",
            spurious.len(),
            spurious,
        ));
    }
    Ok(None)
}

/// A property whose value is a JSON document encoded as a string.
fn json_property(component: &Value, name: &str, what: &str) -> Result<Value, String> {
    let raw = property_value(component, name).ok_or_else(|| format!("{what} binary should emit {name}"))?;
    let s = raw.as_str().ok_or_else(|| format!("{name}: property value is a JSON-encoded string"))?;
    serde_json::from_str(s).map_err(|e| format!("{name}: value must be JSON: {e}"))
}

fn guard_build_provenance(sbom: &Value) -> GuardResult {
    let Some(file_level) = find_file_level(sbom) else {
        return Ok(Some(
            "skipping milestone-098 smoke: no file-level binary component \
             emitted (host's waybill may have been claimed by a package-db \
             reader, or the temp-dir walk found no unclaimed ELF/Mach-O/PE)"
                .to_string(),
        ));
    };
    let binary_class = property_value(file_level, "waybill:binary-class")
        .and_then(|v| v.as_str().map(str::to_string))
        .ok_or("binary-class is present on every file-level binary component")?;

    match binary_class.as_str() {
        "elf" => {
            // FR-001 / SC-001: ELF `.comment` stamps on the file-level
            // component. The prefix is not locked: rustc and statically
            // linked C dependencies may both contribute stamps.
            let parsed = json_property(file_level, "waybill:elf-compiler-stamps", "ELF")?;
            let arr = parsed.as_array().ok_or("compiler-stamps value must decode to a JSON array")?;
            if arr.is_empty() {
                return Err(format!("FR-001: ELF binary should emit ≥1 compiler stamp, got: {arr:?}"));
            }
        }
        "macho" => {
            // FR-002 / SC-002: LC_BUILD_VERSION fields.
            let bv = json_property(file_level, "waybill:macho-build-version", "Mach-O")?;
            if bv["platform"].as_str() != Some("macos") {
                return Err(format!("SC-002: Mach-O scan of waybill-self → platform=macos, got {bv:?}"));
            }
            let tools = json_property(file_level, "waybill:macho-build-tools", "Mach-O")?;
            let tools_arr = tools.as_array().ok_or("build-tools value must decode to a JSON array")?;
            if tools_arr.is_empty() {
                return Err(format!("SC-002: Mach-O scan should emit ≥1 tool entry, got: {tools_arr:?}"));
            }
        }
        "pe" => {
            // FR-003 / SC-003: every parseable PE emits a
            // `waybill:pe-linker-version` matching `^\d+\.\d+$`.
            let lv = property_value(file_level, "waybill:pe-linker-version")
                .ok_or("PE binary should always emit waybill:pe-linker-version")?;
            let s = lv.as_str().ok_or("linker-version value must be a string")?;
            let parts: Vec<&str> = s.split('.').collect();
            if parts.len() != 2 {
                return Err(format!("SC-003: pe-linker-version must match <major>.<minor>, got {s:?}"));
            }
            if !(parts[0].chars().all(|c| c.is_ascii_digit()) && parts[1].chars().all(|c| c.is_ascii_digit())) {
                return Err(format!("SC-003: pe-linker-version segments must be decimal, got {s:?}"));
            }
        }
        other => {
            return Ok(Some(format!("skipping milestone-098 smoke: unrecognized binary-class {other:?}")));
        }
    }
    Ok(None)
}

fn scan_waybill_itself() -> Value {
    let dir = TempDir::new().unwrap();
    let dest = dir.path().join("waybill-under-test");
    std::fs::copy(env!("CARGO_BIN_EXE_waybill"), &dest).unwrap();
    let out_file = dir.path().join("out.cdx.json");
    let output = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args(["sbom", "scan", "--path"])
        .arg(dir.path())
        .arg("--output")
        .arg(&out_file)
        .arg("--no-deep-hash")
        .output()
        .expect("failed to invoke waybill");
    assert!(
        output.status.success(),
        "waybill sbom scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    serde_json::from_slice(&std::fs::read(&out_file).expect("SBOM not written")).expect("invalid SBOM JSON")
}

#[test]
fn waybill_self_scan_guards() {
    let sbom = scan_waybill_itself();
    let guards: [Guard; 4] = [
        ("no openssl:openssl CPE", guard_no_openssl_cpe),
        ("no spurious version strings", guard_no_spurious_version_strings),
        ("no spurious symbol fingerprints", guard_no_spurious_symbol_fingerprints),
        ("build provenance", guard_build_provenance),
    ];
    let mut failures = Vec::new();
    for (name, guard) in guards {
        match guard(&sbom) {
            Ok(None) => {}
            Ok(Some(note)) => eprintln!("{name}: {note}"),
            Err(msg) => failures.push(format!("[{name}] {msg}")),
        }
    }
    assert!(failures.is_empty(), "{} self-scan guard(s) failed:\n{}", failures.len(), failures.join("\n"));
}
