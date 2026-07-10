# Contract: CLI Flags

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md)

## Scope

Adds three new flags to the `mikebom sbom scan` subcommand. All three are opt-in and Backward-compatible: existing invocations without any of the flags produce byte-identical behavior (SC-004).

## Flag Signatures

### `--insecure-registry <HOST[:PORT]>`

**Type**: Repeatable string, each value a `HOST` or `HOST:PORT` form.

**Clap attribute**:
```rust
#[arg(long = "insecure-registry", value_name = "HOST[:PORT]", action = clap::ArgAction::Append)]
pub insecure_registry: Vec<String>,
```

**Semantics** (spec FR-001, FR-004, FR-005):
- Each value is parsed into a `HostMatcher` at scan startup (fail-fast on malformed values per FR-014).
- When mikebom builds a manifest or blob URL for a registry, it consults the composed `InsecureRegistryMatcher` — if any matcher matches on `(host, port)`, the URL uses `http://` instead of `https://`.
- Host-only form matches any port on that host. Explicit `HOST:PORT` matches only that exact host+port.
- The match target is the user-facing registry name (what the operator typed in `--image`), NOT the resolved endpoint. `docker.io` in the flag DOES NOT auto-expand to `registry-1.docker.io`.

**Failure Modes (fail-fast at scan startup)**:
- Empty value → `--insecure-registry value is empty`
- `:port` with no host → `--insecure-registry `:8080` has empty host`
- Non-numeric port → `--insecure-registry `foo:xyz`: port `xyz` is not a valid u16 (expected 1..=65535)`

### `--registry-ca-cert <PATH>`

**Type**: Repeatable path.

**Clap attribute**:
```rust
#[arg(long = "registry-ca-cert", value_name = "PATH", action = clap::ArgAction::Append)]
pub registry_ca_cert: Vec<std::path::PathBuf>,
```

**Semantics** (spec FR-002, FR-006):
- Each path is loaded at scan startup as a PEM file. May contain multiple concatenated certificates (a "bundle"); ALL are extracted.
- Every parsed `reqwest::Certificate` is passed to `reqwest::ClientBuilder::add_root_certificate` — additive to the webpki root set (nothing is removed).
- Fail-fast on load failures — no network call until every cert is loaded successfully.

**Failure Modes (fail-fast at scan startup, before network)**:
- Non-existent file → `reading --registry-ca-cert file `/path/to/x`: No such file or directory (os error 2)`
- File exists but no PEM certs → `no PEM certificates found in --registry-ca-cert file `/path/to/x` (expected one or more \`-----BEGIN CERTIFICATE-----\` blocks)`
- Malformed PEM → `parsing PEM in --registry-ca-cert file `/path/to/x`: <underlying rustls-pemfile error>`
- PEM parses but content is not a valid X.509 certificate → `decoding certificate from --registry-ca-cert file `/path/to/x`: <underlying reqwest::Certificate::from_der error>`

### `--insecure-tls-skip-verify`

**Type**: Boolean flag.

**Clap attribute**:
```rust
#[arg(long = "insecure-tls-skip-verify")]
pub insecure_tls_skip_verify: bool,
```

**Semantics** (spec FR-003, FR-007, FR-008):
- When set, mikebom's `reqwest::Client` is built with `.danger_accept_invalid_certs(true)` — cert chain validation, hostname verification, and expiry checks are ALL disabled for HTTPS pulls in this scan.
- A WARN-level structured log MUST fire at scan start (before any network activity), naming the affected image ref and the flag state (per Constitution Principle X + FR-007).
- Combined with `--registry-ca-cert`: skip-verify wins (skip-verify is a superset of "trust the additional CA"). The `--registry-ca-cert` flag is NOT rejected — it's silently overridden.
- Combined with `--insecure-registry` on the same host: plain-HTTP wins (skip-verify is moot when there's no TLS handshake).

**No failure modes at parse time** — it's a boolean.

## Error-Message Templates (FR-014)

When mikebom hits a TLS/transport failure at pull time (NOT at flag-parse time), the error message MUST name the fix flag(s). Templates:

**Case 1: Plain-HTTP registry contacted without `--insecure-registry`**:
```
TLS handshake failed for GET https://core:8080/v2/library/foo/manifests/latest.
If this registry uses plain HTTP, pass --insecure-registry core:8080.
Underlying error: <reqwest::Error text>
```

**Case 2: Private-CA registry contacted without `--registry-ca-cert`**:
```
TLS certificate chain validation failed for GET https://harbor.example.com/v2/foo/manifests/latest.
For private-CA registries, pass --registry-ca-cert <path-to-ca.pem>.
For self-signed dev/CI certs, pass --insecure-tls-skip-verify (unsafe for production).
Underlying error: <rustls::Error text>
```

**Case 3: Any other transport-layer failure** — unchanged path (existing generic error).

## Backward Compatibility (SC-004)

A scan invocation with NONE of the three flags set:
- `RegistryTlsConfig::default()` produces:
  - `insecure_matcher: InsecureRegistryMatcher::default()` (empty vec) — `matches()` returns false for every URL → `manifest_url`/`blob_url` produce `https://` (unchanged)
  - `ca_bundle: Vec::new()` — no `.add_root_certificate()` calls — trust store remains webpki-only (unchanged)
  - `skip_verify: false` — no `.danger_accept_invalid_certs(true)` call (unchanged)

The `reqwest::Client` construction is byte-identical to pre-m182 for the default path. The `manifest_url`/`blob_url` scheme decision reduces to `https://` when no insecure matcher is set. Every downstream code path (auth, blob fetch, manifest parse, SBOM emission) is untouched.

## Test Coverage

- Unit tests in `scan_cmd.rs::tests` for each flag's clap-parse behavior + parse errors
- Unit tests in `tls_config.rs` for `HostMatcher::parse`, `InsecureRegistryMatcher::matches`, `load_ca_bundle_from_paths` (see data-model.md §5)
- Integration tests per user story (see data-model.md §5 + tasks.md)
