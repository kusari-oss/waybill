# Quickstart: OCI Referrers API SBOM discovery (m186)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Contracts**: [cli-flag.md](./contracts/cli-flag.md) · [referrers-pipeline.md](./contracts/referrers-pipeline.md)

## Operator worked examples

### Example 1 — Prefer upstream SBOM when available, fall through to scan

**Use case**: You maintain a mixed image inventory where some upstream vendors publish SBOMs as OCI referrers and others don't. You want the referrer when it exists (fast + traceable to upstream signer) and a scan when it doesn't.

```bash
mikebom sbom scan \
  --image ghcr.io/kusari-oss/example-app:v1.2.3 \
  --sbom-source either \
  --format cyclonedx-json \
  --output cyclonedx-json=./example-app.cdx.json
```

**Expected outcomes**:
- If the registry publishes a CycloneDX SBOM as a referrer: emitted byte-identically as `./example-app.cdx.json`. INFO log:
  ```
  INFO mikebom::scan_fs::oci_pull: emitted SBOM from OCI Referrers API
    image = "ghcr.io/kusari-oss/example-app:v1.2.3"
    sbom-source = "referrer"
    descriptor-digest = "sha256:abc123..."
    media-type = "application/vnd.cyclonedx+json"
  ```
- If the registry has no referrer OR the endpoint returns HTTP 404: mikebom silently falls through to the existing image-scan pipeline. INFO log:
  ```
  INFO mikebom::scan_fs::oci_pull: no matching SBOM referrer found; falling through to scan
    image = "ghcr.io/kusari-oss/example-app:v1.2.3"
    reason = "no-matching-referrer"
  ```
  Output at `./example-app.cdx.json` is produced by the scan pipeline (byte-identical to pre-m186 behavior).

### Example 2 — Compliance workflow requiring upstream SBOM

**Use case**: A compliance policy requires that emitted SBOMs are traceable to the upstream vendor's signed publication. A scan-produced SBOM is not acceptable.

```bash
mikebom sbom scan \
  --image ghcr.io/kusari-oss/example-app:v1.2.3 \
  --sbom-source referrer \
  --format cyclonedx-json \
  --output cyclonedx-json=./example-app.cdx.json
```

**Expected outcomes**:
- If a matching referrer is found: emitted byte-identically. Exit 0.
- If no referrer is available OR the endpoint returns HTTP 404 OR any fetch step fails: mikebom exits non-zero with an actionable error on stderr per the CLI flag contract (see [cli-flag.md](./contracts/cli-flag.md) §Error message templates).

```
Error: no matching SBOM referrer found for ghcr.io/kusari-oss/example-app:v1.2.3 on registry ghcr.io
Exit code: 1
```

### Example 3 — Preserve pre-m186 byte-identity for existing pipelines

**Use case**: Your CI pipeline uses `mikebom sbom scan --image ... --format cyclonedx-json ...` today. You upgrade mikebom to a version that includes m186. You want NO behavior change — no new network activity, no output drift.

```bash
# No `--sbom-source` flag needed (or explicitly pass `scan`).
mikebom sbom scan \
  --image ghcr.io/kusari-oss/example-app:v1.2.3 \
  --format cyclonedx-json \
  --output cyclonedx-json=./example-app.cdx.json
```

**Expected outcomes**:
- Byte-identical output to pre-m186 for the same image + `--format` + other flags per FR-015 / SC-004.
- Zero HTTP calls to the Referrers endpoint (verified by unit test `default_flag_absence_equivalent_to_scan_mode` + integration test `scan_mode_never_calls_referrers_endpoint`).

### Example 4 — Referrer from a plain-HTTP registry

**Use case**: You run mikebom against a local registry (e.g., a `kind`-cluster registry mirror) that only speaks plain HTTP. The m182 `--insecure-registry` flag composes freely with `--sbom-source`.

```bash
mikebom sbom scan \
  --image localhost:5001/example-app:v1.2.3 \
  --insecure-registry localhost:5001 \
  --sbom-source referrer \
  --format cyclonedx-json \
  --output cyclonedx-json=./example-app.cdx.json
```

**Expected outcomes**:
- The Referrers endpoint is queried at `http://localhost:5001/v2/example-app/referrers/<manifest-digest>` (plain HTTP per the `--insecure-registry` matcher).
- Verified by integration test `referrers_endpoint_honors_insecure_registry_flag` per SC-007.

### Example 5 — Override the 100 MiB size cap

**Use case**: You need to fetch an unusually large SBOM (say, a 200 MiB SPDX 3 rollup for a mega-image) that exceeds the default 100 MiB cap.

```bash
MIKEBOM_REFERRER_MAX_BYTES=$((256 * 1024 * 1024)) \
mikebom sbom scan \
  --image ghcr.io/kusari-oss/mega-app:v1.0.0 \
  --sbom-source referrer \
  --format spdx-2.3-json \
  --output spdx-2.3-json=./mega-app.spdx.json
```

