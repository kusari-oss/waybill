# Configuration

mikebom has no configuration file today. Everything is set via CLI flags
or environment variables. This page documents every operator-visible
environment variable mikebom reads at runtime, plus the global flag
surface and the offline-mode contract.

For per-flag operator documentation see [CLI reference](cli-reference.md).
For the deeper rationale on offline scope semantics see
[Architecture: enrichment](../architecture/enrichment.md).

## Global flags

Global flags apply to every subcommand. They can be passed before the noun
(`mikebom --offline sbom scan ...`) or after it (`mikebom sbom scan
--offline ...`); clap's parser is position-tolerant.

| Flag | Env var | Description |
|---|---|---|
| `--offline` | — | Disables all outbound HTTP calls (deps.dev, ClearlyDefined). The scanner still produces a complete SBOM from local sources. |
| `--exclude-scope <SCOPE>` | — | Drop components whose lifecycle scope matches any listed value. Valid: `dev`, `build`, `test`. Comma-separated; runtime always retained. |
| `--include-declared-deps` | — | Include declared-but-not-on-disk dependencies (manifest SBOM mode). Auto-on for `--path`; explicit for `--image`. |
| `--include-legacy-rpmdb` | `MIKEBOM_INCLUDE_LEGACY_RPMDB=1` | Read legacy Berkeley-DB rpmdb on pre-RHEL-8 / CentOS-7 / Amazon-Linux-2 images. |

## Environment variables

mikebom reads the following environment variables at runtime.

### Production-runtime env vars

These affect actual scan / trace / verify behavior.

| Var | Accepted values | Default | Purpose |
|---|---|---|---|
| `MIKEBOM_INCLUDE_LEGACY_RPMDB` | `1` (any non-empty value enables) | unset | Equivalent to the `--include-legacy-rpmdb` flag. Enables BDB-format rpmdb reading on legacy RHEL/CentOS images. |
| `MIKEBOM_OFFLINE` | `1` (any non-empty value enables) | unset | Equivalent to the `--offline` flag. Disables all outbound HTTP. Useful for CI lanes that should never touch the network. |
| `MIKEBOM_OCI_CACHE` | `0` to disable; unset to enable | enabled | Disable the on-disk OCI blob cache for registry pulls. Equivalent to `--no-oci-cache`. |
| `MIKEBOM_OCI_CACHE_DIR` | absolute path | XDG cache convention | Override the OCI blob cache directory. Resolved before `XDG_CACHE_HOME` when set non-empty. |
| `MIKEBOM_OCI_CACHE_SIZE` | bytes (decimal integer) | `10737418240` (10 GB) | Cap for the on-disk OCI blob cache. Equivalent to `--oci-cache-size`. |
| `MIKEBOM_NO_DEPRECATION_NOTICE` | `1` (any non-empty value) | unset | Suppresses stderr deprecation warnings emitted by deprecated flags / format ids (e.g., `spdx-3-json-experimental`). Useful in CI logs during a controlled migration. |
| `MIKEBOM_FIXED_TIMESTAMP` | RFC 3339 timestamp | unset | Pin emission timestamps for reproducible-build pipelines. When set, every emitted SBOM uses this timestamp instead of "now". |

### Logging

| Var | Effect |
|---|---|
| `RUST_LOG=<filter>` | Set the `tracing` log filter. Default `info`. Useful values: `debug` (verbose), `mikebom_cli=trace` (very verbose, mikebom-only). Logs go to stderr. |
| `MIKEBOM_WALKER_DEBUG` | When `1`, emit per-directory walker stats during filesystem scans. Used to investigate symlink-loop / large-tree issues. |

### Tool-cache discovery (`mikebom trace capture --auto-dirs`)

These are read by the trace-mode auto-dir detector to resolve canonical
build-tool cache paths. mikebom does NOT modify these env vars; it only
reads them.

| Var | Used for |
|---|---|
| `HOME` | Default location for many caches (`$HOME/.cargo/registry/cache`, etc.). |
| `CARGO_HOME` | Override the Cargo cache location (defaults to `$HOME/.cargo`). |
| `GOPATH` / `GOMODCACHE` | Locate Go module cache (`$GOPATH/pkg/mod`). |
| `VIRTUAL_ENV` | Detect Python virtualenv directories. |

### CI-lane env vars

These flip behavior in CI but are inappropriate for normal operator use.

