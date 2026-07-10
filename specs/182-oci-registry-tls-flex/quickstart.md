# Quickstart: OCI registry TLS + transport flexibility (m182)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Operator flow

### Scenario 1 — scan an image from Harbor's docker-compose devenv (plain HTTP)

Harbor's local devenv exposes its registry API at `http://core:8080`. Pre-m182 mikebom would rewrite this to `https://core:8080/...` and fail at TLS handshake.

Post-m182:

```bash
mikebom sbom scan \
    --image core:8080/library/my-app:1.0 \
    --insecure-registry core:8080 \
    --format cyclonedx-json \
    --output mikebom.cdx.json
```

The `--insecure-registry` flag is repeatable — declare each host you want to reach over plain HTTP. The match is on the user-facing name (what you typed in `--image`), not any resolved endpoint. Host-only form (e.g., `--insecure-registry core`) matches any port on that host.

### Scenario 2 — scan an image from a private-CA Harbor deployment

Company-internal Harbor at `https://harbor.internal.example.com` signs its cert with a corporate root CA that's NOT in the webpki bundle. Pre-m182 mikebom would fail chain validation.

Post-m182:

```bash
mikebom sbom scan \
    --image harbor.internal.example.com/team-a/service:2.3.0 \
    --registry-ca-cert /etc/ssl/company-root-ca.pem \
    --format spdx-2.3-json \
    --output mikebom.spdx.json
```

The `--registry-ca-cert` flag is repeatable and each file may contain multiple concatenated certificates (a "bundle" — everything between `-----BEGIN CERTIFICATE-----` and `-----END CERTIFICATE-----` blocks). All certs are added to the trust store additively — webpki-roots remain in place.

### Scenario 3 — scan against a CI/dev instance with self-signed cert

CI pipeline scans an ephemeral Harbor instance with a self-signed cert (or expired, or hostname-mismatched — anything the standard trust chain rejects). You don't have the exact CA, and fetching it isn't practical.

Post-m182:

```bash
mikebom sbom scan \
    --image ci-registry:5000/nightly:abcdef1 \
    --insecure-tls-skip-verify \
    --format cyclonedx-json
```

**mikebom will emit this WARN log at scan start**:

```
WARN mikebom::scan_fs::oci_pull::registry: TLS verification DISABLED via
  --insecure-tls-skip-verify — cert chain, hostname, and expiry checks are
  skipped for this scan. Use only for CI/dev against self-signed or
  hostname-mismatched certs. For production private-CA registries, use
  --registry-ca-cert instead. image=ci-registry:5000
```

The log is intentional (Constitution Principle X) — an operator auditing scan logs later can identify unverified pulls.

### All three flags together (heterogeneous scan)

If one scan invocation touches multiple registries with different transport requirements:

```bash
mikebom sbom scan \
    --image core:8080/foo:1.0 \
    --image harbor.internal.example.com/bar:2.0 \
    --image ci-registry:5000/baz:3.0 \
    --insecure-registry core:8080 \
    --registry-ca-cert /etc/ssl/company-root-ca.pem \
    --insecure-tls-skip-verify
```

Each registry's transport is decided independently:
- `core:8080` → plain HTTP (matches `--insecure-registry`)
- `harbor.internal.example.com` → HTTPS with company CA in the trust store
- `ci-registry:5000` → HTTPS with skip-verify

The skip-verify flag is scan-global — it applies to ALL HTTPS pulls in this invocation. If that's not what you want, run separate scans.

## When to use which flag

| Situation | Flag |
|---|---|
| Registry uses plain HTTP (Harbor devenv, dev clusters, air-gapped mirrors) | `--insecure-registry <host[:port]>` |
| Registry uses HTTPS with a private CA (production internal Harbor, company Nexus, etc.) | `--registry-ca-cert <path-to-pem>` |
| Registry uses HTTPS with a broken cert (self-signed, expired, hostname mismatch) AND you don't have or want the CA | `--insecure-tls-skip-verify` |
| Registry uses HTTPS with a public CA (Docker Hub, GHCR, gcr.io, ECR) | no m182 flags needed — default webpki trust works |

**Prefer `--registry-ca-cert` over `--insecure-tls-skip-verify`** when both would work. Skip-verify bypasses ALL cert validation (including hostname mismatch and expiry). Trusting a specific CA is much safer — it's what production Harbor deployments actually need.

## Failure diagnostics

mikebom's error messages point at the specific fix (spec FR-014):

### Case 1: forgot `--insecure-registry`

```
Error: TLS handshake failed for GET https://core:8080/v2/library/foo/manifests/latest.
       If this registry uses plain HTTP, pass --insecure-registry core:8080.
       Underlying error: <reqwest::Error text>
```

### Case 2: forgot `--registry-ca-cert`

```
Error: TLS certificate chain validation failed for GET https://harbor.internal.example.com/v2/team-a/service/manifests/2.3.0.
       For private-CA registries, pass --registry-ca-cert <path-to-ca.pem>.
       For self-signed dev/CI certs, pass --insecure-tls-skip-verify (unsafe for production).
       Underlying error: <rustls::Error text>
```

### Case 3: bad `--registry-ca-cert` path

```
Error: reading --registry-ca-cert file `/etc/ssl/nonexistent.pem`: No such file or directory (os error 2)
```

### Case 4: `--registry-ca-cert` PEM is malformed

```
Error: no PEM certificates found in --registry-ca-cert file `/etc/ssl/empty.pem`
       (expected one or more `-----BEGIN CERTIFICATE-----` blocks)
```

## Developer flow — extending `RegistryTlsConfig` in the future

If a future milestone needs to add a fourth transport-config field (e.g., mTLS client cert, custom User-Agent, per-host bearer-token override), the pattern is:

1. **Add a field to `RegistryTlsConfig`** in `mikebom-cli/src/scan_fs/oci_pull/tls_config.rs`:
   ```rust
   pub(crate) struct RegistryTlsConfig {
       // existing fields...
       pub(crate) client_cert: Option<ClientCertConfig>,  // NEW
   }
   ```

2. **Add a matching `ScanArgs` field** in `mikebom-cli/src/cli/scan_cmd.rs`:
   ```rust
   #[arg(long = "registry-client-cert", value_name = "PATH")]
   pub registry_client_cert: Option<PathBuf>,
   ```

3. **Extend `RegistryTlsConfig::from_args`** to populate the new field with fail-fast validation.

4. **Consume the new field in Layer 3** — either in `RegistryClient::new` (client builder config) or in the URL/request handling.

5. **Add a WARN log if the new mode is dangerous** (per Constitution Principle X).

The `pull_to_tarball` signature does NOT change — it still receives `tls_config: &RegistryTlsConfig`, which now carries the new field.

## Cross-references

- Spec: [spec.md](./spec.md)
- CLI flag contracts: [contracts/cli-flags.md](./contracts/cli-flags.md)
- Threading contract: [contracts/registry-tls-config.md](./contracts/registry-tls-config.md)
- Peer-tool audit: [research.md](./research.md)
