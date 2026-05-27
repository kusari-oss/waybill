# CLI reference

mikebom follows a strict `mikebom <noun> <verb>` pattern. This page is the canonical
operator reference: every flag accepted at every level, with type, default, repeatable
status, valid value vocab where applicable, a one-paragraph description, and at least
one example invocation.

Top-level nouns:

- **`sbom`** — SBOM generation, enrichment, verification, and parity *(stable)*
- **`policy`** — in-toto layout generation *(stable)*
- **`attestation`** — attestation management *(stable)*
- **`trace`** — eBPF build-process tracing *(experimental, Linux only)*

> **Experimental** means: the output formats are stable, but the trace-mode
> pipeline adds ~2-3× wall-clock overhead on syscall-heavy builds, requires
> CAP_BPF + CAP_PERFMON, and has coverage gaps on some syscall variants
> (`openat2`, `io_uring`). For most SBOM use cases prefer `mikebom sbom scan` —
> it produces richer output with no privilege requirements and runs on any OS.

Global flags apply to every subcommand and must appear **before** the noun:

```bash
mikebom --offline sbom scan --path .
```

## Documentation policy

This reference documents every operator-facing flag exposed by `--help`. Internal
debugging flags (those gated behind `--debug-*` or off the `--help` listing) are
intentionally absent. To verify the reference is in sync with the binary's actual
flag set, run `bash scripts/verify-docs-currency.sh` (exit 0 = in sync).

Deprecated flags are marked with a `**Deprecated:**` block listing the milestone
they were deprecated in, the replacement, and the removal target if scheduled.

---

## Global flags

