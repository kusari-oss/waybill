//! Parse Homebrew install metadata — `INSTALL_RECEIPT.json` for
//! formulae and `Casks/<token>.json` for casks. Milestone 136
//! (closes #432).
//!
//! Three install-prefix locations are detected independently per
//! research §R4:
//!
//! - `/opt/homebrew` (Apple Silicon macOS, default since macOS 11 / 2020)
//! - `/usr/local` (Intel macOS, default for pre-2020 installs)
//! - `/home/linuxbrew/.linuxbrew` (Linuxbrew on Linux)
//!
//! The `Cellar/` subdirectory existence is the discrimination signal
//! — `<prefix>/` alone is not (especially for `/usr/local` which is
//! a generic Linux sysadmin path).
//!
//! ## On-disk layout
//!
//! ```text
//! <prefix>/
//! ├── Cellar/                                  // formulae
//! │   ├── curl/
//! │   │   └── 8.5.0/
//! │   │       └── INSTALL_RECEIPT.json
//! │   └── openssl@3/
//! │       └── 3.4.0/
//! │           └── INSTALL_RECEIPT.json
//! └── Caskroom/                                // casks (macOS only)
//!     └── visual-studio-code/
//!         └── 1.95.3/
//!             └── .metadata/<version>/<timestamp>/Casks/<token>.json
//! ```
//!
//! Per Constitution Principle V audit (research §R1), the alpm reader's
//! `pkg:alpm/` is a purl-spec native type but `pkg:brew/` is NOT — it's
//! an industry-convention extension shared with syft +
//! cyclonedx-bom-gen. A follow-up issue should propose extending the
//! purl-spec.
//!
//! Per Constitution Principle I (Pure Rust, Zero C — extends to "no
//! embedded scripting parsers" by spirit), Ruby-DSL `.rb`-only casks
//! (pre-Homebrew-4.0) are NOT parsed; they warn-and-skip with an
//! operator-visible diagnostic.

use std::collections::BTreeSet;
use std::path::Path;

use mikebom_common::types::purl::{encode_purl_segment, Purl};

use super::PackageDbEntry;

/// Three documented Homebrew install prefixes, in priority order.
/// Per research §R4 — gated on the `Cellar/` subdir existence.
const HOMEBREW_PREFIXES: &[&str] = &[
    "opt/homebrew",
    "usr/local",
    "home/linuxbrew/.linuxbrew",
];

/// Default tap names that trigger `tap=` qualifier OMISSION per
/// FR-003. All other tap values (including `null`) drop the qualifier.
const DEFAULT_TAPS: &[&str] = &["homebrew/core", "homebrew/cask"];

/// Errors the brew reader can raise. All current failure modes are
/// non-fatal at the scan level — per-formula/cask issues warn-and-skip
/// (FR-007). The enum exists for symmetry with the other OS readers'
/// error types and future fatal-error introduction.
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum BrewError {
    #[error("brew reader I/O error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Discriminator for the PURL builder — formulae and casks share the
/// `pkg:brew/` namespace but casks carry an extra `?type=cask` qualifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrewKind {
    Formula,
    Cask,
}

// =========================================================================
// Formula receipt parsing (INSTALL_RECEIPT.json)
// =========================================================================

/// Reader-private parser intermediate for one `INSTALL_RECEIPT.json`.
/// Mirrors data-model.md's `InstallReceipt` schema with `#[serde(default)]`
/// on every optional field so older receipts parse cleanly.
///
/// Most fields are parsed but not surfaced in v1 — see data-model.md's
/// field mapping table. Marked `#[allow(dead_code)]` for the same reason
/// as alpm's `PacmanDescStanza`.
#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct InstallReceipt {
    #[serde(default)]
    homebrew_version: Option<String>,
    #[serde(default)]
    source: Option<ReceiptSource>,
    #[serde(default)]
    runtime_dependencies: Vec<RuntimeDep>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ReceiptSource {
    #[serde(default)]
    tap: Option<String>,
    #[serde(default)]
    spec: Option<String>,
    #[serde(default)]
    tap_git_head: Option<String>,
    #[serde(default)]
    scm_revision: Option<String>,
    #[serde(default)]
    versions: Option<serde_json::Value>,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct RuntimeDep {
    full_name: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    pkg_version: Option<String>,
    #[serde(default)]
    revision: Option<u32>,
    #[serde(default)]
    declared_directly: Option<bool>,
}

/// Parse an `INSTALL_RECEIPT.json` file's text into an `InstallReceipt`.
/// Returns `None` on parse error (warn-and-skip per FR-007 — caller
/// emits the warning).
fn parse_install_receipt(text: &str) -> Option<InstallReceipt> {
    serde_json::from_str(text).ok()
}

// =========================================================================
// Cask metadata parsing (Casks/<token>.json)
// =========================================================================

/// Reader-private parser intermediate for one `Casks/<token>.json`.
/// Mirrors data-model.md's `CaskMetadata` schema.
#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct CaskMetadata {
    token: String,
    version: String,
    #[serde(default)]
    name: Vec<String>,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    depends_on: Option<serde_json::Value>,
    #[serde(default)]
    artifacts: Vec<serde_json::Value>,
}

