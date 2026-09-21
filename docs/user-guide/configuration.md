# Configuration

waybill has no configuration file today. Everything is set via CLI flags
or environment variables. This page documents every operator-visible
environment variable waybill reads at runtime, plus the global flag
surface and the offline-mode contract.

For per-flag operator documentation see [CLI reference](cli-reference.md).
For the deeper rationale on offline scope semantics see
[Architecture: enrichment](../architecture/enrichment.md).

## Global flags

Global flags apply to every subcommand. They can be passed before the noun
(`waybill --offline sbom scan ...`) or after it (`waybill sbom scan
--offline ...`); clap's parser is position-tolerant.

| Flag | Env var | Description |
|---|---|---|
| `--offline` | — | Disables all outbound HTTP calls (deps.dev, ClearlyDefined). The scanner still produces a complete SBOM from local sources. |
| `--exclude-scope <SCOPE>` | — | Drop components whose lifecycle scope matches any listed value. Valid: `dev`, `build`, `test`. Comma-separated; runtime always retained. |
| `--include-declared-deps` | — | Include declared-but-not-on-disk dependencies (manifest SBOM mode). Auto-on for `--path`; explicit for `--image`. |
| `--include-legacy-rpmdb` | `WAYBILL_INCLUDE_LEGACY_RPMDB=1` | Read legacy Berkeley-DB rpmdb on pre-RHEL-8 / CentOS-7 / Amazon-Linux-2 images. |

## Environment variables

waybill reads the following environment variables at runtime.

### Production-runtime env vars

These affect actual scan / trace / verify behavior.

| Var | Accepted values | Default | Purpose |
|---|---|---|---|
| `WAYBILL_INCLUDE_LEGACY_RPMDB` | `1` (any non-empty value enables) | unset | Equivalent to the `--include-legacy-rpmdb` flag. Enables BDB-format rpmdb reading on legacy RHEL/CentOS images. |
| `WAYBILL_OFFLINE` | `1` (any non-empty value enables) | unset | Equivalent to the `--offline` flag. Disables all outbound HTTP. Useful for CI lanes that should never touch the network. |
| `WAYBILL_OCI_CACHE` | `0` to disable; unset to enable | enabled | Disable the on-disk OCI blob cache for registry pulls. Equivalent to `--no-oci-cache`. |
| `WAYBILL_OCI_CACHE_DIR` | absolute path | XDG cache convention | Override the OCI blob cache directory. Resolved before `XDG_CACHE_HOME` when set non-empty. |
| `WAYBILL_OCI_CACHE_SIZE` | bytes (decimal integer) | `10737418240` (10 GB) | Cap for the on-disk OCI blob cache. Equivalent to `--oci-cache-size`. |
| `WAYBILL_NO_DEPRECATION_NOTICE` | `1` (any non-empty value) | unset | Suppresses stderr deprecation warnings emitted by deprecated flags / format ids (e.g., `spdx-3-json-experimental`). Useful in CI logs during a controlled migration. |
| `WAYBILL_FIXED_TIMESTAMP` | RFC 3339 timestamp | unset | Pin emission timestamps for reproducible-build pipelines. When set, every emitted SBOM uses this timestamp instead of "now". |

### Logging

| Var | Effect |
|---|---|
| `RUST_LOG=<filter>` | Set the `tracing` log filter. Default `info`. Useful values: `debug` (verbose), `waybill_cli=trace` (very verbose, waybill-only). Logs go to stderr. |
| `WAYBILL_WALKER_DEBUG` | When `1`, emit per-directory walker stats during filesystem scans. Used to investigate symlink-loop / large-tree issues. |

### Tool-cache discovery (`waybill trace capture --auto-dirs`)

These are read by the trace-mode auto-dir detector to resolve canonical
build-tool cache paths. waybill does NOT modify these env vars; it only
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
| `WAYBILL_REQUIRE_SPDX3_VALIDATOR` | `1` (strict mode) | When set, the SPDX 3 conformance gate (the JPEWdev `spdx3-validate` integration) is REQUIRED to be present and pass. Without this var, the gate runs when the validator is on `$PATH` and silently skips otherwise. CI lanes that strictly enforce SPDX 3 conformance set this; local-dev workflows leave it unset. |
| `WAYBILL_REQUIRE_TRANSITIVE_PARITY` | `1` (strict mode) | When set, the transitive-parity audit suite (`transitive_parity_*` integration tests, milestone 083) REQUIRES trivy 0.69.3 + syft 1.27.0 on `$PATH`. Without this var, tests graceful-skip when the external tools are missing. CI's Linux lane sets this so cross-tool divergence is gated; macOS lane skips entirely (OS-package fixtures are Linux-only per the milestone-083 FR-009). |
| `WAYBILL_PREPR_EBPF` | `1` | Local pre-PR opt-in for the eBPF feature gate. When set, `./scripts/pre-pr.sh` adds `--features ebpf-tracing` to clippy and test invocations. Linux only. |

