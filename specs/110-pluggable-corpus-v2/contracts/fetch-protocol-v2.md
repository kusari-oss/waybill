# Fetch protocol v2 (FR-020 contract for third-party corpus authors)

This contract documents how mikebom-cli fetches a corpus archive from a configured source. It is the stable interface anyone hosting their own corpus must implement so mikebom can consume it. Backward-compatible with milestone-108's fetch protocol; v2 adds optional auth + per-source signature anchors.

## Source URL convention

A corpus source is a stable URL pointing at:

1. **A directory listing** that exposes `release.json` (manifest) + `<sha>.tar.gz` archives, OR
2. **A single archive URL** (`https://corpus.example/release-v0.3.1.tar.gz`) with sibling files `<archive>.sig` + `<archive>.cert` at the same path.

The directory-listing form is the milestone-108 pattern (the public corpus's `release.json` lists pinned SHAs per release). The single-archive form is supported for vendors that pin SHA at the URL level (release-tag-keyed URLs).

### release.json manifest (directory-listing form)

```json
{
  "schema_version": 1,
  "latest_pinned_sha": "abc123def456...",
  "archives": [
    {
      "sha": "abc123def456...",
      "archive_url": "https://corpus.example/abc123.tar.gz",
      "signature_url": "https://corpus.example/abc123.tar.gz.sig",
      "certificate_url": "https://corpus.example/abc123.tar.gz.cert",
      "published_at": "2026-06-01T12:00:00Z"
    }
  ]
}
```

mikebom-cli fetches `release.json`, picks the entry matching the configured pin (or `latest_pinned_sha` if unpinned), then fetches the three archive URLs.

## HTTP semantics

- **Method**: `GET` for every URL.
- **TLS**: HTTPS only (`rustls-tls`); no certificate-pinning at the mikebom layer.
- **Authentication** (optional, per-source): if the source's config declares `credential_env`, mikebom reads `$<ENV_VAR>` and sets the `Authorization: Bearer <value>` request header on every URL under that source. The credential never appears in process argv.
- **Timeouts**: 30 seconds per request (matches SC-003 end-to-end budget).
- **Redirects**: followed up to 5 hops, all of which must remain HTTPS.

### Authentication failure responses

The source SHOULD return:

- `401 Unauthorized` for missing or invalid credentials.
- `403 Forbidden` for valid credentials lacking access to this source.
- `404 Not Found` for unknown SHAs / archives.

mikebom-cli maps these to `FetchFailureKind::InvalidCredential` (401/403) or `FetchFailureKind::NetworkUnreachable` (404, treated as transient) for the SC-005 actionable-error contract.

## Archive shape

Each `<sha>.tar.gz` archive contains, at the archive root:

```text
archive-root/
├── records/
│   ├── <record-id-1>.json     # One v2 record per file, matching corpus-record-v2.schema.json
│   ├── <record-id-2>.json
│   └── ...
├── index.json                  # Optional but recommended; same shape as below
└── VERSION                     # Plain-text: "2" (the corpus schema version)
```

### index.json (optional)

```json
{
  "schema_version": 2,
  "records": [
    { "record_id": "openssl-3.1.4-glibc-amd64", "path": "records/openssl-3.1.4-glibc-amd64.json", "primary_purl": "pkg:github/openssl/openssl@openssl-3.1.4" }
  ]
}
```

When present, the index lets mikebom-cli avoid scanning every JSON file at load time. Absent → mikebom walks `records/*.json`.

### v1 backward compat

A milestone-108 v1 archive at the same URL is detected by mikebom via the `VERSION` file (absent or `1`) and the loader switches into compatibility mode (per spec FR-005). v1 archives MUST NOT carry a `VERSION` file claiming `2`.

## Signature verification

Every archive MUST carry a Sigstore keyless OIDC signature:

- `<archive>.sig` — DSSE envelope (binary).
- `<archive>.cert` — short-lived Fulcio-issued certificate (PEM).

mikebom-cli verifies the signature using the milestone-089/108 sigstore stack:

1. Fetch all three files.
2. Compute SHA-256 of the archive blob.
3. Call `sigstore::verify_blob()` with the cert + signature + blob hash.
4. Extract the identity from the verified certificate (the OIDC issuer + subject).
5. Match against the source's configured `allowed_issuers` list (per research R6).
6. On mismatch → reject the archive; log `FetchFailureKind::SignatureFailure` with the actual identity for operator triage.

### allowed_issuers format

`allowed_issuers` is a list of glob patterns matching the Sigstore certificate's `subject` (the GitHub Actions identity URL or other OIDC identity). Examples:

- `https://github.com/kusari-sandbox/mikebom-fingerprints/.github/workflows/release.yml@refs/tags/*` — the milestone-108 default.
- `https://github.com/kusari/mikebom-corpus-private/.github/workflows/release.yml@refs/tags/*` — a hypothetical private vendor.
- `https://internal-idp.example.com/oidc/build-server/*` — a corporate IdP (once Sigstore supports more issuers).

Empty `allowed_issuers` → mikebom requires the milestone-108 default (safe default; explicit per-source override required for any other issuer).

## Caching

Per research R3:

- Per-source subdir keyed on 16-char BASE32 hash of the source URL.
- Per-pinned-SHA subdir inside each source.
- 24-hour TTL via `last_used.touch` mtime; expired entries trigger re-fetch (which may produce the same SHA and reuse the existing dir).
- `--force` bypasses the TTL.

## Conformance test

A reference test corpus author can validate their server by running:

```bash
# Start a test corpus server on port 8443
./scripts/fixture-corpus-server --port 8443 --root /path/to/corpus/

# Configure mikebom against it
export MIKEBOM_FINGERPRINTS_SOURCES="https://localhost:8443/release.json"
export TEST_CORPUS_TOKEN="test-token-value"

mikebom fingerprints fetch --force
# Expected: archive fetched, signature verified, records loaded, exit 0
```

(The `fixture-corpus-server` script is part of the milestone-110 test harness.)
