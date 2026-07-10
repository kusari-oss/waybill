# Contract: `RegistryTlsConfig` Threading

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md)

## Scope

Documents the threading contract for `RegistryTlsConfig` from CLI-parse time to the HTTP client construction site.

## Layer Diagram

```
┌──────────────────────────────────────────────────────────────┐
│ Layer 1: CLI (mikebom-cli/src/cli/scan_cmd.rs)              │
│                                                              │
│   ScanArgs {                                                 │
│     insecure_registry: Vec<String>,                          │
│     registry_ca_cert: Vec<PathBuf>,                          │
│     insecure_tls_skip_verify: bool,                          │
│     ... (existing fields)                                    │
│   }                                                          │
│                                                              │
│   → RegistryTlsConfig::from_args(                            │
│         &args.insecure_registry,                             │
│         &args.registry_ca_cert,                              │
│         args.insecure_tls_skip_verify,                       │
│     )?                                                       │
│     — validates + PEM-loads at scan startup                  │
│     — fails-fast per FR-014 (before any network)             │
└──────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│ Layer 2: pull_to_tarball (oci_pull/mod.rs)                  │
│                                                              │
│   pub async fn pull_to_tarball(                              │
│       image_ref: &str,                                       │
│       image_platform: Option<&str>,                          │
│       cache_size_cap: Option<u64>,                           │
│       creds_dir: Option<&Path>,                              │
│       tls_config: &RegistryTlsConfig,   ← NEW m182           │
│   ) -> Result<TempDir>                                       │
│                                                              │
│   Passes `tls_config` UNMODIFIED to RegistryClient::new.     │
└──────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│ Layer 3: RegistryClient (oci_pull/registry.rs)              │
│                                                              │
│   pub(super) fn new(                                         │
│       reference: &ImageReference,                            │
│       cache: Option<Cache>,                                  │
│       creds_dir: Option<&Path>,                              │
│       tls_config: &RegistryTlsConfig,   ← NEW m182           │
│   ) -> Result<Self>                                          │
│                                                              │
│   Two consumptions:                                          │
│                                                              │
│   (A) reqwest::Client construction:                          │
│       let mut builder = Client::builder().user_agent(...);   │
│       for cert in &tls_config.ca_bundle {                    │
│           builder = builder.add_root_certificate(cert.clone()); │
│       }                                                      │
│       if tls_config.skip_verify {                            │
│           builder = builder.danger_accept_invalid_certs(true); │
│           tracing::warn!(...);  // FR-007                    │
│       }                                                      │
│       let http = builder.build()?;                           │
│                                                              │
│   (B) URL-scheme decision (later, per request):              │
│       manifest_url(&reference, &self.tls_config)             │
│       blob_url(&reference, digest, &self.tls_config)         │
└──────────────────────────────────────────────────────────────┘
```

## Threading Guarantees

1. **Immutability**: `RegistryTlsConfig` is constructed once at Layer 1 and passed by shared reference through Layer 2 into Layer 3, where it is cloned onto the `RegistryClient` struct. No mutation after construction. Safe against concurrent scans (mikebom currently single-scan per invocation, but the pattern extends trivially).

2. **Fail-fast semantics** (FR-014): `RegistryTlsConfig::from_args` returns `anyhow::Result<Self>` — parse errors on `--insecure-registry` and PEM load failures on `--registry-ca-cert` propagate up to the CLI dispatcher BEFORE any network call. The Harbor devenv scenario (SC-001) fails with an actionable error IF the operator forgot the flag; it does NOT succeed silently OR crash cryptically.

3. **Default-mode zero-overhead**: `RegistryTlsConfig::default()` produces empty vecs + `skip_verify: false`. Layer 3 sees:
   - Zero `add_root_certificate` calls (webpki-only trust — unchanged)
   - No `danger_accept_invalid_certs` call (unchanged)
   - `manifest_url`/`blob_url` return `https://` (unchanged, `matches` returns false on empty matcher)
   Byte-identical to pre-m182 for SC-004.

4. **No global state**: The config lives only on the stack of `pull_to_tarball` and (cloned) on the `RegistryClient` instance. Zero `static`, zero `OnceLock`, zero thread-local.

## Extension Point (Future Milestones)

If a fourth transport-config flag emerges from the Harbor team's testing (mTLS client cert, custom User-Agent override, per-host bearer-token override, etc.), the extension is a one-liner: add a field to `RegistryTlsConfig`, extend `from_args` to populate it from a new `ScanArgs` field, and consume in Layer 3. The threading contract stays identical.

This is the exact same pattern m034/#66 established with `creds_dir` — m182 formalizes it into a struct so future extensions can pack more fields without growing the function signature.

## Cross-References

- CLI flag definitions: [cli-flags.md](./cli-flags.md)
- Type definitions + parsing: [../data-model.md](../data-model.md)
- Peer-tool precedent: [../research.md](../research.md) Decisions 1-6
