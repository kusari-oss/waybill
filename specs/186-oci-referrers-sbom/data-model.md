# Data Model: OCI Referrers API SBOM discovery (m186)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md)

## 1. New Type — `SbomSourceMode`

### 1.1 Enum definition (clap `ValueEnum`)

**File**: `mikebom-cli/src/cli/scan_cmd.rs` (colocated with the existing `ImageSource` enum)

```rust
/// Milestone 186 (#442) — controls whether mikebom fetches a
/// pre-existing SBOM via the OCI Distribution Spec v1.1 Referrers
/// API or scans the image bytes to produce a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum SbomSourceMode {
    /// Default. Always scan the image bytes; never query the
    /// Referrers API. Preserves pre-m186 behavior byte-identically
    /// per FR-015 / SC-004.
    #[value(name = "scan")]
    Scan,
    /// Query the Referrers API and REQUIRE that an SBOM referrer be
    /// found. Exit non-zero with actionable error if absent per
    /// FR-009 / SC-003.
    #[value(name = "referrer")]
    Referrer,
    /// Query the Referrers API and prefer any matching SBOM
    /// referrer; fall through to scan silently if none available
    /// (or if any fetch step fails) per FR-008 / SC-002.
    #[value(name = "either")]
    Either,
}

impl Default for SbomSourceMode {
    fn default() -> Self {
        SbomSourceMode::Scan
    }
}
```

### 1.2 CLI flag on `ScanArgs`

```rust
/// `--sbom-source <mode>` — controls SBOM discovery vs generation.
///
/// * `scan` (default) — scan the image bytes. No Referrers API
///   query. Byte-identical to pre-m186.
/// * `referrer` — REQUIRE a matching SBOM referrer via OCI
///   Distribution Spec v1.1 Referrers API. Exit non-zero if
///   absent.
/// * `either` — prefer a referrer if available; fall through to
///   scan if none.
///
/// Milestone 186 (#442). Applies only to registry-pull scans
/// (`--image <ref>` where the ref is an OCI reference). Rejected
/// (non-zero exit) when used against `--image <local-path>` or
/// `--path` scans per FR-011.
#[arg(long = "sbom-source", value_enum, default_value_t = SbomSourceMode::Scan)]
pub sbom_source: SbomSourceMode,
```

## 2. Referrers Response Type — Reused `oci_spec::image::ImageIndex`

No new type. `oci-spec = "0.9"` (workspace dep, `features = ["distribution", "image"]`) already exposes `ImageIndex`. The OCI Distribution Spec v1.1 Referrers endpoint returns an `application/vnd.oci.image.index.v1+json` body per §Referrers Sample, so deserialization is a `serde_json::from_slice::<ImageIndex>(&body)?` — identical to the m031 manifest-list resolution flow at `mod.rs:144`.

### 2.1 Descriptor filter helper (new)

**File**: `mikebom-cli/src/scan_fs/oci_pull/referrers.rs` (new)

```rust
/// Milestone 186 US1/US2 — pick the best SBOM descriptor from a
/// Referrers-API response per research.md Decision 2's priority
/// order.
///
/// Returns `None` when no descriptor matches an SBOM media type
/// OR when all matching descriptors exceed the size cap.
pub(super) fn pick_sbom_descriptor<'a>(
    index: &'a oci_spec::image::ImageIndex,
    requested_formats: &[&str],
    max_bytes: u64,
) -> Option<&'a oci_spec::image::Descriptor>;
```

### 2.2 Supported SBOM media type constants

