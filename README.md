# mikebom

**NOTE: This tool is in no way production ready. This requires a lot more hardening**

A toolkit for working with software bills of materials end-to-end:

- **Generates SBOMs** from source trees, package caches, and container
  images with lockfile-aware dep-graph extraction, emitting **CycloneDX
  1.6**, **SPDX 2.3**, and **SPDX 3.0.1** with SHA-256 hashes + evidence
  + real dependency relationships. On Linux, optionally captures build-
  time provenance via eBPF.
- **Analyzes SBOMs** — verifies DSSE-signed attestations against keys /
  Fulcio identities / in-toto layouts, and cross-checks already-emitted
  CycloneDX / SPDX 2.3 / SPDX 3.0.1 outputs for per-datum × per-format
  coverage parity via `mikebom sbom parity-check`.
- **Modifies and enriches SBOMs** — today, `mikebom sbom enrich`
  applies RFC 6902 JSON Patches with provenance metadata recorded as
  `mikebom:enrichment-patch[N]` properties. Richer modification
  workflows (license backfill, supplier resolution, VEX merging) are
  on the roadmap.

> **Status: pre-1.0 alpha.** Pre-built binaries are published as GitHub
> Release assets; no crates.io release yet.
>
> - **Stable** — `mikebom sbom scan`, `mikebom sbom verify`,
>   `mikebom sbom enrich`, `mikebom sbom parity-check`,
>   `mikebom sbom verify-binding`, `mikebom sbom trace-binding`,
>   `mikebom policy init`, and `mikebom attestation validate`.
>   Cross-platform, no special privileges.
> - **Experimental, Linux-only** — `mikebom trace capture` /
>   `mikebom trace run`. eBPF-based build-time capture that produces
>   attestations bound to the actual build event. Requires CAP_BPF +
>   CAP_PERFMON and adds ~2–3× wall-clock overhead on syscall-heavy
>   builds.
>
> See [`docs/user-guide/cli-reference.md`](docs/user-guide/cli-reference.md)
> for the per-flag operator reference and
> [`CHANGELOG.md`](CHANGELOG.md) for what shipped when.

## Why

Scan-mode reads lockfiles + package manifests + per-module metadata to
build a proper CycloneDX with:

- **SHA-256 content hashes** on every component, from the bytes on
  disk.
- **Real dep-graph edges**, not a flat fan-out. Per-module `go.mod`
  from the module cache drives the Go graph; `Cargo.lock` drives the
  Rust graph; for Maven there's a full layered strategy
  ([design notes](docs/design-notes.md)) that resolves through
  `~/.m2/` caches, parent POMs, BOM imports, and — when needed —
  deps.dev.
- **CycloneDX evidence blocks** pointing back to the specific file
  path and parser technique that identified each component, with
  confidence scoring.
- **Strict PURL encoding** that round-trips through the
  `packageurl-python` reference implementation (including
  `+` → `%2B` encoding across every ecosystem; `epoch=0` omission
  on RPM; lexicographic qualifier sort).
