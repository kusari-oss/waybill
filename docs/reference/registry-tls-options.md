# Registry TLS + transport options (mikebom sbom scan --image)

Milestone 182 adds three flags to `mikebom sbom scan` that control
how mikebom contacts an OCI registry. All three are opt-in — the
default behavior (webpki-only trust, HTTPS everywhere, full
verification) is unchanged.

| Flag | Purpose | Repeat? |
|---|---|---|
| `--insecure-registry <HOST[:PORT]>` | Contact this registry over plain HTTP instead of HTTPS. | yes |
| `--registry-ca-cert <PATH>` | Trust an additional CA (PEM file, may contain a bundle). Additive to webpki-roots. | yes |
| `--insecure-tls-skip-verify` | Disable TLS chain / hostname / expiry checks for ALL HTTPS pulls in this scan. Emits a WARN log. | no (boolean) |

Complements — does not replace — the existing
[`--registry-credentials-dir`](./identifiers.md#credential-resolution)
flag from milestone 034 (K8s `imagePullSecrets` mount).

## When to use which flag

| Situation | Flag |
|---|---|
| Registry uses plain HTTP (Harbor devenv, dev clusters, air-gapped mirrors) | `--insecure-registry <host[:port]>` |
| Registry uses HTTPS with a private CA (production internal Harbor, company Nexus, etc.) | `--registry-ca-cert <path-to-pem>` |
| Registry uses HTTPS with a broken cert (self-signed, expired, hostname mismatch) AND you don't have or want the CA | `--insecure-tls-skip-verify` |
| Registry uses HTTPS with a public CA (Docker Hub, GHCR, gcr.io, ECR) | no m182 flags needed — default webpki trust works |

**Prefer `--registry-ca-cert` over `--insecure-tls-skip-verify`** when
both would work. Skip-verify bypasses ALL cert validation (including
hostname mismatch and expiry). Trusting a specific CA is much safer —
it's what production Harbor deployments actually need.

## Worked examples

### Harbor devenv (plain HTTP)

Harbor's local docker-compose devenv exposes its registry API at
`http://core:8080`.

```bash
mikebom sbom scan \
    --image core:8080/library/my-app:1.0 \
    --insecure-registry core:8080 \
    --format cyclonedx-json \
    --output mikebom.cdx.json
```

The flag is repeatable — declare each host you want to reach over
plain HTTP. The match is on the user-facing name (what you typed in
`--image`), not any resolved endpoint. Host-only form (e.g.
`--insecure-registry core`) matches any port on that host.

### Private-CA Harbor (production)

Company-internal Harbor at `https://harbor.internal.example.com`
signs its cert with a corporate root CA that's NOT in the webpki
bundle.

```bash
mikebom sbom scan \
    --image harbor.internal.example.com/team-a/service:2.3.0 \
    --registry-ca-cert /etc/ssl/company-root-ca.pem \
    --format spdx-2.3-json \
    --output mikebom.spdx.json
```

The flag is repeatable and each file may contain multiple
concatenated certificates (a "bundle" — everything between
`-----BEGIN CERTIFICATE-----` and `-----END CERTIFICATE-----`
blocks). All certs are added to the trust store additively —
webpki-roots remain in place.

### CI/dev instance with self-signed cert

CI pipeline scans an ephemeral Harbor instance with a self-signed
(or expired, or hostname-mismatched) cert.

```bash
mikebom sbom scan \
    --image ci-registry:5000/nightly:abcdef1 \
    --insecure-tls-skip-verify \
    --format cyclonedx-json
```

mikebom emits a **WARN log** at scan start:

```
WARN mikebom::scan_fs::oci_pull::registry: TLS verification DISABLED
  via --insecure-tls-skip-verify — cert chain, hostname, and expiry
  checks are skipped for this scan. Use only for CI/dev against
  self-signed or hostname-mismatched certs. For production private-CA
  registries, use --registry-ca-cert instead. registry=ci-registry:5000
```

The log is intentional — an operator auditing scan logs later can
identify unverified pulls.

### Composition: all three flags in one scan

If one invocation touches multiple registries with different
transport requirements:

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

The skip-verify flag is scan-global — it applies to ALL HTTPS pulls
in this invocation. If that's not what you want, run separate scans.

## Failure diagnostics

mikebom's error messages point at the specific fix flag:

### Case 1 — forgot `--insecure-registry`

```
Error: TLS handshake failed for GET https://core:8080/v2/library/foo/manifests/latest.
       If this registry uses plain HTTP, pass --insecure-registry core:8080.
       Underlying error: <reqwest::Error text>
```

### Case 2 — forgot `--registry-ca-cert`

```
Error: TLS certificate chain validation failed for GET https://harbor.internal.example.com/v2/team-a/service/manifests/2.3.0.
       For private-CA registries, pass --registry-ca-cert <path-to-ca.pem>.
       For self-signed dev/CI certs, pass --insecure-tls-skip-verify (unsafe for production).
       Underlying error: <rustls::Error text>
```

### Case 3 — bad `--registry-ca-cert` path

```
Error: reading --registry-ca-cert file `/etc/ssl/nonexistent.pem`: No such file or directory (os error 2)
```

### Case 4 — `--registry-ca-cert` PEM is malformed

```
Error: no PEM certificates found in --registry-ca-cert file `/etc/ssl/empty.pem`
       (expected one or more `-----BEGIN CERTIFICATE-----` blocks)
```

## Precedence rules

- `--insecure-registry <host>` matches on the **user-facing** registry
  name (the string you typed in `--image`), NOT the resolved endpoint.
  `--insecure-registry docker.io` does NOT match
  `registry-1.docker.io`.
- Host-only form (`example.com`) matches any port on that host.
- `HOST:PORT` form matches only that exact host+port.
- When `--insecure-registry` and `--insecure-tls-skip-verify` both
  match the same host, **plain HTTP wins** (skip-verify is moot when
  there is no TLS handshake).
- `--insecure-tls-skip-verify` is scan-global — it disables
  verification for every HTTPS pull in the scan. There is no
  per-host scoping.
- All three flags coexist with `--registry-credentials-dir` at the
  CLI-parse layer — orthogonal concerns.

## Security guidance

- **Never use `--insecure-tls-skip-verify` in production.** It
  bypasses cert chain, hostname, and expiry checks. An attacker with
  MITM position can present any cert and mikebom will accept it.
- **Prefer `--registry-ca-cert`** over skip-verify whenever the CA
  material is available. It preserves cert validation while trusting
  your private CA.
- **`--insecure-registry` exposes credentials** if the registry
  requires auth. Only use for registries that don't handle sensitive
  data (dev environments, air-gapped mirrors).
- mikebom emits a WARN log every time verification is disabled, so
  audit-trail workflows can catch unintended use.

## Extending: mTLS, per-image scoping, config files

Deferred to future milestones (see
`specs/182-oci-registry-tls-flex/spec.md` §Deferred):

- mTLS client-cert auth to the registry (`--registry-client-cert`)
- Persistent config file (`~/.mikebom/registry.toml`)
- Per-image (vs. per-invocation) flag scoping
- DER-format CA input (currently PEM only)

## See also

- [`identifiers.md`](./identifiers.md) — SBOM identifier fields
  including `--registry-credentials-dir`
- [`reading-a-mikebom-sbom.md`](./reading-a-mikebom-sbom.md) —
  general SBOM consumption guide
- [`specs/182-oci-registry-tls-flex/`](../../specs/182-oci-registry-tls-flex/)
  — full milestone spec + design decisions
