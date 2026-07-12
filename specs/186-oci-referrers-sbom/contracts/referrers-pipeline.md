# Contract: Referrers Discovery Pipeline (m186)

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md) · **Data model**: [../data-model.md](../data-model.md)

## Pipeline shape

The `try_fetch_referrer_sbom` entry point runs a 5-step pipeline. Each step has a defined success signal + failure signal; failure signals surface to the caller via `Result<Option<ReferrerSbom>>`.

```
--sbom-source referrer|either
        │
        ▼
[1] Parse image reference + resolve platform
    (reuse existing `reference::parse_reference` + `platform::resolve_manifest_list_to_linux`)
        │
        ▼
[2] Fetch image manifest → resolved single-platform digest
    (existing `RegistryClient::fetch_manifest` + platform resolution — same as pull_to_tarball)
        │
        ▼
[3] Query Referrers endpoint
    GET /v2/<repo>/referrers/<resolved-digest>
        │
        ├── HTTP 404      → return Ok(None) — no v1.1 support
        ├── HTTP 5xx      → return Err(...) — transport failure
        ├── HTTP 401/403 → return Err(...) — auth failure (or Ok(None) under `either`)
        ├── HTTP 200 + empty ImageIndex → return Ok(None) — no referrers
        └── HTTP 200 + non-empty ImageIndex → advance
        │
        ▼
[4] Filter + pick best SBOM descriptor
    (new `referrers::pick_sbom_descriptor`)
        │
        ├── No SBOM media type in the index → return Ok(None)
        ├── All matching descriptors exceed size cap → return Ok(None)
        └── One winner → advance
        │
        ▼
[5] Fetch + verify referrer blob
    GET /v2/<repo>/blobs/<descriptor.digest>
    Verify sha256(response body) == descriptor.digest
        │
        ├── HTTP failure       → return Err(...)
        ├── Digest mismatch    → return Err(...)
        └── Success            → return Ok(Some(ReferrerSbom { bytes, digest, media_type }))
```

## Per-step contracts

### Step 1 — Reference parse + platform resolve

**Function**: existing `reference::parse_reference(image_ref)` + `platform::resolve_manifest_list_to_linux(...)`

**Contract**: No new behavior. m186 reuses the exact code path `pull_to_tarball` runs at `mod.rs:103` + platform-resolution loop.

**Failure**: propagates to caller as `Err(_)`. Both `referrer` and `either` modes surface reference-parse failures identically (this is an operator input error, not a fall-through condition).

### Step 2 — Manifest fetch

**Function**: existing `RegistryClient::fetch_manifest(reference)` at `registry.rs:141`

**Contract**: No new behavior. Returns the single-platform manifest for the resolved platform. m186 needs the manifest's own digest (the "subject" of the Referrers query) — this is captured by re-invoking `fetch_manifest` on the manifest reference and taking the digest of the response body OR by using the `Content-Digest` response header (see Decision 1 for the header approach — simpler + spec-compliant).

**Failure**: propagates to caller. Both modes surface manifest fetch failures identically.

### Step 3 — Referrers endpoint query

**Function**: NEW `RegistryClient::fetch_referrers(reference, manifest_digest)` at `registry.rs`

**Contract**:
- URL: `{scheme}://{registry}/v2/{repository}/referrers/{manifest_digest}` where `{scheme}` is chosen by the m182 `is_insecure_registry` matcher.
- Auth: uses the same `fetch_with_auth_retry` shared function as manifest fetches (bearer/basic auth retry flow).
- Accept header: `application/vnd.oci.image.index.v1+json` (per Distribution Spec v1.1 §Referrers Response).

**Success signal**: HTTP 200 + valid ImageIndex body → `Ok(Some(ImageIndex))`.

**Failure signals**:
- HTTP 404 → `Ok(None)` (registry lacks v1.1 support; not an error per se).
- Any other non-2xx → `Err(_)` with `context()`-annotated message.
- Body deserialization failure → `Err(_)`.

**Retry semantics**: reuses `fetch_with_auth_retry`'s built-in 401→auth-fetch→retry loop. No additional retry logic in m186 (rate-limit 429 handling with `Retry-After` per FR is deferred to a follow-up; m186 fails on 429 with the underlying `reqwest::Error`).

### Step 4 — Descriptor filter + pick

**Function**: NEW `referrers::pick_sbom_descriptor(index, requested_formats, max_bytes)` at `referrers.rs`

**Contract** (per research Decision 2 + Decision 4):

