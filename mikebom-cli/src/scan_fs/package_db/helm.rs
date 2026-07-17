//! Milestone 188 (#455) — Helm chart scanning.
//!
//! Two-layer emission per spec.md:
//!
//! - **US1 chart-level**: parse `Chart.yaml` + `Chart.lock` +
//!   `charts/*.tgz` recursively; emit one `pkg:helm/<repo>/<name>@<version>`
//!   component per declared/locked/packaged dep. `Chart.lock` is
//!   authoritative over `Chart.yaml` when both present
//!   (package-lock.json > package.json precedent).
//!
//! - **US2 template-level**: scan `templates/*.yaml` + `crds/*.yaml`
//!   for `image: <ref>` extraction using a permissive line-based regex
//!   with Go-template tolerance. Unresolved
//!   `{{ .Values.image.tag }}` placeholders emit as
//!   `pkg:generic/<placeholder>` with `mikebom:image-ref-unresolved =
//!   "true"` property. Resolved refs emit as
//!   `pkg:docker/<name>@<tag>` (tagged) or
//!   `pkg:oci/<name>@sha256:<digest>` (digested).
//!
//! - **US3 rendered (opt-in `--helm-render`)**: shell out to
//!   `helm template <chart-dir>` with a 60s timeout + env override. On
//!   success: extracts image refs from the fully-rendered YAML (higher
//!   fidelity, no placeholder markers). On failure (helm missing, exit
//!   non-zero, timeout): WARN + fall back to US2 unrendered extraction.
//!
//! **Native-field audit per Constitution Principle V** (documented in
//! `docs/reference/sbom-format-mapping.md` §Milestone 188 addendum):
//! Two new `mikebom:*` properties added — `mikebom:image-ref-unresolved`
//! (per-component) + `mikebom:image-extraction-completeness` (document
//! scope, plumbed via `ScanDiagnostics.helm_extraction_mode`). No native
//! CDX / SPDX 2.3 / SPDX 3 construct exists for either concept.
//!
//! Auto-detection at `<scan-root>/Chart.yaml` presence (research §R1).
//! Composability preserved — other package-DB readers run alongside
//! per Clarifications Q1.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use flate2::read::GzDecoder;
use regex::Regex;
use serde::Deserialize;

use mikebom_common::types::purl::Purl;

use super::{HelmExtractionMode, PackageDbEntry, ScanDiagnostics};

// -----------------------------------------------------------------
// Milestone 203 (#553) — `--helm-render` subprocess types + helper.
// -----------------------------------------------------------------

/// Failure classes for the `helm template` subprocess path (m203 US3).
/// Every variant triggers a WARN-log + fallback to unrendered extraction
/// at the `helm::read` branch site. Scan never aborts due to helm-render
/// issues (FR-007 + Constitution Principle III fail-graceful posture).
#[derive(Debug, thiserror::Error)]
pub(super) enum HelmRenderError {
    #[error("`helm` binary not found on $PATH")]
    BinaryNotFound,

    #[error("`helm template` exited with code {code}; stderr head: {stderr_head}")]
    NonZeroExit { code: i32, stderr_head: String },

    #[error("`helm template` exceeded {timeout_secs}s timeout")]
    Timeout { timeout_secs: u64 },

