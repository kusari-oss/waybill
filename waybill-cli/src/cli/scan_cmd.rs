use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Args, ValueEnum};

use waybill_common::attestation::integrity::TraceIntegrity;
use waybill_common::attestation::metadata::GenerationContext;

use crate::enrich::clearly_defined_source::{
    enrich_components as cd_enrich_components, ClearlyDefinedSource,
};
use crate::enrich::deps_dev_client::DepsDevClient;
use crate::enrich::depsdev_source::{enrich_components, DepsDevSource, EnrichmentPass};
use crate::generate::{OutputConfig, ScanArtifacts, SerializerRegistry};
use crate::scan_fs;

/// Hard-coded default when the user passes no `--format` flag. Kept
/// as `cyclonedx-json` so pre-milestone-010 invocations behave exactly
/// as before (FR-004b).
const DEFAULT_FORMAT: &str = "cyclonedx-json";

/// Pseudo-format override key for the OpenVEX sidecar.
///
/// `openvex` is NOT a real `SbomSerializer` — it's a sidecar the
/// SPDX 2.3 serializer co-emits when a scan produces VEX statements
/// (FR-016a). The user cannot request it via `--format`; they can
/// only retarget its output path via `--output openvex=<path>` when
/// an SPDX format is also requested. Using it without SPDX, or
/// naming it in `--format`, is rejected in `resolve_dispatch` with
/// a clear error.
const OPENVEX_PSEUDO_FORMAT: &str = "openvex";

/// Format ids whose serializers co-emit the OpenVEX sidecar, in the order
/// that picks which document names it (#1443): one sidecar per run, named
/// after the first of these the run requests. Both SPDX serializers emit it.
const OPENVEX_EMITTING_FORMATS: &[&str] =
    &["spdx-2.3-json", "spdx-3-json", "spdx-3-json-experimental"];

/// Enrichment source identifiers for `--enrich-sources`. Selected via
/// comma-separated list; when provided, only the listed sources run.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[clap(rename_all = "kebab-case")]
pub enum EnrichSource {
    /// deps.dev license enrichment (declared + observed licenses).
    DepsDev,
    /// ClearlyDefined concluded-license enrichment.
    ClearlyDefined,
    /// deps.dev transitive dep-graph edge enrichment.
    DepsDevGraph,
}

/// Image source for `--image <ref>` resolution. Selected via
/// `--image-src` (comma-separated, in order of preference).
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageSource {
    /// Local docker daemon: shell out to `docker image inspect` to
    /// probe, then `docker save` to materialize a tarball.
    Docker,
    /// Milestone 206 (#440) — local podman image cache. Filesystem-only
    /// (no daemon/REST API). Requires the target image be pre-pulled
    /// via `podman pull` or `podman build`. Rootless preferred;
    /// rootful supported when waybill has read access to
    /// `/var/lib/containers/storage/`. Linux-only per spec Assumption 1.
    Podman,
    /// OCI distribution-spec registry pull (the milestone-031+
    /// `oci_pull` path).
    Remote,
}

/// Milestone 186 (#442) — where waybill should get the SBOM: scan the image
/// bytes or fetch a pre-existing SBOM via the OCI Distribution Spec v1.1
/// Referrers API.
///
/// Applies only to registry-pull scans (`--image <oci-ref>`). Rejected
/// (non-zero exit) against `--image <local-path>` and `--path <dir>` inputs
/// per FR-011.
#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[clap(rename_all = "kebab-case")]
pub enum SbomSourceMode {
    /// Default. Always scan the image bytes; never query the Referrers API.
    /// Preserves pre-m186 behavior byte-identically per FR-015 / SC-004.
    #[default]
    Scan,
    /// Query the Referrers API and REQUIRE that an SBOM referrer be found.
    /// Exit non-zero with an actionable error if absent per FR-009 / SC-003.
    /// Use for compliance workflows requiring upstream-published SBOMs only.
    Referrer,
    /// Query the Referrers API and prefer any matching SBOM referrer; fall
    /// through to scan silently if none available (or if any fetch step
    /// fails) per FR-008 / SC-002.
    Either,
}

/// Milestone 232 (#660) — `--tier=<mode>` output-filter flag. Filters
/// the emitted SBOM's component set by `sbom_tier` per operator
/// choice. See `specs/232-tier-filter-flag/spec.md` for the mode
/// inventory and downstream-consumer rationale.
///
/// Default is `All` (no-op filter). Per FR-002 / SC-003, pre-232
/// invocations that don't set the flag produce byte-identical output.
#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[clap(rename_all = "kebab-case")]
pub enum TierMode {
    /// Default: emit all resolved components regardless of tier.
    /// Byte-identical to pre-232 emission for any given scan input.
    #[default]
    All,
    /// Emit only components tagged `sbom_tier: "source"`. Recommended
    /// for vulnerability-scanner pipelines that want resolved versions
    /// only.
    SourceOnly,
    /// Emit only components tagged `sbom_tier: "design"`. Recommended
    /// for compliance-attribution pipelines that want the developer-
    /// declared graph without resolver-tier probes.
    DesignOnly,
    /// Emit components tagged `sbom_tier: "source"` OR "binary".
    /// Recommended for container-artifact pipelines that want
    /// everything "actually shipped" but not "declared but not
    /// resolved".
    SourceAndBinary,
}

/// Milestone 665 — modes for the `--no-binary-scan=<MODE>` flag.
///
/// Names a subset of binary-content-scanning readers to skip at pilot
/// registration time. v1 recognizes `Go` only; the enum is designed
/// for extension without CLI-surface churn. See `specs/665-no-binary-
/// scan-flag/data-model.md` for the entity contract.
///
/// Future variants (reserved; NOT currently emitted):
/// - `Elf`  — skip m096 ELF `.dep-v0` section reader only.
/// - `Symbols` — skip m099 symbol fingerprinting only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum BinaryScanMode {
    /// **Deprecated as a perf lever** (#781). The go_binary reader
    /// now short-circuits on non-binary files via a magic-byte
    /// prefilter, so the default scan is already fast. `--no-binary-scan=go`
    /// still works as a component filter (suppresses `pkg:golang/*`
    /// components from BuildInfo probing) but emits a WARN log
    /// pointing at `--no-binary-scan=all` for operators who want to
    /// drop the entire binary tier. See specs/665 FR-002 for the
    /// original semantics.
    #[clap(name = "go")]
    Go,
    /// Skip the entire binary-scanning tier — no `go_binary` BuildInfo
    /// probing AND no m104 role classification (which today opens every
    /// file in the tree to check magic bytes even on non-Go projects).
    /// Trades ALL binary-derived provenance (`pkg:golang/*` from
    /// BuildInfo, ELF/Mach-O/PE role tagging, embedded linkage
    /// attribution) for scan speed on large trees. Components emitted
    /// from other tiers (dpkg/apk/rpm/pip/etc.) keep their tier data
    /// unchanged. See issue #775.
    #[clap(name = "all")]
    All,
}

impl BinaryScanMode {
    /// The canonical string used in `waybill:binary-scan-suppressed`
    /// annotation values AND in `--help` output AND in FR-009
    /// error messages. Kept in sync with the `#[clap(name = ...)]`
    /// attributes above — single source of truth per data-model.md V1.
    pub fn as_annotation_value(&self) -> &'static str {
        match self {
            Self::Go => "go",
            Self::All => "all",
        }
    }
}

/// Milestone 188 (#455) — resolve `--helm-chart <path>` input.
///
/// When `<path>` ends in `.tgz`, extract to a tempdir + find the
/// top-level chart directory (contains `Chart.yaml`); return that
/// path. When `<path>` is a directory, return it verbatim.
///
/// Exits non-zero per FR-017 on: non-existent path, tarball extraction
/// failure, tarball with no `Chart.yaml` at extracted top-level. A
/// directory input WITHOUT `Chart.yaml` is NOT an error (matches
/// `--path` semantics — other package-DB readers still run).
fn resolve_helm_chart_input(
    helm_chart_path: &std::path::Path,
    tempdir_holder: &mut Option<tempfile::TempDir>,
) -> anyhow::Result<PathBuf> {
    if !helm_chart_path.exists() {
        anyhow::bail!(
            "--helm-chart path {} not found",
            helm_chart_path.display()
        );
    }
    // Directory input: pass through.
    if helm_chart_path.is_dir() {
        return Ok(helm_chart_path.to_path_buf());
    }
    // Tarball input: must end in `.tgz`.
    let is_tgz = helm_chart_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("tgz"))
        .unwrap_or(false);
    if !is_tgz {
        anyhow::bail!(
            "--helm-chart path {} is not a directory or a .tgz tarball",
            helm_chart_path.display()
        );
    }
    let tempdir = tempfile::Builder::new()
        .prefix("waybill-helm-chart-")
        .tempdir()
        .with_context(|| "creating tempdir for --helm-chart tarball extraction")?;
    let bytes = std::fs::read(helm_chart_path).with_context(|| {
        format!(
            "--helm-chart tarball {} could not be read",
            helm_chart_path.display()
        )
    })?;
    let gz = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
    let mut archive = tar::Archive::new(gz);
    archive.unpack(tempdir.path()).with_context(|| {
        format!(
            "--helm-chart tarball {} could not be extracted",
            helm_chart_path.display()
        )
    })?;
    // Find the top-level directory containing Chart.yaml. Helm's
    // `helm package` output wraps content in `<chart-name>/`.
    let chart_root = find_helm_chart_root(tempdir.path()).ok_or_else(|| {
        anyhow::anyhow!(
            "--helm-chart tarball {} extracted successfully but no Chart.yaml \
             found at top-level directory (expected <chart-name>/Chart.yaml)",
            helm_chart_path.display()
        )
    })?;
    *tempdir_holder = Some(tempdir);
    Ok(chart_root)
}

fn find_helm_chart_root(extracted: &std::path::Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(extracted).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join("Chart.yaml").is_file() {
            return Some(path);
        }
    }
    None
}

/// Milestone 186 — wire-format string for the `--sbom-source` value used in
/// FR-011 error messages. Matches the `#[clap(rename_all = "kebab-case")]`
/// convention on `SbomSourceMode`.
fn sbom_source_mode_wire_str(mode: SbomSourceMode) -> &'static str {
    match mode {
        SbomSourceMode::Scan => "scan",
        SbomSourceMode::Referrer => "referrer",
        SbomSourceMode::Either => "either",
    }
}

/// Milestone 186 — map a requested `--format` value to the referrer
/// descriptor media type waybill expects to see for byte-identical emission.
/// Returns `None` for `--format` values without a canonical referrer media
/// type mapping (SPDX 3 currently — the ecosystem hasn't converged on a
/// stable media type registration as of m186's landing). Used at emission
/// time to detect format-vs-media-type mismatch and emit the FR-004 WARN log.
///
/// This helper is a local mirror of
/// `scan_fs::oci_pull::referrers::media_type_for_mikebom_format` because that
/// function is `pub(super)`-scoped inside the oci_pull submodule and not
/// reachable from `cli`. Keeping the two in-sync is a static invariant —
/// any new mapping added in one must be added in the other (small enough
/// surface that duplication is cheaper than exposing an entire helper API).
fn referrer_media_type_for_format(fmt: &str) -> Option<&'static str> {
    match fmt {
        "cyclonedx-json" => Some("application/vnd.cyclonedx+json"),
        "spdx-2.3-json" => Some("application/spdx+json"),
        _ => None,
    }
}

/// Milestone 235 — Gradle transitive dependency resolution flags.
///
/// Flattened into `ScanArgs` via `#[command(flatten)]`. Spec:
/// `specs/235-gradle-transitive-ladder/spec.md` FR-001, FR-002, FR-003.
///
/// The 5 flags below all default to the safe/off state so waybill's
/// no-flag behavior is unchanged (`--gradle-resolve` is opt-in; the
/// other 4 have no effect without it).
///
/// Flag values are surfaced to the `gradle::read` reader via env vars
/// (`WAYBILL_GRADLE_*`) so we don't have to plumb through the
/// 75-callsite `scan_path` -> `read_all` signature chain (matches the
/// m102 `WAYBILL_INCLUDE_VENDORED` precedent at
/// `waybill-cli/src/scan_fs/package_db/mod.rs:1315`).
#[derive(Args, Debug, Default)]
pub struct GradleCliFlags {
    /// Opt-in: resolve Gradle transitive dependencies via `./gradlew
    /// :sub:dependencies` subprocess. Requires JDK on `$PATH`.
    /// Default: off (waybill falls back to m106 lockfile reading).
    #[arg(long)]
    pub gradle_resolve: bool,

    /// When `--gradle-resolve` is set, also resolve the buildscript
    /// classpath (Gradle plugins the build itself uses) via
    /// `./gradlew :sub:buildEnvironment`. Doubles subprocess call
    /// count. Default: off. Requires `--gradle-resolve`.
    #[arg(long, requires = "gradle_resolve")]
    pub gradle_resolve_buildscript: bool,

    /// When `--gradle-resolve` is set, use the Gradle daemon (faster
    /// on repeated invocations, but leaves a JVM in the operator's
    /// process list). Default: off (waybill passes `--no-daemon`).
    /// Requires `--gradle-resolve`.
    #[arg(long, requires = "gradle_resolve")]
    pub gradle_daemon: bool,

    /// Per-subprocess timeout in seconds. Applies to each
    /// `./gradlew :sub:dependencies` invocation individually.
    /// Default: 300 (5 minutes).
    #[arg(long, default_value_t = 300, value_parser = clap::value_parser!(u64).range(1..))]
    pub gradle_timeout_secs: u64,

    /// Additional Gradle configurations to resolve beyond the default
    /// `runtimeClasspath` + `testRuntimeClasspath` set. Repeatable
    /// (`--gradle-extra-configurations compileClasspath
    /// --gradle-extra-configurations testCompileClasspath`).
    /// Requires `--gradle-resolve`.
    #[arg(long, action = clap::ArgAction::Append)]
    pub gradle_extra_configurations: Vec<String>,
}

impl GradleCliFlags {
    /// Reject shell-metacharacter names to prevent injection via the
    /// `--configuration <name>` argument. Called by `SbomSubcommand::Scan`
    /// before waybill spawns any Gradle subprocess.
    pub fn validate_configuration_names(&self) -> Result<(), String> {
        for name in &self.gradle_extra_configurations {
            if name.chars().any(|c| {
                matches!(c, ' ' | ';' | '`' | '$' | '|' | '&' | '>' | '<' | '\n' | '\r')
            }) {
                return Err(format!(
                    "--gradle-extra-configurations value contains unsafe characters: {:?}",
                    name
                ));
            }
            if name.is_empty() {
                return Err("--gradle-extra-configurations values cannot be empty".to_string());
            }
        }
        Ok(())
    }

    /// Convert to the env vars the `gradle::read` reader consumes.
    /// Matches the m102 `WAYBILL_INCLUDE_VENDORED` env-bridge pattern
    /// (see `scan_cmd.rs:2562`).
    ///
    /// SAFETY: caller must ensure single-threaded execution at this
    /// point in the scan-cmd lifecycle (matches m102 precedent).
    pub fn export_env(&self) {
        // SAFETY: single-threaded at scan-cmd entry.
        unsafe {
            std::env::set_var(
                "WAYBILL_GRADLE_RESOLVE",
                if self.gradle_resolve { "1" } else { "0" },
            );
            std::env::set_var(
                "WAYBILL_GRADLE_RESOLVE_BUILDSCRIPT",
                if self.gradle_resolve_buildscript { "1" } else { "0" },
            );
            std::env::set_var(
                "WAYBILL_GRADLE_DAEMON",
                if self.gradle_daemon { "1" } else { "0" },
            );
            std::env::set_var(
                "WAYBILL_GRADLE_TIMEOUT_SECS",
                self.gradle_timeout_secs.to_string(),
            );
            // Configurations passed as comma-separated (the validator has
            // already rejected shell metacharacters, so comma is safe).
            std::env::set_var(
                "WAYBILL_GRADLE_EXTRA_CONFIGURATIONS",
                self.gradle_extra_configurations.join(","),
            );
        }
    }
}

/// Issue #927 (m923) — FR-001/FR-003/FR-004. Builds the deps.dev source with
/// the enrichment path the operator's flags select.
///
/// Extracted from the call site on purpose. The decision it encodes — absent
/// `--no-enrich-batch`, take the batched path — is the whole of this feature,
/// and it was previously expressed inline where no test could reach it. The
/// three m923 CLI tests asserted on clap's parse of the flag instead, and a
/// mutation of the inline wiring to `.with_batch(false)` left all three green
/// while every scan silently took the slow path. Routing construction through
/// a named function puts the wiring under test.
fn build_deps_dev_source(
    client: DepsDevClient,
    offline: bool,
    args: &ScanArgs,
) -> DepsDevSource {
    DepsDevSource::new(client, offline)
        // The legacy `--enrich-batch` is deliberately not consulted: it is a
        // no-op kept so existing scripts keep parsing (FR-004).
        .with_batch(!args.no_enrich_batch)
        .with_disk_cache(!args.enrich_no_cache, args.enrich_cache_max_age)
}

#[derive(Args, Debug)]
// Milestone 1283 — the Nix tiers `--nix-allow-ifd` applies to. `multiple`, so
// the two tiers stay combinable.
#[command(group(
    clap::ArgGroup::new("nix_tier")
        .args(["nix_closure", "nix_eval"])
        .multiple(true)
))]
pub struct ScanArgs {
    /// Directory to walk for package artifacts.
    ///
    /// Exactly one of `--path` or `--image` is required. The directory
    /// is traversed recursively; files with recognised package-artifact
    /// suffixes (`.deb`, `.crate`, `.whl`, `.tar.gz`, `.jar`, `.gem`, …)
    /// are stream-hashed and matched against the path resolver.
    #[arg(long, conflicts_with = "image")]
    pub path: Option<PathBuf>,

    /// `docker save`-format tarball to extract, overlay, and scan.
    ///
    /// Exactly one of `--path` or `--image` is required. The tarball is
    /// opened, layers extracted into a tempdir (whiteouts honoured),
    /// then the resulting rootfs is scanned exactly like `--path`.
    #[arg(long, conflicts_with = "path")]
    pub image: Option<PathBuf>,

    /// Image source-resolution order for `--image <ref>` (when the
    /// argument is an OCI reference, not a tarball path on disk).
    ///
    /// Comma-separated list; waybill tries each source in order and
    /// stops at the first one that has the image. Default
    /// `docker,remote` matches trivy's `--image-src` and syft's
    /// auto-detection: prefer the local docker daemon's cache, fall
    /// back to a registry pull. Pass `--image-src remote` to force
    /// a fresh registry fetch (skipping any locally-cached copy);
    /// pass `--image-src docker` to fail rather than touch the
    /// network.
    ///
    /// When `--image` resolves to an existing tarball file on disk,
    /// this flag is ignored — the file is loaded directly.
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "docker,podman,remote",
        value_name = "SRC[,SRC...]",
    )]
    pub image_src: Vec<ImageSource>,

    /// Override the platform that's resolved from a multi-arch image
    /// index. Only meaningful when `--image` points at a registry
    /// reference (not a pre-extracted tarball) — for tarballs the
    /// platform is fixed by whatever `docker save` already wrote.
    ///
    /// Format: `<os>/<arch>` or `<os>/<arch>/<variant>`. Only `linux`
    /// is supported as the OS — waybill's package-database readers
    /// (dpkg / apk / rpm) are linux-rootfs-shaped, so non-Linux
    /// container images aren't a meaningful scan target.
    ///
    /// Common values: `linux/amd64`, `linux/arm64`, `linux/arm/v7`,
    /// `linux/arm/v6`, `linux/386`, `linux/ppc64le`, `linux/s390x`.
    /// When omitted (default), waybill auto-resolves to
    /// `linux/<host-arch>` matching the machine running the scan.
    ///
    /// Use case: a macOS arm64 dev machine scanning a `linux/amd64`
    /// container image deployed to AWS, or Linux x86_64 CI scanning
    /// an `arm64` image deployed to Graviton.
    #[arg(long, requires = "image", value_name = "linux/ARCH[/VARIANT]")]
    pub image_platform: Option<String>,

    /// Disable the OCI blob cache for registry pulls. Equivalent to
    /// `WAYBILL_OCI_CACHE=0`. When set, every blob (config + layer)
    /// is fetched from the registry on every scan, even if waybill
    /// has already cached the same digest from a previous pull.
    /// Cache files on disk are untouched.
    ///
    /// Use case: CI lanes that want pure one-shot semantics, or
    /// debugging a registry-side regression.
    #[arg(long)]
    pub no_oci_cache: bool,
    /// Cap (in bytes) for the on-disk OCI blob cache. When the cache
    /// exceeds this size, oldest-mtime entries are evicted until the
    /// total drops below the cap. Default: 10 GB. Equivalent env
    /// var: `WAYBILL_OCI_CACHE_SIZE=<bytes>`.
    ///
    /// Cache location is resolved from (in priority order):
    /// `$WAYBILL_OCI_CACHE_DIR`, `$XDG_CACHE_HOME/waybill/oci-layers`,
    /// `$HOME/Library/Caches/waybill/oci-layers` on macOS, otherwise
    /// `$HOME/.cache/waybill/oci-layers`.
    #[arg(long, value_name = "BYTES")]
    pub oci_cache_size: Option<u64>,

    /// Issue #235 — directory containing Docker-format registry
    /// credentials. waybill probes `<DIR>/config.json`,
    /// `<DIR>/.dockerconfigjson` (K8s `kubernetes.io/dockerconfigjson`
    /// secret type), and `<DIR>/.dockercfg` (legacy K8s
    /// `kubernetes.io/dockercfg` secret type) in that order; the
    /// first readable+parseable file wins. The file format is the
    /// standard Docker `config.json` shape (`auths`, `credsStore`,
    /// `credHelpers`), so the existing credential-resolution
    /// precedence applies inside the loaded config.
    ///
    /// Use this when running waybill in a container that mounts a
    /// K8s `imagePullSecrets`-derived volume (typically at
    /// `/var/run/secrets/registry/`). For local/CI use with the
    /// standard Docker keychain, leave this unset — waybill falls
    /// back to `$DOCKER_CONFIG/config.json` or
    /// `$HOME/.docker/config.json`.
    ///
    /// Composes with `WAYBILL_REGISTRY_<HOST>_USERNAME/_PASSWORD`
    /// env vars (per-registry, higher priority than the directory
    /// probe) and `WAYBILL_REGISTRY_USERNAME/_PASSWORD` (generic
    /// fallback, also higher priority than the directory probe).
    /// See `docs/reference/identifiers.md` for the full credential
    /// resolution priority chain.
    #[arg(long = "registry-credentials-dir", value_name = "PATH")]
    pub registry_credentials_dir: Option<std::path::PathBuf>,

    /// `--insecure-registry <HOST[:PORT]>` — repeatable. When set, waybill
    /// pulls from the named host over `http://` instead of `https://`.
    /// Host-only form matches any port; explicit `<host>:<port>` matches
    /// only that port. Match target is the user-facing registry name
    /// typed in `--image` — NOT any resolved endpoint (docker.io does
    /// not auto-expand to registry-1.docker.io).
    ///
    /// Milestone 182 / FR-001. Consumers: Harbor devenv
    /// (`--insecure-registry core:8080`), air-gapped mirrors, local dev
    /// registries.
    ///
    /// **Security**: only enable for registries you trust. Plain-HTTP
    /// exposes credentials + blobs to network observers.
    #[arg(
        long = "insecure-registry",
        value_name = "HOST[:PORT]",
        action = clap::ArgAction::Append
    )]
    pub insecure_registry: Vec<String>,

    /// `--registry-ca-cert <PATH>` — repeatable. Additional CA
    /// certificate(s) to trust for HTTPS registry pulls, on top of the
    /// webpki root set. Each file may be a PEM bundle (multiple
    /// concatenated `-----BEGIN CERTIFICATE-----` blocks); ALL
    /// certificates in each file are added.
    ///
    /// Milestone 182 / FR-002 / FR-006. Consumers: private-CA Harbor
    /// deployments, corporate Nexus / JFrog, self-hosted GHCR mirrors.
    ///
    /// **Failure modes** (fail-fast at scan startup, before any network
    /// call): file not found, empty file, non-PEM content, malformed
    /// PEM. All surface actionable errors naming the offending path.
    #[arg(
        long = "registry-ca-cert",
        value_name = "PATH",
        action = clap::ArgAction::Append
    )]
    pub registry_ca_cert: Vec<std::path::PathBuf>,

    /// `--insecure-tls-skip-verify` — disable TLS certificate chain /
    /// hostname / expiry verification for ALL HTTPS registry pulls in
    /// this scan. Emits a WARN-level structured log at scan start
    /// (Constitution Principle X + FR-007).
    ///
    /// Milestone 182 / FR-003. **Security**: extremely dangerous in
    /// production. Use ONLY for CI/dev against self-signed or
    /// hostname-mismatched certs where fetching the CA is impractical.
    /// For private-CA production registries, prefer
    /// `--registry-ca-cert <path>` instead.
    #[arg(long = "insecure-tls-skip-verify")]
    pub insecure_tls_skip_verify: bool,

    /// Milestone 186 (#442) — where waybill should get the SBOM: scan the
    /// image bytes, or fetch a pre-existing SBOM via the OCI Distribution
    /// Spec v1.1 Referrers API.
    ///
    /// - `scan` (default): Always scan the image bytes. Preserves pre-m186
    ///   behavior byte-identically. No network activity on the Referrers
    ///   endpoint.
    /// - `referrer`: REQUIRE a matching SBOM referrer. Exit non-zero if
    ///   absent. Use for compliance workflows requiring upstream-published
    ///   SBOMs only.
    /// - `either`: Prefer a referrer if available; fall through to scan
    ///   silently if none. Cost-effective for images that publish SBOMs.
    ///
    /// Applies only to registry-pull scans. Rejected when used against
    /// `--image <local-tarball-path>` or `--path` scans.
    #[arg(long = "sbom-source", value_enum, default_value_t = SbomSourceMode::Scan)]
    pub sbom_source: SbomSourceMode,

    /// Milestone 188 (#455) — Helm chart tarball or directory to scan.
    ///
    /// When `<path>` ends in `.tgz`, waybill extracts the tarball to a
    /// tempdir and runs the scan pipeline against the extracted
    /// contents. When it is a directory, behavior is identical to
    /// `--path <path>` — Chart.yaml is auto-detected regardless.
    ///
    /// The `.tgz` MUST contain a `Chart.yaml` at the top-level
    /// extracted directory; otherwise waybill exits non-zero.
    /// Directory inputs without Chart.yaml don't cause an error —
    /// other package-DB readers (npm / cargo / etc.) still run.
    ///
    /// Composes freely with all other package-DB readers per
    /// Clarifications Q1.
    #[arg(long = "helm-chart", value_name = "PATH_OR_TGZ", conflicts_with = "path")]
    pub helm_chart: Option<PathBuf>,

    /// Milestone 188 (#455) — Opt-in Helm template rendering.
    ///
    /// When set, waybill shells out to `helm template <chart-dir>`
    /// before extracting container image references, resolving every
    /// `{{ .Values.image.tag }}` placeholder to a concrete value.
    /// Requires the `helm` binary on `$PATH`.
    ///
    /// On failure (missing binary, non-zero exit, timeout), waybill
    /// emits a WARN log and falls back to the default unrendered
    /// extraction — the scan does NOT abort. The emitted SBOM's
    /// document-scope `waybill:image-extraction-completeness`
    /// annotation surfaces whether extraction was "partial" (fallback)
    /// or "full" (helm succeeded).
    ///
    /// Timeout: 60 seconds by default; override via
    /// `WAYBILL_HELM_RENDER_TIMEOUT_SECS=<n>` env var.
    ///
    /// Default (flag omitted): NO helm binary invocation. Zero
    /// external-tool calls per FR-013.
    #[arg(long = "helm-render", default_value_t = false)]
    pub helm_render: bool,

    /// Milestone 1034 (#971 part A) — Opt-in `nix eval` resolution tier.
    ///
    /// **THIS EXECUTES CODE.** Every other `sbom scan` flag reads
    /// files; this one starts `nix` and evaluates Nix expressions.
    /// Treat it the way you would treat running the project's build:
    /// **use it inside a sandbox, or only against a flake you trust.**
    ///
    /// The payoff is a more accurate SBOM. Parsing nixpkgs files
    /// reconstructs what Nix *would* compute; evaluating asks Nix what
    /// it *does* compute. Issue #1033 shipped two wrong versions
    /// because a file that supersedes the generated package set was
    /// not read — evaluation does not have that failure mode.
    ///
    /// What is evaluated today: nixpkgs, at the revision the project's
    /// `flake.lock` pins. The project's *own* flake is NOT evaluated,
    /// so expressions the scanned repository authors do not run. The
    /// code that does run is nixpkgs — third-party, unaudited by
    /// waybill — and the repository chooses which revision of it. That
    /// revision must be a 40-character hex object id or the tier
    /// degrades, so it cannot smuggle an expression in.
    ///
    /// Defences, all on by default and not configurable: evaluation
    /// runs in Nix's pure mode (the host environment is unreadable),
    /// with import-from-derivation refused so evaluation cannot build
    /// or run a builder, and waybill verifies that refusal is actually
    /// in effect first — a `nix` that does not support the setting
    /// accepts the request, ignores it, and exits 0, so asking is not
    /// evidence of getting. Evaluation is also bounded in wall-clock
    /// time, because Nix bounds recursion depth but not time.
    ///
    /// Requires the `nix` binary on `$PATH`. Every failure — absent
    /// tool, unusable daemon, unfetchable revision, no evaluable
    /// attribute, evaluation error, or budget exhaustion — degrades
    /// to file-parsing and records a reason in the emitted document.
    /// The scan does NOT abort.
    ///
    /// Default (flag omitted): no `nix` process is started at all.
    #[arg(long = "nix-eval", default_value_t = false)]
    pub nix_eval: bool,

    /// Milestone 1035 (#1034, #1040) — emit the Nix derivation closure.
    ///
    /// **THIS EXECUTES THE SCANNED PROJECT'S OWN FLAKE.** `--nix-eval`
    /// evaluates nixpkgs at a pinned revision and never runs
    /// repository-authored expressions; this does run them, because a
    /// derivation closure cannot be obtained without instantiating the
    /// project's flake. Use it inside a sandbox, or only against a
    /// flake you trust.
    ///
    /// What it buys: the components a Nix build actually consumed, and
    /// the security patches nixpkgs backported without moving a version
    /// string. Measured on two real Haskell libraries — 216 and 218
    /// components absent from today's output, and `unzip 6.0` carrying
    /// 11 CVEs across 26 patches that no version-keyed SBOM can express.
    ///
    /// Build tooling is emitted too, marked with its role, so a
    /// consumer can filter rather than lose it. The richest
    /// vulnerability signal in both measured closures was build
    /// tooling.
    ///
    /// Defences are milestone 1034's, unchanged and now load-bearing:
    /// pure mode, import-from-derivation refused and the refusal
    /// verified (unless `--nix-allow-ifd`), no `--accept-flake-config`,
    /// and a wall-clock budget.
    ///
    /// Default (flag omitted): no `nix` process is started at all.
    ///
    /// System configurations (milestone 1066): a flake with no `packages`
    /// output and exactly one `darwinConfigurations` or `nixosConfigurations`
    /// entry has that configuration's system closure taken. With several,
    /// nothing is chosen and the scan says why; name one with
    /// `--nix-closure-attr`. A configuration's closure is what the machine is
    /// *built from*, not what is installed on it. Whenever a requested
    /// closure is not recorded, the document says why
    /// (`waybill:nix-closure-degraded`).
    ///
    /// The outputs of closure members that are already present in the nix
    /// store are also read for the Go modules and Rust crates compiled into
    /// their binaries, which are not derivations and would otherwise be
    /// missing. Nothing is built or fetched to do this, so the result
    /// depends on the machine: build the package first to have its contents
    /// listed. `waybill:nix-closure-contents` records what was read. See
    /// `--nix-closure-require-root-contents`.
    #[arg(long = "nix-closure", default_value_t = false)]
    pub nix_closure: bool,

    /// The flake attribute whose closure to take: a name under
    /// `packages.<system>` (defaults to `default`), or a full output path
    /// whose first segment is a standard flake output, e.g.
    /// `darwinConfigurations.laptop.system` or
    /// `nixosConfigurations.web01.config.system.build.toplevel`. A full path
    /// is evaluated for its own platform.
    ///
    /// Attributes are never merged: a project exposing `default`,
    /// `pkg-ghc910`, `pkg-ghc94` and `pkg-ghc96` builds the same library
    /// against three compilers, and merging them would describe a build
    /// nobody performed.
    #[arg(
        long = "nix-closure-attr",
        value_name = "ATTR",
        requires = "nix_closure"
    )]
    pub nix_closure_attr: Option<String>,

    /// Fail the scan when the closure root's built output could not be read.
    ///
    /// `--nix-closure` reads the outputs of closure members that are already
    /// present in the store for the Go modules and Rust crates compiled into
    /// them. It never builds or fetches, so on a machine that has not built
    /// the package — a fresh CI runner — nothing is recovered, the scan
    /// still succeeds, and the document records that the root was not read
    /// (`waybill:nix-closure-contents`).
    ///
    /// With this flag that case is an error instead, and no document is
    /// written: use it where a document without the program's own
    /// dependencies would be mistaken for a complete one. It fails when the
    /// root's output is absent or unreadable, when the closure names no
    /// single root, and when the closure tier itself degraded. A root that
    /// was read and embeds nothing (a C program) passes.
    #[arg(
        long = "nix-closure-require-root-contents",
        default_value_t = false,
        requires = "nix_closure"
    )]
    pub nix_closure_require_root_contents: bool,

    /// Allow import-from-derivation during Nix evaluation. Nix will BUILD
    /// derivations while evaluating the flake (e.g. callCabal2nix). Do not use
    /// on an untrusted flake. Where nix's sandbox is off (the macOS default)
    /// those builds run unsandboxed.
    ///
    /// Requires `--nix-closure` or `--nix-eval`. Every Nix evaluation then
    /// passes `allow-import-from-derivation true`, and waybill confirms nix
    /// applied it before evaluating anything; if it cannot, the tier degrades
    /// (`ifd-allowance-unverified`). Nothing else is relaxed:
    /// `--accept-flake-config` and `--impure` stay refused, so a flake's own
    /// substituters and keys are still ignored. The document records the
    /// policy and nix's sandbox setting (`waybill:nix-evaluation-policy`).
    /// Milestone 1283.
    #[arg(long = "nix-allow-ifd", default_value_t = false, requires = "nix_tier")]
    pub nix_allow_ifd: bool,

    /// The Nix platform to evaluate for, e.g. `x86_64-linux`.
    ///
    /// Defaults to the host's. Naming it matters twice: results are
    /// platform-dependent (`hinotify` is 0.4.2 on `x86_64-linux` and
    /// 0.1.8 on `aarch64-darwin` at one nixpkgs revision), and passing
    /// it explicitly is what lets evaluation stay pure —
    /// `builtins.currentSystem` does not exist in pure mode.
    #[arg(long = "nix-eval-system", value_name = "SYSTEM", requires = "nix_eval")]
    pub nix_eval_system: Option<String>,

    /// Wall-clock budget for `nix` evaluation, in seconds.
    ///
    /// waybill imposes this because Nix does not: `max-call-depth`
    /// stops runaway recursion, and `timeout` is the *build* timeout,
    /// which this tier never reaches. A shallow non-recursive
    /// evaluation ran past 45 seconds unimpeded when measured.
    /// Acquiring the pinned revision is budgeted separately, so a cold
    /// Nix store does not consume the evaluation budget.
    ///
    /// There is no unbounded setting; `0` is rejected.
    #[arg(
        long = "nix-eval-timeout-secs",
        value_name = "SECS",
        requires = "nix_eval",
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub nix_eval_timeout_secs: Option<u64>,

    /// Output path override. Two forms are accepted:
    ///
    /// * Bare `--output <path>` — applies to the single requested
    ///   format. Rejected when more than one format is requested.
    /// * Per-format `--output <fmt>=<path>` — repeatable; each entry
    ///   overrides the default filename for exactly one format id.
    ///
    /// Per-format form is required for multi-format emission. When
    /// omitted, each format writes to its own default filename
    /// (`waybill.cdx.json`, `waybill.spdx.json`, …).
    #[arg(long, action = clap::ArgAction::Append, value_name = "[FMT=]PATH")]
    pub output: Vec<String>,

    /// Milestone 221 (feature 221-cisa-2026-elements-audit / US2a) —
    /// static-key signing.
    ///
    /// Path to a PEM-encoded ECDSA-P256 (default), Ed25519, or RSA
    /// private key. When set, waybill signs each emitted SBOM with
    /// the referenced key: CDX outputs receive a JSF (JSON Signature
    /// Format) object populated at `metadata.signature`; SPDX 2.3 and
    /// SPDX 3 outputs get a companion DSSE envelope at
    /// `<output>.sig.json`.
    ///
    /// Cloud-KMS (`kms://<uri>`) and PKCS#11 (`pkcs11://<uri>`)
    /// references are out of scope for this feature and deferred to
    /// a follow-up milestone (plan.md §Follow-ups).
    ///
    /// Requires `--output <file>` for every emitted format; combining
    /// with `--output -` (stdout) is rejected at parse time per
    /// FR-008a (signing without a durable output path defeats the
    /// signature's purpose).
    ///
    /// When absent, all emitters produce byte-identical output to
    /// today's goldens per FR-009 (no regression).
    #[arg(long = "sign-key", value_name = "PATH")]
    pub sign_key: Option<PathBuf>,

    /// Milestone 221 US2a — environment variable holding the
    /// passphrase for an encrypted PEM key file. When `--sign-key`
    /// references an encrypted key, this env var MUST be set.
    /// Defaults to `WAYBILL_SIGN_KEY_PASSPHRASE` if omitted.
    /// Never accepts a passphrase directly on the command line
    /// (avoids `ps`-visible leak).
    #[arg(long = "sign-key-passphrase-env", value_name = "NAME")]
    pub sign_key_passphrase_env: Option<String>,

    /// Milestone 222 US2b — Sigstore keyless SBOM signing. When set,
    /// waybill fetches an OIDC token (ambient GitHub Actions or
    /// explicit `SIGSTORE_ID_TOKEN`), submits it to Fulcio for a
    /// short-lived signing cert, signs the emitted SBOM bytes,
    /// uploads the signature to Rekor for transparency-log inclusion,
    /// and embeds the resulting Sigstore Bundle in
    /// `metadata.signature` (CDX) or writes a
    /// `<output>.sig.bundle.json` sidecar (SPDX 2.3 + SPDX 3).
    ///
    /// Mutually exclusive with `--sign-key` (a signed SBOM has ONE
    /// signature envelope). Requires `--output <file>` — combining
    /// with stdout is rejected at parse time per FR-008a.
    ///
    /// Endpoints are overridable via `WAYBILL_FULCIO_URL` and
    /// `WAYBILL_REKOR_URL` env vars (defaults: production Sigstore).
    /// Rekor timeout via `WAYBILL_REKOR_TIMEOUT_SECS` (default 30).
    ///
    /// When absent, all emitters produce byte-identical output to
    /// today's goldens per FR-015 (no regression on the default path).
    #[arg(long = "sign", conflicts_with = "sign_key")]
    pub sign: bool,

    /// Milestone 222 US2b — Fulcio endpoint override for Sigstore
    /// keyless signing. Only relevant with `--sign`; otherwise ignored.
    /// Defaults to production Sigstore (`https://fulcio.sigstore.dev`).
    /// Set to `https://fulcio.sigstage.dev` for staging.
    #[arg(
        long = "fulcio-url",
        env = "WAYBILL_FULCIO_URL",
        default_value = "https://fulcio.sigstore.dev",
        value_name = "URL"
    )]
    pub fulcio_url: String,

    /// Milestone 222 US2b — Rekor transparency-log endpoint override
    /// for Sigstore keyless signing. Only relevant with `--sign`;
    /// otherwise ignored. Defaults to production Sigstore
    /// (`https://rekor.sigstore.dev`). Set to
    /// `https://rekor.sigstage.dev` for staging.
    #[arg(
        long = "rekor-url",
        env = "WAYBILL_REKOR_URL",
        default_value = "https://rekor.sigstore.dev",
        value_name = "URL"
    )]
    pub rekor_url: String,

    /// Milestone 222 US2b — Rekor inclusion-proof timeout in seconds.
    /// Only relevant with `--sign`; otherwise ignored. Default 30s
    /// per FR-007 (Rekor is mandatory + fail-close on timeout).
    /// Bump if Sigstore is under abnormal load; do NOT set to 0
    /// (interpreted as "no timeout" would defeat FR-007's fail-close
    /// requirement).
    #[arg(
        long = "rekor-timeout-secs",
        env = "WAYBILL_REKOR_TIMEOUT_SECS",
        default_value_t = 30,
        value_name = "SECS"
    )]
    pub rekor_timeout_secs: u64,

    /// Milestone 221 US4 (feature 221-cisa-2026-elements-audit /
    /// FR-013) — SBOM document version.
    ///
    /// Positive integer matching CDX 1.6's `metadata.version` schema
    /// (`{"type": "integer", "minimum": 1}`). Wires the value into
    /// all three emitters: CDX `metadata.version` as a native
    /// integer, SPDX 2.3 as a document-scope `Annotation` carrying
    /// `waybill:sbom-version=<N>`, SPDX 3 as the same annotation
    /// shape on the `SpdxDocument` root IRI.
    ///
    /// When unset, CDX emits `metadata.version: 1` (today's default,
    /// byte-identical per FR-009); SPDX outputs do not emit a
    /// `waybill:sbom-version` annotation at all.
    ///
    /// Rejects non-integer values (`2.0`, `v2`, `latest`, empty
    /// string, embedded whitespace) and values < 1 at parse time
    /// per FR-014. CISA 2026 § SBOM Version allows semver at the
    /// author's discretion, but CDX 1.6 schema forbids it in this
    /// slot; the integer-only constraint is the intersection.
    ///
    /// Note: waybill's existing UUID `serialNumber` (CDX) +
    /// content-addressed `documentNamespace` / `@id` (SPDX per
    /// milestone 010) already satisfy CISA's RFC 9562 alternative
    /// pathway for revision identity. `--sbom-version` covers the
    /// monotonic-counter pathway consumers who key on
    /// `metadata.version` expect.
    #[arg(long = "sbom-version", value_name = "N")]
    pub sbom_version: Option<waybill_common::types::SbomVersion>,

    /// Milestone 215/219 — split monorepo SBOM into per-workspace-
    /// member OR per-directory sub-SBOMs.
    ///
    /// Accepts an optional value:
    ///
    /// - `--split` (bare) OR `--split=workspace` → per-main-module
    ///   grouping (m215 default; byte-identity preserved with pre-m219
    ///   behavior).
    /// - `--split=directory` → group all main-modules whose
    ///   canonicalized source dirs match into ONE sub-SBOM per dir.
    ///   Useful for polyglot repos where Cargo + package.json coexist.
    ///
    /// Requires `--output-dir <dir>`; incompatible with `--output`
    /// (a single file cannot hold N sub-SBOMs). On single-package
    /// projects with no workspace boundaries, falls back to one SBOM
    /// with a WARN log (FR-009). See
    /// `docs/reference/split-modes.md` for the mode table + worked
    /// examples.
    #[arg(
        long,
        value_enum,
        num_args = 0..=1,
        default_missing_value = "workspace",
        require_equals = true,
        conflicts_with = "output",
    )]
    pub split: Option<crate::generate::split::SplitMode>,

    /// Milestone 215 — output directory for split-mode sub-SBOMs +
    /// `split-manifest.json`. Required when `--split` is set; ignored
    /// otherwise. Directory is created if missing.
    #[arg(long = "output-dir", value_name = "DIR")]
    pub output_dir: Option<PathBuf>,

    /// Output format(s). Comma-separated list, and the flag itself
    /// is repeatable: `--format cyclonedx-json,spdx-2.3-json` is
    /// equivalent to `--format cyclonedx-json --format spdx-2.3-json`.
    /// Duplicates are ignored silently. Default: `cyclonedx-json`.
    ///
    /// Registered formats:
    /// - `cyclonedx-json` — CycloneDX 1.6 JSON (default filename
    ///   `waybill.cdx.json`).
    /// - `spdx-2.3-json` — SPDX 2.3 JSON (default filename
    ///   `waybill.spdx.json`).
    /// - `spdx-3-json` — SPDX 3.0.1 JSON-LD (default filename
    ///   `waybill.spdx3.json`). Full ecosystem coverage; production-
    ///   grade output with native-field + annotation parity vs.
    ///   CycloneDX and SPDX 2.3.
    /// - `spdx-3-json-experimental` [DEPRECATED] — deprecation alias
    ///   for `spdx-3-json`. Byte-identical output; prints a stderr
    ///   deprecation notice. Accepted through milestone 012;
    ///   removed in milestone 013. Set
    ///   `WAYBILL_NO_DEPRECATION_NOTICE=1` to suppress the warning
    ///   in CI logs during a controlled migration.
    #[arg(
        long,
        action = clap::ArgAction::Append,
        value_delimiter = ',',
        value_name = "FORMAT",
    )]
    pub format: Vec<String>,

    /// Maximum file size to hash (bytes). Larger files are skipped. The
    /// default (256 MB) covers the largest realistic package artifact.
    #[arg(long, default_value_t = scan_fs::walker::DEFAULT_SIZE_CAP_BYTES)]
    pub max_file_size: u64,

    /// Milestone 144 — per-file size cap for standalone `.rpm` files,
    /// in bytes. Files exceeding the cap are skipped with a structured
    /// WARN log (`reason="size-cap-exceeded"`). Useful for Yocto debug
    /// RPMs (kernel-dbg, gcc-dbg) which can exceed the 512 MiB default.
    /// Default: 536870912 (512 MiB).
    #[arg(long, value_name = "BYTES", value_parser = parse_nonzero_u64)]
    pub max_rpm_bytes: Option<u64>,

    /// Milestone 144 — override the distro identifier for RPM PURL
    /// namespaces. When set, overrides `/etc/os-release` `ID=` AND
    /// per-RPM RPMTAG_VENDOR/RPMTAG_PACKAGER metadata across the
    /// entire scan. Typical Yocto use: `--rpm-distro poky` to encode
    /// the build's DISTRO variable in emitted PURLs. Default:
    /// auto-detect (CLI override > /etc/os-release > per-RPM header
    /// > empty namespace).
    #[arg(long, value_name = "ID", value_parser = parse_non_empty_lowercase_distro_id)]
    pub rpm_distro: Option<String>,

    /// Omit per-component content hashes from the SBOM.
    #[arg(long)]
    pub no_hashes: bool,

    /// Optional distro codename to stamp on deb PURLs. Overrides the
    /// codename auto-detected from `<root>/etc/os-release` when set.
    /// Useful when scanning a directory that isn't itself a rootfs.
    #[arg(long)]
    pub deb_codename: Option<String>,

    /// Skip reading installed-package databases (`/var/lib/dpkg/status`,
    /// `/lib/apk/db/installed`). On by default because production
    /// container images routinely clean up `.deb`/`.apk` artefact caches
    /// and the db is then the only complete source of installed
    /// packages. Pass this flag to fall back to pure artefact-file
    /// scanning.
    #[arg(long)]
    pub no_package_db: bool,

    /// Issue #363 — operator assertion that declared licenses have been
    /// reviewed and may be promoted to `licenseConcluded`.
    ///
    /// **By passing this flag, YOU (the operator) ASSERT that you have
    /// reviewed and verified the declared license data for accuracy.**
    /// Per SPDX 2.3 § 7.13 / SPDX 3.0.1 `licenseConcluded` carries the
    /// analyst's reviewed conclusion; without this flag waybill leaves
    /// `licenseConcluded` as `NOASSERTION` (no human verified) even
    /// when `licenseDeclared` is populated — technically correct but
    /// invisible to consumers that key on `concluded` (sbomqs,
    /// Kusari Inspector, syft-comparators).
    ///
    /// When set, every component whose `concluded_licenses` is empty
    /// AND `licenses` is non-empty gets its declared licenses copied to
    /// concluded, plus a per-component
    /// `waybill:license-concluded-source = "operator-asserted"`
    /// annotation recording that the conclusion came from THIS FLAG (a
    /// formal operator assertion), not from external enrichment
    /// (ClearlyDefined / deps.dev). Components that already have a
    /// concluded license (e.g. from ClearlyDefined) are untouched.
    ///
    /// Default OFF — preserves pre-feature byte-identity. Don't pass
    /// this flag in unattended pipelines without an upstream review
    /// step; downstream consumers will treat your `licenseConcluded`
    /// as analyst-verified.
    #[arg(long)]
    pub conclude_licenses: bool,

    /// Skip per-file SHA-256 hashing of installed-package contents.
    /// Falls back to a fast SHA-256 over each package's dpkg `.md5sums`
    /// file (microseconds per package; component-level identity only,
    /// no per-file occurrences). Default-on hashing reads every file
    /// referenced by dpkg's `.list` manifest — proportional to
    /// installed size (~3-5 s on debian:bookworm-slim, ~30 s on full
    /// debian).
    #[arg(long)]
    pub no_deep_hash: bool,

    /// Container-scan perf optimization for `--image` mode. When set,
    /// waybill's docker-image extractor skips writing file contents for
    /// everything OUTSIDE the OS-package metadata allow-list
    /// (`/var/lib/{dpkg,apk,rpm}/`, `/usr/share/doc/*/copyright`,
    /// `/etc/os-release`, `/usr/lib/os-release`). Symlinks and
    /// directories are still preserved for correct path resolution.
    ///
    /// Requires `--no-deep-hash` (deep-hash reads the skipped file
    /// contents, so this flag would corrupt its output). Cuts the
    /// `docker_image::extract` step by ~90% on typical OS-only images
    /// (measured 690ms → ~50ms on debian:12-slim).
    ///
    /// NOT recommended when scanning application containers that ship
    /// source-tier content (Python venvs, node_modules, cargo vendor
    /// trees) inside the image — those readers will miss the component
    /// files when they're not extracted. For OS-image scans only.
    #[arg(long, requires = "no_deep_hash")]
    pub fast_container_extract: bool,

    /// Milestone 133 US1 — emit file-tier components for unattributed
    /// content (custom binaries, vendored libraries with no manifest,
    /// embedded archives) surviving the FR-005 content-shape allowlist.
    ///
    /// Modes:
    /// - `off` (US1.B default — preserves pre-milestone-133 byte-
    ///   identity): no file-tier emission.
    /// - `orphan` (US1.C default — planned flip): emit components only
    ///   for content NOT covered by any package-tier or binary-tier
    ///   component's `evidence.occurrences[]` paths or `hashes[]`
    ///   SHA-256 set (FR-011 hybrid dedupe).
    /// - `full`: emit a component for every regular file surviving the
    ///   FR-005 allowlist, regardless of dedupe coverage. Useful for
    ///   forensic / compliance use cases cataloguing every hash on
    ///   disk.
    /// - `source-tree` (m671): opt-in mode that surfaces source-code
    ///   file extensions (`.py`, `.c`, `.h`, etc. per the FR-002
    ///   21-extension allowlist) as file-tier components. Optional
    ///   restriction subset via `--file-inventory-source-shapes`.
    ///   Docs / configs / build-glue stay hard-excluded. See
    ///   `docs/reference/component-tiers.md` for the full flow.
    ///
    /// Emitted file-tier components carry a `waybill:component-tier =
    /// "file"` annotation and a `waybill:file-paths` JSON-encoded
    /// array of every observed path for the unique content.
    ///
    /// **Default flipped to `orphan` in milestone 133 US1.C per FR-015**
    /// — operators wanting pre-milestone-133 byte-identity on SBOMs
    /// opt out via `--file-inventory=off`. Image-scan SBOMs grow by
    /// roughly 180-440 file-tier components on a typical container
    /// image (the SC-001 acceptable range from the FR-022
    /// projection); orphan-mode scan time grows by ~5-15 seconds on
    /// a debian:bookworm-slim-sized rootfs.
    #[arg(long, value_name = "MODE", default_value = "orphan")]
    pub file_inventory: String,

    /// Maximum file size (bytes) considered for file-tier emission.
    /// Files larger than this are skipped; the document-level
    /// `waybill:file-inventory-skipped-oversize` annotation reports
    /// the skip count. Default 100 MB per FR-010.
    #[arg(long, default_value_t = 100 * 1024 * 1024)]
    pub file_inventory_size_limit: u64,

    /// Milestone 671 — comma-separated subset of the FR-002 source-
    /// shape allowlist that gets surfaced under
    /// `--file-inventory=source-tree`. Accepted extensions
    /// (case-insensitive, leading dot optional):
    /// `py, pyi, c, cc, cpp, cxx, h, hh, hpp, rs, go, java, kt, js, ts,
    /// rb, php, cs, swift, m, mm`.
    ///
    /// Unknown extensions fail loudly at CLI-parse time per FR-009
    /// (`SourceShapeParseError::UnknownExtension`); adding new
    /// extensions requires a follow-up milestone (curation review,
    /// not operator-time override).
    ///
    /// **Cross-arg constraint**: this flag is only meaningful under
    /// `--file-inventory=source-tree`. Combining it with any other
    /// mode value fails at parse time with a clear diagnostic (see
    /// `specs/671-file-tier-cpython/contracts/source_shape_restriction.md`).
    #[arg(long, value_name = "SHAPES", value_parser = parse_source_shape_restriction_arg)]
    pub file_inventory_source_shapes: Option<scan_fs::file_tier::source_shape::SourceShapeSet>,

    /// Print a JSON summary to stdout after writing the SBOM.
    #[arg(long)]
    pub json: bool,

    /// Enable ClearlyDefined enrichment (curated concluded licenses).
    /// Off by default since #930: it enriches components already found
    /// rather than establishing what is in the project, and it costs
    /// network time against a service with frequent multi-second stalls.
    /// Has no effect when `--offline` is set.
    #[arg(long, conflicts_with = "no_clearly_defined")]
    pub clearly_defined: bool,

    /// Skip ClearlyDefined enrichment. This has been the default since
    /// #930; the flag is still accepted so existing invocations keep
    /// working.
    #[arg(long)]
    pub no_clearly_defined: bool,

    /// Milestone 207 (#596): AGGREGATE disable — skip BOTH the
    /// deps.dev license enrichment AND the deps.dev transitive
    /// dep-graph enrichment. Combines `--no-deps-dev-license` and
    /// `--no-deps-dev-graph` semantics per operator expectation.
    ///
    /// Pre-m207 this flag disabled only the license path (keeping
    /// the dep-graph active). Scripts that relied on that behavior
    /// can migrate by renaming to `--no-deps-dev-license`.
    ///
    /// Composition: `--no-deps-dev` (aggregate) OR
    /// `--no-deps-dev-license` (license only) OR
    /// `--no-deps-dev-graph` (graph only). Fine-grained flags allow
    /// surgical control. `--enrich-sources <list>` (allowlist mode)
    /// overrides all `--no-*` flags. `--offline` suppresses all
    /// enrichment paths regardless of `--no-*` flags.
    #[arg(long)]
    pub no_deps_dev: bool,

    /// Milestone 207 (#596) — skip deps.dev LICENSE enrichment
    /// only. Keeps the deps.dev transitive dep-graph enrichment
    /// active. This is the pre-m207 semantic of `--no-deps-dev`;
    /// scripts that relied on that behavior can migrate by
    /// renaming `--no-deps-dev` → `--no-deps-dev-license`.
    ///
    /// Has no effect when `--offline` is set (offline suppresses
    /// all enrichment paths). Overridden by `--enrich-sources`
    /// allowlist mode when the operator supplies that flag.
    #[arg(long)]
    pub no_deps_dev_license: bool,

    /// Milestone 839 (#766) — accepted and ignored.
    ///
    /// Batched enrichment is the default as of issue #927, so this flag
    /// asks for what already happens. Kept accepted rather than removed
    /// so scripts that pass it keep working; erroring on an unknown
    /// argument would break them louder than ignoring it.
    ///
    /// To get the old behaviour, use `--no-enrich-batch`.
    #[arg(long, hide = true)]
    pub enrich_batch: bool,

    /// Issue #927 — fetch deps.dev licence metadata one request per
    /// component instead of in bulk via `GetVersionBatch`.
    ///
    /// **Batched is the default**, and is two orders of magnitude faster:
    /// measured at 8.5s against 1302s on a 2,291-package repository, for a
    /// byte-equivalent document. Per-component issues one request per
    /// component; batched issues one per hundred.
    ///
    /// The reason this was ever opt-in, and the reason the escape hatch
    /// stays: `GetVersionBatch` lives on deps.dev's `v3alpha` surface,
    /// which its own documentation says "may change in incompatible ways
    /// from time to time". That is a standing property of the upstream
    /// API, not a bedding-in period — waiting does not make it stable.
    /// What makes the default acceptable is that a batch failure falls
    /// back to this path automatically and costs speed, not coverage.
    /// This flag is for choosing that path up front.
    ///
    /// Has no effect when `--offline`, `--no-deps-dev` or
    /// `--no-deps-dev-license` is set — all of those suppress the licence
    /// path entirely.
    #[arg(long)]
    pub no_enrich_batch: bool,

    /// Milestone 839 (#766) — accept cached deps.dev enrichment up
    /// to this many seconds old, overriding the freshness bound the
    /// upstream response asks for.
    ///
    /// By default waybill honours the response's own
    /// `Cache-Control: max-age`, which deps.dev sets to 3600. That
    /// is deliberate: a pinned package version is immutable but
    /// deps.dev's *record about* it is not — licences get corrected,
    /// source links get added, and Go licences come from a scanner
    /// whose output moves when the scanner does. deps.dev offers no
    /// `ETag`, so a stale entry cannot be detected, only re-fetched.
    ///
    /// Raising this trades currency for speed. The value is recorded
    /// in each entry written under it, so entries fetched with a
    /// longer bound keep it and others are unaffected.
    #[arg(long, value_name = "SECONDS")]
    pub enrich_cache_max_age: Option<u64>,

    /// Milestone 839 (#766) — bypass the on-disk deps.dev enrichment
    /// cache for this scan, neither reading from it nor writing to
    /// it. Use for a cold measurement. To clear the cache instead,
    /// remove `~/.cache/waybill/deps-dev/`.
    #[arg(long)]
    pub enrich_no_cache: bool,

    /// Milestone 102 (FR-016/FR-017): include vendored C/C++
    /// dependencies declared via CMake `add_subdirectory(third_party/...)`
    /// or `add_subdirectory(vendor/...)`. Default OFF — these are
    /// frequently false positives (CMake's `add_subdirectory` is also
    /// used for first-party `src/` and `tests/` sub-modules) so we
    /// require explicit opt-in. When enabled, version is backfilled
    /// from a co-located `version.txt` or `.version` file when present;
    /// otherwise the PURL has no version segment. Also accepts
    /// `WAYBILL_INCLUDE_VENDORED=1` env var which is read directly by
    /// the milestone-102 C/C++ readers in `read_all` (no clap-level
    /// env binding here, so the env var accepts "1"/"true"/etc. without
    /// clap's bool-env strictness).
    #[arg(long)]
    pub include_vendored: bool,

    /// Extend the CMake reader's recursive descent to `third_party/`.
    /// By default (unset) `third_party/` is walked at depth-1 only
    /// (matching milestone-102 behavior); recursive descent applies
    /// only to `cmake/` and `Modules/`. Setting this flag treats
    /// `third_party/` the same way. Useful when the parent project
    /// has vendored a large dep tree (LLVM, Chromium, WebRTC, etc.)
    /// whose transitive `find_package` declarations should surface
    /// in the SBOM.
    ///
    /// Also accepts `WAYBILL_CMAKE_THIRD_PARTY_RECURSIVE=1` env var,
    /// read directly by the milestone-156 cmake reader (mirrors the
    /// `WAYBILL_INCLUDE_VENDORED` env-var propagation pattern).
    #[arg(long)]
    pub cmake_third_party_recursive: bool,

    /// Milestone 235 — Gradle transitive dependency resolution flags.
    /// See `GradleCliFlags` docstring for individual flag semantics.
    #[command(flatten)]
    pub gradle: GradleCliFlags,

    /// Skip the deps.dev transitive dep-graph enrichment step ONLY.
    /// Keeps deps.dev license enrichment active.
    ///
    /// Companion to `--no-deps-dev-license` (m207 #596) which does
    /// the reverse (skip license, keep graph). Use `--no-deps-dev`
    /// for the aggregate "skip both" semantic.
    ///
    /// Has no effect when `--offline` is set.
    #[arg(long)]
    pub no_deps_dev_graph: bool,

    /// Skip resolving Haskell dependency versions through the nixpkgs
    /// revision pinned in `flake.lock` (milestone 926, #947).
    ///
    /// Without this flag, a Nix-built Haskell project that ships no
    /// `cabal.project.freeze` still gets exact versions and source
    /// hashes for the dependencies the pinned revision carries.
    /// Resolution only runs when the repository both pins a
    /// nixpkgs-shaped input AND declares Haskell dependencies, so a
    /// repository meeting neither condition does no extra work.
    ///
    /// Distinct from `--offline`: this disables THIS feature only and
    /// leaves every other network enrichment active. `--offline`
    /// suppresses all of them, this one included.
    #[arg(long)]
    pub no_nixpkgs_haskell: bool,

    /// Skip the transitive runtime closure over the pinned nixpkgs, while
    /// still resolving the versions of DECLARED Haskell dependencies.
    ///
    /// Milestone 985 (#962). By default waybill also walks each declared
    /// dependency's own runtime dependencies through the pinned package set,
    /// which on measured projects multiplies the Haskell component count by
    /// 1.5–3.8× — on the largest, 162 components become 394. Every added
    /// component carries a version, a source hash, and a
    /// `waybill:nixpkgs-component-origin` of `transitive`.
    ///
    /// Set this when that growth is unwanted. Output then matches the
    /// pre-closure behaviour exactly.
    ///
    /// Narrower than `--no-nixpkgs-haskell`, which disables the whole pass
    /// including declared-dependency versions. The two are separately
    /// valuable: wanting versions without the closure is a reasonable
    /// position, and this flag is how to express it.
    #[arg(long)]
    pub no_nixpkgs_haskell_closure: bool,

    /// Skip reading what nixpkgs declares about the closure's packages.
    ///
    /// Milestone 1050 (#1039). Under `--nix-closure`, waybill evaluates each
    /// closure member's `meta.knownVulnerabilities` and emits what it finds
    /// as VEX and, for the entries naming no CVE, as a per-component
    /// annotation.
    ///
    /// Measured on a 1,275-derivation project, warm: the pass costs 706 ms
    /// against a 388 ms closure query, roughly doubling the scan. The
    /// absolute figure is under a second, which is why it still runs by
    /// default -- but the ratio is large enough that an operator who wants
    /// only the composition needs a way out, and this is it.
    ///
    /// Setting this does not affect the closure itself: components, patch
    /// attribution, and the patch-derived VEX statements are unchanged.
    #[arg(long)]
    pub no_nixpkgs_security: bool,

    /// Seconds to wait when retrieving the pinned nixpkgs package set
    /// before degrading to versionless output (milestone 926, #947).
    ///
    /// The default is a BUDGET, not a measurement: a warm fetch of the
    /// 16.6 MB package set measured ~1.0s during milestone-926
    /// research, and 30s is 30x that, sized for slow, proxied or
    /// internal-mirror links whose latency has not been measured.
    /// Raise it when pointing at a slow internal mirror.
    ///
    /// Exceeding the budget is not an error: the scan completes with
    /// the affected dependencies versionless and a recorded reason.
    #[arg(long, value_name = "SECS", default_value_t = 30)]
    pub nixpkgs_timeout_secs: u64,

    /// Comma-separated list of enrichment sources to enable. When
    /// provided, ONLY the listed sources run (overrides all
    /// `--no-clearly-defined` / `--no-deps-dev` / `--no-deps-dev-graph`
    /// flags). Has no effect when `--offline` is set — offline
    /// disables all network calls.
    ///
    /// Example: `--enrich-sources deps-dev,clearly-defined` enables
    /// license enrichment from both sources but skips dep-graph edges.
    #[arg(long, value_delimiter = ',', value_name = "SOURCE[,SOURCE...]")]
    pub enrich_sources: Vec<EnrichSource>,

    /// Path to a source-tier SBOM document (CDX 1.6 / SPDX 2.3 / SPDX 3
    /// JSON) that emitted components will be bound to per milestone 072
    /// (FR-011). When set, waybill emits a `waybill:source-document-binding`
    /// annotation on each first-party component whose PURL appears in
    /// the source SBOM, plus a document-level cross-document reference
    /// (CDX `externalReferences[type:bom]`, SPDX `externalDocumentRefs` +
    /// `DESCENDANT_OF` relationship).
    ///
    /// FR-011 transparency: when the file cannot be loaded or parsed,
    /// the scan exits non-zero rather than silently emitting components
    /// without binding. Components whose PURL has no source-tier
    /// counterpart get an explicit
    /// `binding: unknown { reason: "source-not-found-in-bind-target" }`
    /// marker per FR-003.
    ///
    /// Use `waybill sbom verify-binding --image-sbom <out> --source-sbom <path>`
    /// to verify the binding after emission.
    #[arg(long, value_name = "PATH")]
    pub bind_to_source: Option<PathBuf>,

    /// Milestone 111 (issue #225 Option A) — declare that the binary-
    /// tier PURL on the left should be treated as the source-tier PURL
    /// on the right when computing milestone-072 cross-tier binding.
    /// Format: `LHS_PURL=RHS_PURL`. Repeatable. Both PURLs are
    /// canonicalized; match against scan-output components is strict
    /// canonical-equality on the LHS side.
    ///
    /// Use when an operator's flagship binary inside an image lands as
    /// `pkg:generic/<name>` (no version) but the corresponding source-
    /// tier component carries the ecosystem-specific PURL like
    /// `pkg:cargo/<name>@<ver>` — without an alias, binding strength
    /// stays `unknown { reason: "source-not-found-in-bind-target" }`.
    ///
    /// Requires `--bind-to-source` to have effect. Supplied otherwise,
    /// waybill emits a warning and the alias is discarded (the scan
    /// proceeds; no alias is recorded in the emitted SBOM).
    ///
    /// Also settable via `WAYBILL_PKG_ALIAS` (comma-separated entries
    /// matching the per-flag syntax) for CI ergonomics.
    #[arg(
        long = "pkg-alias",
        value_name = "LHS=RHS",
        value_parser = waybill::binding::alias::parse_pkg_alias,
        action = clap::ArgAction::Append,
    )]
    pub pkg_alias: Vec<waybill::binding::alias::PurlAlias>,

    /// Attach a `repo:` identifier — source repository identity
    /// (URL or git-style ssh URL). Manual override; if both this
    /// flag and the auto-detected `repo:` identifier (from `.git/`
    /// origin remote) produce a value, manual wins per FR-006.
    /// On the same scan, pass `--git-ref <revision>` to upgrade
    /// to a `git:<repo-url>#<revision>` identifier (the `git:`
    /// identifier supersedes — no separate `repo:` is also emitted).
    #[arg(long = "repo", value_name = "URL")]
    pub repo: Option<String>,

    /// Pair with `--repo <url>` to emit a `git:<repo>#<revision>`
    /// identifier (commit/branch/tag-anchored). Cannot be supplied
    /// without `--repo`. When set, supersedes the bare `repo:`
    /// identifier — only the `git:` identifier is emitted.
    #[arg(long = "git-ref", value_name = "REVISION", requires = "repo")]
    pub git_ref: Option<String>,

    /// Attach an `image:` identifier — image identity in the form
    /// `[registry/]name[:tag][@sha256:digest]`. Manual override:
    /// if `--image <PATH>` (the scan input) is also set and
    /// auto-detection produced an `image:` identifier, the manual
    /// value wins per FR-006. Named `--image-id` to avoid colliding
    /// with the `--image <PATH>` scan-input flag.
    #[arg(long = "image-id", value_name = "REF")]
    pub image_id: Option<String>,

    /// Attach an `attestation:` identifier — in-toto attestation
    /// IRI. Manual only; no auto-detection equivalent.
    #[arg(long = "attestation", value_name = "IRI")]
    pub attestation: Option<String>,

    /// Attach a user-defined identifier in `<scheme>=<value>` form.
    /// Repeatable. The `<scheme>` MUST match regex
    /// `^[a-z][a-z0-9_-]*$` (FR-004) and MUST NOT collide with a
    /// built-in scheme (`repo`, `git`, `image`, `attestation`) —
    /// use the dedicated `--repo` / `--git-ref` / `--image-id` /
    /// `--attestation` flags for those. The `<value>` is the
    /// remainder after the first `=`; values may contain `=`
    /// characters.
    ///
    /// User-defined identifiers ride the `waybill:identifiers`
    /// document-level annotation per Constitution Principle V's
    /// documented-exception path; SPDX 3 carries them natively in
    /// `Element.externalIdentifier[]`.
    ///
    /// Worked example: `--id acme_corp_id=svc-alpha-123 --id
    /// internal_ticket=PROJ-456`.
    ///
    /// See `docs/reference/identifiers.md` for the full per-format
    /// carrier table and decode recipes.
    #[arg(
        long = "id",
        action = clap::ArgAction::Append,
        value_name = "SCHEME=VALUE",
        value_parser = parse_user_defined_id_flag,
    )]
    pub id: Vec<waybill::binding::identifiers::Identifier>,

    /// Preserve userinfo (e.g., `USER:TOKEN@host`) in auto-detected git
    /// remote URLs when constructing `repo:` and `git:` identifiers.
    /// By default, waybill strips userinfo to prevent accidental
    /// credential disclosure in published SBOMs. Use this flag only
    /// when the credentials are deliberately non-sensitive (e.g., a
    /// public read-only deploy token, internal-network-only
    /// credentials). Manual `--repo` / `--git-ref` / `--id` flag
    /// values are emitted verbatim regardless of this flag.
    #[arg(long)]
    pub keep_credentials_in_identifiers: bool,

    /// Attach a `subject:` identifier declaring "this SBOM describes
    /// the artifact with the given content hash." Format:
    /// `sha256:<64-lowercase-hex>` or `sha512:<128-lowercase-hex>`.
    /// Repeatable for multi-subject SBOMs. On build-tier scans
    /// (`waybill trace run`), subject identifiers are auto-detected
    /// from the in-toto attestation envelope's subject set; manual
    /// flags augment auto-detected entries (deduplicated by exact
    /// match per milestone 073). On source-tier and image-tier
    /// scans, no auto-detect runs; manual flags are the only source
    /// of `subject:` identifiers.
    #[arg(
        long = "subject-hash",
        action = clap::ArgAction::Append,
        value_name = "ALGO:HEX",
    )]
    pub subject_hash: Vec<String>,

    /// Attach a user-defined identifier to a specific component in the
    /// emitted SBOM. The PURL must byte-equal a component's `purl`
    /// field in the emitted output; the SCHEME must be a non-built-in
    /// scheme name (built-in schemes `repo`, `git`, `image`,
    /// `attestation`, `subject` are reserved for document-level use).
    /// Examples:
    ///
    /// `--component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-shared-lib-v2"`
    ///
    /// `--component-id "pkg:cargo/myapp@0.5.1=acme-asset:myapp-prod-001"`
    ///
    /// Repeatable. If a selector PURL matches multiple components
    /// (same PURL across different bom-ref values), the identifier is
    /// attached to ALL matching components. If a selector matches
    /// zero components, the scan logs a warning and continues.
    #[arg(
        long = "component-id",
        action = clap::ArgAction::Append,
        value_name = "PURL=SCHEME:VALUE",
        value_parser = waybill::binding::identifiers::component_id::parse_component_id_flag,
    )]
    pub component_id:
        Vec<waybill::binding::identifiers::component_id::ComponentIdentifierFlag>,

    /// Override the auto-derived `metadata.component.name` of the
    /// emitted SBOM. Useful when scanning an arbitrary directory whose
    /// basename doesn't reflect the operator-meaningful project
    /// identity. Accepts any non-empty UTF-8 except whitespace, control
    /// characters, `?`, and `#`. URL-encoded automatically when emitted
    /// into the PURL `name` segment.
    ///
    /// When this flag is set on a manifest-driven scan (Cargo, npm,
    /// pip, gem, Maven, Go), the manifest-derived main-module
    /// component is dropped entirely from the emitted SBOM (clean
    /// replacement). To preserve the manifest-derived identity as a
    /// regular library entry alongside the override, track GitHub
    /// issue #151.
    #[arg(
        long = "root-name",
        value_name = "NAME",
        value_parser = validate_root_name,
    )]
    pub root_name: Option<String>,

    /// Override the auto-derived `metadata.component.version`. Same
    /// validation rules as `--root-name`. Independent — can be set
    /// without `--root-name` and vice versa. When unset, falls through
    /// to the auto-derived version (typically `0.0.0` for arbitrary
    /// directories or the manifest-derived version for project scans).
    #[arg(
        long = "root-version",
        value_name = "VERSION",
        value_parser = validate_root_version,
    )]
    pub root_version: Option<String>,

    /// Override the type segment of the root component's PURL.
    /// Defaults to `generic` when `--root-name` is set; this flag
    /// replaces that default so the BOM subject's PURL carries an
    /// operator-supplied ecosystem type. Example: `--root-purl-type
    /// golang --root-name github.com/example/svc` produces
    /// `pkg:golang/github.com/example/svc@<version>`.
    ///
    /// Validated at parse time against the purl-spec type charset
    /// (`^[a-z][a-z0-9.+-]*$`). REQUIRES `--root-name`. Mutually
    /// exclusive with `--no-root-purl`.
    ///
    /// Applied identically across all three output formats: CDX
    /// `metadata.component.purl`, SPDX 2.3 root Package
    /// `externalRefs[purl]`, SPDX 3 root `software_packageUrl` +
    /// `externalIdentifier[packageUrl]`.
    #[arg(
        long = "root-purl-type",
        value_name = "TYPE",
        value_parser = validate_root_purl_type,
        requires = "root_name",
        conflicts_with = "no_root_purl",
    )]
    pub root_purl_type: Option<String>,

    /// Omit the root component's PURL entirely from the emitted SBOM.
    /// CDX: `metadata.component.purl` field absent. SPDX 2.3: no
    /// `purl` entry in the root Package's `externalRefs[]`. SPDX 3:
    /// no `software_packageUrl` AND no `externalIdentifier[]` entry
    /// with `externalIdentifierType: "packageUrl"`.
    ///
    /// Useful when downstream consumers key software identity on
    /// `(type, name)` and a target record was originally produced by
    /// a tool that emitted no root PURL — reproducing that empty-type
    /// identity requires omitting the PURL here.
    ///
    /// REQUIRES `--root-name` (an explicit name is the only identity
    /// signal once the PURL is dropped). Mutually exclusive with
    /// `--root-purl-type`.
    #[arg(
        long = "no-root-purl",
        requires = "root_name",
        conflicts_with = "root_purl_type",
        conflicts_with = "root_purl",
    )]
    pub no_root_purl: bool,

    /// Issue #359 — operator-supplied full PURL string for the root
    /// component. When set, waybill emits this PURL verbatim in every
    /// format (CDX `metadata.component.purl`, SPDX 2.3 root Package
    /// `externalRefs[purl]`, SPDX 3 root `software_packageUrl` +
    /// `externalIdentifier[packageUrl]`). The BOM-subject `name` +
    /// `version` are derived from the PURL itself (`packageurl`-crate
    /// parser).
    ///
    /// Useful when downstream consumers expect a specific
    /// canonical PURL and the operator wants to express it directly
    /// — including PURL features the discrete flags don't reach
    /// (qualifiers like `?arch=amd64`, subpaths like `#cmd/worker`,
    /// custom namespace splits like `pkg:golang/github.com/example/svc`).
    ///
    /// Validated at parse time via `Purl::new`; non-spec input fails
    /// fast with a clap-style error. Mutually exclusive with every
    /// other `--root-*` flag (use one or the other, not both).
    ///
    /// Example: `--root-purl pkg:golang/github.com/example/svc@v1.2.3?arch=amd64`
    /// → `metadata.component.purl = "pkg:golang/github.com/example/svc@v1.2.3?arch=amd64"`,
    ///   name=`github.com/example/svc`, version=`v1.2.3`.
    #[arg(
        long = "root-purl",
        value_name = "PURL",
        value_parser = validate_root_purl,
        conflicts_with = "root_name",
        conflicts_with = "root_version",
        conflicts_with = "root_purl_type",
        conflicts_with = "no_root_purl",
    )]
    pub root_purl: Option<String>,

    /// Milestone 149 (issue #151): when set together with `--root-name`
    /// / `--root-version` / `--root-purl`, preserve the manifest-derived
    /// main-module identity as a `library`-typed entry in
    /// `components[]` rather than dropping it per the milestone-077
    /// clean-replacement default. The demoted entry carries a
    /// `waybill:demoted-from-main-module = "true"` annotation per
    /// Constitution Principle V parity-bridging (none of CDX 1.6
    /// `component.type`, SPDX 2.3 `primaryPackagePurpose`, or SPDX 3
    /// `software_softwarePurpose` expresses demote-provenance).
    ///
    /// **Retained for compatibility; no longer changes output.**
    /// Milestone 860 (#863) made retention unconditional — a root
    /// override now keeps every manifest-derived main module as a
    /// demoted library at any count, so this flag has nothing left to
    /// opt into. It is still accepted so existing callers do not break.
    #[arg(long, default_value = "false")]
    pub preserve_manifest_main_module: bool,

    // ────────────────────────────────────────────────────────────
    // Milestone 080 — user-provided SBOM metadata. See
    // `specs/080-user-sbom-metadata/` for the full design.
    // ────────────────────────────────────────────────────────────

    /// Attach a creator entry to the emitted SBOM. Repeatable. Form:
    /// `<Type>: <Name>` where `<Type>` is one of `Tool`, `Organization`,
    /// `Person` (case-sensitive). Each entry lands at the standards-
    /// native creator/tools field of every emitted format: CDX 1.6
    /// `metadata.tools.components[]` / `metadata.manufacturer` /
    /// `metadata.authors[]` (per Type), SPDX 2.3
    /// `creationInfo.creators[]` (verbatim), SPDX 3 new `Tool` /
    /// `Organization` / `Person` element in `@graph`. waybill's own
    /// auto-populated tool/organization entries are preserved alongside.
    #[arg(
        long = "creator",
        action = clap::ArgAction::Append,
        value_name = "TYPE: NAME",
    )]
    pub creator: Vec<String>,

    /// Attach a document-level annotator. Repeatable. MUST be paired
    /// 1:1 with `--annotation-comment` — each `--annotator` MUST be
    /// immediately followed by exactly one `--annotation-comment` per
    /// the milestone-080 Q1 clarification. Form: same `<Type>: <Name>`
    /// shape as `--creator`.
    #[arg(
        long = "annotator",
        action = clap::ArgAction::Append,
        value_name = "TYPE: NAME",
    )]
    pub annotator: Vec<String>,

    /// Free-text comment that pairs positionally with the preceding
    /// `--annotator` flag. Repeatable. Counts MUST match `--annotator`
    /// exactly.
    #[arg(
        long = "annotation-comment",
        action = clap::ArgAction::Append,
        value_name = "TEXT",
    )]
    pub annotation_comment: Vec<String>,

    /// Free-text comment about the SBOM document as a whole. Single-
    /// valued. Lands at SPDX 2.3 `creationInfo.comment`, SPDX 3
    /// `Annotation` element of type `OTHER`, CDX 1.6 `bom.annotations[]`
    /// (per the milestone-080 native-fields audit; no `waybill:`
    /// parity bridge introduced).
    #[arg(long = "metadata-comment", value_name = "TEXT")]
    pub metadata_comment: Option<String>,

    /// Operator-supplied override for the document/Sbom-level name
    /// field. SPDX 2.3 document `name`, SPDX 3 `software_Sbom.name`,
    /// CDX 1.6 `metadata.component.name`. **Note**: when both
    /// `--scan-target-name` and milestone-077's `--root-name` are set
    /// on a CDX emission, `--root-name` takes precedence on
    /// `metadata.component.name` (a stderr warning is emitted). On
    /// SPDX 2.3 / SPDX 3 the two flags target different fields and
    /// both are honored independently.
    #[arg(long = "scan-target-name", value_name = "NAME")]
    pub scan_target_name: Option<String>,

    /// Path to a JSON sidecar file containing user-supplied metadata.
    /// Schema: `{creators?: [string], annotators?: [{type_name, comment}],
    /// metadata_comment?: string, scan_target_name?: string}` with
    /// `deny_unknown_fields`. Array fields merge additively with their
    /// flag counterparts (file values come first); single-valued
    /// fields fail with a conflict error if specified in both.
    #[arg(long = "metadata-file", value_name = "PATH")]
    pub metadata_file: Option<PathBuf>,

    // ────────────────────────────────────────────────────────────
    // Milestone 081 — operator-asserted CISA SBOM Type override.
    // See `specs/081-sbom-type-clarity/` for the full design.
    // ────────────────────────────────────────────────────────────

    /// Override the auto-detected SBOM type with an operator-asserted
    /// CISA SBOM Type. Valid values: design, source, build, analyzed,
    /// deployed, runtime. Document-level only — per-component
    /// `waybill:sbom-tier` annotations preserve auto-detected values.
    /// When set, CDX `metadata.lifecycles[]`, SPDX 2.3
    /// `creationInfo.comment` "Observed lifecycle phases", and SPDX 3
    /// `software_Sbom.software_sbomType[]` all collapse to a single-
    /// element output reflecting the asserted type via the
    /// equivalence table in `docs/reference/sbom-types.md`.
    #[arg(
        long = "sbom-type",
        value_name = "TYPE",
        value_parser = parse_sbom_type_flag,
    )]
    pub sbom_type:
        Option<crate::generate::lifecycle_phases::SbomType>,

    // ────────────────────────────────────────────────────────────
    // Issue #228 — SPDX 2.3 relationship-vocabulary compatibility.
    // ────────────────────────────────────────────────────────────

    /// Selects the SPDX 2.3 relationship-type vocabulary the emitter
    /// uses for scoped dependency edges (dev / build / test). Both
    /// modes are spec-conformant, but they are NOT equivalent: the
    /// SPDX 2.3 spec defines the typed scoped variants for exactly
    /// the purpose of expressing dev/build/test scope on a
    /// dependency edge, and Constitution Principle X (Transparency)
    /// requires waybill to default to the spec-native mechanism that
    /// carries the most consumer-actionable signal. Operators who
    /// pick `basic` accept information loss in exchange for
    /// compatibility — choose deliberately.
    ///
    /// Default `full` emits the spec-native typed reversed-direction
    /// variants (`DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` /
    /// `TEST_DEPENDENCY_OF`) — the SPDX 2.3 spec's purpose-built
    /// field for the dev/build/test distinction. `basic` emits every
    /// dep as natural-direction `DEPENDS_ON` regardless of scope,
    /// for downstream consumers that don't implement the typed
    /// variants (Trivy, Syft, and tooling built on top of them).
    ///
    /// In BOTH modes the `waybill:lifecycle-scope` annotation is set
    /// on the target Package for non-runtime deps, so the scope
    /// distinction is recoverable from the document regardless of
    /// which mode is in effect. Only affects the `spdx-2.3-json`
    /// format; CDX and SPDX 3 emission are unaffected. See
    /// `docs/reference/sbom-format-mapping.md` C42 for the
    /// cross-format mapping.
    #[arg(
        long = "spdx2-relationship-compat",
        value_name = "PROFILE",
        value_parser = parse_spdx2_relationship_compat_flag,
        default_value = "full",
    )]
    pub spdx2_relationship_compat: crate::generate::Spdx2RelationshipCompat,

    /// Milestone 108 — opt into the external symbol-fingerprint
    /// corpus. Defaults to false: waybill uses the in-source bundled
    /// 7-library corpus exactly as it did pre-108 (SC-003 byte-identity
    /// guarantee). When set, waybill loads fingerprint records from
    /// the operator's per-host cache at the build-time-pinned SHA;
    /// cache-miss triggers a one-shot fetch from
    /// `kusari-sandbox/waybill-fingerprints` (skipped under `--offline`,
    /// falls back to bundled defaults on failure). Components
    /// identified via the symbol-fingerprint path then carry a
    /// `waybill:fingerprint-corpus-sha` annotation (12-hex SHA or
    /// the `bundled` sentinel) per FR-005.
    ///
    /// Also enabled via `WAYBILL_FINGERPRINTS_CORPUS=1`.
    #[arg(long, env = "WAYBILL_FINGERPRINTS_CORPUS")]
    pub fingerprints_corpus: bool,

    /// Milestone 110 Phase 5-Slim (FR-006) — declare additional
    /// fingerprint-corpus sources. Repeatable. Each value is a URL
    /// pointing to a corpus tarball; the optional `=ENV_VAR` suffix
    /// for per-source bearer-token auth is parsed but currently
    /// warned-and-stripped (FR-007 is deferred to a follow-on slice).
    ///
    /// Requires `--fingerprints-corpus`. Sources from this flag are
    /// UNION'd with `WAYBILL_FINGERPRINTS_SOURCES` and the implicit
    /// milestone-108 default (unless `--fingerprints-source-no-default`
    /// is set).
    ///
    /// Also settable via `WAYBILL_FINGERPRINTS_SOURCES` (comma-
    /// separated).
    #[arg(
        long = "fingerprints-source",
        value_name = "URL",
        action = clap::ArgAction::Append,
    )]
    pub fingerprints_source: Vec<String>,

    /// Milestone 110 Phase 5-Slim (FR-006) — suppress the implicit
    /// milestone-108 default fingerprint corpus. Use this for air-
    /// gapped runs that must NOT attempt the public-corpus fetch, or
    /// when the operator's own mirror is the only desired source.
    ///
    /// Also settable via `WAYBILL_FINGERPRINTS_NO_DEFAULT=1`.
    #[arg(
        long = "fingerprints-source-no-default",
        env = "WAYBILL_FINGERPRINTS_NO_DEFAULT",
    )]
    pub fingerprints_source_no_default: bool,

    /// Milestone 108 (US5) — override the build-time-embedded corpus
    /// SHA with a runtime-specified one. Format: 40-char lowercase hex.
    /// Requires `--fingerprints-corpus` (or
    /// `WAYBILL_FINGERPRINTS_CORPUS=1`); when the override is supplied
    /// without the opt-in, waybill emits a warning and ignores the
    /// override (the bundled fallback path is used). Use this to test
    /// newer corpora before they're embedded in a waybill release, or
    /// to pin a specific corpus version for reproducibility across
    /// machines whose embedded SHAs differ.
    ///
    /// Also settable via `WAYBILL_FINGERPRINTS_REV=<SHA>`.
    #[arg(
        long = "fingerprints-rev",
        env = "WAYBILL_FINGERPRINTS_REV",
        value_name = "SHA",
        value_parser = parse_fingerprints_rev_flag,
    )]
    pub fingerprints_rev: Option<String>,

    /// Milestone 173: opt-in Go cache-warming mode. `off` (default)
    /// preserves the milestone-172 `waybill:go-transitive-fallback-count`
    /// annotation as an actionable signal — a non-zero count tells
    /// operators their env is degraded. `per-workspace` invokes `go mod
    /// download` in every discovered Go workspace before the transitive
    /// resolver runs, so step 1 (`go mod graph`) finds every module
    /// locally and produces true parent-child topology instead of
    /// falling through to step 5's go.sum flat fallback.
    ///
    /// No-op when `--offline` is set (the effective mode becomes
    /// `offline-inhibited` and is surfaced via the
    /// `waybill:go-cache-warming-mode` doc-scope annotation).
    ///
    /// Concurrency is controlled by `--warm-go-cache-concurrency`
    /// (default 4). No-op on non-Go scans.
    #[arg(
        long,
        value_enum,
        default_value = "off",
        require_equals = true,
        num_args = 1,
        value_name = "MODE"
    )]
    pub warm_go_cache: WarmGoCacheMode,

    /// Milestone 173: maximum concurrent `go mod download` invocations
    /// during cache warming. Default: 4. Set to 1 for sequential
    /// warming (CI shared-runner friendly). Set to 0 for auto
    /// (`min(logical_cpus, 8)`). Values above 32 are clamped to 32
    /// with a warn-level log (defense against typos flooding
    /// `$GOPROXY`).
    ///
    /// No-op when `--warm-go-cache=off` or in offline mode.
    ///
    /// Rationale: matches m055/m091's `fetch_concurrency = 16`
    /// posture; monorepos are the motivating use case and sequential
    /// warming defeats the ergonomics purpose.
    #[arg(long, default_value_t = 4, value_name = "N")]
    pub warm_go_cache_concurrency: u32,

    /// Milestone 210: bypass the FR-016 trace-noise denylist that
    /// normally filters system paths (`/etc`, `/proc`), user caches
    /// (`~/.cache`), ephemeral paths (`/tmp`), and secret-adjacent
    /// paths (`/var/run/secrets`, `~/.ssh`, `~/.aws`, `*.key`,
    /// `*_rsa` etc.) from the emitted `waybill:source-read-set`
    /// annotation.
    ///
    /// When set, EVERY observed read on a compiler-invocation
    /// descendant lands in the read-set — including any secret
    /// paths the compiler touched. Intended for auditing use cases
    /// where the operator explicitly wants full visibility. NOT
    /// recommended for SBOMs shared beyond the operator's own
    /// audit trail.
    ///
    /// No-op when the compiler-pipeline trace didn't run (default
    /// features / non-Linux host / no `--features ebpf-tracing`).
    #[arg(long)]
    pub include_system_reads: bool,

    /// Milestone 218 (waybill#633): EXPERIMENTAL opt-in cross-
    /// ecosystem dep-name edge resolution.
    ///
    /// When enabled, the graph-dep-name resolver at
    /// `scan_fs/mod.rs:794` bridges `pkg:generic/` main-module
    /// `depends[]` names to matching components across every
    /// ecosystem present in the resolver index (not just the
    /// source ecosystem). This restores outgoing DEPENDS_ON edges
    /// on m216 Gemfile-only Ruby-app main-modules (and, by
    /// generalization, any future m216-alike reader producing
    /// `pkg:generic/` main-modules for pip apps, npm CLI tools,
    /// cargo binary-only crates, etc.).
    ///
    /// Emits three new annotations:
    /// - `waybill:cross-ecosystem-inference` (per-edge) on every
    ///   crossed edge.
    /// - `waybill:cross-ecosystem-inference-ambiguous` (per-edge)
    ///   on ambiguous multi-ecosystem matches per FR-003.
    /// - `waybill:cross-ecosystem-inference-unresolved` (doc-scope)
    ///   listing bare dep names that matched no component in any
    ///   ecosystem.
    ///
    /// Default: OFF. Preserves current post-m216 output byte-
    /// identically. The "experimental" prefix signals that
    /// annotation shapes MAY evolve before flag graduation.
    ///
    /// See `docs/reference/cross-ecosystem-edges.md` for the
    /// consumer-facing interpretation guide.
    ///
    /// The env-var equivalent
    /// `WAYBILL_EXPERIMENTAL_CROSS_ECOSYSTEM_EDGES=1` is read
    /// directly by `scan_fs::scan_path` (bypasses clap) so operators
    /// can enable via env alone in CI without CLI-arg mutation.
    #[arg(
        long = "experimental-cross-ecosystem-edges",
        action = clap::ArgAction::SetTrue,
    )]
    pub experimental_cross_ecosystem_edges: bool,

    /// Milestone 220 — cap main-module discovery scope.
    ///
    /// Accepts:
    /// - `all` (default; current behavior) — every reader discovers
    ///   main-modules wherever they find qualifying manifests. SC-005
    ///   byte-identity contract: `--project-discovery=all` (or the
    ///   flag omitted entirely) produces byte-identical output to
    ///   alpha.68 on every existing test fixture.
    /// - `root-only` — discover only root-level main-modules + their
    ///   ecosystem-native workspace-declared members (via the existing
    ///   `waybill:workspace-member` annotation). Independent nested
    ///   projects are dropped from the SBOM entirely.
    /// - `strict` — discover only the root-level manifest itself.
    ///   Workspace-member declarations are ignored.
    ///
    /// See `docs/reference/project-discovery.md` for the full mode
    /// table + interaction matrix vs `--split[=<mode>]`.
    ///
    /// The env-var equivalent `WAYBILL_PROJECT_DISCOVERY=<mode>` is
    /// bridged from this flag when set to a non-default value (compat
    /// shim for env-only invocations in CI scripts that can't easily
    /// mutate CLI args).
    #[arg(
        long = "project-discovery",
        value_enum,
        default_value = "all",
        require_equals = true,
    )]
    pub project_discovery:
        crate::generate::project_discovery::ProjectDiscoveryMode,

    /// Milestone 232 (#660) — filter the emitted SBOM's component set
    /// by `sbom_tier`. Default `all` preserves pre-232 behavior byte-
    /// for-byte (FR-002 / SC-003 guarantee). See
    /// `docs/reference/component-tiers.md` for the tier taxonomy.
    ///
    /// Modes:
    /// - `all` (default) — emit every resolved component.
    /// - `source-only` — emit only `sbom_tier: "source"` components.
    ///   Recommended for vulnerability-scanner pipelines.
    /// - `design-only` — emit only `sbom_tier: "design"` components.
    ///   Recommended for compliance-attribution pipelines.
    /// - `source-and-binary` — emit `source` OR `binary` components
    ///   (drops `design`, `analyzed`, `file`). Recommended for
    ///   container-artifact pipelines.
    ///
    /// Composes cleanly with every other flag; no mutual exclusions.
    /// Degenerate combinations (filter drops main-module referenced
    /// by `--sign-key`, etc.) produce a WARN log and continue —
    /// never a hard error.
    #[arg(long, value_enum, default_value_t = TierMode::All)]
    pub tier: TierMode,

    /// Skip binary-scanning reader(s). Accepted values: `go`, `all`.
    ///
    /// `go` — skip the `go_binary` reader (no `pkg:golang/*` from
    /// BuildInfo). **Deprecated as a perf lever** (#781): the default
    /// path is now fast on non-Go trees. Keep for component filtering.
    ///
    /// `all` — skip the entire binary-scanning tier. Drops all
    /// binary-tier provenance (Go BuildInfo, role classification,
    /// linkage, fingerprints) for maximum scan speed.
    ///
    /// When set, waybill emits a document-scope
    /// `waybill:binary-scan-suppressed=<mode>` annotation in every
    /// output format so downstream consumers can detect the
    /// suppression. Default (unset) is byte-identical to pre-m665
    /// behavior — FR-003.
    ///
    /// The env-var `WAYBILL_NO_BINARY_SCAN=<mode>` provides the
    /// same value; CLI flag wins when both are set.
    #[arg(
        long = "no-binary-scan",
        value_enum,
        value_name = "MODE",
        env = "WAYBILL_NO_BINARY_SCAN",
        long_help = "\
Skip binary-scanning reader(s). Two modes; each is a filter, not just a perf lever.

MODES:
  `go`  — DEPRECATED as a perf lever (#781): the default path is now fast on non-Go trees \
via a magic-byte prefilter. Still works as a component filter — skips the go_binary reader, so no \
`pkg:golang/*` components from statically-linked Go binaries. Components claimed via OS-package \
readers (dpkg / apk / rpm / pip RECORD) keep their attribution. Emits a WARN log pointing at `all`.

  `all` — Skip the ENTIRE binary-scanning tier. No go_binary BuildInfo probing, no m104 role \
classification, no linkage extraction, no m099 symbol fingerprinting. Drops all binary-tier components. \
Components emitted from other tiers (dpkg/apk/rpm/pip/npm/cargo/etc.) keep their tier data unchanged. \
See issue #775.

WHEN TO USE:
  * `=all`: Scanning a large repo where you don't need any binary-tier provenance and want the max speed win.
  * `=go`: You want to suppress `pkg:golang/*` components in your SBOM (e.g., a compliance regime that \
    can't reason about statically-linked binaries) but keep other binary-tier data.

WHEN NOT TO USE:
  * `=go` for perf: unnecessary post-#781. Just run without the flag.
  * `=all` if you need statically-linked Go module attribution or role/linkage metadata for binaries \
    outside OS-package coverage.

PERF (macOS APFS, warm cache, post-#781):
  * mongo (55k files, C++): default ~4s; `=all` ~2.5s
  * Pre-#781 defaults were ~13.5s on the same tree.

When set, waybill emits a document-scope `waybill:binary-scan-suppressed=<mode>` annotation in every \
output format so downstream consumers can detect the suppression. Default (unset) is byte-identical \
to pre-m665 behavior.

The env-var `WAYBILL_NO_BINARY_SCAN=<mode>` provides the same value; the CLI flag wins when both are set.\
",
    )]
    pub no_binary_scan: Option<BinaryScanMode>,

    /// Milestone 850 (#850) — skip the Go module-proxy fetch step.
    ///
    /// During Go transitive resolution, modules that `go mod graph`
    /// and the local module cache could not resolve are fetched from
    /// the module proxy so their `.mod` files can supply parent-child
    /// dependency topology. That is one network round-trip per module,
    /// and for a module that cannot resolve at all it is pure cost —
    /// the resolver cannot tell which is which in advance, because
    /// finding out is what the fetch is for.
    ///
    /// With this flag those modules fall through to the `go.sum`
    /// closure instead, which still yields root-to-transitive edges
    /// but **without parent-child topology** — `go.sum` does not
    /// encode it.
    ///
    /// That reduction follows from the code path rather than from an
    /// observation: it applies only when the proxy fetch would have
    /// SUCCEEDED, i.e. `go mod graph` failed and the module cache
    /// missed but the module exists upstream. Three attempts to
    /// construct that case during milestone 850 — a tree of
    /// unresolvable modules, a tree with a warm cache, and a forced
    /// cold cache — all produced byte-identical output with and
    /// without the flag, because in each one step 3 either never ran
    /// or returned nothing. Measured cost on a tree of unresolvable
    /// modules: 5.38s to 4.65s.
    ///
    /// Useful when scanning a tree containing unresolvable modules,
    /// when measuring scan performance, or on a slow link. `--offline`
    /// also suppresses this step but disables every other enrichment
    /// source with it; this is the narrow version.
    #[arg(long)]
    pub no_go_proxy_fetch: bool,
}

/// Milestone 173: CLI-side cache-warming mode. Two variants;
/// `OfflineInhibited` is internal to the pipeline and NOT
/// exposed at the CLI parse layer (set at the pipeline entry when
/// `--offline` overrides the operator's choice).
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum WarmGoCacheMode {
    /// No warming performed.
    Off,
    /// One `go mod download` invocation per discovered Go workspace.
    PerWorkspace,
}

impl From<WarmGoCacheMode>
    for crate::scan_fs::package_db::golang::CacheWarmingMode
{
    fn from(cli: WarmGoCacheMode) -> Self {
        match cli {
            WarmGoCacheMode::Off => Self::Off,
            WarmGoCacheMode::PerWorkspace => Self::PerWorkspace,
        }
    }
}

/// Clap value parser for `--fingerprints-rev`. Validates the value is
/// 40-char lowercase hex; returns the string verbatim on success so
/// downstream `LoadOptions` plumbing can parse it via
/// `CorpusSha::from_hex` without re-validating.
pub(crate) fn parse_fingerprints_rev_flag(value: &str) -> Result<String, String> {
    if value.len() != 40 {
        return Err(format!(
            "--fingerprints-rev must be 40-char lowercase hex; got {} chars",
            value.len()
        ));
    }
    if !value.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
        return Err(
            "--fingerprints-rev must be 40-char lowercase hex; \
             rejected uppercase / non-hex characters"
                .to_string(),
        );
    }
    Ok(value.to_string())
}

/// Clap value_parser for `--spdx2-relationship-compat`. Accepts
/// `full` (default, full SPDX 2.3 relationship vocabulary) and
/// `basic` (issue #228 — only the basic vocabulary
/// `DEPENDS_ON`/`CONTAINS`/`DESCRIBES`, scoped deps collapse to
/// `DEPENDS_ON`).
pub(crate) fn parse_spdx2_relationship_compat_flag(
    value: &str,
) -> Result<crate::generate::Spdx2RelationshipCompat, String> {
    match value {
        "full" => Ok(crate::generate::Spdx2RelationshipCompat::Full),
        "basic" => Ok(crate::generate::Spdx2RelationshipCompat::Basic),
        other => Err(format!(
            "invalid --spdx2-relationship-compat '{other}'; valid values: full, basic"
        )),
    }
}

/// Clap value_parser for `--sbom-type`. Wraps
/// `SbomType::parse_str` so the error type implements `Into<String>`
/// (clap's required error shape).
pub(crate) fn parse_sbom_type_flag(
    value: &str,
) -> Result<crate::generate::lifecycle_phases::SbomType, String> {
    crate::generate::lifecycle_phases::SbomType::parse_str(value)
        .map_err(|e| e.to_string())
}

/// Milestone 077 — validate `--root-name` / `--root-version` values
/// at CLI parse time. Per FR-006 + Q1 clarification: rejects empty
/// strings, ASCII whitespace, control characters (`\x00`–`\x1F`,
/// `\x7F`), `?`, and `#`. Accepts any other UTF-8.
///
/// The error messages identify the offending character + position so
/// operators understand which character violated which rule (operators
/// with weird-but-legal names like `@acme/widget-svc` need to know it's
/// the `@` or `/` that's allowed, vs `?`/`#` which are rejected).
///
/// Returns the validated string verbatim on success — the caller stores
/// it in `RootComponentOverride.name` / `.version` for downstream
/// per-format emission. The PURL emitter applies its own RFC 3986
/// percent-encoding via `percent_encode_purl_name` at emission time.
pub(crate) fn validate_root_field(
    value: &str,
    flag_name: &str,
) -> Result<String, String> {
    if value.is_empty() {
        return Err(format!("{flag_name} must not be empty"));
    }
    for (i, c) in value.chars().enumerate() {
        if c.is_whitespace() {
            return Err(format!(
                "{flag_name} contains whitespace at position {i} \
                 (character: {c:?}); whitespace is not allowed"
            ));
        }
        if c.is_control() {
            return Err(format!(
                "{flag_name} contains a control character at position {i} \
                 (codepoint: U+{:04X}); control characters are not allowed",
                c as u32
            ));
        }
        if c == '?' || c == '#' {
            return Err(format!(
                "{flag_name} contains URL-syntax-breaking character '{c}' \
                 at position {i}; '?' and '#' are not allowed"
            ));
        }
    }
    Ok(value.to_string())
}

/// Clap value_parser for `--root-name`. Wraps `validate_root_field`
/// with the canonical flag-name string so error messages identify the
/// flag.
pub(crate) fn validate_root_name(value: &str) -> Result<String, String> {
    validate_root_field(value, "--root-name")
}

/// Clap value_parser for `--root-version`. Wraps `validate_root_field`
/// with the canonical flag-name string.
pub(crate) fn validate_root_version(value: &str) -> Result<String, String> {
    validate_root_field(value, "--root-version")
}

/// Clap value_parser for `--root-purl-type`. Enforces the purl-spec
/// type-token charset (`^[a-z][a-z0-9.+-]*$`). The `packageurl` crate
/// would reject non-conforming tokens at PURL construction time
/// downstream, but pre-validating at parse time gives the operator a
/// fast, flag-named error instead of an opaque "PURL construction
/// failed" message during emission.
pub(crate) fn validate_root_purl_type(value: &str) -> Result<String, String> {
    use std::sync::LazyLock;
    static RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^[a-z][a-z0-9.+-]*$")
            .expect("root-purl-type regex compiles")
    });
    if RE.is_match(value) {
        Ok(value.to_string())
    } else {
        Err(format!(
            "--root-purl-type `{value}` is not a valid purl-spec type token \
             (expected lowercase ASCII alphanumeric + `.+-`, starting with a letter)"
        ))
    }
}

/// Issue #359 — `--root-purl <PURL>` validator. Defers to
/// `waybill_common::types::purl::Purl::new` (which delegates to the
/// `packageurl` crate's spec-correct parser) so the operator gets the
/// same error class clap surfaces for every other PURL-shaped input.
/// Returns the canonicalized form on success so emission uses the same
/// representation `Purl::new` would produce internally.
pub(crate) fn validate_root_purl(value: &str) -> Result<String, String> {
    waybill_common::types::purl::Purl::new(value)
        .map(|p| p.as_str().to_string())
        .map_err(|e| format!("--root-purl `{value}` is not a valid PURL: {e}"))
}

/// Parse a `--id <scheme>=<value>` flag for a user-defined identifier.
///
/// Errors at clap parse time on:
/// - missing `=` separator
/// - empty scheme or value
/// - scheme failing the FR-004 regex (`InvalidSchemeName`)
/// - scheme matching one of the built-in schemes (`repo`, `git`,
///   `image`, `attestation`) — operator is directed to the
///   dedicated `--repo` / `--git-ref` / `--image-id` /
///   `--attestation` flag instead.
///
/// Built-in schemes are EXPLICITLY rejected here so users get a
/// clear error pointing at the right flag instead of a
/// soft-fail-to-opaque downgrade. The `--id` flag is for
/// user-defined namespaces only.
/// Milestone 144 T005: `--rpm-distro <ID>` value parser. Rejects empty
/// strings at clap parse time (FR-003). Lowercases the input so the
/// resulting PURL namespace is canonical per purl-spec §lower-case-rules.
fn parse_non_empty_lowercase_distro_id(s: &str) -> Result<String, String> {
    if s.is_empty() {
        return Err("must be non-empty".to_string());
    }
    Ok(s.to_lowercase())
}

/// Milestone 144 T005: `--max-rpm-bytes <N>` value parser. Rejects
/// zero AND non-numeric input at clap parse time (FR-005). Operators
/// wanting "no cap" should pass a very large number; absence of the
/// flag uses the 512 MiB default.
fn parse_nonzero_u64(s: &str) -> Result<u64, String> {
    let v: u64 = s
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    if v == 0 {
        return Err("must be > 0".to_string());
    }
    Ok(v)
}

/// Milestone 671 T009: `--file-inventory-source-shapes` value parser.
/// Thin wrapper over `source_shape::parse_restriction` that stringifies
/// the `SourceShapeParseError` for `clap`'s `Result<_, String>`
/// contract. Diagnostic messages remain byte-identical to the
/// contract in `contracts/source_shape_restriction.md`.
fn parse_source_shape_restriction_arg(
    raw: &str,
) -> Result<scan_fs::file_tier::source_shape::SourceShapeSet, String> {
    scan_fs::file_tier::source_shape::parse_restriction(raw).map_err(|e| e.to_string())
}

fn parse_user_defined_id_flag(
    raw: &str,
) -> Result<waybill::binding::identifiers::Identifier, String> {
    use waybill::binding::identifiers::{
        BuiltinScheme, Identifier, IdentifierError, IdentifierKind, IdentifierValue, SchemeName,
    };
    let Some(idx) = raw.find('=') else {
        return Err(format!(
            "--id value missing `=` separator: {raw:?} \
             (expected form: --id <scheme>=<value>)"
        ));
    };
    let scheme_str = &raw[..idx];
    let value_str = &raw[idx + 1..];
    let scheme = SchemeName::new(scheme_str.to_string())
        .map_err(|e: IdentifierError| e.to_string())?;
    if let Some(b) = BuiltinScheme::from_scheme_name(&scheme) {
        return Err(format!(
            "--id rejects the built-in scheme `{}` — use the dedicated \
             flag instead (--repo / --git-ref / --image-id / --attestation). \
             --id is for user-defined schemes only.",
            b.as_str()
        ));
    }
    let value =
        IdentifierValue::new(value_str.to_string()).map_err(|e: IdentifierError| e.to_string())?;
    Ok(Identifier::from_parts_with_label(
        scheme,
        value,
        IdentifierKind::UserDefined,
        None,
    ))
}

/// Translate the dedicated built-in flags into the `Identifier`
/// list. Returns the manual identifiers in the supply order
/// `[repo-or-git, image, attestation, ...user-defined]`. The
/// `repo`/`git-ref` pair collapses into a single `git:` identifier
/// when `--git-ref` is set; otherwise emits a `repo:` identifier.
///
/// Each identifier is constructed via `Identifier::parse` (so the
/// FR-004 scheme validation + soft-fail value validation paths run
/// uniformly). Validation failure soft-fails to opaque
/// `IdentifierKind::UserDefined` per VR-005 — same behavior as the
/// old single-flag path.
fn assemble_manual_identifiers(args: &ScanArgs) -> Vec<waybill::binding::identifiers::Identifier> {
    let mut out: Vec<waybill::binding::identifiers::Identifier> = Vec::new();
    // (1) repo / git-ref: when --git-ref is set, emit only the git:
    // form; otherwise emit a bare repo: form.
    if let Some(repo_url) = args.repo.as_deref() {
        let raw = if let Some(rev) = args.git_ref.as_deref() {
            format!("git:{repo_url}#{rev}")
        } else {
            format!("repo:{repo_url}")
        };
        match waybill::binding::identifiers::Identifier::parse(&raw) {
            Ok(id) => out.push(id),
            Err(e) => tracing::warn!(
                error = %e,
                raw = %raw,
                "failed to parse manual --repo/--git-ref identifier; skipping"
            ),
        }
    }
    if let Some(image) = args.image_id.as_deref() {
        let raw = format!("image:{image}");
        match waybill::binding::identifiers::Identifier::parse(&raw) {
            Ok(id) => out.push(id),
            Err(e) => tracing::warn!(
                error = %e,
                raw = %raw,
                "failed to parse manual --image-id identifier; skipping"
            ),
        }
    }
    if let Some(att) = args.attestation.as_deref() {
        let raw = format!("attestation:{att}");
        match waybill::binding::identifiers::Identifier::parse(&raw) {
            Ok(id) => out.push(id),
            Err(e) => tracing::warn!(
                error = %e,
                raw = %raw,
                "failed to parse manual --attestation identifier; skipping"
            ),
        }
    }
    // Milestone 076 — manual --subject-hash flags. Format: `algo:hex`.
    // Wrap each value into a full `subject:<algo>:<hex>` shape and
    // route through `Identifier::parse` so the soft-fail path
    // (downgrade to UserDefined per FR-005) handles malformed input
    // identically to other built-ins.
    for sh in &args.subject_hash {
        let raw = format!("subject:{sh}");
        match waybill::binding::identifiers::Identifier::parse(&raw) {
            Ok(id) => out.push(id),
            Err(e) => tracing::warn!(
                error = %e,
                raw = %raw,
                "failed to parse manual --subject-hash identifier; skipping"
            ),
        }
    }
    // (2) user-defined --id flags in supply order.
    for id in &args.id {
        out.push(id.clone());
    }
    out
}

/// Synthesize an `image:` identifier from the user-supplied `--image`
/// reference and the extracted-image state. Per the Q3 clarification,
/// the canonical shape is `image:<registry>/<name>:<tag>@sha256:<digest>`
/// with documented omissions when components are missing.
///
/// This helper attempts:
/// 1. If the user passed `--image <ref>` and `<ref>` is NOT a tarball
///    file (i.e., an OCI ref like `docker.io/foo/bar:v1`), parse it
///    and combine with the extracted manifest_digest to synthesize the
///    full form.
/// 2. If the user passed a tarball file, use the extracted `repo_tag`
///    (typically `name:tag`) plus `manifest_digest`. The registry
///    portion is omitted when not present in `repo_tag`.
/// 3. If neither yields a usable shape, emit just the digest:
///    `image:@sha256:<manifest_digest>` (rare — defensive fallback).
///
/// Returns `None` when neither field is populated — the scan still
/// emits without the auto-detected `image:` identifier.
fn image_auto_identifier(
    extracted: Option<&scan_fs::docker_image::ExtractedImage>,
    image_arg: Option<&Path>,
) -> Option<waybill::binding::identifiers::Identifier> {
    let extracted = extracted?;
    // Prefer the user-supplied reference when it's an OCI ref (not a
    // tarball file). Even when a registry pull resolved the reference,
    // the user-supplied form carries the human-readable
    // `<registry>/<name>:<tag>` pieces we want in the identifier.
    let arg_str: Option<&str> = match image_arg {
        Some(p) => {
            if p.is_file() {
                None
            } else {
                p.to_str()
            }
        }
        None => None,
    };
    let (registry, name, tag) = if let Some(s) = arg_str {
        parse_image_ref_components(s)
    } else if let Some(rt) = extracted.repo_tag.as_deref() {
        // Tarball path: rely on the extracted RepoTags entry.
        parse_image_ref_components(rt)
    } else {
        (None, String::new(), None)
    };
    // The manifest digest is the SHA-256 of the docker-save manifest.json.
    // It's a stable identifier for THIS specific image artifact even if
    // it differs from the upstream registry's content digest. Operators
    // who need the registry-side digest can pass `--image-id
    // <ref>@sha256:<their-digest>` manually; auto-detection's job
    // is to emit a maximally-informative identifier from what waybill
    // can observe.
    let digest = if extracted.manifest_digest.is_empty() {
        None
    } else {
        Some(extracted.manifest_digest.as_str())
    };
    if name.is_empty() && digest.is_none() {
        return None;
    }
    waybill::binding::identifiers::auto_detect::image_reference_to_identifier(
        registry.as_deref(),
        if name.is_empty() {
            // Fall back to a name pulled from `target_name` (which the
            // scan has access to elsewhere) — but the safest defensive
            // choice when we can't extract a name is to skip emission.
            return None;
        } else {
            &name
        },
        tag.as_deref(),
        digest,
    )
}

/// Parse an OCI-ish image reference into `(registry, name, tag)`.
///
/// Heuristic: if the first `/`-separated segment contains a `.` or `:`,
/// or is the literal `localhost`, it's a registry; otherwise the whole
/// thing is the name. The tag is the LAST `:`-separated piece IF that
/// piece contains no `/`. Digest portions (`@sha256:...`) are stripped
/// before parsing — the digest is extracted from the docker-save
/// manifest, not the user-supplied reference.
fn parse_image_ref_components(raw: &str) -> (Option<String>, String, Option<String>) {
    // Strip any trailing `@sha256:...` portion — digest is sourced
    // from the extracted-image state, not the reference string.
    let raw = match raw.find("@sha256:") {
        Some(i) => &raw[..i],
        None => raw,
    };
    if raw.is_empty() {
        return (None, String::new(), None);
    }
    // Identify potential registry prefix.
    let (registry, rest) = match raw.split_once('/') {
        Some((first, rest)) => {
            let looks_like_registry =
                first.contains('.') || first.contains(':') || first == "localhost";
            if looks_like_registry {
                (Some(first.to_string()), rest)
            } else {
                (None, raw)
            }
        }
        None => (None, raw),
    };
    // Now split off the tag — last `:` whose right-hand-side has no `/`.
    let (name, tag) = if let Some(colon_idx) = rest.rfind(':') {
        let after = &rest[colon_idx + 1..];
        if after.contains('/') || after.is_empty() {
            (rest.to_string(), None)
        } else {
            (rest[..colon_idx].to_string(), Some(after.to_string()))
        }
    } else {
        (rest.to_string(), None)
    };
    (registry, name, tag)
}

// Milestone 074 (T005): the previous in-file `resolve_identifiers`
// was tier-agnostic at the type level (`Option<Identifier>` in,
// `Vec<Identifier>` out) but its single-auto-detected signature
// could not represent the build-tier case where two auto-detected
// entries (`repo:` + `git:`) flow into resolution. The function was
// promoted to `waybill::binding::identifiers::resolve_identifiers`
// with a `Vec<Identifier>`-based signature, applying the same
// override semantics per-scheme. Source-tier and image-tier callers
// still pass at most one auto-detected entry; build-tier passes up
// to two.

/// Resolved enrichment-source enablement. Computed from the CLI flags
/// before any scan work so the decision is testable as a pure function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EnrichConfig {
    deps_dev: bool,
    clearly_defined: bool,
    deps_dev_graph: bool,
}

/// Resolve which enrichment sources are enabled from CLI flags.
///
/// Rules:
/// - `--enrich-sources` (when non-empty) is an explicit allowlist that
///   overrides all `--no-*` flags.
/// - When `--enrich-sources` is empty, individual `--no-*` flags apply.
/// - `offline` is NOT checked here — callers gate on it separately so
///   the inner enrichment functions' own offline short-circuit handles
///   the no-op path and logs correctly.
fn resolve_enrich_sources(args: &ScanArgs) -> EnrichConfig {
    if !args.enrich_sources.is_empty() {
        EnrichConfig {
            deps_dev: args.enrich_sources.contains(&EnrichSource::DepsDev),
            clearly_defined: args.enrich_sources.contains(&EnrichSource::ClearlyDefined),
            deps_dev_graph: args.enrich_sources.contains(&EnrichSource::DepsDevGraph),
        }
    } else {
        // Milestone 207 (#596): `--no-deps-dev` is now an aggregate
        // disable — suppresses BOTH the deps.dev license path AND
        // the deps.dev transitive dep-graph path. Pre-m207 it
        // suppressed only the license path. Fine-grained control
        // preserved via `--no-deps-dev-license` (license only) and
        // `--no-deps-dev-graph` (graph only).
        EnrichConfig {
            deps_dev: !args.no_deps_dev && !args.no_deps_dev_license,
            // #930: opt-in. `--no-clearly-defined` conflicts with
            // `--clearly-defined` at parse time, so this is the flag alone.
            clearly_defined: args.clearly_defined,
            deps_dev_graph: !args.no_deps_dev && !args.no_deps_dev_graph,
        }
    }
}

/// Resolved format-dispatch inputs: the canonical (deduped, in-order)
/// list of format ids the user asked for, plus the per-format path
/// overrides to apply. Computed before any scan work runs so argument
/// errors abort early.
#[derive(Debug)]
struct DispatchPlan {
    formats: Vec<String>,
    overrides: BTreeMap<String, PathBuf>,
}

/// Parse `--format` + `--output` into a [`DispatchPlan`], enforcing
/// the FR-001 / FR-004 / FR-004a / FR-004b error semantics the CLI
/// surface promises: unknown format ids reject with the registered-id
/// enumeration; per-format overrides for unrequested formats reject;
/// bare `--output <path>` is only legal with a single requested
/// format; duplicate overrides and cross-format path collisions
/// reject.
fn resolve_dispatch(
    registry: &SerializerRegistry,
    format_args: &[String],
    output_args: &[String],
) -> anyhow::Result<DispatchPlan> {
    // De-dupe format ids silently while preserving the user's order.
    // `--format cyclonedx-json,cyclonedx-json` collapses to one entry.
    let raw_formats: Vec<String> = if format_args.is_empty() {
        vec![DEFAULT_FORMAT.to_string()]
    } else {
        format_args.to_vec()
    };
    let mut formats: Vec<String> = Vec::new();
    for f in raw_formats {
        let f = f.trim().to_string();
        if f.is_empty() {
            anyhow::bail!("--format value must not be empty");
        }
        if !formats.contains(&f) {
            formats.push(f);
        }
    }

    // Reject unknown format ids with a clear enumeration of what IS
    // registered, so the user can see what changed between versions.
    // OpenVEX is explicitly NOT a registered format; calling it out
    // separately gives a more useful error than "unknown".
    //
    // The milestone-010 typo-guard for `spdx-3-json` was removed —
    // that identifier is now first-class (milestone 011 US1).
    for f in &formats {
        if f == OPENVEX_PSEUDO_FORMAT {
            anyhow::bail!(
                "{OPENVEX_PSEUDO_FORMAT:?} is not a selectable --format — \
                 it is emitted as a sidecar alongside SPDX when a scan \
                 produces VEX. Retarget its output path with \
                 `--output {OPENVEX_PSEUDO_FORMAT}=<path>` alongside \
                 an SPDX `--format`.",
            );
        }
        if registry.get(f).is_none() {
            let known = format_help_list(registry);
            anyhow::bail!(
                "unknown format identifier {:?}; accepted: {}",
                f,
                known.join(", "),
            );
        }
    }

    // Parse --output entries. A bare path (no `=`) is legal only when
    // exactly one format is requested; it then overrides that one
    // format. A `<fmt>=<path>` entry names the format explicitly.
    let mut overrides: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut bare_path: Option<PathBuf> = None;
    for raw in output_args {
        if let Some((fmt, path)) = raw.split_once('=') {
            let fmt = fmt.trim();
            let path = path.trim();
            if fmt.is_empty() || path.is_empty() {
                anyhow::bail!(
                    "--output expects <fmt>=<path> with non-empty parts, got {raw:?}",
                );
            }
            // Special case: `openvex` isn't a format, but is a
            // legal override key when the scan is going to produce
            // the sidecar (i.e., at least one SPDX format was
            // requested). Reject `--output openvex=...` without an
            // SPDX format so typos don't silently no-op.
            if fmt == OPENVEX_PSEUDO_FORMAT {
                let has_spdx = formats
                    .iter()
                    .any(|f| OPENVEX_EMITTING_FORMATS.contains(&f.as_str()));
                if !has_spdx {
                    anyhow::bail!(
                        "`--output {OPENVEX_PSEUDO_FORMAT}=<path>` is only \
                         valid when an SPDX format is also requested \
                         (e.g., --format spdx-2.3-json); it retargets \
                         the OpenVEX sidecar that SPDX emission produces \
                         when a scan has VEX statements. Requested \
                         formats: {}",
                        formats.join(", "),
                    );
                }
                if overrides
                    .insert(fmt.to_string(), PathBuf::from(path))
                    .is_some()
                {
                    anyhow::bail!(
                        "--output for {OPENVEX_PSEUDO_FORMAT:?} specified more than once"
                    );
                }
                continue;
            }
            if !formats.iter().any(|f| f == fmt) {
                anyhow::bail!(
                    "--output targets format {:?} but --format did not request it; \
                     requested: {}",
                    fmt,
                    formats.join(", "),
                );
            }
            if overrides.insert(fmt.to_string(), PathBuf::from(path)).is_some() {
                anyhow::bail!("--output for format {fmt:?} specified more than once");
            }
        } else {
            if bare_path.is_some() {
                anyhow::bail!(
                    "--output bare <path> specified more than once; use \
                     --output <fmt>=<path> to target specific formats"
                );
            }
            bare_path = Some(PathBuf::from(raw));
        }
    }

    if let Some(path) = bare_path {
        if formats.len() != 1 {
            anyhow::bail!(
                "bare --output <path> is only valid with a single --format; \
                 requested formats: {}. Use --output <fmt>=<path> instead.",
                formats.join(", "),
            );
        }
        let fmt = formats[0].clone();
        if overrides.contains_key(&fmt) {
            anyhow::bail!(
                "bare --output <path> conflicts with --output {fmt}=<path>; \
                 specify one form",
            );
        }
        overrides.insert(fmt, path);
    }

    // #1443: without `--output openvex=`, name the sidecar after the SPDX
    // document it accompanies. A fixed name meant two scans into one
    // directory overwrote each other's. Resolved here, as an override, so
    // the writer, every SPDX document's reference and the collision check
    // below all read the one path.
    if !overrides.contains_key(OPENVEX_PSEUDO_FORMAT) {
        let anchor = OPENVEX_EMITTING_FORMATS
            .iter()
            .find(|f| formats.iter().any(|r| r == *f));
        if let Some(fmt) = anchor {
            let doc = overrides.get(*fmt).cloned().unwrap_or_else(|| {
                PathBuf::from(
                    registry
                        .get(fmt)
                        .expect("format id validated above")
                        .default_filename(),
                )
            });
            overrides.insert(
                OPENVEX_PSEUDO_FORMAT.to_string(),
                crate::generate::openvex::sidecar_path_for(&doc),
            );
        }
    }

    // Path-collision check: two formats (default or overridden) must
    // not resolve to the same filesystem path. Done here so the error
    // fires before any scan work runs.
    let mut resolved_paths: BTreeMap<PathBuf, String> = BTreeMap::new();
    for fmt in &formats {
        let ser = registry
            .get(fmt)
            .expect("format id validated above");
        let path = overrides
            .get(fmt)
            .cloned()
            .unwrap_or_else(|| PathBuf::from(ser.default_filename()));
        let canonical = canonicalize_for_collision(&path);
        if let Some(prev) = resolved_paths.insert(canonical.clone(), fmt.clone()) {
            anyhow::bail!(
                "output path collision: format {prev:?} and format {fmt:?} both \
                 resolve to {}",
                canonical.display(),
            );
        }
    }
    // Also check the OpenVEX path against format outputs, whether given or
    // derived above: the sidecar would overwrite the document it collides
    // with.
    if let Some(openvex_path) = overrides.get(OPENVEX_PSEUDO_FORMAT) {
        let canonical = canonicalize_for_collision(openvex_path);
        if let Some(prev) = resolved_paths.insert(canonical.clone(), OPENVEX_PSEUDO_FORMAT.to_string()) {
            anyhow::bail!(
                "output path collision: format {prev:?} and \
                 {OPENVEX_PSEUDO_FORMAT:?} both resolve to {}",
                canonical.display(),
            );
        }
    }

    Ok(DispatchPlan { formats, overrides })
}

/// #1122 — where the OpenVEX sidecar is written: the `--output openvex=`
/// path when given, else beside the SPDX document that references it (the
/// document resolves its relative reference against its own directory).
fn openvex_sidecar_target(override_path: Option<&PathBuf>, spdx_dir: Option<&Path>) -> PathBuf {
    override_path.cloned().unwrap_or_else(|| {
        spdx_dir
            .unwrap_or(Path::new(""))
            .join(crate::generate::openvex::OPENVEX_DEFAULT_FILENAME)
    })
}

/// The SPDX 3 deprecation-alias format id (milestone 011 US3).
/// Kept as a named constant so the notice-emission path in
/// `execute()` and the help-list labeling in [`format_help_list`]
/// reference the same string.
const SPDX_3_DEPRECATED_ALIAS: &str = "spdx-3-json-experimental";

/// Environment override to suppress the
/// `spdx-3-json-experimental` deprecation notice. Set to any
/// non-empty value to silence the stderr warning during a
/// controlled migration; document bytes are unaffected either way.
const NO_DEPRECATION_NOTICE_ENV: &str = "WAYBILL_NO_DEPRECATION_NOTICE";

/// Format the registered-id list for user-facing text. Appends
/// ` [EXPERIMENTAL]` to any serializer where
/// [`SbomSerializer::experimental`] is true (Constitution Principle
/// V), and ` [DEPRECATED]` to the
/// `spdx-3-json-experimental` alias (milestone 011 US3 / research.md
/// §R2). Used by the unknown-format error path — surfaces the
/// status at the exact moment a user encounters the set of
/// accepted format identifiers.
fn format_help_list(registry: &SerializerRegistry) -> Vec<String> {
    registry
        .ids()
        .map(|id| {
            let ser = registry.get(id).expect("id from registry.ids()");
            if id == SPDX_3_DEPRECATED_ALIAS {
                format!("{id} [DEPRECATED]")
            } else if ser.experimental() {
                format!("{id} [EXPERIMENTAL]")
            } else {
                id.to_string()
            }
        })
        .collect()
}

/// Normalize a path for collision detection without touching the
/// filesystem. Relative paths are made absolute against the current
/// directory so two formats writing to `foo.json` and `./foo.json`
/// collide as intended.
fn canonicalize_for_collision(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

/// Resolve an `--image <ref>` OCI reference to a `docker save`-format
/// tarball on disk by trying each source in `args.image_src` in order.
/// First hit wins. The `tempdir` slot is populated with the holder
/// dir so the caller's tarball path stays valid through extraction.
///
/// The OCI-ref parse check from milestone 031 still runs (rejects
/// arguments that are neither tarballs nor parseable refs) so the
/// error message remains the same as before.
async fn resolve_image_ref(
    arg_str: &str,
    args: &ScanArgs,
    tempdir: &mut Option<tempfile::TempDir>,
    selected_source: &mut Option<ImageSource>,
) -> anyhow::Result<PathBuf> {
    #[cfg(feature = "oci-registry")]
    {
        let archive_path = std::path::Path::new(arg_str);
        let kind = scan_fs::oci_pull::detect_image_arg_kind(archive_path);
        if kind != scan_fs::oci_pull::ImageArgKind::OciRef {
            anyhow::bail!(
                "--image argument is neither an existing tarball file nor a parseable OCI image reference: {arg_str}"
            );
        }
    }

    let mut tried: Vec<&'static str> = Vec::new();
    for src in &args.image_src {
        match src {
            ImageSource::Docker => {
                tried.push("docker");
                // `--image-platform` asks for a specific arch/variant
                // pulled from a multi-arch index; the local docker
                // daemon only has whatever it was told to cache. Skip
                // the docker source when a platform is requested.
                if args.image_platform.is_some() {
                    tracing::info!(
                        image_ref = arg_str,
                        "--image-platform set; skipping local docker source (only registry pulls honor platform)"
                    );
                    continue;
                }
                match scan_fs::docker_daemon::inspect(arg_str) {
                    scan_fs::docker_daemon::InspectOutcome::Present => {
                        tracing::info!(
                            image_ref = arg_str,
                            "found image in local docker daemon; exporting via `docker save`"
                        );
                        let td = tempfile::tempdir()
                            .context("creating tempdir for docker-save tarball")?;
                        let tarball = td.path().join("image.tar");
                        scan_fs::docker_daemon::save(arg_str, &tarball)?;
                        *tempdir = Some(td);
                        *selected_source = Some(ImageSource::Docker);
                        return Ok(tarball);
                    }
                    scan_fs::docker_daemon::InspectOutcome::Absent => {
                        tracing::info!(
                            image_ref = arg_str,
                            "image not present in local docker daemon"
                        );
                    }
                    scan_fs::docker_daemon::InspectOutcome::DockerUnavailable => {
                        tracing::info!(
                            image_ref = arg_str,
                            "local docker daemon not available; trying next source"
                        );
                    }
                }
            }
            ImageSource::Podman => {
                tried.push("podman");
                // Milestone 206 (#440) — filesystem-only read of
                // c/storage overlay layout (FR-009: no daemon/REST
                // API). On any PodmanSourceError, WARN + continue
                // to the next --image-src entry per FR-007.
                if args.image_platform.is_some() {
                    tracing::info!(
                        image_ref = arg_str,
                        "--image-platform set; skipping local podman source (only registry pulls honor platform)"
                    );
                    continue;
                }
                let td = tempfile::tempdir()
                    .context("creating tempdir for podman-source tarball")?;
                let tarball = td.path().join("image.tar");
                match scan_fs::podman_source::resolve_and_pack(arg_str, &tarball) {
                    Ok(()) => {
                        tracing::info!(
                            image_ref = arg_str,
                            "found image in local podman storage; packed docker-save tarball"
                        );
                        *tempdir = Some(td);
                        *selected_source = Some(ImageSource::Podman);
                        return Ok(tarball);
                    }
                    Err(e) => {
                        tracing::info!(
                            image_ref = arg_str,
                            error = %e,
                            "podman source failed; trying next --image-src entry"
                        );
                    }
                }
            }
            ImageSource::Remote => {
                tried.push("remote");
                #[cfg(feature = "oci-registry")]
                {
                    tracing::info!(image_ref = arg_str, "pulling image from registry");
                    let cache_disabled = args.no_oci_cache
                        || std::env::var("WAYBILL_OCI_CACHE").as_deref() == Ok("0");
                    let cache_size_cap = if cache_disabled {
                        None
                    } else {
                        let env_size = std::env::var("WAYBILL_OCI_CACHE_SIZE")
                            .ok()
                            .and_then(|s| s.parse::<u64>().ok());
                        Some(
                            args.oci_cache_size
                                .or(env_size)
                                .unwrap_or(10 * 1024 * 1024 * 1024),
                        )
                    };
                    // Milestone 182 — build the TLS/transport config
                    // once up-front. Fails fast (before any network) on
                    // malformed --insecure-registry values or bad CA
                    // paths per FR-014.
                    let tls_config = scan_fs::oci_pull::RegistryTlsConfig::from_args(
                        &args.insecure_registry,
                        &args.registry_ca_cert,
                        args.insecure_tls_skip_verify,
                    )?;
                    let td = scan_fs::oci_pull::pull_to_tarball(
                        arg_str,
                        args.image_platform.as_deref(),
                        cache_size_cap,
                        args.registry_credentials_dir.as_deref(),
                        &tls_config,
                    )
                    .await?;
                    let tarball = td.path().join("image.tar");
                    *tempdir = Some(td);
                    *selected_source = Some(ImageSource::Remote);
                    return Ok(tarball);
                }
                #[cfg(not(feature = "oci-registry"))]
                {
                    anyhow::bail!(
                        "--image-src includes `remote`, but this build of \
                         waybill was compiled with `--no-default-features` \
                         (the `oci-registry` Cargo feature is OFF), so OCI \
                         image references like `alpine:3.19` cannot be \
                         pulled from a registry. Either:\n\
                         (a) reinstall with the default feature set: \
                         `cargo install waybill`, or\n\
                         (b) pre-extract the image with \
                         `docker save <ref> -o image.tar` and pass \
                         `--image image.tar`, or\n\
                         (c) pass `--image-src docker` and ensure the \
                         image is in the local docker daemon."
                    );
                }
            }
        }
    }

    anyhow::bail!(
        "image `{arg_str}` not found in any of the configured `--image-src` sources: [{}]. \
         Pull or build it first, or change `--image-src`.",
        tried.join(", ")
    )
}

/// Milestone 232 (#660) — `--tier` filter's `sbom_tier` predicate.
/// Returns `true` when the given component's tier value should be
/// retained under the requested mode. See
/// `specs/232-tier-filter-flag/data-model.md § New helper`.
fn tier_matches(tier: Option<&str>, mode: TierMode) -> bool {
    match mode {
        TierMode::All => true,
        TierMode::SourceOnly => tier == Some("source"),
        TierMode::DesignOnly => tier == Some("design"),
        TierMode::SourceAndBinary => tier == Some("source") || tier == Some("binary"),
    }
}

/// Milestone 232 (#660) — apply the `--tier=<mode>` filter over the
/// resolved-component set. Early-returns on `TierMode::All` (no-op
/// filter; preserves FR-002 / SC-003 byte-parity). For non-default
/// modes: drops components not matching the mode's predicate, then
/// drops any dependency edge whose source OR target references a
/// dropped component (FR-006). Emits an INFO log with the drop count.
/// Emits an additional WARN log when the post-filter component set
/// is empty (FR-008 observability).
///
/// See `specs/232-tier-filter-flag/data-model.md § New helper`.
fn apply_tier_filter(
    components: &mut Vec<waybill_common::resolution::ResolvedComponent>,
    relationships: &mut Vec<waybill_common::resolution::Relationship>,
    mode: TierMode,
) {
    if mode == TierMode::All {
        return;
    }
    let pre_filter_count = components.len();
    let dropped_purls: std::collections::HashSet<String> = components
        .iter()
        .filter(|c| !tier_matches(c.sbom_tier.as_deref(), mode))
        .map(|c| c.purl.as_str().to_string())
        .collect();
    components.retain(|c| !dropped_purls.contains(c.purl.as_str()));
    relationships.retain(|r| {
        !dropped_purls.contains(&r.from) && !dropped_purls.contains(&r.to)
    });
    let dropped = pre_filter_count.saturating_sub(components.len());
    tracing::info!(
        dropped,
        mode = ?mode,
        "applied --tier filter",
    );
    if components.is_empty() {
        tracing::warn!(
            mode = ?mode,
            "tier filter dropped all components; emitting empty SBOM",
        );
    }
}

/// Milestone 052/part-3 (#101), relocated by #1209 — apply the
/// `--exclude-scope` opt-out filter. Drops components whose
/// `lifecycle_scope` matches any element of `exclude`, plus any
/// dependency edge referencing a dropped component. `Runtime` is never
/// excluded: clap rejects it at parse time via `ExcludeScopeArg`, which
/// has no `Runtime` variant. An empty `exclude` is a no-op.
///
/// Extracted from the inline block it had been since m052 so the
/// behaviour this function names can be asserted without running a
/// scan. The #1209 relocation is the reason it was worth extracting:
/// the call site moved, and a call site is the one thing an end-to-end
/// test cannot pin.
///
/// Returns both the total drop count and `inferred` — how many of the
/// dropped components carried C62
/// `waybill:lifecycle-scope-derivation = transitive`, i.e. a scope this
/// filter could only see because `propagate_non_runtime_scope` had
/// already run. That field is the ordering check in observable form:
/// move this call back above the propagation pass and `inferred` reads
/// 0 on every input, because the marker does not exist yet. Measured on
/// `go-opentelemetry` with `--exclude-scope test`: 33 before the
/// relocation (survived, unscoped, indistinguishable from runtime),
/// 33 dropped after. See
/// `specs/1269-transitive-dev-scope/measurements/t043-exclude-scope-ordering.md`.
#[derive(Debug, Default, PartialEq, Eq)]
struct ExcludeScopeDrops {
    /// Components dropped, whatever assigned their scope.
    total: usize,
    /// Of those, the ones carrying C62 `transitive` — scopes this
    /// feature's propagation pass inferred rather than a reader
    /// declaring them.
    inferred: usize,
}

fn apply_exclude_scope_filter(
    components: &mut Vec<waybill_common::resolution::ResolvedComponent>,
    relationships: &mut Vec<waybill_common::resolution::Relationship>,
    exclude: &[waybill_common::resolution::LifecycleScope],
) -> ExcludeScopeDrops {
    if exclude.is_empty() {
        return ExcludeScopeDrops::default();
    }
    let exclude_set: std::collections::HashSet<waybill_common::resolution::LifecycleScope> =
        exclude.iter().copied().collect();
    let mut inferred = 0usize;
    let dropped_purls: std::collections::HashSet<String> = components
        .iter()
        .filter(|c| {
            c.lifecycle_scope
                .is_some_and(|s| exclude_set.contains(&s))
        })
        .map(|c| {
            if c.extra_annotations
                .get(crate::resolve::scope_propagation::ANN_SCOPE_DERIVATION)
                .and_then(|v| v.as_str())
                == Some(crate::resolve::scope_propagation::DERIVATION_TRANSITIVE)
            {
                inferred += 1;
            }
            c.purl.as_str().to_string()
        })
        .collect();
    let pre_filter_count = components.len();
    components.retain(|c| !dropped_purls.contains(c.purl.as_str()));
    relationships.retain(|r| {
        !dropped_purls.contains(&r.from) && !dropped_purls.contains(&r.to)
    });
    let total = pre_filter_count.saturating_sub(components.len());
    if total > 0 {
        tracing::info!(
            dropped = total,
            dropped_inferred = inferred,
            exclude_scope = ?exclude,
            "applied --exclude-scope filter",
        );
    }
    ExcludeScopeDrops { total, inferred }
}

pub async fn execute(
    mut args: ScanArgs,
    offline: bool,
    exclude_scope: Vec<waybill_common::resolution::LifecycleScope>,
    include_legacy_rpmdb: bool,
    include_declared_deps: bool,
    exclude_set: crate::scan_fs::package_db::exclude_path::ExclusionSet,
    supplement_cdx: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    // Milestone 102 FR-016: propagate the `--include-vendored` flag to
    // the env var that the C/C++ readers read directly. This avoids
    // plumbing through `scan_path`'s 75-callsite chain. The clap derive
    // already populates `args.include_vendored` from either the CLI
    // flag or `WAYBILL_INCLUDE_VENDORED=1` env (whichever was set first);
    // we re-export to the env so `read_all`-internal readers see the
    // unified signal regardless of which input set it.
    // SAFETY: single-threaded at this point in the scan-cmd lifecycle.
    if args.include_vendored {
        // SAFETY: see comment above — single-threaded.
        unsafe {
            std::env::set_var("WAYBILL_INCLUDE_VENDORED", "1");
        }
    }

    // Milestone 235 — validate + propagate Gradle ladder flags.
    // Validation rejects shell metacharacters in
    // `--gradle-extra-configurations` values before any subprocess spawn.
    args.gradle
        .validate_configuration_names()
        .map_err(anyhow::Error::msg)?;
    args.gradle.export_env();

    // Milestone 188 (#455) — propagate `--helm-render` to the env var
    // that the helm reader consumes. Same zero-plumbing pattern as
    // WAYBILL_INCLUDE_VENDORED. Absent env var → HelmRenderMode::Off.
    // SAFETY: single-threaded at this point in the scan-cmd lifecycle.
    if args.helm_render {
        // SAFETY: see comment above — single-threaded.
        unsafe {
            std::env::set_var("WAYBILL_HELM_RENDER", "1");
        }
    }

    // Milestone 156: propagate `--cmake-third-party-recursive` to the
    // env var that the cmake reader reads directly. Same pattern as
    // WAYBILL_INCLUDE_VENDORED above — zero-plumbing propagation
    // avoiding the 75-callsite scan_path -> read_all chain.
    // SAFETY: single-threaded at this point in the scan-cmd lifecycle.
    if args.cmake_third_party_recursive {
        // SAFETY: see comment above — single-threaded.
        unsafe {
            std::env::set_var("WAYBILL_CMAKE_THIRD_PARTY_RECURSIVE", "1");
        }
    }

    // Milestone 108 FR-002: propagate the `--fingerprints-corpus`
    // opt-in to the env var that the binary-scan loop's fingerprint
    // matcher reads directly. Same pattern as WAYBILL_INCLUDE_VENDORED
    // above. Clap's `env = "WAYBILL_FINGERPRINTS_CORPUS"` derive picks
    // up the env-set form; this re-export handles the
    // operator-passed-the-flag-but-not-the-env form. The matcher then
    // calls `LoadOptions::from_env()` (which also reads `WAYBILL_OFFLINE`,
    // already set by main.rs).
    if args.fingerprints_corpus {
        // SAFETY: see comment above — single-threaded.
        unsafe {
            std::env::set_var("WAYBILL_FINGERPRINTS_CORPUS", "1");
        }
    }

    // Milestone 110 Phase 5-Slim (FR-006): re-export the multi-source
    // declarations to the env vars that the fingerprints subsystem's
    // `Sources::from_env()` reads. Same pattern as the `--fingerprints-
    // corpus` and `--fingerprints-rev` re-exports above. When the
    // operator passed `--fingerprints-source` but not `--fingerprints-
    // corpus`, the sources are still propagated; the corpus opt-in
    // gate downstream will short-circuit before any fetch happens, so
    // the env vars are inert.
    if !args.fingerprints_source.is_empty() {
        // SAFETY: env-mutation pattern matches WAYBILL_FINGERPRINTS_CORPUS
        // above; called single-threaded before any scan thread spawns.
        unsafe {
            std::env::set_var(
                "WAYBILL_FINGERPRINTS_SOURCES",
                args.fingerprints_source.join(","),
            );
        }
        if !args.fingerprints_corpus {
            tracing::warn!(
                count = args.fingerprints_source.len(),
                "--fingerprints-source declared without --fingerprints-corpus; \
                 the sources are parsed but no corpus loading will occur. \
                 Add --fingerprints-corpus (or WAYBILL_FINGERPRINTS_CORPUS=1) to enable.",
            );
        }
    }
    if args.fingerprints_source_no_default {
        // SAFETY: see comment above.
        unsafe {
            std::env::set_var("WAYBILL_FINGERPRINTS_NO_DEFAULT", "1");
        }
    }

    // Milestone 218 (waybill#633): bridge the FR-000
    // `--experimental-cross-ecosystem-edges` CLI flag to the env-var
    // read by `scan_fs::scan_path`'s resolver. Same env-var-bridge
    // pattern as WAYBILL_INCLUDE_VENDORED / WAYBILL_DEEP_HASH. Only
    // SET when the CLI arg is true; NEVER unset — otherwise an
    // env-var-only invocation (`WAYBILL_EXPERIMENTAL_...=1 waybill
    // ...`, without the CLI flag) would be silently disabled.
    if args.experimental_cross_ecosystem_edges {
        // SAFETY: see comment above.
        unsafe {
            std::env::set_var("WAYBILL_EXPERIMENTAL_CROSS_ECOSYSTEM_EDGES", "1");
        }
    }

    // Milestone 220: bridge --project-discovery=<mode> to the env
    // var so env-only invocations (CI scripts that can't easily
    // mutate CLI args) also work. Same set-only-never-unset pattern
    // as the m218 cross-ecosystem-edges bridge above — otherwise an
    // env-var-only invocation (`WAYBILL_PROJECT_DISCOVERY=root-only
    // waybill ...`, without the CLI flag) would be silently disabled.
    if args.project_discovery
        != crate::generate::project_discovery::ProjectDiscoveryMode::All
    {
        // SAFETY: see comment above.
        unsafe {
            std::env::set_var(
                "WAYBILL_PROJECT_DISCOVERY",
                args.project_discovery.to_string(),
            );
        }
    }

    // Milestone 108 US5: re-export `--fingerprints-rev` to the env so
    // the matcher's `LoadOptions::from_env()` sees the runtime
    // override (and ignores it if the operator didn't also pass the
    // opt-in flag — implicit-dependency warn handled inline below).
    if let Some(ref rev) = args.fingerprints_rev {
        if !args.fingerprints_corpus {
            tracing::warn!(
                rev = %rev,
                "--fingerprints-rev provided without --fingerprints-corpus; ignoring (bundled fallback will be used)",
            );
        } else {
            // SAFETY: see comment above — single-threaded.
            unsafe {
                std::env::set_var("WAYBILL_FINGERPRINTS_REV", rev);
            }
        }
    }

    // Milestone 173: propagate `--warm-go-cache` + `--warm-go-cache-
    // concurrency` to the Go reader via env vars (same convention as
    // WAYBILL_INCLUDE_VENDORED / WAYBILL_DEEP_HASH). The reader
    // consults these before invoking `warm_workspaces()`.
    //
    // Effective-mode reconciliation with `--offline` (FR-003): when
    // both `--offline` AND `--warm-go-cache=per-workspace` are set,
    // upgrade to `OfflineInhibited` and emit a warn-level log naming
    // the conflict. The mode is still surfaced via the
    // `waybill:go-cache-warming-mode` doc-scope annotation so the
    // operator's request is auditable.
    let effective_warm_mode: crate::scan_fs::package_db::golang::CacheWarmingMode = {
        use crate::scan_fs::package_db::golang::CacheWarmingMode;
        let cli_mode: CacheWarmingMode = args.warm_go_cache.into();
        if offline && matches!(cli_mode, CacheWarmingMode::PerWorkspace) {
            tracing::warn!(
                "--warm-go-cache=per-workspace ignored under --offline; effective mode = offline-inhibited (per FR-003)"
            );
            CacheWarmingMode::OfflineInhibited
        } else {
            cli_mode
        }
    };
    // SAFETY: single-threaded at this point in the scan-cmd lifecycle.
    unsafe {
        std::env::set_var(
            "WAYBILL_WARM_GO_CACHE_MODE",
            effective_warm_mode.as_wire_str(),
        );
        std::env::set_var(
            "WAYBILL_WARM_GO_CACHE_CONCURRENCY",
            args.warm_go_cache_concurrency.to_string(),
        );
        // Milestone 850: `--offline` already suppresses this step via
        // `$GOPROXY=off`; setting the variable here too keeps the
        // resolver's own view consistent rather than leaving it to
        // depend on which knob happened to fire.
        if args.no_go_proxy_fetch || offline {
            std::env::set_var("WAYBILL_NO_GO_PROXY_FETCH", "1");
        }
    }

    // Milestone 052/part-3: the default is to include all lifecycle
    // scopes natively tagged. Readers receive `include_dev = true`
    // unconditionally (inlined as `true` at the two callsites
    // below); the centralized `exclude_scope` filter applied post-
    // resolution drops components per the user's opt-out. The
    // deprecated `--include-dev` CLI shim was removed in issue
    // #101 — see CHANGELOG `[Unreleased]` BREAKING. Per-reader
    // drop gates that still consume the parameter are dead code,
    // slated for removal in a follow-on refactor.
    // Milestone 004 US4: the flag is threaded all the way to
    // `scan_path` so the (future) BDB rpmdb reader can consume it.
    // Until the BDB reader lands (T064), the parameter rides through
    // as a no-op; default behaviour is unchanged from milestone 003.
    let _ = include_legacy_rpmdb;

    // Milestone 188 (#455) — `--helm-chart <path>` input resolution.
    // When `<path>` ends in `.tgz`, extract to a tempdir + descend into
    // the top-level chart directory. When `<path>` is a directory,
    // treat identically to `--path`. Held tempdir kept alive through
    // the whole scan.
    let mut _helm_chart_tempdir: Option<tempfile::TempDir> = None;
    if let Some(helm_chart_path) = args.helm_chart.clone() {
        let scan_path = resolve_helm_chart_input(&helm_chart_path, &mut _helm_chart_tempdir)?;
        args.path = Some(scan_path);
    }

    if args.path.is_none() && args.image.is_none() {
        anyhow::bail!("one of --path, --image, or --helm-chart is required");
    }

    // Milestone 215 — `--split` requires `--output-dir`. `clap`
    // already rejects `--split` + `--output`; here we enforce the
    // positive requirement so operators get a friendly diagnostic
    // pointing at the fix rather than a silent no-op.
    if args.split.is_some() && args.output_dir.is_none() {
        anyhow::bail!(
            "`--split` requires `--output-dir <dir>` — a single file cannot \
             hold N sub-SBOMs. Example:\n  \
             waybill sbom scan --path . --split --output-dir ./sboms/",
        );
    }

    // Milestone 221 US2a (feature 221-cisa-2026-elements-audit / FR-008a)
    // + milestone 222 US2b (feature 222-sigstore-keyless-signing / FR-008a):
    // `--sign-key` or `--sign` requires a durable `--output <file>`.
    // Combining with any `--output -` or `--output <fmt>=-` (stdout) is
    // rejected at parse time because signing without a persisted path
    // defeats the signature's purpose (nothing to hand a verifier) and
    // multiplexing signature bytes into stdout has no standard framing.
    if args.sign_key.is_some() || args.sign {
        if let Some(offender) = args.output.iter().find(|s| is_stdout_output(s)) {
            let which = if args.sign { "--sign" } else { "--sign-key" };
            anyhow::bail!(
                "{which} requires --output <file>; signing does not support \
                 stdout output because verifiers cannot access uncaptured \
                 signature bytes.\n  offending flag: --output {offender}\n  \
                 suggested fix: --output {}", suggest_non_stdout_path(offender),
            );
        }
    }

    // Resolve format dispatch BEFORE any scan work so argument errors
    // abort without having paid for a scan.
    let registry = SerializerRegistry::with_defaults();
    let plan = resolve_dispatch(&registry, &args.format, &args.output)?;


    // Milestone 186 (#442) — OCI Referrers API SBOM discovery.
    //
    // FR-011 input-type guard: the `--sbom-source` flag applies ONLY to
    // registry-pull scans. Reject with an actionable error when combined
    // with `--path` (filesystem scan) or a local tarball path.
    if matches!(
        args.sbom_source,
        SbomSourceMode::Referrer | SbomSourceMode::Either
    ) {
        if args.path.is_some() {
            anyhow::bail!(
                "--sbom-source {mode} is only valid for registry-pull scans (--image <oci-ref>). \
                 Use --sbom-source scan (or omit) to scan a local tarball or filesystem path.",
                mode = sbom_source_mode_wire_str(args.sbom_source),
            );
        }
        if let Some(archive) = args.image.as_ref() {
            if archive.is_file() {
                anyhow::bail!(
                    "--sbom-source {mode} is only valid for registry-pull scans (--image <oci-ref>). \
                     Use --sbom-source scan (or omit) to scan a local tarball or filesystem path.",
                    mode = sbom_source_mode_wire_str(args.sbom_source),
                );
            }
        }
    }

    // FR-002 / US1 / US2 dispatch — attempt the Referrers-API path when the
    // operator opted in via `--sbom-source referrer|either` AND the input is
    // a registry OCI reference. On success, write the referrer bytes verbatim
    // to the resolved output path + emit the FR-007 audit-log line + exit.
    // On fall-through under `Either`, continue to the existing scan pipeline.
    // On fall-through under `Referrer`, bail with an actionable error.
    if matches!(
        args.sbom_source,
        SbomSourceMode::Referrer | SbomSourceMode::Either
    ) {
        #[cfg(feature = "oci-registry")]
        {
            if let Some(image_arg) = args.image.as_ref() {
                let arg_str = image_arg.to_str().context(
                    "--image argument is not valid UTF-8 — required for --sbom-source referrer|either OCI ref parsing",
                )?;
                // Skip if the arg happens to be a local file — the FR-011
                // guard above already rejects this, but a defensive re-check
                // preserves the invariant if the guard is refactored.
                let archive_path = std::path::Path::new(arg_str);
                if !archive_path.is_file() {
                    let tls_config = scan_fs::oci_pull::RegistryTlsConfig::from_args(
                        &args.insecure_registry,
                        &args.registry_ca_cert,
                        args.insecure_tls_skip_verify,
                    )?;
                    let max_bytes = scan_fs::oci_pull::resolve_referrer_max_bytes();
                    let requested: Vec<&str> =
                        plan.formats.iter().map(|s| s.as_str()).collect();
                    let outcome = scan_fs::oci_pull::try_fetch_referrer_sbom(
                        arg_str,
                        args.image_platform.as_deref(),
                        args.registry_credentials_dir.as_deref(),
                        &tls_config,
                        &requested,
                        max_bytes,
                    )
                    .await;
                    match (outcome, args.sbom_source) {
                        (Ok(Some(sbom)), _) => {
                            // Resolved output path for the FIRST requested format.
                            let first_fmt = plan.formats.first().ok_or_else(|| {
                                anyhow::anyhow!("no --format resolved for SBOM referrer emission")
                            })?;
                            let output_path = plan
                                .overrides
                                .get(first_fmt)
                                .cloned()
                                .unwrap_or_else(|| {
                                    let default_name = registry
                                        .get(first_fmt)
                                        .map(|s| s.default_filename())
                                        .unwrap_or("waybill.sbom");
                                    std::path::PathBuf::from(default_name)
                                });
                            std::fs::write(&output_path, &sbom.bytes).with_context(|| {
                                format!(
                                    "writing SBOM referrer bytes to {}",
                                    output_path.display()
                                )
                            })?;
                            // FR-007 / SC-005 audit log — operators consuming
                            // waybill logs identify referrer-sourced emissions
                            // from log content alone. Keys named to match the
                            // strings asserted by the m186 integration tests.
                            tracing::info!(
                                image = %arg_str,
                                sbom_source = "referrer",
                                descriptor_digest = %sbom.descriptor_digest,
                                media_type = %sbom.media_type,
                                output_path = %output_path.display(),
                                bytes = sbom.bytes.len(),
                                "emitted SBOM from OCI Referrers API"
                            );
                            // Format-mismatch WARN: the operator requested a
                            // specific `--format` but the referrer we picked
                            // is a different media type (per FR-004 edge case).
                            if let Some(expected_mt) =
                                referrer_media_type_for_format(first_fmt)
                            {
                                if expected_mt != sbom.media_type {
                                    tracing::warn!(
                                        image = %arg_str,
                                        requested_format = %first_fmt,
                                        got_media_type = %sbom.media_type,
                                        "SBOM referrer emitted with media type differing from --format request (byte-identity preserved; consider --sbom-source scan or --sbom-source either for transcoded output — deferred to a follow-up milestone)"
                                    );
                                }
                            }
                            return Ok(());
                        }
                        (Ok(None), SbomSourceMode::Referrer) => {
                            anyhow::bail!(
                                "no matching SBOM referrer found for {arg_str} on registry \
                                 (or registry does not support the OCI Referrers API HTTP 404). \
                                 Use --sbom-source scan or --sbom-source either to scan the image bytes instead."
                            );
                        }
                        (Err(e), SbomSourceMode::Referrer) => {
                            return Err(e).with_context(|| {
                                format!("SBOM referrer fetch failed for {arg_str} under --sbom-source referrer")
                            });
                        }
                        (Ok(None), SbomSourceMode::Either) => {
                            tracing::info!(
                                image = %arg_str,
                                "no matching SBOM referrer found; falling through to scan"
                            );
                            // fall through to scan pipeline
                        }
                        (Err(e), SbomSourceMode::Either) => {
                            tracing::warn!(
                                image = %arg_str,
                                error = %e,
                                "SBOM referrer fetch failed under --sbom-source either; falling through to scan"
                            );
                            // fall through to scan pipeline
                        }
                        (_, SbomSourceMode::Scan) => {
                            // unreachable — outer `matches!` gate excludes Scan
                            unreachable!("SbomSourceMode::Scan handled by outer match")
                        }
                    }
                }
            }
        }
        #[cfg(not(feature = "oci-registry"))]
        {
            anyhow::bail!(
                "--sbom-source referrer|either requires the `oci-registry` Cargo feature (on by default). \
                 This build was compiled with --no-default-features. Rebuild with the default feature set."
            );
        }
    }

    // FR-002 / research.md §R2: when the deprecated SPDX 3 alias is
    // in the resolved format list, print a two-line stderr notice
    // (deprecation directive + shape-change advisory) exactly once
    // per invocation. Suppress with
    // `WAYBILL_NO_DEPRECATION_NOTICE=<anything>` so CI logs of
    // pipelines on a controlled migration don't drown in repeats.
    // Bytes emitted are unaffected by this flag.
    if plan.formats.iter().any(|f| f == SPDX_3_DEPRECATED_ALIAS)
        && std::env::var_os(NO_DEPRECATION_NOTICE_ENV).is_none()
    {
        eprintln!(
            "warning: --format {SPDX_3_DEPRECATED_ALIAS} is deprecated; use --format spdx-3-json instead."
        );
        eprintln!(
            "note: in this release the alias produces full-coverage SPDX 3 output across all 9 ecosystems — pre-011 releases of this alias emitted an npm-only stub. If your pipeline asserted byte-equality against the milestone-010 stub shape, those assertions will need updating."
        );
    }

    // `--image` dispatches to Docker-tarball extraction, then falls
    // through into the same scan path. Keeping both modes on one code
    // path ensures the CycloneDX output is structurally identical —
    // only `generation-context` differs.
    //
    // `auto_codename` captures the codename we *infer* from the scanned
    // content (the extracted rootfs for --image, or <path>/etc/os-release
    // for a --path root that looks like a rootfs). Explicit
    // `--deb-codename` on the CLI always wins.
    // Hold any OCI-pull tempdir alive through the `extract` call.
    // Dropped immediately after `extract` finishes — the tarball
    // bytes have been read by then and the rootfs lives in its
    // own tempdir.
    // OCI-pull tempdir holder; same lifetime as the docker-save
    // tempdir below — both keep the on-disk tarball alive through the
    // `extract` call further down. Held in an `Option` so the
    // docker-save and remote-pull branches can both populate it
    // without conflict.
    let mut _image_tempdir: Option<tempfile::TempDir> = None;
    // Milestone 206 (#440) — track which --image-src won the
    // dispatch, so the C124 `waybill:image-source` annotation
    // can be emitted (conditional per FR-005 byte-identity).
    let mut selected_image_source: Option<ImageSource> = None;

    // #1170 — the operator naming `--image` IS naming the document's
    // subject, so the root selector must not let a package found inside
    // the rootfs displace it. Derived from `args.image` rather than from
    // `selected_image_source` because the latter stays None for
    // `--image <local-archive>`: no `--image-src` dispatch runs for a
    // tarball, yet a tarball is just as explicitly named.
    let mut scan_target_kind = if args.image.is_some() {
        crate::generate::root_selector::ScanTargetKind::ExplicitImage
    } else {
        crate::generate::root_selector::ScanTargetKind::Path
    };

    let (root_path, target_name, generation_context, auto_codename, _extracted) =
        if let Some(archive) = args.image.as_ref() {
            // `--image` accepts either an on-disk tarball OR an OCI
            // image reference. Tarballs are loaded directly. References
            // are resolved through one or more sources (`--image-src`)
            // — local docker daemon first by default, then registry
            // pull (milestone 044 commit 1).
            let archive_path: std::path::PathBuf = if archive.is_file() {
                // `--image-platform` is registry-pull-only; for a
                // pre-extracted tarball the platform is fixed by
                // whatever `docker save` already wrote, so the flag
                // is meaningless. Reject upfront so users don't get
                // a silent ignore.
                if args.image_platform.is_some() {
                    anyhow::bail!(
                        "--image-platform only applies to registry image references, \
                         not pre-extracted tarballs (--image {} resolved to an existing file).",
                        archive.display()
                    );
                }
                archive.clone()
            } else {
                let arg_str = archive.to_str().context(
                    "--image argument is not valid UTF-8 — required for OCI ref parsing",
                )?;
                resolve_image_ref(arg_str, &args, &mut _image_tempdir, &mut selected_image_source)
                    .await?
            };
            tracing::info!(archive = %archive_path.display(), "extracting docker image");
            let extract_mode = if args.fast_container_extract {
                // Clap `requires = "no_deep_hash"` guarantees pairing.
                scan_fs::docker_image::ExtractMode::OsPackageMetadataOnly
            } else {
                scan_fs::docker_image::ExtractMode::Full
            };
            let extracted = scan_fs::docker_image::extract(&archive_path, extract_mode)?;
            let target = extracted
                .repo_tag
                .clone()
                .unwrap_or_else(|| format!("image@sha256:{}", extracted.manifest_digest));
            let rootfs = extracted.rootfs.clone();
            let codename = extracted.distro_codename.clone();
            if let Some(ref c) = codename {
                tracing::info!(codename = %c, "detected distro codename from rootfs /etc/os-release");
            }
            tracing::info!(rootfs = %rootfs.display(), target = %target, "image extracted");
            (
                rootfs,
                target,
                GenerationContext::ContainerImageScan,
                codename,
                Some(extracted),
            )
        } else {
            let path = args.path.clone().expect("path present after --image check");
            if !path.is_dir() {
                anyhow::bail!("--path must be an existing directory: {}", path.display());
            }
            let target = path_scan_target_name(&path);
            // If --path points at an extracted rootfs (has /etc/os-release
            // at the top), auto-populate the distro tag from it — the
            // canonical `<ID>-<VERSION_ID>` shape (falling back to
            // VERSION_CODENAME when VERSION_ID is absent). Harmless when
            // the path is just a cache dir — the file isn't there and we
            // get None.
            let codename = scan_fs::os_release::read_distro_tag(
                &path.join("etc/os-release"),
            );
            if let Some(ref c) = codename {
                tracing::info!(
                    distro_tag = %c,
                    "detected distro tag from <path>/etc/os-release"
                );
            }
            (
                path,
                target,
                GenerationContext::FilesystemScan,
                codename,
                None,
            )
        };

    // CLI-supplied --deb-codename overrides the auto-detected value.
    let effective_codename = args
        .deb_codename
        .as_deref()
        .or(auto_codename.as_deref());

    // v005 Phase 2: scan_mode drives feature-005 scan-mode-aware scoping
    // (npm internals in particular). ScanMode::Image when the operator
    // invoked --image; ScanMode::Path otherwise.
    let scan_mode = if args.image.is_some() {
        scan_fs::ScanMode::Image
    } else {
        scan_fs::ScanMode::Path
    };
    // Dual-SBOM scope auto-detection (see docs/reference/sbom-scopes.md:). Image scans default to
    // strict "artifact" scope (only list components actually on disk);
    // path scans default to permissive "manifest" scope (declared deps
    // in the lockfile / pom.xml / etc. are in scope even without
    // bytes on disk, because they WOULD be pulled in on install or
    // build). `--include-declared-deps` is an explicit override that
    // forces permissive in image mode; in path mode it's already on
    // by default so the flag is effectively a no-op.
    let effective_include_declared_deps =
        include_declared_deps || matches!(scan_mode, scan_fs::ScanMode::Path);
    tracing::info!(root = %root_path.display(), "scan starting");
    // Milestone 665 T010 (contract C3): FR-009 diagnostic. Emit an
    // INFO log when the operator opted out of binary-content probing
    // via `--no-binary-scan=<mode>` (CLI or env), naming the affected
    // reader(s) so the choice is visible in scan logs without an
    // SBOM diff.
    if let Some(mode) = args.no_binary_scan {
        let (mode_str, affected) = match mode {
            BinaryScanMode::Go => (
                mode.as_annotation_value(),
                "go_binary reader (statically-linked Go BuildInfo probing)",
            ),
            BinaryScanMode::All => (
                mode.as_annotation_value(),
                "entire binary-scanning tier (go_binary reader + m104 role \
                 classification + linkage extraction + m099 symbol fingerprinting)",
            ),
        };
        tracing::info!(
            mode = mode_str,
            "--no-binary-scan={} — skipping {}",
            mode_str,
            affected,
        );
        // Issue #781 — `=go` is deprecated as a perf lever. The
        // go_binary reader now short-circuits on non-binaries via
        // a magic-byte prefilter, so the default path is already
        // fast on source-heavy trees. The flag still works as a
        // component filter for operators who don't want
        // `pkg:golang/*` in their SBOM, but that's a component-scope
        // concern; nudge them toward `=all` if they wanted the perf.
        if matches!(mode, BinaryScanMode::Go) {
            tracing::warn!(
                "--no-binary-scan=go is deprecated as a perf lever \
                 (#781 makes the default path fast on non-Go trees). \
                 The flag still works as a Go-component filter; use \
                 --no-binary-scan=all if your goal is to skip the \
                 entire binary-scanning tier.",
            );
        }
    }
    let scan_fs::ScanResult {
        mut components,
        mut relationships,
        unresolved_declared_dep_count,
        complete_ecosystems,
        os_release_missing_fields,
        go_transitive_coverage,
        go_transitive_fallback_count,
        go_cache_warming,
        go_mod_why,
        go_workspace_mode,
        go_toolchains_detected,
        cross_ecosystem_edges_report,
        helm_extraction_mode,
        pants_resolve_summary,
        haskell_parse_summary,
        gradle_scan_summary,
        scan_target_coord,
        divergence_records,
        no_binary_scan_mode,
    } = scan_fs::scan_path(

        &root_path,
        effective_codename,
        args.max_file_size,
        !args.no_package_db,
        !args.no_deep_hash,
        true, // include_dev — see comment above the scan_path call site
        include_legacy_rpmdb,
        scan_mode,
        effective_include_declared_deps,
        // Scan-target filter: the Maven walker uses this to skip
        // emitting the scan target's own primary coord as a component
        // (it represents the SBOM subject, not a dependency). See
        // `maven::read_with_claims` and docs/reference/sbom-scopes.md ("Scan
        // target identity" for rationale.
        Some(&target_name),
        // Milestone 144: pass `--max-rpm-bytes` + `--rpm-distro` through
        // to the rpm-file reader. `scan_path` sets the corresponding
        // env vars before calling `package_db::read_all`, which builds
        // the per-scan `RpmReaderConfig` from them.
        args.max_rpm_bytes,
        args.rpm_distro.as_deref(),
        &exclude_set,
        args.no_binary_scan,
    )
    .with_context(|| format!("scan failed for {}", root_path.display()))?;

    // Milestone 113 FR-014 / Constitution Principle X: when any
    // exclusion entry is active, install its snapshot via the
    // exclude_path thread-local so the CDX/SPDX 2.3/SPDX 3 metadata
    // emitters can pick it up and emit `waybill:exclude-path` at
    // envelope level. The guard MUST outlive every emitter call
    // below so successive in-process scans (e.g. integration tests)
    // never leak state.
    let _exclude_path_guard = if !exclude_set.is_empty() {
        Some(crate::scan_fs::package_db::exclude_path::install_annotation(
            exclude_set.as_normalized_strings(),
        ))
    } else {
        None
    };
    // Milestone 118 (#343 / FR-010) — when --exclude-path had ≥1 entry
    // in effect, surface the per-scan summary so operators can see how
    // many entries applied and how many directories were suppressed
    // without paging through WAYBILL_LOG=debug output. Per-walker debug
    // events emit centrally from `safe_walk` since milestone 114.
    // When the set is empty, preserve the pre-118 two-field shape
    // byte-identically (FR-010 emission gating).
    if !exclude_set.is_empty() {
        tracing::info!(
            components = components.len(),
            relationships = relationships.len(),
            excluded_entries = exclude_set.entries().len(),
            excluded_literals = exclude_set.count_literals(),
            excluded_patterns = exclude_set.count_patterns(),
            suppressed_dirs = exclude_set
                .suppressed_dirs
                .load(std::sync::atomic::Ordering::Relaxed),
            "scan complete"
        );
    } else {
        tracing::info!(
            components = components.len(),
            relationships = relationships.len(),
            "scan complete"
        );
    }

    // Milestone 127 FR-007. A closure that may resolve defers it: the
    // subject of a `--nix-closure` scan is decided once the closure resolves
    // (milestone 1377), and the warning runs there instead.
    if !args.nix_closure || offline {
        warn_if_root_heuristic_outranked(
            &args,
            &components,
            scan_target_coord.as_ref(),
            &target_name,
            scan_target_kind,
        );
    }

    // Enrichment source control: resolve which sources are enabled.
    // `--offline` is handled by each enrichment source's internal
    // short-circuit (they log "offline, skipping" themselves), so we
    // don't need to gate here — but we avoid emitting misleading
    // "skipped (disabled by flags)" messages when the operative cause
    // is offline mode.
    let enrich_cfg = resolve_enrich_sources(&args);

    // Milestone 207 (#596): FR-006 migration signal. When the
    // operator uses the aggregate `--no-deps-dev` flag WITHOUT any
    // fine-grained escape hatch, emit a one-shot INFO log line
    // explaining the m207 semantic change + linking to the escape
    // hatch. Fine-grained-aware operators (who also set
    // `--no-deps-dev-license` or `--no-deps-dev-graph`) are NOT
    // spammed — they already know what they're doing.
    if args.no_deps_dev && !args.no_deps_dev_license && !args.no_deps_dev_graph {
        tracing::info!(
            "--no-deps-dev now disables ALL deps.dev enrichment paths \
             (m207 aggregate semantic per #596). For the pre-m207 \"license \
             only\" behavior, use --no-deps-dev-license instead."
        );
    }

    // deps.dev enrichment runs after the local scan so it only sees the
    // deduped component set. Components in unsupported ecosystems
    // (deb/apk/generic) are skipped silently inside the enrichment;
    // offline mode turns the whole pass into a no-op. Failures are
    // warnings, not errors — the scan still produces a valid SBOM if
    // deps.dev is unreachable.
    let mut enrichment_degraded: Option<String> = None;
    let deps_dev_client = DepsDevClient::new(std::time::Duration::from_secs(5));
    // Milestone 776: `enrich_components` now also maps the deps.dev
    // `links[]` array onto component externalReferences and reports the
    // links it skipped. The skip counts are carried to the FR-014a
    // summary emitted after the component set stops changing — they
    // cannot be recovered later because skips never reach the document.
    let mut m776_skips = crate::enrich::depsdev_source::LinkMappingSkips::default();
    // Milestone 842: hoisted out of the block below so the post-graph
    // second pass shares it — and therefore shares its in-memory
    // cache, which is what makes that pass nearly free for every
    // component already enriched here.
    let deps_dev_source =
        build_deps_dev_source(deps_dev_client.clone(), offline, &args);
    if enrich_cfg.deps_dev {
        let (enriched, skips, degradation, _stats) =
            enrich_components(&deps_dev_source, &mut components, EnrichmentPass::Initial).await;
        // Milestone 839 (FR-017a): carried to the emitters so a
        // degraded run is distinguishable from a clean one by
        // inspecting the SBOM, not by comparing component counts
        // against an expectation nobody holds.
        enrichment_degraded = degradation.annotation_value();
        m776_skips = skips;
        if enriched > 0 {
            tracing::info!(enriched, "deps.dev added licenses to components");
        }
    } else if !offline {
        tracing::info!("deps.dev license enrichment skipped (disabled by flags)");
    }

    // ClearlyDefined enrichment runs after deps.dev and populates each
    // component's `concluded_licenses` with CD's curated SPDX
    // expression. Fed by the same `--offline` flag — a no-op when set.
    // CD's coverage is good for npm / cargo / gem / pypi / maven /
    // golang and shaky elsewhere; unsupported ecosystems are skipped
    // silently inside the source.
    if enrich_cfg.clearly_defined {
        let cd_source = ClearlyDefinedSource::new(offline);
        let cd_enriched = cd_enrich_components(&cd_source, &mut components).await;
        if cd_enriched > 0 {
            tracing::info!(
                cd_enriched,
                "ClearlyDefined added concluded licenses to components"
            );
        }
    } else if !offline {
        tracing::info!(
            "ClearlyDefined enrichment skipped (off by default; pass --clearly-defined to enable)"
        );
    }

    // deps.dev transitive dep-graph enrichment fills in edges the
    // local scan couldn't produce — shaded-JAR transitives, cold-
    // cache scans, BOM-declared deps. The response tree is merged
    // into the running component set with `source_type =
    // "declared-not-cached"` on any coord not already observed
    // locally; local versions win when deps.dev reports a different
    // version for the same (group, artifact) pair.
    // #877: the post-graph licence pass below exists for the components
    // this pass adds. When it adds none, that pass re-visits only what
    // the initial pass already enriched.
    let pre_graph_count = components.len();
    if enrich_cfg.deps_dev_graph {
        let new_dep_graph_edges =
            crate::enrich::deps_dev_graph::enrich_dep_graph(
                &deps_dev_client,
                &mut components,
                offline,
                effective_include_declared_deps,
            )
            .await;
        if !new_dep_graph_edges.is_empty() {
            tracing::info!(
                count = new_dep_graph_edges.len(),
                "deps.dev added transitive dep-graph edges",
            );
            relationships.extend(new_dep_graph_edges);
        }
    } else if !offline {
        tracing::info!("deps.dev dep-graph enrichment skipped (disabled by flags)");
    }

    // Cross-source dedup pass (Fix A). `scan_fs::scan_path` already ran
    // pass-1 + pass-2 before returning, but `enrich_dep_graph` above
    // pushed `source_type = "declared-not-cached"` entries AFTER that
    // dedup — so pass-2's fold-into-on-disk-twin logic had nothing to
    // fold. Re-running `deduplicate()` here closes the loop: pass-1 is
    // a no-op on an already-deduped set; pass-2 now sees the freshly-
    // pushed declared entries and collapses each one whose canonical
    // `(ecosystem, group, artifact, version)` matches an on-disk
    // component (shade-jar vendored coord or top-level).
    //
    // See `resolve/deduplicator.rs::fold_declared_not_cached` for the
    // full matching rule.
    let graph_added = components.len().saturating_sub(pre_graph_count);
    let pre_fold_count = components.len();
    components = crate::resolve::deduplicator::deduplicate(components);
    let folded = pre_fold_count.saturating_sub(components.len());
    if folded > 0 {
        tracing::info!(
            folded,
            "folded declared-not-cached entries into on-disk twins",
        );
    }

    // Milestone 842 (#842): licence the components the dep-graph pass
    // added.
    //
    // `enrich_components` above runs over the component set as it
    // exists BEFORE graph expansion, so everything deps.dev's
    // dep-graph contributed arrived unenriched — measured on this
    // repository as maven going 29 -> 171 components with the licensed
    // count staying at 14. deps.dev has the data; those components
    // simply never reached the pass.
    //
    // Running it a second time rather than moving it: the graph pass
    // needs the resolved component set, and the dedup + fold above has
    // to settle before enrichment, or work is spent on entries that
    // are about to be folded away. The second pass is cheap — the
    // in-memory cache turns every already-enriched coordinate into a
    // hit, so only genuinely new components reach the network, and
    // with batching that is one request per hundred of them.
    //
    // `apply_version_info` is idempotent (licences and external
    // references both dedupe), so re-visiting an enriched component is
    // a no-op rather than a duplicate.
    //
    // #877: skipped when the graph pass added nothing — `enrich_dep_graph`
    // only appends, so the set is then the one the initial pass saw,
    // minus whatever the dedup folded. Running anyway emitted a second
    // summary line that read as a duplicate pass.
    if enrich_cfg.deps_dev && graph_added > 0 && !components.is_empty() {
        let (second_pass, second_skips, second_degradation, _stats) =
            enrich_components(&deps_dev_source, &mut components, EnrichmentPass::PostGraph)
                .await;
        if second_pass > 0 {
            tracing::info!(
                count = second_pass,
                "deps.dev licensed components contributed by the dep-graph pass",
            );
        }
        m776_skips.unmapped_label += second_skips.unmapped_label;
        m776_skips.malformed_url += second_skips.malformed_url;
        // Counters live on the source, so this record is cumulative
        // across both passes rather than describing only the second.
        if let Some(v) = second_degradation.annotation_value() {
            enrichment_degraded = Some(v);
        }
    }

    // Milestone 191 (#560): second-pass reconciliation, mirroring the
    // scan_fs/mod.rs:807 site. Post-dedup + post-enrichment: any design-
    // tier components that survived the deduplicate() fold pass are now
    // matched against source-tier siblings. Rewrites Relationship edges.
    components = crate::resolve::reconciler::reconcile_design_source_tiers(
        components,
        &mut relationships,
    );


    // Milestone 232 (#660): `--tier=<mode>` output-filter flag.
    // Runs BEFORE format-builder dispatch (SC-004: format builders'
    // internal graph-completeness passes must observe the filtered
    // slice). Sibling to the `--exclude-scope` filter, which #1209
    // moved far below this point — see the relocated call site for
    // why. The two filters are independent retain-on-predicate passes
    // over disjoint fields (`sbom_tier` vs `lifecycle_scope`) and
    // neither predicate reads what the other dropped, so their
    // relative order does not change the surviving set. The `--tier`
    // filter has an ordering gap of its own, untouched here: it runs
    // ABOVE the nixpkgs-haskell and m1035 nix-closure producers, so
    // the components those tiers add are never tier-filtered. See
    // `specs/232-tier-filter-flag/data-model.md § New helper` for the
    // FR-003/FR-004/FR-005/FR-006/FR-008 contract.
    apply_tier_filter(&mut components, &mut relationships, args.tier);

    // Milestone 111 (issue #225 Option A): assemble the operator's
    // `--pkg-alias` declarations into a deterministic AliasMap. CLI-
    // supplied flags are concatenated with `WAYBILL_PKG_ALIAS` env-var
    // entries; conflicts (same LHS, different RHS) abort the scan.
    let pkg_alias_map = build_pkg_alias_map(&args)?;

    // FR-010: warn when `--pkg-alias` was supplied without
    // `--bind-to-source`. The alias has no effect (binding is the only
    // consumer); the warning makes the no-op explicit so operators
    // don't silently miss the intended binding rewrite.
    if !pkg_alias_map.is_empty() && args.bind_to_source.is_none() {
        tracing::warn!(
            count = pkg_alias_map.len(),
            "--pkg-alias declared but --bind-to-source was not supplied; \
             the aliases have no effect on this scan and will not appear \
             in the emitted SBOM. Add --bind-to-source <SOURCE_SBOM> to \
             enable cross-tier binding (milestone 072)."
        );
    }

    // Milestone 072 / T027: when `--bind-to-source <path>` is supplied,
    // resolve the source-tier SBOM and attach per-component
    // `waybill:source-document-binding` annotations to image-tier
    // components whose PURL has a counterpart in the source SBOM.
    // Per FR-011, failure to load the source SBOM aborts the scan.
    let bind_source_ctx: Option<waybill::binding::SourceSbomContext> = if let Some(
        ref source_sbom_path,
    ) = args.bind_to_source
    {
        let ctx = waybill::binding::SourceSbomContext::load(source_sbom_path).with_context(
            || {
                format!(
                    "failed to load --bind-to-source SBOM at {}",
                    source_sbom_path.display()
                )
            },
        )?;
        tracing::info!(
            source_sbom = %source_sbom_path.display(),
            source_purls = ctx.source_purls.len(),
            sha256 = %ctx.source_doc_id.sha256,
            "loaded --bind-to-source SBOM"
        );
        // Per the contract: only emit on non-source-tier SBOMs
        // (i.e., this scan must be `build` or `deployed`). For
        // `--image` scans the tier is `deployed`; for `--path` it's
        // typically `source` and we should NOT emit.
        let is_image_scan = args.image.is_some();
        if is_image_scan {
            let consumed = attach_bindings_to_components(
                &mut components,
                &ctx,
                &pkg_alias_map,
            );
            log_unused_pkg_aliases(&pkg_alias_map, &consumed);
        } else {
            tracing::warn!(
                "--bind-to-source supplied with --path; binding annotations only \
                 emit on image-tier (--image) scans per milestone 072. \
                 Source-tier components stay unmodified."
            );
        }
        Some(ctx)
    } else {
        None
    };

    // `trace_integrity` is a clean record: no eBPF ran, so there's nothing
    // to have overflowed or dropped. Milestone 212 audit (T007a): this
    // site is SCAN-MODE — no eBPF programs are loaded here, so
    // `ring_buffer_overflows: 0` is factually correct + FR-004 compliant.
    // Real-trace attestations flow through `waybill-cli/src/cli/scan.rs`
    // where the value comes from `counters::read_ring_buffer_drops`.
    let integrity = TraceIntegrity {
        ring_buffer_overflows: 0,
        events_dropped: 0,
        uprobe_attach_failures: vec![],
        kprobe_attach_failures: vec![],
        partial_captures: vec![],
        bloom_filter_capacity: 0,
        bloom_filter_false_positive_rate: 0.0,
        filter_categories_applied: vec![],
        unresolved_relative_opens: 0,
        compiler_hash_coverage: None,
        // #1252 — scan mode attempts no opens through the kernel, so
        // nothing could have been discarded. Zero is a fact here, not an
        // unmeasured default, for the same reason as the fields above.
        open_failures_discarded: 0,
        // #618 — scan mode runs no eBPF, so there is no event stream to
        // drop from. Empty is a fact here, not an unmeasured default.
        event_drop_reasons: std::collections::BTreeMap::new(),
        // #1185 — `None` rather than `Some(0)`, and the distinction
        // matters: there is no in-flight-open map in scan mode, so its
        // occupancy is not a measurement of zero. Every other field here
        // states a fact; this one states an absence.
        pending_open_entries_at_close: None,
        // #1185 — scan mode spawns nothing and loads no probes, so there is
        // no traced subtree to have observed. Absent, not zero.
        traced_subtree_opens: None,
    };

    // Milestone 073: resolve identifiers — auto-detected
    // `repo:` (from git origin remote, 3-step fallback) on `--path`
    // scans, auto-detected `image:<registry>/<name>:<tag>@sha256:<digest>`
    // on `--image` scans, plus any manual flags
    // (`--repo` / `--git-ref` / `--image-id` / `--attestation` / `--id`).
    // Manual entries dedup against auto-detected by `(scheme, value)`
    // — manual wins, inheriting the auto-detected entry's position.
    // Order: auto-detected first, then manual in supply order
    // (per FR-009 / VR-008 / data-model.md).
    let auto_detected_id: Option<waybill::binding::identifiers::Identifier> =
        if args.image.is_some() {
            // Image-tier auto-detection — synthesize the canonical
            // `image:` form from the resolved-image fields.
            image_auto_identifier(_extracted.as_ref(), args.image.as_deref())
        } else {
            // Source-tier auto-detection — git-remote 3-step fallback.
            // Milestone 075 — `keep_credentials` boolean controls
            // userinfo sanitization (default: strip for security).
            waybill::binding::identifiers::auto_detect::auto_detect_repo_identifier(
                &root_path,
                args.keep_credentials_in_identifiers,
            )
        };
    let manual_identifiers = assemble_manual_identifiers(&args);
    let identifiers = waybill::binding::identifiers::resolve_identifiers(
        auto_detected_id.into_iter().collect(),
        &manual_identifiers,
    );

    // Milestone 080 — assemble user-supplied SBOM metadata from
    // `--creator` / `--annotator` / `--annotation-comment` /
    // `--metadata-comment` / `--scan-target-name` / `--metadata-file`
    // flags. Per research §3, do the early UX-friendly strict-
    // interleaving check on the raw argv BEFORE the flat clap
    // collection is consumed by `merge_file_and_flags`, so the
    // operator gets a crisp pairing-mistake diagnostic rather than a
    // count-mismatch one.
    let argv: Vec<String> = std::env::args().collect();
    if let Err(diag) =
        waybill::binding::user_metadata::validate_annotator_pair_interleaving(&argv)
    {
        anyhow::bail!(diag);
    }
    let metadata_file = match args.metadata_file.as_deref() {
        Some(p) => Some(
            waybill::binding::user_metadata::load_metadata_file(p)
                .map_err(|e| anyhow::anyhow!(e))?,
        ),
        None => None,
    };
    let user_metadata = waybill::binding::user_metadata::merge_file_and_flags(
        metadata_file,
        args.creator.clone(),
        args.annotator.clone(),
        args.annotation_comment.clone(),
        args.metadata_comment.clone(),
        args.scan_target_name.clone(),
        scan_created_timestamp(),
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    // Stderr warning on the multi-Organization edge case for CDX
    // (research §1: CDX `metadata.manufacturer` is single-valued; 2nd+
    // Organization creators route to bom.annotations[]).
    let org_count = user_metadata
        .creators
        .iter()
        .filter(|c| {
            matches!(
                c.kind,
                waybill::binding::user_metadata::CreatorKind::Organization
            )
        })
        .count();
    if org_count > 1 {
        eprintln!(
            "warning: {} `--creator \"Organization: ...\"` entries supplied; \
             CDX 1.6 permits exactly one `metadata.manufacturer` so the \
             first Organization creator populates that slot and the rest \
             are routed to `bom.annotations[]`.",
            org_count
        );
    }
    if user_metadata.scan_target_name.is_some() && args.root_name.is_some() {
        eprintln!(
            "warning: --root-name overrides --scan-target-name for CDX \
             metadata.component.name; SPDX 2.3 / SPDX 3 honor both \
             independently."
        );
    }

    // Milestone 119 (#326) — `--supplement-cdx <PATH>` opt-in: parse
    // the operator-supplied supplement BEFORE the artifact bundle is
    // constructed; merge it into the scanner-discovered stream so
    // every format builder sees the same combined view; install the
    // merge outcome's provenance + services list on the supplement
    // module's thread-local so per-format metadata emitters can pick
    // them up without churning every ScanArtifacts call site.
    //
    // Parse / I/O / schema-validation failures here exit non-zero
    // BEFORE any emitter runs per FR-002 / SC-005. When the flag is
    // absent the merge is skipped entirely, preserving byte-identity
    // with pre-119 waybill output per FR-013 / SC-006.
    let _supplement_guard = if let Some(path) = supplement_cdx.as_ref() {
        let supp = crate::supplement::load(path).with_context(|| {
            format!(
                "loading supplement file `{}` (see --supplement-cdx)",
                path.display()
            )
        })?;
        let outcome =
            crate::supplement::merge(components, relationships, supp).with_context(
                || {
                    format!(
                        "merging supplement file `{}` into scanner output",
                        path.display()
                    )
                },
            )?;
        components = outcome.components;
        relationships = outcome.dependencies;
        Some(crate::supplement::install(
            outcome.supplement_provenance,
            outcome.services,
            outcome.root_anchors,
        ))
    } else {
        None
    };

    // Milestone 133 US2.2 (FR-013): stamp `waybill:layer-digest` on every
    // component whose `evidence.source_file_paths[0]` matches a path the
    // OCI layer extractor recorded. No-op for non-image scans (path map
    // is `None`). Must run AFTER component resolution + all annotations
    // + path normalization (PR US2.1) so the lookup-keys agree with the
    // layer-map keys (both rootfs-relative, no leading `/`).
    scan_fs::tag_components_with_layer_digest(
        &mut components,
        _extracted.as_ref().map(|e| &e.layer_path_map),
    );

    // Milestone 176 (US1 / FR-001): stamp per-component
    // `waybill:workspace-member` annotation on every component whose
    // evidence.source_file_paths yields a derivable workspace root.
    // Uses the canonicalized scan root so `path+file://<abs>` URI-form
    // source paths (pip/cargo/npm main-modules) can have their scan-
    // root prefix stripped to yield root-relative workspace paths.
    // File-tier components (m133) and any other component with no
    // derivable workspace omit the annotation per FR-002 / Q1. Runs
    // AFTER `tag_components_with_layer_digest` so both tag passes see
    // the same components slice.
    {
        let canonical_root = std::fs::canonicalize(&root_path)
            .unwrap_or_else(|_| root_path.clone());
        scan_fs::tag_components_with_workspace_member(&mut components, &canonical_root, scan_mode);
    }

    // Milestone 220: post-discovery scope filter. Runs AFTER
    // `tag_components_with_workspace_member` so the FR-004 belt-and-
    // suspenders annotation follow-up sees the workspace-member
    // signal populated. Under `All` mode this is a zero-op (returns
    // the slices verbatim; SC-005 byte-identity gate). Under
    // `RootOnly`/`Strict` the filter drops out-of-scope main-modules
    // + their unreachable transitive components. FR-008 empty-root
    // fallback: when the filter reports zero in-scope roots under
    // non-default mode, we WARN + reuse the pre-filter slices (i.e.,
    // treat as `All` for that scan) so the existing synthetic-
    // `pkg:generic/` root path fires cleanly; C140 is NOT emitted on
    // the fallback branch (`project_discovery_mode_for_artifacts`
    // stays `None`).
    let project_discovery_mode_for_artifacts:
        Option<crate::generate::project_discovery::ProjectDiscoveryMode> = {
        let mode = args.project_discovery;
        if mode == crate::generate::project_discovery::ProjectDiscoveryMode::All {
            None
        } else {
            let canonical_root = std::fs::canonicalize(&root_path)
                .unwrap_or_else(|_| root_path.clone());
            // Filter takes owned Vecs; swap out with placeholder,
            // run, then either commit or roll back on empty-root
            // fallback.
            let comps_owned = std::mem::take(&mut components);
            let rels_owned = std::mem::take(&mut relationships);
            let (out_c, out_r, report) =
                crate::generate::project_discovery::filter::apply_scope_filter(
                    comps_owned,
                    rels_owned,
                    mode,
                    &canonical_root,
                );
            if report.root_main_modules == 0 {
                // FR-008 fallback — zero root-level manifests found.
                //
                // `out_c`/`out_r` ARE the full-scope slices here: the
                // filter returns its inputs untouched when nothing is in
                // scope (issue #1045). This branch used to re-run the
                // filter with `All` to "get the unfiltered slices back",
                // which could not work — `All` is a zero-op over whatever
                // it is handed, and what it was handed was the already
                // filtered output. Every dependency was dropped while this
                // WARN claimed full-scope emission.
                tracing::warn!(
                    mode = %mode,
                    components = out_c.len(),
                    "scan: project-discovery=<mode> found zero root-level manifests; falling back to full-scope emission",
                );
                components = out_c;
                relationships = out_r;
                None
            } else {
                tracing::info!(
                    mode = %report.mode,
                    root_main_modules = report.root_main_modules,
                    workspace_members_followed = report.workspace_members_followed,
                    nested_projects_ignored = report.nested_projects_ignored,
                    "scan: project-discovery mode complete",
                );
                components = out_c;
                relationships = out_r;
                Some(mode)
            }
        }
    };

    // Issue #363 — operator-asserted license-concluded promotion. Runs
    // AFTER every external enricher (ClearlyDefined, deps.dev) so the
    // empty-concluded check correctly identifies components those
    // enrichers couldn't fill. Pre-existing concluded values from
    // enrichment ARE preserved (the apply fn skips populated entries).
    if args.conclude_licenses {
        let promoted = scan_fs::apply_operator_asserted_conclusions(&mut components);
        tracing::info!(
            promoted_components = promoted,
            "operator-asserted license-concluded promotion (per --conclude-licenses): \
             you assert the declared licenses have been reviewed."
        );
    }

    // Milestone 133 US1.B (FR-002 + FR-015): file-tier emission for
    // unattributed content (custom binaries, vendored libraries with
    // no manifest, embedded archives). Runs AFTER every package /
    // binary / enrichment step so the FR-011 hybrid dedupe sees the
    // full claim set. Default `off` in US1.B — preserves pre-
    // milestone-133 byte-identity. US1.C flips the default to
    // Milestone 926 (#947) — resolve Haskell dependency versions through the
    // nixpkgs revision pinned in `flake.lock`.
    //
    // Invoked here rather than inside `scan_path` because it needs the
    // operator's flags, and `scan_path` already takes fourteen positional
    // parameters (#844). It runs after resolution so it sees final
    // components, and before emission so the versions it attaches reach
    // every format.
    //
    // The pass gates itself: a repository without both a nixpkgs-shaped
    // pinned input and at least one Haskell dependency does no work and
    // produces no annotation (SC-009).
    // Milestone 1283 (#1283) — the import-from-derivation policy every Nix
    // tier evaluates under, verified by each tier's pre-flight.
    let ifd_policy = if args.nix_allow_ifd {
        scan_fs::package_db::nix::eval::preflight::IfdPolicy::Allow
    } else {
        scan_fs::package_db::nix::eval::preflight::IfdPolicy::Refuse
    };
    if args.nix_allow_ifd && offline {
        // Contract cli-nix-allow-ifd: accepted, but no tier runs offline, so
        // nothing was evaluated with import-from-derivation allowed.
        tracing::info!(
            "--nix-allow-ifd had no effect: --offline admits no Nix tier, so nothing was evaluated"
        );
    }
    // Milestone 1034 (#971 part A) — resolve the operator's `--nix-eval`
    // choice once, before the pass that consumes it. An unparseable
    // `--nix-eval-system` is an argument error rather than a degradation: the
    // operator mistyped an invocation, which is a different thing from the
    // environment being unable to satisfy a valid one.
    let nix_eval_cfg = if args.nix_eval {
        use scan_fs::package_db::nix::eval::TierConfig;
        let system = match args.nix_eval_system.as_deref() {
            Some(s) => Some(s.parse().map_err(|e| anyhow::anyhow!("--nix-eval-system: {e}"))?),
            None => None,
        };
        Some(TierConfig::from_flags(system, args.nix_eval_timeout_secs, ifd_policy))
    } else {
        None
    };

    // Milestone 1283 — the closure tier's C217 record, and the policy its
    // pre-flight verified, which the declaration pass below inherits rather
    // than re-verifying. `None` when no pre-flight ran.
    let mut closure_policy: Option<scan_fs::package_db::nix::closure::summary::NixEvaluationPolicy> =
        None;
    let mut closure_ifd: Option<scan_fs::package_db::nix::eval::result::IfdPolicyVerified> = None;
    // Milestone 1035 (#1034, #1040) — the derivation closure.
    //
    // Runs after the manifest-derived set exists, because it supplements that
    // set rather than replacing it (spec FR-003a): the manifest covers every
    // cabal stanza while a closure covers only what its attribute builds, and
    // GHC boot libraries live inside the compiler derivation rather than
    // beside it. Measured — treating closure-absence as evidence a component
    // is spurious would discard the Haskell standard distribution.
    let nix_closure = {
        use scan_fs::package_db::nix::closure;
        match closure::admission(args.nix_closure, offline) {
            Err(None) => None,
            Err(Some(reason)) => {
                tracing::info!(
                    reason = reason.wire(),
                    "nix-closure: skipped before any nix process started"
                );
                Some(Err(reason))
            }
            Ok(()) => {
                let started = std::time::Instant::now();
                let cfg = closure::ClosureConfig::from_flags(
                    args.nix_closure_attr.clone(),
                    None,
                    ifd_policy,
                );
                // The platform must be explicit for evaluation to stay pure;
                // `builtins.currentSystem` does not exist in pure mode.
                let system = scan_fs::package_db::nix::eval::invoke::detect_host_system(
                    cfg.budget,
                );
                // The pre-flight gates everything below it; its record is
                // kept whether it passes or not (C217).
                let preflighted = system.and_then(|sys| {
                    let pre = closure::preflight(&cfg);
                    closure_policy = Some(match &pre {
                        Ok(p) => closure::summary::NixEvaluationPolicy::verified(&p.ifd, &p.environment),
                        Err(_) => closure::summary::NixEvaluationPolicy::unverified(cfg.ifd_policy),
                    });
                    pre.map(|p| (sys, p))
                });
                Some(match preflighted {
                    Ok((sys, pre)) => closure::resolve(&root_path, &sys, &cfg, &pre).map(|mut c| {
                        tracing::info!(
                            attribute = %c.attribute,
                            derivations = c.raw.derivations.len(),
                            roles = ?c.role_counts(),
                            elapsed_ms = started.elapsed().as_millis(),
                            "nix-closure: classified"
                        );
                        // Milestone 1281 (#1346) — declared licenses for the
                        // members just classified.
                        //
                        // Here rather than inside `resolve` because it needs
                        // the revision `flake.lock` pins, which is a property
                        // of the repository and not of the closure. A missing
                        // or unparseable lockfile is not an error: the closure
                        // is emitted unlicensed, exactly as before this
                        // milestone (spec FR-006).
                        match nixpkgs_revision_for_licenses(&root_path) {
                            Some(rev) => {
                                closure::attach_licenses(&mut c, &rev, &sys, cfg.budget, &pre.ifd);
                            }
                            None => tracing::info!(
                                "nix-closure-licenses: no pinned nixpkgs revision in flake.lock; \
                                 emitting the closure without declared licenses"
                            ),
                        }
                        closure_ifd = Some(pre.ifd);
                        c
                    }),
                    Err(reason) => Err(reason),
                })
            }
        }
    };
    let nixpkgs_haskell_summary = {
        use scan_fs::package_db::nix::haskell_packages as nhp;
        let source = nhp::fetch::HttpSource::new(args.nixpkgs_timeout_secs);
        nhp::enrich(
            &root_path,
            &mut components,
            nhp::ResolveOptions {
                offline,
                disabled: args.no_nixpkgs_haskell,
                closure_disabled: args.no_nixpkgs_haskell_closure,
                // Milestone 1034 (#971 part A). `None` unless the operator
                // opted in, and `None` means no `nix` process is started at
                // all -- which is what keeps the default path byte-identical.
                nix_eval: nix_eval_cfg.clone(),
            },
            &source,
        )
    };
    // #980 (invariant I2): the pass assigns versions, which changes component
    // PURLs, and the dependency graph keys on PURLs. The edges were built
    // above, so rewrite their endpoints now -- otherwise every component this
    // pass resolved is orphaned: CycloneDX keeps an edge pointing at the old
    // PURL, SPDX drops the relationship outright, and neither format errors.
    // #980 — normalise identities BEFORE anything reads them. Version
    // assignment rewrites component PURLs, and the PURL is what
    // `Relationship::from`/`::to` hold.
    if let Some(s) = &nixpkgs_haskell_summary {
        let rewritten = scan_fs::package_db::nix::haskell_packages::apply_renames(
            &s.renames,
            &mut relationships,
        );
        if rewritten > 0 {
            tracing::info!(
                renamed_components = s.renames.len(),
                rewritten_endpoints = rewritten,
                "nixpkgs-haskell: rewrote dependency-edge endpoints after version assignment"
            );
        }
    }

    // #962 / milestone 985 — convert the closure's edges into relationships.
    //
    // The endpoints arrive as ATTRIBUTE NAMES and are resolved to each
    // component's FINAL PURL here, after `enrich` has assigned versions. That
    // ordering is deliberate and is what makes the #980 failure mode
    // structurally impossible for these edges: milestone 980 happened because
    // edges were built from identities that were rewritten afterwards, so
    // building them from the final identities instead means there is nothing
    // left to rewrite. `apply_renames` below still covers the edges that
    // existed BEFORE enrichment, which is the case it was written for.
    if let Some(s) = &nixpkgs_haskell_summary {
        if !s.closure_edges.is_empty() {
            let purl_of: std::collections::HashMap<&str, &str> = components
                .iter()
                .filter(|c| c.purl.as_str().starts_with("pkg:hackage/"))
                .map(|c| (c.name.as_str(), c.purl.as_str()))
                .collect();
            // Dedupe against edges that already exist. The declared-dependency
            // reader and the closure can both produce the same relation — a
            // project declares `ghcide -> aeson` in its cabal, and nixpkgs
            // records the same relation in the package set.
            //
            // CycloneDX hides this: `dependsOn` is a set per component, so
            // duplicates collapse. SPDX does not, and would carry one extra
            // relationship row per duplicate. Measured on the corpus Haskell
            // target before this guard: 225 duplicate rows, and a CDX-vs-SPDX
            // edge-count gap that grew from 1 to 232. Two formats disagreeing
            // about a project's dependency graph is the asymmetry the parity
            // discipline exists to catch.
            let mut present: std::collections::HashSet<(String, String)> = relationships
                .iter()
                .filter(|r| {
                    r.relationship_type
                        == waybill_common::resolution::RelationshipType::DependsOn
                })
                .map(|r| (r.from.clone(), r.to.clone()))
                .collect();
            let mut added = 0usize;
            let mut duplicate = 0usize;
            let mut skipped = 0usize;
            for e in &s.closure_edges {
                // Invariant I2: an edge may only be emitted when BOTH
                // endpoints name a component present in the document. A name
                // with no component is skipped rather than dangled — but the
                // closure emits a component for every name it reached, so
                // this should never fire; the counter exists to say so.
                let (Some(from), Some(to)) =
                    (purl_of.get(e.from.as_str()), purl_of.get(e.to.as_str()))
                else {
                    skipped += 1;
                    continue;
                };
                if !present.insert(((*from).to_string(), (*to).to_string())) {
                    duplicate += 1;
                    continue;
                }
                relationships.push(waybill_common::resolution::Relationship {
                    from: (*from).to_string(),
                    to: (*to).to_string(),
                    relationship_type:
                        waybill_common::resolution::RelationshipType::DependsOn,
                    // Principle X — the edge says where it came from. A
                    // consumer can tell a relation read from the pinned
                    // nixpkgs apart from one the project declared itself.
                    provenance: waybill_common::resolution::EnrichmentProvenance {
                        source: "nixpkgs".to_string(),
                        data_type: "haskell-runtime-closure".to_string(),
                    },
                });
                added += 1;
            }
            if skipped > 0 {
                tracing::warn!(
                    skipped,
                    "nixpkgs-haskell: closure edges skipped because an endpoint \
                     had no component; this should be unreachable (invariant I2)"
                );
            }
            tracing::info!(
                closure_edges = added,
                duplicate,
                "nixpkgs-haskell: closure dependency edges emitted"
            );
        }

        // Milestone 1273 (#1281) — the optional-mark reconciliation now runs
        // inside `haskell_packages::enrich`, because deciding it needs a
        // closure seeded WITHOUT the marked set and therefore needs the
        // relation lookup, which only exists in there. What is left here is
        // the operator-facing report (FR-020).
        //
        // FR-008 — three states, where there used to be one. `restored=0` and
        // "the reconciliation never ran" were the same line, which is the
        // condition under which a feature that cannot fire looks like a
        // feature with nothing to do.
        match s.optional_reconciliation.as_ref() {
            None => tracing::info!(
                "nixpkgs-haskell: optional-mark reconciliation DID NOT RUN \
                 (the closure is disabled or the pass degraded); any \
                 `buildable: False` mark stands unexamined"
            ),
            Some(r) if r.withdrawn.is_empty() => tracing::info!(
                reviewed = r.reviewed,
                declined = r.declined,
                "nixpkgs-haskell: optional-mark reconciliation ran and \
                 withdrew nothing; every mark reviewed is reached only by \
                 the marked set itself"
            ),
            Some(r) => tracing::info!(
                reviewed = r.reviewed,
                withdrawn = r.withdrawn.len(),
                declined = r.declined,
                justifications = ?r.withdrawn,
                "nixpkgs-haskell: optional-mark reconciliation withdrew marks \
                 a non-seed closure path reaches"
            ),
        }
    }

    if let Some(s) = &nixpkgs_haskell_summary {
        tracing::info!(
            resolved = s.resolved,
            unresolved = ?s.unresolved,
            revision = ?s.revision,
            degraded = ?s.degraded_reason,
            "nixpkgs-haskell: version resolution complete"
        );
    }

    // Milestone 1035 (#1034, #1040) — insert the closure's contribution.
    //
    // Deliberately after the nixpkgs-haskell enrichment rather than beside
    // the closure query above. That enrichment assigns versions and rewrites
    // dependency-edge endpoints, and running before it meant the closure's
    // components were present, versionless-adjacent and name-matchable while
    // it worked. Measured: 41 components that a flag-off scan emitted as
    // `pkg:hackage/QuickCheck@2.15.0.1` came out of a flag-on scan as
    // `pkg:generic/QuickCheck@2.15.0.1` -- enabling the closure *downgraded*
    // precise PURLs to the generic fallback the closure uses because
    // `pkg:nix` is not a purl-spec type. More information, emphatically not
    // less, is the whole point of the tier.
    // Milestone 1366 (#1365) — C208, the document-scope record of what the
    // ecosystem identity pass did. `None` when no closure was produced, which
    // is what makes its presence alone say the pass was reached.
    let mut closure_ecosystem_identity: Option<
        scan_fs::package_db::nix::closure::ecosystem_identity::ConfirmationRecord,
    > = None;
    // Milestone 1378 (#1396; C213) — what the contents pass read. Hoisted for
    // the same reason the record above is: its presence says the pass ran.
    let mut closure_contents: Option<ClosureContentsSummary> = None;
    // Milestone 1423 (#1423) — whether the milestone-1275 identity pass had
    // the pinned package set, which decides whether its declared edges can
    // say which Haskell libraries are statically linked; and C216, which the
    // scope pass below decides.
    let mut closure_package_set_available = false;
    let mut closure_scope_method: Option<
        scan_fs::package_db::nix::closure::runtime::ScopeMethodRecord,
    >;
    match &nix_closure {
        // #1446 — WARN, for every reason including `offline-requested`:
        // `nix_closure` is `Some` only when the operator passed
        // `--nix-closure`, and a scan that exits 0 without the closure they
        // asked for must say so above INFO.
        Some(Err(reason)) => tracing::warn!(
            reason = reason.wire(),
            detail = %reason,
            "nix-closure: degrading; --nix-closure was requested but no Nix closure \
             components were added; the document contains only manifest-derived \
             components.{}",
            reason.explanation().map(|e| format!(" {e}")).unwrap_or_default()
        ),
        Some(Ok(classified)) => {
            let added = scan_fs::package_db::nix::closure::emit::components(classified);
            let (merged, appended, appended_identities) =
                merge_closure_components(&mut components, added);
            // Milestone 1377 (#1398) — a resolved closure names the subject:
            // the derivation the evaluated attribute built, which
            // `emit::components` tagged. Set only here, so a degraded or
            // unrequested closure leaves the scan exactly as it was.
            // `--image` is unaffected: the closure tier never runs on one.
            if scan_target_kind == crate::generate::root_selector::ScanTargetKind::Path {
                scan_target_kind = crate::generate::root_selector::ScanTargetKind::NixClosure;
            }
            tracing::info!(
                appended,
                merged,
                existing = components.len() - appended,
                "nix-closure: supplementing the manifest-derived set"
            );

            // Milestone 1275 (#1282) — give a closure-appended member the
            // identity the pinned package set already holds for it.
            //
            // HERE, after the merge, and that placement is measured rather
            // than preferred. Milestone 1035 tried the obvious alternative --
            // moving the merge above `haskell_packages::enrich` so the
            // existing pass would reach these components -- and measured 41
            // PURLs DOWNGRADED from `pkg:hackage/QuickCheck@2.15.0.1` to
            // `pkg:generic/...`; see the comment above. And `is_enrichable`
            // accepts only versionless components, while a closure member
            // always carries the version nix recorded, so that pass would
            // skip them whatever the order.
            //
            // The package set comes from `enrich` when it ran, which costs
            // nothing: re-parsing the 16 MB file measures 1,121 ms (T005).
            // When `enrich` declined -- a repository whose only Haskell
            // content arrives through the closure, which research R2 says is
            // the scan #1282 is about -- the set is loaded from the
            // per-revision cache WITHOUT a network fetch, so FR-010 holds and
            // a cold cache degrades through `PackageSetUnavailable` rather
            // than reaching for the network (findings F3).
            use scan_fs::package_db::nix::haskell_packages as nhp;
            let loaded_set: Option<nhp::package_set::PackageSet> =
                match nixpkgs_haskell_summary.as_ref().and_then(|s| s.package_set.clone()) {
                    Some(set) => Some(set),
                    None => match nhp::load_package_set(&root_path, true, &nhp::fetch::HttpSource::new(args.nixpkgs_timeout_secs)) {
                        nhp::PackageSetLoad::Loaded(l) => Some(l.packages),
                        nhp::PackageSetLoad::NoLockfile => None,
                        nhp::PackageSetLoad::Degraded(reason) => {
                            tracing::info!(
                                reason = reason.as_str(),
                                "nix-closure: identity pass degrading, package set unavailable"
                            );
                            None
                        }
                    },
                };
            // FR-012c's targets are GHC boot libraries, identified from the
            // revision's own configuration rather than from a maintained list
            // (m926). Cache-only, like the package set above.
            let boot = match nhp::load_package_set(&root_path, true, &nhp::fetch::HttpSource::new(args.nixpkgs_timeout_secs)) {
                nhp::PackageSetLoad::Loaded(l) => nhp::boot_set(
                    &nhp::fetch::HttpSource::new(args.nixpkgs_timeout_secs),
                    &l.location,
                    &l.pinned.revision,
                    &root_path,
                    true,
                )
                .map(|b| b.boot)
                .unwrap_or_default(),
                _ => Default::default(),
            };
            closure_package_set_available = loaded_set.is_some();
            let identity = nhp::closure_identity::reclassify(
                &mut components,
                &mut relationships,
                classified,
                loaded_set.as_ref(),
                &boot,
            );
            tracing::info!(
                confirmed = identity.confirmed(),
                outcomes = %identity.to_document_value(),
                "nix-closure: identity pass"
            );

            // Milestone 1366 (#1365) — an ecosystem identity for the
            // closure's PyPI components.
            //
            // HERE: after the m1275 identity pass, and strictly BEFORE the
            // edge build below. The edge join reads `c.purl` from the live
            // component slice at call time, so a correction made here flows
            // into the edges for free; made after, it would orphan every
            // component this pass fixes, because relationship endpoints are
            // PURL strings.
            //
            // `--no-deps-dev` means no deps.dev. The one corpus target that
            // runs a closure passes it, so a corpus run never confirms
            // anything and always records why — see the spec's R7.
            closure_ecosystem_identity = Some(run_closure_ecosystem_identity(
                classified,
                &mut components,
                &appended_identities,
                (enrich_cfg.deps_dev && !offline).then_some(&deps_dev_source),
            )
            .await);

            // Milestone 1378 (#1396) — what is compiled into each member's
            // output, for the outputs already present in the store.
            //
            // HERE, between the identity pass and the edges, for the reason
            // that pass is here: this one may promote a member's identifier
            // (a binary names its own module; the registry confirms it), and
            // the edge join below reads `c.purl` from the live slice.
            let contents = run_closure_contents(
                classified,
                &mut components,
                &scan_fs::package_db::nix::closure::contents::store_dir(),
                (enrich_cfg.deps_dev && !offline).then_some(&deps_dev_source),
            )
            .await;
            relationships.extend(contents.relationships);
            closure_contents = Some(contents.summary);

            // Milestone 1280 (#1346) — the closure's dependency edges.
            //
            // HERE, after the identity pass, because the identity -> PURL join
            // reads whatever PURL the document finally gave each component and
            // that pass rewrites 104 of 381 of them. It is NOT because an
            // earlier placement would dangle: `apply_renames` moves
            // relationship endpoints alongside identities (#980/#981), which
            // this milestone's research R1 got wrong and has retracted.
            //
            // Before this, the tier emitted components and no dependency edges
            // at all, so every closure component was an orphan by
            // construction -- measured, 352 of 406 on the pinned golden.
            let (edges, unanchored) = scan_fs::package_db::nix::closure::graph::relationships(
                &classified.graph,
                &classified.raw,
                &components,
            );
            tracing::info!(
                edges = edges.len(),
                unanchored_root_edges = unanchored,
                "nix-closure: contributing dependency edges"
            );
            if unanchored > 0 {
                // The closure root resolved to no component, so its direct
                // dependencies lost their anchor. Per-emitter anchoring onto
                // `target_ref` is the proper fix and cannot be done from here
                // -- see `closure::graph::relationships`.
                tracing::warn!(
                    unanchored,
                    "nix-closure: the closure root is not a component, so its \
                     direct dependencies are not anchored to the subject"
                );
            }
            relationships.extend(edges);
        }
        None => {}
    }
    // Milestone 127 FR-007, deferred from above for a closure that could
    // resolve: the kind is now final, NixClosure or not.
    if args.nix_closure && !offline {
        warn_if_root_heuristic_outranked(
            &args,
            &components,
            scan_target_coord.as_ref(),
            &target_name,
            scan_target_kind,
        );
    }

    // Milestone 1378 (#1396), FR-015a/b — an unread closure root.
    //
    // HERE, straight after the tier and long before any document is written,
    // so the strict path leaves nothing behind.
    if let Err(message) = closure_root_contents_verdict(
        args.nix_closure,
        args.nix_closure_require_root_contents,
        nix_closure.as_ref().map(|r| r.as_ref().map(|_| ()).map_err(|e| e.wire().to_string())),
        closure_contents.as_ref(),
    ) {
        anyhow::bail!(message);
    }

    // Milestone 1367 (#1371) — give every component the CPE candidates the
    // synthesizer already knows how to produce for it.
    //
    // HERE, outside the `--nix-closure` match, and that placement is the fix.
    // The existing pass runs inside `package_db::read_all`, over the component
    // set as it stands there — which is before the closure's components are
    // merged in above. Six components that match the curated table exactly by
    // name carried nothing as a result.
    //
    // After the match rather than inside it, because the pass is not
    // closure-specific: it runs for every component from every reader, and
    // gating it on `--nix-closure` would leave non-closure scans with the same
    // blind spot. After the milestone-1366 identity pass specifically, because
    // `synthesize_cpes` switches on `purl.ecosystem()` — a component that pass
    // re-identified from `pkg:generic` to `pkg:pypi` takes a different arm
    // depending on which side of it this runs, and the pypi one is what the
    // same package gets from the pip reader today.
    //
    // Unlike the identity pass there is no edge-ordering constraint: CPEs are
    // per-component metadata and no relationship endpoint is derived from them.
    fill_missing_cpes(&mut components);
    // Milestone 1377 US3 — record where each closure component's CPE came
    // from, now that both sources have run and identities are final.
    record_closure_cpe_sources(&mut components);
    // The counts are NOT taken here (milestone 1374, #1380). File-tier
    // components join `components` at the file-inventory step below, and
    // they belong in `excluded_files`; counting now would miss them, which
    // is how milestone 1373 came to omit a population present on 11 of 17
    // corpus targets. See `cpe_coverage`, called after that extend.

    // Milestone 1276 — `None` means the exemption rule was not applicable:
    // no compiler configuration was read, which is every scan of a project
    // that is not Nix-built Haskell. Set by the propagation block below when
    // an exempt class exists.
    let mut scope_exemption_json: Option<String> = None;

    // #1209 — propagate non-runtime lifecycle scope transitively.
    //
    // Three constraints pin this to exactly here (research R1):
    //
    //   1. The component set must be final. This is the constraint the
    //      first placement got wrong, and the correction is why the
    //      call sits here rather than beside the m191 reconciler.
    //      "Final" was checked against `deduplicate()` and both
    //      reconciler passes — all of which precede that earlier
    //      site — and NOT against the component *producers*
    //      downstream of it. The nixpkgs-haskell runtime-closure
    //      resolver and the m1035 nix-closure tier both add
    //      components and edges after the reconcilers; on
    //      `haskell-language-server` they add 232 components and
    //      2,126 edges, including every one of that target's 26
    //      invariant-I3 violators. A pass placed before them saw a
    //      set that was final only for the ecosystems with no
    //      post-reconciler producer, which is why `python-flask`
    //      worked and all three hackage targets silently did not.
    //      Measured: relocating here took them 26→0, 13→0 and 9→0
    //      with `python-flask`, `npm-express` and `rust-ripgrep`
    //      unchanged. Anything added after this point — currently
    //      only the file-tier walker — is outside the pass's view,
    //      deliberately: a file-tier component is not a dependency
    //      and has no scope to inherit.
    //   2. It must run once, before all three format builders, or the
    //      formats could disagree. One pass upstream is what makes the
    //      cross-format requirements hold by construction rather than
    //      by three parallel implementations.
    //   3. It must see no anchoring edge, which the assertion below
    //      enforces; see the paragraph after this list.
    //
    // `--exclude-scope` was ABOVE this call when the pass first
    // landed, and has been moved BELOW it (see its own call site, and
    // `measurements/t043-exclude-scope-ordering.md` for the before and
    // after). While it was above, an operator excluding a scope got
    // the declared members dropped and their exclusive transitives
    // kept — and kept *unscoped*, because the filter had removed the
    // seeds this pass propagates from. Measured on `go-opentelemetry`,
    // `--exclude-scope test`: 33 survivors before, 0 after.
    //
    // That gap was reasoned from code position for two reviews before
    // anyone measured it, and the measurement corrected the reasoning
    // as well as confirming it: on `haskell-language-server` the old
    // order already dropped all 37 development-scoped components, not
    // because the filter was placed correctly but because removing the
    // 11 declared dev members starved the nixpkgs-haskell closure
    // resolver below it, so the 26 transitives were never produced.
    // Same flag, same release, opposite behaviour per ecosystem.
    //
    // It also has to run before the builders for correctness, not just
    // tidiness: every anchoring pass (m868 resolve-ownership, m860
    // retained-main-module, m158 peer linkage, and the CDX
    // "primary depends on every component nothing else depends on"
    // fallback) synthesizes edges FROM the subject inside the
    // builders. An anchoring edge here would make the orphaned
    // non-runtime closure look runtime-reachable and silently exempt
    // it from propagation — the population this pass exists to mark.
    // The pass asserts it sees none of them.
    {
        let root_override_for_seeds = crate::generate::RootComponentOverride {
            name: args.root_name.clone(),
            version: args.root_version.clone(),
            purl_type: args.root_purl_type.clone(),
            omit_purl: args.no_root_purl,
            full_purl: args.root_purl.clone(),
            full_purl_name: None,
            full_purl_version: None,
        };
        let selection = crate::generate::root_selector::select_root(
            &components,
            &root_override_for_seeds,
            scan_target_coord.as_ref(),
            &target_name,
            "0.0.0",
            scan_target_kind,
        );
        // Reuses the emitters' own seed derivation rather than forming
        // a second notion of what the root is.
        let (seeds, per_ecosystem_root) =
            crate::generate::graph_completeness::bfs::component_root_seeds(
                &components,
                &selection,
            );
        let ecosystems_with_root: std::collections::HashSet<String> =
            per_ecosystem_root.into_keys().collect();
        // Milestone 1276 (#1340) — the packages the compiler ships, which no
        // scope may report as absent from the artifact. Carried out of the
        // nixpkgs-haskell pass, which computes the set to classify with and
        // would otherwise drop it; `None` when that pass did not run, which
        // exempts nothing (FR-005). Measured: `haskell-aeson` is exactly that
        // case, and one of its components would be wrongly exempted by a set
        // assembled any other way (findings F2).
        let boot_libraries: std::collections::HashSet<String> = nixpkgs_haskell_summary
            .as_ref()
            .and_then(|s| s.boot_libraries.clone())
            .map(|b| b.into_iter().collect())
            .unwrap_or_default();
        // #1283 (invariant I9) — the direct origin milestone 1276 left open: a
        // reader's own non-runtime scope on a package the compiler supplies.
        // Before the propagation, so the withdrawn mark seeds nothing.
        crate::resolve::scope_propagation::withdraw_compiler_supplied_scope(
            &mut components,
            &mut relationships,
            &boot_libraries,
        );
        let scope_propagation_summary =
            crate::resolve::scope_propagation::propagate_non_runtime_scope(
                &mut components,
                &relationships,
                &seeds,
                &ecosystems_with_root,
                &boot_libraries,
            );
        // Milestone 1276 (#1340) — FR-007. Recorded at document scope because
        // its reader is comparing two documents of one project across waybill
        // versions and needs to see that a RULE changed rather than the
        // project. `PropagationSummary` has always held the figure and only
        // ever logged it, which is the gap C174 exists to close for the
        // neighbouring pass.
        //
        // `Some` whenever the propagation ran, including when it exempted
        // nothing: "the rule exempted nothing" and "the rule did not run" are
        // different facts.
        //
        // Emitted only when the exempt class was AVAILABLE — a compiler
        // configuration was read — and then always, including a count of
        // zero. Three states, carried by presence and value together:
        //
        //   absent              the rule was not applicable; no compiler
        //                       configuration exists, which is every scan of
        //                       a project that is not Nix-built Haskell
        //   {"…":0}             applicable, and it exempted nothing
        //   {"…":N}             applicable, and it exempted N
        //
        // The first draft emitted the record unconditionally with a second
        // `exempt-class` key to distinguish those states. T028 measured the
        // cost: every document in every ecosystem gained a Haskell-specific
        // property, and 35 golden byte-identity tests failed — for a row that
        // on a Cargo or Python project can only ever say zero. US3's operator
        // is the one scanning Haskell, so applicability is the right gate, and
        // absence is a clearer way to say "not applicable" than a key saying
        // so on every document in the repository.
        if !boot_libraries.is_empty() {
            scope_exemption_json = Some(
                serde_json::json!({
                    "compiler-supplied": scope_propagation_summary.immune_compiler_supplied,
                })
                .to_string(),
            );
        }
        // Re-type edges into anything the pass just marked. The
        // function skips every edge that is no longer a plain
        // `DependsOn`, so this types exactly the newly-marked targets'
        // edges and is a no-op on everything the reader-side call
        // already typed — which is why it is re-run here rather than
        // moved from `scan_fs`.
        crate::scan_fs::apply_lifecycle_scope_to_edges(&components, &mut relationships);

        // Milestone 1277 (#1342) — the non-runtime classification the nix
        // closure role establishes.
        //
        // Placed beside the propagation because both inputs are already here
        // and nothing else needs plumbing: `boot_libraries` is the set
        // milestone 1276 threads out of the nixpkgs-haskell pass, and the
        // C182 role is on each component's `extra_annotations` by now.
        //
        // Runs for every scan, like the propagation does, and is a no-op when
        // no component carries a closure role — which is every scan without
        // `--nix-closure` and every scan whose closure tier degraded
        // (FR-007, FR-008).
        //
        // Order against the propagation does not matter: the two write
        // disjoint fields, `lifecycle_scope` and `artifact_presence`, and
        // neither ever asserts runtime presence. Every combination of the
        // two is enumerated in `contracts/role-scope.md`; none requires a
        // precedence rule. Should a future mechanism assert `required`, that
        // stops being true.
        //
        // Milestone 1423 (#1423, T017) — the evidence the pass decides from:
        // the evaluated output's runtime closure plus what is statically
        // linked into it, read against the document as it stands now (after
        // the identity, contents and edge passes), so the declared edges and
        // the host → module edges are final. With no runtime closure the
        // reachability walk from the evaluated package decides (US2); with no
        // single evaluated package the role rule decides, exactly as before.
        // C216 records which, and why.
        let (runtime_evidence, scope_method) = match &nix_closure {
            Some(Ok(classified)) => scan_fs::package_db::nix::closure::runtime::evidence(
                classified,
                &components,
                &relationships,
                closure_package_set_available,
            ),
            _ => (
                scan_fs::package_db::nix::closure::runtime::RuntimeEvidence::None,
                None,
            ),
        };
        closure_scope_method = scope_method;
        let role_scope_summary = crate::resolve::closure_role_scope::assign_role_scope(
            &mut components,
            &boot_libraries,
            &runtime_evidence,
            &relationships,
        );
        if role_scope_summary.roles_seen > 0 {
            tracing::info!(
                roles_seen = role_scope_summary.roles_seen,
                classified = role_scope_summary.assigned(),
                build_tooling = role_scope_summary.assigned_build_tooling,
                unreferenced = role_scope_summary.assigned_unreferenced,
                exempt_compiler_supplied = role_scope_summary.exempt_compiler_supplied,
                decided_runtime = role_scope_summary.decided_runtime,
                statically_linked = role_scope_summary.statically_linked,
                decided_not_in_artifact = role_scope_summary.decided_not_in_artifact,
                "nix-closure: artifact-presence classification"
            );
        }
    }
    // The document-scope record (FR-018). Built only on the success path, so
    // a degraded tier emits no counts rather than zeros -- a zero would read
    // as "the closure held nothing", which is a different claim.
    let nix_closure_summary = match &nix_closure {
        Some(Ok(classified)) => {
            use scan_fs::package_db::nix::closure;
            let patched = closure::patches::attribute(&classified.raw);
            let mut summary =
                closure::summary::NixClosureSummary::build(classified, &patched);
            // Milestone 1423 — C216, decided by the scope pass above.
            summary.scope_method = closure_scope_method.take();
            tracing::info!(
                patches = summary.patches,
                without_cve = summary.patches_without_cve,
                distinct_cves = summary.distinct_cves,
                "nix-closure: patch attribution"
            );
            Some(summary)
        }
        _ => None,
    };

    // Milestone 1050 (#1039, #1040) — what nixpkgs itself declares about the
    // packages this build contains.
    //
    // Runs automatically under `--nix-closure`: the operator already consented
    // to evaluation by passing that flag, and this reads the same package set
    // for more of the same kind of information.
    //
    // It is not free. Measured warm on a 1,275-derivation project it costs
    // 706 ms against a 388 ms closure query -- it roughly doubles the scan
    // rather than disappearing into it, which is why `--no-nixpkgs-security`
    // exists. The default stands because the absolute cost is under a second
    // and the pass is silent on projects with nothing to declare.
    //
    // Degrades independently of the closure query. A closure that resolved
    // still emits its components when this fails, because "the closure is
    // unknown" and "what nixpkgs says about it is unknown" are different
    // gaps and a consumer needs to tell them apart.
    let nixpkgs_security_summary = match (&nix_closure, &closure_ifd) {
        (Some(Ok(classified)), Some(ifd)) if !args.no_nixpkgs_security => {
            use scan_fs::package_db::nix::declarations;
            // Timed because the pass runs automatically: an operator who
            // finds a scan slower has to be able to see whether this is why,
            // and the decision to run it without a flag rests on a ratio
            // that only a per-phase figure can settle.
            let started = std::time::Instant::now();
            let by_name: std::collections::HashMap<String, String> = components
                .iter()
                .map(|c| (c.name.clone(), c.purl.as_str().to_string()))
                .collect();
            let system = scan_fs::package_db::nix::eval::invoke::detect_host_system(
                std::time::Duration::from_secs(60),
            );
            match system.and_then(|sys| {
                declarations::run(
                    classified,
                    &root_path,
                    sys.as_str(),
                    std::time::Duration::from_secs(300),
                    ifd,
                    &|pname| by_name.get(pname).cloned(),
                )
            }) {
                Ok((mut s, resolutions)) => {
                    // US3: prose declarations become a per-component
                    // annotation. Composition facts -- each says there are
                    // components inside this one the SBOM does not list --
                    // so they belong in the SBOM, unlike the CVE-bearing
                    // entries which go only to VEX.
                    let stamped =
                        declarations::annotate_prose(&mut components, &resolutions);
                    // FR-013a. Computed here because it is a property of the
                    // interaction between two passes, and counted through the
                    // same helper the emitter withholds by, so the number
                    // cannot disagree with the document.
                    s.reconciliations_withheld =
                        declarations::withheld_pairs(&components, &s.findings).len();
                    tracing::info!(
                        checked = s.members_checked,
                        unchecked = s.members_unchecked,
                        declarations = s.declarations_total,
                        without_cve = s.declarations_without_cve,
                        distinct_cves = s.distinct_cves,
                        without_identifier = s.declarations_without_identifier,
                        distinct_identifiers = s.distinct_identifiers,
                        withheld = s.reconciliations_withheld,
                        prose_components = stamped,
                        elapsed_ms = started.elapsed().as_millis(),
                        "nixpkgs-declarations: what the package set says"
                    );
                    Some(s)
                }
                Err(reason) => {
                    tracing::info!(
                        reason = reason.wire(),
                        detail = %reason,
                        elapsed_ms = started.elapsed().as_millis(),
                        "nixpkgs-declarations: degrading; the closure is unaffected"
                    );
                    None
                }
            }
        }
        _ => None,
    };

    // `orphan`. `full` mode forwards an empty `DedupeIndex` so every
    // surviving content-shape match emits regardless of coverage.
    let file_inventory_mode = scan_fs::file_tier::FileInventoryMode::parse(&args.file_inventory)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "--file-inventory expects one of: off, orphan, full, source-tree (got: {:?})",
                args.file_inventory
            )
        })?;
    // Milestone 671 T009: cross-arg validation + composition. The
    // companion flag `--file-inventory-source-shapes` is ONLY
    // meaningful under `--file-inventory=source-tree` (FR-001). When
    // both are present, combine into a single
    // `FileInventoryMode::SourceTree { restriction: Some(set) }`; if
    // the companion flag is present under any other mode, fail loudly.
    let file_inventory_mode = match (
        file_inventory_mode,
        args.file_inventory_source_shapes.take(),
    ) {
        (scan_fs::file_tier::FileInventoryMode::SourceTree { .. }, Some(set)) => {
            scan_fs::file_tier::FileInventoryMode::SourceTree {
                restriction: Some(set),
            }
        }
        (scan_fs::file_tier::FileInventoryMode::SourceTree { .. }, None) => {
            scan_fs::file_tier::FileInventoryMode::SourceTree { restriction: None }
        }
        (_, Some(_)) => {
            return Err(anyhow::anyhow!(
                "--file-inventory-source-shapes is only meaningful when \
                 --file-inventory=source-tree (got: --file-inventory={:?}). \
                 See specs/671-file-tier-cpython/contracts/source_shape_restriction.md.",
                args.file_inventory
            ));
        }
        (other, None) => other,
    };
    // Milestone 133 US3: hoist `WalkerStats` so the per-format
    // document-level annotation emission paths can read the
    // diagnostic counters via `ScanArtifacts::file_inventory_stats`.
    let mut file_inventory_stats: Option<scan_fs::file_tier::walker::WalkerStats> = None;
    if file_inventory_mode != scan_fs::file_tier::FileInventoryMode::Off {
        // Milestone 133 US1.C: if the operator excluded the scan root
        // itself via `--exclude-path=<abs-root>`, every package-DB
        // reader emits zero components — the file-tier walker MUST
        // honor the same contract. Pre-strip the leading `/` so the
        // absolute root matches the leading-`/`-stripped literal in
        // the exclusion set.
        let root_excluded = {
            let canon =
                std::fs::canonicalize(&root_path).unwrap_or_else(|_| root_path.clone());
            let s = canon.to_string_lossy().into_owned();
            let stripped = s.trim_start_matches('/');
            !exclude_set.is_empty() && exclude_set.matches(stripped)
        };
        if root_excluded {
            tracing::info!(
                scan_root = %root_path.display(),
                "file-tier walker skipped: scan root is on --exclude-path"
            );
        } else {
        let exclusion_globs = scan_fs::file_tier::content_shape::build_orphan_exclusion_globs();
        let dedupe_index = match file_inventory_mode {
            scan_fs::file_tier::FileInventoryMode::Full => {
                // Full mode bypasses dedupe — caller wants every
                // content-shape-passing file regardless of coverage.
                scan_fs::file_tier::dedupe::DedupeIndex::default()
            }
            _ => scan_fs::file_tier::dedupe::DedupeIndex::build(&components),
        };
        // Milestone 671 T008: derive the source-tree-restriction
        // parameter from the effective mode. `SourceTree { restriction }`
        // → `Some(restriction.as_ref())`; every other mode → `None`
        // (default classifier behavior preserved).
        let source_tree_restriction: Option<
            Option<&scan_fs::file_tier::source_shape::SourceShapeSet>,
        > = match &file_inventory_mode {
            scan_fs::file_tier::FileInventoryMode::SourceTree { restriction } => {
                Some(restriction.as_ref())
            }
            _ => None,
        };
        let walker_cfg = scan_fs::file_tier::walker::WalkerConfig {
            size_limit_bytes: args.file_inventory_size_limit,
            exclusion_globs: &exclusion_globs,
            dedupe_index: &dedupe_index,
            exclude_set: &exclude_set,
            source_tree_restriction,
            // #934: directory scans honor the shared default-descent
            // skip set; `--image` rootfs scans keep the full inventory.
            skip_build_dirs: args.image.is_none(),
        };
        let (entries, stats) =
            scan_fs::file_tier::walker::walk_file_tier(&root_path, &walker_cfg);
        tracing::info!(
            file_tier_components = entries.len(),
            mode = ?file_inventory_mode,
            shape_skipped = stats.shape_skipped,
            dedupe_skipped = stats.dedupe_skipped,
            oversize_skipped = stats.oversize_skipped,
            special_skipped = stats.special_skipped,
            unreadable_skipped = stats.unreadable_skipped,
            "file-tier walker complete"
        );
        components.extend(entries.into_iter().map(|e| e.into_resolved_component()));
        // (milestone 1374, #1380) — the C209 counts are taken after this
        // point; see the `cpe_coverage` call below.
        file_inventory_stats = Some(stats);
        } // end `else` branch for !root_excluded
    }

    // Milestone 134 — aggregate divergent-PURL collision records
    // into a `CollisionsSummary` when at least one was detected.
    // `None` when empty so the document-scope annotation is
    // omitted entirely (FR-009).
    let collisions_summary: Option<waybill_common::divergence::CollisionsSummary> =
        if divergence_records.is_empty() {
            None
        } else {
            Some(waybill_common::divergence::CollisionsSummary::from_records(
                divergence_records.clone(),
            ))
        };

    // `--exclude-scope` (m052/part-3, #101), relocated here by #1209.
    //
    // Its own comment used to call it "the final step before
    // serialization", and that stopped being true when the
    // nixpkgs-haskell and m1035 nix-closure tiers landed above it.
    // Here it is true: nothing below this point adds, drops or
    // re-scopes a component.
    //
    // Four properties pin it to exactly here, and each one is a
    // position it must be *after*, not merely a tidiness preference:
    //
    //   1. **After `propagate_non_runtime_scope`** (~line 4700). This
    //      is the reason for the move. A scope the pass inferred is
    //      invisible to a filter that ran before the pass, so an
    //      operator asking to drop a scope got the declared members
    //      and kept their exclusive transitives. Worse than missing
    //      them: running the filter first DESTROYS the information,
    //      because dropping the declared members removes the seeds
    //      Phase B would have walked from, so the transitives come out
    //      carrying no scope at all — byte-identical on the wire to a
    //      runtime dependency. Measured on `go-opentelemetry` with
    //      `--exclude-scope test`: 33 of the 50 test-scoped
    //      components survived the filter unscoped before this move.
    //   2. **After the file-tier walker** (just above), which builds
    //      its dedupe index from `components`. Filtering first shrinks
    //      that index, so files covered by a dropped package stop
    //      being deduped and reappear as file-tier entries — an
    //      operator who asked for *less* would get more. Here the
    //      index is the unfiltered one. This constraint is the one
    //      argued from the code rather than measured: no available
    //      input puts a file in the scanned tree that a dropped
    //      package covers, and for Go it cannot, since module sources
    //      live in `$GOMODCACHE`. File-tier output was confirmed
    //      unchanged under the flag, which corroborates and does not
    //      prove; the measurement says so in those words.
    //   3. **Before the m167 orphan-reason classifier** (just below),
    //      which must see the graph that will actually be emitted.
    //      Dropping a component can orphan its neighbours; classified
    //      first, those orphans would reach the document with no
    //      reason, which is exactly what #1218 exists to prevent.
    //   4. **Before all three format builders**, for the same reason
    //      the propagation pass is: one filter upstream is what makes
    //      the three formats agree by construction.
    //
    // The `--tier` filter is NOT moved with it. Doing so would newly
    // subject the nixpkgs and nix-closure components to `--tier`,
    // which is a change to a different operator-visible flag; the two
    // filters commute, so leaving it above costs nothing here.
    // The counts are logged by the callee and not otherwise consumed;
    // they are returned so the unit tests can assert on them.
    let _ = apply_exclude_scope_filter(&mut components, &mut relationships, &exclude_scope);

    // Milestone 776 (FR-014a + FR-014b + SC-009a): source-provenance
    // reference summary.
    //
    // Emitted HERE, after every pass that can add or drop components,
    // rather than adjacent to the enrichment call. m776's SC-009a
    // forbids this count from overcounting relative to the emitted
    // document, and the count is taken by walking the final component
    // set rather than tallying at enrichment time precisely so that
    // the two cannot drift.
    //
    // **Moved by #1209**, from above the `--exclude-scope` filter to
    // below it. The claim in this comment's first sentence was the
    // reason: relocating the filter down here would otherwise have
    // falsified it, and measurably so — on `go-opentelemetry` with
    // `--exclude-scope test` the log read 244 against a document
    // holding 224. The claim is now true of more passes than it was
    // when written: the filter and the nixpkgs/nix-closure producers
    // are both above this point.
    {
        let mut by_kind: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for c in components.iter() {
            for r in &c.external_references {
                *by_kind.entry(r.ref_type.as_str()).or_insert(0) += 1;
            }
        }
        let total: usize = by_kind.values().sum();
        let by_kind_str = by_kind
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ");
        tracing::info!(
            total_references = total,
            by_kind = %by_kind_str,
            skipped_unmapped_label = m776_skips.unmapped_label,
            skipped_malformed_url = m776_skips.malformed_url,
            "source-provenance references emitted"
        );
    }


    // Milestone 167 (T011) — emit-time `waybill:orphan-reason`
    // classifier. Stamps per-component annotations on BFS-unreachable
    // Go/npm components per the extended C45 vocabulary + fires the
    // FR-008 grep-friendly log with per-code counters. Runs AFTER
    // every component-producing pass (package DBs, enrichment,
    // dedupe, supplement merge, file-tier walker) and BEFORE the
    // ScanArtifacts bundle is built so the mutation lands on the
    // shared `components` Vec every emitter (CDX / SPDX 2.3 / SPDX 3)
    // reads. The `waybill:orphan-reason` annotation flows through
    // the format emitters unchanged via the existing per-format
    // `extra_annotations` serialization (parity-catalog C45).
    //
    // Runs with a `RootComponentOverride::default()` placeholder for
    // classification purposes only: BFS-reachability of components
    // is over PURL keys, and override-active vs override-inactive
    // classification produces identical orphan sets because the
    // override only affects which main-module entries are dropped
    // from the emitted `components[]` (a per-format concern). The
    // actual `ScanArtifacts::root_override` below is built from the
    // operator's `--root-*` flags and is unaffected by this call.
    let _m167_orphan_reason_counts =
        crate::generate::orphan_reason::classify_orphans_pre_emit(
            &mut components,
            &relationships,
            &crate::generate::RootComponentOverride::default(),
            scan_target_coord.as_ref(),
            &target_name,
            scan_target_kind,
        );

    // Build the neutral artifacts bundle once and hand it to every
    // serializer the user requested — the single-pass guarantee of
    // FR-004 / SC-009.
    // #947: the reason the nixpkgs-Haskell pass degraded, if it did.
    let nixpkgs_haskell_degraded: Option<&str> = nixpkgs_haskell_summary
        .as_ref()
        .and_then(|s| s.degraded_reason.as_deref());
    // #973 (C174): the pass's own record. `enrich` returns `Some` exactly
    // when the pass ran, so this is absent — and the document byte-identical
    // — for a scan with no Haskell dependencies, no `flake.lock`, or
    // `--no-nixpkgs-haskell`. Rendered here so the borrow outlives
    // `artifacts`.
    let nixpkgs_haskell_resolution_json: Option<String> = nixpkgs_haskell_summary
        .as_ref()
        .map(|s| s.to_document_value().to_string());
    // #962 — `Some` exactly when the closure ran, so a scan without it is
    // byte-identical to the pre-feature output. Rendered here so the borrow
    // outlives `artifacts`.
    let nixpkgs_haskell_closure_json: Option<String> = nixpkgs_haskell_summary
        .as_ref()
        .and_then(|s| s.closure.as_ref())
        .map(|c| c.to_document_value().to_string());
    // #971 part A (C178/C180): `Some` exactly when the tier ran, so a scan
    // without `--nix-eval` is byte-identical. Rendered here so the borrow
    // outlives `artifacts`.
    let nix_eval_tier_json: Option<String> = nixpkgs_haskell_summary
        .as_ref()
        .and_then(|s| s.nix_eval.as_ref())
        .map(|n| {
            serde_json::json!({
                "revision": nixpkgs_haskell_summary
                    .as_ref()
                    .and_then(|s| s.revision.as_deref()),
                "system": n.system,
                "evaluated": n.evaluated,
                "superseded": n.superseded,
                "degraded-reason": n.degraded_reason,
            })
            .to_string()
        });
    // C190 (m1066, #1115): requested and no closure recorded, for any reason,
    // including the offline refusal before any nix process starts.
    // m1274 (#1283 part 1): the value is `{"reason": ...}` with a `"detail"`
    // when the reason carries one, not a bare wire string. Built by
    // `to_document_value` so all three emitters receive the same bytes and no
    // emitter learns about any individual variant (FR-002a, FR-004).
    let nix_closure_degraded_json: Option<String> = match &nix_closure {
        Some(Err(reason)) => Some(reason.to_document_value()),
        _ => None,
    };
    let nix_closure_degraded: Option<&str> = nix_closure_degraded_json.as_deref();
    // C217 (m1283, #1283) — the import-from-derivation policy the Nix
    // evaluations ran under. Present whenever a Nix tier was admitted, on
    // success and degradation alike; absent with no tier and under
    // `--offline`, where C190/C180 say `offline-requested`. Each tier's
    // pre-flight reports its own record; a tier that was admitted but whose
    // pre-flight never completed (nix absent, or `--nix-eval` with nothing to
    // evaluate) contributes an unverified one only when no tier verified.
    let nix_evaluation_policy_json: Option<String> =
        ((args.nix_closure || args.nix_eval) && !offline).then(|| {
            use scan_fs::package_db::nix::closure::summary::NixEvaluationPolicy;
            let eval_policy = nixpkgs_haskell_summary
                .as_ref()
                .and_then(|s| s.nix_eval.as_ref())
                .and_then(|n| n.policy.clone());
            match (closure_policy.clone(), eval_policy) {
                (Some(a), Some(b)) => a.merge(b),
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => NixEvaluationPolicy::unverified(ifd_policy),
            }
            .wire()
        });
    // C208 (m1366, #1365) — rendered beside C190 because the two answer
    // adjacent questions about the same tier: C190 says the closure was not
    // taken, C208 says what the ecosystem identity pass did with the one that
    // was.
    let closure_graph_admission_json: Option<String> = match &nix_closure {
        Some(Ok(c)) => Some(c.graph_admission.wire()),
        _ => None,
    };
    let closure_ecosystem_identity_json: Option<String> =
        closure_ecosystem_identity.as_ref().map(|r| r.wire());
    let closure_ecosystem_identity_value: Option<&str> =
        closure_ecosystem_identity_json.as_deref();
    let nix_eval_degraded: Option<&str> = nixpkgs_haskell_summary
        .as_ref()
        .and_then(|s| s.nix_eval.as_ref())
        .and_then(|n| n.degraded_reason.as_deref());

    let nix_eval_system: Option<&str> = nixpkgs_haskell_summary
        .as_ref()
        .and_then(|s| s.nix_eval.as_ref())
        .and_then(|n| n.system.as_deref());

    // Milestone 1374 (#1380) — count here, over the final vector.
    //
    // This is the only point at which both excluded populations exist:
    // closure members arrived with the closure merge, file-tier entries with
    // the file-inventory extend above.
    let cpe_coverage = cpe_coverage(&components);
    tracing::info!(
        with_candidates = cpe_coverage.with_candidates,
        addressable_without_cpe = cpe_coverage.addressable_without_cpe,
        unaddressable = cpe_coverage.unaddressable,
        excluded_members = cpe_coverage.excluded_members,
        excluded_files = cpe_coverage.excluded_files,
        "cpe: synthesis coverage"
    );
    let cpe_coverage_json = cpe_coverage.wire();

    let artifacts = ScanArtifacts {
        nix_eval_tier: nix_eval_tier_json.as_deref(),
        nix_eval_system,
        nix_eval_degraded,
        nix_closure_degraded,
        closure_ecosystem_identity: closure_ecosystem_identity_value,
        cpe_coverage: Some(cpe_coverage_json.as_str()),
        closure_graph_admission: closure_graph_admission_json.as_deref(),
        nix_evaluation_policy: nix_evaluation_policy_json.as_deref(),
        nix_closure_contents: closure_contents.as_ref().map(|c| c.wire.as_str()),
        nix_closure_contents_unread: closure_contents.as_ref().is_some_and(|c| c.unread),
        // C192 (m1067): answers served from the disk cache count as online.
        deps_dev_online: enrich_cfg.deps_dev && !offline,
        nixpkgs_haskell_degraded,
        nixpkgs_haskell_resolution: nixpkgs_haskell_resolution_json.as_deref(),
        nixpkgs_haskell_closure: nixpkgs_haskell_closure_json.as_deref(),
        scope_exemptions: scope_exemption_json.as_deref(),
        unresolved_declared_dep_count,
        target_name: &target_name,
        components: &components,
        relationships: &relationships,
        integrity: &integrity,
        complete_ecosystems: &complete_ecosystems,
        os_release_missing_fields: &os_release_missing_fields,
        scan_target_coord: scan_target_coord.as_ref(),
        generation_context: generation_context.clone(),
        include_dev: true,
        include_hashes: !args.no_hashes,
        include_source_files: true, // path-pattern evidence is the whole value prop here
        // #1084: canonical first, then as given — they differ wherever a
        // symlink sits above the root (macOS `/var` -> `/private/var`).
        scan_roots: {
            let canonical =
                std::fs::canonicalize(&root_path).unwrap_or_else(|_| root_path.clone());
            if canonical == root_path { vec![canonical] } else { vec![canonical, root_path.clone()] }
        },
        // Milestone 221 US4 (feature 221-cisa-2026-elements-audit /
        // FR-013) — operator-supplied SBOM document version.
        // `None` when `--sbom-version` is unset (byte-identity path
        // per FR-009).
        sbom_version: args.sbom_version,
        enrichment_degraded: enrichment_degraded.as_deref(),
        scope_mode: if effective_include_declared_deps {
            crate::generate::ScopeMode::Manifest
        } else {
            crate::generate::ScopeMode::Artifact
        },
        // Milestone 160 (T034/T035): doc-scope Go-transitive coverage
        // signal for the C110/C111 annotations. Distinct from
        // graph-completeness per research.md R1.
        go_transitive_coverage: go_transitive_coverage.as_ref(),
        // Milestone 172: doc-scope Go step-5 fallback counter for the
        // C117 annotation. Sibling of coverage; Go-gated per FR-002.
        go_transitive_fallback_count,
        // Milestone 173: doc-scope Go cache-warming outcome for the
        // C118 (mode) + C119 (failed) annotations. Sibling of
        // coverage; Go-gated per FR-011.
        go_cache_warming: go_cache_warming.as_ref(),
        go_mod_why,
        // Milestone 161 (T014): doc-scope Go-workspace-mode signal
        // for the C112 annotation.
        go_workspace_mode: go_workspace_mode.as_ref(),
        // Milestone 217 (waybill#631): doc-scope Go-toolchain-detected
        // signal for the C136 `waybill:go-toolchain-detected`
        // annotation. `None` when no Go toolchain was observed in the
        // scanned rootfs (byte-identity for non-Go and Go-project-only
        // scans; annotation absent).
        go_toolchains_detected: go_toolchains_detected.as_deref(),
        cross_ecosystem_edges_report: cross_ecosystem_edges_report.as_ref(),
        // Milestone 204 (#554): doc-scope helm image-extraction-mode
        // signal for the C123 annotation.
        helm_extraction_mode: helm_extraction_mode.as_ref(),
        nix_closure_summary: nix_closure_summary.as_ref(),
        nixpkgs_security_summary: nixpkgs_security_summary.as_ref(),
        pants_resolve_summary,
        // The unsplit document represents every resolve rather than one, so
        // it has no identity to state (FR-008). The split sets this per
        // projection; everything else here is repository-wide.
        resolve_identity: None,
        haskell_parse_summary,
        gradle_scan_summary: gradle_scan_summary.as_ref(),
        // Milestone 665: propagate the operator's `--no-binary-scan=<MODE>`
        // choice so every emitter can attach the doc-scope
        // `waybill:binary-scan-suppressed=<mode>` annotation. `None` on
        // the default (unset) path preserves byte-identity per FR-003.
        no_binary_scan_mode,
        // Milestone 206 (#440): doc-scope image-source signal for
        // the C124 annotation. Conditional emission (podman-only)
        // preserves FR-005 byte-identity for docker/remote scans.
        image_source: selected_image_source.as_ref(),
        scan_target_kind,
        // Milestone 072 / T010-T014: when --bind-to-source was set
        // AND the scan target is image-tier, expose the source-doc
        // identifier so each format's metadata builder can emit the
        // standards-native cross-document reference.
        source_document_binding: bind_source_ctx
            .as_ref()
            .filter(|_| args.image.is_some())
            .map(|ctx| &ctx.source_doc_id),
        // Milestone 073: identifiers — populated by T013's
        // resolution pipeline before this struct is constructed.
        identifiers: &identifiers,
        // Milestone 076: per-component user-defined identifiers from
        // `--component-id <PURL>=<scheme>:<value>` flags. Threaded to
        // per-format emitters which match against `components[].purl`.
        component_identifiers: &args.component_id,
        file_inventory_stats: file_inventory_stats.as_ref(),
        // Milestone 133 US4 (Constitution Strict Boundary §5): the
        // mode label rides as `None` when the walker didn't run
        // (`off`), `Some("orphan")` for the default mode (silent
        // passthrough — no doc-level marker), and `Some("full")`
        // for the explicit override that triggers the marker.
        file_inventory_mode: match file_inventory_mode {
            scan_fs::file_tier::FileInventoryMode::Off => None,
            scan_fs::file_tier::FileInventoryMode::Orphan => Some("orphan"),
            scan_fs::file_tier::FileInventoryMode::Full => Some("full"),
            // Milestone 671 T005 placeholder: `source-tree` is a new
            // opt-in mode. Emits the same doc-level marker as `full`
            // in terms of triggering non-orphan-mode metadata; the
            // finer-grained C156 annotation carries mode+restriction
            // and is emitted separately by T010.
            scan_fs::file_tier::FileInventoryMode::SourceTree { .. } => Some("source-tree"),
        },
        // Milestone 671 T010 — projects the parsed `--file-inventory-
        // source-shapes` restriction (if any) into a sorted-lex list
        // consumed by the C156 doc-scope emission. `None` on every
        // non-source-tree path so pre-671 SBOMs remain byte-identical
        // (SC-005). Also `None` under source-tree without a companion
        // restriction — the emitter treats that as
        // `"restriction": null`.
        file_inventory_source_shapes: match &file_inventory_mode {
            scan_fs::file_tier::FileInventoryMode::SourceTree {
                restriction: Some(set),
            } => {
                // `BTreeSet<SourceShape>` iterates in enum-discriminant
                // order (grouped by language family, NOT lex). C156
                // wants a strictly lex-sorted `Array<String>` per
                // data-model.md §"C156 Constraints"; sort explicitly
                // after `as_str()` conversion.
                let mut v: Vec<String> = set
                    .iter()
                    .map(|s| s.as_str().to_string())
                    .collect();
                v.sort();
                Some(v)
            }
            _ => None,
        },
        // Milestone 077 + issue #359: operator-supplied overrides for
        // the root component's name + version + PURL. Constructed
        // from the `--root-name` / `--root-version` / `--root-purl-type`
        // / `--no-root-purl` flags (milestone 077) plus the full-PURL
        // shortcut `--root-purl` (#359). The discrete flags +
        // `--root-purl` are clap-`conflicts_with` mutually exclusive so
        // only one branch is populated at any time.
        root_override: {
            let (full_purl, full_name, full_version) = match args.root_purl.as_deref() {
                Some(raw) => {
                    let parsed = waybill_common::types::purl::Purl::new(raw).expect(
                        "--root-purl already validated at clap parse time via validate_root_purl",
                    );
                    let name = match parsed.namespace() {
                        Some(ns) => format!("{ns}/{}", parsed.name()),
                        None => parsed.name().to_string(),
                    };
                    let version = parsed.version().unwrap_or("").to_string();
                    (Some(parsed.as_str().to_string()), Some(name), Some(version))
                }
                None => (None, None, None),
            };
            crate::generate::RootComponentOverride {
                name: args.root_name.clone(),
                version: args.root_version.clone(),
                purl_type: args.root_purl_type.clone(),
                omit_purl: args.no_root_purl,
                full_purl,
                full_purl_name: full_name,
                full_purl_version: full_version,
            }
        },
        // Milestone 149 (issue #151): opt-in flag to preserve the
        // manifest-derived main-module as a library entry when the
        // root-override flags above fire. Default OFF preserves
        // milestone-077 clean-replacement byte-identity (SC-002).
        preserve_manifest_main_module: args.preserve_manifest_main_module,
        // Milestone 080: user-provided SBOM metadata aggregated from
        // the new flags (--creator / --annotator / --annotation-comment
        // / --metadata-comment / --scan-target-name / --metadata-file).
        user_metadata: user_metadata.clone(),
        // Milestone 081: operator-asserted CISA SBOM Type from the
        // new --sbom-type flag. When set, all three formats'
        // document-level lifecycle aggregations collapse to a
        // single-element output reflecting the asserted type;
        // per-component `waybill:sbom-tier` annotations preserve
        // auto-detected values.
        sbom_type_override: args.sbom_type,
        // Issue #228: relationship-vocabulary compat flag (default
        // `full` preserves the milestone-052/part-2 typed
        // reversed-direction emission).
        spdx2_relationship_compat: args.spdx2_relationship_compat,
        // Milestone 134 — document-scope collisions summary built
        // from `divergence_records`. `None` when no divergence
        // detected (FR-009: no annotation emitted on clean scans).
        collisions_summary: collisions_summary.as_ref(),
        // Milestone 210 — scan-mode never populates the compiler-
        // pipeline field; that data comes from `waybill trace`
        // (eBPF-observed) and reaches the SBOM emitter via the
        // `sbom generate --attestation` code path. Preserving `None`
        // here per m208 defensive-default pattern.
        compiler_pipeline: None,
        // Milestone 220: document-scope project-discovery mode when
        // the scan ran under a non-default `--project-discovery` value.
        // `None` under default `All` mode OR under the FR-008 empty-
        // root fallback branch (SC-005 byte-identity gate).
        project_discovery_mode: project_discovery_mode_for_artifacts,
    };
    let output_cfg = OutputConfig {
        mikebom_version: crate::version::VERSION,
        created: scan_created_timestamp(),
        overrides: plan.overrides.clone(),
    };

    // Milestone 215 — `--split` fan-out. When set and at least one
    // workspace boundary is detected, emit one SBOM per subproject +
    // a `split-manifest.json` into `--output-dir`, then early-return.
    // Zero-boundary fallback (FR-009): `emit_split` returns
    // `Ok(false)` and we fall through to the pre-feature single-SBOM
    // emit below (WARN log already emitted).
    if let Some(mode) = args.split {
        let output_dir = args.output_dir.as_ref().expect(
            "--output-dir required when --split; validated at CLI parse",
        );
        let split_handled = crate::generate::split::emit_split(
            &artifacts,
            &plan.formats,
            &registry,
            output_dir,
            output_cfg.created,
            output_cfg.mikebom_version,
            &root_path,
            mode,
        )?;
        if split_handled {
            if args.json {
                let ctx_str = match generation_context {
                    GenerationContext::FilesystemScan => "filesystem-scan",
                    GenerationContext::ContainerImageScan => "container-image-scan",
                    GenerationContext::BuildTimeTrace => "build-time-trace",
                };
                let summary = serde_json::json!({
                    "split_output_dir": output_dir.to_string_lossy(),
                    "split_manifest": output_dir.join("split-manifest.json").to_string_lossy(),
                    "components": components.len(),
                    "relationships": relationships.len(),
                    "scanned_root": root_path.to_string_lossy(),
                    "target_name": target_name,
                    "generation_context": ctx_str,
                });
                println!("{}", serde_json::to_string_pretty(&summary)?);
            }
            return Ok(());
        }
        // else: zero-boundary fallback — fall through to normal emit.
    }

    // Milestone 221 US2a + milestone 222 US2b — construct the signing
    // mode from CLI args. Kept out of `OutputConfig` because signing is
    // a post-serialize concern that doesn't affect the emit code path;
    // passing it separately preserves the OutputConfig contract from
    // m011+. `--sign` and `--sign-key` are mutually exclusive (enforced
    // by clap `conflicts_with = "sign_key"` on the `--sign` flag), so
    // the two branches cannot both fire.
    let signing_mode = if args.sign {
        crate::sbom::signer::SigningMode::Keyless {
            fulcio_url: args.fulcio_url.clone(),
            rekor_url: args.rekor_url.clone(),
            rekor_timeout: std::time::Duration::from_secs(args.rekor_timeout_secs),
        }
    } else {
        match &args.sign_key {
            Some(path) => crate::sbom::signer::SigningMode::StaticKey {
                key_ref: path.clone(),
                passphrase_env: args
                    .sign_key_passphrase_env
                    .clone()
                    .unwrap_or_else(|| "WAYBILL_SIGN_KEY_PASSPHRASE".to_string()),
            },
            None => crate::sbom::signer::SigningMode::Unsigned,
        }
    };

    // Milestone 778 (FR-016) — per-run record of what was signed and
    // where each detached signature landed, so an operator can confirm
    // from the run's own output that a signature was produced without
    // going to the filesystem to look.
    let mut signed_artifacts: Vec<(String, String)> = Vec::new();

    // Milestone 221 US2a (FR-009a): fail-close cleanup tracker. Every
    // file we write goes into this list; on any signing failure we
    // unlink each one before propagating the error, so consumers never
    // see a partial `--output <path>` file.
    let mut written_files: Vec<PathBuf> = Vec::new();

    // Dispatch: serialize to every requested format and write each
    // emitted artifact to the chosen path. The first format's first
    // artifact path drives the backwards-compatible `--json` summary
    // output below (matches pre-milestone behavior, which only knew
    // about one file).
    let mut primary_output_path: Option<PathBuf> = None;
    let mut primary_format: Option<String> = None;
    let mut openvex_written: Option<PathBuf> = None;
    for fmt in &plan.formats {
        let serializer = registry
            .get(fmt)
            .expect("format id validated by resolve_dispatch");
        let emitted = serializer.serialize(&artifacts, &output_cfg)?;
        // The primary artifact comes first; a sidecar lands in its directory.
        let mut primary_dir: Option<PathBuf> = None;
        for artifact in emitted {
            // The primary artifact (first returned by the serializer)
            // honors the per-format --output override; side artifacts
            // (e.g. the OpenVEX sidecar in US2) always use their
            // relative_path relative to the primary's directory.
            // Three cases:
            //   (1) The primary artifact (filename == the
            //       serializer's default) → honor a per-format
            //       --output override for this `fmt`.
            //   (2) The OpenVEX sidecar (relative_path matches the
            //       sidecar's default filename) → honor the
            //       `--output openvex=<path>` pseudo-format override.
            //   (3) Any other side artifact (none today; future
            //       formats may add more) → keep its relative_path.
            let is_primary =
                artifact.relative_path == Path::new(serializer.default_filename());
            let is_openvex = !is_primary
                && artifact.relative_path
                    == Path::new(crate::generate::openvex::OPENVEX_DEFAULT_FILENAME);
            let target = if is_primary {
                let t = plan
                    .overrides
                    .get(fmt)
                    .cloned()
                    .unwrap_or_else(|| artifact.relative_path.clone());
                primary_dir = t.parent().map(Path::to_path_buf);
                t
            } else if is_openvex {
                openvex_sidecar_target(
                    plan.overrides.get(OPENVEX_PSEUDO_FORMAT),
                    primary_dir.as_deref(),
                )
            } else {
                artifact.relative_path.clone()
            };
            // #1443: each SPDX serializer returns the same sidecar, built from
            // the same scan, and every document references the one path. Write
            // and report it once.
            if is_openvex {
                if openvex_written.as_ref() == Some(&target) {
                    continue;
                }
                openvex_written = Some(target.clone());
            }
            let artifact_format = if is_openvex { OPENVEX_PSEUDO_FORMAT } else { fmt.as_str() };
            // Milestone 221 US2a — inject the signing hook at the
            // write boundary. Three cases:
            //   (a) Unsigned mode: write bytes as-emitted (FR-009
            //       byte-identity guarantee).
            //   (b) CDX + signing enabled: parse → in-place JSF sign
            //       → re-serialize → write signed bytes.
            //   (c) SPDX (2.3 or 3) + signing enabled: write primary
            //       verbatim, then emit companion DSSE sidecar at
            //       `<target>.sig.json`.
            // Milestone 778 — keyless CycloneDX signs to a detached
            // sidecar, not in-document, so it must NOT take the
            // in-document branch below. A Sigstore bundle has no
            // conformant JSON Signature Format representation (the
            // transparency-log proof has no slot), so there is nothing
            // valid to put inside the document. See #804 / m777.
            let cdx_keyless = matches!(
                signing_mode,
                crate::sbom::signer::SigningMode::Keyless { .. }
            ) && fmt == "cyclonedx-json"
                && artifact.relative_path == Path::new(serializer.default_filename());

            let final_bytes = if signing_mode.is_enabled()
                && fmt == "cyclonedx-json"
                && !cdx_keyless
                && artifact.relative_path == Path::new(serializer.default_filename())
            {
                match crate::sbom::signer::sign_cdx_bytes_for_write(&artifact.bytes, &signing_mode) {
                    Ok(signed) => signed,
                    Err(e) => {
                        cleanup_written_files(&written_files);
                        return Err(anyhow::anyhow!(
                            "signing failed for {} (target: {}): {e}",
                            fmt,
                            target.display(),
                        ));
                    }
                }
            } else if cdx_keyless {
                // Milestone 778 US2 — record where the signature will
                // live, BEFORE the document is written and therefore
                // before it is signed (FR-013). The reference is part of
                // the signed content; adding it afterwards would
                // invalidate the signature.
                //
                // The suffix comes from the signing mode rather than
                // from the resulting sidecar, because the sidecar does
                // not exist yet. The two are cross-checked below.
                let suffix = signing_mode
                    .sidecar_suffix()
                    .expect("keyless mode always produces a sidecar");
                let sidecar_name = format!(
                    "{}{suffix}",
                    target
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                );
                match crate::sbom::signer::inject_signature_reference(&artifact.bytes, &sidecar_name) {
                    Ok(with_ref) => with_ref,
                    Err(e) => {
                        cleanup_written_files(&written_files);
                        return Err(anyhow::anyhow!(
                            "cannot record signature location in {} (target: {}): {e}",
                            fmt,
                            target.display(),
                        ));
                    }
                }
            } else {
                artifact.bytes.clone()
            };
            write_bytes_to(&target, &final_bytes)?;
            written_files.push(target.clone());
            if primary_output_path.is_none() {
                primary_output_path = Some(target.clone());
                primary_format = Some(fmt.clone());
            }
            tracing::info!(
                format = %artifact_format,
                path = %target.display(),
                bytes = final_bytes.len(),
                "wrote SBOM artifact"
            );

            // Milestone 221 US2a + m222 US2b — detached signature
            // sidecar. Only for primary artifacts (not OpenVEX
            // side-artifacts). Static-key path emits `.sig.json`
            // (DSSE); Sigstore keyless path emits `.sig.bundle.json`.
            //
            // Milestone 778 added keyless CycloneDX here. The payload
            // is `final_bytes` — the exact buffer just handed to
            // `write_bytes_to` — so the signature covers the file as it
            // sits on disk and `cosign verify-blob` works against it
            // with no preprocessing (FR-004, FR-004a). Do not
            // canonicalize: that is the in-document signer's contract,
            // not this one's.
            if signing_mode.is_enabled()
                && (fmt == "spdx-2.3-json" || fmt == "spdx-3-json" || cdx_keyless)
                && artifact.relative_path == Path::new(serializer.default_filename())
            {
                match crate::sbom::signer::sign_sbom_bytes_to_sidecar(
                    &final_bytes,
                    &signing_mode,
                ) {
                    Ok(Some(sidecar_payload)) => {
                        // Milestone 778 — if the document recorded a
                        // signature location, the artifact must actually
                        // land there. The reference was written from the
                        // mode's predicted suffix and is already inside
                        // the signed bytes, so a mismatch here would ship
                        // a document pointing at a file that does not
                        // exist. Fail rather than emit that.
                        if cdx_keyless {
                            let predicted = signing_mode
                                .sidecar_suffix()
                                .expect("keyless mode always produces a sidecar");
                            let actual = sidecar_payload.sidecar_suffix();
                            if predicted != actual {
                                cleanup_written_files(&written_files);
                                return Err(anyhow::anyhow!(
                                    "signature location recorded in the document ({predicted}) \
                                     does not match the sidecar produced ({actual}); refusing to \
                                     emit a document pointing at a file that will not exist",
                                ));
                            }
                        }
                        let sidecar = target
                            .with_extension(crate::sbom::signer::sidecar_extension_for(&target, &sidecar_payload));
                        let kind = sidecar_payload.kind_label();
                        let json = sidecar_payload.to_json_bytes().map_err(|e| {
                            anyhow::anyhow!("cannot serialize {kind} sidecar: {e}")
                        })?;
                        write_bytes_to(&sidecar, &json)?;
                        written_files.push(sidecar.clone());
                        tracing::info!(
                            format = %fmt,
                            sidecar = %sidecar.display(),
                            bytes = json.len(),
                            kind = %kind,
                            "wrote SBOM signature sidecar"
                        );
                        // m779 FR-009a/FR-009b — hand the operator a
                        // command that runs as printed. Every value comes
                        // off the issued certificate, so it cannot
                        // disagree with what actually verifies; deriving
                        // the issuer from the token instead is what makes
                        // federated signatures look broken when they are
                        // not.
                        if let crate::sbom::signer::Sidecar::SigstoreBundle {
                            identity,
                            environment,
                            ..
                        } = &sidecar_payload
                        {
                            let cmd =
                                crate::attestation::signer::VerificationCommand::render(
                                    *environment,
                                    identity,
                                    &target,
                                    &sidecar,
                                );
                            if cmd.is_template {
                                eprintln!(
                                    "\nTo verify (template — supply your deployment's trust root):\n{}\n",
                                    cmd.rendered
                                );
                            } else {
                                eprintln!("\nTo verify:\n{}\n", cmd.rendered);
                            }
                        }
                        signed_artifacts
                            .push((fmt.clone(), sidecar.display().to_string()));
                    }
                    Ok(None) => {
                        // Unreachable — is_enabled() gate already checked.
                        unreachable!("Unsigned mode short-circuited above");
                    }
                    Err(e) => {
                        cleanup_written_files(&written_files);
                        return Err(anyhow::anyhow!(
                            "signing failed for {} sidecar: {e}",
                            fmt,
                        ));
                    }
                }
            }
        }
    }

    // Milestone 778 FR-016 / SC-010 — signing summary.
    if !signed_artifacts.is_empty() {
        tracing::info!(
            signed_formats = %signed_artifacts
                .iter()
                .map(|(f, _)| f.as_str())
                .collect::<Vec<_>>()
                .join(","),
            artifacts = %signed_artifacts
                .iter()
                .map(|(f, p)| format!("{f}={p}"))
                .collect::<Vec<_>>()
                .join(" "),
            count = signed_artifacts.len(),
            "detached signature artifacts written"
        );
    }

    if args.json {
        let ctx_str = match generation_context {
            GenerationContext::FilesystemScan => "filesystem-scan",
            GenerationContext::ContainerImageScan => "container-image-scan",
            GenerationContext::BuildTimeTrace => "build-time-trace",
        };
        let summary = serde_json::json!({
            "output_file": primary_output_path
                .as_ref()
                .map(|p| p.to_string_lossy())
                .unwrap_or_default(),
            "format": primary_format.clone().unwrap_or_default(),
            "components": components.len(),
            "relationships": relationships.len(),
            "scanned_root": root_path.to_string_lossy(),
            "target_name": target_name,
            "generation_context": ctx_str,
        });
        println!("{}", serde_json::to_string_pretty(&summary)?);
    }

    // Milestone 173 — FR-004 advisory log. Emitted at INFO level
    // exactly once when ALL FOUR predicates hold:
    //   1. Scan produced ≥1 Go component (FR-009 gate).
    //   2. `--offline` is NOT set.
    //   3. `--warm-go-cache` was NOT explicitly set (took default `off`).
    //   4. The C117 `waybill:go-transitive-fallback-count` value is > 0.
    // Suppressed otherwise. The literal string here MUST match
    // contracts/cli-surface.md verbatim so consumers can grep with a
    // stable substring.
    let advisory_ctx = AdvisoryContext {
        fallback_count: go_transitive_fallback_count,
        warm_flag_was_default: !std::env::args()
            .any(|a| a.starts_with("--warm-go-cache=")),
        offline,
        scan_has_go_components: components
            .iter()
            .any(|c| c.purl.as_str().starts_with("pkg:golang/")),
    };
    if advisory_ctx.should_advise() {
        tracing::info!(
            "waybill:go-transitive-fallback-count > 0 detected. Prime the cache with --warm-go-cache=per-workspace or 'go mod download' per workspace before scanning."
        );
    }

    // Milestone 176 — FR-004 advisory log. Emitted at INFO level
    // exactly once when TWO predicates hold:
    //   1. The scan detected N > 1 workspaces (union of every
    //      component's `waybill:workspace-member` annotation).
    //   2. The scan produced ≥1 component (else there's nothing to
    //      slice per-workspace and no useful advice to give).
    // Suppressed otherwise (single-project + bare-directory scans stay
    // quiet per FR-005). NOT gated on --offline: the remediation
    // (jq per-workspace slicing) is entirely consumer-side and
    // requires no network (per FR-006). The stable grep substring
    // `"monorepo shape detected: "` is load-bearing — dashboards
    // grep-detect monorepo scans via this token.
    {
        use std::collections::BTreeSet;
        let mut workspaces: BTreeSet<String> = BTreeSet::new();
        for c in &components {
            if let Some(v) = c.extra_annotations.get("waybill:workspace-member") {
                if let Some(s) = v.as_str() {
                    if let Ok(paths) = serde_json::from_str::<Vec<String>>(s) {
                        for p in paths {
                            workspaces.insert(p);
                        }
                    }
                }
            }
        }
        if workspaces.len() > 1 && !components.is_empty() {
            let list = workspaces.iter().cloned().collect::<Vec<_>>().join(", ");
            tracing::info!(
                "monorepo shape detected: {} workspaces ({}). Downstream consumers can filter per-workspace via `waybill:workspace-member`; see docs/reference/monorepos.md for jq recipes.",
                workspaces.len(),
                list,
            );
        }
    }

    // Milestone 175 — FR-002 advisory log. Emitted at INFO level exactly
    // once when THREE predicates hold:
    //   1. At least one component has `sbom_tier = "design"` (constraint-
    //      only manifest — pip `requirements.txt` alone, Ruby `Gemfile`
    //      without `Gemfile.lock`, npm root `package.json` without
    //      `package-lock.json`, etc.).
    //   2. The scan produced ≥1 component (empty scans stay quiet).
    //   3. The `WAYBILL_NO_DESIGN_TIER_ADVISORY` env var is unset (or
    //      set to a value other than "1" / "true"). Env-var precedent:
    //      milestone-110 `WAYBILL_NO_DEPRECATION_NOTICE=1`.
    // NOT gated on --offline: the remediation (generate a lockfile,
    // install into a venv) works fully offline (FR-002 explicit).
    // Stable grep substring: "design-tier components detected: " —
    // dashboards grep-detect design-tier scans via this token.
    {
        let design_tier_count = components
            .iter()
            .filter(|c| c.sbom_tier.as_deref() == Some("design"))
            .count();
        let suppress = std::env::var("WAYBILL_NO_DESIGN_TIER_ADVISORY")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if design_tier_count > 0 && !components.is_empty() && !suppress {
            tracing::info!(
                "design-tier components detected: {design_tier_count} components lack \
                 resolved versions. Remediation: generate a lockfile (uv lock / poetry \
                 lock / pip-compile / npm install / bundle lock / cargo generate-lockfile) \
                 OR install into a venv and re-scan. See \
                 docs/reference/reading-a-waybill-sbom.md#design-tier-components for jq \
                 recipes and per-ecosystem guidance."
            );
        }
    }

    tracing::info!(
        output = %primary_output_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        components = components.len(),
        relationships = relationships.len(),
        "SBOM written"
    );
    Ok(())
}

/// Milestone 173 — FR-004 advisory-log predicate. Fires the "prime
/// the cache" hint exactly once per scan when the operator's env is
/// degraded AND they haven't explicitly opted in or out of warming.
///
/// The four-input contract is codified in data-model.md Entity 8:
/// suppression on any single false gate — non-Go scan (FR-009),
/// offline mode (FR-004 offline exception), explicit flag (operator
/// took a stance), or clean fallback count (nothing to advise about).
#[derive(Debug)]
struct AdvisoryContext {
    fallback_count: Option<usize>,
    warm_flag_was_default: bool,
    offline: bool,
    scan_has_go_components: bool,
}

impl AdvisoryContext {
    fn should_advise(&self) -> bool {
        self.scan_has_go_components
            && !self.offline
            && self.warm_flag_was_default
            && self.fallback_count.map(|n| n > 0).unwrap_or(false)
    }
}

/// Fold the closure's components into the manifest-derived set.
///
/// Spec FR-003a says the closure supplements and never replaces. A plain
/// append satisfies the letter of that and breaks its intent: the closure
/// identifies members as `pkg:generic/<name>@<version>`, because `pkg:nix`
/// is not a purl-spec type and a wrong-but-well-formed PURL would be worse
/// than a generic one. Appending a generic twin beside an existing
/// `pkg:hackage/...` leaves the document asserting two identities for one
/// package.
///
/// So a closure member that matches something already present by
/// `(name, version)` contributes its annotations to that component -- the
/// role, and any patches it applies -- and does not become a second entry.
/// The existing PURL is kept, since the reader that produced it knew the
/// ecosystem and the closure does not. Only members nothing else found are
/// appended, which is where the tier's value actually is: the C toolchain,
/// the build tooling, and the patches applied to them.
///
/// Matching is on `(name, version)` and not on name alone. A design-tier
/// component whose version never resolved therefore does not absorb a
/// closure member of the same name: measured, one of 370 components comes
/// out as both `pkg:hackage/os-string` (design, no version) and
/// `pkg:generic/os-string@2.0.10` (build). Those are two different
/// observations -- what the manifest declares, and what the build used --
/// and collapsing them would assert the manifest resolved something it did
/// not. Folding design into build is the reconciler's kind of job, not this
/// function's; the milestone-191 reconciler folds design into *source* and
/// deliberately leaves build alone.
///
/// Returns `(merged, appended)`.
/// Milestone 1366 (#1365) — give the closure's PyPI components an identifier a
/// vulnerability feed can match.
///
/// # Where this runs
///
/// Between the closure merge and `closure::graph::relationships`, and that
/// window is the mechanism rather than a convenience. `Relationship.from` and
/// `.to` are PURL **strings**, so an identifier changed after the edges are
/// built orphans every edge pointing at it — which would turn each correctly
/// identified package into an orphan and make this feature a net regression.
/// It does not happen because `relationships` builds its join table from the
/// live component slice, reading `c.purl` when it is called. Move this call
/// below that one and `closure_ecosystem_identity_runs_before_the_edges` goes
/// red.
///
/// # Why a named function
///
/// The same reason `build_deps_dev_source` is one (#927): wiring expressed
/// inline at the call site is wiring no test can reach. The m923 CLI tests
/// asserted on clap's parse of a flag while a mutation of the inline wiring
/// left all three green and every scan taking the wrong path.
async fn run_closure_ecosystem_identity(
    classified: &scan_fs::package_db::nix::closure::ClassifiedClosure,
    components: &mut [waybill_common::resolution::ResolvedComponent],
    appended_identities: &std::collections::HashSet<(String, String)>,
    confirmation: Option<&crate::enrich::depsdev_source::DepsDevSource>,
) -> scan_fs::package_db::nix::closure::ecosystem_identity::ConfirmationRecord {
    use crate::enrich::depsdev_source::LookupResult;
    use crate::enrich::request_key::EnrichmentKey;
    use scan_fs::package_db::nix::closure::ecosystem_identity as eco;

    let found = eco::candidates(classified, components, appended_identities);
    let (eligible, settled) = eco::screen(found);

    let mut outcomes: Vec<(eco::EcosystemCandidate, eco::ConfirmationOutcome)> = settled;

    match confirmation {
        // FR-009 / the corpus path. `--no-deps-dev` is an instruction, not a
        // hint: the one corpus target that runs a closure passes it, so a
        // corpus run always lands here and never confirms anything. That
        // blindness is recorded in the spec and compensated by the stubbed
        // test, not papered over by querying anyway.
        None => outcomes.extend(
            eligible
                .into_iter()
                .map(|c| (c, eco::ConfirmationOutcome::Unchecked(eco::UncheckedReason::Disabled))),
        ),
        Some(source) => {
            let keys: Vec<Option<EnrichmentKey>> = eligible
                .iter()
                .map(|c| {
                    EnrichmentKey::from_purl_parts(c.ecosystem, None, &c.normalised_name, &c.version)
                })
                .collect();
            // A candidate that yields no key is settled without asking, and
            // recorded rather than dropped: an unusable entry in a batch
            // consumes a slot and comes back empty, which is
            // indistinguishable from a package the registry does not carry.
            let askable: Vec<EnrichmentKey> = keys.iter().flatten().cloned().collect();
            let answers = source.confirm(&askable).await;

            let mut next = answers.into_iter();
            for (candidate, key) in eligible.into_iter().zip(keys) {
                let outcome = match key {
                    None => eco::ConfirmationOutcome::Unchecked(eco::UncheckedReason::Unkeyable),
                    Some(_) => match next.next() {
                        // Exhaustive on purpose: a state added to
                        // `LookupResult` must break this build rather than
                        // fall into a default that reads as "could not ask".
                        Some(LookupResult::Found(_)) => {
                            eco::ConfirmationOutcome::from_answer(eco::LookupAnswer::Found)
                        }
                        Some(LookupResult::Absent) => {
                            eco::ConfirmationOutcome::from_answer(eco::LookupAnswer::Absent)
                        }
                        Some(LookupResult::Failed) => {
                            eco::ConfirmationOutcome::from_answer(eco::LookupAnswer::Failed)
                        }
                        Some(LookupResult::Unqueried) => {
                            eco::ConfirmationOutcome::from_answer(eco::LookupAnswer::Unqueried)
                        }
                        // Fewer answers than keys cannot happen — `confirm`
                        // returns one per key — and if it ever did, the
                        // honest reading is that we did not learn.
                        None => eco::ConfirmationOutcome::Unchecked(
                            eco::UncheckedReason::Unreachable,
                        ),
                    },
                };
                outcomes.push((candidate, outcome));
            }
        }
    }

    for (candidate, outcome) in &outcomes {
        eco::apply_outcome(components, candidate, *outcome);
    }

    let record = eco::ConfirmationRecord::tally(
        &outcomes.iter().map(|(_, o)| *o).collect::<Vec<_>>(),
    );
    debug_assert!(record.is_consistent(), "C208 counts must add up");
    tracing::info!(
        candidates = record.candidates,
        confirmed = record.confirmed,
        declined = record.declined,
        unchecked = record.unchecked,
        reason = record.reason.map(|r| r.wire()).unwrap_or("-"),
        "nix-closure: ecosystem identity"
    );
    record
}

/// Milestone 1378 (#1396) — what the contents pass leaves for the rest of
/// the scan: the rendered C213 record and the facts the strict check needs.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ClosureContentsSummary {
    wire: String,
    unread: bool,
    root: scan_fs::package_db::nix::closure::contents::RootState,
    root_label: Option<String>,
}

struct ClosureContentsRun {
    relationships: Vec<waybill_common::resolution::Relationship>,
    summary: ClosureContentsSummary,
}

/// Milestone 1378 (#1396) — read present outputs, settle the identities
/// binaries report for themselves, and append the recovered modules.
///
/// A named function for the reason [`run_closure_ecosystem_identity`] is one:
/// wiring expressed inline is wiring no test can reach.
///
/// `confirmation` is `None` when deps.dev is off. Then every candidate is
/// recorded `unchecked: disabled` and nothing is promoted — an identifier
/// built by joining a binary's module path to a closure's version is an
/// inference until a registry has seen it (FR-012b).
async fn run_closure_contents(
    classified: &scan_fs::package_db::nix::closure::ClassifiedClosure,
    components: &mut Vec<waybill_common::resolution::ResolvedComponent>,
    store: &std::path::Path,
    confirmation: Option<&crate::enrich::depsdev_source::DepsDevSource>,
) -> ClosureContentsRun {
    use crate::enrich::depsdev_source::LookupResult;
    use crate::enrich::request_key::EnrichmentKey;
    use scan_fs::package_db::nix::closure::contents::{self, IdentityStatus, UncheckedReason};

    let outcome = contents::read(classified, components, store);

    // Settle identities. A finding the pass already settled (ambiguous, no
    // version, existing identity, collision) is not asked about.
    let mut statuses: Vec<IdentityStatus> = Vec::with_capacity(outcome.identities.len());
    let keys: Vec<Option<EnrichmentKey>> = outcome
        .identities
        .iter()
        .map(|f| {
            if f.settled.is_some() {
                return None;
            }
            EnrichmentKey::from_purl_parts(
                f.registry_system()?,
                None,
                f.name.as_deref()?,
                f.version.as_deref()?,
            )
        })
        .collect();
    let askable: Vec<EnrichmentKey> = keys.iter().flatten().cloned().collect();
    let mut answers = match confirmation {
        Some(source) if !askable.is_empty() => source.confirm(&askable).await.into_iter(),
        _ => Vec::new().into_iter(),
    };
    for (finding, key) in outcome.identities.iter().zip(&keys) {
        let status = match (finding.settled, key, confirmation) {
            (Some(settled), _, _) => settled,
            (None, None, _) => IdentityStatus::Unchecked(UncheckedReason::Unkeyable),
            (None, Some(_), None) => IdentityStatus::Unchecked(UncheckedReason::Disabled),
            (None, Some(_), Some(_)) => match answers.next() {
                // Exhaustive on purpose, as in the m1366 pass: a state added
                // to `LookupResult` must break this build rather than fall
                // into a default.
                Some(LookupResult::Found(_)) => IdentityStatus::Confirmed,
                Some(LookupResult::Absent) => IdentityStatus::Declined,
                Some(LookupResult::Failed) | Some(LookupResult::Unqueried) | None => {
                    IdentityStatus::Unchecked(UncheckedReason::Unreachable)
                }
            },
        };
        statuses.push(status);
    }
    for (finding, status) in outcome.identities.iter().zip(&statuses) {
        contents::apply_identity(components, finding, *status);
    }

    // Edges last: they are read against the members' final identifiers.
    let relationships = outcome.relationships(components);
    let recovered = outcome.components.len();
    let record = &outcome.record;
    tracing::info!(
        members = record.members,
        read = record.read,
        partial = record.partial,
        absent = record.absent,
        failed = record.failed,
        yielded = record.yielded,
        golang = record.modules.get("golang").copied().unwrap_or(0),
        cargo = record.modules.get("cargo").copied().unwrap_or(0),
        recovered_components = recovered,
        identities_confirmed = statuses
            .iter()
            .filter(|s| **s == IdentityStatus::Confirmed)
            .count(),
        root = record.root.wire(),
        store = %store.display(),
        "nix-closure: contents"
    );
    let summary = ClosureContentsSummary {
        wire: record.wire(),
        unread: record.has_unread(),
        root: record.root,
        root_label: outcome.root_label.clone(),
    };
    components.extend(outcome.components);
    ClosureContentsRun {
        relationships,
        summary,
    }
}

/// Milestone 1378 (#1396), FR-015a/b — what an unread closure root means for
/// this scan.
///
/// `Ok` to continue (having warned, if there was something to warn about);
/// `Err` with the message to fail with. Pure, so both branches are testable
/// without a store or a nix.
///
/// `tier` is `None` when `--nix-closure` produced no attempt, `Some(Err)`
/// with the degradation's wire reason when it degraded, `Some(Ok)` otherwise.
fn closure_root_contents_verdict(
    requested: bool,
    strict: bool,
    tier: Option<Result<(), String>>,
    contents: Option<&ClosureContentsSummary>,
) -> Result<(), String> {
    use scan_fs::package_db::nix::closure::contents::RootState;
    const FLAG: &str = "--nix-closure-require-root-contents";

    if !requested {
        return Ok(());
    }
    let (label, state) = match (tier, contents) {
        (Some(Ok(())), Some(c)) => (c.root_label.as_deref(), c.root),
        (Some(Err(reason)), _) => {
            return if strict {
                Err(format!(
                    "{FLAG}: the closure was not recorded ({reason}), so its root's contents could not be read"
                ))
            } else {
                // The tier already logged its degradation and C190 records it.
                Ok(())
            };
        }
        _ => {
            return if strict {
                Err(format!("{FLAG}: no closure was taken, so there is no root to read"))
            } else {
                Ok(())
            };
        }
    };
    let (satisfied, wire) = match state {
        RootState::Member(s) => (s.satisfies_strict(), s.wire()),
        RootState::None => (false, "none"),
    };
    let name = label.unwrap_or("<unknown>");
    if satisfied {
        if wire != "read" {
            tracing::warn!(
                root = name,
                state = wire,
                "nix-closure: the closure root was only partly read; some of its outputs are not present in the store"
            );
        }
        return Ok(());
    }
    let why = match state {
        RootState::None => "the closure names no single root component".to_string(),
        _ => format!("the closure root `{name}` could not be read ({wire})"),
    };
    if strict {
        return Err(format!(
            "{FLAG}: {why}. Its built output must be present in the nix store; build or fetch it first. Nothing was written."
        ));
    }
    tracing::warn!(
        root = name,
        state = wire,
        "nix-closure: {why}; the modules compiled into it are NOT in this document. Build or fetch the package first, or pass {FLAG} to make this an error"
    );
    Ok(())
}

/// Milestone 1367 (#1371) — the document-scope record of what CPE synthesis
/// did (C209).
///
/// Two counts rather than one, and that is the whole point. A document with
/// `with_candidates: 0, eligible_unmapped: 0` says *"there was nothing to
/// do"*. One with `with_candidates: 0, eligible_unmapped: 264` says **"the
/// pass did not run"**. Those were the same document for a whole milestone —
/// zero CPEs either way — which is how `synthesize_cpes` came to be invisible
/// to every closure component without anything failing, warning or logging.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CpeSynthesisCoverage {
    /// Components carrying at least one CPE candidate after the pass.
    ///
    /// Unchanged in meaning by milestone 1373 (FR-006), and the sameness was
    /// measured rather than argued: the post-pass "has CPEs" set is exactly
    /// (components that already had CPEs) ∪ (components the synthesizer
    /// served), which is what the v1 field counted. 78 either way on the
    /// pinned closure.
    pub(crate) with_candidates: usize,
    /// Components with no CPE whose PURL type a vulnerability feed indexes.
    ///
    /// **Not a gap.** A `pkg:hackage` component is answerable through OSV
    /// using the PURL this document already carries; measured end to end,
    /// six of the closure's 163 return advisories, with the affected-version
    /// range applied.
    pub(crate) addressable_without_cpe: usize,
    /// Components with no CPE whose PURL type no feed indexes.
    ///
    /// **This is the gap.** `pkg:generic` names no registry, so a CPE is the
    /// only route to a feed at all: `pkg:generic/openssl@3.0.0` and
    /// `pkg:generic/zlib@1.2.11` return nothing from OSV despite well-known
    /// CVEs.
    ///
    /// Expect this to be the largest count on a closure document — 733 of 977
    /// on the pinned one — and that is the honest figure rather than a
    /// regression. The v1 field it replaces read 284, because its
    /// release-shape gate excluded most `pkg:generic` components while
    /// counting `pkg:hackage` ones that were never unreachable.
    pub(crate) unaddressable: usize,
    /// Closure members — patches, source tarballs, build fragments
    /// (milestone 1374, #1380).
    ///
    /// **Not software**, and therefore in none of the three counts above.
    /// Milestone 1373 counted them `unaddressable`, which made that word
    /// answer two questions at once: 539 of its 733 on the pinned closure
    /// were `aeson-2.2.4.1.tar.gz` and `CVE-2019-13232-1.patch`.
    ///
    /// C204 `waybill:nix-closure` carries the per-role breakdown; this is
    /// only the total, so the row balances its own books.
    pub(crate) excluded_members: usize,
    /// File-tier entries (milestone 1374, #1380).
    ///
    /// Also not software. Counted separately from members rather than folded
    /// in: "is a source tarball" and "is a file" are different facts, and
    /// collapsing them would repeat this row's own defect one level down.
    pub(crate) excluded_files: usize,
}

impl CpeSynthesisCoverage {
    /// The C209 value: a JSON object encoded as a string, on the C192 / C204 /
    /// C208 template.
    ///
    /// `/v2` because the shape changed incompatibly. A consumer reading
    /// `eligible_unmapped` now gets nothing, and the schema string is how it
    /// learns that rather than reading an absent key as zero.
    pub(crate) fn wire(&self) -> String {
        let mut obj = serde_json::Map::new();
        obj.insert("schema".into(), "waybill-cpe-coverage/v3".into());
        obj.insert("with_candidates".into(), self.with_candidates.into());
        obj.insert(
            "addressable_without_cpe".into(),
            self.addressable_without_cpe.into(),
        );
        obj.insert("unaddressable".into(), self.unaddressable.into());
        obj.insert("excluded_members".into(), self.excluded_members.into());
        obj.insert("excluded_files".into(), self.excluded_files.into());
        serde_json::Value::Object(obj).to_string()
    }

    /// The five counts partition the resolved component set (FR-002, SC-002).
    ///
    /// **Every resolved component**, not a subset the reader has to derive.
    /// Under `/v2` the sum matched only components that carried a PURL and
    /// were not `type: "file"`, which left 11 of 17 corpus targets
    /// uncheckable without knowing that rule.
    ///
    /// Milestone 1376 (#1385) corrected an overreach in this sentence. The
    /// resolved set is not the same thing as "the document's components",
    /// because each emitter projects it differently: CycloneDX may promote
    /// the root out of `.components`, and SPDX 3 splits it across `Package`
    /// and `File` nodes. The row is right in all three formats; the
    /// expression that recovers it is per-format, and
    /// `docs/reference/sbom-format-mapping.md` states all three.
    pub(crate) fn total(&self) -> usize {
        self.with_candidates
            + self.addressable_without_cpe
            + self.unaddressable
            + self.excluded_members
            + self.excluded_files
    }
}

/// PURL type → the OSV ecosystem that indexes it (milestone 1373, FR-003).
///
/// # Both halves, and why neither alone will do
///
/// A row may exist only when **both** hold against committed output in
/// `specs/1373-hackage-cpe-decision/measurements/`:
///
/// 1. the ecosystem appears in OSV's published list
///    (`osv-ecosystems-2026-11-07.txt`, 47 entries), and
/// 2. a probe returned that ecosystem's advisories for a known-vulnerable
///    version.
///
/// Membership alone asserts coverage nobody measured. A probe alone admits
/// `generic`: `pkg:generic/curl@7.60.0` returns exactly one advisory, and it
/// is `OSV-2017-79`, an OSS-Fuzz finding keyed on a project name rather than
/// any curl CVE — while `openssl@3.0.0` and `zlib@1.2.11` return zero despite
/// well-known CVEs. OSV's list has no `generic`, so (1) excludes it.
///
/// # Why only three rows
///
/// A sixteen-row draft covering every type waybill emits was cut: thirteen
/// satisfied (1) alone. The cut changed no emitted figure — across all 17
/// corpus targets, **zero** components are classified addressable on an
/// unprobed type, because `cargo`, `golang`, `maven` and the rest already
/// carry CPEs from existing arms and never reach that branch.
///
/// A type absent from this table is [`FeedAddressability::Unaddressable`].
/// That is conservative on purpose: the honest answer for an unprobed type is
/// "nobody measured whether a feed indexes this".
///
/// # Maintenance
///
/// **This is a dated observation, not a truth.** OSV adds ecosystems. Adding
/// a row without re-probing is the defect `scripts/probe-cpe-dictionary.sh`
/// exists to prevent, one feed over. `measurements/partition.py` holds the
/// same table and a test asserts they stay in step.
pub(crate) const OSV_INDEXED_PURL_TYPES: &[(&str, &str)] = &[
    ("hackage", "Hackage"),
    ("npm", "npm"),
    ("pypi", "PyPI"),
];

/// Can any vulnerability feed address this component? (milestone 1373)
///
/// A **closed enum with an exhaustive match**, not an allowlist consulted
/// with a default arm. Issue #892 survived roughly two years behind an
/// allowlist that fell out of date silently; the same shape here would let a
/// new PURL type be counted without anyone noticing it was never considered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FeedAddressability {
    /// Carries at least one CPE, from any source.
    WithCandidates,
    /// No CPE, but a feed indexes this PURL type.
    AddressableWithoutCpe,
    /// No CPE, and no feed indexes this PURL type.
    Unaddressable,
    /// A closure member — a patch, a source tarball, a build fragment
    /// (milestone 1374, #1380).
    ///
    /// Not software, so not a candidate for any of the three counts above.
    /// `bash-5.3.tar.gz` is named after bash and is not bash; the component
    /// that really is bash is emitted separately.
    ExcludedMember,
    /// A file-tier entry (milestone 1374, #1380).
    ///
    /// `utils.sh` is a file. Counting one `Unaddressable` would make that
    /// word mean two different things in one row.
    ExcludedFile,
}

/// Classify one component.
///
/// `WithCandidates` wins over addressability, and the precedence is
/// load-bearing: 72 `pkg:pypi` components on the pinned closure both carry a
/// CPE and belong to an OSV-indexed ecosystem. Counting them in two buckets
/// would break the partition; counting them as merely addressable would hide
/// that the synthesizer served them.
pub(crate) fn classify_addressability(
    component: &waybill_common::resolution::ResolvedComponent,
) -> FeedAddressability {
    // Milestone 1374 (#1380) — the exclusions come **first**, and the order
    // is load-bearing.
    //
    // FR-003: a member is excluded because of what it *is*, not because of
    // what was or was not synthesized for it. The synthesizer's guard means
    // no member carries a CPE today — 0 of 539 on darwin, 0 of 1,087 on
    // linux — but if that guard changed, testing `cpes` first would let a
    // member re-enter the partition through `with_candidates`.
    //
    // Membership also wins over addressability, and that is not
    // hypothetical: `pkg:hackage/time` on the linux closure is a member with
    // role `fragment` *and* carries an OSV-addressable PURL.
    if component
        .extra_annotations
        .contains_key(crate::scan_fs::package_db::nix::closure::member::ANN_MEMBER_ROLE)
    {
        return FeedAddressability::ExcludedMember;
    }
    // The in-memory signal is the annotation, NOT `type` and NOT the absence
    // of a PURL. `ResolvedComponent` has no `type` field — CDX derives
    // `type: "file"` at `cyclonedx/builder.rs` from this same annotation —
    // and a file-tier component carries
    // `pkg:generic/file-tier?content-sha256=…` in memory, which the emitter
    // drops. Testing "no PURL" here would match nothing.
    if component
        .extra_annotations
        .get(crate::scan_fs::file_tier::COMPONENT_TIER_KEY)
        .and_then(|v| v.as_str())
        == Some(crate::scan_fs::file_tier::COMPONENT_TIER_FILE_VALUE)
    {
        return FeedAddressability::ExcludedFile;
    }
    if !component.cpes.is_empty() {
        return FeedAddressability::WithCandidates;
    }
    let ecosystem = component.purl.ecosystem();
    if OSV_INDEXED_PURL_TYPES
        .iter()
        .any(|(purl_type, _)| *purl_type == ecosystem)
    {
        FeedAddressability::AddressableWithoutCpe
    } else {
        FeedAddressability::Unaddressable
    }
}

/// Is this version string something NVD could match? (FR-003)
///
/// A CPE's version field is a **release identifier**. `format_cpe` escapes per
/// CPE 2.3 §6.2, so a strange version does not produce a corrupt string — it
/// produces a syntactically valid one that matches nothing, like
/// `cpe:2.3:a:x:x:0-unstable-2018-06-17:*:*:*:*:*:*:*`. Emitting those would
/// overstate what a scanner can do with the document, which is a worse failure
/// than emitting nothing: a count of 40 CPEs of which 34 cannot match reads as
/// coverage.
///
/// **This lives here rather than inside `generate::cpe::synthesize_cpes`** on
/// purpose. That function is shared with every deb, apk and rpm component in
/// every existing golden; adding a gate to it would change their output as a
/// side effect of fixing a closure ordering bug.
///
/// The declining shapes are not hypothetical. All five exist on the pinned
/// closure today: `21.1.6+apple-sdk-26.4` (libcxx), `20251016-3.1` (libedit),
/// `0-unstable-2018-06-17` (CoreSymbolication), `26.05pre-git` (nixpkgs
/// hooks), `boot` (bootstrap stages). None is currently mapped, so this gate
/// changes nothing today and exists for the table's growth — which is why its
/// test uses constructed input rather than the corpus.
fn is_release_shaped(version: &str) -> bool {
    if version.is_empty() {
        return false;
    }
    // #1283 — nixpkgs' `<release>-<patch level>` (glibc `2.40-66`): grype
    // matches the whole string as a CPE version. Admitted only when the
    // release is dotted digits and the patch level is digits, so none of
    // the declined shapes above (`20251016-3.1`, `0-unstable-...`) qualify.
    if let Some((release, patch)) = version.split_once('-') {
        return release.contains('.')
            && release.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            && !patch.is_empty()
            && patch.bytes().all(|b| b.is_ascii_digit());
    }
    let mut chars = version.chars().peekable();
    // A leading digit is required: `boot` and `v1.2` are both out.
    if !chars.peek().is_some_and(char::is_ascii_digit) {
        return false;
    }
    let mut seen_alpha = false;
    for ch in version.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            // A letter suffix may not be followed by more separators:
            // `1.0.2k` is a release, `20251016-3.1` is not.
            if ch == '.' && seen_alpha {
                return false;
            }
            continue;
        }
        if ch.is_ascii_lowercase() && !seen_alpha {
            seen_alpha = true;
            continue;
        }
        return false;
    }
    true
}

/// Milestone 1367 (#1371) — give the closure's components the CPE candidates
/// every other reader's components already get.
///
/// # The defect this closes
///
/// `c.cpes = synthesize_cpes(c)` runs at `scan_fs/mod.rs:1346`, inside
/// `read_all`, over the post-dedup component set. The closure's components are
/// merged in later, in `execute`. The pass had therefore never seen a single
/// closure component, and six that match the curated table exactly by name —
/// `openssl 3.6.2`, `curl 8.20.0`, `zlib 1.3.2`, `sqlite 3.51.2`,
/// `pcre2 10.46`, `llvm 21.1.8` — carried nothing.
///
/// # Why fill-only-if-empty, and not reassign
///
/// `supplement::merge` lands the operator's `--supplement-cdx` CPEs **before**
/// this runs. An unconditional `c.cpes = synthesize_cpes(c)` would overwrite
/// an operator's explicit identifiers with synthesized guesses — and nothing
/// on the pinned closure would reveal it, because no component there both
/// receives a supplement and matches the table. The guard is what makes FR-004
/// and FR-005 true by construction rather than by care.
///
/// # Why not move the existing pass instead
///
/// `package_db::read_all` has two production callers — `scan_fs/mod.rs` and
/// `report/mod.rs`. Moving the pass would strip CPEs from the report path,
/// which has nothing to do with this defect.
fn fill_missing_cpes(components: &mut [waybill_common::resolution::ResolvedComponent]) {
    for c in components.iter_mut() {
        if !c.cpes.is_empty() {
            // An earlier pass, or the operator. Not ours to recompute.
            continue;
        }
        if !is_release_shaped(&c.version) {
            continue;
        }
        // FR-002 — a closure member is a patch file, a source tarball or a
        // build fragment, not installed software. `bash-5.3.tar.gz` is named
        // after bash and is not bash; a CPE on it would tell a scanner that
        // bash 5.3 is present, and the component that really is bash is
        // emitted separately.
        //
        // On the pinned closure this guard never fires — zero of the 539
        // members reach here, because only one carries a version at all and
        // its value is `boot`. It is kept because that is a property of one
        // closure rather than of members, and because the unit test that
        // covers it has to construct a version-bearing member for exactly
        // that reason.
        if c.extra_annotations.contains_key(
            crate::scan_fs::package_db::nix::closure::member::ANN_MEMBER_ROLE,
        ) {
            continue;
        }
        let synthesized = crate::generate::cpe::synthesize_cpes(c);
        if !synthesized.is_empty() {
            c.cpes = synthesized;
        }
    }
}

/// Milestone 1377 US3 (FR-008) — record each closure component's CPE source.
///
/// Runs after `fill_missing_cpes` and after the milestone-1366 identity pass,
/// so it sees both sources' final output and each component's final PURL:
///
/// - A nixpkgs-declared CPE (C212 present): `overrode` is recomputed against
///   the final identity. The identity pass can turn `pkg:generic` into
///   `pkg:pypi`, and the curated table answers differently for the two, so a
///   value computed at emission could name a CPE this component would never
///   have been given.
/// - Otherwise a closure component (C182 present) whose CPEs are exactly what
///   the curated table gives it, and which the operator did not supply, is
///   marked curated.
///
/// Anything else (a reader-derived CPE, an operator's `--supplement-cdx`
/// value) gets no closure provenance, so the record always describes the CPE
/// actually emitted (FR-013). Components without C182, which is every
/// component of a scan without a closure, are untouched (FR-011).
fn record_closure_cpe_sources(components: &mut [waybill_common::resolution::ResolvedComponent]) {
    use scan_fs::package_db::nix::closure::identifiers as ids;
    for c in components.iter_mut() {
        if c.cpes.is_empty() {
            continue;
        }
        if let Some(prov) = c.extra_annotations.get(ids::ANN_DECLARED_CPE) {
            let basis = ids::provenance_basis(prov).unwrap_or(ids::JoinBasis::NameVersion);
            let curated = crate::generate::cpe::synthesize_cpes(c);
            let overrode = curated.first().filter(|cur| Some(*cur) != c.cpes.first()).cloned();
            c.extra_annotations.insert(
                ids::ANN_DECLARED_CPE.to_string(),
                ids::provenance(basis, overrode.as_deref()),
            );
            continue;
        }
        let closure_member = c
            .extra_annotations
            .contains_key(crate::resolve::closure_role_scope::CLOSURE_ROLE);
        let operator = c
            .extra_annotations
            .get("waybill:source-tier")
            .and_then(|v| v.as_str())
            == Some("declared");
        if closure_member && !operator && c.cpes == crate::generate::cpe::synthesize_cpes(c) {
            c.extra_annotations
                .insert(ids::ANN_CURATED_CPE.to_string(), serde_json::Value::Bool(true));
        }
    }
}

/// Count the five C209 buckets over the final component vector (milestone
/// 1374, #1380).
///
/// # Why this is separate from `fill_missing_cpes`
///
/// It must run **later**. File-tier components join the vector at the
/// file-inventory step, some 400 lines after synthesis, and they belong in
/// `excluded_files` — so counting at synthesis time would miss a population
/// the document contains. Milestone 1373 counted inside the synthesis
/// function and so could not see them at all.
///
/// Separating also makes the shapes honest: `fill_missing_cpes` mutates and
/// returns nothing, this observes and cannot mutate. A counting function
/// taking `&[..]` cannot accidentally change what it counts.
pub(crate) fn cpe_coverage(
    components: &[waybill_common::resolution::ResolvedComponent],
) -> CpeSynthesisCoverage {
    let mut coverage = CpeSynthesisCoverage::default();
    for c in components.iter() {
        match classify_addressability(c) {
            FeedAddressability::WithCandidates => coverage.with_candidates += 1,
            FeedAddressability::AddressableWithoutCpe => coverage.addressable_without_cpe += 1,
            FeedAddressability::Unaddressable => coverage.unaddressable += 1,
            FeedAddressability::ExcludedMember => coverage.excluded_members += 1,
            FeedAddressability::ExcludedFile => coverage.excluded_files += 1,
        }
    }

    // FR-002 / SC-002 — the five partition the slice this function was
    // handed, and nothing more. An assertion rather than a comment: it is
    // the cheapest check that nothing was classified twice or not at all.
    //
    // Milestone 1376 (#1385): note what this does NOT establish. The slice
    // is the resolved component set; the *document* is what each emitter
    // makes of it, and the two differ. CycloneDX promotes a lone main
    // module into `metadata.component`, leaving `.components` one short. A
    // reader checking the row against the document needs the per-format
    // expression in `docs/reference/sbom-format-mapping.md`, not
    // `.components | length`. The integration tests in
    // `waybill-cli/tests/root_selection_integration.rs` (`m1376_*`) cover
    // all three root shapes, which is the part this assertion cannot see.
    debug_assert_eq!(
        coverage.total(),
        components.len(),
        "cpe coverage must partition the component set"
    );

    coverage
}

/// #1401 — the name a `--path` scan gives its target: the directory's
/// final component as the operator spelled it, so a symlink keeps the
/// link's name. A spelling with no final name (`.`, `..`, `sub/..`) is
/// resolved first; only the filesystem root has none, and falls back to
/// `filesystem-scan`. The resolved path is used for its last component
/// only and never leaves this function.
fn path_scan_target_name(path: &Path) -> String {
    let name = |p: &Path| p.file_name().and_then(|s| s.to_str()).map(str::to_owned);
    name(path)
        .or_else(|| path.canonicalize().ok().as_deref().and_then(name))
        .unwrap_or_else(|| "filesystem-scan".to_string())
}

/// Milestone 127 FR-007 — warn when the root-selection heuristic fell
/// through past at least one detected main-module. The warning names the
/// picked subject AND the loser main-modules' PURLs so operators can pass
/// `--root-name`/`--root-purl-type` for deterministic control. Gated on
/// `losers.is_empty() == false` (matches the FR-006 annotation emission
/// gate).
///
/// Milestone 1377 — a function so it can run where the final
/// [`ScanTargetKind`](crate::generate::root_selector::ScanTargetKind) is
/// known: a `--nix-closure` scan learns its subject only once the closure
/// resolves, and a warning computed before that would name losers of a
/// selection the evaluated-package rung then overrides.
fn warn_if_root_heuristic_outranked(
    args: &ScanArgs,
    components: &[waybill_common::resolution::ResolvedComponent],
    scan_target_coord: Option<&crate::scan_fs::package_db::maven::ScanTargetCoord>,
    target_name: &str,
    scan_target_kind: crate::generate::root_selector::ScanTargetKind,
) {
    let root_override_tmp = crate::generate::RootComponentOverride {
        name: args.root_name.clone(),
        version: args.root_version.clone(),
        purl_type: args.root_purl_type.clone(),
        omit_purl: args.no_root_purl,
        // Issue #359 — the early root-selection helper above runs
        // before the full RootComponentOverride is constructed.
        // Pass the same #359 fields so the selector treats
        // --root-purl identically to the final override.
        full_purl: args.root_purl.clone(),
        full_purl_name: None,
        full_purl_version: None,
    };
    let selection = crate::generate::root_selector::select_root(
        components,
        &root_override_tmp,
        scan_target_coord,
        target_name,
        "0.0.0",
        scan_target_kind,
    );
    if let Some(h) = selection.heuristic {
        if !selection.losers.is_empty() {
            let loser_strs: Vec<String> = selection
                .losers
                .iter()
                .map(|p| p.as_str().to_string())
                .collect();
            let selected_str = match &selection.subject {
                crate::generate::root_selector::ResolvedRootSubject::MainModule(idx) => {
                    components
                        .get(*idx)
                        .map(|c| c.purl.as_str().to_string())
                        .unwrap_or_default()
                }
                crate::generate::root_selector::ResolvedRootSubject::MavenCoord => {
                    scan_target_coord
                        .as_ref()
                        .map(|c| {
                            format!(
                                "pkg:maven/{}/{}@{}",
                                c.group, c.artifact, c.version
                            )
                        })
                        .unwrap_or_default()
                }
                crate::generate::root_selector::ResolvedRootSubject::SyntheticPlaceholder {
                    name,
                    version,
                } => format!("pkg:generic/{name}@{version}"),
                crate::generate::root_selector::ResolvedRootSubject::OperatorOverride => {
                    String::new()
                }
            };
            tracing::warn!(
                selected = %selected_str,
                losers = ?loser_strs,
                heuristic = h.name(),
                confidence = h.confidence(),
                hint = "pass --root-name and --root-purl-type to override",
                "root-component selected via heuristic; operator override recommended for deterministic identity"
            );
        }
    }
}

/// Returns `(merged, appended, appended_identities)`.
///
/// The third element exists for milestone 1366 (#1365). Its pass may only
/// touch components the closure **appended**: one that merged belongs to a
/// reader that knew the ecosystem and already carries its identifier, and
/// changing it would violate FR-006. "Appended" is not recoverable after the
/// fact — a merged component carries the closure's annotations too, because
/// the merge folds them in — so the merge is the only place that knows, and
/// it says so rather than leaving the caller to infer it.
fn merge_closure_components(
    components: &mut Vec<waybill_common::resolution::ResolvedComponent>,
    added: Vec<waybill_common::resolution::ResolvedComponent>,
) -> (usize, usize, std::collections::HashSet<(String, String)>) {
    let index: std::collections::HashMap<(String, String), usize> = components
        .iter()
        .enumerate()
        .map(|(i, c)| ((c.name.clone(), c.version.clone()), i))
        .collect();
    let (mut merged, mut appended) = (0usize, 0usize);
    let mut appended_identities: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for mut c in added {
        // #1466, milestone 1282 FR-011 — a member that declares no `pname`
        // (C206) never deduplicates against another reader's component. Its
        // `(name, version)` is its derivation's bare name and an empty
        // version, which is exactly the shape a versionless boot library
        // takes: on nixpkgs `a799d3e3` the live-bootstrap C fragment `time`
        // merged into the manifest's `pkg:hackage/time`, which then described
        // itself as a build-tooling fragment.
        let nameless = c
            .extra_annotations
            .contains_key(scan_fs::package_db::nix::closure::member::ANN_MEMBER_ROLE);
        let existing = (!nameless)
            .then(|| index.get(&(c.name.clone(), c.version.clone())))
            .flatten();
        match existing {
            Some(&i) => {
                // Milestone 1281 (#1346), FR-004 — reconcile declared
                // licenses without replacing an existing declaration.
                //
                // Fill only where the existing component has none. A reader
                // that knew the ecosystem and found a license is better
                // evidence than nixpkgs' packaging metadata, so it wins; but
                // where it found nothing, dropping the closure's declaration
                // would silently discard the only license available. Before
                // this, every merged member lost its license outright,
                // because the merge carried annotations and nothing else.
                if components[i].licenses.is_empty() && !c.licenses.is_empty() {
                    components[i].licenses = c.licenses;
                }
                // Milestone 1377 FR-013 — a nixpkgs-declared CPE replaces the
                // existing component's only when that is empty or exactly
                // what the curated table gives it. Anything else is better
                // evidence (a reader found it in the artifact) or the
                // operator's word (`--supplement-cdx`), and is kept; the
                // closure's provenance is then dropped, so it never
                // describes a CPE that was not emitted.
                if let Some(prov) = c
                    .extra_annotations
                    .remove(scan_fs::package_db::nix::closure::identifiers::ANN_DECLARED_CPE)
                {
                    let existing = &components[i];
                    let operator_asserted = existing
                        .extra_annotations
                        .get("waybill:source-tier")
                        .and_then(|v| v.as_str())
                        == Some("declared");
                    let replaceable = existing.cpes.is_empty()
                        || (!operator_asserted
                            && existing.cpes == crate::generate::cpe::synthesize_cpes(existing));
                    if replaceable {
                        // `overrode` describes THIS component: what the
                        // curated table gave it, if that differs.
                        let declared = c.cpes.first().cloned();
                        let overrode = existing
                            .cpes
                            .first()
                            .filter(|old| Some(*old) != declared.as_ref())
                            .cloned();
                        let basis =
                            scan_fs::package_db::nix::closure::identifiers::provenance_basis(&prov)
                                .unwrap_or(
                                    scan_fs::package_db::nix::closure::identifiers::JoinBasis::NameVersion,
                                );
                        components[i].cpes = std::mem::take(&mut c.cpes);
                        components[i].extra_annotations.insert(
                            scan_fs::package_db::nix::closure::identifiers::ANN_DECLARED_CPE
                                .to_string(),
                            scan_fs::package_db::nix::closure::identifiers::provenance(
                                basis,
                                overrode.as_deref(),
                            ),
                        );
                    }
                }
                for (k, v) in c.extra_annotations {
                    // The existing component's own annotations win: it was
                    // produced by a reader that knew the ecosystem.
                    components[i].extra_annotations.entry(k).or_insert(v);
                }
                merged += 1;
            }
            None => {
                appended_identities.insert((c.name.clone(), c.version.clone()));
                components.push(c);
                appended += 1;
            }
        }
    }
    (merged, appended, appended_identities)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod advisory_tests {
    use super::AdvisoryContext;

    fn ctx(
        fallback: Option<usize>,
        default: bool,
        offline: bool,
        has_go: bool,
    ) -> AdvisoryContext {
        AdvisoryContext {
            fallback_count: fallback,
            warm_flag_was_default: default,
            offline,
            scan_has_go_components: has_go,
        }
    }

    #[test]
    fn advises_when_all_four_predicates_hold() {
        assert!(ctx(Some(3), true, false, true).should_advise());
    }

    #[test]
    fn suppressed_when_offline() {
        assert!(!ctx(Some(3), true, true, true).should_advise());
    }

    #[test]
    fn suppressed_when_flag_explicit() {
        assert!(!ctx(Some(3), false, false, true).should_advise());
    }

    #[test]
    fn suppressed_on_zero_fallback() {
        assert!(!ctx(Some(0), true, false, true).should_advise());
    }

    #[test]
    fn suppressed_when_fallback_none() {
        assert!(!ctx(None, true, false, true).should_advise());
    }

    #[test]
    fn suppressed_on_non_go_scan() {
        assert!(!ctx(Some(3), true, false, false).should_advise());
    }
}

/// Write `bytes` to `path`, creating any missing parent directories.
///
/// Shared by every serializer artifact (CDX today; SPDX + OpenVEX in
/// Resolve the `created` timestamp for the SBOM output config.
///
/// Defaults to `chrono::Utc::now()`. **Test-only override**: when the
/// `WAYBILL_FIXED_TIMESTAMP` env var is set to an RFC 3339 string,
/// that value is used instead — required for tests that compare raw
/// SBOM bytes across two `waybill sbom scan` subprocesses (e.g.
/// `format_dispatch::spdx_3_alias_bytes_are_byte_identical_to_stable_identifier`).
/// Without the override, the two subprocesses' independent
/// `Utc::now()` calls can cross a second boundary on slow runners
/// and produce non-byte-identical output, surfacing as a CI flake
/// even on docs-only PRs.
///
/// Production scans MUST NOT set this env var. An unparseable value
/// is treated as "unset" — silently fall back to `Utc::now()` rather
/// than panic, since this is a defensive belt-and-braces helper, not
/// a hard contract.
fn scan_created_timestamp() -> chrono::DateTime<chrono::Utc> {
    if let Ok(s) = std::env::var("WAYBILL_FIXED_TIMESTAMP") {
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&s) {
            return parsed.with_timezone(&chrono::Utc);
        }
    }
    chrono::Utc::now()
}

/// Milestone 072 / T027 helper: walk the resolved component set and
/// attach a `waybill:source-document-binding` annotation to each
/// component whose PURL appears in the source-tier SBOM.
///
/// Components matching by PURL get the source-tier's binding metadata
/// (provenance-preserved). Components whose PURL has no source-tier
/// counterpart get an explicit
/// `binding: unknown { reason: "source-not-found-in-bind-target" }`
/// per FR-003.
///
/// The annotation rides through `extra_annotations` (the milestone-023
/// generic per-component bag). Existing CDX `properties[]`,
/// SPDX 2.3 `Package.annotations[]` envelope, and SPDX 3
/// `Annotation.statement` envelope serializers all consume that bag
/// transparently — no per-format emission code change needed for
/// per-component binding annotations.
/// Milestone 111: assemble the operator's `--pkg-alias` declarations
/// and `WAYBILL_PKG_ALIAS` env-var entries into a single deterministic
/// `AliasMap`. Conflicts (same LHS, different RHS) abort the scan with
/// an actionable error per FR-008.
///
/// Env-var format: comma-separated `LHS=RHS` entries; whitespace
/// trimmed; empty entries silently skipped.
fn build_pkg_alias_map(
    args: &ScanArgs,
) -> anyhow::Result<waybill::binding::alias::AliasMap> {
    use waybill::binding::alias::{parse_pkg_alias, AliasMap};

    let mut map = AliasMap::new();

    // CLI flags first (insertion order = flag order).
    for alias in &args.pkg_alias {
        map.insert(alias.clone())
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    }

    // Then env-var entries.
    if let Ok(raw) = std::env::var("WAYBILL_PKG_ALIAS") {
        for entry in raw.split(',') {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                continue;
            }
            let alias = parse_pkg_alias(trimmed)
                .map_err(|e| anyhow::anyhow!("WAYBILL_PKG_ALIAS entry: {}", e))?;
            map.insert(alias)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
        }
    }

    Ok(map)
}

/// FR-011: emit an info-level log for any alias whose LHS PURL did
/// not match a component during the binding pass. Operator typos in
/// the LHS are common; surfacing them as info-level diagnostics gives
/// the operator a clear signal without escalating to warn.
fn log_unused_pkg_aliases(
    alias_map: &waybill::binding::alias::AliasMap,
    consumed: &std::collections::HashSet<String>,
) {
    for alias in alias_map.iter() {
        let lhs = alias.lhs().as_str();
        if !consumed.contains(lhs) {
            tracing::info!(
                lhs = %lhs,
                rhs = %alias.rhs().as_str(),
                "--pkg-alias LHS did not match any scan-output component; \
                 no alias applied for this entry. (Verify the LHS PURL \
                 matches what the scan emits for the intended component.)"
            );
        }
    }
}

fn attach_bindings_to_components(
    components: &mut [waybill_common::resolution::ResolvedComponent],
    ctx: &waybill::binding::SourceSbomContext,
    alias_map: &waybill::binding::alias::AliasMap,
) -> std::collections::HashSet<String> {
    use waybill::binding::BindingStrength;
    use waybill_common::types::purl::Purl;

    // Track which alias LHSes were consumed during this pass so the
    // FR-011 unused-alias info log can surface declared-but-unmatched
    // entries to the operator.
    let mut consumed_aliases: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    for c in components.iter_mut() {
        let component_purl_str = c.purl.as_str().to_string();

        // Determine whether an alias applies to this component. Match
        // is strict canonical-PURL equality per spec FR-001 + research
        // §3 (Q1 clarification).
        let alias_pair: Option<(Purl, Purl)> = Purl::new(&component_purl_str)
            .ok()
            .and_then(|component_purl| {
                alias_map
                    .get(&component_purl)
                    .map(|rhs| (component_purl, rhs.clone()))
            });

        // Choose which PURL to look up in the source SBOM: when an
        // alias applies, we look up the RHS; otherwise the component's
        // own PURL.
        let lookup_purl_str = match &alias_pair {
            Some((_, rhs)) => rhs.as_str().to_string(),
            None => component_purl_str.clone(),
        };

        let mut binding = ctx.binding_for_purl(&lookup_purl_str);

        // FR-005 + FR-013: stamp alias_from / alias_to onto the
        // envelope regardless of strength outcome so the operator
        // can see the alias was applied even when the RHS is
        // missing from the source SBOM.
        if let Some((lhs, rhs)) = alias_pair {
            // FR-007: when an alias was applied AND the RHS was not
            // found in the bind-source, rewrite the failure reason
            // from `source-not-found-in-bind-target` to the alias-
            // specific reason so operators can debug their alias
            // declaration separately from a missing-source-document
            // state.
            if binding.strength == BindingStrength::Unknown
                && binding.reason.as_deref() == Some("source-not-found-in-bind-target")
            {
                binding.reason = Some("alias-target-not-found-in-bind-target".to_string());
            }
            consumed_aliases.insert(lhs.as_str().to_string());
            binding.alias_from = Some(lhs);
            binding.alias_to = Some(rhs);
            // Milestone 116 — operator-supplied alias overrides any
            // automatic alias that binding_for_purl may have stamped
            // (this preserves the FR-004 operator-precedence rule even
            // in the edge case where the operator's RHS itself triggers
            // the pkg:generic/<name> auto-alias path).
            binding.alias_source = Some(waybill::binding::AliasSource::OperatorSupplied);
        }

        // Serialize via the canonical serde shape so emission is
        // byte-stable across reruns. The CDX side will JSON-encode
        // this Value to a string at emission time (the milestone-023
        // bag does that automatically); the SPDX side wraps it in
        // the MikebomAnnotationCommentV1 envelope.
        if let Ok(value) = waybill::binding::serialize_to_envelope_value(&binding) {
            c.extra_annotations.insert(
                waybill::binding::BINDING_PROPERTY_NAME.to_string(),
                value,
            );
        }
    }
    consumed_aliases
}

/// later phases). Kept local to this CLI module so the generator crate
/// has no filesystem dependencies.
/// Milestone 221 US2a (FR-008a support): true when an `--output`
/// argument value targets stdout — either bare `-` or `<fmt>=-`.
fn is_stdout_output(value: &str) -> bool {
    let raw = value.trim();
    if raw == "-" {
        return true;
    }
    // Per-format form `<fmt>=<path>`; look at the path portion.
    if let Some((_fmt, path)) = raw.split_once('=') {
        return path.trim() == "-";
    }
    false
}

/// Milestone 221 US2a: produce a suggested replacement path when the
/// operator combined `--sign-key` with stdout output. Preserves the
/// `<fmt>=` prefix when present so the diagnostic reads naturally.
fn suggest_non_stdout_path(offender: &str) -> String {
    if let Some((fmt, _)) = offender.split_once('=') {
        format!("{fmt}=signed.{fmt}.json")
    } else {
        "signed.cdx.json".to_string()
    }
}

/// Milestone 221 US2a (FR-009a) — best-effort unlink of every file
/// waybill wrote during this scan. Called on signing failure so
/// consumers never see a partial `--output <path>` file. Errors on
/// individual unlinks are logged at WARN but do not mask the
/// primary signing error.
fn cleanup_written_files(files: &[PathBuf]) {
    for path in files {
        if let Err(e) = std::fs::remove_file(path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "could not clean up partial output on signing failure"
                );
            }
        }
    }
}

fn write_bytes_to(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use anyhow::Context;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory: {}", parent.display()))?;
        }
    }
    std::fs::write(path, bytes)
        .with_context(|| format!("writing SBOM to {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::sbom::signer::inject_signature_reference;

    fn reg() -> SerializerRegistry {
        SerializerRegistry::with_defaults()
    }

    // #1401 — end-to-end spellings are in `tests/path_target_name.rs`;
    // the filesystem root is here because scanning it is a whole-disk scan.
    #[test]
    fn path_scan_target_name_falls_back_only_at_the_filesystem_root() {
        let cwd = std::env::current_dir().unwrap();
        let root = cwd.ancestors().last().unwrap();
        assert_eq!(path_scan_target_name(root), "filesystem-scan");
        assert_eq!(path_scan_target_name(&root.join("..")), "filesystem-scan");

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("waybill-fixture-name");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        assert_eq!(path_scan_target_name(&dir.join("sub").join("..")), "waybill-fixture-name");
        assert_eq!(path_scan_target_name(&dir.join(".")), "waybill-fixture-name");
    }

    // ----- Milestone 778 US2 — signature-location reference -----

    #[test]
    fn m778_reference_uses_a_bare_filename_with_no_directory_component() {
        // FR-012a / SC-009: the reference must resolve wherever the
        // document and its artifact are stored together, which an
        // absolute path or any directory component would break.
        let doc = br#"{"bomFormat":"CycloneDX","specVersion":"1.6","components":[]}"#;
        let out = inject_signature_reference(doc, "signed.cdx.json.sig.bundle.json").unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let url = parsed["externalReferences"][0]["url"].as_str().unwrap();

        assert_eq!(url, "signed.cdx.json.sig.bundle.json");
        assert!(
            !url.contains('/') && !url.contains('\\'),
            "reference MUST carry no directory component (FR-012a); got {url}"
        );
        assert_eq!(parsed["externalReferences"][0]["type"], "attestation");
    }

    #[test]
    fn m778_reference_carries_no_hash_of_the_artifact() {
        // The artifact signs the document; a hash of the artifact inside
        // the document would make each depend on the other. Excluded by
        // construction, not preference — pinned so nobody "improves" it.
        let doc = br#"{"bomFormat":"CycloneDX","specVersion":"1.6"}"#;
        let out = inject_signature_reference(doc, "x.sig.bundle.json").unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert!(
            parsed["externalReferences"][0].get("hashes").is_none(),
            "a checksum here is circular and MUST NOT be added"
        );
    }

    #[test]
    fn m778_reference_preserves_existing_document_content() {
        let doc = br#"{"bomFormat":"CycloneDX","specVersion":"1.6","components":[{"name":"a"}]}"#;
        let out = inject_signature_reference(doc, "x.sig.bundle.json").unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed["bomFormat"], "CycloneDX");
        assert_eq!(parsed["components"][0]["name"], "a");
        assert_eq!(parsed["externalReferences"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn m778_reference_appends_to_existing_external_references() {
        let doc = br#"{"bomFormat":"CycloneDX","specVersion":"1.6",
                       "externalReferences":[{"type":"website","url":"https://example.test"}]}"#;
        let out = inject_signature_reference(doc, "x.sig.bundle.json").unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let refs = parsed["externalReferences"].as_array().unwrap();
        assert_eq!(refs.len(), 2, "must append, not replace");
        assert_eq!(refs[0]["type"], "website");
        assert_eq!(refs[1]["type"], "attestation");
    }

    // ----- Milestone 221 US2a (FR-008a) — stdout+signing rejection helpers -----

    #[test]
    fn is_stdout_output_detects_bare_dash_m221() {
        assert!(is_stdout_output("-"));
        assert!(is_stdout_output(" - "));
    }

    #[test]
    fn is_stdout_output_detects_fmt_dash_m221() {
        assert!(is_stdout_output("cyclonedx-json=-"));
        assert!(is_stdout_output("spdx-2.3-json=-"));
        assert!(is_stdout_output("spdx-3-json= -"));
    }

    #[test]
    fn is_stdout_output_ignores_normal_paths_m221() {
        assert!(!is_stdout_output("/tmp/scan.cdx.json"));
        assert!(!is_stdout_output("cyclonedx-json=/tmp/scan.cdx.json"));
        assert!(!is_stdout_output("./scan.spdx.json"));
        assert!(!is_stdout_output("waybill.cdx.json"));
        // Filename that happens to contain '-' but isn't stdout.
        assert!(!is_stdout_output("signed-scan.cdx.json"));
        assert!(!is_stdout_output("cyclonedx-json=signed-scan.cdx.json"));
    }

    #[test]
    fn suggest_non_stdout_path_preserves_fmt_prefix_m221() {
        assert_eq!(suggest_non_stdout_path("-"), "signed.cdx.json");
        assert_eq!(
            suggest_non_stdout_path("cyclonedx-json=-"),
            "cyclonedx-json=signed.cyclonedx-json.json"
        );
    }

    /// T027 / SC-006 / C-7.1 — `--preserve-manifest-main-module` is
    /// still accepted after milestone 860 made its behaviour
    /// unconditional. It is a no-op, not an error: removing it would
    /// break existing callers for no benefit.
    ///
    /// Output equivalence (flag on vs off producing identical bytes) is
    /// verified at the emitter level; what can break independently here
    /// is the CLI surface, so that is what this pins.
    #[test]
    fn preserve_manifest_main_module_is_still_accepted_m860() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan", "--path", ".", "--preserve-manifest-main-module",
        ])
        .expect("C-7.1: the flag MUST still parse, even though it no longer changes output");
        assert!(parsed.inner.preserve_manifest_main_module);

        let default = <ScanArgsForTest as clap::Parser>::try_parse_from(["scan", "--path", "."])
            .expect("baseline parse");
        assert!(!default.inner.preserve_manifest_main_module, "still defaults off");
    }

    /// T054 / FR-021 — the declaration pass is declinable.
    ///
    /// Pins the CLI surface only. The gate itself is one `if !` at the call
    /// site, and what can break independently is the flag's existence,
    /// spelling and default: a rename would silently stop honouring every
    /// existing `--no-nixpkgs-security` on the command line, because clap
    /// rejects unknown flags loudly but a renamed one is a different flag.
    ///
    /// Defaulting off is the load-bearing half. The pass costs 706 ms
    /// against a 388 ms closure query (measurements Q5), and the decision
    /// to pay that by default is what this asserts has not been quietly
    /// inverted.
    #[test]
    fn no_nixpkgs_security_is_accepted_and_defaults_off_m1050() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--no-nixpkgs-security",
        ])
        .expect("FR-021: the opt-out MUST parse");
        assert!(parsed.inner.no_nixpkgs_security);

        let default = <ScanArgsForTest as clap::Parser>::try_parse_from(["scan", "--path", "."])
            .expect("baseline parse");
        assert!(
            !default.inner.no_nixpkgs_security,
            "FR-021: absent the flag the pass MUST run, so no existing invocation changes meaning"
        );
    }

    // ----- Issue #927 (m923) — enrichment default -----

    /// Build the source the way `run()` does, from a parsed command line.
    ///
    /// Going through `build_deps_dev_source` rather than re-deriving the
    /// decision here is the point: a test that recomputes `!no_enrich_batch`
    /// itself agrees with the production wiring only by coincidence, and
    /// keeps agreeing after the wiring changes.
    fn source_for(argv: &[&str]) -> crate::enrich::depsdev_source::DepsDevSource {
        let parsed =
            <ScanArgsForTest as clap::Parser>::try_parse_from(argv).expect("args must parse");
        build_deps_dev_source(
            DepsDevClient::new(std::time::Duration::from_secs(5)),
            false,
            &parsed.inner,
        )
    }

    /// FR-001 / SC-001. **Asserted as path SELECTION, not as output.**
    ///
    /// Both enrichment paths produce equivalent documents (asserted by
    /// `both_paths_produce_the_same_content_when_batch_succeeds`), so no
    /// comparison of emitted bytes can tell you which one ran.
    ///
    /// This asserts on the constructed source, not on the parsed flag. The
    /// earlier version checked `!args.no_enrich_batch` and was found during
    /// the T022 teeth-check to pass against the *pre-change* default — it was
    /// testing clap, not the flip, and a mutation of the wiring to
    /// `.with_batch(false)` left it green while every scan took the slow path.
    #[test]
    fn enrichment_is_batched_by_default_m923() {
        assert!(
            source_for(&["scan", "--path", "."]).is_batched(),
            "a scan with no enrichment flags must select the batched path",
        );
    }

    /// FR-003 / SC-006 / C-3. The escape hatch selects the per-component
    /// path. It is the fallback the default's safety argument rests on, so
    /// it has to be reachable.
    #[test]
    fn no_enrich_batch_selects_the_per_component_path_m923() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan", "--path", ".", "--no-enrich-batch",
        ])
        .expect("the opt-out MUST parse");
        assert!(parsed.inner.no_enrich_batch);
        assert!(
            !source_for(&["scan", "--path", ".", "--no-enrich-batch"]).is_batched(),
            "--no-enrich-batch must actually reach the source, not just parse",
        );
    }

    /// FR-004 / SC-007 / C-4. The legacy opt-in keeps parsing. Scripts that
    /// pass it today must keep working; erroring on an unknown argument
    /// would break them louder than ignoring it.
    #[test]
    fn legacy_enrich_batch_flag_is_still_accepted_m923() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan", "--path", ".", "--enrich-batch",
        ])
        .expect("C-4: the legacy flag MUST still parse, even though it is now a no-op");
        assert!(parsed.inner.enrich_batch, "parsed, and ignored downstream");
        assert!(
            !parsed.inner.no_enrich_batch,
            "passing the legacy opt-in must not accidentally select the opt-out"
        );
        assert!(
            source_for(&["scan", "--path", ".", "--enrich-batch"]).is_batched(),
            "the legacy opt-in must still land on the batched path — as a no-op \
             it selects what the default already selects",
        );
    }

    // ----- Milestone 222 US2b (feature 222-sigstore-keyless-signing) — CLI parsing -----

    #[test]
    fn sign_flag_defaults_off_m222() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from(["scan", "--path", "."])
            .expect("baseline parse");
        assert!(!parsed.inner.sign, "sign defaults off");
        assert_eq!(parsed.inner.fulcio_url, "https://fulcio.sigstore.dev");
        assert_eq!(parsed.inner.rekor_url, "https://rekor.sigstore.dev");
        assert_eq!(parsed.inner.rekor_timeout_secs, 30);
    }

    #[test]
    fn sign_flag_accepts_bare_toggle_m222() {
        let parsed =
            <ScanArgsForTest as clap::Parser>::try_parse_from(["scan", "--path", ".", "--sign"])
                .expect("bare --sign parses");
        assert!(parsed.inner.sign);
    }

    #[test]
    fn sign_and_sign_key_are_mutually_exclusive_m222() {
        let err = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--sign",
            "--sign-key",
            "/tmp/x.pem",
        ])
        .expect_err("--sign + --sign-key must be rejected by clap");
        let rendered = err.to_string();
        assert!(
            rendered.contains("cannot be used with") || rendered.contains("conflicts_with")
                || rendered.contains("--sign-key"),
            "clap conflict-message expected, got: {rendered}"
        );
    }

    #[test]
    fn sign_endpoint_overrides_via_flags_m222() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--sign",
            "--fulcio-url",
            "https://fulcio.sigstage.dev",
            "--rekor-url",
            "https://rekor.sigstage.dev",
            "--rekor-timeout-secs",
            "60",
        ])
        .expect("staging-endpoint override parses");
        assert!(parsed.inner.sign);
        assert_eq!(parsed.inner.fulcio_url, "https://fulcio.sigstage.dev");
        assert_eq!(parsed.inner.rekor_url, "https://rekor.sigstage.dev");
        assert_eq!(parsed.inner.rekor_timeout_secs, 60);
    }

    #[test]
    fn default_format_is_cyclonedx_when_no_flag_given() {
        let plan = resolve_dispatch(&reg(), &[], &[]).unwrap();
        assert_eq!(plan.formats, vec!["cyclonedx-json".to_string()]);
        assert!(plan.overrides.is_empty());
    }

    #[test]
    fn duplicate_format_ids_dedupe_silently() {
        let plan = resolve_dispatch(
            &reg(),
            &["cyclonedx-json".into(), "cyclonedx-json".into()],
            &[],
        )
        .unwrap();
        assert_eq!(plan.formats, vec!["cyclonedx-json".to_string()]);
    }

    #[test]
    fn unknown_format_rejects_with_known_list() {
        let err = resolve_dispatch(&reg(), &["totally-fake-format".into()], &[])
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("unknown format identifier") && err.contains("cyclonedx-json"),
            "error should enumerate registered ids, got: {err}"
        );
    }

    #[test]
    fn bare_output_applies_to_single_requested_format() {
        let plan =
            resolve_dispatch(&reg(), &[], &["out.cdx.json".into()]).unwrap();
        assert_eq!(
            plan.overrides.get("cyclonedx-json"),
            Some(&PathBuf::from("out.cdx.json"))
        );
    }

    #[test]
    fn fmt_equals_path_parses_as_per_format_override() {
        let plan = resolve_dispatch(
            &reg(),
            &["cyclonedx-json".into()],
            &["cyclonedx-json=custom.cdx.json".into()],
        )
        .unwrap();
        assert_eq!(
            plan.overrides.get("cyclonedx-json"),
            Some(&PathBuf::from("custom.cdx.json"))
        );
    }

    /// #1122: the reference an SPDX document carries, resolved against the
    /// document's directory as a relative reference is, names the file the
    /// sidecar is written to. Before the fix the sidecar landed in the working
    /// directory, so an SPDX file written elsewhere referenced nothing.
    #[test]
    fn the_sidecar_reference_resolves_to_where_the_sidecar_is_written() {
        use std::collections::BTreeMap;
        let norm = |p: PathBuf| -> PathBuf {
            let mut out = PathBuf::new();
            for c in std::path::absolute(p).unwrap().components() {
                match c {
                    std::path::Component::ParentDir => {
                        out.pop();
                    }
                    std::path::Component::CurDir => {}
                    other => out.push(other.as_os_str()),
                }
            }
            out
        };
        // (SPDX --output, openvex --output)
        let cases: [(Option<&str>, Option<&str>); 6] = [
            (None, None),
            (Some("out/sbom.spdx.json"), None),
            (Some("out/sbom.spdx.json"), Some("out/vex.json")),
            (Some("out/sbom.spdx.json"), Some("vex/vex.json")),
            (Some("a/b/sbom.spdx.json"), Some("vex.json")),
            (None, Some("elsewhere/vex.json")),
        ];
        for (spdx, vex) in cases {
            let mut overrides: BTreeMap<String, PathBuf> = BTreeMap::new();
            if let Some(s) = spdx {
                overrides.insert("spdx-2.3-json".into(), PathBuf::from(s));
            }
            if let Some(v) = vex {
                overrides.insert("openvex".into(), PathBuf::from(v));
            }
            let doc = PathBuf::from(spdx.unwrap_or("waybill.spdx.json"));
            let doc_dir = doc.parent().map(Path::to_path_buf).unwrap_or_default();
            let reference = crate::generate::openvex::sidecar_reference(
                &overrides,
                &["spdx-2.3-json"],
                "waybill.spdx.json",
            );
            assert!(!Path::new(&reference).is_absolute(), "{spdx:?} {vex:?}: {reference}");
            let written = openvex_sidecar_target(overrides.get("openvex"), Some(&doc_dir));
            assert_eq!(
                norm(doc_dir.join(&reference)),
                norm(written.clone()),
                "{spdx:?} {vex:?}: reference {reference} vs written {}",
                written.display(),
            );
        }
    }

    #[test]
    fn openvex_cannot_be_requested_via_format_flag() {
        let err = resolve_dispatch(&reg(), &["openvex".into()], &[])
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("not a selectable --format")
                && err.contains("sidecar alongside SPDX"),
            "got: {err}"
        );
    }

    #[test]
    fn openvex_override_without_spdx_format_rejects() {
        let err = resolve_dispatch(
            &reg(),
            &["cyclonedx-json".into()],
            &["openvex=/tmp/vex.json".into()],
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("`--output openvex=<path>` is only valid when an SPDX format"),
            "got: {err}"
        );
    }

    #[test]
    fn openvex_override_with_spdx_format_is_accepted() {
        let plan = resolve_dispatch(
            &reg(),
            &["cyclonedx-json".into(), "spdx-2.3-json".into()],
            &[
                "cyclonedx-json=out.cdx.json".into(),
                "spdx-2.3-json=out.spdx.json".into(),
                "openvex=out.vex.json".into(),
            ],
        )
        .unwrap();
        assert_eq!(
            plan.overrides.get("openvex"),
            Some(&PathBuf::from("out.vex.json"))
        );
        // openvex is NOT in the formats list — it's a sidecar key
        // only, never dispatched as a serializer.
        assert!(!plan.formats.iter().any(|f| f == "openvex"));
    }

    /// #1443 — without `--output openvex=`, the sidecar is named after the
    /// SPDX document it accompanies, so scans written into one directory
    /// under different names keep their own sidecars. One sidecar per run:
    /// the SPDX 2.3 document names it when both SPDX formats are requested.
    #[test]
    fn the_sidecar_path_derives_from_the_spdx_output() {
        let all3: &[&str] = &["cyclonedx-json", "spdx-2.3-json", "spdx-3-json"];
        let cases: &[(&[&str], &[&str], &str)] = &[
            // Default paths keep the historical name.
            (&["spdx-2.3-json"], &[], "waybill.openvex.json"),
            (&["spdx-3-json"], &[], "waybill.openvex.json"),
            (all3, &[], "waybill.openvex.json"),
            // A bare --output for one SPDX format.
            (&["spdx-2.3-json"], &["out/a.spdx.json"], "out/a.openvex.json"),
            (&["spdx-3-json"], &["out/a.spdx3.json"], "out/a.openvex.json"),
            (&["spdx-2.3-json"], &["scan1.json"], "scan1.openvex.json"),
            (&["spdx-2.3-json"], &["out/a.spdx23.json"], "out/a.spdx23.openvex.json"),
            // Several formats: SPDX 2.3 anchors, whatever the --format order.
            (
                &["spdx-3-json", "cyclonedx-json", "spdx-2.3-json"],
                &["cyclonedx-json=o/x.cdx.json", "spdx-2.3-json=o/x.spdx.json", "spdx-3-json=p/y.spdx3.json"],
                "o/x.openvex.json",
            ),
            // SPDX 2.3 at its default while SPDX 3 is placed elsewhere.
            (all3, &["spdx-3-json=p/y.spdx3.json"], "waybill.openvex.json"),
            (&["cyclonedx-json", "spdx-3-json"], &["spdx-3-json=p/y.spdx3.json"], "p/y.openvex.json"),
            // An explicit openvex path wins.
            (&["spdx-2.3-json"], &["spdx-2.3-json=o/x.spdx.json", "openvex=v.json"], "v.json"),
            (&["spdx-3-json"], &["openvex=v.json"], "v.json"),
        ];
        for (formats, outputs, expected) in cases {
            let formats: Vec<String> = formats.iter().map(|s| s.to_string()).collect();
            let outputs: Vec<String> = outputs.iter().map(|s| s.to_string()).collect();
            let plan = resolve_dispatch(&reg(), &formats, &outputs)
                .unwrap_or_else(|e| panic!("{formats:?} {outputs:?}: {e}"));
            assert_eq!(
                plan.overrides.get(OPENVEX_PSEUDO_FORMAT),
                Some(&PathBuf::from(expected)),
                "{formats:?} {outputs:?}",
            );
            assert!(!plan.formats.iter().any(|f| f == OPENVEX_PSEUDO_FORMAT));
        }
    }

    #[test]
    fn a_run_without_spdx_has_no_sidecar_path() {
        let plan = resolve_dispatch(&reg(), &["cyclonedx-json".into()], &["x.cdx.json".into()]).unwrap();
        assert!(!plan.overrides.contains_key(OPENVEX_PSEUDO_FORMAT), "{:?}", plan.overrides);
    }

    /// The derived path is checked for collisions like an explicit one: the
    /// sidecar would otherwise overwrite another format's document.
    #[test]
    fn a_derived_sidecar_path_colliding_with_a_format_output_rejects() {
        let err = resolve_dispatch(
            &reg(),
            &["cyclonedx-json".into(), "spdx-2.3-json".into()],
            &["cyclonedx-json=o/x.openvex.json".into(), "spdx-2.3-json=o/x.spdx.json".into()],
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("collision"), "got: {err}");
    }

    #[test]
    fn openvex_override_collides_with_cdx_path() {
        let err = resolve_dispatch(
            &reg(),
            &["cyclonedx-json".into(), "spdx-2.3-json".into()],
            &[
                "spdx-2.3-json=out.spdx.json".into(),
                "openvex=waybill.cdx.json".into(),
            ],
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("output path collision")
                && err.contains("openvex"),
            "got: {err}"
        );
    }

    #[test]
    fn override_for_unrequested_format_rejects() {
        let err = resolve_dispatch(
            &reg(),
            &["cyclonedx-json".into()],
            &["spdx-2.3-json=s.json".into()],
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("but --format did not request it"),
            "got: {err}"
        );
    }

    #[test]
    fn duplicate_override_for_same_format_rejects() {
        let err = resolve_dispatch(
            &reg(),
            &["cyclonedx-json".into()],
            &[
                "cyclonedx-json=a.json".into(),
                "cyclonedx-json=b.json".into(),
            ],
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("specified more than once"), "got: {err}");
    }

    #[test]
    fn bare_output_rejected_when_multiple_formats_requested() {
        // Register a second (fake) format by using the existing
        // `cyclonedx-json` twice won't test this — multiple distinct
        // registered ids only appear once SPDX lands. We simulate the
        // condition by checking that bare `--output` with two format
        // args (even before dedup) resolves to one-format and succeeds,
        // then confirm the negative path by forcing the check via the
        // error message branch below.
        //
        // Cross-check: build args that survive dedup as a single
        // format — bare path works. Using two *identical* ids dedupes,
        // so this is actually the default path. The multi-format
        // negative case is covered by `format_dispatch.rs` integration
        // test once SPDX lands; this unit test guards the happy dedup
        // case.
        let plan = resolve_dispatch(
            &reg(),
            &["cyclonedx-json".into(), "cyclonedx-json".into()],
            &["out.cdx.json".into()],
        )
        .unwrap();
        assert_eq!(plan.formats.len(), 1);
        assert_eq!(
            plan.overrides.get("cyclonedx-json"),
            Some(&PathBuf::from("out.cdx.json"))
        );
    }

    #[test]
    fn empty_format_value_rejects() {
        let err = resolve_dispatch(&reg(), &["".into()], &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("must not be empty"), "got: {err}");
    }

    #[test]
    fn bare_and_per_format_override_for_same_format_rejects() {
        let err = resolve_dispatch(
            &reg(),
            &["cyclonedx-json".into()],
            &[
                "cyclonedx-json=a.json".into(),
                "b.json".into(),
            ],
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("conflicts with --output"), "got: {err}");
    }

    /// Wrapper Parser for clap-parsing tests — `ScanArgs` derives
    /// `Args`, not `Parser`, so we flatten it into a top-level Parser.
    #[derive(clap::Parser, Debug)]
    struct ScanArgsForTest {
        #[command(flatten)]
        inner: ScanArgs,
    }

    #[test]
    fn image_src_defaults_to_docker_podman_remote_m206() {
        // Milestone 206 (#440): default order bumped from
        // `[Docker, Remote]` to `[Docker, Podman, Remote]` per FR-006.
        // Docker-first preserves backward compat; Podman inserted
        // before Remote so local podman wins over network fetches.
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan", "--path", ".",
        ])
        .unwrap();
        assert_eq!(
            parsed.inner.image_src,
            vec![ImageSource::Docker, ImageSource::Podman, ImageSource::Remote]
        );
    }

    #[test]
    fn image_src_accepts_comma_separated_list() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--image-src",
            "remote,docker",
        ])
        .unwrap();
        assert_eq!(
            parsed.inner.image_src,
            vec![ImageSource::Remote, ImageSource::Docker]
        );
    }

    #[test]
    fn image_src_accepts_single_value() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--image-src",
            "remote",
        ])
        .unwrap();
        assert_eq!(parsed.inner.image_src, vec![ImageSource::Remote]);
    }

    #[test]
    fn image_src_accepts_podman_value_m206() {
        // Milestone 206 (#440): `podman` is now a valid --image-src value.
        // Pre-m206 this was rejected as unknown.
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--image-src",
            "podman",
        ])
        .unwrap();
        assert_eq!(parsed.inner.image_src, vec![ImageSource::Podman]);
    }

    #[test]
    fn image_src_rejects_unknown_value() {
        let err = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--image-src",
            "definitely-not-a-source",
        ])
        .unwrap_err()
        .to_string();
        assert!(
            err.to_lowercase().contains("invalid value")
                || err.to_lowercase().contains("possible values"),
            "expected clap to reject unknown image-src value, got: {err}"
        );
    }

    // ── Milestone 182 — TLS/transport flag parse tests ────────────

    #[test]
    fn insecure_registry_flag_repeatable_parses() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--insecure-registry",
            "core:8080",
            "--insecure-registry",
            "dev-registry",
        ])
        .unwrap();
        assert_eq!(
            parsed.inner.insecure_registry,
            vec!["core:8080".to_string(), "dev-registry".to_string()]
        );
    }

    #[test]
    fn registry_ca_cert_flag_repeatable_parses() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--registry-ca-cert",
            "/etc/ssl/ca1.pem",
            "--registry-ca-cert",
            "/etc/ssl/ca2.pem",
        ])
        .unwrap();
        assert_eq!(
            parsed.inner.registry_ca_cert,
            vec![
                std::path::PathBuf::from("/etc/ssl/ca1.pem"),
                std::path::PathBuf::from("/etc/ssl/ca2.pem"),
            ]
        );
    }

    #[test]
    fn insecure_tls_skip_verify_bool_defaults_false() {
        let default = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan", "--path", ".",
        ])
        .unwrap();
        assert!(!default.inner.insecure_tls_skip_verify);
        let with_flag = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--insecure-tls-skip-verify",
        ])
        .unwrap();
        assert!(with_flag.inner.insecure_tls_skip_verify);
    }

    #[test]
    fn all_three_flags_combined_parse_ok() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--insecure-registry",
            "core:8080",
            "--registry-ca-cert",
            "/etc/ssl/ca.pem",
            "--insecure-tls-skip-verify",
        ])
        .unwrap();
        assert_eq!(parsed.inner.insecure_registry, vec!["core:8080".to_string()]);
        assert_eq!(
            parsed.inner.registry_ca_cert,
            vec![std::path::PathBuf::from("/etc/ssl/ca.pem")]
        );
        assert!(parsed.inner.insecure_tls_skip_verify);
    }

    // ── Enrichment-control flag tests ─────────────────────────────

    #[test]
    fn no_clearly_defined_flag_parses() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan", "--path", ".", "--no-clearly-defined",
        ])
        .unwrap();
        assert!(parsed.inner.no_clearly_defined);
        assert!(!parsed.inner.no_deps_dev);
        assert!(!parsed.inner.no_deps_dev_graph);
        // Milestone 207 (#596): new `--no-deps-dev-license` flag
        // defaults to OFF (enrichment on).
        assert!(!parsed.inner.no_deps_dev_license);
    }

    #[test]
    fn no_deps_dev_flag_parses() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan", "--path", ".", "--no-deps-dev",
        ])
        .unwrap();
        assert!(parsed.inner.no_deps_dev);
        assert!(!parsed.inner.no_clearly_defined);
        assert!(!parsed.inner.no_deps_dev_graph);
    }

    #[test]
    fn no_deps_dev_graph_flag_parses() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan", "--path", ".", "--no-deps-dev-graph",
        ])
        .unwrap();
        assert!(!parsed.inner.no_clearly_defined);
        assert!(parsed.inner.no_deps_dev_graph);
    }

    #[test]
    fn all_no_flags_combine() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--no-clearly-defined",
            "--no-deps-dev",
            "--no-deps-dev-graph",
        ])
        .unwrap();
        assert!(parsed.inner.no_clearly_defined);
        assert!(parsed.inner.no_deps_dev);
        assert!(parsed.inner.no_deps_dev_graph);
    }

    #[test]
    fn enrich_sources_parses_comma_separated() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--enrich-sources",
            "deps-dev,clearly-defined",
        ])
        .unwrap();
        assert_eq!(
            parsed.inner.enrich_sources,
            vec![EnrichSource::DepsDev, EnrichSource::ClearlyDefined]
        );
    }

    #[test]
    fn enrich_sources_single_value() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--enrich-sources",
            "deps-dev-graph",
        ])
        .unwrap();
        assert_eq!(
            parsed.inner.enrich_sources,
            vec![EnrichSource::DepsDevGraph]
        );
    }

    #[test]
    fn enrich_sources_rejects_unknown_value() {
        let err = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--enrich-sources",
            "clear-defined",
        ])
        .unwrap_err()
        .to_string();
        assert!(
            err.to_lowercase().contains("invalid value")
                || err.to_lowercase().contains("possible values"),
            "expected clap to reject unknown enrich-sources value, got: {err}"
        );
    }

    #[test]
    fn enrich_sources_defaults_to_empty() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan", "--path", ".",
        ])
        .unwrap();
        assert!(parsed.inner.enrich_sources.is_empty());
    }

    // ── resolve_enrich_sources logic tests ────────────────────────

    /// Helper: build a minimal ScanArgs with only the enrichment
    /// fields set, rest defaulted.
    fn enrich_args(
        no_deps_dev: bool,
        no_clearly_defined: bool,
        no_deps_dev_graph: bool,
        no_deps_dev_license: bool,
        enrich_sources: Vec<EnrichSource>,
    ) -> ScanArgs {
        ScanArgs {
            path: Some(PathBuf::from(".")),
            nix_eval: false,
            nix_eval_system: None,
            nix_eval_timeout_secs: None,
            nix_closure: false,
            nix_closure_attr: None,
            nix_closure_require_root_contents: false,
            nix_allow_ifd: false,
            no_nixpkgs_haskell: false,
            no_nixpkgs_haskell_closure: false,
            no_nixpkgs_security: false,
            nixpkgs_timeout_secs: 30,
            image: None,
            image_src: vec![],
            image_platform: None,
            no_oci_cache: false,
            oci_cache_size: None,
            registry_credentials_dir: None,
            // Issue #927 — batched enrichment is the default, so the
            // helper must default to it too. A helper still defaulting to
            // the per-component path would make every test exercise the
            // path users no longer get.
            enrich_batch: false,
            no_enrich_batch: false,
            enrich_cache_max_age: None,
            enrich_no_cache: false,
            // Milestone 182 — test helper defaults preserve pre-m182
            // behavior (no insecure registries, no additional CAs,
            // full TLS verification). Byte-identity SC-004.
            insecure_registry: vec![],
            registry_ca_cert: vec![],
            insecure_tls_skip_verify: false,
            // Milestone 186 — test helper defaults preserve pre-m186
            // behavior (no Referrers-API query; byte-identity SC-004).
            sbom_source: SbomSourceMode::Scan,
            // Milestone 188 — test helper defaults preserve pre-m188
            // behavior (no Helm chart processing; byte-identity per
            // FR-016 / SC-005).
            helm_chart: None,
            helm_render: false,
            // Milestone 221 US2a — test helper defaults preserve pre-m221
            // behavior (no signing; byte-identity guaranteed per FR-009).
            sign_key: None,
            sign_key_passphrase_env: None,
            // Milestone 222 US2b — test helper defaults preserve pre-m222
            // behavior (no keyless signing; byte-identity guaranteed per
            // FR-015). Fulcio + Rekor URL defaults match the CLI's own
            // defaults so any test that flips `sign = true` gets the
            // same endpoint resolution the real CLI would.
            sign: false,
            fulcio_url: "https://fulcio.sigstore.dev".to_string(),
            rekor_url: "https://rekor.sigstore.dev".to_string(),
            rekor_timeout_secs: 30,
            // Milestone 221 US4 — no --sbom-version by default; CDX
            // metadata.version stays at the pre-m221 hardcoded 1 and
            // SPDX outputs emit no waybill:sbom-version annotation.
            sbom_version: None,
            split: None,
            output_dir: None,
            output: vec![],
            format: vec![],
            max_file_size: 256 * 1024 * 1024,
            // Milestone 144 — test helper preserves default-cap behavior
            // (RpmReaderConfig::default() at 512 MiB) and no distro
            // override; equivalent to operator omitting both flags.
            max_rpm_bytes: None,
            rpm_distro: None,
            no_hashes: false,
            deb_codename: None,
            no_package_db: false,
            no_deep_hash: false,
            fast_container_extract: false,
            conclude_licenses: false,
            // Test helper preserves byte-identity on enrichment-
            // focused unit tests by keeping file-tier emission off.
            // Production default in `scan_cmd.rs` is `orphan` per
            // milestone 133 US1.C.
            file_inventory: "off".to_string(),
            file_inventory_size_limit: 100 * 1024 * 1024,
            // Milestone 671 T009 — test helper default: no source-shape
            // restriction. `None` on the `Option<SourceShapeSet>` field.
            file_inventory_source_shapes: None,
            json: false,
            clearly_defined: false,
            no_clearly_defined,
            no_deps_dev,
            no_deps_dev_graph,
            no_deps_dev_license,
            enrich_sources,
            bind_to_source: None,
            repo: None,
            git_ref: None,
            image_id: None,
            attestation: None,
            id: vec![],
            keep_credentials_in_identifiers: false,
            subject_hash: vec![],
            component_id: vec![],
            root_name: None,
            root_version: None,
            root_purl_type: None,
            no_root_purl: false,
            root_purl: None,
            preserve_manifest_main_module: false,
            // Milestone 080 — defaults for new fields keep the test
            // helper's "minimal flags" contract intact.
            creator: vec![],
            annotator: vec![],
            annotation_comment: vec![],
            metadata_comment: None,
            scan_target_name: None,
            metadata_file: None,
            // Milestone 081 — default the new operator-assert flag to
            // None so the helper's "minimal flags" contract holds.
            sbom_type: None,
            spdx2_relationship_compat: crate::generate::Spdx2RelationshipCompat::Full,
            // Milestone 102 — default vendored-deps emission OFF.
            include_vendored: false,
            // Milestone 156 — default third_party/ recursive-walk OFF.
            cmake_third_party_recursive: false,
            // Milestone 235 — default Gradle ladder opt-out (subprocess off,
            // no daemon, no buildscript, 300s timeout, no extra configs).
            gradle: GradleCliFlags::default(),
            // Milestone 108 — default external fingerprint-corpus opt-in OFF.
            fingerprints_corpus: false,
            // Milestone 110 Phase 5-Slim — defaults for new multi-
            // source flags keep the test helper's "minimal flags"
            // contract intact.
            fingerprints_source: vec![],
            fingerprints_source_no_default: false,
            // Milestone 111 — default no operator-supplied PURL aliases.
            pkg_alias: vec![],
            // Milestone 108 US5 — default runtime SHA override unset.
            fingerprints_rev: None,
            // Milestone 173 — test helper defaults match CLI defaults.
            warm_go_cache: WarmGoCacheMode::Off,
            warm_go_cache_concurrency: 4,
            // Milestone 210 — trace-noise filter escape hatch off
            // by default. Individual tests flip on when auditing
            // secret paths in fixtures.
            include_system_reads: false,
            // Milestone 218 — opt-in cross-ecosystem edges off by
            // default (matches CLI default; preserves SC-009).
            experimental_cross_ecosystem_edges: false,
            // Milestone 220 — default project-discovery mode = All
            // (matches CLI default; preserves SC-005 byte-identity).
            project_discovery: crate::generate::project_discovery::ProjectDiscoveryMode::All,
            tier: TierMode::All,
            // Milestone 665 — test helper defaults preserve pre-665
            // behavior (no binary-scan suppression; byte-identity
            // per FR-003).
            no_binary_scan: None,
            no_go_proxy_fetch: false,
        }
    }

    #[test]
    fn resolve_defaults_resolution_on_clearly_defined_off() {
        // #930: ClearlyDefined is enrichment, so it is opt-in.
        let args = enrich_args(false, false, false, false, vec![]);
        let cfg = resolve_enrich_sources(&args);
        assert_eq!(cfg, EnrichConfig {
            deps_dev: true,
            clearly_defined: false,
            deps_dev_graph: true,
        });
    }

    #[test]
    fn resolve_clearly_defined_flag_opts_in() {
        let mut args = enrich_args(false, false, false, false, vec![]);
        args.clearly_defined = true;
        assert!(resolve_enrich_sources(&args).clearly_defined);
    }

    #[test]
    fn clearly_defined_and_its_negation_conflict() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan", "--path", ".", "--clearly-defined", "--no-clearly-defined",
        ]);
        assert!(parsed.is_err());
    }

    #[test]
    fn resolve_no_clearly_defined_disables_cd() {
        let args = enrich_args(false, true, false, false, vec![]);
        let cfg = resolve_enrich_sources(&args);
        assert!(cfg.deps_dev);
        assert!(!cfg.clearly_defined);
        assert!(cfg.deps_dev_graph);
    }

    #[test]
    fn resolve_enrich_no_deps_dev_disables_both_paths_m207() {
        // Milestone 207 (#596) US1 acceptance — `--no-deps-dev` is
        // now an aggregate disable: BOTH the license AND dep-graph
        // paths turn off. Pre-m207 this test asserted deps_dev_graph
        // stayed true.
        let mut args = enrich_args(true, false, false, false, vec![]);
        // #930: ClearlyDefined is opt-in; opt in so this still tests that
        // the deps.dev flags leave it alone.
        args.clearly_defined = true;
        let cfg = resolve_enrich_sources(&args);
        assert_eq!(
            cfg,
            EnrichConfig {
                deps_dev: false,
                clearly_defined: true,
                deps_dev_graph: false,
            }
        );
    }

    #[test]
    fn resolve_enrich_no_deps_dev_license_disables_license_only_m207() {
        // Milestone 207 (#596) US2 acceptance — new fine-grained
        // flag `--no-deps-dev-license` restores the pre-m207
        // `--no-deps-dev` semantic (license only).
        let mut args = enrich_args(false, false, false, true, vec![]);
        // #930: ClearlyDefined is opt-in; opt in so this still tests that
        // the deps.dev flags leave it alone.
        args.clearly_defined = true;
        let cfg = resolve_enrich_sources(&args);
        assert_eq!(
            cfg,
            EnrichConfig {
                deps_dev: false,
                clearly_defined: true,
                deps_dev_graph: true,
            }
        );
    }

    #[test]
    fn resolve_enrich_no_deps_dev_wins_over_no_deps_dev_graph_m207() {
        // Composition sanity — `--no-deps-dev` combined with
        // `--no-deps-dev-graph` produces the same aggregate result.
        let mut args = enrich_args(true, false, true, false, vec![]);
        // #930: ClearlyDefined is opt-in; opt in so this still tests that
        // the deps.dev flags leave it alone.
        args.clearly_defined = true;
        let cfg = resolve_enrich_sources(&args);
        assert_eq!(
            cfg,
            EnrichConfig {
                deps_dev: false,
                clearly_defined: true,
                deps_dev_graph: false,
            }
        );
    }

    #[test]
    fn resolve_enrich_no_deps_dev_license_and_graph_equals_aggregate_m207() {
        // Composition sanity — setting both fine-grained flags is
        // equivalent to setting the aggregate `--no-deps-dev`.
        let mut args = enrich_args(false, false, true, true, vec![]);
        // #930: ClearlyDefined is opt-in; opt in so this still tests that
        // the deps.dev flags leave it alone.
        args.clearly_defined = true;
        let cfg = resolve_enrich_sources(&args);
        assert_eq!(
            cfg,
            EnrichConfig {
                deps_dev: false,
                clearly_defined: true,
                deps_dev_graph: false,
            }
        );
    }

    #[test]
    fn resolve_enrich_sources_allowlist_overrides_no_deps_dev_m207() {
        // FR-004 regression guard — allowlist mode ignores every
        // `--no-*` flag, including the aggregate `--no-deps-dev`.
        let args = enrich_args(
            true, false, false, false,
            vec![EnrichSource::DepsDev],
        );
        let cfg = resolve_enrich_sources(&args);
        assert!(cfg.deps_dev, "allowlist wins per FR-004");
    }

    #[test]
    fn resolve_enrich_no_clearly_defined_unaffected_by_no_deps_dev_m207() {
        // Regression guard — `--no-deps-dev` doesn't affect the
        // ClearlyDefined path.
        let mut args = enrich_args(true, false, false, false, vec![]);
        // #930: ClearlyDefined is opt-in; opt in so this still tests that
        // the deps.dev flags leave it alone.
        args.clearly_defined = true;
        let cfg = resolve_enrich_sources(&args);
        assert!(cfg.clearly_defined);
    }

    #[test]
    fn no_deps_dev_help_mentions_enrich_sources_m207() {
        // FR-008 — operators reading `waybill sbom scan --help`
        // see the composition hint next to the flag they're setting.
        // `ScanArgsForTest` flattens `ScanArgs` at the top level (no
        // subcommand nesting), so args are directly discoverable via
        // `get_arguments`.
        let cmd = <ScanArgsForTest as clap::CommandFactory>::command();
        let arg = cmd
            .get_arguments()
            .find(|a| a.get_id().as_str() == "no_deps_dev")
            .expect("--no-deps-dev arg present");
        // long_help includes the multi-line doc-comment; short help
        // is just the first line. Combine both to search.
        let help_text = format!(
            "{}\n{}",
            arg.get_help().map(|s| s.to_string()).unwrap_or_default(),
            arg.get_long_help().map(|s| s.to_string()).unwrap_or_default(),
        );
        assert!(
            help_text.contains("enrich-sources"),
            "FR-008: --no-deps-dev help text must mention `--enrich-sources`. Got:\n{help_text}"
        );
    }

    #[test]
    fn resolve_no_deps_dev_graph_disables_graph() {
        let mut args = enrich_args(false, false, true, false, vec![]);
        // #930: ClearlyDefined is opt-in; opt in so this still tests that
        // the deps.dev flags leave it alone.
        args.clearly_defined = true;
        let cfg = resolve_enrich_sources(&args);
        assert!(cfg.deps_dev);
        assert!(cfg.clearly_defined);
        assert!(!cfg.deps_dev_graph);
    }

    #[test]
    fn resolve_all_no_flags_disables_everything() {
        let args = enrich_args(true, true, true, false, vec![]);
        let cfg = resolve_enrich_sources(&args);
        assert_eq!(cfg, EnrichConfig {
            deps_dev: false,
            clearly_defined: false,
            deps_dev_graph: false,
        });
    }

    #[test]
    fn resolve_allowlist_overrides_no_flags() {
        // --enrich-sources clearly-defined --no-clearly-defined
        // → allowlist wins: CD enabled
        let args = enrich_args(
            false,
            true,  // --no-clearly-defined
            true,  // --no-deps-dev-graph
            false, // --no-deps-dev-license (m207)
            vec![EnrichSource::ClearlyDefined],
        );
        let cfg = resolve_enrich_sources(&args);
        assert!(!cfg.deps_dev);         // not in allowlist
        assert!(cfg.clearly_defined);   // in allowlist, overrides --no flag
        assert!(!cfg.deps_dev_graph);   // not in allowlist
    }

    #[test]
    fn resolve_allowlist_subset_only_enables_listed() {
        let args = enrich_args(
            false, false, false, false,
            vec![EnrichSource::DepsDev, EnrichSource::DepsDevGraph],
        );
        let cfg = resolve_enrich_sources(&args);
        assert!(cfg.deps_dev);
        assert!(!cfg.clearly_defined); // not in allowlist
        assert!(cfg.deps_dev_graph);
    }

    // ----------------------------------------------------------------
    // Milestone 073 — identifier resolution pipeline
    // (T013 unit-test coverage). FR-006 + FR-009 override-position
    // rule.
    // ----------------------------------------------------------------

    use waybill::binding::identifiers::Identifier;

    fn make_id(raw: &str, label: Option<&str>) -> Identifier {
        let mut id = Identifier::parse(raw).unwrap();
        id.source_label = label.map(|s| s.to_string());
        id
    }

    // Milestone 074 (T005): resolve_identifiers moved to
    // `waybill::binding::identifiers::resolve_identifiers` with a
    // `Vec<Identifier>`-based auto-detected param. Tests pass through
    // an alias so the existing assertions read the same.
    use waybill::binding::identifiers::resolve_identifiers;

    #[test]
    fn resolve_auto_detected_only_emits_one_entry() {
        let auto = make_id("repo:git@github.com:foo/bar.git", Some("auto"));
        let out = resolve_identifiers(vec![auto.clone()], &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].as_wire(), auto.as_wire());
    }

    #[test]
    fn resolve_manual_only_emits_in_supply_order() {
        let m1 = make_id("repo:git@example.com:a.git", None);
        let m2 = make_id("acme_corp_id:abc123", None);
        let out = resolve_identifiers(vec![], &[m1.clone(), m2.clone()]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].as_wire(), m1.as_wire());
        assert_eq!(out[1].as_wire(), m2.as_wire());
    }

    #[test]
    fn resolve_manual_inherits_auto_detected_position_on_dedup() {
        // (c) — manual entry with same (scheme, value) as auto-detected
        // inherits the auto-detected entry's position (front of list).
        let auto = make_id("repo:git@github.com:foo/bar.git", Some("auto-label"));
        let manual_dup = make_id("repo:git@github.com:foo/bar.git", None);
        let manual_other = make_id("acme_corp_id:abc", None);
        let out = resolve_identifiers(
            vec![auto.clone()],
            &[manual_dup.clone(), manual_other.clone()],
        );
        assert_eq!(out.len(), 2);
        // Position 0: manual entry inherits auto-detected slot.
        assert_eq!(out[0].as_wire(), manual_dup.as_wire());
        // The replacement carries the manual entry's source_label
        // (None), not the auto-detected label.
        assert_eq!(out[0].source_label, None);
        // Position 1: the other manual entry follows in supply order.
        assert_eq!(out[1].as_wire(), manual_other.as_wire());
    }

    #[test]
    fn resolve_manual_different_value_drops_auto_detected() {
        // (d) — true override: same scheme, different value. The
        // auto-detected entry is dropped, manual follows in supply
        // order (NOT promoted to front).
        let auto = make_id("repo:git@github.com:o/foo.git", Some("auto"));
        let manual_override = make_id("repo:git@github.com:m/foo.git", None);
        let manual_other = make_id("acme_corp_id:abc", None);
        // Supply order: other first, then override. Override should
        // append after `other` (no front-of-list migration).
        let out = resolve_identifiers(
            vec![auto.clone()],
            &[manual_other.clone(), manual_override.clone()],
        );
        assert_eq!(out.len(), 2);
        // After auto-detected dropped, the supply order applies:
        // [other, override].
        assert_eq!(out[0].as_wire(), manual_other.as_wire());
        assert_eq!(out[1].as_wire(), manual_override.as_wire());
    }

    #[test]
    fn resolve_two_manual_with_same_scheme_value_first_wins() {
        // (e) — manual-vs-manual collision on (scheme, value):
        // first-supplied wins.
        let m1 = make_id("acme_corp_id:abc123", None);
        let m2 = make_id("acme_corp_id:abc123", None);
        let out = resolve_identifiers(vec![], &[m1.clone(), m2.clone()]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].as_wire(), m1.as_wire());
    }

    // Milestone 074: build-tier multi-auto-detected-entry coverage.
    #[test]
    fn resolve_multi_auto_detected_per_scheme_override_only_target_scheme() {
        // Build-tier scenario: auto-detected [repo:, git:].
        // Manual --repo with a different value should drop only the
        // auto-detected `repo:`, leaving the auto-detected `git:`
        // intact.
        let auto_repo = make_id("repo:git@github.com:o/foo.git", Some("auto-build-tier"));
        let auto_git = make_id(
            "git:git@github.com:o/foo.git#0123456789abcdef0123456789abcdef01234567",
            Some("auto-build-tier"),
        );
        let manual_override_repo = make_id("repo:git@github.com:m/foo.git", None);
        let out = resolve_identifiers(
            vec![auto_repo.clone(), auto_git.clone()],
            std::slice::from_ref(&manual_override_repo),
        );
        // Expected: auto-detected `git:` stays at position 0,
        // manual `repo:` appended at position 1 (supply-order).
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].as_wire(), auto_git.as_wire());
        assert_eq!(out[1].as_wire(), manual_override_repo.as_wire());
    }

    #[test]
    fn resolve_multi_auto_detected_exact_dedup_per_entry() {
        let auto_repo = make_id("repo:git@github.com:o/foo.git", Some("auto"));
        let auto_git = make_id(
            "git:git@github.com:o/foo.git#0123456789abcdef0123456789abcdef01234567",
            Some("auto"),
        );
        // Manual --repo matching the auto-detected one: dedup in place.
        let manual_dup_repo = make_id("repo:git@github.com:o/foo.git", None);
        let out = resolve_identifiers(
            vec![auto_repo.clone(), auto_git.clone()],
            std::slice::from_ref(&manual_dup_repo),
        );
        assert_eq!(out.len(), 2);
        // Repo at index 0 has been replaced by manual (label is None).
        assert_eq!(out[0].as_wire(), manual_dup_repo.as_wire());
        assert_eq!(out[0].source_label, None);
        // Git remains at index 1, with its auto-detected label.
        assert_eq!(out[1].as_wire(), auto_git.as_wire());
        assert_eq!(out[1].source_label.as_deref(), Some("auto"));
    }

    // ----------------------------------------------------------------
    // parse_user_defined_id_flag — `--id` value parsing
    // ----------------------------------------------------------------

    #[test]
    fn parse_user_defined_id_flag_accepts_user_defined_scheme() {
        let id = parse_user_defined_id_flag("acme_corp_id=abc123").unwrap();
        assert_eq!(id.scheme.as_str(), "acme_corp_id");
        assert_eq!(id.value.as_str(), "abc123");
        assert!(matches!(
            id.kind,
            waybill::binding::identifiers::IdentifierKind::UserDefined
        ));
    }

    #[test]
    fn parse_user_defined_id_flag_value_can_contain_equals() {
        // Split-on-first-`=` rule: trailing `=`s belong to the value.
        let id = parse_user_defined_id_flag("acme_corp_id=key=val=foo").unwrap();
        assert_eq!(id.scheme.as_str(), "acme_corp_id");
        assert_eq!(id.value.as_str(), "key=val=foo");
    }

    #[test]
    fn parse_user_defined_id_flag_rejects_missing_separator() {
        let err = parse_user_defined_id_flag("acme_corp_id_no_eq").unwrap_err();
        assert!(
            err.contains("missing `=` separator"),
            "expected missing-separator error; got {err}"
        );
    }

    #[test]
    fn parse_user_defined_id_flag_rejects_empty_value() {
        let err = parse_user_defined_id_flag("acme_corp_id=").unwrap_err();
        assert!(
            err.contains("identifier value is empty"),
            "expected EmptyValue error; got {err}"
        );
    }

    #[test]
    fn parse_user_defined_id_flag_rejects_invalid_scheme() {
        let err = parse_user_defined_id_flag("ACME_CORP_ID=abc").unwrap_err();
        assert!(
            err.contains("fails regex"),
            "expected InvalidSchemeName error; got {err}"
        );
    }

    #[test]
    fn parse_user_defined_id_flag_rejects_each_built_in_scheme() {
        // Per the user-instruction: --id <built-in>=<value> MUST
        // produce a clap parse error pointing at the dedicated flag.
        for built_in in ["repo", "git", "image", "attestation"] {
            let raw = format!("{built_in}=anything");
            let err = parse_user_defined_id_flag(&raw).unwrap_err();
            assert!(
                err.contains("--id rejects the built-in scheme")
                    && err.contains(built_in)
                    && err.contains("--repo")
                    && err.contains("--image-id"),
                "expected built-in-rejection error pointing at the dedicated flag; got {err}"
            );
        }
    }

    // ----------------------------------------------------------------
    // assemble_manual_identifiers — translate dedicated flags into Vec
    // ----------------------------------------------------------------

    #[test]
    fn assemble_manual_identifiers_repo_only_emits_repo_scheme() {
        let mut args = enrich_args(false, false, false, false, vec![]);
        args.repo = Some("git@github.com:foo/bar.git".to_string());
        let ids = assemble_manual_identifiers(&args);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].as_wire(), "repo:git@github.com:foo/bar.git");
    }

    #[test]
    fn assemble_manual_identifiers_repo_plus_git_ref_emits_git_only() {
        // --repo + --git-ref → ONE git: identifier (supersedes repo:),
        // not two entries.
        let mut args = enrich_args(false, false, false, false, vec![]);
        args.repo = Some("https://github.com/foo/bar".to_string());
        args.git_ref = Some("abc1234567890".to_string());
        let ids = assemble_manual_identifiers(&args);
        assert_eq!(ids.len(), 1);
        assert_eq!(
            ids[0].as_wire(),
            "git:https://github.com/foo/bar#abc1234567890"
        );
    }

    #[test]
    fn assemble_manual_identifiers_image_attestation_id_in_supply_order() {
        let mut args = enrich_args(false, false, false, false, vec![]);
        args.image_id = Some("docker.io/foo/bar:v1".to_string());
        args.attestation = Some("https://example.org/att/1".to_string());
        args.id = vec![
            parse_user_defined_id_flag("acme_corp_id=svc-alpha").unwrap(),
            parse_user_defined_id_flag("internal_ticket=PROJ-456").unwrap(),
        ];
        let ids = assemble_manual_identifiers(&args);
        assert_eq!(ids.len(), 4);
        assert_eq!(ids[0].scheme.as_str(), "image");
        assert_eq!(ids[1].scheme.as_str(), "attestation");
        assert_eq!(ids[2].scheme.as_str(), "acme_corp_id");
        assert_eq!(ids[3].scheme.as_str(), "internal_ticket");
    }

    // ----------------------------------------------------------------
    // parse_image_ref_components — image-tier auto-detection helper
    // ----------------------------------------------------------------

    #[test]
    fn parse_image_ref_full_form() {
        let (registry, name, tag) =
            parse_image_ref_components("docker.io/acme/foo:v1");
        assert_eq!(registry.as_deref(), Some("docker.io"));
        assert_eq!(name, "acme/foo");
        assert_eq!(tag.as_deref(), Some("v1"));
    }

    #[test]
    fn parse_image_ref_no_registry() {
        let (registry, name, tag) = parse_image_ref_components("acme/foo:v1");
        assert_eq!(registry, None);
        assert_eq!(name, "acme/foo");
        assert_eq!(tag.as_deref(), Some("v1"));
    }

    #[test]
    fn parse_image_ref_no_tag() {
        let (registry, name, tag) = parse_image_ref_components("docker.io/acme/foo");
        assert_eq!(registry.as_deref(), Some("docker.io"));
        assert_eq!(name, "acme/foo");
        assert_eq!(tag, None);
    }

    #[test]
    fn parse_image_ref_localhost_registry() {
        let (registry, name, tag) =
            parse_image_ref_components("localhost:5000/foo:v1");
        assert_eq!(registry.as_deref(), Some("localhost:5000"));
        assert_eq!(name, "foo");
        assert_eq!(tag.as_deref(), Some("v1"));
    }

    #[test]
    fn parse_image_ref_strips_trailing_digest() {
        let (registry, name, tag) = parse_image_ref_components(
            "docker.io/acme/foo:v1@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        );
        assert_eq!(registry.as_deref(), Some("docker.io"));
        assert_eq!(name, "acme/foo");
        assert_eq!(tag.as_deref(), Some("v1"));
    }

    // ---------- Milestone 077 — validate_root_field ----------

    #[test]
    fn validate_root_field_accepts_simple_name() {
        let r = validate_root_field("widget-svc", "--root-name");
        assert_eq!(r.as_deref().ok(), Some("widget-svc"));
    }

    #[test]
    fn validate_root_field_accepts_npm_scoped_name() {
        // Per Q1 clarification: `@` and `/` are PURL-reserved but NOT
        // rejected at parse — they're URL-encoded at PURL emission.
        let r = validate_root_field("@acme/widget-svc", "--root-name");
        assert_eq!(r.as_deref().ok(), Some("@acme/widget-svc"));
    }

    #[test]
    fn validate_root_field_accepts_version_with_dots() {
        let r = validate_root_field("1.2.3", "--root-version");
        assert_eq!(r.as_deref().ok(), Some("1.2.3"));
    }

    #[test]
    fn validate_root_field_rejects_empty() {
        let r = validate_root_field("", "--root-name");
        let err = r.unwrap_err();
        assert!(err.contains("must not be empty"), "got: {err}");
        assert!(err.contains("--root-name"), "got: {err}");
    }

    #[test]
    fn validate_root_field_rejects_whitespace() {
        let r = validate_root_field("my widget svc", "--root-name");
        let err = r.unwrap_err();
        assert!(err.contains("whitespace"), "got: {err}");
        assert!(err.contains("position 2"), "got: {err}");
    }

    #[test]
    fn validate_root_field_rejects_control_char() {
        let r = validate_root_field("foo\x01bar", "--root-name");
        let err = r.unwrap_err();
        assert!(err.contains("control character"), "got: {err}");
        assert!(err.contains("U+0001"), "got: {err}");
    }

    #[test]
    fn validate_root_field_rejects_question_mark() {
        let r = validate_root_field("foo?bar", "--root-name");
        let err = r.unwrap_err();
        assert!(err.contains("URL-syntax-breaking"), "got: {err}");
        assert!(err.contains("'?'"), "got: {err}");
        assert!(err.contains("position 3"), "got: {err}");
    }

    #[test]
    fn validate_root_field_rejects_hash() {
        let r = validate_root_field("foo#bar", "--root-name");
        let err = r.unwrap_err();
        assert!(err.contains("URL-syntax-breaking"), "got: {err}");
        assert!(err.contains("'#'"), "got: {err}");
    }

    // ──────────────────────────────────────────────────────────────
    // Milestone 111 — pkg-alias wiring tests.
    //
    // Cover the algorithmic surface: build_pkg_alias_map (CLI + env
    // composition + conflict detection) and attach_bindings_to_components
    // (alias-rewrite behavior, FR-007 reason rewrite, FR-013 envelope
    // population, FR-011 unused-alias tracking, SC-004 byte-identity
    // for no-alias path). End-to-end CLI integration via a real
    // --image fixture is deferred to a follow-on PR.
    // ──────────────────────────────────────────────────────────────

    use waybill::binding::alias::{AliasMap, PurlAlias};
    use waybill::binding::{
        BindingStrength, SourceDocumentBinding, SourceDocumentId, SourceSbomContext,
    };
    use waybill_common::resolution::ResolvedComponent;
    use waybill_common::types::purl::Purl;

    /// Shared env-mutation lock for env-var-touching tests below.
    /// Same pattern as milestone 110's `fingerprints::test_env_lock`.
    fn pkg_alias_env_lock() -> std::sync::MutexGuard<'static, ()> {
        use std::sync::Mutex;
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Milestone 1281 (#1346), FR-004 — a closure license fills a gap but
    /// never replaces a reader's own declaration.
    ///
    /// Before this, the merge carried `extra_annotations` and nothing else,
    /// so every member that matched a manifest component lost its license
    /// silently — the closure contributed it and the merge dropped it.
    #[test]
    fn m1281_closure_licenses_fill_a_gap_but_never_replace_one() {
        use waybill_common::types::license::SpdxExpression;
        let mit = SpdxExpression::try_canonical("MIT").unwrap();
        let apache = SpdxExpression::try_canonical("Apache-2.0").unwrap();

        // The existing component already knows its license: it wins.
        let mut existing = vec![make_component("pkg:generic/held@1.0")];
        existing[0].licenses = vec![apache.clone()];
        let mut from_closure = make_component("pkg:generic/held@1.0");
        from_closure.licenses = vec![mit.clone()];
        let (merged, appended, _) =
            merge_closure_components(&mut existing, vec![from_closure]);
        assert_eq!((merged, appended), (1, 0));
        assert_eq!(
            existing[0].licenses.iter().map(|e| e.as_str()).collect::<Vec<_>>(),
            vec!["Apache-2.0"],
            "a reader that knew the ecosystem must not be overruled"
        );

        // The existing component has none: the closure's declaration is the
        // only evidence there is, and must survive the merge.
        let mut bare = vec![make_component("pkg:generic/bare@1.0")];
        assert!(bare[0].licenses.is_empty());
        let mut from_closure = make_component("pkg:generic/bare@1.0");
        from_closure.licenses = vec![mit.clone()];
        merge_closure_components(&mut bare, vec![from_closure]);
        assert_eq!(
            bare[0].licenses.iter().map(|e| e.as_str()).collect::<Vec<_>>(),
            vec!["MIT"],
            "dropping it here would discard the only license available"
        );
    }

    /// #1466, milestone 1282 FR-011 — a member that declares no `pname` is
    /// appended beside a component of the same `(name, version)`, never folded
    /// into it.
    #[test]
    fn m1466_a_nameless_member_is_appended_beside_its_namesake() {
        use crate::scan_fs::package_db::nix::closure::member::ANN_MEMBER_ROLE;
        let named = |purl: &str| {
            let mut c = make_component(purl);
            c.name = "waybill-fixture-clock".to_string();
            c.version = String::new();
            c
        };
        let mut existing = vec![named("pkg:hackage/waybill-fixture-clock")];
        let mut fragment = named("pkg:generic/waybill-fixture-clock-c10ck");
        fragment
            .extra_annotations
            .insert(ANN_MEMBER_ROLE.to_string(), serde_json::Value::from("fragment"));
        let (merged, appended, _) = merge_closure_components(&mut existing, vec![fragment]);
        assert_eq!((merged, appended), (0, 1));
        assert!(!existing[0].extra_annotations.contains_key(ANN_MEMBER_ROLE));

        // CONTROL: the same pair without the role still merges, so the
        // exclusion is the role and not the PURL or the empty version.
        let mut existing = vec![named("pkg:hackage/waybill-fixture-clock")];
        let (merged, appended, _) = merge_closure_components(
            &mut existing,
            vec![named("pkg:generic/waybill-fixture-clock-c10ck")],
        );
        assert_eq!((merged, appended), (1, 0));
    }

    /// Milestone 1377 FR-013 — a closure member carrying a nixpkgs-declared
    /// CPE, as `closure::emit` builds one.
    fn m1377_declared(purl: &str, cpe: &str, overrode: Option<&str>) -> ResolvedComponent {
        use crate::scan_fs::package_db::nix::closure::identifiers as ids;
        let mut c = make_component(purl);
        c.cpes = vec![cpe.to_string()];
        c.extra_annotations.insert(
            ids::ANN_DECLARED_CPE.to_string(),
            ids::provenance(ids::JoinBasis::Drv, overrode),
        );
        c
    }

    const M1377_DECLARED: &str = "cpe:2.3:a:waybill-fixture-vendor:zlib:9.9.9:*:*:*:*:*:*:*";

    /// FR-013 — the existing component carries exactly the curated CPE: the
    /// declared one replaces it, and the provenance names what it overrode
    /// on THIS component.
    #[test]
    fn m1377_a_declared_cpe_replaces_an_existing_curated_cpe() {
        use crate::scan_fs::package_db::nix::closure::identifiers as ids;
        let mut existing = vec![make_component("pkg:generic/zlib@9.9.9")];
        existing[0].cpes = crate::generate::cpe::synthesize_cpes(&existing[0]);
        let curated = existing[0].cpes.first().cloned().expect("zlib is curated");
        merge_closure_components(
            &mut existing,
            vec![m1377_declared("pkg:generic/zlib@9.9.9", M1377_DECLARED, None)],
        );
        assert_eq!(existing[0].cpes, vec![M1377_DECLARED.to_string()]);
        assert_eq!(
            existing[0].extra_annotations.get(ids::ANN_DECLARED_CPE),
            Some(&ids::provenance(ids::JoinBasis::Drv, Some(&curated)))
        );
    }

    /// FR-013 — an empty slot is filled, with nothing recorded as overridden.
    #[test]
    fn m1377_a_declared_cpe_fills_an_empty_slot() {
        use crate::scan_fs::package_db::nix::closure::identifiers as ids;
        let mut existing = vec![make_component("pkg:generic/waybill-fixture-x@1.0")];
        let cpe = "cpe:2.3:a:waybill-fixture-vendor:waybill-fixture-x:1.0:*:*:*:*:*:*:*";
        merge_closure_components(
            &mut existing,
            vec![m1377_declared("pkg:generic/waybill-fixture-x@1.0", cpe, None)],
        );
        assert_eq!(existing[0].cpes, vec![cpe.to_string()]);
        assert_eq!(
            existing[0].extra_annotations.get(ids::ANN_DECLARED_CPE),
            Some(&ids::provenance(ids::JoinBasis::Drv, None))
        );
    }

    /// FR-013 — any other CPE (a reader found it in the artifact) is kept,
    /// and no nixpkgs provenance is attached to a CPE nixpkgs did not supply.
    #[test]
    fn m1377_a_reader_derived_cpe_is_kept_without_nixpkgs_provenance() {
        use crate::scan_fs::package_db::nix::closure::identifiers as ids;
        let reader = "cpe:2.3:a:zlib:zlib:9.9.9:*:*:*:*:*:*:arm64";
        let mut existing = vec![make_component("pkg:generic/zlib@9.9.9")];
        existing[0].cpes = vec![reader.to_string()];
        merge_closure_components(
            &mut existing,
            vec![m1377_declared("pkg:generic/zlib@9.9.9", M1377_DECLARED, None)],
        );
        assert_eq!(existing[0].cpes, vec![reader.to_string()]);
        assert!(!existing[0].extra_annotations.contains_key(ids::ANN_DECLARED_CPE));
    }

    /// FR-013 — an operator's `--supplement-cdx` CPE is kept even when it
    /// equals the curated value: who asserted it is what matters.
    #[test]
    fn m1377_an_operator_cpe_equal_to_the_curated_one_is_kept() {
        use crate::scan_fs::package_db::nix::closure::identifiers as ids;
        let mut existing = vec![make_component("pkg:generic/zlib@9.9.9")];
        existing[0].cpes = crate::generate::cpe::synthesize_cpes(&existing[0]);
        let operator = existing[0].cpes.clone();
        existing[0].extra_annotations.insert(
            "waybill:source-tier".to_string(),
            serde_json::Value::String("declared".to_string()),
        );
        merge_closure_components(
            &mut existing,
            vec![m1377_declared("pkg:generic/zlib@9.9.9", M1377_DECLARED, None)],
        );
        assert_eq!(existing[0].cpes, operator);
        assert!(!existing[0].extra_annotations.contains_key(ids::ANN_DECLARED_CPE));
    }

    fn make_alias(lhs: &str, rhs: &str) -> PurlAlias {
        PurlAlias::try_new(lhs, rhs).unwrap()
    }

    fn make_component(purl_str: &str) -> ResolvedComponent {
        use waybill_common::resolution::{ResolutionEvidence, ResolutionTechnique};
        let purl = Purl::new(purl_str).unwrap();
        ResolvedComponent {
            build_inclusion: None,
            artifact_presence: None,
            scope_evidence: None,
            name: purl.name().to_string(),
            version: purl.version().unwrap_or("0.0.0").to_string(),
            purl,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 0.95,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: vec![],
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: vec![],
            extra_annotations: Default::default(),
            binary_role: None,
        }
    }

    fn make_source_ctx(
        purls: &[(&str, Option<SourceDocumentBinding>)],
    ) -> SourceSbomContext {
        let mut source_purls = std::collections::BTreeSet::new();
        let mut source_bindings_by_purl = std::collections::BTreeMap::new();
        for (purl, binding) in purls {
            source_purls.insert((*purl).to_string());
            if let Some(b) = binding {
                source_bindings_by_purl.insert((*purl).to_string(), b.clone());
            }
        }
        SourceSbomContext {
            source_doc_id: SourceDocumentId {
                sha256: "1".repeat(64),
                iri: None,
            },
            source_purls,
            source_bindings_by_purl,
            binary_name_to_purl: std::collections::HashMap::new(),
        }
    }

    fn fixture_verified_source_binding() -> SourceDocumentBinding {
        SourceDocumentBinding {
            source_doc_id: SourceDocumentId {
                sha256: "1".repeat(64),
                iri: None,
            },
            hash: Some(
                waybill::binding::BindingHash::from_hex("a".repeat(64)).unwrap(),
            ),
            strength: BindingStrength::Verified,
            reason: None,
            algo: "v1".to_string(),
            alias_from: None,
            alias_to: None,
            alias_source: None,
        }
    }

    // ── build_pkg_alias_map ───────────────────────────────────────

    #[test]
    fn build_pkg_alias_map_empty_when_no_input() {
        let _g = pkg_alias_env_lock();
        let mut args = enrich_args(false, false, false, false, vec![]);
        args.pkg_alias = vec![];
        unsafe {
            std::env::remove_var("WAYBILL_PKG_ALIAS");
        }
        let map = build_pkg_alias_map(&args).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn build_pkg_alias_map_collects_cli_flags() {
        let _g = pkg_alias_env_lock();
        let mut args = enrich_args(false, false, false, false, vec![]);
        args.pkg_alias =
            vec![make_alias("pkg:generic/baz", "pkg:cargo/baz@1.0.0")];
        unsafe {
            std::env::remove_var("WAYBILL_PKG_ALIAS");
        }
        let map = build_pkg_alias_map(&args).unwrap();
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn build_pkg_alias_map_unions_cli_and_env_var_entries() {
        let _g = pkg_alias_env_lock();
        let mut args = enrich_args(false, false, false, false, vec![]);
        args.pkg_alias =
            vec![make_alias("pkg:generic/baz", "pkg:cargo/baz@1.0.0")];
        unsafe {
            std::env::set_var(
                "WAYBILL_PKG_ALIAS",
                "pkg:generic/qux=pkg:npm/qux@2.0.0",
            );
        }
        let map = build_pkg_alias_map(&args).unwrap();
        unsafe {
            std::env::remove_var("WAYBILL_PKG_ALIAS");
        }
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn build_pkg_alias_map_skips_blank_env_entries() {
        let _g = pkg_alias_env_lock();
        let mut args = enrich_args(false, false, false, false, vec![]);
        args.pkg_alias = vec![];
        unsafe {
            std::env::set_var(
                "WAYBILL_PKG_ALIAS",
                ",,pkg:generic/baz=pkg:cargo/baz@1.0.0,,",
            );
        }
        let map = build_pkg_alias_map(&args).unwrap();
        unsafe {
            std::env::remove_var("WAYBILL_PKG_ALIAS");
        }
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn build_pkg_alias_map_rejects_conflicting_lhs_across_cli_and_env() {
        let _g = pkg_alias_env_lock();
        let mut args = enrich_args(false, false, false, false, vec![]);
        args.pkg_alias =
            vec![make_alias("pkg:generic/baz", "pkg:cargo/baz@1.0.0")];
        unsafe {
            std::env::set_var(
                "WAYBILL_PKG_ALIAS",
                "pkg:generic/baz=pkg:cargo/baz@1.1.0",
            );
        }
        let result = build_pkg_alias_map(&args);
        unsafe {
            std::env::remove_var("WAYBILL_PKG_ALIAS");
        }
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("declared twice with conflicting RHS"),
            "expected conflict-named error; got: {msg}"
        );
    }

    // ── attach_bindings_to_components: alias-rewrite ──────────────

    #[test]
    fn attach_bindings_empty_alias_map_preserves_pre_feature_behavior() {
        // SC-004 byte-identity prerequisite: with no aliases, the
        // function MUST behave exactly as before — components with
        // PURL in source get the source binding; components without
        // get Unknown { reason: "source-not-found-in-bind-target" }.
        let mut components = vec![make_component("pkg:cargo/baz@1.0.0")];
        let ctx = make_source_ctx(&[(
            "pkg:cargo/baz@1.0.0",
            Some(fixture_verified_source_binding()),
        )]);
        let alias_map = AliasMap::new();

        let consumed =
            attach_bindings_to_components(&mut components, &ctx, &alias_map);

        assert!(consumed.is_empty());
        let envelope = components[0]
            .extra_annotations
            .get(waybill::binding::BINDING_PROPERTY_NAME)
            .expect("envelope present");
        let binding: SourceDocumentBinding =
            serde_json::from_value(envelope.clone()).unwrap();
        assert_eq!(binding.strength, BindingStrength::Verified);
        assert!(binding.alias_from.is_none());
        assert!(binding.alias_to.is_none());
    }

    #[test]
    fn attach_bindings_with_alias_rewrites_lookup_and_stamps_envelope() {
        // The US1 motivating case: image-tier emits pkg:generic/baz,
        // source-tier carries pkg:cargo/baz@1.0.0. Alias declares the
        // synonym → binding.strength becomes Verified AND
        // alias_from/alias_to are populated.
        let mut components = vec![make_component("pkg:generic/baz")];
        let ctx = make_source_ctx(&[(
            "pkg:cargo/baz@1.0.0",
            Some(fixture_verified_source_binding()),
        )]);
        let mut alias_map = AliasMap::new();
        alias_map
            .insert(make_alias("pkg:generic/baz", "pkg:cargo/baz@1.0.0"))
            .unwrap();

        let consumed =
            attach_bindings_to_components(&mut components, &ctx, &alias_map);

        assert_eq!(consumed.len(), 1);
        assert!(consumed.contains("pkg:generic/baz"));

        let envelope = components[0]
            .extra_annotations
            .get(waybill::binding::BINDING_PROPERTY_NAME)
            .expect("envelope present");
        let binding: SourceDocumentBinding =
            serde_json::from_value(envelope.clone()).unwrap();
        assert_eq!(
            binding.strength,
            BindingStrength::Verified,
            "alias-rewritten lookup should bind to source's verified entry"
        );
        assert_eq!(
            binding.alias_from.as_ref().unwrap().as_str(),
            "pkg:generic/baz"
        );
        assert_eq!(
            binding.alias_to.as_ref().unwrap().as_str(),
            "pkg:cargo/baz@1.0.0"
        );
    }

    #[test]
    fn attach_bindings_rewrites_reason_when_alias_target_missing() {
        // FR-007: when an alias was applied but the RHS PURL is NOT
        // in the bind-source SBOM, the reason rewrites from
        // `source-not-found-in-bind-target` to
        // `alias-target-not-found-in-bind-target` so operators can
        // distinguish the alias-misconfiguration case from the
        // missing-source-document case.
        let mut components = vec![make_component("pkg:generic/baz")];
        // Source SBOM is empty — neither LHS nor RHS exists in it.
        let ctx = make_source_ctx(&[]);
        let mut alias_map = AliasMap::new();
        alias_map
            .insert(make_alias("pkg:generic/baz", "pkg:cargo/baz@1.0.0"))
            .unwrap();

        attach_bindings_to_components(&mut components, &ctx, &alias_map);

        let envelope = components[0]
            .extra_annotations
            .get(waybill::binding::BINDING_PROPERTY_NAME)
            .expect("envelope present");
        let binding: SourceDocumentBinding =
            serde_json::from_value(envelope.clone()).unwrap();
        assert_eq!(binding.strength, BindingStrength::Unknown);
        assert_eq!(
            binding.reason.as_deref(),
            Some("alias-target-not-found-in-bind-target")
        );
        // alias_from/alias_to MUST still be populated so consumers can
        // see the alias was attempted even though it didn't resolve.
        assert!(binding.alias_from.is_some());
        assert!(binding.alias_to.is_some());
    }

    #[test]
    fn attach_bindings_unused_lhs_not_in_consumed_set() {
        // FR-011: when an alias's LHS does not match any scan-output
        // component, the LHS is NOT marked as consumed (the caller
        // logs it as an info-level unused-alias diagnostic).
        let mut components = vec![make_component("pkg:cargo/other@1.0.0")];
        let ctx = make_source_ctx(&[("pkg:cargo/other@1.0.0", None)]);
        let mut alias_map = AliasMap::new();
        alias_map
            .insert(make_alias("pkg:generic/baz", "pkg:cargo/baz@1.0.0"))
            .unwrap();

        let consumed =
            attach_bindings_to_components(&mut components, &ctx, &alias_map);

        assert!(consumed.is_empty(), "no alias LHS matched any component");
    }

    #[test]
    fn attach_bindings_supports_same_rhs_multiple_lhs_distinct_components() {
        // U1 non-collapse invariant per /speckit-analyze: two distinct
        // LHS aliases targeting the same RHS keep their components
        // distinct in the output; both bind to the same source-tier
        // counterpart with strength=Verified.
        let mut components = vec![
            make_component("pkg:generic/baz-cli"),
            make_component("pkg:generic/baz-daemon"),
        ];
        let ctx = make_source_ctx(&[(
            "pkg:cargo/baz@1.0.0",
            Some(fixture_verified_source_binding()),
        )]);
        let mut alias_map = AliasMap::new();
        alias_map
            .insert(make_alias("pkg:generic/baz-cli", "pkg:cargo/baz@1.0.0"))
            .unwrap();
        alias_map
            .insert(make_alias(
                "pkg:generic/baz-daemon",
                "pkg:cargo/baz@1.0.0",
            ))
            .unwrap();

        let consumed =
            attach_bindings_to_components(&mut components, &ctx, &alias_map);

        assert_eq!(
            consumed.len(),
            2,
            "both LHSes should be consumed independently"
        );
        for c in &components {
            let envelope = c
                .extra_annotations
                .get(waybill::binding::BINDING_PROPERTY_NAME)
                .expect("envelope present on both");
            let binding: SourceDocumentBinding =
                serde_json::from_value(envelope.clone()).unwrap();
            assert_eq!(binding.strength, BindingStrength::Verified);
            assert_eq!(
                binding.alias_to.as_ref().unwrap().as_str(),
                "pkg:cargo/baz@1.0.0"
            );
        }
        // Critical: component identities (their own purl) MUST NOT
        // collapse — two distinct components must still be present.
        assert_eq!(components.len(), 2);
        assert_eq!(components[0].purl.as_str(), "pkg:generic/baz-cli");
        assert_eq!(components[1].purl.as_str(), "pkg:generic/baz-daemon");
    }

    // ----- Milestone 144: clap-parser tests for --max-rpm-bytes
    // and --rpm-distro (T025-T027 + T032-T034). -----

    /// T025 (FR-005): valid unsigned integer is accepted; `parsed.inner.max_rpm_bytes`
    /// carries the operator-supplied value.
    #[test]
    fn max_rpm_bytes_accepts_valid_unsigned() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--max-rpm-bytes",
            "1073741824",
        ])
        .unwrap();
        assert_eq!(parsed.inner.max_rpm_bytes, Some(1073741824));
    }

    /// T026 (FR-005 + US3-3): zero is rejected at clap parse time with
    /// the "must be > 0" error message.
    #[test]
    fn max_rpm_bytes_rejects_zero() {
        let err = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--max-rpm-bytes",
            "0",
        ])
        .unwrap_err()
        .to_string();
        assert!(err.contains("must be > 0"), "got: {err}");
    }

    /// T027 (FR-005): non-numeric input is rejected at clap parse time.
    #[test]
    fn max_rpm_bytes_rejects_non_numeric() {
        let err = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--max-rpm-bytes",
            "abc",
        ])
        .unwrap_err();
        // Error message will mention parsing failure; exact wording is
        // clap-version-dependent, so just assert it failed at parse time.
        let s = err.to_string();
        assert!(!s.is_empty(), "expected a parse error");
    }

    /// T032 (FR-003): a lowercase slug is accepted unchanged.
    #[test]
    fn rpm_distro_accepts_lowercase_slug() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--rpm-distro",
            "poky",
        ])
        .unwrap();
        assert_eq!(parsed.inner.rpm_distro.as_deref(), Some("poky"));
    }

    /// T033 (FR-003 + US4-3): empty string is rejected at clap parse time
    /// with the "must be non-empty" error message.
    #[test]
    fn rpm_distro_rejects_empty_string() {
        let err = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--rpm-distro",
            "",
        ])
        .unwrap_err()
        .to_string();
        assert!(err.contains("must be non-empty"), "got: {err}");
    }

    /// T034 (FR-003 + assumption 2): mixed-case input is lowercased by
    /// the `value_parser` closure so the resulting PURL namespace is
    /// canonical per purl-spec §lower-case-rules.
    #[test]
    fn rpm_distro_lowercases_input() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--rpm-distro",
            "Poky",
        ])
        .unwrap();
        assert_eq!(parsed.inner.rpm_distro.as_deref(), Some("poky"));
    }

    // =========================================================
    // Milestone 232 — --tier=<mode> output-filter flag
    // =========================================================

    fn mk_tier_component(purl_str: &str, tier: &str) -> ResolvedComponent {
        let mut c = make_component(purl_str);
        c.sbom_tier = Some(tier.to_string());
        c
    }

    fn mk_edge(from: &str, to: &str) -> waybill_common::resolution::Relationship {
        use waybill_common::resolution::{
            EnrichmentProvenance, Relationship, RelationshipType,
        };
        Relationship {
            from: from.to_string(),
            to: to.to_string(),
            relationship_type: RelationshipType::DependsOn,
            provenance: EnrichmentProvenance {
                source: "test".to_string(),
                data_type: "dependency".to_string(),
            },
        }
    }

    #[test]
    fn apply_tier_filter_source_only_drops_design() {
        let mut components = vec![
            mk_tier_component("pkg:cargo/src1@1.0.0", "source"),
            mk_tier_component("pkg:cargo/src2@1.0.0", "source"),
            mk_tier_component("pkg:cargo/src3@1.0.0", "source"),
            mk_tier_component("pkg:cargo/dsn1@1.0.0", "design"),
            mk_tier_component("pkg:cargo/dsn2@1.0.0", "design"),
            mk_tier_component("pkg:cargo/bin1@1.0.0", "binary"),
        ];
        let mut edges: Vec<waybill_common::resolution::Relationship> = vec![];
        apply_tier_filter(&mut components, &mut edges, TierMode::SourceOnly);
        assert_eq!(components.len(), 3);
        assert!(components.iter().all(|c| c.sbom_tier.as_deref() == Some("source")));
    }

    #[test]
    fn apply_tier_filter_drops_dangling_edges() {
        let mut components = vec![
            mk_tier_component("pkg:cargo/src1@1.0.0", "source"),
            mk_tier_component("pkg:cargo/src2@1.0.0", "source"),
            mk_tier_component("pkg:cargo/dsn1@1.0.0", "design"),
            mk_tier_component("pkg:cargo/dsn2@1.0.0", "design"),
        ];
        let mut edges = vec![
            mk_edge("pkg:cargo/src1@1.0.0", "pkg:cargo/src2@1.0.0"),
            mk_edge("pkg:cargo/src1@1.0.0", "pkg:cargo/dsn1@1.0.0"),
            mk_edge("pkg:cargo/dsn1@1.0.0", "pkg:cargo/src2@1.0.0"),
            mk_edge("pkg:cargo/dsn1@1.0.0", "pkg:cargo/dsn2@1.0.0"),
        ];
        apply_tier_filter(&mut components, &mut edges, TierMode::SourceOnly);
        assert_eq!(edges.len(), 1, "only src→src survives; got {:?}", edges);
        assert_eq!(edges[0].from, "pkg:cargo/src1@1.0.0");
        assert_eq!(edges[0].to, "pkg:cargo/src2@1.0.0");
    }

    #[test]
    fn apply_tier_filter_design_only_keeps_only_design() {
        let mut components = vec![
            mk_tier_component("pkg:cargo/src1@1.0.0", "source"),
            mk_tier_component("pkg:cargo/src2@1.0.0", "source"),
            mk_tier_component("pkg:cargo/src3@1.0.0", "source"),
            mk_tier_component("pkg:cargo/dsn1@1.0.0", "design"),
            mk_tier_component("pkg:cargo/dsn2@1.0.0", "design"),
            mk_tier_component("pkg:cargo/bin1@1.0.0", "binary"),
        ];
        let mut edges: Vec<waybill_common::resolution::Relationship> = vec![];
        apply_tier_filter(&mut components, &mut edges, TierMode::DesignOnly);
        assert_eq!(components.len(), 2);
        assert!(components.iter().all(|c| c.sbom_tier.as_deref() == Some("design")));
    }

    #[test]
    fn apply_tier_filter_source_and_binary_keeps_both() {
        let mut components = vec![
            mk_tier_component("pkg:cargo/src1@1.0.0", "source"),
            mk_tier_component("pkg:cargo/src2@1.0.0", "source"),
            mk_tier_component("pkg:cargo/src3@1.0.0", "source"),
            mk_tier_component("pkg:cargo/dsn1@1.0.0", "design"),
            mk_tier_component("pkg:cargo/dsn2@1.0.0", "design"),
            mk_tier_component("pkg:cargo/bin1@1.0.0", "binary"),
        ];
        let mut edges: Vec<waybill_common::resolution::Relationship> = vec![];
        apply_tier_filter(&mut components, &mut edges, TierMode::SourceAndBinary);
        assert_eq!(components.len(), 4, "3 source + 1 binary survive");
        assert!(components
            .iter()
            .all(|c| matches!(c.sbom_tier.as_deref(), Some("source") | Some("binary"))));
    }

    #[test]
    fn apply_tier_filter_all_is_noop() {
        let mut components = vec![
            mk_tier_component("pkg:cargo/src1@1.0.0", "source"),
            mk_tier_component("pkg:cargo/dsn1@1.0.0", "design"),
        ];
        let mut edges = vec![mk_edge(
            "pkg:cargo/src1@1.0.0",
            "pkg:cargo/dsn1@1.0.0",
        )];
        apply_tier_filter(&mut components, &mut edges, TierMode::All);
        assert_eq!(components.len(), 2, "All mode is a no-op filter");
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn apply_tier_filter_empty_result() {
        // FR-008 unit-level check — an all-design fixture with
        // TierMode::SourceOnly leaves both components and edges empty.
        // The WARN log emission is asserted at integration-test tier
        // via T017b.
        let mut components = vec![
            mk_tier_component("pkg:cargo/dsn1@1.0.0", "design"),
            mk_tier_component("pkg:cargo/dsn2@1.0.0", "design"),
        ];
        let mut edges: Vec<waybill_common::resolution::Relationship> =
            vec![mk_edge("pkg:cargo/dsn1@1.0.0", "pkg:cargo/dsn2@1.0.0")];
        apply_tier_filter(&mut components, &mut edges, TierMode::SourceOnly);
        assert!(components.is_empty());
        assert!(edges.is_empty());
    }

    #[test]
    fn apply_tier_filter_analyzed_and_file_dropped_under_source_only() {
        // FR-010 strict-literal-match — analyzed and file tiers do NOT
        // survive source-only per Clarifications §1.
        let mut components = vec![
            mk_tier_component("pkg:cargo/src1@1.0.0", "source"),
            mk_tier_component("pkg:cargo/ana1@1.0.0", "analyzed"),
            mk_tier_component("pkg:generic/file1@0.0.0", "file"),
        ];
        let mut edges: Vec<waybill_common::resolution::Relationship> = vec![];
        apply_tier_filter(&mut components, &mut edges, TierMode::SourceOnly);
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].sbom_tier.as_deref(), Some("source"));
    }

    // =========================================================
    // Feature 1269 (#1209) — --exclude-scope, extracted + relocated
    // =========================================================

    fn mk_scoped(
        purl_str: &str,
        scope: waybill_common::resolution::LifecycleScope,
        inferred: bool,
    ) -> ResolvedComponent {
        let mut c = make_component(purl_str);
        c.lifecycle_scope = Some(scope);
        if inferred {
            c.extra_annotations.insert(
                crate::resolve::scope_propagation::ANN_SCOPE_DERIVATION.to_string(),
                serde_json::Value::String(
                    crate::resolve::scope_propagation::DERIVATION_TRANSITIVE.to_string(),
                ),
            );
        }
        c
    }

    /// An empty exclude list changes nothing — the default for every
    /// scan that does not pass the flag, so the no-op has to be exact
    /// rather than merely cheap.
    #[test]
    fn exclude_scope_empty_list_is_a_no_op() {
        use waybill_common::resolution::LifecycleScope;
        let mut components = vec![
            mk_scoped("pkg:cargo/dev@1.0.0", LifecycleScope::Development, false),
            make_component("pkg:cargo/rt@1.0.0"),
        ];
        let mut edges = vec![mk_edge("pkg:cargo/rt@1.0.0", "pkg:cargo/dev@1.0.0")];
        let drops = apply_exclude_scope_filter(&mut components, &mut edges, &[]);
        assert_eq!(drops, ExcludeScopeDrops::default());
        assert_eq!(components.len(), 2);
        assert_eq!(edges.len(), 1);
    }

    /// The scope match is per-variant: excluding `Development` must not
    /// take `Build` or `Test` with it. `apply_tier_filter`'s sibling
    /// test asserts the same shape for tiers.
    #[test]
    fn exclude_scope_drops_only_the_named_variants() {
        use waybill_common::resolution::LifecycleScope;
        let mut components = vec![
            mk_scoped("pkg:cargo/dev@1.0.0", LifecycleScope::Development, false),
            mk_scoped("pkg:cargo/bld@1.0.0", LifecycleScope::Build, false),
            mk_scoped("pkg:cargo/tst@1.0.0", LifecycleScope::Test, false),
            mk_scoped("pkg:cargo/opt@1.0.0", LifecycleScope::Optional, false),
            mk_scoped("pkg:cargo/run@1.0.0", LifecycleScope::Runtime, false),
            make_component("pkg:cargo/none@1.0.0"),
        ];
        let mut edges: Vec<waybill_common::resolution::Relationship> = vec![];
        let drops = apply_exclude_scope_filter(
            &mut components,
            &mut edges,
            &[LifecycleScope::Development, LifecycleScope::Test],
        );
        assert_eq!(drops.total, 2);
        let surviving: Vec<&str> =
            components.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(surviving, vec!["bld", "opt", "run", "none"]);
    }

    /// FR-006's shape, inherited from m052: an edge whose source OR
    /// target was dropped goes with it, so the document never carries a
    /// reference to a component it does not hold.
    #[test]
    fn exclude_scope_drops_edges_on_either_endpoint() {
        use waybill_common::resolution::LifecycleScope;
        let mut components = vec![
            mk_scoped("pkg:cargo/dev@1.0.0", LifecycleScope::Development, false),
            make_component("pkg:cargo/root@1.0.0"),
            make_component("pkg:cargo/leaf@1.0.0"),
        ];
        let mut edges = vec![
            // dropped as target
            mk_edge("pkg:cargo/root@1.0.0", "pkg:cargo/dev@1.0.0"),
            // dropped as source
            mk_edge("pkg:cargo/dev@1.0.0", "pkg:cargo/leaf@1.0.0"),
            // survives: neither endpoint dropped
            mk_edge("pkg:cargo/root@1.0.0", "pkg:cargo/leaf@1.0.0"),
        ];
        let drops = apply_exclude_scope_filter(
            &mut components,
            &mut edges,
            &[LifecycleScope::Development],
        );
        assert_eq!(drops.total, 1);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to, "pkg:cargo/leaf@1.0.0");
        assert_eq!(edges[0].from, "pkg:cargo/root@1.0.0");
    }

    /// The ordering check in observable form (#1209).
    ///
    /// `inferred` counts dropped components carrying C62
    /// `waybill:lifecycle-scope-derivation = transitive` — a scope that
    /// exists only because `propagate_non_runtime_scope` ran first.
    /// Move the call site back above that pass and this number is 0 on
    /// every input, because no component carries the marker yet. It is
    /// the one property of the relocation that a test can hold, since
    /// the relocation itself is a call-site position.
    #[test]
    fn exclude_scope_counts_the_scopes_the_propagation_pass_inferred() {
        use waybill_common::resolution::LifecycleScope;
        let mut components = vec![
            // declared by a reader
            mk_scoped("pkg:cargo/dev@1.0.0", LifecycleScope::Development, false),
            // inferred by the 1269 pass — two of them, so the count
            // cannot pass by being accidentally 1
            mk_scoped("pkg:cargo/t1@1.0.0", LifecycleScope::Development, true),
            mk_scoped("pkg:cargo/t2@1.0.0", LifecycleScope::Development, true),
            // inferred, but a scope that is NOT excluded: must not be
            // counted, because it is not dropped
            mk_scoped("pkg:cargo/t3@1.0.0", LifecycleScope::Test, true),
            make_component("pkg:cargo/rt@1.0.0"),
        ];
        let mut edges: Vec<waybill_common::resolution::Relationship> = vec![];
        let drops = apply_exclude_scope_filter(
            &mut components,
            &mut edges,
            &[LifecycleScope::Development],
        );
        assert_eq!(
            drops,
            ExcludeScopeDrops {
                total: 3,
                inferred: 2
            },
            "3 development components dropped, 2 of them scoped by the \
             propagation pass rather than by a reader"
        );
        let surviving: Vec<&str> =
            components.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(surviving, vec!["t3", "rt"]);
    }

    /// C62's enum is open and shared: the Go readers write
    /// `test-only-closure` and `go-mod-why` into the same key. Those
    /// are reader-derived, so they must not be counted as `inferred` —
    /// otherwise the ordering signal reads non-zero on a Go scan
    /// whatever the call site's position, which is exactly the kind of
    /// proxy that makes a check look like it passed when it never ran.
    #[test]
    fn exclude_scope_does_not_count_the_go_readers_own_derivations() {
        use waybill_common::resolution::LifecycleScope;
        let mut go_derived = make_component("pkg:golang/example.test/m@v1.0.0");
        go_derived.lifecycle_scope = Some(LifecycleScope::Test);
        go_derived.extra_annotations.insert(
            crate::resolve::scope_propagation::ANN_SCOPE_DERIVATION.to_string(),
            serde_json::Value::String("test-only-closure".to_string()),
        );
        let mut components = vec![go_derived];
        let mut edges: Vec<waybill_common::resolution::Relationship> = vec![];
        let drops = apply_exclude_scope_filter(
            &mut components,
            &mut edges,
            &[LifecycleScope::Test],
        );
        assert_eq!(
            drops,
            ExcludeScopeDrops {
                total: 1,
                inferred: 0
            },
        );
        assert!(components.is_empty());
    }
}

/// The nixpkgs revision `flake.lock` pins, for the milestone-1281 license
/// lookup.
///
/// Reads the lockfile and nothing else — deliberately not
/// `load_package_set`, which also retrieves the Haskell package set and would
/// turn a license lookup into a fetch. Every failure is `None`: a repository
/// without a pinned nixpkgs is a repository whose closure is emitted
/// unlicensed, which is the pre-milestone-1281 behaviour rather than an error.
fn nixpkgs_revision_for_licenses(root: &std::path::Path) -> Option<String> {
    use crate::scan_fs::package_db::nix;
    let lock = root.join("flake.lock");
    if !lock.exists() {
        return None;
    }
    let doc = nix::lockfile::parse_flake_lock(&lock).ok()?;
    nix::haskell_packages::pinned_nixpkgs(&doc)
        .ok()
        .map(|p| p.revision)
}

/// Milestone 1366 (#1365) — the ecosystem identity pass.
///
/// These drive [`run_closure_ecosystem_identity`] directly rather than the
/// binary, because the behaviour under test is reached only when deps.dev is
/// enabled and **the corpus can never reach it**: the one corpus target that
/// runs a closure passes `--no-deps-dev`. A green corpus lane says nothing
/// about confirmation, so the confirming path needs a stub of its own.
#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod closure_ecosystem_identity_tests {
    use super::*;
    use crate::enrich::deps_dev_client::DepsDevClient;
    use crate::enrich::depsdev_source::DepsDevSource;
    use scan_fs::package_db::nix::closure::ecosystem_identity as eco;
    use scan_fs::package_db::nix::closure::{classify::classify, derivation::RawClosure};
    use waybill_common::resolution::ResolvedComponent;

    fn fixture() -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/nix_closure_ecosystem/closure.json"),
        )
        .unwrap()
    }

    /// The fixture, classified and merged exactly as a scan would merge it.
    fn merged() -> (
        scan_fs::package_db::nix::closure::ClassifiedClosure,
        Vec<ResolvedComponent>,
        std::collections::HashSet<(String, String)>,
    ) {
        let raw = RawClosure::parse(&fixture()).unwrap();
        let roles = classify(&raw);
        let classified = scan_fs::package_db::nix::closure::ClassifiedClosure {
            runtime: None,
            attribute: "default".into(),
            raw,
            roles,
            graph_admitted: Default::default(),
            graph_admission: Default::default(),
            read_outcome: Default::default(),
            graph: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
            identifiers: Default::default(),
        };
        // The real projected graph, so the ordering test below has edges to
        // assert on. Built exactly as `closure::resolve` builds it.
        let classified = scan_fs::package_db::nix::closure::ClassifiedClosure {
            graph: scan_fs::package_db::nix::closure::graph::project(
                &classified.raw,
                &scan_fs::package_db::nix::closure::emit::emitted_drv_keys(
                    &classified.raw,
                    &classified.roles,
                ),
            ),
            ..classified
        };
        let added = scan_fs::package_db::nix::closure::emit::components(&classified);
        let mut components: Vec<ResolvedComponent> = Vec::new();
        let (_, _, appended) = merge_closure_components(&mut components, added);
        (classified, components, appended)
    }

    fn purl_of<'a>(components: &'a [ResolvedComponent], name: &str) -> &'a str {
        components
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.purl.as_str())
            .unwrap_or("<absent>")
    }

    /// CONTROL. Every assertion below is about three candidates; if the
    /// fixture stopped producing them these tests would pass over an empty
    /// set and say nothing.
    #[test]
    fn the_fixture_yields_the_three_candidates_the_rule_admits() {
        let (classified, components, appended) = merged();
        let found = eco::candidates(&classified, &components, &appended);
        let mut names: Vec<&str> = found.iter().map(|c| c.declared_name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            vec![
                "waybill-fixture-alpha",
                "waybill-fixture-gamma",
                "waybill-fixture-nonesuch"
            ],
            "the setup hook, the bootstrap variant and the pname-less source \
             member must all be excluded"
        );
        // `gamma` declares its pname ONLY in `structuredAttrs`. Reading the
        // flat `env` map alone would drop it, which is the carrier mistake
        // that produced a reported defect in correct code.
        assert!(found.iter().any(|c| c.declared_name == "waybill-fixture-gamma"));
    }

    /// US2 scenario 2 — a closure of unconfirmable candidates emits today's
    /// document.
    #[tokio::test]
    async fn with_confirmation_disabled_nothing_moves() {
        let (classified, mut components, appended) = merged();
        let before: Vec<String> = components.iter().map(|c| c.purl.as_str().to_string()).collect();

        let record =
            run_closure_ecosystem_identity(&classified, &mut components, &appended, None).await;

        let after: Vec<String> = components.iter().map(|c| c.purl.as_str().to_string()).collect();
        assert_eq!(before, after, "no identifier may move when nothing confirmed");
        assert!(components
            .iter()
            .all(|c| !c.extra_annotations.contains_key(eco::ANN_ECOSYSTEM_IDENTITY)));

        // FR-008: the record still says what WOULD have been done.
        assert_eq!(record.candidates, 3);
        assert_eq!(record.confirmed, 0);
        assert_eq!(record.unchecked, 3);
        assert_eq!(record.reason, Some(eco::UncheckedReason::Disabled));
        assert!(record.is_consistent());
    }

    /// US2 scenario 1 + US1 scenarios 1-3, against a stub registry.
    ///
    /// The 404 arm is the load-bearing half: FR-003 turns on "the ecosystem
    /// has no such package" being a **decline** rather than a failure, and
    /// this is the only place that distinction is exercised.
    #[tokio::test]
    async fn a_confirmed_candidate_moves_and_a_declined_one_does_not() {
        let server = wiremock::MockServer::start().await;
        for (name, version) in [("waybill-fixture-alpha", "1.2.3"), ("waybill-fixture-gamma", "0.4")]
        {
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path(format!(
                    "/systems/pypi/packages/{name}/versions/{version}"
                )))
                .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({
                        "versionKey": {"system": "PYPI", "name": name, "version": version},
                        "licenses": [],
                        "links": []
                    }),
                ))
                .mount(&server)
                .await;
        }
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(
                "/systems/pypi/packages/waybill-fixture-nonesuch/versions/9.9.9",
            ))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let source = DepsDevSource::new(
            DepsDevClient::new(std::time::Duration::from_secs(5)).with_base_url(server.uri()),
            false,
        )
        .with_batch(false)
        .with_disk_cache(false, None);

        let (classified, mut components, appended) = merged();
        let record =
            run_closure_ecosystem_identity(&classified, &mut components, &appended, Some(&source))
                .await;

        assert_eq!(
            purl_of(&components, "waybill-fixture-alpha"),
            "pkg:pypi/waybill-fixture-alpha@1.2.3"
        );
        // Declared only in `structuredAttrs`, and confirmed all the same.
        assert_eq!(
            purl_of(&components, "waybill-fixture-gamma"),
            "pkg:pypi/waybill-fixture-gamma@0.4"
        );
        // A 404 is an answer, and the answer is no. Today's identifier stays.
        assert_eq!(
            purl_of(&components, "waybill-fixture-nonesuch"),
            "pkg:generic/waybill-fixture-nonesuch@9.9.9"
        );
        // FR-005: the source member keeps `pkg:generic`, which is CORRECT
        // for it. 539 of the pinned closure's 781 generic components are
        // members like this one, and moving any of them is the regression
        // SC-003 exists to catch.
        let members: Vec<&ResolvedComponent> = components
            .iter()
            .filter(|c| {
                c.extra_annotations
                    .contains_key("waybill:closure-member-role")
            })
            .collect();
        assert!(!members.is_empty(), "the fixture must contain a member");
        assert!(members
            .iter()
            .all(|c| c.purl.as_str().starts_with("pkg:generic/")));

        assert_eq!(record.candidates, 3);
        assert_eq!(record.confirmed, 2);
        assert_eq!(record.declined, 1);
        assert_eq!(record.unchecked, 0);
        assert_eq!(record.reason, None);
        assert!(record.is_consistent());

        // US3 scenario 2 — the disclosure is present exactly where an
        // identity was derived, and absent where it was not.
        let disclosed = |n: &str| {
            components
                .iter()
                .find(|c| c.name == n)
                .map(|c| c.extra_annotations.contains_key(eco::ANN_ECOSYSTEM_IDENTITY))
                .unwrap_or(false)
        };
        assert!(disclosed("waybill-fixture-alpha"));
        assert!(disclosed("waybill-fixture-gamma"));
        assert!(!disclosed("waybill-fixture-nonesuch"));
        assert!(members
            .iter()
            .all(|c| !c.extra_annotations.contains_key(eco::ANN_ECOSYSTEM_IDENTITY)));
    }

    /// FR-012 — an unreachable registry cannot fail a scan, and must be
    /// distinguishable in the record from one that answered "no".
    #[tokio::test]
    async fn an_unreachable_registry_leaves_every_identifier_alone() {
        // A port nothing listens on, so every request fails at the transport.
        //
        // The first attempt at this control started a `MockServer`, mounted
        // nothing and dropped it before the call — and measured three
        // DECLINES, because the server was still answering and an unmounted
        // path is a 404. That would have asserted the opposite of what this
        // test is for: a 404 is an answer, and the point here is that a
        // connection failure is not one.
        let uri = "http://127.0.0.1:1".to_string();
        let source = DepsDevSource::new(
            DepsDevClient::new(std::time::Duration::from_millis(250)).with_base_url(uri),
            false,
        )
        .with_batch(false)
        .with_disk_cache(false, None);

        let (classified, mut components, appended) = merged();
        let before: Vec<String> = components.iter().map(|c| c.purl.as_str().to_string()).collect();
        let record =
            run_closure_ecosystem_identity(&classified, &mut components, &appended, Some(&source))
                .await;
        let after: Vec<String> = components.iter().map(|c| c.purl.as_str().to_string()).collect();

        assert_eq!(before, after);
        assert_eq!(record.confirmed, 0);
        assert_eq!(record.declined, 0, "a transport failure is NOT a decline");
        assert_eq!(record.unchecked, 3);
        assert_eq!(record.reason, Some(eco::UncheckedReason::Unreachable));
    }

    /// Research R11 — **the ordering is the mechanism, not a convenience.**
    ///
    /// `Relationship.from` and `.to` are PURL strings, and
    /// `closure::graph::relationships` resolves them by reading `c.purl` from
    /// the live component slice when it is called. The identity pass therefore
    /// has exactly one correct window: after the merge, before the edge build.
    ///
    /// Move the `run_closure_ecosystem_identity` call below
    /// `graph::relationships` in `execute` and this goes red — every edge that
    /// pointed at a corrected component would name a `pkg:generic` coordinate
    /// no component carries any more, and each of those components would be
    /// an orphan. That failure mode would make this feature a net regression
    /// against milestone 158's graph-completeness record, so it is asserted
    /// rather than trusted to a comment.
    #[tokio::test]
    async fn the_edges_are_built_from_the_corrected_identifiers() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(
                "/systems/pypi/packages/waybill-fixture-alpha/versions/1.2.3",
            ))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
                serde_json::json!({
                    "versionKey": {"system":"PYPI","name":"waybill-fixture-alpha","version":"1.2.3"},
                    "licenses": [], "links": []
                }),
            ))
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::any())
            .respond_with(wiremock::ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let source = DepsDevSource::new(
            DepsDevClient::new(std::time::Duration::from_secs(5)).with_base_url(server.uri()),
            false,
        )
        .with_batch(false)
        .with_disk_cache(false, None);

        let (classified, mut components, appended) = merged();

        // CONTROL: before the pass, the edges name the generic coordinate.
        let (before, _) = scan_fs::package_db::nix::closure::graph::relationships(
            &classified.graph,
            &classified.raw,
            &components,
        );
        assert!(
            before
                .iter()
                .any(|r| r.to == "pkg:generic/waybill-fixture-alpha@1.2.3"),
            "the fixture must produce an edge onto alpha, or this test asserts              nothing; got {:?}",
            before.iter().map(|r| &r.to).collect::<Vec<_>>()
        );

        run_closure_ecosystem_identity(&classified, &mut components, &appended, Some(&source))
            .await;

        let (after, _) = scan_fs::package_db::nix::closure::graph::relationships(
            &classified.graph,
            &classified.raw,
            &components,
        );
        assert!(
            after
                .iter()
                .any(|r| r.to == "pkg:pypi/waybill-fixture-alpha@1.2.3"),
            "the edge must follow the corrected identifier; got {:?}",
            after.iter().map(|r| &r.to).collect::<Vec<_>>()
        );
        assert!(
            !after
                .iter()
                .any(|r| r.to == "pkg:generic/waybill-fixture-alpha@1.2.3"),
            "no edge may still name the superseded coordinate"
        );
        // The graph did not change shape, only the names in it.
        assert_eq!(before.len(), after.len());
    }
}

/// Milestone 1367 (#1371) — the CPE coverage pass.
///
/// These are unit tests over `fill_missing_cpes` and its version predicate.
/// The integration behaviour is in `tests/closure_component_cpes.rs`; what
/// cannot be tested there is the `--supplement-cdx` precedence case and the
/// release-shape guard, because **the pinned closure cannot produce either**:
/// no component on it both receives a supplement and matches the curated
/// table, and no mapped component carries a non-release version. Both
/// therefore need constructed input, which is what this module is for.
#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod cpe_coverage_tests {
    use super::*;
    use waybill_common::resolution::ResolvedComponent;

    /// Every shape in the accept/decline table was read off the pinned
    /// closure, not invented: the declining five are `libcxx`, `libedit`,
    /// `CoreSymbolication`, the nixpkgs hooks and the bootstrap stages.
    #[test]
    fn release_shapes_are_accepted_and_nix_shapes_are_not() {
        for v in ["3.6.2", "8.20.0", "10.46", "21.1.8", "1.3.2", "1.0.2k", "9"] {
            assert!(is_release_shaped(v), "{v} must be accepted");
        }
        for v in [
            "21.1.6+apple-sdk-26.4", // libcxx
            "20251016-3.1",          // libedit
            "0-unstable-2018-06-17", // CoreSymbolication
            "26.05pre-git",          // nixpkgs hooks
            "boot",                  // bootstrap stages
            "",
        ] {
            assert!(!is_release_shaped(v), "{v} must be declined");
        }
    }

    /// #1283 — nixpkgs' glibc carries its patch level after a dash
    /// (`2.40-66`: 2.40 with 66 backported patches). grype matched
    /// `cpe:2.3:a:gnu:glibc:2.40-66` for all 36 glibc findings of
    /// slack-web's recall truth (recall-truth run 43107760056), so this
    /// shape is matchable, unlike the declined ones above. Only a dotted
    /// release followed by one dash and digits is admitted.
    #[test]
    fn a_dotted_release_with_a_numeric_patch_level_is_accepted() {
        for v in ["2.40-66", "2.42-51", "2.40-36"] {
            assert!(is_release_shaped(v), "{v} must be accepted");
        }
        for v in [
            "2.40-",       // no patch level
            "2.40-rc1",    // not numeric
            "2.40-66-1",   // two dashes
            "2.40-6.1",    // dotted after the dash
            "240-66",      // no dot before the dash
            "1.0.2k-3",    // letter-suffixed release
            "-66",         // nothing before the dash
        ] {
            assert!(!is_release_shaped(v), "{v} must be declined");
        }
    }

    /// FR-003 — a curated name does not rescue a version NVD cannot match.
    #[test]
    fn a_mapped_name_with_a_nix_version_gets_nothing() {
        let mut c = vec![component("openssl", "0-unstable-2018-06-17")];
        fill_missing_cpes(&mut c);
        let cov = cpe_coverage(&c);
        assert!(c[0].cpes.is_empty());
        assert_eq!(cov.with_candidates, 0);
        // Milestone 1373 (#1375) — this component used to be in **no**
        // bucket: the v1 metric counted inside the synthesis loop, and that
        // loop skips a non-release-shaped version before counting anything.
        //
        // It is now `unaddressable`, which is the truthful answer. It is
        // `pkg:generic`, so no feed indexes it, and its version means no CPE
        // can be synthesized either — it is reachable by nothing, and that is
        // precisely what the count is for. Components like this are why the
        // v1 figure read 284 where the real number is 733.
        assert_eq!(cov.unaddressable, 1);
        assert_eq!(cov.total(), 1, "every component lands in exactly one bucket");
    }

    /// FR-002 — a closure member gets nothing.
    ///
    /// **This member carries a release version on purpose.** Measured on the
    /// pinned closure, *zero* of the 539 members reach the pass eligible —
    /// only one carries a version at all and its value is `boot`. So a test
    /// written against the real closure would pass whether or not this guard
    /// exists, which is no test at all.
    #[test]
    fn a_closure_member_gets_nothing_even_with_a_release_version() {
        let mut c = vec![component("openssl", "3.6.2")];
        c[0].extra_annotations.insert(
            "waybill:closure-member-role".to_string(),
            serde_json::Value::from("source"),
        );
        fill_missing_cpes(&mut c);
        let cov = cpe_coverage(&c);
        assert!(
            c[0].cpes.is_empty(),
            "a source tarball named like a library is not that library"
        );
        assert_eq!(cov.with_candidates, 0);
    }

    /// FR-007 — an unmapped name is silent, and counted.
    #[test]
    fn an_unmapped_name_is_counted_not_ignored() {
        let mut c = vec![component("waybill-fixture-unmapped", "1.2.3")];
        fill_missing_cpes(&mut c);
        let cov = cpe_coverage(&c);
        assert!(c[0].cpes.is_empty());
        assert_eq!(cov.with_candidates, 0);
        assert_eq!(
            cov.unaddressable, 1,
            "a component the table has no row for must be visible in C209, or \
             'no mapped software' and 'the pass did not run' stay the same \
             document"
        );
    }

    /// FR-001 — the six are what this feature is for.
    #[test]
    fn a_curated_name_with_a_release_version_gets_candidates() {
        let mut c = vec![component("openssl", "3.6.2")];
        fill_missing_cpes(&mut c);
        let cov = cpe_coverage(&c);
        assert!(
            c[0].cpes.iter().any(|v| v.starts_with("cpe:2.3:a:openssl:openssl:3.6.2")),
            "got {:?}",
            c[0].cpes
        );
        assert_eq!(cov.with_candidates, 1);
        assert_eq!(cov.unaddressable, 0);
    }

    /// FR-004 / FR-005 — the operator wins.
    ///
    /// `supplement::merge` runs before this pass, so an unconditional
    /// reassignment would silently replace an operator's explicit CPE with a
    /// synthesized guess. Nothing on the pinned closure would reveal that.
    #[test]
    fn an_operator_supplied_cpe_is_never_recomputed() {
        let supplied = "cpe:2.3:a:openssl_project:openssl:3.6.2:*:*:*:*:*:*:*";
        let mut c = vec![component("openssl", "3.6.2")];
        c[0].cpes = vec![supplied.to_string()];

        fill_missing_cpes(&mut c);
        let cov = cpe_coverage(&c);

        assert_eq!(
            c[0].cpes,
            vec![supplied.to_string()],
            "the operator's vendor slug must survive; the curated table would \
             have said `openssl:openssl`"
        );
        assert_eq!(cov.with_candidates, 1);
    }

    /// FR-005 — no value appears twice.
    #[test]
    fn candidates_are_free_of_duplicates() {
        let mut c = vec![component("openssl", "3.6.2")];
        fill_missing_cpes(&mut c);
        let mut seen = std::collections::HashSet::new();
        for v in &c[0].cpes {
            assert!(seen.insert(v), "duplicate CPE {v} in {:?}", c[0].cpes);
        }
    }

    /// Milestone 1367's FR-008 — the counts tell three states apart.
    ///
    /// Milestone 1373 (#1375) replaced `eligible_unmapped`; the distinction
    /// this test exists for is unchanged, and `unaddressable` carries it.
    #[test]
    fn the_record_distinguishes_nothing_to_do_from_did_not_run() {
        let nothing = cpe_coverage(&[]);
        let mut unmapped = vec![component("waybill-fixture-unmapped", "1.0")];
        fill_missing_cpes(&mut unmapped);
        let did_nothing = cpe_coverage(&unmapped);

        assert_eq!(nothing.with_candidates, 0);
        assert_eq!(nothing.unaddressable, 0);
        assert_eq!(did_nothing.with_candidates, 0);
        assert_eq!(did_nothing.unaddressable, 1);
        assert_ne!(
            nothing.wire(),
            did_nothing.wire(),
            "these two documents must not be identical — that they WERE is \
             how this defect survived a milestone"
        );
        assert!(nothing.wire().contains("\"with_candidates\":0"));
        assert!(did_nothing.wire().contains("\"unaddressable\":1"));
    }

    // ---- milestone 1373 (#1375): feed addressability ----

    /// Build a component with an arbitrary PURL type, which `component`
    /// cannot: it hardcodes `pkg:generic`, and every case below turns on the
    /// type.
    fn typed(purl: &str, name: &str, version: &str) -> ResolvedComponent {
        let mut c = component(name, version);
        c.purl = waybill_common::types::purl::Purl::new(purl).unwrap();
        c
    }

    /// T008 — `WithCandidates` wins over addressability.
    ///
    /// 72 `pkg:pypi` components on the pinned closure are in exactly this
    /// position: they carry a CPE *and* belong to an OSV-indexed ecosystem.
    /// Counting them in two buckets breaks the partition.
    #[test]
    fn m1373_t008_a_cpe_bearing_addressable_component_is_counted_once() {
        let mut c = typed("pkg:pypi/urllib3@2.2.1", "urllib3", "2.2.1");
        c.cpes = vec!["cpe:2.3:a:urllib3:urllib3:2.2.1:*:*:*:*:*:*:*".into()];
        assert_eq!(
            classify_addressability(&c),
            FeedAddressability::WithCandidates
        );

        let mut v = vec![c];
        fill_missing_cpes(&mut v);
        let cov = cpe_coverage(&v);
        assert_eq!(cov.with_candidates, 1);
        assert_eq!(cov.addressable_without_cpe, 0);
        assert_eq!(cov.total(), 1, "counted twice or not at all");
    }

    /// T009 — the two populations the whole feature exists to separate.
    #[test]
    fn m1373_t009_generic_is_unaddressable_and_hackage_is_not() {
        assert_eq!(
            classify_addressability(&typed("pkg:generic/openssl@3.0.0", "openssl", "3.0.0")),
            FeedAddressability::Unaddressable,
            "pkg:generic names no registry: OSV returns nothing for openssl 3.0.0 \
             despite its CVEs"
        );
        assert_eq!(
            classify_addressability(&typed("pkg:hackage/aeson@2.2.4.1", "aeson", "2.2.4.1")),
            FeedAddressability::AddressableWithoutCpe,
            "OSV indexes Hackage and reaches this exact PURL"
        );
    }

    /// T010 (FR-008) — a versionless PURL is still addressable.
    ///
    /// The 24 such `pkg:hackage` components on the pinned closure are in no
    /// count at all before this milestone, because the v1 metric counted
    /// inside the synthesis loop and that loop skips them.
    #[test]
    fn m1373_t010_a_versionless_purl_is_still_addressable() {
        let c = typed("pkg:hackage/base", "base", "");
        assert_eq!(
            classify_addressability(&c),
            FeedAddressability::AddressableWithoutCpe
        );

        let mut v = vec![c];
        fill_missing_cpes(&mut v);
        let cov = cpe_coverage(&v);
        assert_eq!(
            cov.addressable_without_cpe, 1,
            "a component with no version must still land in exactly one bucket"
        );
        assert_eq!(cov.total(), 1);
    }

    /// T011 — an unknown PURL type falls to `Unaddressable`, not through a
    /// crack. `pkg:github` is the real instance: 1 component on the pinned
    /// closure.
    #[test]
    fn m1373_t011_an_unmapped_purl_type_is_unaddressable() {
        assert_eq!(
            classify_addressability(&typed(
                "pkg:github/kusari-oss/waybill@1.0.0",
                "waybill",
                "1.0.0"
            )),
            FeedAddressability::Unaddressable
        );
    }

    /// T012 (FR-003 half a, SC-005) — every row's ecosystem is one OSV
    /// publishes. This is the half that stops a row being added from memory.
    #[test]
    fn m1373_t012_every_row_is_in_osvs_declared_ecosystem_list() {
        let declared = include_str!(
            "../../../specs/1373-hackage-cpe-decision/measurements/osv-ecosystems-2026-11-07.txt"
        );
        let names: std::collections::HashSet<String> = declared
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                l.split(':').next().unwrap_or(l).trim().to_ascii_lowercase()
            })
            .collect();

        // CONTROL: the list parsed to something, or every assertion below
        // passes over an empty set.
        assert!(
            names.len() > 40,
            "the committed ecosystem list parsed to {} names; it should hold 47",
            names.len()
        );
        assert!(
            !names.contains("generic"),
            "OSV publishes no `generic` ecosystem; if it now does, FR-003's \
             reasoning needs re-measuring rather than this test relaxing"
        );

        for (purl_type, osv) in OSV_INDEXED_PURL_TYPES {
            assert!(
                names.contains(&osv.to_ascii_lowercase()),
                "`{purl_type}` claims OSV ecosystem `{osv}`, which is not in \
                 the committed list. Re-probe before adding a row."
            );
        }
    }

    /// T016 (FR-002, SC-002) — the three counts partition the component set.
    ///
    /// **Added because the mutation check found the invariant uncovered.**
    /// `fill_missing_cpes` carries a `debug_assert_eq!` for this, and
    /// deleting that assertion left the whole suite green: nothing else
    /// compared the sum to the component count. An assertion no test can
    /// distinguish from its own absence is a comment with punctuation.
    ///
    /// The population is deliberately mixed — one of each bucket, plus the
    /// two shapes the synthesis loop `continue`s past (an empty version and a
    /// non-release-shaped one), because those are exactly the components the
    /// v1 metric lost.
    #[test]
    fn m1373_t016_the_counts_partition_the_component_set() {
        let mut cpe_bearing = typed("pkg:pypi/urllib3@2.2.1", "urllib3", "2.2.1");
        cpe_bearing.cpes = vec!["cpe:2.3:a:urllib3:urllib3:2.2.1:*:*:*:*:*:*:*".into()];

        let mut v = vec![
            cpe_bearing,
            typed("pkg:hackage/aeson@2.2.4.1", "aeson", "2.2.4.1"),
            typed("pkg:hackage/base", "base", ""),
            typed("pkg:generic/openssl@3.0.0", "openssl", "3.0.0"),
            typed("pkg:generic/libcxx@0-unstable-2018-06-17", "libcxx", "0-unstable-2018-06-17"),
            typed("pkg:github/kusari-oss/waybill@1.0.0", "waybill", "1.0.0"),
        ];
        let n = v.len();
        fill_missing_cpes(&mut v);
        let cov = cpe_coverage(&v);

        assert_eq!(
            cov.total(),
            n,
            "the three counts must sum to the component count: {cov:?}"
        );

        // And the split itself, so a classifier that put everything in one
        // bucket would still fail even though the sum held.
        assert_eq!(cov.with_candidates, 2, "urllib3 plus synthesized openssl: {cov:?}");
        assert_eq!(cov.addressable_without_cpe, 2, "aeson and base: {cov:?}");
        assert_eq!(cov.unaddressable, 2, "libcxx and the github PURL: {cov:?}");
    }

    // ---- milestone 1374 (#1380): the two exclusions ----

    /// T007 (FR-003) — a member is excluded even when it carries a CPE.
    ///
    /// **No member carries one today** — 0 of 539 on darwin, 0 of 1,087 on
    /// linux — because the synthesizer refuses them. So this test has to
    /// construct the case: the guard it protects is someone else's, and if
    /// it changed, testing `cpes` before membership would let a member back
    /// into `with_candidates`.
    #[test]
    fn m1374_t007_a_member_is_excluded_even_carrying_a_cpe() {
        let mut c = typed("pkg:generic/bash@5.3", "bash", "5.3");
        c.cpes = vec!["cpe:2.3:a:gnu:bash:5.3:*:*:*:*:*:*:*".into()];
        c.extra_annotations.insert(
            crate::scan_fs::package_db::nix::closure::member::ANN_MEMBER_ROLE.to_string(),
            serde_json::json!("source"),
        );
        assert_eq!(
            classify_addressability(&c),
            FeedAddressability::ExcludedMember,
            "membership must win over a CPE, or the ordering is wrong"
        );
    }

    /// T008 — a member carrying an OSV-addressable PURL is still excluded.
    ///
    /// Real: `pkg:hackage/time` with role `fragment` on the linux closure.
    /// It is why `addressable_without_cpe` moves 163 → 162 there, and why
    /// the spec's original claim that only `unaddressable` could move was
    /// wrong.
    #[test]
    fn m1374_t008_a_member_with_an_addressable_purl_is_still_excluded() {
        let mut c = typed("pkg:hackage/time", "time", "");
        c.extra_annotations.insert(
            crate::scan_fs::package_db::nix::closure::member::ANN_MEMBER_ROLE.to_string(),
            serde_json::json!("fragment"),
        );
        assert_eq!(
            classify_addressability(&c),
            FeedAddressability::ExcludedMember
        );

        // CONTROL: without the member role the same component IS addressable,
        // so the assertion above is about membership and not about the PURL.
        let bare = typed("pkg:hackage/time", "time", "");
        assert_eq!(
            classify_addressability(&bare),
            FeedAddressability::AddressableWithoutCpe
        );
    }

    /// T009 — file-tier is recognised by its annotation, not by `type` or a
    /// missing PURL.
    ///
    /// `ResolvedComponent` has no `type` field, and a file-tier component
    /// carries `pkg:generic/file-tier?content-sha256=…` in memory — which
    /// the emitter drops, and which would otherwise classify `Unaddressable`
    /// because its type is `generic`. The control below proves that.
    #[test]
    fn m1374_t009_a_file_tier_component_is_excluded_by_its_annotation() {
        let purl = "pkg:generic/file-tier?content-sha256=\
                    e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let mut c = typed(purl, "utils.sh", "");
        c.extra_annotations.insert(
            crate::scan_fs::file_tier::COMPONENT_TIER_KEY.to_string(),
            serde_json::json!(crate::scan_fs::file_tier::COMPONENT_TIER_FILE_VALUE),
        );
        assert_eq!(
            classify_addressability(&c),
            FeedAddressability::ExcludedFile
        );

        // CONTROL: the identical PURL without the annotation is ordinary
        // `pkg:generic`, so this test turns on the annotation alone.
        let bare = typed(purl, "utils.sh", "");
        assert_eq!(
            classify_addressability(&bare),
            FeedAddressability::Unaddressable,
            "a file-tier PURL is otherwise indistinguishable from pkg:generic"
        );
    }

    /// T010 — ordinary components are untouched by the new arms.
    #[test]
    fn m1374_t010_ordinary_components_classify_as_before() {
        assert_eq!(
            classify_addressability(&typed("pkg:generic/openssl@3.0.0", "openssl", "3.0.0")),
            FeedAddressability::Unaddressable
        );
        assert_eq!(
            classify_addressability(&typed("pkg:hackage/aeson@2.2.4.1", "aeson", "2.2.4.1")),
            FeedAddressability::AddressableWithoutCpe
        );
        let mut withcpe = typed("pkg:pypi/urllib3@2.2.1", "urllib3", "2.2.1");
        withcpe.cpes = vec!["cpe:2.3:a:urllib3:urllib3:2.2.1:*:*:*:*:*:*:*".into()];
        assert_eq!(
            classify_addressability(&withcpe),
            FeedAddressability::WithCandidates
        );
    }

    /// T014 (FR-002, SC-002) — the five counts partition the component set.
    #[test]
    fn m1374_t014_the_five_counts_partition_the_component_set() {
        let mut cpe_bearing = typed("pkg:pypi/urllib3@2.2.1", "urllib3", "2.2.1");
        cpe_bearing.cpes = vec!["cpe:2.3:a:urllib3:urllib3:2.2.1:*:*:*:*:*:*:*".into()];
        let mut member = typed("pkg:generic/aeson-2.2.4.1.tar.gz", "aeson-2.2.4.1.tar.gz", "");
        member.extra_annotations.insert(
            crate::scan_fs::package_db::nix::closure::member::ANN_MEMBER_ROLE.to_string(),
            serde_json::json!("source"),
        );
        let mut file = typed("pkg:generic/file-tier?content-sha256=abc", "utils.sh", "");
        file.extra_annotations.insert(
            crate::scan_fs::file_tier::COMPONENT_TIER_KEY.to_string(),
            serde_json::json!(crate::scan_fs::file_tier::COMPONENT_TIER_FILE_VALUE),
        );

        let v = vec![
            cpe_bearing,
            typed("pkg:hackage/aeson@2.2.4.1", "aeson", "2.2.4.1"),
            typed("pkg:generic/openssl@3.0.0", "openssl", "3.0.0"),
            member,
            file,
        ];
        let cov = cpe_coverage(&v);

        assert_eq!(cov.total(), v.len(), "the five must partition: {cov:?}");
        assert_eq!(cov.with_candidates, 1, "{cov:?}");
        assert_eq!(cov.addressable_without_cpe, 1, "{cov:?}");
        assert_eq!(cov.unaddressable, 1, "{cov:?}");
        assert_eq!(cov.excluded_members, 1, "{cov:?}");
        assert_eq!(cov.excluded_files, 1, "{cov:?}");
    }

    /// T012b (F4) — `partition.py` holds the same table and is the
    /// instrument for SC-001. If the two drift, the check and the code it
    /// checks agree by construction and the check is worthless.
    #[test]
    fn m1373_t012b_the_probe_and_the_code_hold_the_same_table() {
        let probe = include_str!(
            "../../../specs/1373-hackage-cpe-decision/measurements/partition.py"
        );
        let table = probe
            .split("PURL_TO_OSV = {")
            .nth(1)
            .expect("partition.py must define PURL_TO_OSV")
            .split('}')
            .next()
            .expect("unterminated PURL_TO_OSV");

        let rows: std::collections::HashSet<String> = table
            .split(',')
            .filter_map(|entry| {
                let mut parts = entry.split(':');
                let k = parts.next()?.trim().trim_matches('"').to_string();
                let v = parts.next()?.trim().trim_matches('"').to_string();
                if k.is_empty() || v.is_empty() {
                    None
                } else {
                    Some(format!("{k}={}", v.to_ascii_lowercase()))
                }
            })
            .collect();

        let ours: std::collections::HashSet<String> = OSV_INDEXED_PURL_TYPES
            .iter()
            .map(|(t, e)| format!("{t}={}", e.to_ascii_lowercase()))
            .collect();

        assert_eq!(
            rows, ours,
            "partition.py and OSV_INDEXED_PURL_TYPES have drifted; SC-001's \
             check would then agree with the code by construction"
        );
    }

    fn component(name: &str, version: &str) -> ResolvedComponent {
        use waybill_common::resolution::{ResolutionEvidence, ResolutionTechnique};
        use waybill_common::types::purl::Purl;
        let coordinate = if version.is_empty() {
            format!("pkg:generic/{name}")
        } else {
            format!("pkg:generic/{name}@{version}")
        };
        ResolvedComponent {
            purl: Purl::new(&coordinate).unwrap(),
            name: name.to_string(),
            version: version.to_string(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 0.95,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: vec![],
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            build_inclusion: None,
            artifact_presence: None,
            scope_evidence: None,
            requirement_ranges: vec![],
            source_type: None,
            sbom_tier: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: vec![],
            extra_annotations: Default::default(),
            binary_role: None,
        }
    }
}

/// Milestone 1378 (#1396) — the wiring around the contents pass: reading,
/// asking the registry, and promoting only what it confirms.
///
/// The pass itself is unit-tested in `closure/contents.rs` and the emitted
/// documents in `tests/closure_binary_contents.rs`, which runs with
/// `--no-deps-dev`. Neither reaches the confirming path, so it has a stub of
/// its own here, for the same reason the m1366 pass does.
#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod closure_contents_tests {
    use super::*;
    use crate::enrich::deps_dev_client::DepsDevClient;
    use crate::enrich::depsdev_source::DepsDevSource;
    use scan_fs::binary::embedded::fixtures;
    use scan_fs::package_db::nix::closure::contents::{self, ReadState, RootState};
    use scan_fs::package_db::nix::closure::{classify::classify, derivation::RawClosure};
    use waybill_common::resolution::ResolvedComponent;

    const APP: &str = "h0-waybill-fixture-app-1.2.3";
    const TOOL: &str = "h2-waybill-fixture-tool-3.0.0";

    fn merged() -> (
        scan_fs::package_db::nix::closure::ClassifiedClosure,
        Vec<ResolvedComponent>,
    ) {
        let json = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/nix_closure_contents/app.json"),
        )
        .unwrap();
        let raw = RawClosure::parse(&json).unwrap();
        let roles = classify(&raw);
        let graph = scan_fs::package_db::nix::closure::graph::project(
            &raw,
            &scan_fs::package_db::nix::closure::emit::emitted_drv_keys(&raw, &roles),
        );
        let classified = scan_fs::package_db::nix::closure::ClassifiedClosure {
            runtime: None,
            attribute: "default".into(),
            raw,
            roles,
            graph_admitted: Default::default(),
            graph_admission: Default::default(),
            read_outcome: Default::default(),
            graph,
            identifiers: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
        };
        let added = scan_fs::package_db::nix::closure::emit::components(&classified);
        let mut components = Vec::new();
        merge_closure_components(&mut components, added);
        (classified, components)
    }

    /// The root holds a Go program, the build tool a Rust one.
    fn store() -> tempfile::TempDir {
        let store = tempfile::tempdir().unwrap();
        for (output, file, bytes) in [
            (
                APP,
                "app",
                fixtures::go_binary(
                    "example.test/waybill-fixture-app",
                    "(devel)",
                    &[("example.test/waybill-fixture-goa", "v1.0.0")],
                ),
            ),
            (
                TOOL,
                "tool",
                fixtures::cargo_binary(&[
                    ("waybill-fixture-tool", "3.0.0", "local", "runtime"),
                    ("waybill-fixture-cratea", "0.5.0", "crates.io", "runtime"),
                ]),
            ),
        ] {
            let dir = store.path().join(output).join("bin");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(file), bytes).unwrap();
        }
        store
    }

    fn by_name<'a>(c: &'a [ResolvedComponent], name: &str) -> &'a ResolvedComponent {
        let found: Vec<_> = c.iter().filter(|x| x.name == name).collect();
        assert_eq!(found.len(), 1, "exactly one component named {name}");
        found[0]
    }

    fn identity(c: &ResolvedComponent) -> serde_json::Value {
        serde_json::from_str(
            c.extra_annotations
                .get(contents::ANN_BINARY_IDENTITY)
                .and_then(|v| v.as_str())
                .expect("C215"),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn with_confirmation_disabled_modules_arrive_and_no_identifier_moves() {
        let (classified, mut components) = merged();
        let store = store();
        let run = run_closure_contents(&classified, &mut components, store.path(), None).await;

        // CONTROL: the pass did something.
        assert_eq!(run.relationships.len(), 3, "goa, stdlib, cratea");
        assert_eq!(run.summary.root, RootState::Member(ReadState::Read));
        assert_eq!(run.summary.root_label.as_deref(), Some("waybill-fixture-app 1.2.3"));
        assert!(run.summary.unread, "the library's outputs are absent");

        let app = by_name(&components, "waybill-fixture-app");
        assert_eq!(app.purl.as_str(), "pkg:generic/waybill-fixture-app@1.2.3");
        assert_eq!(identity(app)["status"], "unchecked");
        assert_eq!(identity(app)["reason"], "disabled");
        assert!(run
            .relationships
            .iter()
            .any(|r| r.from == "pkg:generic/waybill-fixture-app@1.2.3"));
    }

    /// FR-012a / FR-012b, both arms in one run: the registry knows the Go
    /// program at the closure's version and does not know the Rust one.
    #[tokio::test]
    async fn a_confirmed_identity_is_promoted_and_a_declined_one_is_not() {
        let server = wiremock::MockServer::start().await;
        // The Go key carries the `v` the closure's version lacks. If the
        // pass asked for `1.2.3` this mock would not match and the member
        // would be declined — which is what the real registry does (measured:
        // 404 for the un-prefixed form of a real version).
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path_regex(
                r"^/systems/go/packages/.*waybill-fixture-app/versions/v1\.2\.3$",
            ))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "versionKey": {"system": "GO", "name": "example.test/waybill-fixture-app", "version": "v1.2.3"},
                "licenses": [],
                "links": []
            })))
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(
                "/systems/cargo/packages/waybill-fixture-tool/versions/3.0.0",
            ))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let source = DepsDevSource::new(
            DepsDevClient::new(std::time::Duration::from_secs(5)).with_base_url(server.uri()),
            false,
        )
        .with_batch(false)
        .with_disk_cache(false, None);

        let (classified, mut components) = merged();
        let store = store();
        let run =
            run_closure_contents(&classified, &mut components, store.path(), Some(&source)).await;

        let app = by_name(&components, "waybill-fixture-app");
        assert_eq!(
            app.purl.as_str(),
            "pkg:golang/example.test/waybill-fixture-app@v1.2.3",
            "confirmed, so promoted"
        );
        assert_eq!(app.version, "1.2.3", "the closure's version is kept");
        assert_eq!(identity(app)["status"], "confirmed");
        assert_eq!(identity(app)["candidate"], app.purl.as_str());

        let tool = by_name(&components, "waybill-fixture-tool");
        assert_eq!(
            tool.purl.as_str(),
            "pkg:generic/waybill-fixture-tool@3.0.0",
            "a 404 is an answer, and the answer is no"
        );
        assert_eq!(identity(tool)["status"], "declined");
        assert_eq!(identity(tool)["candidate"], "pkg:cargo/waybill-fixture-tool@3.0.0");

        // The contents edges follow the promoted identifier...
        assert!(run.relationships.iter().any(|r| r.from
            == "pkg:golang/example.test/waybill-fixture-app@v1.2.3"
            && r.to == "pkg:golang/example.test/waybill-fixture-goa@v1.0.0"));
        // ...and so do the closure's own edges, which is why the pass runs
        // before they are joined.
        let (edges, unanchored) = scan_fs::package_db::nix::closure::graph::relationships(
            &classified.graph,
            &classified.raw,
            &components,
        );
        assert_eq!(unanchored, 0);
        assert_eq!(
            edges
                .iter()
                .filter(|e| e.from == "pkg:golang/example.test/waybill-fixture-app@v1.2.3")
                .count(),
            2,
            "root → lib and root → tool, from the promoted identifier"
        );
        // FR-011 — still one component for the program.
        assert_eq!(
            components
                .iter()
                .filter(|c| c.purl.as_str().contains("waybill-fixture-app"))
                .count(),
            1
        );
    }

    // ---- the strict verdict ---------------------------------------------

    fn summary(root: RootState) -> ClosureContentsSummary {
        ClosureContentsSummary {
            wire: String::new(),
            unread: true,
            root,
            root_label: Some("waybill-fixture-app 1.2.3".to_string()),
        }
    }

    #[test]
    fn without_the_flag_an_unread_root_never_fails() {
        for root in [
            RootState::Member(ReadState::Absent),
            RootState::Member(ReadState::Failed),
            RootState::Member(ReadState::NoOutputs),
            RootState::None,
        ] {
            assert_eq!(
                closure_root_contents_verdict(true, false, Some(Ok(())), Some(&summary(root))),
                Ok(()),
                "{root:?}"
            );
        }
        assert_eq!(
            closure_root_contents_verdict(true, false, Some(Err("nix-missing".into())), None),
            Ok(())
        );
        assert_eq!(closure_root_contents_verdict(false, false, None, None), Ok(()));
    }

    #[test]
    fn strict_fails_exactly_when_the_root_was_not_read() {
        let verdict = |root| {
            closure_root_contents_verdict(true, true, Some(Ok(())), Some(&summary(root)))
        };
        assert!(verdict(RootState::Member(ReadState::Read)).is_ok());
        assert!(
            verdict(RootState::Member(ReadState::Partial)).is_ok(),
            "partly read passes: which output holds the program is not knowable"
        );
        for root in [
            RootState::Member(ReadState::Absent),
            RootState::Member(ReadState::Failed),
            RootState::Member(ReadState::NoOutputs),
            RootState::None,
        ] {
            let err = verdict(root).expect_err("must fail");
            assert!(err.contains("--nix-closure-require-root-contents"), "{err}");
        }
        let err = verdict(RootState::Member(ReadState::Absent)).unwrap_err();
        assert!(err.contains("waybill-fixture-app 1.2.3") && err.contains("absent"), "{err}");

        // A degraded tier has no root to read.
        let err = closure_root_contents_verdict(true, true, Some(Err("nix-missing".into())), None)
            .unwrap_err();
        assert!(err.contains("nix-missing"), "{err}");
    }
}