```rust
pub(super) fn pick_sbom_descriptor<'a>(
    index: &'a ImageIndex,
    requested_formats: &[&str],
    max_bytes: u64,
) -> Option<&'a Descriptor> {
    let candidates: Vec<&Descriptor> = index
        .manifests()
        .iter()
        .filter(|d| SBOM_MEDIA_TYPES.contains(&d.media_type().to_string().as_str()))
        .filter(|d| (d.size() as u64) <= max_bytes)
        .collect();
    if candidates.is_empty() {
        return None;
    }

    // Tier 1: explicit format match (Decision 2).
    for fmt in requested_formats {
        if let Some(target_mt) = media_type_for_mikebom_format(fmt) {
            if let Some(d) = candidates
                .iter()
                .find(|d| d.media_type().to_string() == target_mt)
            {
                return Some(*d);
            }
        }
    }

    // Tier 2: CDX-first fallback (Decision 2).
    for target_mt in &[
        "application/vnd.cyclonedx+json",
        "application/spdx+json",
        "application/vnd.cyclonedx+xml",
    ] {
        if let Some(d) = candidates
            .iter()
            .find(|d| d.media_type().to_string() == *target_mt)
        {
            return Some(*d);
        }
    }

    // Tier 3: first-descriptor tiebreaker (should never reach here
    // because SBOM_MEDIA_TYPES == Tier 2 iter set, but defensive).
    candidates.into_iter().next()
}
```

**Success signal**: `Some(&Descriptor)` — the winning descriptor.

**Failure signals**:
- Empty candidate list → `None`. Caller under `either` mode falls through to scan; under `referrer` mode errors.
- All candidates exceed `max_bytes` → `None` (WARN log for each skipped descriptor with size + cap for diagnostic visibility).

### Step 5 — Blob fetch + digest verify

**Function**: existing `RegistryClient::fetch_blob(reference, digest)` at `registry.rs:171`

**Contract**: No new behavior. Reuses the existing blob-fetch + `verify_sha256` path. m186's descriptor.digest becomes the blob's expected SHA-256.

**Success signal**: `Ok(Vec<u8>)` — verified body bytes.

**Failure signals**:
- HTTP failure → `Err(_)`.
- Digest mismatch → `Err(_)` with the `verify_sha256` diagnostic ("blob digest mismatch: expected sha256:..., got sha256:...").

## Overall return shape

The end-to-end `try_fetch_referrer_sbom` returns:

| Result             | Trigger                                                                      | Under `referrer` mode          | Under `either` mode      |
|--------------------|------------------------------------------------------------------------------|----------------------------------|--------------------------|
| `Ok(Some(sbom))`   | Steps 1–5 all succeed                                                        | Emit `sbom.bytes` verbatim + INFO log | Same as `referrer` |
| `Ok(None)`         | Step 3 → 404, Step 3 → empty index, Step 4 → no match / all oversize         | Exit non-zero with reason msg   | Fall through to scan + INFO log |
| `Err(_)`           | Step 1/2 fail, Step 3 non-404 non-2xx, Step 5 fetch/digest fails             | Exit non-zero with reason msg   | Fall through to scan + WARN log |

## Provenance emission (FR-007)

On `Ok(Some(sbom))` return, the caller (scan_cmd.rs dispatcher) MUST:

1. Write `sbom.bytes` to the operator's `--output` path verbatim. Zero mutation.
2. Emit an INFO-level log entry:
   ```
   INFO mikebom::scan_fs::oci_pull: emitted SBOM from OCI Referrers API
     image = "<image-ref>"
     sbom-source = "referrer"
     descriptor-digest = "<sbom.descriptor_digest>"
     media-type = "<sbom.media_type>"
     output-path = "<--output value>"
   ```
3. NOT emit any `mikebom:sbom-source-*` fields into the SBOM content. Byte-identity per Decision 3.

## Size-cap enforcement (Decision 4 / FR-014)

- Read `MIKEBOM_REFERRER_MAX_BYTES` env var at scan startup; default 100 * 1024 * 1024 bytes.
- Pass the resolved cap to `pick_sbom_descriptor` (which filters oversize descriptors) AND to `RegistryClient::fetch_referrers` (as a request-side content-length hint — deferred; m186 relies on descriptor-side filtering only).
- When any descriptor is skipped due to the cap, log at WARN level:
  ```
  WARN mikebom::scan_fs::oci_pull::referrers: skipping oversize referrer descriptor
    digest = "<descriptor.digest>"
    declared-size = <descriptor.size>
    cap = <max_bytes>
    hint = "override via MIKEBOM_REFERRER_MAX_BYTES env var if trusted"
  ```

## Test surface (data-model.md §6 concrete tests)

Each step has explicit unit + integration test coverage. See data-model.md §6 for the full list.