    #[error("`helm template` I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Milestone 203 (FR-002): read `MIKEBOM_HELM_RENDER_TIMEOUT_SECS` env
/// var, parse as `u64`, clamp to `[1, 3600]`, default to 60. Silent
/// clamp semantics per research R4 — matches m173/m089 env-var handling
/// posture (parse-fail or out-of-range value silently falls back to
/// default rather than aborting the scan for an operator override
/// experiment).
fn resolve_render_timeout() -> Duration {
    const DEFAULT_SECS: u64 = 60;
    const MIN_SECS: u64 = 1;
    const MAX_SECS: u64 = 3600;
    let secs = std::env::var("MIKEBOM_HELM_RENDER_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(|n| n.clamp(MIN_SECS, MAX_SECS))
        .unwrap_or(DEFAULT_SECS);
    Duration::from_secs(secs)
}

/// Cap the first `max_lines` lines of a UTF-8-lossy stderr byte buffer
/// per m188 FR-018 (secrets guard). Prevents kubeconfig / secret leakage
/// in WARN logs from `helm template` subprocess stderr.
fn cap_stderr_lines(bytes: &[u8], max_lines: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Milestone 203 (FR-001, closes #553): shell out to `helm template
/// <chart-dir>` and extract image refs from the fully-rendered stdout.
/// Reuses the m055 `run_go_mod_graph` subprocess-with-timeout pattern:
/// probe binary → spawn worker thread → main-thread `recv_timeout`.
/// Same pattern as m053 (`git describe`) + m173 (warm-go-cache).
///
/// Returns `Ok(refs)` on successful helm template + regex extraction.
/// Returns `Err(HelmRenderError::*)` on any failure class; caller
/// WARN-logs + falls back to `extract_image_refs_unrendered`.
pub(super) fn extract_image_refs_rendered(
    chart_dir: &Path,
    timeout: Duration,
) -> Result<Vec<ImageRef>, HelmRenderError> {
    use std::process::Command;
    use std::sync::mpsc;
    use std::thread;

    // Probe: `helm version --short` — fails fast on missing binary.
    match Command::new("helm").arg("version").arg("--short").output() {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(HelmRenderError::BinaryNotFound);
        }
        Err(e) => return Err(HelmRenderError::IoError(e)),
    }

    // Spawn `helm template` in a worker thread so main can enforce
    // timeout via `recv_timeout`. Worker leaks on timeout (documented
    // per m055 comment at go_mod_graph.rs:117-118) — subprocess reaped
    // by the OS eventually.
    let (tx, rx) = mpsc::channel();
    let chart_dir_owned = chart_dir.to_path_buf();
    thread::spawn(move || {
        let result = Command::new("helm")
            .args(["template", &chart_dir_owned.to_string_lossy()])
            .output();
        let _ = tx.send(result);
    });

    let output = match rx.recv_timeout(timeout) {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(HelmRenderError::IoError(e)),
        Err(_) => {
            return Err(HelmRenderError::Timeout {
                timeout_secs: timeout.as_secs(),
            });
        }
    };

    if !output.status.success() {
        return Err(HelmRenderError::NonZeroExit {
            code: output.status.code().unwrap_or(-1),
            stderr_head: cap_stderr_lines(&output.stderr, 20),
        });
    }

    // Success: apply the existing IMAGE_REGEX to the rendered stdout.
    // Use a synthetic path "helm-template-rendered" for the source_path
    // field since the ref came from post-render stdout, not a specific
    // template file.
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let mut refs = Vec::new();
    for caps in image_regex().captures_iter(&stdout_str) {
        if let Some(m) = caps.get(1) {
            let raw = m.as_str().trim().to_string();
            if raw.is_empty() {
                continue;
            }
            let kind = classify_image_ref(&raw);
            refs.push(ImageRef {
                raw,
                kind,
                source_path: "helm-template-rendered".to_string(),
            });
        }
    }
    Ok(refs)
}

/// Depth cap for recursive `charts/*.tgz` descent per FR-005. Matches
/// m114 filesystem walker's depth ceiling.
const MAX_CHART_RECURSION_DEPTH: usize = 12;

/// Milestone 188 (#455) — CLI-mode selector for the optional `helm
/// template` shell-out. See `--helm-render` flag on `ScanArgs` +
/// contracts/cli-flags.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HelmRenderMode {
    /// Default. mikebom does NOT invoke the `helm` binary. Extraction
    /// runs unrendered per FR-013.
    #[default]
    Off,
    /// Operator passed `--helm-render`. mikebom attempts to shell out
    /// to `helm template <chart-dir>`; on failure (missing binary,
    /// non-zero exit, timeout), falls back to Off behavior per FR-012.
    OptIn,
}

// -----------------------------------------------------------------
// Milestone 188 (#455) — Chart.yaml + Chart.lock deserialization types.
// -----------------------------------------------------------------

/// Deserialized `Chart.yaml`. Follows Helm 3.x chart schema.
#[allow(dead_code)] // description/keywords/home retained for future annotation emission
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ChartMetadata {
    /// Chart name (`name:` field). REQUIRED.
    pub name: String,
    /// SemVer version (`version:` field). REQUIRED.
    pub version: String,
    /// Chart type (`type: application | library`). Defaults to
    /// `"application"` when omitted (Helm convention).
    #[serde(default = "default_chart_type", rename = "type")]
    pub chart_type: String,
    /// Application version (`appVersion:` — the version of the app
    /// packaged by the chart). Optional.
    #[serde(default, rename = "appVersion")]
    pub app_version: Option<String>,
    /// Dependencies declared in Chart.yaml. Each is emitted as its own
    /// component per FR-003.
    #[serde(default)]
    pub dependencies: Vec<ChartDep>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub home: Option<String>,
}

fn default_chart_type() -> String {
    "application".to_string()
}

/// A Chart.yaml `dependencies[]` entry OR a Chart.lock locked-
/// resolution entry.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ChartDep {
    /// Dep chart name. REQUIRED.
    pub name: String,
    /// Dep chart version. REQUIRED.
    pub version: String,
    /// Dep repository — URL or `@`-prefixed alias. Modern charts always
    /// set this; legacy charts may omit.
    #[serde(default)]
    pub repository: Option<String>,
    /// Optional dependency alias. When present, takes precedence over
    /// `repository` for the PURL `<namespace>` segment.
    #[serde(default)]
    pub alias: Option<String>,
    /// Conditional-inclusion expression (not evaluated in m188).
    #[serde(default)]
    pub condition: Option<String>,
}

/// Deserialized `Chart.lock`. Scoped to locked resolutions.
#[allow(dead_code)] // digest/generated retained for future integrity-check work
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ChartLock {
    #[serde(default)]
    pub dependencies: Vec<ChartDep>,
    /// SHA-256 digest of the dependencies block (Helm-internal
    /// integrity check; not verified in m188).
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub generated: Option<String>,
}

// -----------------------------------------------------------------
// Milestone 188 (#455) — Image-reference types.
// -----------------------------------------------------------------

/// An image reference extracted from a template file, before PURL
/// construction.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ImageRef {
    /// The exact string extracted (post-regex, pre-normalization).
    pub raw: String,
    /// Classification driving PURL type selection per research §R2.
    pub kind: ImageRefKind,
    /// The source template path this ref came from (relative to chart
    /// root). Multiple occurrences of the same ref emit multiple
    /// `PackageDbEntry` records (one per occurrence); the resolver's
    /// `ResolutionEvidence.occurrences` dedupes downstream per F4
    /// analyze-report remediation.
    pub source_path: String,
}

/// Classification of an image reference for PURL type selection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ImageRefKind {
    /// Ref contains a `sha256:<hex>` digest → `pkg:oci/...`.
    Digested {
        image: String,
        digest: String,
    },
    /// Ref contains a `:<tag>` version marker → `pkg:docker/...`.
    /// Docker Hub's `library/` prefix added for unqualified refs.
    Tagged {
        image: String,
        tag: String,
    },
    /// Ref contains one or more `{{ ... }}` blocks →
    /// `pkg:generic/<placeholder-slug>` with
    /// `mikebom:image-ref-unresolved = "true"` property.
    TemplatePlaceholder {
        /// URL-safe slug with `{{ ... }}` blocks replaced by
        /// `__PLACEHOLDER_N__` tokens.
        slug: String,
    },
}

