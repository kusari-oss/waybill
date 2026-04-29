# CLI reference

mikebom follows a strict `mikebom <noun> <verb>` pattern. Top-level nouns:

- **`sbom`** — SBOM generation, enrichment, verification *(stable)*
- **`policy`** — in-toto layout generation + enforcement *(stable)*
- **`attestation`** — attestation management *(stable)*
- **`trace`** — eBPF build-process tracing *(**experimental**, Linux only)*

> **Experimental** means: the output formats are stable, but the trace-mode
> pipeline adds ~2-3× wall-clock overhead on syscall-heavy builds, requires
> CAP_BPF + CAP_PERFMON, and has coverage gaps on some syscall variants
> (`openat2`, `io_uring`). For most SBOM use cases, prefer `mikebom sbom
> scan` — it produces richer CycloneDX output with no privilege requirements
> and runs on any OS.

Global flags apply to every subcommand and must appear **before** the noun:

```bash
mikebom --offline sbom scan --path .
mikebom --include-dev sbom scan --path .
```

## Global flags

| Flag | Env | Default | Effect |
|---|---|---|---|
| `--offline` | — | off | Disable all outbound network calls (deps.dev, ClearlyDefined). Enrichment falls back to whatever the local filesystem can produce. Useful for air-gapped scanners, reproducible builds, and CI with no internet. |
| `--include-dev` | — | off | Include dev/test/optional dependencies. Affects ecosystems with a dev/prod distinction (npm, Poetry, Pipfile). Included components carry the property `mikebom:dev-dependency = true` so downstream tools can filter them out. |
| `--include-legacy-rpmdb` | `MIKEBOM_INCLUDE_LEGACY_RPMDB=1` | off | Enable reading legacy Berkeley-DB rpmdb (`/var/lib/rpm/Packages`) on pre-RHEL-8 / CentOS-7 / Amazon-Linux-2 images. Default-off preserves milestone-003 behavior (diagnostic log + zero components). The BDB reader itself ships in milestone 004 US4 — the flag threads through today as a no-op until that code lands. |
| `--include-declared-deps` | — | off (`--image`) / on (`--path`) | Include declared-but-not-on-disk dependencies. By default, mikebom emits only components whose bytes are physically present in the scanned tree or image (**artifact SBOM**). When set, also includes: (1) deps.dev-reported transitives not observed locally (`source_type = declared-not-cached`); (2) Maven `pom.xml`-declared direct deps with no matching JAR / `.m2` cache entry (`source_type = workspace`); (3) Maven BFS cache-miss transitives (`source_type = transitive`, no `.pom` on disk). Auto-enabled for `sbom scan --path` scans so source-tree users get a **manifest SBOM** of what *would* be pulled in on build; explicit override for `--image` if you want manifest-style output from a container scan. See `docs/design-notes.md` "Scope: artifact vs manifest SBOM" for the rationale. |

---

## `mikebom trace capture`

**Status:** **Experimental.** Linux-only. On non-Linux hosts this subcommand
errors with a message pointing to Lima / `mikebom-dev`. Adds ~2-3× wall-clock
overhead on syscall-heavy builds; has coverage gaps on `openat2` and
`io_uring`. Prefer `mikebom sbom scan` for most use cases.

Capture a build via eBPF uprobes on `libssl` (`SSL_read` / `SSL_write`) and
kprobes on file operations, produce an in-toto attestation.

```bash
mikebom trace capture --output build.attestation.json -- <command>
mikebom trace capture --target-pid 12345 --output build.attestation.json
```

Exactly one of **`--target-pid <PID>`** or a command after **`--`** is required.
They are mutually exclusive.