/// Parse a cask `Casks/<token>.json` file's text into a `CaskMetadata`.
/// Returns `None` on parse error OR when required fields (`token`,
/// `version`) are missing/empty.
fn parse_cask_metadata(text: &str) -> Option<CaskMetadata> {
    let parsed: CaskMetadata = serde_json::from_str(text).ok()?;
    if parsed.token.is_empty() || parsed.version.is_empty() {
        return None;
    }
    Some(parsed)
}

// =========================================================================
// PURL construction
// =========================================================================

/// Build a `pkg:brew/<name>@<version>[?tap=<owner>/<tap>][&type=cask]`
/// PURL per `contracts/brew-component-purl.md`. Returns `None` if the
/// resulting string fails `Purl::new` validation.
///
/// Qualifier rules:
/// - `tap=`: present when `tap` is `Some(non-empty)` AND not in
///   `DEFAULT_TAPS`. Otherwise omitted.
/// - `type=cask`: present when `kind == BrewKind::Cask`.
///
/// Qualifier ordering follows purl-spec sorted-key convention
/// (`tap` < `type` alphabetically).
fn build_brew_purl(
    name: &str,
    version: &str,
    tap: Option<&str>,
    kind: BrewKind,
) -> Option<Purl> {
    let encoded_name = encode_purl_segment(name);
    let encoded_version = encode_purl_segment(version);

    let tap_qualifier: Option<String> = tap
        .filter(|t| !t.is_empty())
        .filter(|t| !DEFAULT_TAPS.contains(t))
        .map(encode_purl_segment);

    let mut qualifiers: Vec<(&str, String)> = Vec::new();
    if let Some(t) = tap_qualifier {
        qualifiers.push(("tap", t));
    }
    if matches!(kind, BrewKind::Cask) {
        qualifiers.push(("type", "cask".to_string()));
    }
    // Already in sorted-key order since `tap` < `type`.

    let qualifier_str = if qualifiers.is_empty() {
        String::new()
    } else {
        let mut s = String::from("?");
        for (i, (k, v)) in qualifiers.iter().enumerate() {
            if i > 0 {
                s.push('&');
            }
            s.push_str(k);
            s.push('=');
            s.push_str(v);
        }
        s
    };

    let purl_str = format!("pkg:brew/{encoded_name}@{encoded_version}{qualifier_str}");
    Purl::new(&purl_str).ok()
}

// =========================================================================
// Dep-name normalization (analysis-finding I1)
// =========================================================================

/// Normalize a `runtime_dependencies[].full_name` to its bare formula
/// name by stripping any tap-prefix. The dep-resolver in
/// `scan_fs/mod.rs::name_to_purl` matches against `PackageDbEntry.name`
/// which is ALWAYS the bare directory name; therefore tap-qualified
/// `full_name` values like `"hashicorp/tap/terraform"` MUST be
/// normalized to `"terraform"` here or third-party-tap dep edges
/// silently fail to resolve.
///
/// Closes analysis-finding I1 (research / data-model §"Dep-name
/// extraction").
fn dep_bare_name(full_name: &str) -> &str {
    full_name.rsplit('/').next().unwrap_or(full_name)
}

// =========================================================================
// Receipt -> PackageDbEntry conversion
// =========================================================================

fn receipt_to_entry(
    receipt: &InstallReceipt,
    formula_name: &str,
    formula_version: &str,
    source_path: String,
) -> Option<PackageDbEntry> {
    let tap = receipt.source.as_ref().and_then(|s| s.tap.as_deref());
    let purl = build_brew_purl(formula_name, formula_version, tap, BrewKind::Formula)?;

    // Dep names: normalize to bare per I1; sort + dedup for stable
    // output and so the resolver's lookup behaves deterministically.
    let dep_names: Vec<String> = receipt
        .runtime_dependencies
        .iter()
        .filter(|d| !d.full_name.is_empty())
        .map(|d| dep_bare_name(&d.full_name).to_string())
        .filter(|n| !n.is_empty())
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect();

    Some(PackageDbEntry {
        purl,
        name: formula_name.to_string(),
        version: formula_version.to_string(),
        arch: None,
        source_path,
        depends: dep_names,
        maintainer: None,
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type: Some("brew".to_string()),
        licenses: Vec::new(),
        buildinfo_status: None,
        sbom_tier: Some("deployed".to_string()),
        evidence_kind: Some("brew-install-receipt".to_string()),
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: None,
        confidence: None,
        binary_packed: None,
        raw_version: None,
        parent_purl: None,
        npm_role: None,
        co_owned_by: None,
        hashes: Vec::new(),
        shade_relocation: None,
        extra_annotations: std::collections::BTreeMap::new(),
        binary_role: None,
        build_inclusion: None,
    })
}