// -----------------------------------------------------------------
// Milestone 188 (#455) — Intermediate component + error types.
// -----------------------------------------------------------------

/// Intermediate helm-reader emission before conversion to
/// `PackageDbEntry`.
#[allow(dead_code)] // chart_type/app_version/alias/condition retained for future annotation work
#[derive(Debug)]
pub(crate) enum HelmComponent {
    /// The chart itself — Chart.yaml top-level entry.
    Chart {
        name: String,
        version: String,
        chart_type: String,
        app_version: Option<String>,
        source_file: String,
    },
    /// A declared chart dep (from Chart.yaml) or locked chart dep
    /// (from Chart.lock — takes precedence per FR-004).
    ChartDep {
        name: String,
        version: String,
        repo: String,
        source_kind: ChartDepSource,
        alias: Option<String>,
        condition: Option<String>,
    },
    /// Image reference extracted from a template file.
    Image {
        image_ref: ImageRef,
    },
}

/// Which source file supplied this chart-dep entry.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ChartDepSource {
    ChartYaml,
    /// Chart.lock authoritative per FR-004.
    ChartLock,
    /// Recursive descent through `charts/*.tgz` per FR-005.
    ChartsTarball,
}

/// Errors surfaced by helm parsing. Distinct from anyhow so the caller
/// can pattern-match specific failure classes for WARN vs error
/// dispatch.
#[derive(Debug, thiserror::Error)]
pub(crate) enum HelmParseError {
    #[error("failed to read Chart.yaml at {path}: {source}")]
    ChartYamlRead {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse Chart.yaml at {path}: {source}")]
    ChartYamlParse {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("failed to parse Chart.lock at {path}: {source}")]
    #[allow(dead_code)] // wired in T009 (US1 impl)
    ChartLockParse {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("failed to process subchart tarball at {path}: {reason}")]
    #[allow(dead_code)] // wired in T012 (US1 impl)
    SubchartTarballFailed { path: String, reason: String },
    #[error("--helm-chart tarball {path} extraction failed: {reason}")]
    #[allow(dead_code)] // wired in T016 (US1 impl)
    HelmChartTarballInvalid { path: String, reason: String },
}

// -----------------------------------------------------------------
// Milestone 188 (#455) — CLI-mode enum + entry point.
// -----------------------------------------------------------------

/// Milestone 188 (#455) — Helm reader entry point.
///
/// Auto-detects a Helm chart at `<rootfs>/Chart.yaml` (research §R1);
/// when absent, returns `Ok(vec![])` — non-Helm scans see zero drift
/// per FR-016 / SC-005 byte-identity guarantee.
///
/// Sequence when a chart is detected:
///   1. Phase A — parse `Chart.yaml` + `Chart.lock` + recursive
///      `charts/*.tgz` subcharts.
///   2. Phase B (or C under `HelmRenderMode::OptIn`) — extract image
///      refs from `templates/*.yaml` + `crds/*.yaml`.
///   3. Phase D — convert `HelmComponent`s to `PackageDbEntry`s.
///
/// Sequence:
///   1. Phase A — parse `Chart.yaml` + `Chart.lock` + recursive
///      `charts/*.tgz` subcharts.
///   2. Phase B (or C under `HelmRenderMode::OptIn`) — extract image
///      refs from `templates/*.yaml` + `crds/*.yaml`.
///   3. Phase D — convert `HelmComponent`s to `PackageDbEntry`s.
#[allow(dead_code)] // wired into read_all in T016
pub fn read(
    rootfs: &Path,
    render_mode: HelmRenderMode,
    diagnostics: &mut ScanDiagnostics,
) -> anyhow::Result<Vec<PackageDbEntry>> {
    let chart_yaml_path = rootfs.join("Chart.yaml");
    // Auto-detect gate per research §R1. Non-Helm scans see zero drift
    // (FR-016 / SC-005 byte-identity guarantee).
    if !chart_yaml_path.is_file() {
        return Ok(Vec::new());
    }

    // Phase A — chart-level enumeration.
    let mut components = read_chart_at(rootfs, 0)?;

    // Phase B or C — line-based OR rendered image-ref extraction per
    // m188 contracts/extraction-pipeline.md §Phase C flow diagram.
    // Milestone 203 (#553): `HelmRenderMode::OptIn` triggers the
    // rendered subprocess path via `extract_image_refs_rendered`.
    // Every failure class (BinaryNotFound, NonZeroExit, Timeout,
    // IoError) falls back to `extract_image_refs_unrendered` with a
    // WARN log per FR-007 + Constitution Principle III.
    let (image_refs, extraction_mode) = match render_mode {
        HelmRenderMode::OptIn => {
            let timeout = resolve_render_timeout();
            match extract_image_refs_rendered(rootfs, timeout) {
                Ok(refs) => (refs, HelmExtractionMode::Rendered),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        chart_dir = %rootfs.display(),
                        "helm-render failed; falling back to unrendered extraction"
                    );
                    (
                        extract_image_refs_unrendered(rootfs),
                        HelmExtractionMode::Unrendered,
                    )
                }
            }
        }
        HelmRenderMode::Off => (
            extract_image_refs_unrendered(rootfs),
            HelmExtractionMode::Unrendered,
        ),
    };
    components.extend(image_refs.into_iter().map(|r| HelmComponent::Image {
        image_ref: r,
    }));

    // Phase D — mark ScanDiagnostics for document-scope annotation.
    diagnostics.helm_extraction_mode = Some(extraction_mode);

    Ok(components_to_package_db_entries(components))
}

// -----------------------------------------------------------------
// Phase A — chart-level enumeration.
// -----------------------------------------------------------------