```rust
/// Milestone 186 — SBOM media types recognized by the referrer
/// filter. Extend in follow-up milestones (SPDX 3, SPDX
/// tag-value, KDL, etc.) per spec.md §Deferred.
pub(super) const SBOM_MEDIA_TYPES: &[&str] = &[
    "application/vnd.cyclonedx+json",  // CDX 1.x JSON
    "application/spdx+json",            // SPDX 2.3 JSON
    "application/vnd.cyclonedx+xml",    // CDX 1.x XML
];

/// Map mikebom's `--format` value to the referrer descriptor
/// media type it corresponds to. Used by `pick_sbom_descriptor`
/// for the format-match preference (Decision 2 tier 1).
pub(super) fn media_type_for_mikebom_format(fmt: &str) -> Option<&'static str> {
    match fmt {
        "cyclonedx-json" => Some("application/vnd.cyclonedx+json"),
        "spdx-2.3-json" => Some("application/spdx+json"),
        // SPDX 3 + XML variants — not in the initial media-type set.
        _ => None,
    }
}
```

## 3. Fetch Entry Point — `try_fetch_referrer_sbom`

**File**: `mikebom-cli/src/scan_fs/oci_pull/mod.rs` (extend existing module)

```rust
/// Milestone 186 (#442) — attempt to fetch an SBOM from the OCI
/// Distribution Spec v1.1 Referrers API for the given image ref.
///
/// Returns:
/// - `Ok(Some(bytes))` — a matching SBOM was fetched + digest-
///   verified. Bytes are the SBOM content verbatim (opaque per
///   FR-006). The `descriptor_digest` field is populated for
///   provenance logging.
/// - `Ok(None)` — no matching referrer found (endpoint 404,
///   empty descriptor list, no SBOM media types, size cap
///   exceeded). Caller falls through to scan under `either`
///   mode OR errors under `referrer` mode per FR-008/FR-009.
/// - `Err(_)` — HTTP or verify-level failure. Caller decides
///   how to surface it based on `SbomSourceMode`.
///
/// The `requested_formats` slice is passed through to
/// `pick_sbom_descriptor` for format-match preference (Decision
/// 2). Callers pass their `--format` values in the order the
/// operator specified them.
pub async fn try_fetch_referrer_sbom(
    image_ref: &str,
    image_platform: Option<&str>,
    creds_dir: Option<&std::path::Path>,
    tls_config: &RegistryTlsConfig,
    requested_formats: &[&str],
    max_bytes: u64,
) -> anyhow::Result<Option<ReferrerSbom>>;

/// Milestone 186 — the fetched SBOM + provenance markers for
/// audit-logging by the caller.
pub struct ReferrerSbom {
    pub bytes: Vec<u8>,
    pub descriptor_digest: String,  // "sha256:..." for FR-007 log
    pub media_type: String,          // for the WARN-log if format mismatched
}
```

## 4. Client-level Fetch — `RegistryClient::fetch_referrers`

**File**: `mikebom-cli/src/scan_fs/oci_pull/registry.rs` (extend existing struct)

```rust
impl RegistryClient {
    /// Milestone 186 (#442) — GET `/v2/<repo>/referrers/<digest>`
    /// per OCI Distribution Spec v1.1 §Referrers. Uses the same
    /// bearer/basic auth retry flow as `fetch_manifest` (shared
    /// via `fetch_with_auth_retry`).
    ///
    /// Returns:
    /// - `Ok(Some(ImageIndex))` — the endpoint responded with a
    ///   valid ImageIndex body (may contain zero descriptors).
    /// - `Ok(None)` — the endpoint returned HTTP 404. Signals
    ///   "this registry does not support the Referrers API
    ///   (pre-v1.1)"; caller decides fall-through vs error per
    ///   `SbomSourceMode`.
    /// - `Err(_)` — HTTP or auth-level failure.
    pub(super) async fn fetch_referrers(
        &self,
        reference: &ImageReference,
        manifest_digest: &str,
    ) -> anyhow::Result<Option<oci_spec::image::ImageIndex>>;
}
```

## 5. Dispatch Matrix

The `scan_cmd.rs` image-scan dispatcher (at `scan_cmd.rs:1717`) branches on `args.sbom_source` per this matrix:

| `--sbom-source` | Referrers fetch outcome | Action |
|---|---|---|
| `scan` (default) | (not invoked) | Execute existing `pull_to_tarball` → scan pipeline unchanged. |
| `referrer` | `Ok(Some(bytes))` | Write `bytes` to `--output` path verbatim. Log INFO with `mikebom:sbom-source = "referrer"` + descriptor digest + media type. Exit 0. |
| `referrer` | `Ok(None)` | Exit non-zero. stderr message: `no matching SBOM referrer found for <image> on registry <name>` (or `registry does not support Referrers API (HTTP 404)`). |
| `referrer` | `Err(_)` | Exit non-zero. stderr message: `Referrers API fetch failed for <image>: <underlying error>`. |
| `either` | `Ok(Some(bytes))` | Same as `referrer` + `Ok(Some(bytes))`. |
| `either` | `Ok(None)` OR `Err(_)` | INFO-log the reason for fall-through. Continue to `pull_to_tarball` → scan pipeline unchanged. |

## 6. Test Contract

### 6.1 Unit tests (colocated with `referrers.rs`)

- `pick_sbom_descriptor_prefers_format_match` — Decision 2 tier 1
- `pick_sbom_descriptor_cdx_first_fallback` — Decision 2 tier 2
- `pick_sbom_descriptor_first_descriptor_tiebreaker` — Decision 2 tier 3
- `pick_sbom_descriptor_returns_none_on_empty_index` — no descriptors
- `pick_sbom_descriptor_returns_none_on_non_sbom_types` — only signatures/attestations present
- `pick_sbom_descriptor_skips_oversize_descriptors` — Decision 4 size cap
- `media_type_for_mikebom_format_maps_cdx_and_spdx23` — regression pin

### 6.2 Integration tests (wiremock-based, reuses m182 pattern)

**File**: `mikebom-cli/tests/oci_referrers_either_mode.rs`
- `either_prefers_referrer_when_available` (US1 acceptance 1)
- `either_falls_through_to_scan_when_no_referrer` (US1 acceptance 3)
- `either_falls_through_silently_on_404` (US1 acceptance 4)
- `either_falls_through_on_size_cap_exceeded` (Decision 4)

**File**: `mikebom-cli/tests/oci_referrers_strict_mode.rs`
- `referrer_mode_emits_matching_referrer` (US2 acceptance 1)
- `referrer_mode_errors_on_no_match` (US2 acceptance 2)
- `referrer_mode_errors_on_404_registry` (US2 acceptance 3)
- `referrer_mode_errors_on_size_cap` (Decision 4)

**File**: `mikebom-cli/tests/oci_referrers_backward_compat.rs`
- `scan_mode_never_calls_referrers_endpoint` (US3 acceptance 1 + FR-010)
- `default_flag_absence_equivalent_to_scan_mode` (US3 acceptance 2)
- `sbom_source_rejected_on_local_tarball_input` (FR-011)

### 6.3 FR-013 m182 TLS-flag inheritance verification

- `referrers_endpoint_honors_insecure_registry_flag` — reuse m182 wiremock plain-HTTP + `--sbom-source referrer` (SC-007)
- `referrers_endpoint_honors_ca_cert_flag` — deferred per m182 T019 rationale (would need in-test TLS server); coverage via unit-test level in `referrers.rs` demonstrating that the fetch calls flow through the m182-configured reqwest client.

## 7. Backward Compatibility

- **`--sbom-source` unspecified** (or explicitly `scan`): zero network activity on the Referrers endpoint (FR-010). Byte-identical to pre-m186 for the same image ref + `--format` + other flags. SC-004 gate.
- **Existing `--image-src remote` invocations**: fully compatible. m186 adds `--sbom-source` orthogonally to the existing `--image-src` (docker/remote) source-selection flag.
- **Existing golden fixtures**: zero drift expected (no fixture invokes `--sbom-source`). Regen produces byte-identical output.
