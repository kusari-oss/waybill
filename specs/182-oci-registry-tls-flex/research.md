# Research: OCI registry TLS + transport flexibility (m182)

**Date**: 2026-07-10
**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Decision 1 — `RegistryTlsConfig` threading pattern

**Chosen**: Add a new struct-typed parameter to `pull_to_tarball` alongside the existing `creds_dir: Option<&Path>` argument. `RegistryClient::new` receives the same value.

**Rationale**: Matches the m034/#66 pattern exactly (per-scan config threaded from `ScanArgs` through `pull_to_tarball` to `RegistryClient::new`). Immutable per-scan value — safe against Principle IV (Type-Driven Correctness). Cheap to reason about: `ScanArgs → RegistryTlsConfig → RegistryClient` is a linear data flow with no shared state, no thread-local, no dynamic dispatch.

**Alternatives considered**:
- **Global mutable state** (`static` or `OnceLock`): rejected, violates Principle IV and complicates test isolation
- **Thread-local**: rejected, adds test-isolation surface for no benefit
- **Callback-based scheme resolver** (`Box<dyn Fn(&str) -> Scheme>`): rejected, over-engineered for a static config
- **Extend the reference struct** to carry per-URL transport state: rejected — the reference struct describes image identity, not transport policy

**Threading contract**:
```
ScanArgs::validated_registry_tls_config()  // clap-parsed, validated
    → pull_to_tarball(image_ref, image_platform, cache_cap, creds_dir, tls_config)
    → RegistryClient::new(reference, cache, creds_dir, tls_config)
```

## Decision 2 — PEM parsing: multi-cert bundles

**Chosen**: `reqwest::Certificate::from_pem` (already in dep graph via `reqwest`) — verify at Phase 5 (implementation) that it handles multi-cert PEM bundles per FR-006.

**Rationale**: `reqwest::Certificate::from_pem` is the API mikebom already indirectly uses. Docs say it handles a single certificate; multi-cert bundles may need manual iteration.

**Fallback**: iterate PEM blocks using `rustls-pemfile::certs`:
```
let file = std::fs::File::open(&path)?;
let mut reader = std::io::BufReader::new(file);
for cert_result in rustls_pemfile::certs(&mut reader) {
    let cert_der = cert_result?;
    let cert = reqwest::Certificate::from_der(&cert_der)?;
    builder = builder.add_root_certificate(cert);
}
```

`rustls-pemfile` is available transitively via `rustls` (verify at Phase 5; if not directly exposed, add as a workspace dep — pure Rust, no C, MIT-licensed).

**Alternatives considered**:
- **Split PEM manually via string search for `-----BEGIN CERTIFICATE-----`**: rejected — fragile, doesn't handle edge cases (comments, whitespace, base64 line wrapping variants)
- **`x509-parser` crate for full PEM handling**: rejected — mikebom already depends on `x509-parser 0.16` for the sigstore path (see `mikebom-cli/Cargo.toml`); reusing it here is fine but overkill for cert extraction — `rustls-pemfile` is lighter-weight

**Failure modes to test**:
- Non-existent file → `std::io::Error` mapped to actionable "file not found: {path}" via `anyhow::Context`
- File exists but contains no PEM certificates (empty, non-PEM content) → "no PEM certificates found in {path}"
- File contains malformed PEM (broken base64, corrupted block) → "invalid PEM at {path}: {underlying error}"
- Cert is PEM-valid but not a certificate (private key, CSR) → "PEM at {path} contains non-certificate data"

## Decision 3 — `HostMatcher` semantic

**Chosen**: `HostMatcher::HostOnly(String) | HostMatcher::HostPort(String, u16)`.

**Parse rules** (from `--insecure-registry <val>`):
- `val` contains no `:` → `HostOnly(val)`
- `val` matches `<host>:<port>` where `<port>` parses as `u16` → `HostPort(host, port)`
- Any other input → parse error at CLI time (fail-fast per FR-014)

**Match semantics** (`InsecureRegistryMatcher::matches(host, port)`):
- For each `HostMatcher` in the config:
  - `HostOnly(h)` matches any port on host `h`
  - `HostPort(h, p)` matches only exactly `(h, p)`

**Rationale**: Direct translation of podman's `[[registry]] location = "..."` semantics. Explicit `:port` means "only this port"; host-only means "any port on this host". Matches operator mental model.

**Alternatives considered**:
- **Glob patterns** (`*.example.com`): rejected — extra parsing surface for no strong Harbor use case; can add in follow-up
- **Regex**: rejected — same reason, plus DoS risk on operator-supplied patterns
- **Match on registry name INCLUDING resolved endpoint** (e.g., `docker.io` triggers `registry-1.docker.io` insecure): rejected per spec FR-005 — matches on user-facing name, not resolved endpoint

**Edge case — port normalization**: URLs sometimes carry `:443` explicitly, sometimes not. mikebom's `manifest_url` currently builds `https://{registry}/...` — if `{registry}` includes an explicit port, it's preserved; otherwise HTTPS default (443) applies. The matcher compares on the STRING PORT extracted from the URL, not the normalized default. A `--insecure-registry core:8080` config MUST match a URL that includes `core:8080`. If the operator declares `--insecure-registry core` (no port), it matches both `core:8080` and any other port.

## Decision 4 — URL-scheme policy in `--image`