/// Parse `<chart_dir>/Chart.yaml` into `ChartMetadata`.
fn parse_chart_yaml(chart_dir: &Path) -> Result<ChartMetadata, HelmParseError> {
    let path = chart_dir.join("Chart.yaml");
    let path_display = path.to_string_lossy().into_owned();
    let content = std::fs::read_to_string(&path).map_err(|source| {
        HelmParseError::ChartYamlRead {
            path: path_display.clone(),
            source,
        }
    })?;
    serde_yaml::from_str(&content).map_err(|source| HelmParseError::ChartYamlParse {
        path: path_display,
        source,
    })
}

/// Parse `<chart_dir>/Chart.lock` if present. Returns `None` when the
/// file is absent (this is normal — no chart lock). On parse failure,
/// log WARN + return `None` per contracts §Phase A — Chart.lock is
/// best-effort and does not propagate errors.
fn parse_chart_lock(chart_dir: &Path) -> Option<ChartLock> {
    let path = chart_dir.join("Chart.lock");
    if !path.is_file() {
        return None;
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_yaml::from_str::<ChartLock>(&content) {
            Ok(lock) => Some(lock),
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "failed to parse Chart.lock; falling back to Chart.yaml-only version resolution"
                );
                None
            }
        },
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "failed to read Chart.lock; falling back to Chart.yaml-only version resolution"
            );
            None
        }
    }
}

/// Resolve a chart-dep's version per FR-004 precedence: Chart.lock
/// wins when it has a matching `(name, repository)` entry. Returns
/// `(version, source_kind)` for downstream `mikebom:evidence-kind`
/// emission.
fn resolve_chart_dep(dep: &ChartDep, lock: Option<&ChartLock>) -> (String, ChartDepSource) {
    if let Some(lock) = lock {
        for locked in &lock.dependencies {
            if locked.name == dep.name && locked.repository == dep.repository {
                return (locked.version.clone(), ChartDepSource::ChartLock);
            }
        }
    }
    (dep.version.clone(), ChartDepSource::ChartYaml)
}

/// Build a `pkg:helm/<namespace>/<name>@<version>` PURL. When
/// `repo_or_alias` is `None`, falls back to `pkg:generic/<name>@<version>`
/// with a WARN log per contracts §Phase A. The alias case (`@`-prefixed)
/// preserves the alias verbatim; URL case uses the host portion.
fn build_helm_purl(name: &str, version: &str, repo_or_alias: Option<&str>) -> Option<Purl> {
    let namespace = match repo_or_alias {
        Some(s) if s.starts_with('@') => s.to_string(),
        Some(url) => match url::Url::parse(url) {
            Ok(u) => match u.host_str() {
                Some(host) => host.to_string(),
                None => {
                    tracing::warn!(
                        %url,
                        %name,
                        "chart dep repository URL has no host; falling back to pkg:generic/"
                    );
                    return Purl::new(&format!("pkg:generic/{name}@{version}")).ok();
                }
            },
            Err(_) => {
                // Not a URL and not `@`-prefixed — could be a bare
                // registry name (`bitnami`). Preserve verbatim.
                url.to_string()
            }
        },
        None => {
            tracing::warn!(
                %name,
                %version,
                "chart dep has no repository or alias; falling back to pkg:generic/"
            );
            return Purl::new(&format!("pkg:generic/{name}@{version}")).ok();
        }
    };
    Purl::new(&format!("pkg:helm/{namespace}/{name}@{version}")).ok()
}

/// Enumerate + emit HelmComponents for a chart directory at
/// `chart_dir`. `depth` tracks recursion into `charts/*.tgz` subcharts.
fn read_chart_at(
    chart_dir: &Path,
    depth: usize,
) -> Result<Vec<HelmComponent>, HelmParseError> {
    let metadata = parse_chart_yaml(chart_dir)?;
    let lock = parse_chart_lock(chart_dir);
    let chart_yaml_path = chart_dir
        .join("Chart.yaml")
        .to_string_lossy()
        .into_owned();

    let mut components = Vec::new();

    // (a) Emit the chart itself.
    components.push(HelmComponent::Chart {
        name: metadata.name.clone(),
        version: metadata.version.clone(),
        chart_type: metadata.chart_type.clone(),
        app_version: metadata.app_version.clone(),
        source_file: chart_yaml_path.clone(),
    });

    // (b) For each declared dep, resolve version (Chart.lock wins) +
    // emit ChartDep.
    for dep in &metadata.dependencies {
        let (version, source_kind) = resolve_chart_dep(dep, lock.as_ref());
        let repo = dep
            .alias
            .clone()
            .or_else(|| dep.repository.clone())
            .unwrap_or_default();
        components.push(HelmComponent::ChartDep {
            name: dep.name.clone(),
            version,
            repo,
            source_kind,
            alias: dep.alias.clone(),
            condition: dep.condition.clone(),
        });
    }

    // (c) Recurse into `charts/*.tgz` subcharts.
    if depth < MAX_CHART_RECURSION_DEPTH {
        let charts_dir = chart_dir.join("charts");
        if charts_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&charts_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("tgz") {
                        match process_subchart_tgz(&path, depth + 1) {
                            Ok(sub_components) => components.extend(sub_components),
                            Err(e) => {
                                tracing::warn!(
                                    path = %path.display(),
                                    error = %e,
                                    "failed to process subchart tarball; parent chart continues"
                                );
                            }
                        }
                    }
                }
            }
        }
    } else {
        tracing::warn!(
            depth = MAX_CHART_RECURSION_DEPTH,
            path = %chart_dir.display(),
            "chart-recursion depth cap reached; deeper subcharts skipped"
        );
    }

    Ok(components)
}

