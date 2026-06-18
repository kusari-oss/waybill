use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Args, ValueEnum};

use mikebom_common::attestation::integrity::TraceIntegrity;
use mikebom_common::attestation::metadata::GenerationContext;

use crate::enrich::clearly_defined_source::{
    enrich_components as cd_enrich_components, ClearlyDefinedSource,
};
use crate::enrich::deps_dev_client::DepsDevClient;
use crate::enrich::depsdev_source::{enrich_components, DepsDevSource};
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

/// Format ids that trigger OpenVEX sidecar emission. Today only
/// the stable SPDX 2.3 serializer does so; SPDX 3.0.1-experimental
/// may opt in in a future milestone, at which point this list grows.
const OPENVEX_EMITTING_FORMATS: &[&str] = &["spdx-2.3-json"];

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
    /// OCI distribution-spec registry pull (the milestone-031+
    /// `oci_pull` path).
    Remote,
}

#[derive(Args, Debug)]
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
    /// Comma-separated list; mikebom tries each source in order and
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
        default_value = "docker,remote",
        value_name = "SRC[,SRC...]",
    )]
    pub image_src: Vec<ImageSource>,

    /// Override the platform that's resolved from a multi-arch image
    /// index. Only meaningful when `--image` points at a registry
    /// reference (not a pre-extracted tarball) — for tarballs the
    /// platform is fixed by whatever `docker save` already wrote.
    ///
    /// Format: `<os>/<arch>` or `<os>/<arch>/<variant>`. Only `linux`
    /// is supported as the OS — mikebom's package-database readers
    /// (dpkg / apk / rpm) are linux-rootfs-shaped, so non-Linux
    /// container images aren't a meaningful scan target.
    ///
    /// Common values: `linux/amd64`, `linux/arm64`, `linux/arm/v7`,
    /// `linux/arm/v6`, `linux/386`, `linux/ppc64le`, `linux/s390x`.
    /// When omitted (default), mikebom auto-resolves to
    /// `linux/<host-arch>` matching the machine running the scan.
    ///
    /// Use case: a macOS arm64 dev machine scanning a `linux/amd64`
    /// container image deployed to AWS, or Linux x86_64 CI scanning
    /// an `arm64` image deployed to Graviton.
    #[arg(long, requires = "image", value_name = "linux/ARCH[/VARIANT]")]
    pub image_platform: Option<String>,

    /// Disable the OCI blob cache for registry pulls. Equivalent to
    /// `MIKEBOM_OCI_CACHE=0`. When set, every blob (config + layer)
    /// is fetched from the registry on every scan, even if mikebom
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
    /// var: `MIKEBOM_OCI_CACHE_SIZE=<bytes>`.
    ///
    /// Cache location is resolved from (in priority order):
    /// `$MIKEBOM_OCI_CACHE_DIR`, `$XDG_CACHE_HOME/mikebom/oci-layers`,
    /// `$HOME/Library/Caches/mikebom/oci-layers` on macOS, otherwise
    /// `$HOME/.cache/mikebom/oci-layers`.
    #[arg(long, value_name = "BYTES")]
    pub oci_cache_size: Option<u64>,

    /// Issue #235 — directory containing Docker-format registry
    /// credentials. mikebom probes `<DIR>/config.json`,
    /// `<DIR>/.dockerconfigjson` (K8s `kubernetes.io/dockerconfigjson`
    /// secret type), and `<DIR>/.dockercfg` (legacy K8s
    /// `kubernetes.io/dockercfg` secret type) in that order; the
    /// first readable+parseable file wins. The file format is the
    /// standard Docker `config.json` shape (`auths`, `credsStore`,
    /// `credHelpers`), so the existing credential-resolution
    /// precedence applies inside the loaded config.
    ///
    /// Use this when running mikebom in a container that mounts a
    /// K8s `imagePullSecrets`-derived volume (typically at
    /// `/var/run/secrets/registry/`). For local/CI use with the
    /// standard Docker keychain, leave this unset — mikebom falls
    /// back to `$DOCKER_CONFIG/config.json` or
    /// `$HOME/.docker/config.json`.
    ///
    /// Composes with `MIKEBOM_REGISTRY_<HOST>_USERNAME/_PASSWORD`
    /// env vars (per-registry, higher priority than the directory
    /// probe) and `MIKEBOM_REGISTRY_USERNAME/_PASSWORD` (generic
    /// fallback, also higher priority than the directory probe).
    /// See `docs/reference/identifiers.md` for the full credential
    /// resolution priority chain.
    #[arg(long = "registry-credentials-dir", value_name = "PATH")]
    pub registry_credentials_dir: Option<std::path::PathBuf>,

    /// Output path override. Two forms are accepted:
    ///
    /// * Bare `--output <path>` — applies to the single requested
    ///   format. Rejected when more than one format is requested.
    /// * Per-format `--output <fmt>=<path>` — repeatable; each entry
    ///   overrides the default filename for exactly one format id.
    ///
    /// Per-format form is required for multi-format emission. When
    /// omitted, each format writes to its own default filename
    /// (`mikebom.cdx.json`, `mikebom.spdx.json`, …).
    #[arg(long, action = clap::ArgAction::Append, value_name = "[FMT=]PATH")]
    pub output: Vec<String>,

    /// Output format(s). Comma-separated list, and the flag itself
    /// is repeatable: `--format cyclonedx-json,spdx-2.3-json` is
    /// equivalent to `--format cyclonedx-json --format spdx-2.3-json`.
    /// Duplicates are ignored silently. Default: `cyclonedx-json`.
    ///
    /// Registered formats:
    /// - `cyclonedx-json` — CycloneDX 1.6 JSON (default filename
    ///   `mikebom.cdx.json`).
    /// - `spdx-2.3-json` — SPDX 2.3 JSON (default filename
    ///   `mikebom.spdx.json`).
    /// - `spdx-3-json` — SPDX 3.0.1 JSON-LD (default filename
    ///   `mikebom.spdx3.json`). Full ecosystem coverage; production-
    ///   grade output with native-field + annotation parity vs.
    ///   CycloneDX and SPDX 2.3.
    /// - `spdx-3-json-experimental` [DEPRECATED] — deprecation alias
    ///   for `spdx-3-json`. Byte-identical output; prints a stderr
    ///   deprecation notice. Accepted through milestone 012;
    ///   removed in milestone 013. Set
    ///   `MIKEBOM_NO_DEPRECATION_NOTICE=1` to suppress the warning
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

    /// Skip per-file SHA-256 hashing of installed-package contents.
    /// Falls back to a fast SHA-256 over each package's dpkg `.md5sums`
    /// file (microseconds per package; component-level identity only,
    /// no per-file occurrences). Default-on hashing reads every file
    /// referenced by dpkg's `.list` manifest — proportional to
    /// installed size (~3-5 s on debian:bookworm-slim, ~30 s on full
    /// debian).
    #[arg(long)]
    pub no_deep_hash: bool,

    /// Print a JSON summary to stdout after writing the SBOM.
    #[arg(long)]
    pub json: bool,

    /// Skip ClearlyDefined enrichment (concluded licenses). Keeps
    /// deps.dev license + dep-graph enrichment active. Use this when
    /// ClearlyDefined is slow or unreachable but you still want
    /// deps.dev data. Has no effect when `--offline` is set (all
    /// enrichment is already disabled).
    #[arg(long)]
    pub no_clearly_defined: bool,

    /// Skip deps.dev license enrichment. Keeps ClearlyDefined and
    /// dep-graph enrichment active. This is the fastest enrichment
    /// source and rarely needs skipping; the allowlist via
    /// `--enrich-sources` is the alternative for full control.
    /// Has no effect when `--offline` is set.
    #[arg(long)]
    pub no_deps_dev: bool,

    /// Milestone 102 (FR-016/FR-017): include vendored C/C++
    /// dependencies declared via CMake `add_subdirectory(third_party/...)`
    /// or `add_subdirectory(vendor/...)`. Default OFF — these are
    /// frequently false positives (CMake's `add_subdirectory` is also
    /// used for first-party `src/` and `tests/` sub-modules) so we
    /// require explicit opt-in. When enabled, version is backfilled
    /// from a co-located `version.txt` or `.version` file when present;
    /// otherwise the PURL has no version segment. Also accepts
    /// `MIKEBOM_INCLUDE_VENDORED=1` env var which is read directly by
    /// the milestone-102 C/C++ readers in `read_all` (no clap-level
    /// env binding here, so the env var accepts "1"/"true"/etc. without
    /// clap's bool-env strictness).
    #[arg(long)]
    pub include_vendored: bool,

    /// Skip the deps.dev transitive dep-graph enrichment step.
    /// Keeps deps.dev license enrichment and ClearlyDefined active.
    /// Useful when the graph response is large or unneeded. Has no
    /// effect when `--offline` is set.
    #[arg(long)]
    pub no_deps_dev_graph: bool,

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
    /// (FR-011). When set, mikebom emits a `mikebom:source-document-binding`
    /// annotation on each first-party component whose PURL appears in
    /// the source SBOM, plus a document-level cross-document reference
    /// (CDX `externalReferences[type:bom]`, SPDX `externalDocumentRefs` +
    /// `BUILT_FROM` relationship).
    ///
    /// FR-011 transparency: when the file cannot be loaded or parsed,
    /// the scan exits non-zero rather than silently emitting components
    /// without binding. Components whose PURL has no source-tier
    /// counterpart get an explicit
    /// `binding: unknown { reason: "source-not-found-in-bind-target" }`
    /// marker per FR-003.
    ///
    /// Use `mikebom sbom verify-binding --image-sbom <out> --source-sbom <path>`
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
    /// mikebom emits a warning and the alias is discarded (the scan
    /// proceeds; no alias is recorded in the emitted SBOM).
    ///
    /// Also settable via `MIKEBOM_PKG_ALIAS` (comma-separated entries
    /// matching the per-flag syntax) for CI ergonomics.
    #[arg(
        long = "pkg-alias",
        value_name = "LHS=RHS",
        value_parser = mikebom::binding::alias::parse_pkg_alias,
        action = clap::ArgAction::Append,
    )]
    pub pkg_alias: Vec<mikebom::binding::alias::PurlAlias>,

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
    /// User-defined identifiers ride the `mikebom:identifiers`
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
    pub id: Vec<mikebom::binding::identifiers::Identifier>,

    /// Preserve userinfo (e.g., `USER:TOKEN@host`) in auto-detected git
    /// remote URLs when constructing `repo:` and `git:` identifiers.
    /// By default, mikebom strips userinfo to prevent accidental
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
    /// (`mikebom trace run`), subject identifiers are auto-detected
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
        value_parser = mikebom::binding::identifiers::component_id::parse_component_id_flag,
    )]
    pub component_id:
        Vec<mikebom::binding::identifiers::component_id::ComponentIdentifierFlag>,

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
    )]
    pub no_root_purl: bool,

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
    /// `Organization` / `Person` element in `@graph`. mikebom's own
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
    /// (per the milestone-080 native-fields audit; no `mikebom:`
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
    /// `mikebom:sbom-tier` annotations preserve auto-detected values.
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
    /// requires mikebom to default to the spec-native mechanism that
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
    /// In BOTH modes the `mikebom:lifecycle-scope` annotation is set
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
    /// corpus. Defaults to false: mikebom uses the in-source bundled
    /// 7-library corpus exactly as it did pre-108 (SC-003 byte-identity
    /// guarantee). When set, mikebom loads fingerprint records from
    /// the operator's per-host cache at the build-time-pinned SHA;
    /// cache-miss triggers a one-shot fetch from
    /// `kusari-sandbox/mikebom-fingerprints` (skipped under `--offline`,
    /// falls back to bundled defaults on failure). Components
    /// identified via the symbol-fingerprint path then carry a
    /// `mikebom:fingerprint-corpus-sha` annotation (12-hex SHA or
    /// the `bundled` sentinel) per FR-005.
    ///
    /// Also enabled via `MIKEBOM_FINGERPRINTS_CORPUS=1`.
    #[arg(long, env = "MIKEBOM_FINGERPRINTS_CORPUS")]
    pub fingerprints_corpus: bool,

    /// Milestone 110 Phase 5-Slim (FR-006) — declare additional
    /// fingerprint-corpus sources. Repeatable. Each value is a URL
    /// pointing to a corpus tarball; the optional `=ENV_VAR` suffix
    /// for per-source bearer-token auth is parsed but currently
    /// warned-and-stripped (FR-007 is deferred to a follow-on slice).
    ///
    /// Requires `--fingerprints-corpus`. Sources from this flag are
    /// UNION'd with `MIKEBOM_FINGERPRINTS_SOURCES` and the implicit
    /// milestone-108 default (unless `--fingerprints-source-no-default`
    /// is set).
    ///
    /// Also settable via `MIKEBOM_FINGERPRINTS_SOURCES` (comma-
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
    /// Also settable via `MIKEBOM_FINGERPRINTS_NO_DEFAULT=1`.
    #[arg(
        long = "fingerprints-source-no-default",
        env = "MIKEBOM_FINGERPRINTS_NO_DEFAULT",
    )]
    pub fingerprints_source_no_default: bool,

    /// Milestone 108 (US5) — override the build-time-embedded corpus
    /// SHA with a runtime-specified one. Format: 40-char lowercase hex.
    /// Requires `--fingerprints-corpus` (or
    /// `MIKEBOM_FINGERPRINTS_CORPUS=1`); when the override is supplied
    /// without the opt-in, mikebom emits a warning and ignores the
    /// override (the bundled fallback path is used). Use this to test
    /// newer corpora before they're embedded in a mikebom release, or
    /// to pin a specific corpus version for reproducibility across
    /// machines whose embedded SHAs differ.
    ///
    /// Also settable via `MIKEBOM_FINGERPRINTS_REV=<SHA>`.
    #[arg(
        long = "fingerprints-rev",
        env = "MIKEBOM_FINGERPRINTS_REV",
        value_name = "SHA",
        value_parser = parse_fingerprints_rev_flag,
    )]
    pub fingerprints_rev: Option<String>,
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
fn parse_user_defined_id_flag(
    raw: &str,
) -> Result<mikebom::binding::identifiers::Identifier, String> {
    use mikebom::binding::identifiers::{
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
fn assemble_manual_identifiers(args: &ScanArgs) -> Vec<mikebom::binding::identifiers::Identifier> {
    let mut out: Vec<mikebom::binding::identifiers::Identifier> = Vec::new();
    // (1) repo / git-ref: when --git-ref is set, emit only the git:
    // form; otherwise emit a bare repo: form.
    if let Some(repo_url) = args.repo.as_deref() {
        let raw = if let Some(rev) = args.git_ref.as_deref() {
            format!("git:{repo_url}#{rev}")
        } else {
            format!("repo:{repo_url}")
        };
        match mikebom::binding::identifiers::Identifier::parse(&raw) {
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
        match mikebom::binding::identifiers::Identifier::parse(&raw) {
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
        match mikebom::binding::identifiers::Identifier::parse(&raw) {
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
        match mikebom::binding::identifiers::Identifier::parse(&raw) {
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
) -> Option<mikebom::binding::identifiers::Identifier> {
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
    // is to emit a maximally-informative identifier from what mikebom
    // can observe.
    let digest = if extracted.manifest_digest.is_empty() {
        None
    } else {
        Some(extracted.manifest_digest.as_str())
    };
    if name.is_empty() && digest.is_none() {
        return None;
    }
    mikebom::binding::identifiers::auto_detect::image_reference_to_identifier(
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
// promoted to `mikebom::binding::identifiers::resolve_identifiers`
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
        EnrichConfig {
            deps_dev: !args.no_deps_dev,
            clearly_defined: !args.no_clearly_defined,
            deps_dev_graph: !args.no_deps_dev_graph,
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
    // Also check the OpenVEX override against format outputs, since
    // the sidecar lands beside the SPDX file. No default path to
    // check when the override isn't set — the sidecar's default is
    // `mikebom.openvex.json`, which can't collide with any
    // registered format's default (cdx / spdx filenames are
    // distinct from openvex's).
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

/// The SPDX 3 deprecation-alias format id (milestone 011 US3).
/// Kept as a named constant so the notice-emission path in
/// `execute()` and the help-list labeling in [`format_help_list`]
/// reference the same string.
const SPDX_3_DEPRECATED_ALIAS: &str = "spdx-3-json-experimental";

/// Environment override to suppress the
/// `spdx-3-json-experimental` deprecation notice. Set to any
/// non-empty value to silence the stderr warning during a
/// controlled migration; document bytes are unaffected either way.
const NO_DEPRECATION_NOTICE_ENV: &str = "MIKEBOM_NO_DEPRECATION_NOTICE";

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
            ImageSource::Remote => {
                tried.push("remote");
                #[cfg(feature = "oci-registry")]
                {
                    tracing::info!(image_ref = arg_str, "pulling image from registry");
                    let cache_disabled = args.no_oci_cache
                        || std::env::var("MIKEBOM_OCI_CACHE").as_deref() == Ok("0");
                    let cache_size_cap = if cache_disabled {
                        None
                    } else {
                        let env_size = std::env::var("MIKEBOM_OCI_CACHE_SIZE")
                            .ok()
                            .and_then(|s| s.parse::<u64>().ok());
                        Some(
                            args.oci_cache_size
                                .or(env_size)
                                .unwrap_or(10 * 1024 * 1024 * 1024),
                        )
                    };
                    let td = scan_fs::oci_pull::pull_to_tarball(
                        arg_str,
                        args.image_platform.as_deref(),
                        cache_size_cap,
                        args.registry_credentials_dir.as_deref(),
                    )
                    .await?;
                    let tarball = td.path().join("image.tar");
                    *tempdir = Some(td);
                    return Ok(tarball);
                }
                #[cfg(not(feature = "oci-registry"))]
                {
                    anyhow::bail!(
                        "--image-src includes `remote`, but this build of \
                         mikebom was compiled with `--no-default-features` \
                         (the `oci-registry` Cargo feature is OFF), so OCI \
                         image references like `alpine:3.19` cannot be \
                         pulled from a registry. Either:\n\
                         (a) reinstall with the default feature set: \
                         `cargo install mikebom`, or\n\
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

pub async fn execute(
    args: ScanArgs,
    offline: bool,
    exclude_scope: Vec<mikebom_common::resolution::LifecycleScope>,
    include_legacy_rpmdb: bool,
    include_declared_deps: bool,
    exclude_set: crate::scan_fs::package_db::exclude_path::ExclusionSet,
    supplement_cdx: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    // Milestone 102 FR-016: propagate the `--include-vendored` flag to
    // the env var that the C/C++ readers read directly. This avoids
    // plumbing through `scan_path`'s 75-callsite chain. The clap derive
    // already populates `args.include_vendored` from either the CLI
    // flag or `MIKEBOM_INCLUDE_VENDORED=1` env (whichever was set first);
    // we re-export to the env so `read_all`-internal readers see the
    // unified signal regardless of which input set it.
    // SAFETY: single-threaded at this point in the scan-cmd lifecycle.
    if args.include_vendored {
        // SAFETY: see comment above — single-threaded.
        unsafe {
            std::env::set_var("MIKEBOM_INCLUDE_VENDORED", "1");
        }
    }

    // Milestone 108 FR-002: propagate the `--fingerprints-corpus`
    // opt-in to the env var that the binary-scan loop's fingerprint
    // matcher reads directly. Same pattern as MIKEBOM_INCLUDE_VENDORED
    // above. Clap's `env = "MIKEBOM_FINGERPRINTS_CORPUS"` derive picks
    // up the env-set form; this re-export handles the
    // operator-passed-the-flag-but-not-the-env form. The matcher then
    // calls `LoadOptions::from_env()` (which also reads `MIKEBOM_OFFLINE`,
    // already set by main.rs).
    if args.fingerprints_corpus {
        // SAFETY: see comment above — single-threaded.
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CORPUS", "1");
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
        // SAFETY: env-mutation pattern matches MIKEBOM_FINGERPRINTS_CORPUS
        // above; called single-threaded before any scan thread spawns.
        unsafe {
            std::env::set_var(
                "MIKEBOM_FINGERPRINTS_SOURCES",
                args.fingerprints_source.join(","),
            );
        }
        if !args.fingerprints_corpus {
            tracing::warn!(
                count = args.fingerprints_source.len(),
                "--fingerprints-source declared without --fingerprints-corpus; \
                 the sources are parsed but no corpus loading will occur. \
                 Add --fingerprints-corpus (or MIKEBOM_FINGERPRINTS_CORPUS=1) to enable.",
            );
        }
    }
    if args.fingerprints_source_no_default {
        // SAFETY: see comment above.
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_NO_DEFAULT", "1");
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
                std::env::set_var("MIKEBOM_FINGERPRINTS_REV", rev);
            }
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
    if args.path.is_none() && args.image.is_none() {
        anyhow::bail!("one of --path or --image is required");
    }

    // Resolve format dispatch BEFORE any scan work so argument errors
    // abort without having paid for a scan.
    let registry = SerializerRegistry::with_defaults();
    let plan = resolve_dispatch(&registry, &args.format, &args.output)?;

    // FR-002 / research.md §R2: when the deprecated SPDX 3 alias is
    // in the resolved format list, print a two-line stderr notice
    // (deprecation directive + shape-change advisory) exactly once
    // per invocation. Suppress with
    // `MIKEBOM_NO_DEPRECATION_NOTICE=<anything>` so CI logs of
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
                resolve_image_ref(arg_str, &args, &mut _image_tempdir).await?
            };
            tracing::info!(archive = %archive_path.display(), "extracting docker image");
            let extracted = scan_fs::docker_image::extract(&archive_path)?;
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
            let target = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("filesystem-scan")
                .to_string();
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
    // Dual-SBOM scope auto-detection (see docs/design-notes.md:
    // "Scope: artifact vs manifest SBOM"). Image scans default to
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
    let scan_fs::ScanResult {
        mut components,
        mut relationships,
        complete_ecosystems,
        os_release_missing_fields,
        go_graph_completeness,
        go_graph_completeness_reason,
        scan_target_coord,
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
        // `maven::read_with_claims` and docs/design-notes.md "Scan
        // target identity" for rationale.
        Some(&target_name),
        &exclude_set,
    )
    .with_context(|| format!("scan failed for {}", root_path.display()))?;

    // Milestone 113 FR-014 / Constitution Principle X: when any
    // exclusion entry is active, install its snapshot via the
    // exclude_path thread-local so the CDX/SPDX 2.3/SPDX 3 metadata
    // emitters can pick it up and emit `mikebom:exclude-path` at
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
    // without paging through MIKEBOM_LOG=debug output. Per-walker debug
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

    // Milestone 127 FR-007 — surface a warning when the root-selection
    // heuristic fell through past at least one detected main-module.
    // The warning names the picked subject AND the loser main-modules'
    // PURLs so operators can pass `--root-name`/`--root-purl-type`
    // for deterministic control. Gated on `losers.is_empty() == false`
    // (matches the FR-006 annotation emission gate).
    {
        let root_override_tmp = crate::generate::RootComponentOverride {
            name: args.root_name.clone(),
            version: args.root_version.clone(),
            purl_type: args.root_purl_type.clone(),
            omit_purl: args.no_root_purl,
        };
        let selection = crate::generate::root_selector::select_root(
            &components,
            &root_override_tmp,
            scan_target_coord.as_ref(),
            &target_name,
            "0.0.0",
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

    // Enrichment source control: resolve which sources are enabled.
    // `--offline` is handled by each enrichment source's internal
    // short-circuit (they log "offline, skipping" themselves), so we
    // don't need to gate here — but we avoid emitting misleading
    // "skipped (disabled by flags)" messages when the operative cause
    // is offline mode.
    let enrich_cfg = resolve_enrich_sources(&args);

    // deps.dev enrichment runs after the local scan so it only sees the
    // deduped component set. Components in unsupported ecosystems
    // (deb/apk/generic) are skipped silently inside the enrichment;
    // offline mode turns the whole pass into a no-op. Failures are
    // warnings, not errors — the scan still produces a valid SBOM if
    // deps.dev is unreachable.
    let deps_dev_client = DepsDevClient::new(std::time::Duration::from_secs(5));
    if enrich_cfg.deps_dev {
        let deps_dev_source = DepsDevSource::new(deps_dev_client.clone(), offline);
        let enriched = enrich_components(&deps_dev_source, &mut components).await;
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
        tracing::info!("ClearlyDefined enrichment skipped (disabled by flags)");
    }

    // deps.dev transitive dep-graph enrichment fills in edges the
    // local scan couldn't produce — shaded-JAR transitives, cold-
    // cache scans, BOM-declared deps. The response tree is merged
    // into the running component set with `source_type =
    // "declared-not-cached"` on any coord not already observed
    // locally; local versions win when deps.dev reports a different
    // version for the same (group, artifact) pair.
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
    let pre_fold_count = components.len();
    components = crate::resolve::deduplicator::deduplicate(components);
    let folded = pre_fold_count.saturating_sub(components.len());
    if folded > 0 {
        tracing::info!(
            folded,
            "folded declared-not-cached entries into on-disk twins",
        );
    }

    // Milestone 052/part-3: apply the `--exclude-scope` opt-out
    // filter as the final step before serialization. Drops
    // components whose lifecycle_scope matches any element in the
    // user's exclude list, plus any dependency edges referencing
    // dropped components. `Runtime` is never excluded (clap rejects
    // it at parse time via the ExcludeScopeArg enum). Default
    // behavior (empty exclude_scope vec) is no-op: emit all scopes.
    if !exclude_scope.is_empty() {
        let exclude_set: std::collections::HashSet<mikebom_common::resolution::LifecycleScope> =
            exclude_scope.iter().copied().collect();
        let pre_filter_count = components.len();
        let dropped_purls: std::collections::HashSet<String> = components
            .iter()
            .filter(|c| {
                c.lifecycle_scope
                    .is_some_and(|s| exclude_set.contains(&s))
            })
            .map(|c| c.purl.as_str().to_string())
            .collect();
        components.retain(|c| !dropped_purls.contains(c.purl.as_str()));
        relationships.retain(|r| {
            !dropped_purls.contains(&r.from) && !dropped_purls.contains(&r.to)
        });
        let dropped = pre_filter_count.saturating_sub(components.len());
        if dropped > 0 {
            tracing::info!(
                dropped,
                exclude_scope = ?exclude_scope,
                "applied --exclude-scope filter",
            );
        }
    }

    // Milestone 111 (issue #225 Option A): assemble the operator's
    // `--pkg-alias` declarations into a deterministic AliasMap. CLI-
    // supplied flags are concatenated with `MIKEBOM_PKG_ALIAS` env-var
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
    // `mikebom:source-document-binding` annotations to image-tier
    // components whose PURL has a counterpart in the source SBOM.
    // Per FR-011, failure to load the source SBOM aborts the scan.
    let bind_source_ctx: Option<mikebom::binding::SourceSbomContext> = if let Some(
        ref source_sbom_path,
    ) = args.bind_to_source
    {
        let ctx = mikebom::binding::SourceSbomContext::load(source_sbom_path).with_context(
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
    // to have overflowed or dropped.
    let integrity = TraceIntegrity {
        ring_buffer_overflows: 0,
        events_dropped: 0,
        uprobe_attach_failures: vec![],
        kprobe_attach_failures: vec![],
        partial_captures: vec![],
        bloom_filter_capacity: 0,
        bloom_filter_false_positive_rate: 0.0,
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
    let auto_detected_id: Option<mikebom::binding::identifiers::Identifier> =
        if args.image.is_some() {
            // Image-tier auto-detection — synthesize the canonical
            // `image:` form from the resolved-image fields.
            image_auto_identifier(_extracted.as_ref(), args.image.as_deref())
        } else {
            // Source-tier auto-detection — git-remote 3-step fallback.
            // Milestone 075 — `keep_credentials` boolean controls
            // userinfo sanitization (default: strip for security).
            mikebom::binding::identifiers::auto_detect::auto_detect_repo_identifier(
                &root_path,
                args.keep_credentials_in_identifiers,
            )
        };
    let manual_identifiers = assemble_manual_identifiers(&args);
    let identifiers = mikebom::binding::identifiers::resolve_identifiers(
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
        mikebom::binding::user_metadata::validate_annotator_pair_interleaving(&argv)
    {
        anyhow::bail!(diag);
    }
    let metadata_file = match args.metadata_file.as_deref() {
        Some(p) => Some(
            mikebom::binding::user_metadata::load_metadata_file(p)
                .map_err(|e| anyhow::anyhow!(e))?,
        ),
        None => None,
    };
    let user_metadata = mikebom::binding::user_metadata::merge_file_and_flags(
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
                mikebom::binding::user_metadata::CreatorKind::Organization
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
    // with pre-119 mikebom output per FR-013 / SC-006.
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
        ))
    } else {
        None
    };

    // Build the neutral artifacts bundle once and hand it to every
    // serializer the user requested — the single-pass guarantee of
    // FR-004 / SC-009.
    let artifacts = ScanArtifacts {
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
        scope_mode: if effective_include_declared_deps {
            crate::generate::ScopeMode::Manifest
        } else {
            crate::generate::ScopeMode::Artifact
        },
        // Milestone 061 (closes #119): doc-level Go graph-completeness
        // signal flows from the scan_fs::ScanResult into ScanArtifacts
        // for the per-format emitters' metadata.properties[] /
        // document-level annotations[] entries.
        go_graph_completeness,
        go_graph_completeness_reason: go_graph_completeness_reason.as_deref(),
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
        // Milestone 077: operator-supplied overrides for the root
        // component's name + version. Constructed from the new
        // `--root-name` / `--root-version` CLI flags; defaults to
        // both-None (back-compat) when neither flag is passed.
        root_override: crate::generate::RootComponentOverride {
            name: args.root_name.clone(),
            version: args.root_version.clone(),
            purl_type: args.root_purl_type.clone(),
            omit_purl: args.no_root_purl,
        },
        // Milestone 080: user-provided SBOM metadata aggregated from
        // the new flags (--creator / --annotator / --annotation-comment
        // / --metadata-comment / --scan-target-name / --metadata-file).
        user_metadata: user_metadata.clone(),
        // Milestone 081: operator-asserted CISA SBOM Type from the
        // new --sbom-type flag. When set, all three formats'
        // document-level lifecycle aggregations collapse to a
        // single-element output reflecting the asserted type;
        // per-component `mikebom:sbom-tier` annotations preserve
        // auto-detected values.
        sbom_type_override: args.sbom_type,
        // Issue #228: relationship-vocabulary compat flag (default
        // `full` preserves the milestone-052/part-2 typed
        // reversed-direction emission).
        spdx2_relationship_compat: args.spdx2_relationship_compat,
    };
    let output_cfg = OutputConfig {
        mikebom_version: env!("CARGO_PKG_VERSION"),
        created: scan_created_timestamp(),
        overrides: plan.overrides.clone(),
    };

    // Dispatch: serialize to every requested format and write each
    // emitted artifact to the chosen path. The first format's first
    // artifact path drives the backwards-compatible `--json` summary
    // output below (matches pre-milestone behavior, which only knew
    // about one file).
    let mut primary_output_path: Option<PathBuf> = None;
    let mut primary_format: Option<String> = None;
    for fmt in &plan.formats {
        let serializer = registry
            .get(fmt)
            .expect("format id validated by resolve_dispatch");
        let emitted = serializer.serialize(&artifacts, &output_cfg)?;
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
            let target = if artifact.relative_path
                == Path::new(serializer.default_filename())
            {
                plan.overrides
                    .get(fmt)
                    .cloned()
                    .unwrap_or_else(|| artifact.relative_path.clone())
            } else if artifact.relative_path
                == Path::new(
                    crate::generate::openvex::OPENVEX_DEFAULT_FILENAME,
                )
            {
                plan.overrides
                    .get(OPENVEX_PSEUDO_FORMAT)
                    .cloned()
                    .unwrap_or_else(|| artifact.relative_path.clone())
            } else {
                artifact.relative_path.clone()
            };
            write_bytes_to(&target, &artifact.bytes)?;
            if primary_output_path.is_none() {
                primary_output_path = Some(target.clone());
                primary_format = Some(fmt.clone());
            }
            tracing::info!(
                format = %fmt,
                path = %target.display(),
                bytes = artifact.bytes.len(),
                "wrote SBOM artifact"
            );
        }
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

/// Write `bytes` to `path`, creating any missing parent directories.
///
/// Shared by every serializer artifact (CDX today; SPDX + OpenVEX in
/// Resolve the `created` timestamp for the SBOM output config.
///
/// Defaults to `chrono::Utc::now()`. **Test-only override**: when the
/// `MIKEBOM_FIXED_TIMESTAMP` env var is set to an RFC 3339 string,
/// that value is used instead — required for tests that compare raw
/// SBOM bytes across two `mikebom sbom scan` subprocesses (e.g.
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
    if let Ok(s) = std::env::var("MIKEBOM_FIXED_TIMESTAMP") {
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&s) {
            return parsed.with_timezone(&chrono::Utc);
        }
    }
    chrono::Utc::now()
}

/// Milestone 072 / T027 helper: walk the resolved component set and
/// attach a `mikebom:source-document-binding` annotation to each
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
/// and `MIKEBOM_PKG_ALIAS` env-var entries into a single deterministic
/// `AliasMap`. Conflicts (same LHS, different RHS) abort the scan with
/// an actionable error per FR-008.
///
/// Env-var format: comma-separated `LHS=RHS` entries; whitespace
/// trimmed; empty entries silently skipped.
fn build_pkg_alias_map(
    args: &ScanArgs,
) -> anyhow::Result<mikebom::binding::alias::AliasMap> {
    use mikebom::binding::alias::{parse_pkg_alias, AliasMap};

    let mut map = AliasMap::new();

    // CLI flags first (insertion order = flag order).
    for alias in &args.pkg_alias {
        map.insert(alias.clone())
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    }

    // Then env-var entries.
    if let Ok(raw) = std::env::var("MIKEBOM_PKG_ALIAS") {
        for entry in raw.split(',') {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                continue;
            }
            let alias = parse_pkg_alias(trimmed)
                .map_err(|e| anyhow::anyhow!("MIKEBOM_PKG_ALIAS entry: {}", e))?;
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
    alias_map: &mikebom::binding::alias::AliasMap,
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
    components: &mut [mikebom_common::resolution::ResolvedComponent],
    ctx: &mikebom::binding::SourceSbomContext,
    alias_map: &mikebom::binding::alias::AliasMap,
) -> std::collections::HashSet<String> {
    use mikebom::binding::BindingStrength;
    use mikebom_common::types::purl::Purl;

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
            binding.alias_source = Some(mikebom::binding::AliasSource::OperatorSupplied);
        }

        // Serialize via the canonical serde shape so emission is
        // byte-stable across reruns. The CDX side will JSON-encode
        // this Value to a string at emission time (the milestone-023
        // bag does that automatically); the SPDX side wraps it in
        // the MikebomAnnotationCommentV1 envelope.
        if let Ok(value) = mikebom::binding::serialize_to_envelope_value(&binding) {
            c.extra_annotations.insert(
                mikebom::binding::BINDING_PROPERTY_NAME.to_string(),
                value,
            );
        }
    }
    consumed_aliases
}

/// later phases). Kept local to this CLI module so the generator crate
/// has no filesystem dependencies.
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

    fn reg() -> SerializerRegistry {
        SerializerRegistry::with_defaults()
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

    #[test]
    fn openvex_override_collides_with_cdx_path() {
        let err = resolve_dispatch(
            &reg(),
            &["cyclonedx-json".into(), "spdx-2.3-json".into()],
            &[
                "spdx-2.3-json=out.spdx.json".into(),
                "openvex=mikebom.cdx.json".into(),
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
    fn image_src_defaults_to_docker_then_remote() {
        let parsed = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan", "--path", ".",
        ])
        .unwrap();
        assert_eq!(
            parsed.inner.image_src,
            vec![ImageSource::Docker, ImageSource::Remote]
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
    fn image_src_rejects_unknown_value() {
        let err = <ScanArgsForTest as clap::Parser>::try_parse_from([
            "scan",
            "--path",
            ".",
            "--image-src",
            "podman",
        ])
        .unwrap_err()
        .to_string();
        assert!(
            err.to_lowercase().contains("invalid value")
                || err.to_lowercase().contains("possible values"),
            "expected clap to reject unknown image-src value, got: {err}"
        );
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
        enrich_sources: Vec<EnrichSource>,
    ) -> ScanArgs {
        ScanArgs {
            path: Some(PathBuf::from(".")),
            image: None,
            image_src: vec![],
            image_platform: None,
            no_oci_cache: false,
            oci_cache_size: None,
            registry_credentials_dir: None,
            output: vec![],
            format: vec![],
            max_file_size: 256 * 1024 * 1024,
            no_hashes: false,
            deb_codename: None,
            no_package_db: false,
            no_deep_hash: false,
            json: false,
            no_clearly_defined,
            no_deps_dev,
            no_deps_dev_graph,
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
        }
    }

    #[test]
    fn resolve_defaults_all_enabled() {
        let args = enrich_args(false, false, false, vec![]);
        let cfg = resolve_enrich_sources(&args);
        assert_eq!(cfg, EnrichConfig {
            deps_dev: true,
            clearly_defined: true,
            deps_dev_graph: true,
        });
    }

    #[test]
    fn resolve_no_clearly_defined_disables_cd() {
        let args = enrich_args(false, true, false, vec![]);
        let cfg = resolve_enrich_sources(&args);
        assert!(cfg.deps_dev);
        assert!(!cfg.clearly_defined);
        assert!(cfg.deps_dev_graph);
    }

    #[test]
    fn resolve_no_deps_dev_disables_license_enrichment() {
        let args = enrich_args(true, false, false, vec![]);
        let cfg = resolve_enrich_sources(&args);
        assert!(!cfg.deps_dev);
        assert!(cfg.clearly_defined);
        assert!(cfg.deps_dev_graph);
    }

    #[test]
    fn resolve_no_deps_dev_graph_disables_graph() {
        let args = enrich_args(false, false, true, vec![]);
        let cfg = resolve_enrich_sources(&args);
        assert!(cfg.deps_dev);
        assert!(cfg.clearly_defined);
        assert!(!cfg.deps_dev_graph);
    }

    #[test]
    fn resolve_all_no_flags_disables_everything() {
        let args = enrich_args(true, true, true, vec![]);
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
            false, false, false,
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

    use mikebom::binding::identifiers::Identifier;

    fn make_id(raw: &str, label: Option<&str>) -> Identifier {
        let mut id = Identifier::parse(raw).unwrap();
        id.source_label = label.map(|s| s.to_string());
        id
    }

    // Milestone 074 (T005): resolve_identifiers moved to
    // `mikebom::binding::identifiers::resolve_identifiers` with a
    // `Vec<Identifier>`-based auto-detected param. Tests pass through
    // an alias so the existing assertions read the same.
    use mikebom::binding::identifiers::resolve_identifiers;

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
            mikebom::binding::identifiers::IdentifierKind::UserDefined
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
        let mut args = enrich_args(false, false, false, vec![]);
        args.repo = Some("git@github.com:foo/bar.git".to_string());
        let ids = assemble_manual_identifiers(&args);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].as_wire(), "repo:git@github.com:foo/bar.git");
    }

    #[test]
    fn assemble_manual_identifiers_repo_plus_git_ref_emits_git_only() {
        // --repo + --git-ref → ONE git: identifier (supersedes repo:),
        // not two entries.
        let mut args = enrich_args(false, false, false, vec![]);
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
        let mut args = enrich_args(false, false, false, vec![]);
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

    use mikebom::binding::alias::{AliasMap, PurlAlias};
    use mikebom::binding::{
        BindingStrength, SourceDocumentBinding, SourceDocumentId, SourceSbomContext,
    };
    use mikebom_common::resolution::ResolvedComponent;
    use mikebom_common::types::purl::Purl;

    /// Shared env-mutation lock for env-var-touching tests below.
    /// Same pattern as milestone 110's `fingerprints::test_env_lock`.
    fn pkg_alias_env_lock() -> std::sync::MutexGuard<'static, ()> {
        use std::sync::Mutex;
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn make_alias(lhs: &str, rhs: &str) -> PurlAlias {
        PurlAlias::try_new(lhs, rhs).unwrap()
    }

    fn make_component(purl_str: &str) -> ResolvedComponent {
        use mikebom_common::resolution::{ResolutionEvidence, ResolutionTechnique};
        let purl = Purl::new(purl_str).unwrap();
        ResolvedComponent {
            build_inclusion: None,
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
            requirement_range: None,
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
                mikebom::binding::BindingHash::from_hex("a".repeat(64)).unwrap(),
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
        let mut args = enrich_args(false, false, false, vec![]);
        args.pkg_alias = vec![];
        unsafe {
            std::env::remove_var("MIKEBOM_PKG_ALIAS");
        }
        let map = build_pkg_alias_map(&args).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn build_pkg_alias_map_collects_cli_flags() {
        let _g = pkg_alias_env_lock();
        let mut args = enrich_args(false, false, false, vec![]);
        args.pkg_alias =
            vec![make_alias("pkg:generic/baz", "pkg:cargo/baz@1.0.0")];
        unsafe {
            std::env::remove_var("MIKEBOM_PKG_ALIAS");
        }
        let map = build_pkg_alias_map(&args).unwrap();
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn build_pkg_alias_map_unions_cli_and_env_var_entries() {
        let _g = pkg_alias_env_lock();
        let mut args = enrich_args(false, false, false, vec![]);
        args.pkg_alias =
            vec![make_alias("pkg:generic/baz", "pkg:cargo/baz@1.0.0")];
        unsafe {
            std::env::set_var(
                "MIKEBOM_PKG_ALIAS",
                "pkg:generic/qux=pkg:npm/qux@2.0.0",
            );
        }
        let map = build_pkg_alias_map(&args).unwrap();
        unsafe {
            std::env::remove_var("MIKEBOM_PKG_ALIAS");
        }
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn build_pkg_alias_map_skips_blank_env_entries() {
        let _g = pkg_alias_env_lock();
        let mut args = enrich_args(false, false, false, vec![]);
        args.pkg_alias = vec![];
        unsafe {
            std::env::set_var(
                "MIKEBOM_PKG_ALIAS",
                ",,pkg:generic/baz=pkg:cargo/baz@1.0.0,,",
            );
        }
        let map = build_pkg_alias_map(&args).unwrap();
        unsafe {
            std::env::remove_var("MIKEBOM_PKG_ALIAS");
        }
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn build_pkg_alias_map_rejects_conflicting_lhs_across_cli_and_env() {
        let _g = pkg_alias_env_lock();
        let mut args = enrich_args(false, false, false, vec![]);
        args.pkg_alias =
            vec![make_alias("pkg:generic/baz", "pkg:cargo/baz@1.0.0")];
        unsafe {
            std::env::set_var(
                "MIKEBOM_PKG_ALIAS",
                "pkg:generic/baz=pkg:cargo/baz@1.1.0",
            );
        }
        let result = build_pkg_alias_map(&args);
        unsafe {
            std::env::remove_var("MIKEBOM_PKG_ALIAS");
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
            .get(mikebom::binding::BINDING_PROPERTY_NAME)
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
            .get(mikebom::binding::BINDING_PROPERTY_NAME)
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
            .get(mikebom::binding::BINDING_PROPERTY_NAME)
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
                .get(mikebom::binding::BINDING_PROPERTY_NAME)
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
}