**Chosen**: `--image http://...` OR `--image https://...` does NOT implicitly enable insecure or skip-verify. The scheme is a hint for reference parsing only; the transport decision comes from the explicit m182 flags.

**Rationale** (spec FR-013): docker's mental model wins over podman's. Podman auto-downgrades to HTTP on registries listed in `registries.conf` as insecure OR when the URL scheme is `http://`; docker requires the daemon config flag. mikebom is a one-shot CLI, not a daemon, so we follow docker's more-conservative "explicit flag required" pattern to avoid surprise operator experience.

**Alternatives considered**:
- **Auto-downgrade on `http://` scheme**: rejected per FR-013
- **Warn but allow when scheme is `http://` without flag**: rejected — surprising and operator error-prone
- **Reject `--image http://...` outright unless `--insecure-registry` matches**: adopted implicitly via FR-014's error message ("the URL scheme suggests HTTP but no `--insecure-registry` matches")

**Implementation note**: `reference::parse_reference` at `oci_pull/mod.rs:93` already tolerates the scheme prefix (see `reference.rs`). No change to reference parsing. The transport-scheme decision is entirely inside `manifest_url` / `blob_url` in `registry.rs`, based on the `InsecureRegistryMatcher` alone.

## Decision 5 — Testing infrastructure

**Chosen**: `wiremock` (already dev-dep since m055) for the HTTP endpoint mock + `rcgen = "0.13"` (NEW dev-dep candidate) for throwaway CA cert generation.

**Rationale**:
- `wiremock` is proven in mikebom's test suite (m055 introduced it for the Go proxy fetch fixtures) — mocks HTTP endpoints, verifies request shapes, no network egress
- `rcgen` is pure-Rust, no C toolchain requirement, in-process cert generation. Alternative would be shelling out to `openssl req -x509 -newkey rsa:2048 ...` in test setup — reproducible on all CI runners (macOS + Linux ship with openssl by default) but adds a filesystem-tempfile step vs. `rcgen`'s in-memory generation.

**Verification**: Phase 0 confirmed `rcgen` is NOT already transitively present in the dep graph (checked `Cargo.lock` — no `rcgen` entry). Adding it would be a legitimate new dev-dep. Verify at Phase 1 that the Cargo.toml change stays dev-only (`[dev-dependencies]` section, not `[dependencies]`).

**Fallback (if dev-dep addition is contested at review)**: shell out to `openssl req -x509 -newkey rsa:2048 -sha256 -days 365 -nodes -keyout <key> -out <cert> -subj "/CN=test"` inside test setup. Slightly more code + brittle-on-Windows, but zero new Cargo dep. Decision: propose `rcgen` at review; accept the fallback if pushback arrives.

**wiremock scenarios covered**:
- **US1**: mock server on 127.0.0.1:{ephemeral} serving `GET /v2/repo/manifests/tag` and `GET /v2/repo/blobs/<digest>` over PLAIN HTTP; verify mikebom pulls when `--insecure-registry 127.0.0.1:{port}` is set, fails with actionable error when it isn't
- **US2**: same server + `rcgen`-generated CA + server-cert; verify mikebom pulls when `--registry-ca-cert /tmp/ca.pem` is set, fails when it isn't
- **US3**: `rcgen`-generated server cert with intentional CN mismatch OR expired timestamp; verify mikebom pulls when `--insecure-tls-skip-verify` is set, fails when it isn't; verify WARN log fires
- **US4**: three mock servers (public-style, private-CA-style, plain-HTTP-style) — one scan invocation with all three flags — each server pulled correctly

## Decision 6 — Delivery cadence

**Chosen**: All 4 US ship in one PR. Single-file scope changes (`scan_cmd.rs` + `oci_pull/registry.rs` + `oci_pull/mod.rs` + new `tls_config.rs`) + 5 integration test files.

**Rationale**: The three flags all thread through the SAME `RegistryClient::new` integration point. Splitting would create a temporary state where the file is half-migrated to the new signature — not worth the overhead for what should be ~25-30 tasks. US4 composition guard cannot be tested in isolation from US1/US2/US3.

**Fallback**: none needed — if implementation surprises arise, the tasks are naturally split by user story with independent test criteria; individual US commits can land incrementally on the same branch.

## Open Questions

None. All design decisions resolved.

## Alternatives Considered (Not Adopted)

- **Persistent config file** (`~/.mikebom/registries.toml`): rejected per spec Assumption 1 — mikebom's existing CLI posture is per-invocation flags only. Adding a config file is a philosophy shift, deferred to follow-up if operator feedback demands it.
- **mTLS client-cert authentication**: deferred per spec Deferred section. Different code path (client cert presentation, not server cert trust); no Harbor-blocker evidence yet.
- **Per-image flag scoping** (`--image X --tls-verify=false --image Y`): deferred per spec Assumption 3 — global-per-invocation is simpler; multi-image scans are rare.
- **DER cert format**: rejected per spec Assumption 4 — PEM covers all documented Harbor/prod-CA distribution mechanisms.
- **Global `--tls-verify=false` inverting semantic**: rejected — `--insecure-tls-skip-verify` is more explicit about the security tradeoff. Peer tools (podman `--tls-verify=false`) have caused operator confusion; the explicit "insecure" prefix is clearer.