| Flag | Default | Purpose |
|---|---|---|
| `--output <path>` | `mikebom.attestation.json` | Attestation output path |
| `--target-pid <pid>` | — | PID to trace (mutually exclusive with `--` command) |
| `--trace-children` | off | Follow forked children of the traced process |
| `--libssl-path <path>` | auto-detect | Override `libssl.so` path for uprobe attachment |
| `--go-binary <path>` | — | Path to a Go binary for Go-specific instrumentation |
| `--ring-buffer-size <bytes>` | `8388608` (8 MB) | BPF ring buffer size (must be power of two) |
| `--timeout <seconds>` | `0` (no timeout) | Abort trace after N seconds |
| `--artifact-dir <path>` | — | Directory to scan for freshly-landed artifact files after the traced command exits. Any recognised package file (`.deb`, `.crate`, `.whl`, `.tar.gz`, …) whose mtime is ≥ trace start is hashed and added to the file-access record. Accepts the flag multiple times or a comma-separated list. |
| `--auto-dirs` | off | Auto-detect artifact directories by matching `argv[0]` against a table of build tools (`cargo` → `$CARGO_HOME/registry/cache`, `pip`, `npm`, `go`, `apt-get`, …). Merges with explicit `--artifact-dir` values; skipped for shell-wrapped commands. |
| `--json` | off | Print a JSON summary to stdout |

The attestation predicate type is
`https://mikebom.dev/attestation/build-trace/v1`; see
[architecture/attestations.md](../architecture/attestations.md) for the schema.

---

## `mikebom trace run`

**Status:** **Experimental.** Linux-only. Same caveats as `trace capture` —
2-3× wall-clock overhead, requires `CAP_BPF + CAP_PERFMON`, coverage gaps on
`openat2` / `io_uring`. Prefer scan-mode unless you specifically need a
trace-backed attestation.

Capture a trace and derive an SBOM from it in one shot. Equivalent to
`trace capture` followed by `sbom generate`.

```bash
mikebom trace run \
  --sbom-output mybuild.cdx.json \
  -- cargo install ripgrep
```

Positional command after `--` is **required**.