/// Extract a `charts/<subchart>.tgz` to a tempdir + recursively read
/// its content. Returns the HelmComponents from the subchart tree.
fn process_subchart_tgz(
    tgz_path: &Path,
    depth: usize,
) -> Result<Vec<HelmComponent>, HelmParseError> {
    let tempdir = tempfile::Builder::new()
        .prefix("mikebom-helm-subchart-")
        .tempdir()
        .map_err(|e| HelmParseError::SubchartTarballFailed {
            path: tgz_path.to_string_lossy().into_owned(),
            reason: format!("tempdir creation failed: {e}"),
        })?;
    let bytes = std::fs::read(tgz_path).map_err(|e| HelmParseError::SubchartTarballFailed {
        path: tgz_path.to_string_lossy().into_owned(),
        reason: format!("read failed: {e}"),
    })?;
    let gz = GzDecoder::new(std::io::Cursor::new(bytes));
    let mut archive = tar::Archive::new(gz);
    archive
        .unpack(tempdir.path())
        .map_err(|e| HelmParseError::SubchartTarballFailed {
            path: tgz_path.to_string_lossy().into_owned(),
            reason: format!("tar unpack failed: {e}"),
        })?;

    // Chart tarballs by convention wrap contents in a single top-level
    // dir named after the chart (e.g., `mychart-1.0.0.tgz` extracts
    // to `mychart/`). Find that dir.
    let sub_root = find_subchart_root(tempdir.path()).ok_or_else(|| {
        HelmParseError::SubchartTarballFailed {
            path: tgz_path.to_string_lossy().into_owned(),
            reason: "no Chart.yaml found at extracted root".to_string(),
        }
    })?;

    // Read the subchart. Mark all deps with source = ChartsTarball to
    // distinguish from primary-chart deps.
    let mut sub_components =
        read_chart_at(&sub_root, depth).map_err(|e| HelmParseError::SubchartTarballFailed {
            path: tgz_path.to_string_lossy().into_owned(),
            reason: format!("recursive read failed: {e}"),
        })?;
    // Retag the source_kind on ChartDeps that came from THIS subchart's
    // Chart.yaml → ChartsTarball (since they're transitively packaged).
    for component in &mut sub_components {
        if let HelmComponent::ChartDep { source_kind, .. } = component {
            if matches!(source_kind, ChartDepSource::ChartYaml) {
                *source_kind = ChartDepSource::ChartsTarball;
            }
        }
    }
    Ok(sub_components)
}

/// Find the single top-level directory under `extracted` containing
/// `Chart.yaml`. Returns `None` if there are 0 or 2+ candidates.
fn find_subchart_root(extracted: &Path) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(extracted).ok()?;
    let mut chart_dirs: Vec<std::path::PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join("Chart.yaml").is_file() {
            chart_dirs.push(path);
        }
    }
    if chart_dirs.len() == 1 {
        chart_dirs.pop()
    } else {
        None
    }
}

// -----------------------------------------------------------------
// Phase B — line-based image-ref extraction (US2).
// -----------------------------------------------------------------

/// Regex per contracts §Phase B. Captures leading whitespace, the
/// image value (may include `{{ ... }}` blocks), and an optional
/// trailing comment.
static IMAGE_REGEX: OnceLock<Regex> = OnceLock::new();

fn image_regex() -> &'static Regex {
    IMAGE_REGEX.get_or_init(|| {
        // Matches `image: <value>` at the start of a line, tolerating
        // YAML list-item prefixes (`- image: ...`), optional quoting
        // (single or double), Go-template blocks (`{{ ... }}`), and
        // trailing comments (`# ...`).
        Regex::new(
            r#"(?m)^\s*(?:-\s+)?image:\s*['"]?([^'"\s{]*(?:\{\{[^}]+\}\}[^'"\s]*)*)['"]?\s*(?:#.*)?$"#,
        )
        .expect("image ref regex compiles")
    })
}

/// Classify a raw image-ref string per research §R2. See `ImageRefKind`.
fn classify_image_ref(raw: &str) -> ImageRefKind {
    if raw.contains("{{") {
        // Templated — build a slug replacing each `{{ ... }}` block
        // with `__PLACEHOLDER_N__`.
        let re = Regex::new(r"\{\{[^}]+\}\}").expect("placeholder regex compiles");
        let mut idx = 0;
        let slug: String = re
            .replace_all(raw, |_: &regex::Captures| {
                let n = idx;
                idx += 1;
                format!("__PLACEHOLDER_{n}__")
            })
            .to_string();
        return ImageRefKind::TemplatePlaceholder { slug };
    }
    // Digested — `@sha256:<hex>` shape.
    if let Some(at_pos) = raw.rfind('@') {
        let digest = &raw[at_pos + 1..];
        if digest.starts_with("sha256:") {
            let image = raw[..at_pos].to_string();
            return ImageRefKind::Digested {
                image: normalize_image_name(&image),
                digest: digest.to_string(),
            };
        }
    }
    // Tagged — `<name>:<tag>` shape (or bare `<name>` → tag=`latest`).
    if let Some(colon_pos) = raw.rfind(':') {
        let (name, tag) = raw.split_at(colon_pos);
        let tag = &tag[1..]; // strip the colon
        // Distinguish "port in URL" (registry.io:5000/foo:v1) from
        // "no tag" (registry.io:5000/foo). The last `:` splits at the
        // TAG boundary only if the substring AFTER it doesn't contain
        // `/`. If it does, the `:` is a port separator, and the ref
        // has no tag.
        if !tag.contains('/') && !tag.is_empty() {
            return ImageRefKind::Tagged {
                image: normalize_image_name(name),
                tag: tag.to_string(),
            };
        }
    }
    // No tag, no digest, no placeholder — Helm default is `latest`.
    ImageRefKind::Tagged {
        image: normalize_image_name(raw),
        tag: "latest".to_string(),
    }
}