| Var | Accepted values | Purpose |
|---|---|---|
| `MIKEBOM_REQUIRE_SPDX3_VALIDATOR` | `1` (strict mode) | When set, the SPDX 3 conformance gate (the JPEWdev `spdx3-validate` integration) is REQUIRED to be present and pass. Without this var, the gate runs when the validator is on `$PATH` and silently skips otherwise. CI lanes that strictly enforce SPDX 3 conformance set this; local-dev workflows leave it unset. |
| `MIKEBOM_REQUIRE_TRANSITIVE_PARITY` | `1` (strict mode) | When set, the transitive-parity audit suite (`transitive_parity_*` integration tests, milestone 083) REQUIRES trivy 0.69.3 + syft 1.27.0 on `$PATH`. Without this var, tests graceful-skip when the external tools are missing. CI's Linux lane sets this so cross-tool divergence is gated; macOS lane skips entirely (OS-package fixtures are Linux-only per the milestone-083 FR-009). |
| `MIKEBOM_PREPR_EBPF` | `1` | Local pre-PR opt-in for the eBPF feature gate. When set, `./scripts/pre-pr.sh` adds `--features ebpf-tracing` to clippy and test invocations. Linux only. |

### Test-side env vars (golden regeneration)

These are recognized by the test harness, NOT by the production binary. They
exist for maintainer workflows — golden regeneration after intentional
output changes — and operators should not need to set them.

| Var | Accepted values | Purpose |
|---|---|---|
| `MIKEBOM_UPDATE_CDX_GOLDENS` | `1` | Regenerate CycloneDX 1.6 byte-identity goldens during `cargo test --workspace`. Active only inside the `cdx_regression` test target. |
| `MIKEBOM_UPDATE_SPDX_GOLDENS` | `1` | Regenerate SPDX 2.3 byte-identity goldens during `cargo test --workspace`. Active only inside the `spdx_regression` test target. |
| `MIKEBOM_UPDATE_SPDX3_GOLDENS` | `1` | Regenerate SPDX 3.0.1 byte-identity goldens. Active only inside the `spdx3_regression` test target. |

### OCI / docker integration test env vars

| Var | Purpose |
|---|---|
| `MIKEBOM_OCI_AUTH_TESTS` / `MIKEBOM_OCI_AUTH_PRIVATE_IMAGE_REF` | Gate registry-auth integration tests; require live access to a private registry. |
| `MIKEBOM_OCI_NETWORK_TESTS` | Gate network-touching OCI tests. |
| `MIKEBOM_SKIP_DOCKER_INTEGRATION` | Skip docker-CLI integration tests when set. |
| `MIKEBOM_PERF_IMAGE` | Override the image used by the performance-bench fixture. |
| `MIKEBOM_SBOMQS_BIN` | Override the path to the `sbomqs` binary used by external-tool comparison fixtures. |

## Offline mode semantics

Under `--offline` (or `MIKEBOM_OFFLINE=1`), mikebom disables:

- **deps.dev license enrichment** — no license lookups, no external
  references resolved online.
- **ClearlyDefined concluded licenses** — no `concluded_licenses[]`
  enrichment.
- **deps.dev transitive dep-graph** — Maven transitive edges from shaded
  JARs or cold `~/.m2` caches are not filled in.
- **Hash resolution via deps.dev** — the resolution pipeline's hash-match
  step is skipped.
- **OCI registry pulls** — `--image-src remote` becomes a hard error;
  only locally-cached images via `--image-src docker` are scanned.

Offline mode still produces a complete SBOM with:

- Every component declared by local lockfiles, installed-package DBs, and
  manifests.
- All declared licenses from local manifests (`dpkg copyright`,
  `Cargo.toml` `license` field, `package.json` `license` field).
- All component hashes provided by local sources (`Cargo.lock` `checksum`,
  `package-lock.json` `integrity`, Maven sidecar `.jar.sha512`, PyPI
  `requirements.txt --hash=`).
- Full dependency graph from installed-package DBs and lockfiles that
  encode the tree.

What changes under offline: licenses for cargo crates drop sharply because
crates.io doesn't publish licenses into `Cargo.lock` — they only come
through the deps.dev enrichment pass. License coverage for npm, pip, gem,
and Maven is largely unaffected because their manifests carry license info
locally.

## Permission model

- **`mikebom trace capture` / `mikebom trace run`** require Linux kernel
  ≥ 5.8 and eBPF privilege — root, `--privileged` container, or
  CAP_BPF + CAP_PERFMON.
- **`mikebom sbom scan` / `mikebom sbom verify` / `mikebom sbom enrich` /
  `mikebom sbom verify-binding` / `mikebom sbom trace-binding` /
  `mikebom policy init`** run unprivileged on any platform Rust compiles on.
- mikebom never writes outside its explicitly specified output paths
  (default: CWD). It does not modify the directories it scans.
- Network behavior: `mikebom sbom scan` and `mikebom sbom enrich` make
  outbound HTTP calls for enrichment by default. `--offline` disables
  these. `mikebom sbom verify` makes outbound calls only for transparency
  log verification (`--no-transparency-log` disables).
