# Contract: `--sbom-source` CLI Flag (m186)

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md) · **Data model**: [../data-model.md](../data-model.md)

## Flag surface

Adds one flag to the `mikebom sbom scan` subcommand. Zero conflict with existing flags.

### Signature

```
--sbom-source <mode>          Where mode ∈ {scan, referrer, either}
                              Default: scan (backward compatible)
```

Clap attribute (per data-model.md §1.2):

```rust
#[arg(long = "sbom-source", value_enum, default_value_t = SbomSourceMode::Scan)]
pub sbom_source: SbomSourceMode,
```

### Help text (rendered by `--help`)

```
--sbom-source <MODE>
    Where mikebom should get the SBOM: scan the image bytes or fetch a
    pre-existing SBOM via the OCI Distribution Spec v1.1 Referrers API.

    - scan (default): Always scan the image bytes. Preserves pre-m186
      behavior byte-identically. No network activity on the Referrers
      endpoint.
    - referrer: REQUIRE a matching SBOM referrer. Exit non-zero if
      absent. Use for compliance workflows requiring upstream-published
      SBOMs only.
    - either: Prefer a referrer if available; fall through to scan
      silently if none. Cost-effective for images that publish SBOMs.

    Applies only to registry-pull scans. Rejected when used against
    `--image <local-tarball-path>` or `--path` scans.

    [default: scan] [possible values: scan, referrer, either]
```

## Mode × Input-Type × Output-Format decision matrix

### Mode × Input-Type

| Input type                        | `scan`         | `referrer`                                            | `either`                                              |
|-----------------------------------|----------------|--------------------------------------------------------|--------------------------------------------------------|
| `--image <oci-ref>` (registry)    | Scan image     | Fetch referrer OR error                                | Try referrer; fall through to scan                     |
| `--image <local-tarball-path>`    | Scan tarball   | **REJECTED** — exit non-zero (FR-011)                 | **REJECTED** — exit non-zero (FR-011)                 |
| `--path <dir>`                    | Scan directory | **REJECTED** — exit non-zero (FR-011)                 | **REJECTED** — exit non-zero (FR-011)                 |

**Rejection error message** (FR-011):
```
Error: --sbom-source <mode> is only valid for registry-pull scans (--image <oci-ref>).
       Use --sbom-source scan (or omit) to scan a local tarball or filesystem path.
```

### Mode × Output-Format Priority (Decision 2)

When multiple SBOM referrers are present on a registry, the descriptor selection follows this priority:

| Requested `--format` (first value)  | Preferred descriptor media type              | Fallback order                                                              |
|--------------------------------------|-----------------------------------------------|------------------------------------------------------------------------------|
| `cyclonedx-json`                    | `application/vnd.cyclonedx+json`             | → SPDX+json → CDX+xml → skip                                                 |
| `spdx-2.3-json`                     | `application/spdx+json`                       | → CDX+json → CDX+xml → skip                                                  |
| `spdx-3-json` (not in initial set)  | (no format match)                             | → CDX+json → SPDX+json → CDX+xml → skip                                      |
| `cyclonedx-xml` (not implemented)   | (no format match)                             | → CDX+json → SPDX+json → CDX+xml → skip                                      |
| Multiple `--format` specified       | Use FIRST format for match; fallback for rest | Emit ONE referrer at most (multi-emission deferred per spec.md §Deferred)   |

## Backward compatibility guard (FR-015 / SC-004)

- **Any invocation without `--sbom-source`** (or explicitly `--sbom-source scan`) MUST:
  - NOT invoke the Referrers endpoint.
  - Produce output byte-identical to pre-m186 for the same image + `--format` + other flags.
- Verified by:
  - Unit test `default_flag_absence_equivalent_to_scan_mode` (assert enum default = `Scan`).
  - Integration test `scan_mode_never_calls_referrers_endpoint` (wiremock: assert zero `.received()` on the Referrers handler).
  - Golden regen: zero drift on existing fixtures (no fixture invokes `--sbom-source`).

## Flag composition with m182 TLS flags

- `--sbom-source` composes freely with `--insecure-registry`, `--registry-ca-cert`, `--insecure-tls-skip-verify`, and `--registry-credentials-dir`. The Referrers-endpoint fetch reuses the same `RegistryClient` (built with the m182 `RegistryTlsConfig`) as manifest / blob fetches — no additional plumbing.
- Verified by:
  - Integration test `referrers_endpoint_honors_insecure_registry_flag`.
  - Data model §6.3 documents the FR-013 SC-007 gate.

## Flag composition with `--image-src`

- `--image-src` selects WHERE the image comes from (docker daemon, remote registry, local tarball). `--sbom-source` selects HOW the SBOM is produced (fetch vs scan). Orthogonal concerns; both flags can be specified together.
- When `--image-src` resolves to a NON-remote source (docker daemon or local tarball) AND `--sbom-source` is `referrer` or `either`, mikebom exits non-zero per FR-011 — the Referrers API only exists on remote registries.
- Verified by:
  - Integration test `sbom_source_rejected_on_local_tarball_input`.

## Error message templates (FR-009 strict-mode reasons)

Under `--sbom-source referrer`, the error message varies by failure reason:

| Reason                                              | stderr message                                                                                     | Exit code |
|-----------------------------------------------------|----------------------------------------------------------------------------------------------------|-----------|
| Referrers endpoint returned HTTP 404                | `registry <name> does not support the OCI Referrers API (HTTP 404); use --sbom-source scan or --sbom-source either to scan the image bytes instead` | 1 |
| Referrers response has zero SBOM-shaped descriptors | `no matching SBOM referrer found for <image> on registry <name>`                                    | 1 |
| Descriptor exceeds `MIKEBOM_REFERRER_MAX_BYTES`     | `SBOM referrer for <image> declares size <bytes> exceeding cap <cap> bytes; override via MIKEBOM_REFERRER_MAX_BYTES env var if trusted` | 1 |
| Auth failure (401/403) on Referrers endpoint        | `authentication failed for Referrers API on <registry>; verify credentials for the referrer namespace` | 1 |
| TLS handshake failure                                | (m182 FR-014 templates apply verbatim)                                                              | 1 |
| SHA-256 verify failure on fetched descriptor blob   | `SBOM referrer blob digest mismatch for <descriptor>: expected sha256:<expected>, got sha256:<actual>` | 1 |

Under `--sbom-source either`, each of these reasons logs at INFO/WARN level and falls through to scan silently.