/// Add Docker Hub's `library/` prefix for unqualified image names.
/// Qualified (`ghcr.io/foo/bar`, `docker.io/library/nginx`, etc.) →
/// preserve verbatim.
fn normalize_image_name(image: &str) -> String {
    // Qualified iff contains `/` (namespaced) OR contains `.` before
    // any `/` (registry host).
    let first_slash = image.find('/');
    let has_registry_host = match first_slash {
        Some(pos) => image[..pos].contains('.') || image[..pos].contains(':'),
        None => false,
    };
    if first_slash.is_some() && (has_registry_host || image.contains('/')) {
        // Already qualified (namespaced or has registry host).
        image.to_string()
    } else {
        // Unqualified — Docker Hub library convention.
        format!("library/{image}")
    }
}

/// Enumerate `templates/**/*.yaml`, `templates/**/*.yml`, `crds/*.yaml`,
/// `crds/*.yml`; extract `image:` refs; return `Vec<ImageRef>` with one
/// entry per (raw ref, source path) tuple. Dedup happens downstream at
/// the resolver level per F4 remediation — reader emits per-occurrence.
fn extract_image_refs_unrendered(chart_dir: &Path) -> Vec<ImageRef> {
    let mut out = Vec::new();
    for subdir in ["templates", "crds"] {
        let target = chart_dir.join(subdir);
        if !target.is_dir() {
            continue;
        }
        collect_image_refs_recursive(&target, chart_dir, &mut out);
    }
    out
}

fn collect_image_refs_recursive(
    dir: &Path,
    chart_root: &Path,
    out: &mut Vec<ImageRef>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_image_refs_recursive(&path, chart_root, out);
            continue;
        }
        let is_yaml = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e == "yaml" || e == "yml")
            .unwrap_or(false);
        if !is_yaml {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "helm template file unreadable; skipping"
                );
                continue;
            }
        };
        let rel = path
            .strip_prefix(chart_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        for caps in image_regex().captures_iter(&content) {
            if let Some(m) = caps.get(1) {
                let raw = m.as_str().trim().to_string();
                if raw.is_empty() {
                    continue;
                }
                let kind = classify_image_ref(&raw);
                out.push(ImageRef {
                    raw,
                    kind,
                    source_path: rel.clone(),
                });
            }
        }
    }
}

// -----------------------------------------------------------------
// Phase D — HelmComponent → PackageDbEntry conversion.
// -----------------------------------------------------------------

fn components_to_package_db_entries(components: Vec<HelmComponent>) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    for c in components {
        match c {
            HelmComponent::Chart {
                name,
                version,
                chart_type: _,
                app_version: _,
                source_file,
            } => {
                if let Ok(purl) = Purl::new(&format!("pkg:helm/local/{name}@{version}")) {
                    let mut extra: BTreeMap<String, serde_json::Value> = Default::default();
                    extra.insert(
                        "mikebom:source-mechanism".to_string(),
                        serde_json::Value::String("helm-chart-yaml".to_string()),
                    );
                    out.push(build_helm_entry(
                        purl,
                        name,
                        version,
                        source_file,
                        "helm-chart-yaml".to_string(),
                        extra,
                    ));
                }
            }
            HelmComponent::ChartDep {
                name,
                version,
                repo,
                source_kind,
                alias: _,
                condition: _,
            } => {
                let repo_opt = if repo.is_empty() { None } else { Some(repo.as_str()) };
                if let Some(purl) = build_helm_purl(&name, &version, repo_opt) {
                    let mut extra: BTreeMap<String, serde_json::Value> = Default::default();
                    let (evidence_kind, mechanism) = match source_kind {
                        ChartDepSource::ChartYaml => ("helm-chart-yaml", "helm-chart-yaml"),
                        ChartDepSource::ChartLock => ("helm-chart-lock", "helm-chart-lock"),
                        ChartDepSource::ChartsTarball => ("helm-chart-yaml", "helm-charts-tgz"),
                    };
                    extra.insert(
                        "mikebom:source-mechanism".to_string(),
                        serde_json::Value::String(mechanism.to_string()),
                    );
                    if matches!(source_kind, ChartDepSource::ChartLock) {
                        extra.insert(
                            "mikebom:helm-lock-authoritative".to_string(),
                            serde_json::Value::String("true".to_string()),
                        );
                    }
                    out.push(build_helm_entry(
                        purl,
                        name,
                        version,
                        String::new(),
                        evidence_kind.to_string(),
                        extra,
                    ));
                }
            }
            HelmComponent::Image { image_ref } => {
                let (purl_opt, extra) = image_ref_to_purl_and_annotations(&image_ref);
                if let Some(purl) = purl_opt {
                    let name = purl_display_name(&image_ref.kind);
                    let version = purl_display_version(&image_ref.kind);
                    out.push(build_helm_entry(
                        purl,
                        name,
                        version,
                        image_ref.source_path,
                        "helm-template-image-ref".to_string(),
                        extra,
                    ));
                }
            }
        }
    }
    out
}

fn image_ref_to_purl_and_annotations(
    image_ref: &ImageRef,
) -> (Option<Purl>, BTreeMap<String, serde_json::Value>) {
    let mut extra: BTreeMap<String, serde_json::Value> = Default::default();
    extra.insert(
        "mikebom:source-mechanism".to_string(),
        serde_json::Value::String("helm-template-image-ref".to_string()),
    );
    let purl_str = match &image_ref.kind {
        ImageRefKind::Digested { image, digest } => {
            format!("pkg:oci/{image}@{digest}")
        }
        ImageRefKind::Tagged { image, tag } => {
            format!("pkg:docker/{image}@{tag}")
        }
        ImageRefKind::TemplatePlaceholder { slug } => {
            extra.insert(
                "mikebom:image-ref-unresolved".to_string(),
                serde_json::Value::String("true".to_string()),
            );
            extra.insert(
                "mikebom:image-ref-raw".to_string(),
                serde_json::Value::String(image_ref.raw.clone()),
            );
            // Slug may contain characters not URL-safe for a PURL name
            // segment; use a simple sanitizer.
            let safe_slug: String = slug
                .chars()
                .map(|c| if c == ':' || c == '/' { '_' } else { c })
                .collect();
            format!("pkg:generic/{safe_slug}")
        }
    };
    (Purl::new(&purl_str).ok(), extra)
}

