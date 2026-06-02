# Contract: Network fetch protocol

## Target URL

```text
https://github.com/kusari-sandbox/mikebom-fingerprints/archive/<full-40-hex-sha>.tar.gz
```

GitHub's archive-download endpoint resolves a tarball-per-commit by SHA. No auth required for a public repo. Resolves via HTTP 302 to a `codeload.github.com` URL; `reqwest` follows redirects by default.

## Request

- Method: `GET`
- Headers:
  - `User-Agent: mikebom/<version> (corpus-fetch)`
  - `Accept: application/x-gzip` (informational; GitHub ignores)
- Timeout: 30 seconds total wall-clock (covers DNS + connect + transfer).
- Followed redirects: max 5 (GitHub does 1; budget for `codeload.github.com` legacy chains).

## Success behavior

1. Response is HTTP 200 + a gzip-compressed tarball body.
2. Streamed into `flate2::read::GzDecoder` → `tar::Archive`.
3. Extracted per `cache-layout.md`'s atomic-write protocol.
4. Return `CorpusSource::Fetched { sha }` for the loader to consume.

## Failure modes + behavior

| Failure | mikebom-cli behavior |
|---|---|
| DNS resolution fails (no network at all) | `tracing::warn!("corpus fetch network error: {err}")`, fall back to bundled defaults, stamp `bundled` sentinel on SBOM. Scan does NOT abort. |
| HTTP 404 (SHA doesn't exist in the corpus repo) | `tracing::error!`, exit non-zero from the `fingerprints fetch` subcommand (FR-008). If hit during a `sbom scan` cache-miss auto-fetch: fall back to bundled defaults + warn (the operator's `--fingerprints-rev` value was wrong but the scan should still complete). |
| HTTP 5xx (GitHub transient) | Retry up to 3 times with exponential backoff (1s, 2s, 4s). Final failure → same as the no-network case (warn + fall back). |
| HTTP 429 (rate-limited) | Respect `Retry-After` header up to 60s. Final failure → same as 5xx. |
| Gzip decompression fails (corrupt response body) | Same as no-network: warn + fall back. |
| Tar extraction fails (malformed archive) | Same as no-network: warn + fall back. |
| Disk write fails (permissions, ENOSPC) | `tracing::error!` with the specific syscall error. Fall back to bundled defaults; the operator can address the disk issue + retry. |
| `--offline` set AND cache miss | NO fetch attempted. `tracing::warn!("external corpus requested but cache is empty and --offline is set; falling back to bundled defaults")`. SBOM stamps `bundled` sentinel. |

## What we deliberately do NOT do

- **No GitHub API auth flow**. The archive endpoint is unauthenticated for public repos. Adding auth = complexity + secret-handling surface; YAGNI.
- **No mirror / CDN fallback**. If `codeload.github.com` is down, operators with a fresh install fall back to bundled defaults. Air-gapped operators pre-fetched (FR-008) so they're unaffected. The bundled-defaults safety net IS the fallback story.
- **No certificate pinning**. `rustls-tls` uses the OS trust store; that's the established mikebom posture.
- **No partial-file caching**. A failed fetch leaves NOTHING in the cache for that SHA (atomic-write protocol cleans up). The next attempt starts from scratch.
- **No streaming-to-disk during extraction with cap detection**. The tarball is small (~75 KB at 100 libraries); we don't need streaming caps. If the corpus ever grows beyond ~1 MB compressed, we add a `tar::Archive::set_max_size()` guard.

## Bandwidth + latency budget

Per SC-004: ≤5 seconds wall-clock on a typical broadband connection. At a 100-library corpus (~75 KB compressed):

- DNS + TCP + TLS handshake: ~100ms
- TLS-decrypted body transfer at 1 Mbps: ~600ms (worst case)
- Gzip decompression: <50ms
- Tar extraction + per-file writes: <500ms (~100 small files, mostly metadata syscalls)

Total typical: ~1.5 seconds. The 5-second budget has substantial headroom for slow connections.

## Test surface

Network-touching integration tests are gated behind `MIKEBOM_FINGERPRINTS_NETWORK_TESTS=1` so CI's default lane (`cargo +stable test --workspace`) stays offline. The gate-on test fixture uses a real fetch against a known SHA in the seeded sibling repo (`scan_fingerprint_corpus_external.rs`); the gate-off test fixture short-circuits to bundled defaults.

Local-fetch testing uses a `wiremock`-style intercept (no new dep; we can hand-roll a `tokio::net::TcpListener` mock in <50 LOC if needed). Per the existing milestone 055's pattern.