| Flag | Default | Purpose |
|---|---|---|
| `--sbom-output <path>` | `mikebom.cdx.json` | SBOM output path |
| `--attestation-output <path>` | `mikebom.attestation.json` | Attestation output path |
| `--format <fmt>` | `cyclonedx-json` | SBOM output format — see [output formats](#output-formats) |
| `--no-enrich` | off | Skip enrichment step (no deps.dev / ClearlyDefined calls) |
| `--include-source-files` | off | Also include observed source files, not just packages. Switches SBOM scope from `packages` to `source`. |
| `--no-hashes` | off | Omit per-component hashes from the SBOM |
| `--trace-children` | off | Follow forked children |
| `--libssl-path <path>` | auto-detect | Override `libssl.so` path |
| `--ring-buffer-size <bytes>` | `8388608` | BPF ring buffer size |
| `--timeout <seconds>` | `0` | Trace timeout |
| `--skip-purl-validation` | off | Skip online PURL existence validation |
| `--lockfile <path>` | — | Path to a lockfile for dependency-relationship enrichment |
| `--artifact-dir <path>` | — | Artifact directory to scan post-trace (see `trace capture`) |
| `--auto-dirs` | off | Auto-detect artifact directories (see `trace capture`) |
| `--json` | off | JSON summary to stdout |

`trace run` currently does not thread the global `--offline` flag through to
the generate step. The enrichment pass is non-fatal on network failure, so
offline users get the same SBOM minus license / CPE upgrades.

---

## `mikebom sbom scan`

**Status:** Implemented. Runs on any platform Rust runs on; no privilege, no eBPF.

Walk a directory or extracted container image, produce a CycloneDX SBOM.

```bash
mikebom sbom scan --path ~/.cargo/registry/cache --output cargo.cdx.json
mikebom sbom scan --image alpine.tar --output alpine.cdx.json

# Pull directly from the registry — no `docker save` required.
# `oci-registry` is on by default as of milestone 033.
mikebom sbom scan --image alpine:3.19 --output alpine.cdx.json
mikebom sbom scan --image gcr.io/distroless/static-debian12:latest \
    --output distroless.cdx.json
```

Exactly one of **`--path <DIR>`** or **`--image <TAR_OR_REF>`** is required.

| Flag | Default | Purpose |
|---|---|---|
| `--path <dir>` | — | Directory to walk recursively. Stream-hashes files with recognised package-artifact suffixes (`.deb`, `.crate`, `.whl`, `.tar.gz`, `.jar`, `.gem`, `.apk`, …). |
| `--image <tar-or-ref>` | — | Either (a) a `docker save` tarball path on disk, or (b) an OCI image reference like `alpine:3.19` or `gcr.io/foo/bar@sha256:...`. mikebom auto-detects which based on whether the path exists. Refs are pulled from the registry, layers decompressed, and the resulting tarball is extracted to a tempdir (OCI whiteouts honoured) before being scanned like `--path`. Multi-arch image indexes resolve to `linux/<host-arch>` by default — pass `--image-platform <linux/ARCH[/VARIANT]>` to pick a different platform from a multi-arch index. Both anonymous and authenticated pulls are supported — for private registries, configure `~/.docker/config.json` (the same keychain `docker pull` uses; see ["Authenticating to private registries"](#authenticating-to-private-registries) below). Disable the registry-pull capability with `cargo install mikebom --no-default-features` if you want a minimal-deps build for embedded use; tarball-extraction still works. |
| `--image-platform <linux/ARCH[/VARIANT]>` | host-arch | Override the platform that's resolved from a multi-arch image index. Only meaningful with a registry `--image <ref>` — for tarballs the platform is fixed by what `docker save` already wrote (passed alongside a tarball, the flag errors). Only `linux` is supported as the OS — mikebom's package-database readers (dpkg / apk / rpm) are linux-rootfs-shaped, so non-Linux containers aren't a meaningful scan target. Common values: `linux/amd64`, `linux/arm64`, `linux/arm/v7`, `linux/386`, `linux/ppc64le`, `linux/s390x`. Use case: a macOS arm64 dev machine scanning a `linux/amd64` image deployed to AWS, or Linux x86_64 CI scanning an `arm64` image deployed to Graviton. When the index doesn't contain a matching entry, the error lists the available platforms. |
| `--no-oci-cache` | off | Disable the on-disk OCI blob cache for registry pulls. Equivalent to `MIKEBOM_OCI_CACHE=0`. When set, every blob is fetched from the registry on every scan; existing cache files on disk are not touched. Use case: CI lanes that want pure one-shot semantics, or debugging a registry-side regression. See ["OCI layer caching"](#oci-layer-caching) below. |
| `--oci-cache-size <BYTES>` | 10 GB | Cap (in bytes) for the on-disk OCI blob cache. When the cache exceeds this size after an insert, oldest-mtime entries are evicted until the total drops below the cap. Equivalent env var: `MIKEBOM_OCI_CACHE_SIZE=<bytes>`. See ["OCI layer caching"](#oci-layer-caching) below. |
| `--output <[FMT=]PATH>` | per-format default (`mikebom.cdx.json`, `mikebom.spdx.json`, …) | Output path override. Two forms: bare `--output <path>` (applies to the single requested format — rejected with multiple formats) and per-format `--output <fmt>=<path>` (repeatable; each entry retargets one format). The special key `openvex` retargets the OpenVEX sidecar that SPDX emission co-produces when VEX is present — legal only alongside an SPDX format. |
| `--format <fmt>` | `cyclonedx-json` | See [output formats](#output-formats). Comma-separated list + repeatable flag: `--format cyclonedx-json,spdx-2.3-json` produces both from a single scan. Duplicates dedupe silently. |
| `--max-file-size <bytes>` | `268435456` (256 MB) | Skip hashing files larger than this |
| `--no-hashes` | off | Omit per-component content hashes from the SBOM |
| `--deb-codename <value>` | auto | Value to stamp as the `distro=` qualifier on deb PURLs (e.g., `debian-12`, `ubuntu-24.04`, `kali-rolling`). Stamped verbatim. Overrides the value auto-derived from `<root>/etc/os-release` (`ID` + `VERSION_ID` → `distro=<id>-<version_id>`). Despite the flag name, it accepts any string; the canonical shape is `<namespace>-<VERSION_ID>` matching rpm and apk. |
| `--no-package-db` | off (DB reading is on by default) | Skip reading `/var/lib/dpkg/status` and `/lib/apk/db/installed`. Falls back to artefact-file-only scanning. Use when you want to verify a download cache and ignore the installed set. |
| `--no-deep-hash` | off | Skip per-file SHA-256 of installed-package contents. Falls back to a fast SHA-256 of each package's dpkg `.md5sums` file. Produces component-level identity but no `evidence.occurrences[]`. |
| `--json` | off | JSON summary to stdout |

Behaviour notes:

- Enrichment runs inline on scan: deps.dev version info, ClearlyDefined
  concluded licenses, and deps.dev transitive-dep-graph edges (Maven-primary).
  All three respect the global `--offline` flag — under `--offline` they become
  silent no-ops.
- Deb, apk, and RPM components carry a CycloneDX `evidence.identity` block at
  confidence 0.85 with `technique: "manifest-analysis"` — they come from the
  installed-package database, not from observing the install event.
- Artifact-file-resolved components carry confidence 0.70 with
  `technique: "filename"`.
- Maven fat-jars built with the shade-plugin emit nested
  `components[].components[]` entries per shade-relocated ancestor (one
  `pkg:maven/<g>/<a>@<v>` per ancestor declared in the JAR's
  `META-INF/DEPENDENCIES`), tagged with the property
  `mikebom:shade-relocation = true`. Emission is gated on
  bytecode-presence verification — declared-but-not-relocated ancestors
  are dropped. Feature 009; see
  [`docs/ecosystems.md`](../ecosystems.md) and
  [`specs/009-maven-shade-deps/spec.md`](../../specs/009-maven-shade-deps/spec.md).
- `sbom scan` inherits the top-level `--offline`, `--include-dev`,
  `--include-declared-deps`, and `--include-legacy-rpmdb` flags — they
  can be passed either before or after `scan`. See [global flags](#global-flags)
  and [Configuration](configuration.md).

### Authenticating to private registries

When `--image <ref>` resolves to an OCI image reference (rather than a
tarball path), mikebom uses the same Docker keychain that `docker pull`
uses — `~/.docker/config.json` (or `$DOCKER_CONFIG/config.json` if set).
No mikebom-specific credential file or CLI flag is required.

Credentials resolve in this priority order, matching Docker's:

1. **`credHelpers.<registry>`** — per-registry credential helper override.
   Used by AWS ECR (`docker-credential-ecr-login`) and Google Artifact
   Registry (`docker-credential-gcloud`). The helper is invoked as a
   subprocess: `docker-credential-<helper> get` with the registry
   hostname on stdin, returning `{Username, Secret, ServerURL}` on stdout.
2. **`credsStore`** — registry-wide helper (e.g. `osxkeychain`, `wincred`,
   `secretservice`, `pass`, `desktop`). Same subprocess protocol.
3. **`auths.<registry>.auth`** — direct credentials, base64-encoded as
   `user:password`. Written by `docker login` on systems without a
   keychain.
4. **`auths.<registry>.identitytoken`** — registry-issued bearer token
   (Azure ACR, some Hub flows). Sent as the secret with username
   `<token>`.

Examples — `~/.docker/config.json` shapes that mikebom understands:

```json
// GHCR with a Personal Access Token (PAT) requires the read:packages scope.
{
  "auths": {
    "ghcr.io": { "auth": "<base64('username:ghp_xxxxxxxxxxxx')>" }
  }
}
```

```json
// macOS Docker Desktop, credentials in the system keychain.
{
  "auths": { "ghcr.io": {} },
  "credsStore": "desktop"
}
```

```json
// AWS ECR via the standard helper.
{
  "credHelpers": {
    "123456789012.dkr.ecr.us-east-1.amazonaws.com": "ecr-login"
  }
}
```

Behaviour notes:

- **Anonymous fallback.** If no `~/.docker/config.json` exists, or no
  entry matches the target registry, mikebom falls through to the
  anonymous bearer-token flow that public images already use. No error
  is raised.
- **Helper failure → anonymous fallback.** If a credential helper exits
  non-zero or emits the documented `"credentials not found"` sentinel,
  mikebom falls through to anonymous (so `credsStore: desktop` doesn't
  block unrelated public-image scans). Helper stderr is intentionally
  *not* captured into mikebom's logs — some helpers print partial
  credentials there.
- **No secret leakage.** Credentials are never printed to stdout, stderr,
  `--verbose` output, or `RUST_LOG=debug` traces; the in-memory
  `Credential` struct redacts both fields in its `Debug` impl. If you
  observe a leak, please file an issue.
- **ECR token TTL.** AWS ECR tokens expire after 12 hours. Since
  mikebom is a one-shot CLI, this never matters in practice — by the
  time a token would expire, the scan is long since complete.
- **GHCR PAT scope.** Private GHCR images require `read:packages` on
  the PAT. `repo` scope alone is not sufficient.

What's not yet supported (deferred to follow-ons):

- `--registry-auth user:pass@host` CLI flag (file `docker login` first).
- OAuth refresh-token flows beyond the bearer-token retry.
- Native AWS SDK integration for ECR (`docker-credential-ecr-login`
  remains the supported path).
- Registry mirror configs (`registries.conf`, `mirrors`).

### OCI layer caching

OCI distribution-spec blobs (image config + each layer) are
content-addressed by SHA-256, so caching them on disk is correct by
construction: a cache hit on a digest is identical-bytes to a fresh
network fetch of that digest. mikebom caches every blob it pulls,
keyed on digest; subsequent scans of the same image reuse the
cached bytes and complete in seconds rather than tens of seconds.
The image's manifest is intentionally NOT cached — a floating tag
like `:latest` should re-fetch the manifest every time so updates
are detected, after which any new layer digests will naturally
cache-miss.

**Cache location** (priority order):

1. `$MIKEBOM_OCI_CACHE_DIR` (when set non-empty).
2. `$XDG_CACHE_HOME/mikebom/oci-layers` (Linux convention).
3. `$HOME/Library/Caches/mikebom/oci-layers` (macOS).
4. `$HOME/.cache/mikebom/oci-layers` (fallback).

**Layout**: `<cache-dir>/sha256/<64-hex>` per blob.

**Eviction**: the cache enforces a default 10 GB cap. When an
insert pushes the total over, oldest-mtime entries are removed
until the total is back under the cap. Override the cap with
`--oci-cache-size <bytes>` or `MIKEBOM_OCI_CACHE_SIZE=<bytes>`.

**Disabling**: pass `--no-oci-cache` (or set
`MIKEBOM_OCI_CACHE=0`) to skip the cache entirely for one
invocation. Existing cache files on disk are untouched.

**Clearing**: `rm -rf "$XDG_CACHE_HOME/mikebom/oci-layers"` (or
the macOS / fallback equivalent). mikebom doesn't ship a
`--clear-oci-cache` command; the directory is the canonical handle.

**Safety properties**:

- Every cache read re-verifies the on-disk SHA-256 against the
  filename. Corrupted entries (truncated, bit-flipped) are
  detected, deleted, and re-fetched from the network.
- Cache writes go through `tempfile + atomic rename` (intra-fs
  rename, atomic on every POSIX-ish filesystem). Concurrent scans
  of the same image don't corrupt the final blob — both writers
  produce identical bytes (same digest = same content) and the
  rename atomically swaps in one of them.
- A read-only cache directory or any other IO failure is
  non-fatal: mikebom logs a warning and falls through to network-
  only behavior. The scan completes either way.
- Path-traversal is impossible: only `sha256:<64-hex>` digests
  resolve to a path; anything else (including `sha256:..`)
  fails the hex grammar check at lookup time.

---

## `mikebom sbom generate`

**Status:** Implemented.

Derive a CycloneDX SBOM from an in-toto attestation produced by
`mikebom trace capture`.

```bash
mikebom sbom generate build.attestation.json \
  --output build.cdx.json \
  --scope source \
  --enrich \
  --lockfile Cargo.lock
```

Positional **attestation file** is required.

| Flag | Default | Purpose |
|---|---|---|
| `--output <path>` | `mikebom.cdx.json` | SBOM output path |
| `--format <fmt>` | `cyclonedx-json` | See [output formats](#output-formats) |
| `--scope <kind>` | `packages` | `packages` = resolved PURLs only. `source` = packages plus observed source files (with hashes). |
| `--no-hashes` | off | Omit per-component hashes |
| `--enrich` | off | Run enrichment (license, VEX, supplier). |
| `--lockfile <path>` | — | Path to a lockfile for dependency-relationship enrichment. Auto-detects format (`Cargo.lock`, `package-lock.json`, `go.sum`). Unrecognised formats are logged and skipped. |
| `--deps-dev-timeout <ms>` | `5000` | Timeout per deps.dev API call |
| `--skip-purl-validation` | off | Skip online PURL existence validation |
| `--vex-overrides <path>` | — | VEX override file for manual triage states |
| `--json` | off | JSON summary to stdout |

Note: as of the current code, the only enrichment source wired into the
`EnrichmentPipeline` inside `sbom generate` is `LockfileSource`. Inline
enrichment via deps.dev / ClearlyDefined happens in `sbom scan` but has not
yet been threaded into the `generate` flow — this is why `--enrich` takes
effect but does not currently fetch licenses from deps.dev.

---

## `mikebom sbom enrich`

**Status: Implemented (feature 006 US5).** Detailed documentation is
below in the "`mikebom sbom enrich` — feature 006 US5" section. In
short: accepts one or more RFC 6902 JSON Patch files via `--patch`
and records per-patch provenance on the BOM metadata.

---

## `mikebom attestation validate`

**Status: Implemented.** Validates an attestation file for in-toto /
SBOMit schema conformance (shape checks only — does *not* verify
signatures; use `mikebom sbom verify` for that).

```bash
mikebom attestation validate some.attestation.json
```

Planned purpose: validate an in-toto attestation file for schema conformance.

---

## `mikebom sbom verify` — feature 006 US1

Verify a signed DSSE envelope produced by `mikebom` or any other
SBOMit-compliant tool.

```
mikebom sbom verify <ATTESTATION> [flags]
```

Flags:
- `--public-key <PATH>` — PEM-encoded public key for local-key
  verification (mutually exclusive with `--identity`)
- `--identity <PATTERN>` — keyless identity matcher (email, URL, or
  glob) against the Fulcio cert's SAN
- `--expected-subject <PATH>` — verify on-disk SHA-256 of `PATH`
  matches a subject in the envelope. Repeatable.
- `--layout <PATH>` — enforce an in-toto layout
- `--no-transparency-log` — tolerate keyless envelopes without a
  Rekor inclusion proof
- `--fulcio-url` / `--rekor-url` — custom sigstore endpoints
- `--json` — structured `VerificationReport` on stdout

Exit codes per `specs/006-sbomit-suite/contracts/cli.md`: `0` pass,
`1` crypto failure, `2` envelope failure, `3` layout failure.

## `mikebom policy init` — feature 006 US4

Generate a starter in-toto layout bound to a functionary key.

```
mikebom policy init --functionary-key signing.pub [flags]
```

Flags:
- `--functionary-key <PATH>` *(required)* — PEM public key
- `--step-name <NAME>` — default `build-trace-capture`
- `--expires <DURATION>` — `6m` / `1y` / `18mo` / `2y`; default `1y`
- `--readme <TEXT>` — embedded description
- `--output <PATH>` — default `layout.json`

Use the emitted layout with `mikebom sbom verify --layout …` to enforce
functionary + step-name policy on signed attestations. Layouts are
standard in-toto — any in-toto-aware verifier accepts them.

## Signing flags — `trace capture` / `trace run`

Add one category of new flag to an otherwise-unchanged invocation to
start producing signed DSSE envelopes:

- `--signing-key <PATH>` — local PEM private key (mutually exclusive
  with `--keyless`)
- `--signing-key-passphrase-env <NAME>` — env var name holding the
  passphrase for an encrypted key (no interactive prompt)
- `--keyless` — OIDC → Fulcio → Rekor (CI-friendly; auto-detects
  GitHub Actions)
- `--no-transparency-log` — keyless mode only; skip Rekor upload
- `--fulcio-url` / `--rekor-url` — custom sigstore endpoints
- `--require-signing` — hard-fail if no signing identity configured
- `--subject <PATH>` — explicit subject; repeatable; suppresses
  artifact auto-detection

## `mikebom sbom enrich` — feature 006 US5

Apply one or more RFC 6902 JSON Patch files to a generated CycloneDX
SBOM, recording per-patch provenance (`mikebom:enrichment-patch[N]`)
in the SBOM's top-level `properties[]` array.

```
mikebom sbom enrich <SBOM> --patch <PATCH> [--patch <PATCH>…] [flags]
```

Flags:
- `--patch <PATH>` *(at least one required)* — patch file; repeatable
- `--author <STRING>` — recorded author (defaults to `"unknown"` with
  a warning)
- `--base-attestation <PATH>` — attestation file whose SHA-256 gets
  embedded so verifiers can walk back to the attested source
- `--output <PATH>` — default overwrites the input SBOM

---

## Output formats

The `--format` flag on `sbom scan` accepts a comma-separated list;
the flag itself is repeatable. Duplicates dedupe silently. Default
is `cyclonedx-json`. Every registered format id:

| Value | Status | Default filename |
|---|---|---|
| `cyclonedx-json` | **Stable.** Default. CycloneDX 1.6 JSON. | `mikebom.cdx.json` |
| `spdx-2.3-json` | **Stable.** SPDX 2.3 JSON. Covers all 9 supported ecosystems. Validates clean against the official SPDX 2.3 JSON schema. | `mikebom.spdx.json` |
| `spdx-3-json-experimental` | **[EXPERIMENTAL]** SPDX 3.0.1 JSON-LD. **npm ecosystem only**; non-npm components are filtered out of the `@graph`. License emission, CPE, supplier, and evidence fields are out of stub scope — see `docs/reference/sbom-format-mapping.md` for the full scope contract. Opt-in; the format id carries the `-experimental` suffix so it can't be picked up by accident. | `mikebom.spdx3-experimental.json` |

**OpenVEX sidecar** — when the scan produces VEX statements (via
`ResolvedComponent.advisories`) AND SPDX 2.3 output is requested,
mikebom co-emits an OpenVEX 0.2.0 JSON file alongside the SPDX
file. The SPDX document carries a `DocumentRef-OpenVEX` entry in
`externalDocumentRefs` with a SHA-256 of the sidecar bytes. Use
`--output openvex=<path>` to retarget the sidecar. When the scan
has no advisories, no sidecar is written and no
`externalDocumentRefs` entry appears.

See [architecture/generation.md](../architecture/generation.md) for
the CycloneDX 1.6 mapping details and
[`docs/reference/sbom-format-mapping.md`](../reference/sbom-format-mapping.md)
for the full cross-format data-placement map.