- **Compiled-binary identity across Linux, macOS, and Windows** —
  every binary mikebom scans carries a cross-platform identity in
  the SBOM:
  - **ELF (Linux):** `NT_GNU_BUILD_ID`, `DT_RPATH` / `DT_RUNPATH`,
    `.gnu_debuglink` → `mikebom:elf-build-id` /
    `mikebom:elf-runpath` / `mikebom:elf-debuglink`. The build-id is
    the canonical Linux binary-identity field used by `eu-unstrip`,
    `coredumpctl`, `debuginfod`, and `*-dbgsym` packaging.
  - **Mach-O (macOS / iOS):** `LC_UUID`, `LC_RPATH`, and minimum-OS
    version (`LC_BUILD_VERSION` or `LC_VERSION_MIN_*`) →
    `mikebom:macho-uuid` / `mikebom:macho-rpath` /
    `mikebom:macho-min-os`. The UUID is what `dwarfdump`,
    `xcrun symbolicatecrash`, the macOS crash reporter, and every
    `*.dSYM` bundle key on for symbol matching. Fat / universal
    binaries report from the first slice.

    Plus codesign metadata from the `LC_CODE_SIGNATURE` SuperBlob's
    CodeDirectory: `mikebom:macho-codesign-identifier` (e.g.
    `com.apple.ls`), `mikebom:macho-codesign-flags` (decoded names
    from the flags bitfield — `hardened-runtime`,
    `library-validation`, `adhoc`, etc.), and
    `mikebom:macho-codesign-team-id` (10-char Apple Team ID for
    developer-signed binaries). This is what `codesign -dvv` reads.
  - **PE (Windows):** CodeView pdb-id (`<guid>:<age>` from
    `IMAGE_DIRECTORY_ENTRY_DEBUG`), machine type
    (`IMAGE_FILE_HEADER.Machine`), and subsystem
    (`IMAGE_OPTIONAL_HEADER.Subsystem`) → `mikebom:pe-pdb-id` /
    `mikebom:pe-machine` / `mikebom:pe-subsystem`. The pdb-id is
    what Microsoft Symbol Server, Mozilla / Chromium symbol stores,
    WinDbg, and drmingw use to locate matching `.pdb` files.

  All twelve annotations emit symmetrically across CDX, SPDX 2.3,
  and SPDX 3, making cross-image binary dedup, debug-symbol
  correlation, and signing-identity provenance a direct lookup
  regardless of OS.
- **Go VCS provenance** — extracts `vcs.revision` (commit SHA),
  `vcs.time` (RFC 3339 build timestamp), and `vcs.modified` (dirty-tree
  flag) from every Go binary's BuildInfo. Surfaced as
  `mikebom:go-vcs-revision` / `mikebom:go-vcs-time` /
  `mikebom:go-vcs-modified` on the main-module entry. Same data
  `go version -m` shows, baked into the SBOM so consumers don't have
  to shell out.