**Expected outcomes**:
- The size-cap enforcement in `pick_sbom_descriptor` accepts descriptors up to 256 MiB.
- Without the env-var override, the 200 MiB descriptor would be skipped with a WARN log; under `--sbom-source referrer` mode this would result in a non-zero exit.

## Developer worked example (contributor flow)

### Adding a new supported SBOM media type

To extend `SBOM_MEDIA_TYPES` in a follow-up milestone (e.g., add SPDX 3 support once the ecosystem's SPDX 3 referrer media type is stable):

1. Edit `mikebom-cli/src/scan_fs/oci_pull/referrers.rs`:
   ```rust
   pub(super) const SBOM_MEDIA_TYPES: &[&str] = &[
       "application/vnd.cyclonedx+json",
       "application/spdx+json",
       "application/vnd.cyclonedx+xml",
       "application/vnd.spdx+json",  // NEW — SPDX 3 (once ecosystem-stable)
   ];
   ```

2. Extend `media_type_for_mikebom_format`:
   ```rust
   pub(super) fn media_type_for_mikebom_format(fmt: &str) -> Option<&'static str> {
       match fmt {
           "cyclonedx-json" => Some("application/vnd.cyclonedx+json"),
           "spdx-2.3-json" => Some("application/spdx+json"),
           "spdx-3-json" => Some("application/vnd.spdx+json"),  // NEW
           _ => None,
       }
   }
   ```

3. Add unit tests to the `#[cfg(test)] mod tests` block in `referrers.rs`:
   - `pick_sbom_descriptor_prefers_spdx3_when_format_matches`
   - `media_type_for_mikebom_format_maps_spdx3`

4. Add integration coverage in `mikebom-cli/tests/oci_referrers_strict_mode.rs`:
   - `referrer_mode_emits_spdx3_referrer` mounting a wiremock handler that returns an SPDX 3 blob.

5. Update the fallback tier in `pick_sbom_descriptor`'s "Tier 2: CDX-first fallback" iter set (the ordering encodes mikebom's preference; SPDX 3 belongs after CDX+json).

### Running the m186 integration tests locally

```bash
# Unit tests only (fast; ~1s):
cargo +stable test -p mikebom-cli --lib scan_fs::oci_pull::referrers

# US1 integration tests (~10s each; wiremock spins up an in-process HTTP server):
cargo +stable test -p mikebom-cli --test oci_referrers_either_mode

# US2 integration tests:
cargo +stable test -p mikebom-cli --test oci_referrers_strict_mode

# US3 backward-compat guard tests (verifies zero network calls under scan mode):
cargo +stable test -p mikebom-cli --test oci_referrers_backward_compat

# Full pre-PR gate:
./scripts/pre-pr.sh
```

### Verification checklist for merge

Before opening a PR:
- [ ] `cargo +stable clippy --workspace --all-targets -- -D warnings` — zero warnings.
- [ ] `cargo +stable test --workspace` — every suite passes with `0 failed`.
- [ ] `./scripts/pre-pr.sh` runs to green.
- [ ] Zero drift in `cargo tree --workspace` output (SC-008 zero-new-deps gate; capture pre/post + diff to confirm).
- [ ] Existing golden fixtures produce byte-identical output (FR-015 / SC-004; regen fixtures + `git diff` — expect zero drift).

## FAQ

**Q: Does m186 verify the signature on a Cosign-signed SBOM referrer?**
A: No — signature verification is deferred per spec.md §Deferred. m186 verifies only the SHA-256 digest of the fetched blob against the descriptor's declared digest.

**Q: What happens if the registry publishes multiple SBOMs (one CDX + one SPDX) as referrers?**
A: `pick_sbom_descriptor` picks ONE — the priority order is: (1) format match for the operator's `--format`, (2) CDX-first fallback, (3) first-descriptor tiebreaker. See [research.md Decision 2](./research.md#decision-2--media-type-filter--priority-ordering-fr-004).

**Q: What happens if the referrer content is a different format than my `--format` request (e.g., I asked for CDX, only SPDX is available)?**
A: The referrer is emitted BYTE-IDENTICALLY at the `--output cyclonedx-json=<path>` path even though the file is actually SPDX. mikebom does NOT transcode. A WARN log surfaces the mismatch. Under `--sbom-source either`, if the operator is strict about the format, use `--sbom-source referrer` with a matching format and fall back to a separate scan invocation when no format match exists.

**Q: Can I use `--sbom-source` with a local tarball (`--image /path/to/image.tar`)?**
A: No — mikebom exits non-zero per FR-011. The Referrers API is a registry-only concept.

**Q: Does the m036 blob cache short-circuit repeated Referrers-endpoint queries?**
A: No — each `--sbom-source referrer|either` invocation re-queries the endpoint per plan.md §Storage. The m036 cache is for image-layer blobs, which are content-addressed and safe to cache; the Referrers-endpoint response is a mutable registry surface and mikebom prefers freshness over cache-hit latency.