fn purl_display_name(kind: &ImageRefKind) -> String {
    match kind {
        ImageRefKind::Digested { image, .. } | ImageRefKind::Tagged { image, .. } => {
            image.clone()
        }
        ImageRefKind::TemplatePlaceholder { slug } => slug.clone(),
    }
}

fn purl_display_version(kind: &ImageRefKind) -> String {
    match kind {
        ImageRefKind::Digested { digest, .. } => digest.clone(),
        ImageRefKind::Tagged { tag, .. } => tag.clone(),
        ImageRefKind::TemplatePlaceholder { .. } => String::new(),
    }
}

fn build_helm_entry(
    purl: Purl,
    name: String,
    version: String,
    source_path: String,
    evidence_kind: String,
    extra_annotations: BTreeMap<String, serde_json::Value>,
) -> PackageDbEntry {
    PackageDbEntry {
        build_inclusion: None,
        purl,
        name,
        version,
        arch: None,
        source_path,
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type: None,
        buildinfo_status: None,
        evidence_kind: Some(evidence_kind),
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
        // Helm charts are declarative deployment manifests, not build
        // products. Design-tier per m122 convention.
        sbom_tier: Some("design".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    // ── T007 US1 — chart-yaml + chart-lock unit tests ──

    #[test]
    fn chart_yaml_parses_minimal_shape() {
        let content = "name: hello\nversion: 1.0.0\n";
        let meta: ChartMetadata = serde_yaml::from_str(content).unwrap();
        assert_eq!(meta.name, "hello");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.chart_type, "application"); // default
        assert!(meta.dependencies.is_empty());
    }

    #[test]
    fn chart_yaml_parses_full_shape() {
        let content = r#"
apiVersion: v2
name: myapp
version: 1.2.3
appVersion: "2.0.0"
type: application
description: A test chart
keywords:
  - kubernetes
  - test
home: https://example.com
dependencies:
  - name: postgres
    version: 11.0.0
    repository: https://charts.bitnami.com/bitnami
  - name: redis
    version: 17.0.0
    repository: "@bitnami"
    alias: cache
    condition: cache.enabled
"#;
        let meta: ChartMetadata = serde_yaml::from_str(content).unwrap();
        assert_eq!(meta.name, "myapp");
        assert_eq!(meta.app_version.as_deref(), Some("2.0.0"));
        assert_eq!(meta.keywords, vec!["kubernetes", "test"]);
        assert_eq!(meta.dependencies.len(), 2);
        assert_eq!(meta.dependencies[1].alias.as_deref(), Some("cache"));
    }

    #[test]
    fn chart_lock_takes_precedence_over_chart_yaml() {
        let dep = ChartDep {
            name: "postgres".to_string(),
            version: "11.0.0".to_string(),
            repository: Some("https://charts.bitnami.com/bitnami".to_string()),
            alias: None,
            condition: None,
        };
        let lock = ChartLock {
            dependencies: vec![ChartDep {
                name: "postgres".to_string(),
                version: "11.9.5".to_string(),
                repository: Some("https://charts.bitnami.com/bitnami".to_string()),
                alias: None,
                condition: None,
            }],
            digest: None,
            generated: None,
        };
        let (version, source) = resolve_chart_dep(&dep, Some(&lock));
        assert_eq!(version, "11.9.5");
        assert!(matches!(source, ChartDepSource::ChartLock));
    }

    #[test]
    fn resolve_chart_dep_falls_back_to_yaml_when_no_lock_match() {
        let dep = ChartDep {
            name: "postgres".to_string(),
            version: "11.0.0".to_string(),
            repository: Some("https://charts.bitnami.com/bitnami".to_string()),
            alias: None,
            condition: None,
        };
        let (version, source) = resolve_chart_dep(&dep, None);
        assert_eq!(version, "11.0.0");
        assert!(matches!(source, ChartDepSource::ChartYaml));
    }

    #[test]
    fn chart_dep_with_url_repo_produces_correct_purl() {
        let purl = build_helm_purl(
            "nginx",
            "13.0.0",
            Some("https://charts.bitnami.com/bitnami"),
        )
        .unwrap();
        assert_eq!(purl.as_str(), "pkg:helm/charts.bitnami.com/nginx@13.0.0");
    }

    #[test]
    fn chart_dep_with_alias_repo_uses_alias() {
        let purl = build_helm_purl("nginx", "13.0.0", Some("@bitnami")).unwrap();
        assert!(purl.as_str().contains("@bitnami"));
        assert!(purl.as_str().contains("nginx@13.0.0"));
    }

    #[test]
    fn chart_dep_with_no_repo_falls_back_to_generic() {
        let purl = build_helm_purl("nginx", "13.0.0", None).unwrap();
        assert_eq!(purl.as_str(), "pkg:generic/nginx@13.0.0");
    }

    // ── T018 US2 — image-ref regex + classification unit tests ──

    #[test]
    fn image_ref_regex_extracts_tagged() {
        let content = "        image: nginx:1.27.0\n";
        let caps = image_regex().captures(content).unwrap();
        assert_eq!(&caps[1], "nginx:1.27.0");
        let kind = classify_image_ref("nginx:1.27.0");
        assert!(matches!(kind, ImageRefKind::Tagged { .. }));
    }

    #[test]
    fn image_ref_regex_extracts_digested() {
        let raw = "nginx@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let content = format!("      image: {raw}\n");
        let caps = image_regex().captures(&content).unwrap();
        assert_eq!(&caps[1], raw);
        let kind = classify_image_ref(raw);
        assert!(matches!(kind, ImageRefKind::Digested { .. }));
    }

    #[test]
    fn image_ref_regex_extracts_placeholder() {
        let raw = "{{ .Values.image.repository }}:{{ .Values.image.tag }}";
        let content = format!("      image: \"{raw}\"\n");
        let caps = image_regex().captures(&content).unwrap();
        assert_eq!(&caps[1], raw);
        let kind = classify_image_ref(raw);
        assert!(matches!(kind, ImageRefKind::TemplatePlaceholder { .. }));
    }

    #[test]
    fn image_ref_extracts_mixed_placeholder_and_literal() {
        let raw = "registry.example.com/{{ .Values.image.name }}:v1.2.3";
        let kind = classify_image_ref(raw);
        assert!(matches!(kind, ImageRefKind::TemplatePlaceholder { .. }));
    }

    #[test]
    fn image_ref_regex_handles_quoted_and_unquoted() {
        let unquoted = "      image: nginx:1.27.0\n";
        let quoted = "      image: \"nginx:1.27.0\"\n";
        assert_eq!(image_regex().captures(unquoted).unwrap()[1].to_string(), "nginx:1.27.0");
        assert_eq!(image_regex().captures(quoted).unwrap()[1].to_string(), "nginx:1.27.0");
    }

    #[test]
    fn image_ref_regex_handles_trailing_comments() {
        let content = "      image: nginx:1.27.0  # deploy target\n";
        let caps = image_regex().captures(content).unwrap();
        assert_eq!(&caps[1], "nginx:1.27.0");
    }

    #[test]
    fn library_prefix_added_for_dockerhub_unqualified() {
        let kind = classify_image_ref("nginx:1.27.0");
        if let ImageRefKind::Tagged { image, tag } = kind {
            assert_eq!(image, "library/nginx");
            assert_eq!(tag, "1.27.0");
        } else {
            panic!("expected Tagged");
        }
    }

    #[test]
    fn library_prefix_not_added_for_registry_prefixed() {
        let kind = classify_image_ref("ghcr.io/foo/bar:v1");
        if let ImageRefKind::Tagged { image, tag } = kind {
            assert_eq!(image, "ghcr.io/foo/bar");
            assert_eq!(tag, "v1");
        } else {
            panic!("expected Tagged");
        }
    }

    // ── T007 m203 (#553) — HelmRenderError + resolve_render_timeout ──
    //
    // These tests mutate the process-wide `MIKEBOM_HELM_RENDER_TIMEOUT_SECS`
    // env var. Run with `cargo test ... -- --test-threads=1` when the full
    // suite is invoked, or scope invocation to this file via
    // `cargo test -- resolve_render_timeout` (which naturally serializes
    // matching tests). We avoid a `serial_test` dep per plan Technical
    // Context (zero-new-Cargo-deps).

    fn with_helm_render_timeout_env<F: FnOnce()>(value: Option<&str>, f: F) {
        let prev = std::env::var("MIKEBOM_HELM_RENDER_TIMEOUT_SECS").ok();
        match value {
            Some(v) => std::env::set_var("MIKEBOM_HELM_RENDER_TIMEOUT_SECS", v),
            None => std::env::remove_var("MIKEBOM_HELM_RENDER_TIMEOUT_SECS"),
        }
        f();
        match prev {
            Some(v) => std::env::set_var("MIKEBOM_HELM_RENDER_TIMEOUT_SECS", v),
            None => std::env::remove_var("MIKEBOM_HELM_RENDER_TIMEOUT_SECS"),
        }
    }

    #[test]
    fn resolve_render_timeout_default_when_env_var_absent_m203() {
        with_helm_render_timeout_env(None, || {
            assert_eq!(resolve_render_timeout(), Duration::from_secs(60));
        });
    }

    #[test]
    fn resolve_render_timeout_honors_env_var_m203() {
        with_helm_render_timeout_env(Some("42"), || {
            assert_eq!(resolve_render_timeout(), Duration::from_secs(42));
        });
    }

    #[test]
    fn resolve_render_timeout_clamps_below_min_m203() {
        with_helm_render_timeout_env(Some("0"), || {
            assert_eq!(resolve_render_timeout(), Duration::from_secs(1));
        });
    }

    #[test]
    fn resolve_render_timeout_clamps_above_max_m203() {
        with_helm_render_timeout_env(Some("99999"), || {
            assert_eq!(resolve_render_timeout(), Duration::from_secs(3600));
        });
    }

    #[test]
    fn resolve_render_timeout_ignores_parse_error_m203() {
        with_helm_render_timeout_env(Some("notanumber"), || {
            assert_eq!(resolve_render_timeout(), Duration::from_secs(60));
        });
    }

    #[test]
    fn helm_render_error_display_formats_all_variants_m203() {
        let e = HelmRenderError::BinaryNotFound;
        assert_eq!(format!("{e}"), "`helm` binary not found on $PATH");

        let e = HelmRenderError::NonZeroExit {
            code: 42,
            stderr_head: "boom".into(),
        };
        assert_eq!(
            format!("{e}"),
            "`helm template` exited with code 42; stderr head: boom"
        );

        let e = HelmRenderError::Timeout { timeout_secs: 7 };
        assert_eq!(format!("{e}"), "`helm template` exceeded 7s timeout");

        // T016a (CG1): IoError variant unit test.
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
        let e = HelmRenderError::from(io_err);
        assert_eq!(format!("{e}"), "`helm template` I/O error: nope");
    }

    #[test]
    fn cap_stderr_lines_truncates_at_max_m203() {
        let bytes = b"line1\nline2\nline3\nline4\nline5";
        let capped = cap_stderr_lines(bytes, 3);
        assert_eq!(capped, "line1\nline2\nline3");
    }

    #[test]
    fn cap_stderr_lines_handles_shorter_input_m203() {
        let bytes = b"only";
        let capped = cap_stderr_lines(bytes, 20);
        assert_eq!(capped, "only");
    }
}