- **Rust crate-closure provenance** — extracts the full build-time
  crate dependency closure from the `.dep-v0` linker section that
  [`cargo auditable build`](https://github.com/rust-secure-code/cargo-auditable)
  embeds. Each crate becomes a `pkg:cargo/<name>@<version>` component
  with `evidence-kind = "cargo-auditable"` and `confidence = "high"`
  (build-time truth — distinct from `embedded-version-string`'s
  heuristic tier), `parent_purl` cross-linking back to the file-level
  binary. The binary itself gains a
  `mikebom:detected-cargo-auditable = true` cross-link annotation
  (the Rust analog of `mikebom:detected-go = true`). Cargo wrappers
  shipped with **Debian Trixie+, Fedora 40+, Alpine Edge, and the
  official Rust container images** auto-enable the embedding, so most
  Rust binaries built in those environments surface their full crate
  closure without source access. Cross-format: ELF / Mach-O / PE.
- **Curated embedded-version-string detection** for **11
  high-CVE-volume native libraries** statically-linked into compiled
  binaries — the heuristic-tier counterpart to source-tree manifest
  parsing. mikebom walks the binary's read-only string region
  (`.rodata` / `__TEXT,__cstring` / `.rdata` — never the full image,
  to bound the false-positive surface) and recognises each library's
  canonical version banner anchored at a NUL boundary:
  - **Crypto / TLS:** OpenSSL, BoringSSL, LibreSSL, GnuTLS
  - **Compression / data:** zlib, SQLite
  - **Networking:** curl
  - **Regex:** PCRE, PCRE2
  - **Compiler / runtime:** LLVM, OpenJDK (handles both modern JEP-322
    `21.0.1+12` and legacy Java-8 `8u362-b09`)

  Each detection emits a `pkg:generic/<library>@<version>` component
  with `mikebom:evidence-kind = "embedded-version-string"` and
  `mikebom:confidence = "heuristic"`, so downstream CVE matchers
  (Vex / OSV / NVD / Kusari Inspector) have pre-resolved coordinates
  to query against — no need to know in advance which libraries a
  binary statically links.

On top of scan-mode, mikebom adds:

- **Signed DSSE envelope attestations** via sigstore (local-key or
  keyless OIDC → Fulcio → Rekor).
- **In-toto layout verification** for build-policy enforcement.
- **Witness-collection v0.1** output compatible with `sbomit generate`
  and any go-witness-aware verifier.

## What kind of SBOM does mikebom emit?

A common question when comparing mikebom's component count to
trivy's, syft's, or another scanner's: **are we counting the
same thing?** Often the answer is no — and the gap is a scope
choice, not a bug. mikebom self-describes its scope on every
output so consumers can answer the question by reading the SBOM
rather than reverse-engineering it from the component list.

### Two axes

mikebom uses two orthogonal scope axes:

**1. Document-level scope mode** — the answer to "what set of
things is this SBOM trying to describe?"

| Mode | Meaning | When |
|---|---|---|
| **Artifact** (default for `--image`) | On-disk components only — every emitted component has its bytes physically present in the scanned tree or image. CDX phase aggregation typically shows `operations` (deployed runtime) plus build-time tiers from installed packages. | Scanning a container image or a built artifact — answers "what's actually here right now?" |
| **Manifest** (default for `--path`) | On-disk components plus declared-but-not-on-disk transitives (lockfile-pinned but absent from local caches, deps.dev-resolved, Maven cache-miss BFS). | Scanning a source tree — answers "what would this build pull in?" |

The mode is controlled by `--include-declared-deps` (auto-on for
`--path`, auto-off for `--image`; explicit override available).

**2. Per-component lifecycle tier** — the answer to "where in
the build/deploy lifecycle was this component observed?"
Annotated as `mikebom:sbom-tier` on every component, with five
values:

| Tier | Meaning |
|---|---|
| `design` | Declared but not pinned (e.g., `>=1.0` ranges in `requirements.txt`). |
| `source` | Lockfile-pinned, byte-resolvable (e.g., `Cargo.lock`, `package-lock.json`). |
| `build` | Captured during a live build event via eBPF tracing. |
| `deployed` | Installed in the runtime image — dpkg, apk, rpm, populated `node_modules`, populated venv `dist-info`. |
| `analyzed` | Artifact file on disk, identified by filename + content hash. |

### How mikebom self-describes scope in each format

mikebom ships scope information through native fields in every
output format, not as `mikebom:`-prefixed extensions, so any
spec-compliant SBOM reader picks it up:

| Format | Document-level scope hint | Per-component tier |
|---|---|---|
| **CycloneDX 1.6** | `metadata.lifecycles[]` (aggregated from per-component tiers, deduplicated, sorted) + `compositions[].aggregate` | `properties[].name = "mikebom:sbom-tier"` |
| **SPDX 2.3** | `creationInfo.comment` (free-text scope summary) | `packages[].annotations[]` with `mikebom:sbom-tier` |
| **SPDX 3.0.1** | `SpdxDocument.comment` (free-text scope summary) + `software_Sbom.software_sbomType[]` (native enum) | top-level `annotations[]` with `mikebom:sbom-tier` |

For the operator-facing **SBOM type** classification (CISA Design /
Source / Build / Analyzed / Deployed / Runtime), per-format `jq`
recipes, the four-column equivalence table, and the
`--sbom-type <type>` operator-assert flag, see
[`docs/reference/sbom-types.md`](docs/reference/sbom-types.md).

### Industry / consumer terminology bridge

When operators compare mikebom's count to other scanners, the
delta usually traces back to a different scope choice rather
than a real coverage gap. As a rule of thumb:

- mikebom's `--image` output ≈ NTIA "deployed" SBOM. CDX phase
  `operations` dominates. Tighter than tools that walk a build
  cache (e.g. trivy's `~/.m2/`) but more accurate for "what's
  actually running in this image."
- mikebom's `--path` output ≈ NTIA "build" SBOM. CDX phases
  `pre-build` (lockfile entries) and `build` (eBPF-traced
  events, when applicable) dominate. Closer to a manifest
  view; useful for license compliance and full transitive
  coverage.

For the deeper rationale on why mikebom takes this stance — and
why class-presence verification deliberately prunes Maven shade-
relocation ancestors that *aren't actually in the JAR* — see
[`docs/design-notes.md`](docs/design-notes.md)'s "Scope: artifact vs
manifest SBOM" section.

## Install

Pre-built binaries are published with every release as GitHub Release
assets. Discover the latest tag and download:

```bash
TAG=$(gh release list -R kusari-sandbox/mikebom --limit 1 --json tagName --jq '.[0].tagName')
gh release download "$TAG" -R kusari-sandbox/mikebom -p "mikebom-${TAG}-*-$(uname -m)-*.tar.gz"
tar -xzf mikebom-*.tar.gz
sudo install -m 0755 mikebom /usr/local/bin/mikebom
mikebom --version
```

### Via cargo binstall (Rust toolchain users)

If you already have [`cargo binstall`](https://github.com/cargo-bins/cargo-binstall)
installed, you can skip the `gh release download` dance:

```bash
cargo binstall --git https://github.com/kusari-sandbox/mikebom mikebom
```

`mikebom-cli`'s [`[package.metadata.binstall]`](mikebom-cli/Cargo.toml)
block pins the URL template to the existing release-tarball naming so
discovery is deterministic. Once mikebom is published to crates.io
(planned for a future milestone), bare `cargo binstall mikebom` will
work without the `--git` flag.

### Or build from source

```bash
git clone https://github.com/kusari-sandbox/mikebom.git
cd mikebom
cargo build --release
# binary: ./target/release/mikebom
```

**Rust toolchain.** Scan, verify, enrich, parity-check, policy, and
attestation subcommands build under the **stable** toolchain (CI runs
`cargo +stable`). Trace subcommands additionally need nightly for the
eBPF target — see
[`docs/user-guide/installation.md`](docs/user-guide/installation.md).

**Platform support.**

| Platform          | `sbom *` / `policy` / `attestation` | `trace capture`/`run`       |
|-------------------|-------------------------------------|-----------------------------|
| Linux x86_64      | ✅ supported                         | ✅ kernel ≥ 5.8, CAP_BPF    |
| Linux aarch64     | ✅ supported                         | ✅ kernel ≥ 5.8, CAP_BPF    |
| macOS (Apple/Intel)| ✅ supported                        | ❌ use Lima/Docker (below)  |
| Windows x86_64    | ✅ supported (milestone 100)         | ❌                          |

On macOS, run tracing inside the `mikebom-dev` container
([`Dockerfile.dev`](Dockerfile.dev)) or a Lima VM
([`lima.yaml`](lima.yaml)). Everything else runs natively.

### Windows install

Download `mikebom-v<version>-x86_64-pc-windows-msvc.zip` from the
[latest release](https://github.com/kusari-sandbox/mikebom/releases),
extract `mikebom.exe`, and place it on your `PATH`.

```powershell
mikebom.exe sbom scan --path C:\Users\dev\my-project --output out.cdx.json
```

The Windows build covers the cross-platform readers (cargo, npm, pip,
gem, maven, golang) and the cross-format binary scanner (ELF, Mach-O,
PE). Linux-specific OS-package readers (dpkg, rpm, apk), Docker-daemon
scanning, and eBPF tracing are unavailable on Windows hosts; they no-op
silently. Path strings in emitted SBOMs are forward-slash-normalized
regardless of host OS so the same input produces the same SBOM bytes
on Linux, macOS, and Windows.

## Supported ecosystems

Nine production ecosystem readers plus a generic binary scanner.
[`docs/ecosystems.md`](docs/ecosystems.md) holds the full matrix;
summary below.

| Ecosystem         | OS package DB                       | Lockfile / manifest                                                         | Dep-graph                          |
|-------------------|-------------------------------------|-----------------------------------------------------------------------------|------------------------------------|
| **deb**           | `/var/lib/dpkg/status`              | —                                                                           | Full (via `Depends:`)              |
| **apk**           | `/lib/apk/db/installed`             | —                                                                           | Direct only (apk encodes no transitive) |
| **rpm**           | `/var/lib/rpm/rpmdb.sqlite` + `.rpm`| —                                                                           | Full (via `REQUIRES`). BDB opt-in via `--include-legacy-rpmdb`. |
| **cargo**         | —                                   | `Cargo.lock` v3/v4                                                          | Full                                |
| **gem**           | —                                   | `Gemfile.lock` (indent-6 edges), `specifications/*.gemspec`                 | Full                                |
| **golang (src)**  | —                                   | `go.mod` + `go.sum` + `$GOMODCACHE/`                                        | Full when cache warm                |
| **golang (bin)**  | —                                   | `runtime/debug.BuildInfo` (Go 1.18+ ELF/Mach-O/PE)                          | Modules only (BuildInfo has no edges) |
| **maven**         | Fedora sidecar POMs                 | `pom.xml`, embedded `META-INF/maven/`, `~/.m2/`, deps.dev fallback          | Full, 5-layer resolver              |
| **npm**           | —                                   | `package-lock.json` v2/v3, `pnpm-lock.yaml`, `node_modules/`                | Full. v1 locks refused.             |
| **pip**           | venv `dist-info/METADATA`           | `poetry.lock`, `Pipfile.lock`, `requirements.txt`                           | Flat venv; tree in locks            |
| *generic binary*  | —                                   | ELF / Mach-O / PE headers (`DT_NEEDED`, `LC_LOAD_DYLIB`, PE IMPORT)         | Linkage only                        |

Maven fat-jars built with the shade-plugin are emitted as nested
`components[].components[]` with a `mikebom:shade-relocation = true`
property, gated on bytecode-presence verification so
declared-but-not-relocated ancestors do not inflate the SBOM. See
[`specs/009-maven-shade-deps/spec.md`](specs/009-maven-shade-deps/spec.md).

**Go: build the binary for richer per-component classification.**
A source-only scan (`mikebom sbom scan --path .` on a Go project
before `go build`) emits the full `go.sum` closure — every module
the resolver ever fetched, including build-tag alternatives the
linker DCE'd and test scaffolding never linked. With the binary
present, mikebom keeps the same components but annotates each one
the linker didn't embed with `mikebom:not-linked = true`, so
consumers get both the broad lockfile view AND a precise
"what shipped" filter on a single SBOM. On `apigatewayv2/config`
(typical service): 65 modules with binary, 24 of them carrying
`mikebom:not-linked`; consumers wanting the binary-tight view
filter on the property and see ~41:

```bash
go build .                                    # produces ./apigatewayv2-config
mikebom sbom scan --path . --output app.cdx.json
# → 65 components, 24 carrying mikebom:not-linked = true
jq '[.components[] | select(.properties[]? | select(.name=="mikebom:not-linked") | not)]' app.cdx.json
# → strict "what shipped" view (~41 components, no annotation noise)
```

When no binary is found, mikebom emits a one-line `tracing::info`
hint pointing you at this workflow — no `mikebom:not-linked` data
is computed in that case.

## Quickstart

Produce a CycloneDX 1.6 SBOM from any source tree:

```bash
./target/release/mikebom sbom scan \
  --path ./my-project \
  --output project.cdx.json

jq '.components | length, .dependencies | length' project.cdx.json
```

## Stable recipes

**1. Scan a source tree.** Any host, no privileges. Lockfile-driven
dep graph. `--path` defaults to *manifest scope*
(`--include-declared-deps` is auto-on).

```bash
mikebom sbom scan --path ./my-project --output project.cdx.json
```

**2. Scan a container image.** Defaults to *artifact scope*
(on-disk presence required). Pass `--include-declared-deps` for the
manifest view. `--image` accepts either an OCI reference (`alpine:3.19`,
`gcr.io/distroless/static-debian12:latest`, or any other registry path)
or a `docker save` tarball on disk; mikebom auto-detects which.

For OCI references, mikebom checks the **local docker daemon's cache
first** and falls back to a registry pull on miss — matching `docker
run` semantics and the trivy / syft default. So if you've already
done `docker pull alpine:3.19` (or are scanning an image you just
built locally), no network round-trip happens.

```bash
# OCI ref — local docker first, registry fallback.
mikebom sbom scan --image alpine:3.19 --output alpine.cdx.json

# Force a fresh registry fetch (skip the local docker cache):
mikebom sbom scan --image alpine:3.19 --image-src remote \
    --output alpine.cdx.json

# Pre-extracted tarball still works.
docker save alpine:3.19 -o alpine.tar
mikebom sbom scan --image alpine.tar --output alpine.cdx.json
```

Authenticated registries are supported via `~/.docker/config.json`
(both `Bearer`-style — Docker Hub, GHCR, gcr.io — and `Basic`-style
— AWS ECR — challenges). Run `docker login <registry>` (or
`aws ecr get-login-password | docker login --username AWS …` for ECR)
once and mikebom picks up the credentials.

**3. Scan a package cache.** Treats cached bytes as present-on-disk;
useful for CI cache audits.

```bash
mikebom sbom scan --path ~/.cargo/registry/cache --output cargo.cdx.json
```

**4. Scan a Maven fat-jar and see shaded ancestors.** With feature
009 the SBOM emits one nested component per shade-relocated ancestor
whose bytecode is actually in the JAR.

```bash
mikebom sbom scan --path ./target/ --output app.cdx.json

jq '
  .components[]
  | select(.purl | test("pkg:maven/"))
  | .components // []
  | map(select(.properties // [] | any(.name == "mikebom:shade-relocation" and .value == "true")))
  | map({purl, bom_ref: ."bom-ref"})
' app.cdx.json
```

**5. Verify a signed DSSE attestation.**

```bash
mikebom sbom verify some.dsse.json --public-key signer.pub
# → PASS — verified with public_key sha256:…
```

**6. Generate a starter in-toto layout bound to a functionary key.**

```bash
mikebom policy init --functionary-key signer.pub --output layout.json
mikebom sbom verify some.dsse.json --layout layout.json
```

**7. Enrich an SBOM with an RFC 6902 JSON Patch.** Each patch is
recorded as a `mikebom:enrichment-patch[N]` property on the BOM
metadata so the provenance of every change survives.

```bash
mikebom sbom enrich project.cdx.json \
  --patch add-supplier.json --author you@example.com
```

**Common flags** across every `sbom *` subcommand: `--offline`,
`--exclude-scope`, `--include-declared-deps`,
`--include-legacy-rpmdb` (env: `MIKEBOM_INCLUDE_LEGACY_RPMDB=1`).
See [`docs/user-guide/cli-reference.md`](docs/user-guide/cli-reference.md)
for the full per-flag reference and `mikebom sbom <verb> --help` for the
canonical source.

## Cross-tier correlation

When the same software produces multiple SBOMs across its lifecycle —
a source SBOM scanned from the repo, a build SBOM captured during
compilation, and an image SBOM scanned from the resulting container —
external consumers need a way to tell *which goes with which*. mikebom
ships two complementary mechanisms.

### Stable identifiers (auto-detected)

mikebom attaches scheme-prefixed identifiers to every SBOM it emits.
The five built-in schemes (`repo:`, `git:`, `image:`, `attestation:`,
`subject:`) are auto-detected when possible; any operator-defined
scheme like `acme_corp_id:` rides through unchanged.

**Source-tier and build-tier auto-detect from a git checkout.**
No flags required. `repo:` comes from `git remote get-url` (origin
→ upstream → first-listed); the build-tier scan additionally captures
a commit-anchored `git:` from `git rev-parse HEAD`.

```bash
cd ~/projects/my-rust-app  # any git checkout

# Source-tier — auto-detects `repo:`
mikebom sbom scan --path . --output source.cdx.json

# Build-tier — auto-detects `repo:` and `git:<repo-url>#<sha>`
mikebom trace run -- ./build.sh
```

**Image-tier auto-detects `image:`** from the resolved registry
reference + digest:

```bash
mikebom sbom scan --image my-app:v1 --output image.cdx.json
```

**External consumers correlate by reading identifier fields directly**
from `metadata.component.externalReferences[]` (CDX) /
`Package.externalRefs[]` (SPDX 2.3) / `Element.externalIdentifier[]`
(SPDX 3):

```bash
jq '.metadata.component.externalReferences[]
    | select(.type == "vcs" or .type == "distribution")
    | {url, comment}' source.cdx.json
# {
#   "url": "git@github.com:acme/my-rust-app.git",
#   "comment": "auto-detected from git remote `origin`"
# }
```

Manual overrides (`--repo`, `--git-ref`, `--image-id`,
`--attestation`, `--id <scheme>=<value>`) win over auto-detect on a
per-scheme basis.

**Credentials in git remote URLs are stripped by default.** If your
`origin` is configured with embedded credentials
(`https://USER:TOKEN@github.com/...`, common in CI runners using
GitHub App tokens or deploy tokens), the userinfo is stripped before
the URL is embedded in the SBOM — no token leakage in published
artifacts. Pass `--keep-credentials-in-identifiers` to opt out for
non-sensitive credentials. Manual flag values are emitted verbatim;
the operator typed them, the tool respects that.

See [`docs/reference/identifiers.md`](docs/reference/identifiers.md)
for the full wire format and per-format extraction recipes.

### Cross-tier handshake via `subject:`

The build-tier `subject:` identifier completes the content-addressable
correlation chain. When `mikebom trace run` produces an output binary
with hash `X`, the build SBOM body carries `subject:sha256:X` as a
first-class identifier. When the resulting image is scanned with
`mikebom sbom scan --image foo:v1`, the image SBOM carries
`image:foo:v1@sha256:X` — the `X` portion matches by string.

```bash
# Build (auto-detects subject:sha256:X from in-toto attestation subjects)
mikebom trace run --signing-key ./key \
    --sbom-output build.cdx.json \
    -- docker build -t my-app:v1 .

# Image (auto-detects image:my-app:v1@sha256:X)
mikebom sbom scan --image my-app:v1 --output image.cdx.json

# External tool walks: image's image: digest matches build's subject: hex →
# build SBOM is found by string match, no mikebom resolver needed.
```

External SBOM-store consumers walk `image-SBOM.components[].hashes[].sha256
== X → SBOM whose subject:sha256:X matches → that build SBOM's git: commit
→ matching source SBOM` purely by string match — no mikebom-side resolver.

### Per-component user-defined identifiers

Attach internal asset IDs (or any operator-scoped identifier) to
specific components via `--component-id <PURL>=<scheme>:<value>`.
Identifiers ride standards-native per-component carriers (CDX
`components[].properties[]`, SPDX 2.3 `Package.externalRefs[PERSISTENT-ID]`,
SPDX 3 `Element.externalIdentifier[]`) — any SBOM-aware tool reads
them as ordinary first-class data.

```bash
mikebom sbom scan --path . \
    --component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-shared-lib-v2" \
    --component-id "pkg:cargo/myapp@0.5.1=acme-asset:myapp-prod-001"
```

Built-in scheme names (`repo`, `git`, `image`, `attestation`, `subject`)
are reserved for the document level and rejected at parse time on
`--component-id`. The flag is repeatable; selectors that match zero
components log a warning and the scan continues.

### Cross-tier SBOM binding (`--bind-to-source`)

A binding is a content-hashed reference embedded in one SBOM that
points to another. Use it when you want a verifier to be able to
re-derive that "this image SBOM was built from *this exact* source
SBOM file" without trusting filename heuristics.

```bash
# Bind an image SBOM to the source SBOM it was built from.
mikebom sbom scan --image my-app:v1 \
    --bind-to-source ./source.cdx.json \
    --output image.cdx.json

# Verify a binding from anywhere — re-hashes the source SBOM and
# checks against the embedded reference.
mikebom sbom verify-binding \
    --image-sbom image.cdx.json \
    --source-sbom ./source.cdx.json
# → PASS — source-document binding verified

# Trace the chain across an arbitrary set of SBOMs without manual
# lookups (matches by content hash + identifier overlap):
mikebom sbom trace-binding \
    --component-purl "pkg:cargo/serde@1.0.0" \
    --image-sbom image.cdx.json \
    --candidate-sources-dir ./sboms/
```

The binding annotation lives in standards-native carriers
(`mikebom:source-document-binding`) so any SPDX/CDX-aware tool can
extract it. See
[`docs/reference/cross-tier-binding.md`](docs/reference/cross-tier-binding.md)
for the full schema and verifier protocol.

## Experimental: build-time trace (Linux only)

> **Status:** experimental. Requires CAP_BPF + CAP_PERFMON. Adds ~2–3×
> wall-clock overhead on syscall-heavy builds. Coverage varies by
> syscall path (gaps on `openat2` / `io_uring`). For most SBOM use
> cases, prefer the scan recipes above — they produce richer output
> with no privilege requirements. Trace-mode exists for workflows
> where the SBOM needs to be provably bound to a specific build event
> (attestation-first provenance).

```bash
# Trace a cargo build and produce an SBOM + signed attestation in one pass.
mikebom trace run \
  --signing-key ./signing.key \
  --sbom-output ripgrep.cdx.json \
  --attestation-output ripgrep.attestation.dsse.json \
  -- cargo install ripgrep

# Then verify from anywhere (works on macOS; verify is pure-scan):
mikebom sbom verify ripgrep.attestation.dsse.json --public-key ./signing.pub
```

See [`docs/architecture/signing.md`](docs/architecture/signing.md),
[`docs/architecture/attestations.md`](docs/architecture/attestations.md),
and
[`specs/006-sbomit-suite/quickstart.md`](specs/006-sbomit-suite/quickstart.md)
for keyless (Fulcio/Rekor) flows, policy layouts, and the
witness-v0.1 attestation format (compatible with `sbomit generate`).

## Documentation

- **[User guide](docs/user-guide/)** — installation, quickstart, CLI
  reference, configuration.
- **[Architecture](docs/architecture/)** — four-stage pipeline
  (scan → resolve → enrich → generate), PURL & CPE emission rules,
  license resolution, in-toto attestation schema.
- **[Ecosystems](docs/ecosystems.md)** — per-ecosystem coverage
  matrix (authoritative).
- **[Identifiers reference](docs/reference/identifiers.md)** — the
  four built-in schemes, auto-detection rules, per-format wire
  carriers, decode recipes for external consumers.
- **[SBOM types reference](docs/reference/sbom-types.md)** — CISA
  Design / Source / Build / Analyzed / Deployed / Runtime
  classification, per-format `jq` recipes, and the
  `--sbom-type <type>` operator-assert flag.
- **[Cross-tier binding reference](docs/reference/cross-tier-binding.md)**
  — `--bind-to-source` schema, verifier protocol, multi-tier
  trace flows.
- **[SBOM format mapping](docs/reference/sbom-format-mapping.md)** —
  per-feature carrier matrix across CDX 1.6, SPDX 2.3, and SPDX 3.
- **[Conformance harness guide](docs/reference/conformance-harness-guide.md)**
  — for external implementers writing cross-format conformance suites.
- **[Design notes](docs/design-notes.md)** — living architectural
  decisions at the cross-cutting level.
- **[Changelog](CHANGELOG.md)** — what shipped in which release.
- **[Specs](specs/)** — per-milestone planning specs
  (001 build-trace pipeline → 076 subject + per-component identifiers).

## Workspace layout

```text
mikebom-cli/      User-space CLI: scan, resolve, enrich, generate, verify, trace
mikebom-common/   Shared types: PURL, attestation schema, resolution types
mikebom-ebpf/     Kernel-side eBPF probes (uprobe on libssl, kprobe on file ops)
xtask/            Workspace build/dev tooling
docs/             User guide, architecture, ecosystems, design notes
specs/            Per-milestone planning specs
tests/fixtures/   Real + synthetic fixtures consumed by integration tests
```

## Reporting issues and contributing

Open an issue or PR at
[github.com/kusari-sandbox/mikebom](https://github.com/kusari-sandbox/mikebom).
CI enforces `cargo +stable clippy --workspace --all-targets` and
`cargo +stable test --workspace` on every PR; run both locally before
opening one.

## License

Apache-2.0. See the workspace [`Cargo.toml`](Cargo.toml) for the
declared `license` field.