These flags can be passed before any subcommand. They are also accepted after the
noun on every subcommand (clap's flag-position-tolerant parser).

| Flag | Type | Default | Description |
|---|---|---|---|
| `--offline` | bool | off | Disable all outbound network calls (deps.dev, ClearlyDefined). |
| `--exclude-scope <SCOPE>` | enum (repeatable, comma-separated) | (none) | Drop components whose lifecycle scope matches any listed value. Valid: `dev`, `build`, `test`. Runtime scope is always retained. |
| `--include-declared-deps` | bool | off (`--image`) / on (`--path`) | Include declared-but-not-on-disk dependencies (manifest SBOM mode). |
| `--include-legacy-rpmdb` | bool | off | Read legacy Berkeley-DB rpmdb on pre-RHEL-8 / CentOS-7 / Amazon-Linux-2 images. Also enabled via `MIKEBOM_INCLUDE_LEGACY_RPMDB=1`. |
| `--timeout <SECONDS>` | u64 | disabled | Wall-clock time limit for the entire mikebom invocation. Exits with status 124 (POSIX `timeout(1)` convention) when exceeded. Set to `0` (or omit) to disable. |
| `--include-dev` | bool | off | **Deprecated.** See the deprecated flags section. |

### `--offline`

Disable all outbound HTTP. When set, deps.dev license/CPE lookups, deps.dev hash
queries, and ClearlyDefined enrichment all become no-ops. The scanner still
produces a complete SBOM from local sources (lockfiles, package databases,
manifests). Useful for air-gapped scanners, reproducible-build environments, and
CI lanes that can't reach the internet.

```bash
mikebom --offline sbom scan --path . --output offline.cdx.json
```

Accepts three equivalent forms:

- `--offline` — alone, equivalent to `--offline=true`
- `--offline=true` — explicit on (handy for scripts toggled by a boolean variable)
- `--offline=false` — explicit off (overrides a `MIKEBOM_OFFLINE=1` environment
  default for a single invocation)

The `=` is required when a value is supplied; `--offline true` (with a space)
is rejected so the next positional argument is never silently consumed.

See also: [Configuration](configuration.md) for the full offline-mode contract.

### `--exclude-scope <SCOPE[,SCOPE...]>`

Drop components whose lifecycle scope matches any of the listed values. Comma-
separated. Valid values: `dev`, `build`, `test`. Runtime-scope is always
retained — excluding all of runtime would produce an empty SBOM.

`--exclude-scope dev,build,test` produces the strict "what shipped to production"
view. `--exclude-scope test` drops only test deps; `--exclude-scope dev,build`
keeps test deps for security-audit workflows.

```bash
mikebom sbom scan --path . --exclude-scope dev,build,test --output runtime.cdx.json
```

When omitted, mikebom emits all scopes (Runtime + Development + Build + Test).

### `--include-declared-deps`

Include declared-but-not-on-disk dependencies (manifest SBOM). By default,
mikebom emits only components physically present in the scanned tree or image
("artifact SBOM" — if it's in the image, it's in the SBOM). When set, also
emits: deps.dev-reported transitives with no on-disk trace
(`source_type = declared-not-cached`); Maven `pom.xml`-declared direct deps
with no matching JAR or `.m2` cache entry (`source_type = workspace`); Maven
BFS cache-miss transitives (`source_type = transitive`, no `.pom` on disk).

Auto-enabled for `sbom scan --path` so source-tree scans keep the "what would
be pulled in on build" view; explicit for `--image` when you want the same
permissive output from a container scan.

See `docs/design-notes.md` for the full artifact-vs-manifest SBOM rationale.

### `--include-legacy-rpmdb`

Enable reading of the legacy Berkeley-DB rpmdb (`/var/lib/rpm/Packages`) on
pre-RHEL-8 / CentOS-7 / Amazon-Linux-2 images. Off by default. Also enabled
via `MIKEBOM_INCLUDE_LEGACY_RPMDB=1`.

```bash
mikebom sbom scan --image centos7.tar --include-legacy-rpmdb --output centos7.cdx.json
```

Modern RHEL-8+ / Fedora / Amazon-Linux-2023 images use the SQLite rpmdb
(`/var/lib/rpm/rpmdb.sqlite`) which mikebom reads by default; the BDB format
is opt-in to keep the modern hot-path slim.

### `--include-dev` (deprecated)

**Deprecated:** since milestone 052/part-3. Replacement: `--exclude-scope`.
Removal target: not yet scheduled (the flag still parses for back-compat).

Pre-052 this flag was off-by-default and gated dev/test/build dep emission.
Post-052 the default is to include ALL scopes natively tagged. To restore
the strict deployed-runtime view, use `--exclude-scope dev,build,test` (see
the deprecation warning emitted on stderr). Set `MIKEBOM_NO_DEPRECATION_NOTICE=1`
to suppress the warning during a controlled migration.

### `--timeout <SECONDS>`

Wall-clock time limit for the entire mikebom invocation, in seconds. If
exceeded, mikebom emits a `tracing::error` to stderr and exits with status
**124** (POSIX [`timeout(1)`](https://www.gnu.org/software/coreutils/manual/html_node/timeout-invocation.html)
convention). Disabled when omitted or set to `0`.

```bash
mikebom --timeout 600 sbom scan --image registry.example.com/big-image:latest --output big.cdx.json
echo "exit: $?"  # 124 if the scan ran longer than 600s, 0 otherwise
```

Use cases:

- **CI**: bound a runaway scan against an unknown image.
- **Kubernetes CronJob**: protect the pod-disruption budget when a per-pod scan
  could otherwise outlast the job's deadline.
- **Exploratory scans**: cap discovery against potentially-large container
  filesystems.

#### Interaction with other timeouts

| Flag | Scope | Default |
|---|---|---|
| `--timeout <SECONDS>` (this flag) | Wall-clock cap on the entire `mikebom` invocation | Disabled |
| `mikebom trace run --timeout <SECONDS>` | Caps the SUBPROCESS being traced (not mikebom itself) | `0` (no timeout) |
| Internal per-fetch timeouts | OCI registry pulls, deps.dev HTTP requests, `go mod graph` subprocess | Hardcoded defaults |

Whichever timeout fires first wins. The global `--timeout` is the only one
that brings mikebom itself to a hard stop; the others are scoped to specific
operations.

#### Partial output

Partial output may not be written when the watchdog fires — there are no
atomic-flush guarantees. Operators who need "produce-the-best-SBOM-you-can-
in-N-seconds" semantics should pair `--timeout` with `--output` to a specific
path and check for that file's presence (and validity) after the run:

```bash
mikebom --timeout 600 sbom scan --path . --output project.cdx.json
case $? in
  0)   echo "scan completed within the time limit" ;;
  124) echo "scan exceeded the time limit; partial output may not have been written" ;;
  *)   echo "scan failed with another error: $?" ;;
esac
```

---

## `mikebom sbom scan`

Walk a directory or extracted container image and produce one or more SBOM
formats from the package artifacts on disk. No eBPF required — runs anywhere
Rust runs.

Exactly one of `--path` or `--image` is required.

### Quick reference

| Flag | Type | Default | Description |
|---|---|---|---|
| `--path <PATH>` | path | (required if no `--image`) | Directory to walk recursively. |
| `--image <IMAGE>` | path-or-OCI-ref | (required if no `--path`) | Tarball path or OCI reference. |
| `--image-src <SRC[,SRC...]>` | enum (`docker`, `remote`) | `docker,remote` | Image source-resolution order. |
| `--image-platform <linux/ARCH[/VARIANT]>` | string | host arch | Multi-arch image platform pick. |
| `--no-oci-cache` | bool | off | Disable the OCI blob cache for registry pulls. |
| `--oci-cache-size <BYTES>` | u64 | `10737418240` (10 GB) | Cap for the on-disk OCI blob cache. |
| `--registry-credentials-dir <PATH>` | path | (unset) | Directory containing Docker-format registry credentials (issue #235; for in-cluster operation). |
| `--output <[FMT=]PATH>` | string (repeatable) | per-format default | Output path override. |
| `--format <FORMAT>` | enum (repeatable, comma-separated) | `cyclonedx-json` | Output format(s). |
| `--max-file-size <BYTES>` | u64 | `268435456` (256 MB) | Skip files larger than this. |
| `--no-hashes` | bool | off | Omit per-component content hashes. |
| `--deb-codename <VALUE>` | string | auto-detect | Override `distro=` qualifier on deb PURLs. |
| `--no-package-db` | bool | off | Skip installed-package DB reads (dpkg/apk). |
| `--include-vendored` | bool | off | Emit CMake `add_subdirectory(third_party/\|vendor/...)` entries. |
| `--no-deep-hash` | bool | off | Skip per-file SHA-256 of installed-package contents. |
| `--json` | bool | off | Print a JSON summary to stdout. |
| `--no-clearly-defined` | bool | off | Skip ClearlyDefined enrichment. |
| `--no-deps-dev` | bool | off | Skip deps.dev license enrichment. |
| `--no-deps-dev-graph` | bool | off | Skip deps.dev transitive dep-graph enrichment. |
| `--enrich-sources <SRC[,SRC...]>` | enum (`deps-dev`, `clearly-defined`, `deps-dev-graph`) | (all enabled) | Allowlist of enrichment sources. |
| `--bind-to-source <PATH>` | path | (none) | Source-tier SBOM to bind emitted components against. |
| `--repo <URL>` | URL | auto-detect | Attach a `repo:` identifier. |
| `--git-ref <REVISION>` | string | auto-detect | Pair with `--repo` to upgrade to `git:<repo>#<ref>`. |
| `--image-id <REF>` | string | auto-detect | Attach an `image:` identifier. |
| `--attestation <IRI>` | IRI | (none) | Attach an `attestation:` identifier. |
| `--id <SCHEME=VALUE>` | string (repeatable) | (none) | Attach a user-defined identifier. |
| `--keep-credentials-in-identifiers` | bool | off | Preserve userinfo in auto-detected git URLs. |
| `--subject-hash <ALGO:HEX>` | string (repeatable) | (none) | Attach a `subject:` content-hash identifier. |
| `--component-id <PURL=SCHEME:VALUE>` | string (repeatable) | (none) | Attach a user-defined identifier to a specific component. |
| `--root-name <NAME>` | string | auto-derived | Override `metadata.component.name`. |
| `--root-version <VERSION>` | string | auto-derived | Override `metadata.component.version`. |
| `--creator <TYPE: NAME>` | string (repeatable) | (none) | Attach a creator entry to the SBOM. |
| `--annotator <TYPE: NAME>` | string (repeatable, paired) | (none) | Document-level annotator. Pair 1:1 with `--annotation-comment`. |
| `--annotation-comment <TEXT>` | string (repeatable, paired) | (none) | Comment that pairs positionally with the preceding `--annotator`. |
| `--metadata-comment <TEXT>` | string | (none) | Free-text comment about the SBOM document. |
| `--scan-target-name <NAME>` | string | auto-derived | Operator override for the document/SBOM name field. |
| `--metadata-file <PATH>` | path | (none) | JSON sidecar with user-supplied metadata. |
| `--sbom-type <TYPE>` | enum (`design`/`source`/`build`/`analyzed`/`deployed`/`runtime`) | auto-detect | Operator-asserted CISA SBOM Type. |
| `--spdx2-relationship-compat <PROFILE>` | enum (`full`/`basic`) | `full` | SPDX 2.3 relationship-vocabulary compatibility for scoped deps (issue #228). |

### `--path <PATH>`

Directory to walk recursively for package artifacts. Files with recognised
package-artifact suffixes (`.deb`, `.crate`, `.whl`, `.tar.gz`, `.jar`, `.gem`,
`.apk`, …) are stream-hashed and matched against the path resolver.

```bash
mikebom sbom scan --path . --output project.cdx.json
```

### `--image <IMAGE>`

Container image to scan. Two accepted forms:

1. A `docker save`-format tarball path on disk. Layers are extracted into a
   tempdir (whiteouts honoured), then the resulting rootfs is scanned exactly
   like `--path`.
2. An OCI image reference (e.g., `alpine:3.19` or `gcr.io/foo/bar@sha256:...`).
   mikebom auto-detects which based on whether the path exists on disk.

```bash
mikebom sbom scan --image alpine.tar --output alpine.cdx.json
mikebom sbom scan --image alpine:3.19 --output alpine.cdx.json
```

For OCI references mikebom checks the local docker daemon's cache first, then
falls back to a registry pull on miss (matches `docker run` semantics).
Override the source-resolution order with `--image-src`.

### `--image-src <SRC[,SRC...]>`

Image source-resolution order. Comma-separated; mikebom tries each source in
order and stops at the first one that has the image. Default `docker,remote`
matches trivy's `--image-src` and syft's auto-detection.

Possible values:

- `docker` — local docker daemon (shells out to `docker image inspect` then
  `docker save`).
- `remote` — OCI distribution-spec registry pull.

```bash
mikebom sbom scan --image alpine:3.19 --image-src remote --output alpine.cdx.json
```

`--image-src remote` forces a fresh registry fetch; `--image-src docker` fails
rather than touching the network. Ignored when `--image` resolves to a
tarball file on disk.

### `--image-platform <linux/ARCH[/VARIANT]>`

Override the platform that's resolved from a multi-arch image index. Only
meaningful when `--image` points at a registry reference. Format
`<os>/<arch>` or `<os>/<arch>/<variant>`. Only `linux` is supported as the OS.

Common values: `linux/amd64`, `linux/arm64`, `linux/arm/v7`, `linux/386`,
`linux/ppc64le`, `linux/s390x`. When omitted, auto-resolves to
`linux/<host-arch>`.

```bash
mikebom sbom scan --image alpine:3.19 --image-platform linux/arm64 --output alpine-arm64.cdx.json
```

Use case: a macOS arm64 dev machine scanning a `linux/amd64` container deployed
to AWS, or Linux x86_64 CI scanning an arm64 image deployed to Graviton.

### `--no-oci-cache`

Disable the OCI blob cache for registry pulls. Equivalent to
`MIKEBOM_OCI_CACHE=0`. When set, every blob (config + layer) is fetched fresh
on every scan, even if the same digest is already cached. Cache files on disk
are untouched.

Use case: CI lanes that want pure one-shot semantics, or debugging a
registry-side regression.

### `--oci-cache-size <BYTES>`

Cap (in bytes) for the on-disk OCI blob cache. When the cache exceeds this
size, oldest-mtime entries are evicted until the total drops below the cap.
Default 10 GB. Equivalent env var `MIKEBOM_OCI_CACHE_SIZE`.

### `--registry-credentials-dir <PATH>`

Directory containing Docker-format registry credentials. Probes the K8s
secret-mount filenames in order: `config.json` (plain Docker convention),
`.dockerconfigjson` (K8s `kubernetes.io/dockerconfigjson` secret type),
`.dockercfg` (legacy K8s `kubernetes.io/dockercfg` secret type). First
readable + parseable file wins. The file format is the standard Docker
`config.json` shape (`auths`, `credsStore`, `credHelpers`); the existing
credential-resolution precedence applies inside the loaded config.

Use this when running mikebom in a container that mounts a K8s
`imagePullSecrets`-derived volume (typically at
`/var/run/secrets/registry/`). For local/CI use with the standard Docker
keychain, leave this unset — mikebom falls back to
`$DOCKER_CONFIG/config.json` or `$HOME/.docker/config.json` (issue #235).

**Full credential-resolution priority chain** (highest to lowest):

1. Per-registry env vars `MIKEBOM_REGISTRY_<HOST>_USERNAME` +
   `MIKEBOM_REGISTRY_<HOST>_PASSWORD`, where `<HOST>` is the registry
   hostname normalized to uppercase with `[^A-Z0-9]` replaced by `_`
   (e.g. `ghcr.io` → `MIKEBOM_REGISTRY_GHCR_IO_USERNAME`).
2. Generic env vars `MIKEBOM_REGISTRY_USERNAME` + `MIKEBOM_REGISTRY_PASSWORD`
   (applies to every registry).
3. The `--registry-credentials-dir` path described above.
4. `$DOCKER_CONFIG/config.json` or `$HOME/.docker/config.json`
   (legacy/default behavior, unchanged).

If every source fails, mikebom falls through to anonymous registry access
— which works for public registries hosting public images.

```bash
# In-cluster CronJob pattern: K8s mounts an imagePullSecret-derived
# volume; mikebom reads creds from there.
mikebom sbom scan \
  --image my-ecr.amazonaws.com/app:v1 \
  --image-src remote \
  --registry-credentials-dir /var/run/secrets/registry
```

### `--output <[FMT=]PATH>`

Output path override. Two forms accepted:

- Bare `--output <path>` — applies to the single requested format. Rejected
  when more than one format is requested.
- Per-format `--output <fmt>=<path>` — repeatable; each entry overrides the
  default filename for exactly one format id.

```bash
mikebom sbom scan --path . \
    --format cyclonedx-json,spdx-2.3-json \
    --output cyclonedx-json=out.cdx.json \
    --output spdx-2.3-json=out.spdx.json
```

When omitted, each format writes to its own default filename
(`mikebom.cdx.json`, `mikebom.spdx.json`, `mikebom.spdx3.json`).

### `--format <FORMAT>`

Output format(s). Comma-separated and the flag itself is repeatable:
`--format cyclonedx-json,spdx-2.3-json` is equivalent to
`--format cyclonedx-json --format spdx-2.3-json`. Default `cyclonedx-json`.
Duplicates dedupe silently.

Registered formats:

- `cyclonedx-json` — CycloneDX 1.6 JSON. Default filename `mikebom.cdx.json`.
- `spdx-2.3-json` — SPDX 2.3 JSON. Default filename `mikebom.spdx.json`.
- `spdx-3-json` — SPDX 3.0.1 JSON-LD. Default filename `mikebom.spdx3.json`.
- `spdx-3-json-experimental` *(deprecated alias)* — byte-identical to
  `spdx-3-json`; emits a stderr deprecation notice. Set
  `MIKEBOM_NO_DEPRECATION_NOTICE=1` to suppress.

See [SBOM types](../reference/sbom-types.md) for the per-format SBOM-type
field positions and CISA SBOM Types vocab.

### `--max-file-size <BYTES>`

Maximum file size to hash, in bytes. Larger files are skipped. Default
`268435456` (256 MB) covers the largest realistic package artifact.

### `--no-hashes`

Omit per-component content hashes from the SBOM. Reduces output size but
disables byte-level tamper detection.

### `--deb-codename <VALUE>`

Optional distro codename to stamp on deb PURLs. Overrides the value
auto-derived from `<root>/etc/os-release` (`ID` + `VERSION_ID` →
`distro=<id>-<version_id>`). Useful when scanning a directory that isn't
itself a rootfs (e.g., a bare directory of `.deb` files).

```bash
mikebom sbom scan --path ./debs --deb-codename debian-12 --output debs.cdx.json
```

### `--no-package-db`

Skip reading installed-package databases (`/var/lib/dpkg/status`,
`/lib/apk/db/installed`). On by default because production container images
routinely clean up `.deb`/`.apk` artefact caches and the db is then the only
complete source. Pass this flag to fall back to pure artefact-file scanning.

### `--include-vendored`

Include vendored C/C++ dependencies declared via CMake
`add_subdirectory(third_party/<name>)` or `add_subdirectory(vendor/<name>)`.
Default **OFF** — bare `add_subdirectory(...)` is also how CMake projects
include first-party `src/` and `tests/` sub-modules, so the gate requires
an explicit opt-in to avoid false positives. The path-prefix check rejects
anything that isn't under `third_party/` or `vendor/`.

When enabled, mikebom emits one `pkg:generic/<name>` component per
vendored entry with a `mikebom:vendored = true` property. The version
segment is backfilled from a co-located `version.txt` or `.version` file
when present; otherwise the PURL has no version.

```bash
# Opt in via flag
mikebom sbom scan --path . --output project.cdx.json --include-vendored

# Opt in via env var (accepts "1", "true", etc.)
MIKEBOM_INCLUDE_VENDORED=1 mikebom sbom scan --path . --output project.cdx.json
```

### `--no-deep-hash`

Skip per-file SHA-256 hashing of installed-package contents. Falls back to a
fast SHA-256 over each package's dpkg `.md5sums` file (microseconds per
package; component-level identity only, no per-file occurrences).

Default-on hashing reads every file referenced by dpkg's `.list` manifest —
proportional to installed size (~3-5 s on `debian:bookworm-slim`, ~30 s on
full `debian`).

### `--json`

Print a JSON summary to stdout after writing the SBOM.

```bash
mikebom sbom scan --path . --output project.cdx.json --json
```

### `--no-clearly-defined`

Skip ClearlyDefined enrichment (concluded licenses). Keeps deps.dev license +
dep-graph enrichment active. Use this when ClearlyDefined is slow or
unreachable but you still want deps.dev data. No effect when `--offline` is set.

### `--no-deps-dev`

Skip deps.dev license enrichment. Keeps ClearlyDefined and dep-graph enrichment
active. The fastest enrichment source and rarely needs skipping.

### `--no-deps-dev-graph`

Skip deps.dev transitive dep-graph enrichment. Keeps deps.dev license
enrichment and ClearlyDefined active. Useful when the graph response is large
or unneeded.

### `--enrich-sources <SRC[,SRC...]>`

Comma-separated allowlist of enrichment sources to enable. When provided, ONLY
the listed sources run (overrides all `--no-clearly-defined` /
`--no-deps-dev` / `--no-deps-dev-graph` flags). No effect when `--offline` is
set.

Possible values:

- `deps-dev` — deps.dev license enrichment (declared + observed licenses).
- `clearly-defined` — ClearlyDefined concluded-license enrichment.
- `deps-dev-graph` — deps.dev transitive dep-graph edge enrichment.

```bash
mikebom sbom scan --path . --enrich-sources deps-dev,clearly-defined --output project.cdx.json
```

### `--bind-to-source <PATH>`

Path to a source-tier SBOM document (CDX 1.6 / SPDX 2.3 / SPDX 3 JSON) that
emitted components will be bound to. When set, mikebom emits a
`mikebom:source-document-binding` annotation on each first-party component
whose PURL appears in the source SBOM, plus a document-level cross-document
reference.

Components whose PURL has no source-tier counterpart get an explicit
`binding: unknown { reason: "source-not-found-in-bind-target" }` marker.

```bash
mikebom sbom scan --image my-image:latest --bind-to-source source.cdx.json --output image.cdx.json
mikebom sbom verify-binding --image-sbom image.cdx.json --source-sbom source.cdx.json
```

See also: [Cross-tier binding](../reference/cross-tier-binding.md) for the
full algorithm and per-format carrier shapes.

### `--repo <URL>`

Attach a `repo:` identifier — source repository identity (URL or git-style
ssh URL). Manual override; if both this flag and an auto-detected `repo:`
identifier (from `.git/` origin remote) produce a value, manual wins. Pair
with `--git-ref <revision>` to upgrade to a `git:<repo-url>#<revision>`
identifier (the `git:` identifier supersedes — no separate `repo:` is also
emitted).

```bash
mikebom sbom scan --path . --repo https://github.com/example/proj --git-ref v1.0.0 --output project.cdx.json
```

See also: [Identifiers](../reference/identifiers.md) for the full identity
model.

### `--git-ref <REVISION>`

Pair with `--repo <url>` to emit a `git:<repo>#<revision>` identifier
(commit/branch/tag-anchored). Cannot be supplied without `--repo`. When set,
supersedes the bare `repo:` identifier — only the `git:` identifier is
emitted.

### `--image-id <REF>`

Attach an `image:` identifier — image identity in the form
`[registry/]name[:tag][@sha256:digest]`. Manual override: if `--image <PATH>`
(the scan input) is also set and auto-detection produced an `image:` value,
the manual value wins. Named `--image-id` to avoid colliding with the
`--image <PATH>` scan-input flag.

### `--attestation <IRI>`

Attach an `attestation:` identifier — in-toto attestation IRI. Manual only;
no auto-detection equivalent.

### `--id <SCHEME=VALUE>`

Attach a user-defined identifier in `<scheme>=<value>` form. Repeatable. The
`<scheme>` MUST match regex `^[a-z][a-z0-9_-]*$` and MUST NOT collide with a
built-in scheme (`repo`, `git`, `image`, `attestation`) — use the dedicated
flags for those.

```bash
mikebom sbom scan --path . \
    --id acme_corp_id=svc-alpha-123 \
    --id internal_ticket=PROJ-456 \
    --output project.cdx.json
```

User-defined identifiers ride the `mikebom:identifiers` document-level
annotation; SPDX 3 carries them natively in `Element.externalIdentifier[]`.

See also: [Identifiers](../reference/identifiers.md) for the full per-format
carrier table and decode recipes.

### `--keep-credentials-in-identifiers`

Preserve userinfo (e.g., `USER:TOKEN@host`) in auto-detected git remote URLs
when constructing `repo:` and `git:` identifiers. By default, mikebom strips
userinfo to prevent accidental credential disclosure in published SBOMs.

Use this flag only when the credentials are deliberately non-sensitive
(public read-only deploy token, internal-network-only credentials). Manual
`--repo` / `--git-ref` / `--id` flag values are emitted verbatim regardless
of this flag.

See also: [Identifiers](../reference/identifiers.md) for the credential-
stripping algorithm.

### `--subject-hash <ALGO:HEX>`

Attach a `subject:` identifier declaring "this SBOM describes the artifact
with the given content hash." Format: `sha256:<64-lowercase-hex>` or
`sha512:<128-lowercase-hex>`. Repeatable for multi-subject SBOMs.

```bash
mikebom sbom scan --path . --subject-hash sha256:abc...def --output project.cdx.json
```

On build-tier scans (`mikebom trace run`), subject identifiers are
auto-detected from the in-toto attestation envelope's subject set; manual
flags augment auto-detected entries (deduplicated by exact match). On
source-tier and image-tier scans, no auto-detect runs; manual flags are the
only source of `subject:` identifiers.

### `--component-id <PURL=SCHEME:VALUE>`

Attach a user-defined identifier to a specific component in the emitted SBOM.
The PURL must byte-equal a component's `purl` field; the SCHEME must be a
non-built-in scheme name (built-in schemes `repo`, `git`, `image`,
`attestation`, `subject` are reserved for document-level use). Repeatable.

```bash
mikebom sbom scan --path . \
    --component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-shared-lib-v2" \
    --component-id "pkg:cargo/myapp@0.5.1=acme-asset:myapp-prod-001" \
    --output project.cdx.json
```

If a selector PURL matches multiple components (same PURL across different
bom-ref values), the identifier is attached to ALL matching components. If a
selector matches zero components, the scan logs a warning and continues.

See also: [Identifiers](../reference/identifiers.md) for component-level
identifier semantics.

### `--root-name <NAME>`

Override the auto-derived `metadata.component.name` of the emitted SBOM.
Useful when scanning an arbitrary directory whose basename doesn't reflect
the operator-meaningful project identity. Accepts any non-empty UTF-8 except
whitespace, control characters, `?`, and `#`. URL-encoded automatically when
emitted into the PURL `name` segment.

```bash
mikebom sbom scan --path /tmp/extracted --root-name acme-platform --output platform.cdx.json
```

When this flag is set on a manifest-driven scan (Cargo, npm, pip, gem,
Maven, Go), the manifest-derived main-module component is dropped entirely
from the emitted SBOM (clean replacement).

### `--root-version <VERSION>`

Override the auto-derived `metadata.component.version`. Same validation
rules as `--root-name`. Independent — can be set without `--root-name` and
vice versa. When unset, falls through to the auto-derived version
(typically `0.0.0` for arbitrary directories or the manifest-derived
version for project scans).

```bash
mikebom sbom scan --path . --root-name acme-platform --root-version 2.4.1 --output platform.cdx.json
```

### `--creator <TYPE: NAME>`

Attach a creator entry to the emitted SBOM. Repeatable. Form:
`<Type>: <Name>` where `<Type>` is one of `Tool`, `Organization`, `Person`
(case-sensitive).

Each entry lands at the standards-native creator/tools field of every
emitted format:

- CDX 1.6 `metadata.tools.components[]` / `metadata.manufacturer` /
  `metadata.authors[]` (per Type).
- SPDX 2.3 `creationInfo.creators[]` (verbatim).
- SPDX 3 `Tool` / `Organization` / `Person` element in `@graph`.

mikebom's own auto-populated tool/organization entries are preserved
alongside.

```bash
mikebom sbom scan --path . \
    --creator "Tool: trivy@0.50.0" \
    --creator "Organization: Acme Corp" \
    --creator "Person: alice@acme.example" \
    --output project.cdx.json
```

See also: [Identifiers](../reference/identifiers.md) for the per-format
carrier table.

### `--annotator <TYPE: NAME>` + `--annotation-comment <TEXT>`

Attach a document-level annotator. Repeatable. MUST be paired 1:1 with
`--annotation-comment` — each `--annotator` MUST be immediately followed by
exactly one `--annotation-comment`. Form: same `<Type>: <Name>` shape as
`--creator`.

```bash
mikebom sbom scan --path . \
    --annotator "Person: bob@acme.example" \
    --annotation-comment "Reviewed by security team 2026-04-15" \
    --output project.cdx.json
```

See also: [Identifiers](../reference/identifiers.md) for the annotation
landing slots.

### `--metadata-comment <TEXT>`

Free-text comment about the SBOM document as a whole. Single-valued. Lands
at SPDX 2.3 `creationInfo.comment`, SPDX 3 `Annotation` element of type
`OTHER`, CDX 1.6 `bom.annotations[]`.

```bash
mikebom sbom scan --path . --metadata-comment "Generated for SOC2 audit 2026-Q2" --output project.cdx.json
```

### `--scan-target-name <NAME>`

Operator-supplied override for the document/SBOM-level name field. Lands at
SPDX 2.3 document `name`, SPDX 3 `software_Sbom.name`, CDX 1.6
`metadata.component.name`.

When both `--scan-target-name` and `--root-name` are set on a CDX emission,
`--root-name` takes precedence on `metadata.component.name` (a stderr
warning is emitted). On SPDX 2.3 / SPDX 3 the two flags target different
fields and both are honored independently.

### `--metadata-file <PATH>`

Path to a JSON sidecar file containing user-supplied metadata. Schema:

```json
{
  "creators": ["Tool: trivy@0.50.0"],
  "annotators": [{"type_name": "Person: bob@acme.example", "comment": "reviewed"}],
  "metadata_comment": "Generated for SOC2 audit",
  "scan_target_name": "acme-platform"
}
```

`deny_unknown_fields` applies. Array fields merge additively with their flag
counterparts (file values come first); single-valued fields fail with a
conflict error if specified in both.

```bash
mikebom sbom scan --path . --metadata-file metadata.json --output project.cdx.json
```

### `--sbom-type <TYPE>`

Override the auto-detected SBOM type with an operator-asserted CISA SBOM
Type. Valid values: `design`, `source`, `build`, `analyzed`, `deployed`,
`runtime`. Document-level only — per-component `mikebom:sbom-tier`
annotations preserve auto-detected values.

When set, CDX `metadata.lifecycles[]`, SPDX 2.3 `creationInfo.comment`
"Observed lifecycle phases", and SPDX 3
`software_Sbom.software_sbomType[]` all collapse to a single-element output
reflecting the asserted type.

```bash
mikebom sbom scan --path . --sbom-type build --output build.cdx.json
```

See also: [SBOM types](../reference/sbom-types.md) for the four-column
equivalence reference and per-format field positions.

### `--spdx2-relationship-compat <PROFILE>`

Selects the SPDX 2.3 relationship-type vocabulary mikebom uses for
scoped dependency edges (dev, build, test). Only affects the
`spdx-2.3-json` format — CDX and SPDX 3 emission are unaffected.

**Both modes are spec-conformant, but they are not equivalent.** The
SPDX 2.3 spec defines `DEV_DEPENDENCY_OF`, `BUILD_DEPENDENCY_OF`,
and `TEST_DEPENDENCY_OF` for exactly the purpose of expressing
dev/build/test scope on a dependency edge — the spec's intent is
clearly that you should use the most specific field that applies.
Constitution Principle X (Transparency) further requires mikebom to
default to the spec-native mechanism that preserves the most
consumer-actionable signal. mikebom defaults to `full` for both
reasons: we want more transparency in SBOM output, not less.

`basic` is provided as an explicit downshift for compatibility with
downstream tooling that doesn't implement the typed scoped variants.
Choose it deliberately, knowing you're trading spec-rich expression
for tool-compat reach.

Valid values:

- **`full`** (default): emit the spec-native typed reversed-direction
  variants `DEV_DEPENDENCY_OF`, `BUILD_DEPENDENCY_OF`,
  `TEST_DEPENDENCY_OF`. Every scoped edge carries its scope in the
  relationship type itself — the SPDX 2.3 spec's purpose-built
  field. Use this when emitting SBOMs for tooling that implements
  the full SPDX 2.3 relationshipType enum, and as the standing
  default for any output you intend to be maximally informative.
- **`basic`**: emit every dep, regardless of scope, as a natural-
  direction `DEPENDS_ON` edge. Use this when emitting SBOMs for
  downstream tooling that only implements the basic relationship
  vocabulary (e.g., Trivy, Syft, and tooling built on top of them).
  Such consumers would silently drop the typed scoped variants
  altogether, so collapsing scoped deps to `DEPENDS_ON` makes the
  graph readable to them — at the cost of moving the dev/build/test
  signal off the edge and onto the Package annotation.

**Crucially, the scope distinction is preserved in BOTH modes via the
`mikebom:lifecycle-scope` annotation on the target Package** (values
`development` / `build` / `test`; absent on runtime deps). This means
the dev/build/test signal is always recoverable from the document
itself — `full` puts it both on the edge and on the Package; `basic`
puts it on the Package only. This is consumer-critical signal:
vulnerability scanners, license auditors, and deployment-policy tools
need it to distinguish a CVE on a shipped component from one against
a test-only dep like `testify` or `junit`.

```bash
# Default — full SPDX 2.3 relationship vocabulary (typed scoped variants).
mikebom sbom scan --path . --format spdx-2.3-json --output project.spdx.json

# Basic vocabulary only — every dep emits as natural-direction DEPENDS_ON.
mikebom sbom scan --path . \
  --spdx2-relationship-compat basic \
  --format spdx-2.3-json \
  --output project.spdx.json
```

See also: [SBOM format mapping](../reference/sbom-format-mapping.md)
rows B2 + C42 for the full cross-format consumer story.

---

## `mikebom sbom verify`

Verify a signed attestation (DSSE envelope) against a key, identity, or
in-toto layout.

### Quick reference

| Flag | Type | Default | Description |
|---|---|---|---|
| `--public-key <PEM>` | path | (none) | PEM public key (mutually exclusive with `--identity`). |
| `--identity <PATTERN>` | string | (none) | Keyless identity (email, URL, glob). |
| `--layout <PATH>` | path | (none) | Verify against an in-toto layout. |
| `--expected-subject <PATH>` | path (repeatable) | (none) | Verify on-disk SHA-256 matches a subject. |
| `--no-transparency-log` | bool | off | Tolerate keyless envelopes without Rekor proof. |
| `--fulcio-url <URL>` | URL | `https://fulcio.sigstore.dev` | Custom Fulcio URL. |
| `--rekor-url <URL>` | URL | `https://rekor.sigstore.dev` | Custom Rekor URL. |
| `--json` | bool | off | Emit a structured `VerificationReport` to stdout. |

### `<ATTESTATION>` (positional, required)

Path to a signed `.json` / `.dsse` attestation file.

```bash
mikebom sbom verify build.attestation.json --public-key signer.pub
```

### `--public-key <PEM>`

PEM-encoded public key expected to have signed the attestation. Mutually
exclusive with `--identity`.

### `--identity <PATTERN>`

Expected signer identity (email, URL, or glob) for keyless-signed
attestations.

```bash
mikebom sbom verify build.attestation.json --identity 'alice@acme.example'
```

### `--layout <PATH>`

Verify against an in-toto layout. When omitted, only envelope-level checks
run (signature + subject).

### `--expected-subject <PATH>`

Verify the on-disk SHA-256 of `<PATH>` matches one of the attestation's
subjects. Repeatable for multi-subject envelopes.

```bash
mikebom sbom verify build.attestation.json \
    --public-key signer.pub \
    --expected-subject ./my-binary
```

### `--no-transparency-log`

Don't require a Rekor inclusion proof in the envelope. Keyless-mode only.

### `--fulcio-url <URL>` / `--rekor-url <URL>`

Override the Fulcio certificate-issuance URL / Rekor transparency-log URL.
Useful for private sigstore instances. Defaults
`https://fulcio.sigstore.dev` and `https://rekor.sigstore.dev`.

### `--json`

Emit a structured verification report to stdout. Non-zero exit codes:
`1` crypto failure, `2` envelope failure, `3` layout failure.

---

## `mikebom sbom enrich`

Add license, VEX, and supplier data to an existing SBOM, applying RFC 6902
JSON Patch ops with per-patch provenance recording.

### Quick reference

| Flag | Type | Default | Description |
|---|---|---|---|
| `--patch <PATH>` | path (repeatable) | (none) | RFC 6902 JSON Patch file. |
| `--author <NAME>` | string | `unknown` | Recorded author of the enrichment. |
| `--output <PATH>` | path | (overwrite input) | Output path. |
| `--base-attestation <PATH>` | path | (none) | Attestation the SBOM was derived from (SHA-256 embedded). |
| `--vex-overrides <PATH>` | path | (none) | OpenVEX 0.2.0 document with statements to propagate. |
| `--vex-propagation-mode <MODE>` | enum (`permissive`, `caveated`, `strict`) | `caveated` | Binding-aware VEX propagation mode. |
| `--skip-licenses` | bool | off | Skip license enrichment. |
| `--skip-supplier` | bool | off | Skip supplier enrichment. |
| `--skip-vex` | bool | off | Skip VEX enrichment. |
| `--deps-dev-timeout <MS>` | u64 | `5000` | Timeout per deps.dev API call. |
| `--json` | bool | off | JSON summary to stdout. |

### `<SBOM_FILE>` (positional, required)

Path to the CycloneDX SBOM to enrich in place.

### `--patch <PATH>`

RFC 6902 JSON Patch file. Repeatable: patches are applied in order (later
ops see earlier ones). At least one patch is required.

```bash
mikebom sbom enrich project.cdx.json \
    --patch licenses.patch.json \
    --patch vex.patch.json \
    --author "alice@acme.example"
```

### `--author <NAME>`

Recorded author of the enrichment. Defaults to `unknown` with a warning.
Lands in the per-patch provenance property.

### `--output <PATH>`

Output path. Defaults to overwriting the input SBOM in place.

### `--base-attestation <PATH>`

Optional path to the attestation the SBOM was derived from. Its SHA-256
gets embedded so verifiers can walk back to the attested source.

### `--vex-overrides <PATH>`

Path to a source-tier OpenVEX 0.2.0 document whose statements will be
propagated onto components in `<SBOM_FILE>`. Each propagation is gated by
the target component's `mikebom:source-document-binding` strength per
`--vex-propagation-mode`.

### `--vex-propagation-mode <MODE>`

VEX propagation mode. Possible values:

- `permissive` — pre-072 behavior; propagate by PURL match without
  binding check.
- `caveated` *(default)* — propagate but tag binding-unverified
  statements with `mikebom:vex-binding-status: unverified`.
- `strict` — refuse propagation when binding strength != Verified
  (exit non-zero).

See also: [Cross-tier binding](../reference/cross-tier-binding.md) for the
binding strength definitions.

### `--skip-licenses` / `--skip-supplier` / `--skip-vex`

Skip the corresponding enrichment phase. Useful for narrow re-runs (e.g.,
only re-enrich VEX after a triage pass).

### `--deps-dev-timeout <MS>`

Timeout per deps.dev API call, in milliseconds. Default `5000`.

### `--json`

Print a JSON summary to stdout.

---

## `mikebom sbom verify-binding`

Verify that an image-tier SBOM's per-component `mikebom:source-document-binding`
annotations match the recompute against a source-tier SBOM. Exits non-zero on
any verification failure.

### Quick reference

| Flag | Type | Default | Description |
|---|---|---|---|
| `--image-sbom <PATH>` | path (required) | — | Image-tier SBOM (CDX/SPDX 2.3/SPDX 3 JSON). |
| `--source-sbom <PATH>` | path (required) | — | Source-tier SBOM (JSON). |
| `--format <FORMAT>` | enum (`table`, `json`) | `table` | Output format. |

### `--image-sbom <PATH>`

Path to the image-tier SBOM (JSON). Required.

### `--source-sbom <PATH>`

Path to the source-tier SBOM (JSON). Required.

### `--format <FORMAT>`

Output format. Possible values:

- `table` *(default)* — plain-text per-row table.
- `json` — `VerifyReport` JSON for CI pipelines / machine consumption.

```bash
mikebom sbom verify-binding \
    --image-sbom image.cdx.json \
    --source-sbom source.cdx.json \
    --format json
```

See also: [Cross-tier binding](../reference/cross-tier-binding.md) for the
verification algorithm.

---

## `mikebom sbom trace-binding`

Trace an image-tier component back to its candidate source-tier SBOMs. For
each instance of the supplied PURL in the image SBOM, reports the binding
state against every candidate source SBOM. Always exits 0 (informational).

### Quick reference

| Flag | Type | Default | Description |
|---|---|---|---|
| `--component-purl <PURL>` | string (required) | — | PURL of the component to trace. |
| `--image-sbom <PATH>` | path (required) | — | Image-tier SBOM. |
| `--source-sbom <PATH>` | path | (none) | Single candidate source-tier SBOM. |
| `--candidate-sources-dir <DIR>` | path | (none) | Directory of candidate source-tier SBOMs. |
| `--format <FORMAT>` | enum (`table`, `json`) | `table` | Output format. |

`--source-sbom` and `--candidate-sources-dir` are mutually exclusive.

### `--component-purl <PURL>`

PURL of the component to trace. Required.

### `--image-sbom <PATH>`

Image-tier SBOM (CDX/SPDX 2.3/SPDX 3 JSON). Required.

### `--source-sbom <PATH>`

Single candidate source-tier SBOM. Mutually exclusive with
`--candidate-sources-dir`.

### `--candidate-sources-dir <DIR>`

Directory containing candidate source-tier SBOMs. Every `*.cdx.json`,
`*.spdx.json`, `*.spdx3.json`, or `*.json` file in the directory is loaded
and tested.

```bash
mikebom sbom trace-binding \
    --component-purl "pkg:cargo/serde@1.0.0" \
    --image-sbom image.cdx.json \
    --candidate-sources-dir ./source-sboms
```

### `--format <FORMAT>`

Output format. Possible values:

- `table` *(default)* — plain-text per-row table.
- `json` — `TraceReport` JSON for CI pipelines / machine consumption.

See also: [Cross-tier binding](../reference/cross-tier-binding.md) for the
binding-state vocabulary.

---

## `mikebom trace run`

> **Status: experimental.** Linux-only. Adds ~2-3× wall-clock overhead on
> syscall-heavy builds; requires CAP_BPF + CAP_PERFMON; coverage gaps on
> `openat2` and `io_uring`. Prefer `mikebom sbom scan` unless you need a
> trace-bound attestation.

Capture a build trace and produce both an SBOM and an in-toto attestation
in one step.

### Quick reference

| Flag | Type | Default | Description |
|---|---|---|---|
| `<COMMAND>...` | positional (required) | — | Build command to trace (after `--`). |
| `--sbom-output <PATH>` | path | `mikebom.cdx.json` | SBOM output path. |
| `--attestation-output <PATH>` | path | `mikebom.attestation.json` | Attestation output path. |
| `--format <FORMAT>` | enum | `cyclonedx-json` | SBOM output format. |
| `--no-enrich` | bool | off | Skip enrichment step. |
| `--include-source-files` | bool | off | Also include observed source files. |
| `--no-hashes` | bool | off | Omit per-component hashes. |
| `--trace-children` | bool | off | Follow forked children. |
| `--libssl-path <PATH>` | path | auto-detect | Override libssl.so path for uprobe attachment. |
| `--ring-buffer-size <BYTES>` | u32 | `8388608` | BPF ring buffer size (must be power of two). |
| `--timeout <SECONDS>` | u64 | `0` (no timeout) | Trace timeout. |
| `--skip-purl-validation` | bool | off | Skip online PURL existence validation. |
| `--lockfile <PATH>` | path | (none) | Lockfile for dependency-relationship enrichment. |
| `--artifact-dir <DIR>` | path (repeatable) | (none) | Post-trace artifact directories. |
| `--auto-dirs` | bool | off | Auto-detect artifact directories. |
| `--json` | bool | off | JSON summary to stdout. |
| `--repo <URL>` | URL | auto-detect (git remote) | Override the auto-detected `repo:` identifier. |
| `--git-ref <REVISION>` | string | auto-detect | Pair with `--repo` for `git:` identifier. |
| `--image-id <REF>` | string | (none) | Attach an `image:` identifier. |
| `--attestation <IRI>` | IRI | (none) | Attach an `attestation:` identifier. |
| `--id <SCHEME=VALUE>` | string (repeatable) | (none) | Attach a user-defined identifier. |
| `--keep-credentials-in-identifiers` | bool | off | Preserve userinfo in auto-detected URLs. |
| `--subject-hash <ALGO:HEX>` | string (repeatable) | auto-detect | Attach a `subject:` identifier. |
| `--component-id <PURL=SCHEME:VALUE>` | string (repeatable) | (none) | Component-level user identifier. |
| `--root-name <NAME>` | string | auto-derived | Override `metadata.component.name`. |
| `--root-version <VERSION>` | string | auto-derived | Override `metadata.component.version`. |
| `--creator <TYPE: NAME>` | string (repeatable) | (none) | Attach a creator entry. |
| `--annotator <TYPE: NAME>` | string (repeatable, paired) | (none) | Document-level annotator. |
| `--annotation-comment <TEXT>` | string (repeatable, paired) | (none) | Pairs with `--annotator`. |
| `--metadata-comment <TEXT>` | string | (none) | SBOM-level free-text comment. |
| `--scan-target-name <NAME>` | string | auto-derived | Operator override for document name. |
| `--metadata-file <PATH>` | path | (none) | JSON sidecar for user metadata. |
| `--sbom-type <TYPE>` | enum | auto-detect | Operator-asserted CISA SBOM Type. |
| `--signing-key <PATH>` | path | (none) | PEM private key for local-key signing. |
| `--signing-key-passphrase-env <NAME>` | env var name | (none) | Env var holding signing-key passphrase. |
| `--keyless` | bool | off | Keyless signing via OIDC → Fulcio → Rekor. |
| `--fulcio-url <URL>` | URL | `https://fulcio.sigstore.dev` | Custom Fulcio URL. |
| `--rekor-url <URL>` | URL | `https://rekor.sigstore.dev` | Custom Rekor URL. |
| `--no-transparency-log` | bool | off | Skip Rekor upload (keyless mode). |
| `--require-signing` | bool | off | Hard-fail if no signing identity is configured. |
| `--subject <PATH>` | path (repeatable) | auto-detect | Explicit subject artifact path. |
| `--attestation-format <FORMAT>` | enum (`witness-v0.1`, `mikebom-v1`) | `witness-v0.1` | Attestation output format. |

### `<COMMAND>...` (positional, required)

Build command to trace. Pass after `--` to separate mikebom flags from the
build command's flags.

```bash
mikebom trace run --sbom-output build.cdx.json -- cargo install ripgrep
```

### `--sbom-output <PATH>`

SBOM output path. Default `mikebom.cdx.json`.

### `--attestation-output <PATH>`

Attestation output path. Default `mikebom.attestation.json`.

### `--format <FORMAT>`

SBOM output format. Default `cyclonedx-json`. See `mikebom sbom scan
--format` for the registered format set.

### `--no-enrich`

Skip the enrichment step entirely (no deps.dev / ClearlyDefined calls).

### `--include-source-files`

Also include observed source files (not just packages). Switches SBOM scope
from `packages` to `source`.

### `--trace-children`

Follow forked children of the traced command. Useful when the build command
spawns subprocesses (cargo → rustc, npm → node, etc.).

### `--libssl-path <PATH>`

Override `libssl.so` path for uprobe attachment. Default: auto-detect from
the build's process environment.

### `--ring-buffer-size <BYTES>`

BPF ring buffer size in bytes (must be a power of two). Default `8388608`
(8 MB). Increase for high-syscall-rate builds where the buffer overflows.

### `--timeout <SECONDS>`

Trace timeout in seconds. `0` means no timeout. Useful for unattended CI
runs that should not hang indefinitely.

### `--skip-purl-validation`

Skip the online PURL existence validation step.

### `--lockfile <PATH>`

Path to a lockfile for dependency-relationship enrichment. Auto-detects
format (`Cargo.lock`, `package-lock.json`, `go.sum`). Unrecognised formats
are logged and skipped.

### `--artifact-dir <DIR>` / `--auto-dirs`

Post-trace artifact-directory scanning. See the `mikebom trace capture`
section below for the full semantics — `trace run` forwards both flags
verbatim to capture.

### `--repo <URL>`, `--git-ref <REVISION>`, `--image-id <REF>`, `--attestation <IRI>`, `--id <SCHEME=VALUE>`, `--keep-credentials-in-identifiers`, `--subject-hash <ALGO:HEX>`, `--component-id <PURL=SCHEME:VALUE>`, `--root-name <NAME>`, `--root-version <VERSION>`, `--creator <TYPE: NAME>`, `--annotator <TYPE: NAME>`, `--annotation-comment <TEXT>`, `--metadata-comment <TEXT>`, `--scan-target-name <NAME>`, `--metadata-file <PATH>`, `--sbom-type <TYPE>`

These flags share semantics with their `mikebom sbom scan` counterparts —
see the `mikebom sbom scan` section above for the full per-flag documentation.

Build-tier-specific notes:

- `--repo` and `--git-ref` auto-detect from `git remote get-url` when the
  invocation cwd is a git checkout; the flag overrides the auto-detected
  value.
- `--subject-hash` augments the auto-detected subject set from the in-toto
  attestation envelope (deduplicated by exact match).

See also: [Identifiers](../reference/identifiers.md) for build-tier
auto-detection semantics.

### `--signing-key <PATH>`

Path to a PEM-encoded private key for local-key DSSE signing. Mutually
exclusive with `--keyless`.

### `--signing-key-passphrase-env <NAME>`

Env var name holding the passphrase for an encrypted `--signing-key`. No
effect on unencrypted keys. No interactive prompt — CI-friendly by design.

### `--keyless`

Keyless signing via OIDC → Fulcio → Rekor. Mutually exclusive with
`--signing-key`. Auto-detects GitHub Actions OIDC tokens.

### `--fulcio-url <URL>` / `--rekor-url <URL>`

Override the Fulcio / Rekor URLs. Defaults `https://fulcio.sigstore.dev`
and `https://rekor.sigstore.dev`.

### `--no-transparency-log`

Skip Rekor upload + inclusion-proof embedding. Keyless-mode only.

### `--require-signing`

Fail if no signing identity was configured. Flips the default "emit
unsigned + warn" behavior to a hard error.

### `--subject <PATH>`

Explicit subject artifact path. Repeatable. When set, auto-detection is
suppressed — mikebom signs exactly what you told it to.

### `--attestation-format <FORMAT>`

Attestation output format. Possible values:

- `witness-v0.1` *(default)* — in-toto Statement v0.1 wrapped around a
  witness attestation-collection (`material` + `command-run` + `product`
  + `network-trace` inner attestors). Directly consumable by `sbomit
  generate` and any go-witness-aware verifier.
- `mikebom-v1` — mikebom's native `BuildTracePredicate` Statement v1.
  Richer network-trace semantics but only mikebom understands it.

---

## `mikebom trace capture`

> **Status: experimental.** Same caveats as `trace run`.

Capture a build trace via eBPF and produce an in-toto attestation. Lower-level
than `trace run` — produces no SBOM, just the attestation.

Exactly one of `--target-pid <PID>` or a command after `--` is required (mutually
exclusive).

### Quick reference

| Flag | Type | Default | Description |
|---|---|---|---|
| `[COMMAND]...` | positional | — | Build command to trace (after `--`). |
| `--target-pid <PID>` | u32 | (none) | Existing PID to attach to. |
| `--output <PATH>` | path | `mikebom.attestation.json` | Attestation output path. |
| `--trace-children` | bool | off | Follow forked children. |
| `--libssl-path <PATH>` | path | auto-detect | Override libssl.so path. |
| `--go-binary <PATH>` | path | (none) | Go binary for Go-specific instrumentation. |
| `--ring-buffer-size <BYTES>` | u32 | `8388608` | BPF ring buffer size (must be power of two). |
| `--timeout <SECONDS>` | u64 | `0` | Trace timeout. |
| `--artifact-dir <DIR>` | path (repeatable) | (none) | Post-trace artifact directories. |
| `--auto-dirs` | bool | off | Auto-detect artifact directories. |
| `--json` | bool | off | JSON summary to stdout. |
| `--signing-key <PATH>` | path | (none) | PEM private key for local-key signing. |
| `--signing-key-passphrase-env <NAME>` | env var name | (none) | Env var holding signing-key passphrase. |
| `--keyless` | bool | off | Keyless signing via OIDC → Fulcio → Rekor. |
| `--fulcio-url <URL>` | URL | `https://fulcio.sigstore.dev` | Custom Fulcio URL. |
| `--rekor-url <URL>` | URL | `https://rekor.sigstore.dev` | Custom Rekor URL. |
| `--no-transparency-log` | bool | off | Skip Rekor upload (keyless mode). |
| `--require-signing` | bool | off | Hard-fail if no signing identity is configured. |
| `--subject <PATH>` | path (repeatable) | auto-detect | Explicit subject artifact path. |
| `--attestation-format <FORMAT>` | enum | `witness-v0.1` | Attestation output format. |

### `--target-pid <PID>`

Existing process ID to attach the trace to. Mutually exclusive with the
`-- <COMMAND>` form.

### `--output <PATH>`

Attestation output path. Default `mikebom.attestation.json`.

### `--artifact-dir <DIR>`

Directories to scan for freshly-landed artifact files after the traced
command exits. Any recognised package file (`.deb`, `.crate`, `.whl`,
`.tar.gz`, …) whose mtime is newer than the trace start is hashed and
added to the file-access record. The resulting SBOM (when `trace run`
produces one) carries real content hashes even when the kernel-side
kprobe misses the output-file open. Repeatable or comma-separated.

### `--auto-dirs`

Auto-detect artifact directories from the traced command. Matches
`argv[0]` against a table of known build tools (cargo, pip, npm, go,
apt-get, …) and merges the canonical cache paths with any explicit
`--artifact-dir` values. Skipped for shell-wrapped commands
(`bash -c "…"`) — those are too dynamic to introspect.

### `--go-binary <PATH>`

Path to a Go binary for Go-specific instrumentation. Used to attach
uprobes to the Go runtime's TLS internals (Go binaries don't link
`libssl.so` so the standard `--libssl-path` path doesn't apply).

### Other flags

`--trace-children`, `--libssl-path`, `--ring-buffer-size`, `--timeout`,
`--json`, `--signing-key`, `--signing-key-passphrase-env`, `--keyless`,
`--fulcio-url`, `--rekor-url`, `--no-transparency-log`, `--require-signing`,
`--subject`, `--attestation-format` — see the `mikebom trace run` section
above; identical semantics.

---

## `mikebom policy init`

Generate a starter in-toto layout bound to a functionary key. Use the emitted
layout with `mikebom sbom verify --layout` to enforce functionary + step-name
policy on signed attestations. Layouts are standard in-toto — any
in-toto-aware verifier accepts them.

### Quick reference

| Flag | Type | Default | Description |
|---|---|---|---|
| `--functionary-key <PATH>` | path (required) | — | PEM-encoded public key of the expected signer. |
| `--output <PATH>` | path | `layout.json` | Output path. |
| `--step-name <NAME>` | string | `build-trace-capture` | Name of the layout's single step. |
| `--expires <DURATION>` | duration | `1y` | Validity window. Accepts `1y`, `6m`, `18mo`, `2y`, `30d`, `52w`. |
| `--readme <TEXT>` | string | (none) | Embedded human-readable description. |

### `--functionary-key <PATH>`

PEM-encoded public key of the expected signer. Required.

```bash
mikebom policy init --functionary-key signer.pub --output layout.json
```

### `--output <PATH>`

Where to write the layout. Default `layout.json`.

### `--step-name <NAME>`

Name of the single step the layout expects. Default `build-trace-capture`.

### `--expires <DURATION>`

How long the layout is valid. Default `1y`. Accepted suffixes: `y` (years),
`m`/`mo` (months), `d` (days), `w` (weeks).

### `--readme <TEXT>`

Optional human-readable description embedded in the layout.

---

## Output formats

The `--format` flag accepts a comma-separated list and is itself repeatable.
Default is `cyclonedx-json`. Duplicates dedupe silently.

| Format id | Status | Default filename |
|---|---|---|
| `cyclonedx-json` | Stable. Default. CycloneDX 1.6 JSON. | `mikebom.cdx.json` |
| `spdx-2.3-json` | Stable. SPDX 2.3 JSON. Validates against the official SPDX 2.3 JSON schema. | `mikebom.spdx.json` |
| `spdx-3-json` | Stable. SPDX 3.0.1 JSON-LD. Production-grade output with native-field + annotation parity vs. CDX and SPDX 2.3. | `mikebom.spdx3.json` |
| `spdx-3-json-experimental` *(deprecated)* | Byte-identical to `spdx-3-json`; emits a stderr deprecation notice. | `mikebom.spdx3-experimental.json` |

**OpenVEX sidecar** — when the scan produces VEX statements AND SPDX 2.3
output is requested, mikebom co-emits an OpenVEX 0.2.0 JSON file alongside
the SPDX file. The SPDX document carries a `DocumentRef-OpenVEX` entry in
`externalDocumentRefs` with a SHA-256 of the sidecar bytes. Use
`--output openvex=<path>` to retarget the sidecar.

See [Generation](../architecture/generation.md) for the per-format builder
architecture and [SBOM format mapping](../reference/sbom-format-mapping.md)
for the cross-format data-placement map.

---

## Authenticating to private registries

When `--image <ref>` resolves to an OCI reference (rather than a tarball),
mikebom uses the same Docker keychain that `docker pull` uses —
`~/.docker/config.json` (or `$DOCKER_CONFIG/config.json` if set). No
mikebom-specific credential file or CLI flag is required.

Credentials resolve in this priority order, matching Docker's:

1. `credHelpers.<registry>` — per-registry helper override (AWS ECR via
   `docker-credential-ecr-login`, Google Artifact Registry via
   `docker-credential-gcloud`).
2. `credsStore` — registry-wide helper (`osxkeychain`, `wincred`,
   `secretservice`, `pass`, `desktop`).
3. `auths.<registry>.auth` — direct credentials, base64-encoded as
   `user:password`.
4. `auths.<registry>.identitytoken` — registry-issued bearer token.

Example `~/.docker/config.json` shapes:

```json
{
  "auths": {
    "ghcr.io": { "auth": "<base64('username:ghp_xxx')>" }
  }
}
```

```json
{
  "auths": { "ghcr.io": {} },
  "credsStore": "desktop"
}
```

```json
{
  "credHelpers": {
    "123456789012.dkr.ecr.us-east-1.amazonaws.com": "ecr-login"
  }
}
```

Behavior notes:

- Anonymous fallback when no entry matches.
- Helper failure (non-zero exit, "credentials not found" sentinel) falls
  through to anonymous so unrelated public-image scans aren't blocked.
- Credentials are never logged at any verbosity level.
- AWS ECR token TTL is 12 hours — never matters for a one-shot CLI run.
- Private GHCR images need `read:packages` on the PAT.

---

## OCI layer caching

OCI distribution-spec blobs (image config + each layer) are content-addressed
by SHA-256, so caching them on disk is correct by construction: a cache hit
on a digest is identical-bytes to a fresh network fetch of that digest.
mikebom caches every blob it pulls; subsequent scans of the same image
complete in seconds rather than tens of seconds.

The image's manifest is intentionally NOT cached — a floating tag like
`:latest` re-fetches the manifest every time so updates are detected.

**Cache location** (priority order):

1. `$MIKEBOM_OCI_CACHE_DIR` (when set non-empty).
2. `$XDG_CACHE_HOME/mikebom/oci-layers` (Linux convention).
3. `$HOME/Library/Caches/mikebom/oci-layers` (macOS).
4. `$HOME/.cache/mikebom/oci-layers` (fallback).

**Layout**: `<cache-dir>/sha256/<64-hex>` per blob.

**Eviction**: default 10 GB cap. Override with `--oci-cache-size <bytes>`
or `MIKEBOM_OCI_CACHE_SIZE=<bytes>`.

**Disabling**: pass `--no-oci-cache` (or set `MIKEBOM_OCI_CACHE=0`) to skip
the cache entirely for one invocation.

**Clearing**: `rm -rf "$XDG_CACHE_HOME/mikebom/oci-layers"` (or the macOS /
fallback equivalent). mikebom doesn't ship a `--clear-oci-cache` command.

---

## See also

- [Quickstart](quickstart.md) — operator onboarding recipes.
- [Configuration](configuration.md) — global flags and environment variables.
- [SBOM types](../reference/sbom-types.md) — the `--sbom-type` flag and CISA
  SBOM Types vocab.
- [Identifiers](../reference/identifiers.md) — the four-layer identity model
  and per-flag identity behavior.
- [Cross-tier binding](../reference/cross-tier-binding.md) — `--bind-to-source`,
  `verify-binding`, and `trace-binding` flow.
- [Generation](../architecture/generation.md) — per-format builder design.