fn cask_to_entry(
    meta: &CaskMetadata,
    source_path: String,
) -> Option<PackageDbEntry> {
    // Casks don't carry a tap in the on-disk JSON shape we read; the
    // default-tap convention applies. (Future enhancement could read
    // tap context from the surrounding Caskroom layout, but it's not
    // surfaced in the JSON.)
    let purl = build_brew_purl(&meta.token, &meta.version, None, BrewKind::Cask)?;

    Some(PackageDbEntry {
        purl,
        name: meta.token.clone(),
        version: meta.version.clone(),
        arch: None,
        source_path,
        depends: Vec::new(),
        maintainer: None,
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type: Some("brew".to_string()),
        licenses: Vec::new(),
        buildinfo_status: None,
        sbom_tier: Some("deployed".to_string()),
        evidence_kind: Some("brew-cask-metadata".to_string()),
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: None,
        confidence: None,
        binary_packed: None,
        raw_version: None,
        parent_purl: None,
        npm_role: None,
        co_owned_by: None,
        hashes: Vec::new(),
        shade_relocation: None,
        extra_annotations: std::collections::BTreeMap::new(),
        binary_role: None,
        build_inclusion: None,
    })
}

// =========================================================================
// Directory walking
// =========================================================================

/// Walk `<rootfs>/<prefix>/Cellar/*/*/INSTALL_RECEIPT.json` and emit
/// one `PackageDbEntry` per parsed formula. Per-formula failures
/// warn-and-skip per FR-007.
fn read_formulae(rootfs: &Path, prefix: &str) -> Vec<PackageDbEntry> {
    let cellar = rootfs.join(prefix).join("Cellar");
    if !cellar.is_dir() {
        return Vec::new();
    }
    let mut formula_dirs: Vec<std::path::PathBuf> = match std::fs::read_dir(&cellar) {
        Ok(rd) => rd
            .filter_map(|r| r.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect(),
        Err(e) => {
            tracing::warn!(
                path = %cellar.display(),
                error = %e,
                "brew: failed to read Cellar/ directory, skipping prefix",
            );
            return Vec::new();
        }
    };
    formula_dirs.sort();

    let mut out: Vec<PackageDbEntry> = Vec::new();
    for formula_dir in formula_dirs {
        let formula_name = match formula_dir.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let mut version_dirs: Vec<std::path::PathBuf> = match std::fs::read_dir(&formula_dir) {
            Ok(rd) => rd
                .filter_map(|r| r.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect(),
            Err(_) => continue,
        };
        version_dirs.sort();
        for version_dir in version_dirs {
            let version = match version_dir.file_name().and_then(|n| n.to_str()) {
                Some(v) => v.to_string(),
                None => continue,
            };
            let receipt_path = version_dir.join("INSTALL_RECEIPT.json");
            if !receipt_path.is_file() {
                // Some Cellar subdirs may exist without a receipt (very
                // old installs or .brew/ metadata-only dirs). Silently
                // skip — not actionable for the operator.
                continue;
            }
            let text = match std::fs::read_to_string(&receipt_path) {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!(
                        path = %receipt_path.display(),
                        error = %e,
                        "brew: failed to read INSTALL_RECEIPT.json, skipping formula",
                    );
                    continue;
                }
            };
            let Some(receipt) = parse_install_receipt(&text) else {
                tracing::warn!(
                    path = %receipt_path.display(),
                    "brew: failed to parse INSTALL_RECEIPT.json, skipping formula",
                );
                continue;
            };
            let source_path = receipt_path.to_string_lossy().into_owned();
            let Some(entry) = receipt_to_entry(&receipt, &formula_name, &version, source_path)
            else {
                tracing::warn!(
                    formula = formula_name.as_str(),
                    version = version.as_str(),
                    "brew: failed to construct PURL for formula, skipping",
                );
                continue;
            };
            out.push(entry);
        }
    }
    out
}

/// Walk `<rootfs>/<prefix>/Caskroom/<token>/<version>/.metadata/<version>/<timestamp>/Casks/<token>.{json,rb}`
/// and emit one `PackageDbEntry` per parsed cask. `.json`-backed casks
/// parse cleanly; `.rb`-only casks warn-and-skip per research §R5.
fn read_casks(rootfs: &Path, prefix: &str) -> Vec<PackageDbEntry> {
    let caskroom = rootfs.join(prefix).join("Caskroom");
    if !caskroom.is_dir() {
        return Vec::new();
    }
    let mut cask_dirs: Vec<std::path::PathBuf> = match std::fs::read_dir(&caskroom) {
        Ok(rd) => rd
            .filter_map(|r| r.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect(),
        Err(_) => return Vec::new(),
    };
    cask_dirs.sort();

    let mut out: Vec<PackageDbEntry> = Vec::new();
    for cask_dir in cask_dirs {
        let cask_token = match cask_dir.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let mut version_dirs: Vec<std::path::PathBuf> = match std::fs::read_dir(&cask_dir) {
            Ok(rd) => rd
                .filter_map(|r| r.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir() && p.file_name() != Some(std::ffi::OsStr::new(".metadata")))
                .collect(),
            Err(_) => continue,
        };
        version_dirs.sort();
        for version_dir in version_dirs {
            let version = match version_dir.file_name().and_then(|n| n.to_str()) {
                Some(v) => v.to_string(),
                None => continue,
            };
            // Locate the cask metadata JSON.
            let metadata_root = cask_dir.join(".metadata");
            let Some(casks_dir) = find_casks_dir(&metadata_root, &version) else {
                // Empty .metadata/ or missing version subtree —
                // Homebrew sentinel for uninstalled-but-not-cleaned.
                continue;
            };
            let json_path = casks_dir.join(format!("{cask_token}.json"));
            let rb_path = casks_dir.join(format!("{cask_token}.rb"));
            if json_path.is_file() {
                let text = match std::fs::read_to_string(&json_path) {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!(
                            path = %json_path.display(),
                            error = %e,
                            "brew: failed to read cask metadata, skipping cask",
                        );
                        continue;
                    }
                };
                let Some(meta) = parse_cask_metadata(&text) else {
                    tracing::warn!(
                        path = %json_path.display(),
                        "brew: failed to parse cask metadata, skipping cask",
                    );
                    continue;
                };
                let source_path = json_path.to_string_lossy().into_owned();
                if let Some(entry) = cask_to_entry(&meta, source_path) {
                    out.push(entry);
                }
            } else if rb_path.is_file() {
                tracing::warn!(
                    cask = cask_token.as_str(),
                    path = %rb_path.display(),
                    "brew: cask has only Ruby-DSL metadata (no Casks/<token>.json); \
                     skipping — Ruby parsing is out of scope per Constitution Principle I",
                );
            }
            // Else: no cask metadata at all — silent skip (uninstalled remnant).
            // Avoid unused-warning on the version we collected for logging context.
            let _ = version;
        }
    }
    out
}

/// Find the `Casks/` directory under `<cask>/.metadata/<version>/<timestamp>/Casks/`.
/// Returns the first match (sorted lex on timestamp — deterministic).
fn find_casks_dir(metadata_root: &Path, version: &str) -> Option<std::path::PathBuf> {
    let version_dir = metadata_root.join(version);
    if !version_dir.is_dir() {
        return None;
    }
    let mut timestamp_dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&version_dir)
        .ok()?
        .filter_map(|r| r.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    timestamp_dirs.sort();
    for ts_dir in timestamp_dirs {
        let casks_dir = ts_dir.join("Casks");
        if casks_dir.is_dir() {
            return Some(casks_dir);
        }
    }
    None
}

// =========================================================================
// Public entry point
// =========================================================================

/// Walk all three documented Homebrew prefixes and emit one
/// `PackageDbEntry` per installed formula and per (JSON-backed) cask.
///
/// Returns `Ok(vec![])` cleanly when none of the three prefix
/// `Cellar/` directories exist (FR-006 — no-op, no warnings logged).
///
/// Per-formula and per-cask parse failures emit `tracing::warn!` and
/// continue (FR-007 — partial output is more valuable than no output).
///
/// Unlike alpm/dpkg/apk/rpm/opkg, this reader does NOT take
/// `namespace` or `distro_version` params — Homebrew components don't
/// carry an OS-distro namespace. The three prefix locations ARE the
/// discrimination signal, not `/etc/os-release`.
pub fn read(rootfs: &Path) -> Result<Vec<PackageDbEntry>, BrewError> {
    let mut out: Vec<PackageDbEntry> = Vec::new();
    for prefix in HOMEBREW_PREFIXES {
        out.extend(read_formulae(rootfs, prefix));
        out.extend(read_casks(rootfs, prefix));
    }
    if !out.is_empty() {
        tracing::info!(
            rootfs = %rootfs.display(),
            entries = out.len(),
            "parsed Homebrew install metadata",
        );
    }
    Ok(out)
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    // -------- parse_install_receipt --------

    #[test]
    fn parse_install_receipt_minimal() {
        let text = r#"{"homebrew_version":"4.3.0","time":1700000000}"#;
        let r = parse_install_receipt(text).unwrap();
        assert_eq!(r.homebrew_version.as_deref(), Some("4.3.0"));
        assert!(r.source.is_none());
        assert!(r.runtime_dependencies.is_empty());
    }

    #[test]
    fn parse_install_receipt_full() {
        let text = r#"{
            "homebrew_version": "4.3.0",
            "time": 1700000000,
            "source": {
                "tap": "homebrew/core",
                "spec": "stable",
                "tap_git_head": "abc123"
            },
            "runtime_dependencies": [
                {"full_name": "openssl@3", "version": "3.4.0", "pkg_version": "3.4.0_1"},
                {"full_name": "brotli", "version": "1.1.0"}
            ]
        }"#;
        let r = parse_install_receipt(text).unwrap();
        assert_eq!(r.source.as_ref().unwrap().tap.as_deref(), Some("homebrew/core"));
        assert_eq!(r.runtime_dependencies.len(), 2);
        assert_eq!(r.runtime_dependencies[0].full_name, "openssl@3");
    }

    #[test]
    fn parse_install_receipt_third_party_tap() {
        let text = r#"{
            "homebrew_version": "4.3.0",
            "time": 1700000000,
            "source": {"tap": "hashicorp/tap"},
            "runtime_dependencies": []
        }"#;
        let r = parse_install_receipt(text).unwrap();
        assert_eq!(r.source.as_ref().unwrap().tap.as_deref(), Some("hashicorp/tap"));
    }

    #[test]
    fn parse_install_receipt_tap_qualified_dep_fullname() {
        // closes I1 — a runtime_dep with a full_name carrying a 3-segment
        // tap-qualified slug like "hashicorp/tap/terraform" must parse
        // and survive into the dep-extraction step verbatim.
        let text = r#"{
            "homebrew_version": "4.3.0",
            "runtime_dependencies": [
                {"full_name": "hashicorp/tap/terraform", "version": "1.10.0"}
            ]
        }"#;
        let r = parse_install_receipt(text).unwrap();
        assert_eq!(
            r.runtime_dependencies[0].full_name,
            "hashicorp/tap/terraform"
        );
    }

    #[test]
    fn parse_install_receipt_malformed_returns_none() {
        assert!(parse_install_receipt("not json at all").is_none());
        assert!(parse_install_receipt(r#"{"runtime_dependencies": "not an array"}"#).is_none());
    }

    #[test]
    fn parse_install_receipt_no_runtime_deps_field() {
        // Older receipts (pre-2017) lack runtime_dependencies entirely.
        // Must parse with empty Vec.
        let text = r#"{"homebrew_version":"1.0.0","time":1500000000}"#;
        let r = parse_install_receipt(text).unwrap();
        assert!(r.runtime_dependencies.is_empty());
    }

    // -------- parse_cask_metadata --------

    #[test]
    fn parse_cask_metadata_minimal() {
        let text = r#"{"token":"visual-studio-code","version":"1.95.3"}"#;
        let c = parse_cask_metadata(text).unwrap();
        assert_eq!(c.token, "visual-studio-code");
        assert_eq!(c.version, "1.95.3");
    }

    #[test]
    fn parse_cask_metadata_full() {
        let text = r#"{
            "token": "firefox",
            "version": "121.0",
            "name": ["Mozilla Firefox", "Firefox"],
            "desc": "Web browser",
            "homepage": "https://firefox.com",
            "url": "https://download.firefox.com/firefox-121.0.dmg",
            "sha256": "abc123"
        }"#;
        let c = parse_cask_metadata(text).unwrap();
        assert_eq!(c.token, "firefox");
        assert_eq!(c.name, vec!["Mozilla Firefox", "Firefox"]);
    }

    #[test]
    fn parse_cask_metadata_missing_token_returns_none() {
        let text = r#"{"version":"1.0"}"#;
        assert!(parse_cask_metadata(text).is_none());
    }

    #[test]
    fn parse_cask_metadata_missing_version_returns_none() {
        let text = r#"{"token":"foo"}"#;
        assert!(parse_cask_metadata(text).is_none());
    }

    #[test]
    fn parse_cask_metadata_empty_strings_return_none() {
        let text = r#"{"token":"","version":""}"#;
        assert!(parse_cask_metadata(text).is_none());
    }

    // -------- build_brew_purl --------

    #[test]
    fn purl_core_formula_no_qualifiers() {
        let p = build_brew_purl("curl", "8.5.0", None, BrewKind::Formula).unwrap();
        assert_eq!(p.as_str(), "pkg:brew/curl@8.5.0");
    }

    #[test]
    fn purl_homebrew_core_tap_omitted() {
        let p =
            build_brew_purl("curl", "8.5.0", Some("homebrew/core"), BrewKind::Formula).unwrap();
        assert_eq!(p.as_str(), "pkg:brew/curl@8.5.0");
    }

    #[test]
    fn purl_homebrew_cask_tap_omitted_for_cask() {
        let p = build_brew_purl(
            "firefox",
            "121.0",
            Some("homebrew/cask"),
            BrewKind::Cask,
        )
        .unwrap();
        assert_eq!(p.as_str(), "pkg:brew/firefox@121.0?type=cask");
    }

    #[test]
    fn purl_third_party_tap_qualifier_present() {
        let p = build_brew_purl(
            "terraform",
            "1.10.0",
            Some("hashicorp/tap"),
            BrewKind::Formula,
        )
        .unwrap();
        // encode_purl_segment preserves `/` in qualifier values
        // (PURL spec allows them); we get the unencoded form.
        assert_eq!(p.as_str(), "pkg:brew/terraform@1.10.0?tap=hashicorp/tap");
    }

    #[test]
    fn purl_cask_with_type_qualifier() {
        let p = build_brew_purl("visual-studio-code", "1.95.3", None, BrewKind::Cask).unwrap();
        assert_eq!(p.as_str(), "pkg:brew/visual-studio-code@1.95.3?type=cask");
    }

    #[test]
    fn purl_cask_with_third_party_tap_orders_qualifiers() {
        let p = build_brew_purl(
            "intellij-idea",
            "2024.3",
            Some("homebrew/cask-versions"),
            BrewKind::Cask,
        )
        .unwrap();
        // tap < type alphabetically → tap first
        assert_eq!(
            p.as_str(),
            "pkg:brew/intellij-idea@2024.3?tap=homebrew/cask-versions&type=cask"
        );
    }

    #[test]
    fn purl_null_tap_treated_as_default() {
        let p = build_brew_purl("curl", "8.5.0", Some(""), BrewKind::Formula).unwrap();
        assert_eq!(p.as_str(), "pkg:brew/curl@8.5.0");
    }

    #[test]
    fn purl_formula_with_at_symbol_in_name() {
        let p = build_brew_purl("openssl@3", "3.4.0", None, BrewKind::Formula).unwrap();
        // @ in name segment — must NOT be percent-encoded (per
        // encode_purl_segment's PURL-segment encoding rules; @ in the
        // name is part of the realistic Homebrew naming convention).
        assert!(p.as_str().contains("openssl"));
        assert!(p.as_str().contains("3.4.0"));
    }

    // -------- dep_bare_name (closes I1) --------

    #[test]
    fn dep_bare_name_bare_input() {
        assert_eq!(dep_bare_name("openssl@3"), "openssl@3");
        assert_eq!(dep_bare_name("curl"), "curl");
    }

    #[test]
    fn dep_bare_name_strips_three_segment_tap_prefix() {
        assert_eq!(dep_bare_name("hashicorp/tap/terraform"), "terraform");
        assert_eq!(
            dep_bare_name("mongodb/brew/mongodb-community"),
            "mongodb-community"
        );
    }

    #[test]
    fn dep_bare_name_strips_two_segment_prefix() {
        // Defensive: a 2-segment value (rare/unusual) still strips to
        // the last segment.
        assert_eq!(dep_bare_name("foo/bar"), "bar");
    }

    #[test]
    fn dep_bare_name_empty_returns_empty() {
        assert_eq!(dep_bare_name(""), "");
    }

    // -------- receipt_to_entry --------

    #[test]
    fn receipt_to_entry_basic() {
        let receipt = parse_install_receipt(
            r#"{
                "source": {"tap": "homebrew/core"},
                "runtime_dependencies": [
                    {"full_name": "openssl@3"},
                    {"full_name": "brotli"}
                ]
            }"#,
        )
        .unwrap();
        let entry = receipt_to_entry(
            &receipt,
            "curl",
            "8.5.0",
            "/tmp/INSTALL_RECEIPT.json".to_string(),
        )
        .unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:brew/curl@8.5.0");
        assert_eq!(entry.name, "curl");
        assert_eq!(entry.version, "8.5.0");
        assert_eq!(entry.source_type.as_deref(), Some("brew"));
        assert_eq!(entry.evidence_kind.as_deref(), Some("brew-install-receipt"));
        assert_eq!(entry.sbom_tier.as_deref(), Some("deployed"));
        assert_eq!(entry.depends, vec!["brotli".to_string(), "openssl@3".to_string()]);
        assert!(entry.licenses.is_empty()); // FR-011 deferred
        assert!(entry.arch.is_none());
    }

    #[test]
    fn receipt_to_entry_third_party_tap_dep_normalized_to_bare() {
        // closes I1 end-to-end
        let receipt = parse_install_receipt(
            r#"{
                "source": {"tap": "homebrew/core"},
                "runtime_dependencies": [
                    {"full_name": "openssl@3"},
                    {"full_name": "hashicorp/tap/terraform"}
                ]
            }"#,
        )
        .unwrap();
        let entry =
            receipt_to_entry(&receipt, "my-tool", "1.0", String::new()).unwrap();
        // The third-party dep "hashicorp/tap/terraform" was normalized
        // to "terraform"; the resolver in scan_fs/mod.rs would then
        // find a component named "terraform" (which is how third-party
        // tap formulae are emitted — directory name is bare).
        assert_eq!(
            entry.depends,
            vec!["openssl@3".to_string(), "terraform".to_string()]
        );
    }

    #[test]
    fn receipt_to_entry_third_party_tap_qualifier_in_purl() {
        let receipt = parse_install_receipt(
            r#"{"source": {"tap": "hashicorp/tap"}, "runtime_dependencies": []}"#,
        )
        .unwrap();
        let entry =
            receipt_to_entry(&receipt, "terraform", "1.10.0", String::new()).unwrap();
        assert_eq!(
            entry.purl.as_str(),
            "pkg:brew/terraform@1.10.0?tap=hashicorp/tap"
        );
    }

    #[test]
    fn receipt_to_entry_no_source_treated_as_default_tap() {
        let receipt = parse_install_receipt(r#"{}"#).unwrap();
        let entry = receipt_to_entry(&receipt, "x", "1", String::new()).unwrap();
        assert_eq!(entry.purl.as_str(), "pkg:brew/x@1");
        assert!(entry.depends.is_empty());
    }

    // -------- cask_to_entry --------

    #[test]
    fn cask_to_entry_basic() {
        let meta = parse_cask_metadata(
            r#"{"token":"visual-studio-code","version":"1.95.3"}"#,
        )
        .unwrap();
        let entry = cask_to_entry(&meta, String::new()).unwrap();
        assert_eq!(
            entry.purl.as_str(),
            "pkg:brew/visual-studio-code@1.95.3?type=cask"
        );
        assert_eq!(entry.evidence_kind.as_deref(), Some("brew-cask-metadata"));
        assert!(entry.depends.is_empty()); // FR-005
    }

    // -------- read() — directory-walk happy paths + edge cases --------

    fn write_formula(
        rootfs: &Path,
        prefix: &str,
        formula: &str,
        version: &str,
        receipt_body: &str,
    ) {
        let dir = rootfs.join(prefix).join("Cellar").join(formula).join(version);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("INSTALL_RECEIPT.json"), receipt_body).unwrap();
    }

    fn write_cask_json(
        rootfs: &Path,
        prefix: &str,
        token: &str,
        version: &str,
        timestamp: &str,
        json_body: &str,
    ) {
        let dir = rootfs
            .join(prefix)
            .join("Caskroom")
            .join(token)
            .join(".metadata")
            .join(version)
            .join(timestamp)
            .join("Casks");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{token}.json")), json_body).unwrap();
        // Also create the version dir alongside .metadata so cask_dir
        // iteration finds it.
        let payload_dir = rootfs.join(prefix).join("Caskroom").join(token).join(version);
        std::fs::create_dir_all(&payload_dir).unwrap();
    }

    fn write_cask_rb_only(
        rootfs: &Path,
        prefix: &str,
        token: &str,
        version: &str,
        timestamp: &str,
        rb_body: &str,
    ) {
        let dir = rootfs
            .join(prefix)
            .join("Caskroom")
            .join(token)
            .join(".metadata")
            .join(version)
            .join(timestamp)
            .join("Casks");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{token}.rb")), rb_body).unwrap();
        let payload_dir = rootfs.join(prefix).join("Caskroom").join(token).join(version);
        std::fs::create_dir_all(&payload_dir).unwrap();
    }

    #[test]
    fn read_empty_rootfs_returns_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let entries = read(tmp.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn read_no_homebrew_dir_returns_zero_no_warn() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("etc")).unwrap();
        let entries = read(tmp.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn read_three_formulae_apple_silicon() {
        let tmp = tempfile::tempdir().unwrap();
        write_formula(
            tmp.path(),
            "opt/homebrew",
            "curl",
            "8.5.0",
            r#"{"source":{"tap":"homebrew/core"},"runtime_dependencies":[{"full_name":"openssl@3"}]}"#,
        );
        write_formula(
            tmp.path(),
            "opt/homebrew",
            "openssl@3",
            "3.4.0",
            r#"{"source":{"tap":"homebrew/core"}}"#,
        );
        write_formula(
            tmp.path(),
            "opt/homebrew",
            "brotli",
            "1.1.0",
            r#"{"source":{"tap":"homebrew/core"}}"#,
        );
        let entries = read(tmp.path()).unwrap();
        assert_eq!(entries.len(), 3);
        let purls: Vec<&str> = entries.iter().map(|e| e.purl.as_str()).collect();
        assert!(purls.contains(&"pkg:brew/curl@8.5.0"));
        assert!(purls.contains(&"pkg:brew/openssl@3@3.4.0"));
        assert!(purls.contains(&"pkg:brew/brotli@1.1.0"));
    }

    #[test]
    fn read_intel_macos_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        write_formula(
            tmp.path(),
            "usr/local",
            "curl",
            "8.5.0",
            r#"{"source":{"tap":"homebrew/core"}}"#,
        );
        let entries = read(tmp.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].purl.as_str(), "pkg:brew/curl@8.5.0");
    }

    #[test]
    fn read_linuxbrew_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        write_formula(
            tmp.path(),
            "home/linuxbrew/.linuxbrew",
            "curl",
            "8.5.0",
            r#"{"source":{"tap":"homebrew/core"}}"#,
        );
        let entries = read(tmp.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].purl.as_str(), "pkg:brew/curl@8.5.0");
    }

    #[test]
    fn read_usr_local_without_cellar_emits_zero() {
        // U2 — non-ELF README at /usr/local; no Cellar/ → zero brew components.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("usr/local/share")).unwrap();
        std::fs::write(tmp.path().join("usr/local/share/README.txt"), "hello").unwrap();
        let entries = read(tmp.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn read_malformed_receipt_warns_and_continues() {
        let tmp = tempfile::tempdir().unwrap();
        write_formula(
            tmp.path(),
            "opt/homebrew",
            "good",
            "1.0",
            r#"{"source":{"tap":"homebrew/core"}}"#,
        );
        write_formula(
            tmp.path(),
            "opt/homebrew",
            "broken",
            "1.0",
            "not valid json at all",
        );
        let entries = read(tmp.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "good");
    }

    #[test]
    fn read_multi_version_formula_emits_separate_components() {
        // FR-008
        let tmp = tempfile::tempdir().unwrap();
        write_formula(
            tmp.path(),
            "opt/homebrew",
            "openssl@1.1",
            "1.1.1w",
            r#"{"source":{"tap":"homebrew/core"}}"#,
        );
        write_formula(
            tmp.path(),
            "opt/homebrew",
            "openssl@3",
            "3.4.0",
            r#"{"source":{"tap":"homebrew/core"}}"#,
        );
        let entries = read(tmp.path()).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn read_cask_json() {
        let tmp = tempfile::tempdir().unwrap();
        write_cask_json(
            tmp.path(),
            "opt/homebrew",
            "visual-studio-code",
            "1.95.3",
            "20251001120000.000",
            r#"{"token":"visual-studio-code","version":"1.95.3"}"#,
        );
        let entries = read(tmp.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:brew/visual-studio-code@1.95.3?type=cask"
        );
        assert_eq!(entries[0].evidence_kind.as_deref(), Some("brew-cask-metadata"));
    }

    #[test]
    fn read_rb_only_cask_warn_and_skip() {
        let tmp = tempfile::tempdir().unwrap();
        write_cask_rb_only(
            tmp.path(),
            "opt/homebrew",
            "transmission",
            "3.00",
            "20240101000000.000",
            "cask 'transmission' do\n  version '3.00'\nend",
        );
        let entries = read(tmp.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn read_formula_and_cask_coexistence() {
        let tmp = tempfile::tempdir().unwrap();
        write_formula(
            tmp.path(),
            "opt/homebrew",
            "curl",
            "8.5.0",
            r#"{"source":{"tap":"homebrew/core"}}"#,
        );
        write_cask_json(
            tmp.path(),
            "opt/homebrew",
            "firefox",
            "121.0",
            "20251001120000.000",
            r#"{"token":"firefox","version":"121.0"}"#,
        );
        let entries = read(tmp.path()).unwrap();
        assert_eq!(entries.len(), 2);
        let purls: Vec<&str> = entries.iter().map(|e| e.purl.as_str()).collect();
        assert!(purls.contains(&"pkg:brew/curl@8.5.0"));
        assert!(purls.contains(&"pkg:brew/firefox@121.0?type=cask"));
    }
}