### Test-side env vars (golden regeneration)

These are recognized by the test harness, NOT by the production binary. They
exist for maintainer workflows — golden regeneration after intentional
output changes — and operators should not need to set them.

| Var | Accepted values | Purpose |
|---|---|---|
| `WAYBILL_UPDATE_CDX_GOLDENS` | `1` | Regenerate CycloneDX 1.6 byte-identity goldens during `cargo test`. Honored by the main `cdx_regression` target AND any other test that pins its own CDX golden (e.g., `pkg_alias_binding_us1`). |
| `WAYBILL_UPDATE_SPDX_GOLDENS` | `1` | Regenerate SPDX 2.3 byte-identity goldens during `cargo test`. Honored by the main `spdx_regression` target AND any other test that pins its own SPDX 2.3 golden. |
| `WAYBILL_UPDATE_SPDX3_GOLDENS` | `1` | Regenerate SPDX 3.0.1 byte-identity goldens during `cargo test`. Honored by the main `spdx3_regression` target AND any other test that pins its own SPDX 3 golden. |

To regenerate every golden the workspace can produce in one pass, run
[`./scripts/regen-goldens.sh`](https://github.com/kusari-oss/waybill/blob/main/scripts/regen-goldens.sh).
The wrapper sets all three env vars and runs `cargo test --workspace`, so
per-test pinned goldens outside the three main regression targets are
covered. Do NOT narrow cargo to `--test cdx_regression --test
spdx_regression --test spdx3_regression`; that silently skips the
per-test pinned goldens (see
[issue #361](https://github.com/kusari-oss/waybill/issues/361)).

### OCI / docker integration test env vars

| Var | Purpose |
|---|---|
| `WAYBILL_OCI_AUTH_TESTS` / `WAYBILL_OCI_AUTH_PRIVATE_IMAGE_REF` | Gate registry-auth integration tests; require live access to a private registry. |
| `WAYBILL_OCI_NETWORK_TESTS` | Gate network-touching OCI tests. |
| `WAYBILL_SKIP_DOCKER_INTEGRATION` | Skip docker-CLI integration tests when set. |
| `WAYBILL_PERF_IMAGE` | Override the image used by the performance-bench fixture. |
| `WAYBILL_SBOMQS_BIN` | Override the path to the `sbomqs` binary used by external-tool comparison fixtures. |

## Offline mode semantics

Under `--offline` (or `WAYBILL_OFFLINE=1`), waybill disables:

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

## License enrichment (deps.dev)

waybill fills in licenses the local manifests do not carry by querying
[deps.dev](https://deps.dev). This is the only outbound call a default scan
makes that materially affects the document, and `--offline` disables it.

| Flag | Description |
|---|---|
| `--no-enrich-batch` | Look each package up individually instead of in batches. |
| `--enrich-batch` | Accepted and ignored. Batching is the default; the flag remains so existing scripts keep working. |
| `--enrich-no-cache` | Bypass the on-disk response cache. |
| `--enrich-cache-max-age <SECS>` | Override the cache freshness bound. |

### Why batching is the default

Batched lookups ask for up to 100 packages per request. On a 2,291-package
repository that is **23 requests instead of 2,291** — and measured end to
end, enrichment takes about 6s rather than about 14s.

The request count is the more important number, and the more durable one.
The per-component path's cost scales with per-request round-trip time, so
the time saving shrinks on a fast link and grows on a slow one; the 100x
reduction in load placed on a free public API does not move either way.

### The trade this default accepts

The batch endpoint lives on deps.dev's `v3alpha` surface, which its
publisher documents as liable to "change in incompatible ways from time to
time". A default that depends on an unstable API needs a reason, and the
reason is that it degrades rather than breaks:

- If the batch endpoint fails, waybill falls back to per-component requests
  for the rest of the scan. **Enrichment content is unaffected** — the same
  licenses, the same components, the same edges. The scan is slower.
- It tries the batch endpoint **once**. A persistent failure costs one
  wasted request, not one per batch.
- A scan that fell back says so, in a log line while it runs and in a
  document-scope annotation afterwards, so a slow scan is diagnosable and a
  degraded document is identifiable after the fact.

Pass `--no-enrich-batch` to take the per-component path unconditionally. It
produces the same document; it is there for anyone who needs to avoid the
alpha surface entirely.

A scheduled check watches for the endpoint leaving `v3alpha`
(`.github/workflows/deps-dev-alpha-canary.yml`) — the argument above is only
valid while no stable equivalent exists, and it should be revisited when one
does.

## Repository observation report

`waybill repo report` answers a different question from `sbom scan`: not "what
is in this repository" but **"what did waybill understand, ignore, and fail to
determine here"**.

```sh
waybill repo report --path . --output report.json
```

Use it when a scan produced fewer components than you expected and you cannot
tell whether that is correct. The report names, per directory, which readers
claimed files, which directories nothing recognised, and which could not be
classified at all.

| Flag | Description |
|---|---|
| `--path <DIR>` | Repository to observe. Defaults to `.`. |
| `--output <FILE>` | Where to write. Defaults to stdout. |
| `--redact` | Replace repository-relative path segments with stable identifiers. |
| `--exclude-path <PATH>` | Skip subtrees. Same semantics as `sbom scan`. |

### What the report is for

Three questions it answers directly:

- **Is my project shape supported?** Directories with `claim_status: claimed`
  were recognised. Ones with an ecosystem named and `support: no_reader` are a
  known gap — waybill can see what they are and has no reader for them.
- **Did a reader engage and produce nothing?** `files_matched > 0` with
  `components_emitted: 0` is a parse failure or an unsupported dialect, not a
  coverage gap. The two need opposite responses.
- **What could not be determined?** Directories carrying an `ambiguity` record,
  with the competing interpretations and the evidence behind each.

### Ambiguity is an answer, not a failure

The report does not guess. A directory holding lockfiles from several
ecosystems may be a polyglot project, test fixtures, or vendored examples, and
nothing observable distinguishes them — so it records the ambiguity and the
evidence rather than picking one.

Where a directory cannot be classified at all, it still carries what *was*
observable: file count, depth, whether the contents are predominantly binary or
text, and an extension histogram. A directory of 47 binary files and one of 47
text files are both unclassified and mean very different things.

### Sharing a report

The report retains **repository-relative** directory names by default, because
those are what make it actionable to someone who cannot see your repository.
It never contains absolute paths or any content read from a scanned file, in
either mode.

If directory names are themselves sensitive:

```sh
waybill repo report --path . --redact --output report.json
```

Segments become stable identifiers: nesting depth and repeated segments still
correlate, the names do not survive. Every report states which mode produced
it in `redaction_mode`, so a recipient never has to guess whether a missing
name was absent or removed.

Nothing is ever sent anywhere automatically. Sharing is something you do after
reading the file.

### Comparing two reports

Check `significance_threshold` first. It governs which directories earned their
own record, and **two reports produced under different thresholds are not
comparable**.

To diff two runs, mask the fields the document itself declares volatile rather
than hard-coding a list — that stays correct as the schema grows:

```sh
jq -r '.volatile_fields[]' report.json
```

The schema is versioned `major.minor` and marked **alpha**. Minor bumps are
additive; a major bump means a field was removed, renamed, or changed meaning,
and a consumer should refuse a major it does not recognise rather than guess.

## Permission model

- **`waybill trace capture` / `waybill trace run`** require Linux kernel
  ≥ 5.8 and eBPF privilege — root, `--privileged` container, or
  CAP_BPF + CAP_PERFMON.
- **`waybill sbom scan` / `waybill sbom verify` / `waybill sbom enrich` /
  `waybill sbom verify-binding` / `waybill sbom trace-binding` /
  `waybill policy init`** run unprivileged on any platform Rust compiles on.
- waybill never writes outside its explicitly specified output paths
  (default: CWD). It does not modify the directories it scans.
- Network behavior: `waybill sbom scan` and `waybill sbom enrich` make
  outbound HTTP calls for enrichment by default. `--offline` disables
  these. `waybill sbom verify` makes outbound calls only for transparency
  log verification (`--no-transparency-log` disables).
