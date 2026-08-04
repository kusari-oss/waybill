# Waybill

A toolkit for working with software bills of materials (SBOMs) end-to-end:

- **Generates SBOMs** from source trees, package caches, and container
  images with lockfile-aware dep-graph extraction, emitting **CycloneDX
  1.6**, **SPDX 2.3**, and **SPDX 3.0.1** with SHA-256 hashes + evidence
  + real dependency relationships. On Linux, optionally captures build-
  time provenance via eBPF.
- **Analyzes SBOMs** — verifies DSSE-signed attestations against keys /
  Fulcio identities / in-toto layouts, and cross-checks already-emitted
  CycloneDX / SPDX 2.3 / SPDX 3.0.1 outputs for per-datum × per-format
  coverage parity via `waybill sbom parity-check`.
- **Modifies and enriches SBOMs** — today, `waybill sbom enrich`
  applies RFC 6902 JSON Patches with provenance metadata recorded as
  `waybill:enrichment-patch[N]` properties. Richer modification
  workflows (license backfill, supplier resolution, VEX merging) are
  on the roadmap.


> **Waybill was previously known as Mikebom.** The project was renamed in v0.1.0-alpha.66. Historical release tags (`v0.1.0-alpha.7`..`v0.1.0-alpha.65`) and pre-rename SBOMs using `mikebom:*` annotations remain accessible; see [docs/migration/mikebom-to-waybill.md](docs/migration/mikebom-to-waybill.md) for the drop-in migration recipe.

> **Status: pre-1.0 alpha.** The CLI surface, output formats, and
> per-ecosystem coverage are still being stabilized — expect breaking
> changes between alpha releases, and expect additional ecosystem
> readers + binary-analysis surface to keep landing
> release-over-release. Pre-built binaries are published as GitHub
> Release assets; no crates.io release yet.
>
> - **Stable** — `waybill sbom scan`, `waybill sbom verify`,
>   `waybill sbom enrich`, `waybill sbom parity-check`,
>   `waybill sbom verify-binding`, `waybill sbom trace-binding`,
>   `waybill policy init`, and `waybill attestation validate`.
>   Cross-platform, no special privileges.
> - **Experimental, Linux-only** — `waybill trace capture` /
>   `waybill trace run`. eBPF-based build-time capture that produces
>   attestations bound to the actual build event. Requires CAP_BPF +
>   CAP_PERFMON and adds ~2–3× wall-clock overhead on syscall-heavy
>   builds.
>
> See [`docs/user-guide/cli-reference.md`](docs/user-guide/cli-reference.md)
> for the per-flag operator reference and
> [`CHANGELOG.md`](CHANGELOG.md) for what shipped when.

## Table of contents

- [Install](#install)
- [Why Waybill?](#why-waybill)
- [What kind of SBOM does Waybill emit?](#what-kind-of-sbom-does-waybill-emit)
- [Standards & compliance](#standards--compliance)
- [Supported ecosystems](#supported-ecosystems)
- [Usage](#usage)
- [Cross-tier correlation](#cross-tier-correlation)
- [Experimental: build-time trace (Linux only)](#experimental-build-time-trace-linux-only)
- [Documentation](#documentation)
- [Workspace layout](#workspace-layout)
- [Reporting issues and contributing](#reporting-issues-and-contributing)
- [License](#license)

## Install

Pre-built binaries are published with every release as GitHub Release
assets. Discover the latest tag and download:

```bash
TAG=$(gh release list -R kusari-sandbox/waybill --limit 1 --json tagName --jq '.[0].tagName')
gh release download "$TAG" -R kusari-sandbox/waybill -p "waybill-${TAG}-*-$(uname -m)-*.tar.gz"
tar -xzf waybill-*.tar.gz
sudo install -m 0755 waybill /usr/local/bin/waybill
waybill --version
```

### Via cargo binstall (Rust toolchain users)

If you already have [`cargo binstall`](https://github.com/cargo-bins/cargo-binstall)
installed, you can skip the `gh release download` dance:

```bash
cargo binstall --git https://github.com/kusari-sandbox/waybill waybill
```

`waybill-cli`'s [`[package.metadata.binstall]`](waybill-cli/Cargo.toml)
block pins the URL template to the existing release-tarball naming so
discovery is deterministic. Once Waybill is published to crates.io
(planned for a future milestone), bare `cargo binstall waybill` will
work without the `--git` flag.

### Or build from source

```bash
git clone https://github.com/kusari-sandbox/waybill.git
cd waybill
cargo build --release
# binary: ./target/release/waybill
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
| Windows x86_64    | 🧪 experimental (milestone 100, [#210](https://github.com/kusari-sandbox/waybill/issues/210)) | ❌ |

On macOS, run tracing inside the `waybill-dev` container
([`Dockerfile.dev`](Dockerfile.dev)) or a Lima VM
([`lima.yaml`](lima.yaml)). Everything else runs natively.

### Windows install

🧪 **Experimental** (milestone 100; runtime parity tracked in
[#210](https://github.com/kusari-sandbox/waybill/issues/210)).
Download `waybill-v<version>-x86_64-pc-windows-msvc.zip` from the
[latest release](https://github.com/kusari-sandbox/waybill/releases)
and place `waybill.exe` on your `PATH`. Known gaps and coverage
details:
[`docs/user-guide/installation.md`](docs/user-guide/installation.md#windows-install-experimental).

### First scan

```bash
waybill sbom scan --path ./my-project --output project.cdx.json

jq '.components | length, .dependencies | length' project.cdx.json
```


## Why Waybill?

Many SBOM tools emit a flat component list with heuristic
identifiers — good enough to eyeball, hard to build automation on.
Waybill aims for SBOMs that are precise, verifiable, and
self-describing. Scan-mode reads lockfiles + package manifests +
per-module metadata to build a proper CycloneDX with:

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
  ELF build-id, Mach-O UUID + codesign metadata, and PE pdb-id on
  every scanned binary, emitted symmetrically across all three
  output formats. Plus **Go VCS provenance** (commit / timestamp /
  dirty flag from BuildInfo), **Rust crate-closure provenance**
  (from `cargo auditable` builds), and **curated
  embedded-version-string detection** for 11 high-CVE-volume native
  libraries (OpenSSL, zlib, SQLite, curl, …) statically linked into
  binaries. Full reference:
  [`docs/reference/binary-identity.md`](docs/reference/binary-identity.md).

On top of scan-mode, Waybill adds:

- **Signed DSSE envelope attestations** via sigstore (local-key or
  keyless OIDC → Fulcio → Rekor).
- **In-toto layout verification** for build-policy enforcement.
- **Witness-collection v0.1** output compatible with `sbomit generate`
  and any go-witness-aware verifier.

## What kind of SBOM does Waybill emit?

Comparing Waybill's component count to trivy's or syft's? The gap is
usually a **scope choice, not a bug** — and Waybill self-describes its
scope on every output. Two orthogonal axes:

- **Document-level scope mode:** *artifact* (on-disk components only;
  default for `--image`) vs *manifest* (plus declared-but-not-on-disk
  transitives; default for `--path`). Controlled by
  `--include-declared-deps`.
- **Per-component lifecycle tier:** `waybill:sbom-tier` on every
  component — `design` / `source` / `build` / `deployed` / `analyzed`.

Rule of thumb: `--image` output ≈ NTIA "deployed" SBOM; `--path`
output ≈ NTIA "build" SBOM. Full details — the scope tables, how each
output format carries the scope natively, and the terminology bridge
to other scanners — in
[`docs/reference/sbom-scopes.md`](docs/reference/sbom-scopes.md); the
CISA SBOM-type classification and `--sbom-type` flag in
[`docs/reference/sbom-types.md`](docs/reference/sbom-types.md).

## Standards & compliance

Waybill tracks the **CISA 2026 Minimum Elements for a Software Bill
of Materials** (TLP:CLEAR, published 2026-07-29) as the primary
compliance baseline. Every one of the 17 data-field elements + 6
practices/processes has a per-emitter verdict at
[`docs/cisa-2026-coverage.md`](docs/cisa-2026-coverage.md) —
evidence-cited, backed by a machine-verifying integration test that
runs against every CI build, so regressions surface immediately.

**Row 2 (SBOM Author Signature)** ships as opt-in: `--sign-key <PATH>`
produces JSF static-key signatures on CDX (in-slot) and DSSE sidecars
on SPDX; `--sign` produces Sigstore keyless signatures via OIDC →
Fulcio → Rekor with the resulting Sigstore Bundle embedded in CDX
`metadata.signature` or written to `.sig.bundle.json` sidecar for
SPDX. **v1 scope**: `--sign` requires a token from an email-emitting
OIDC provider (e.g., `cosign login`, Sigstore-dex, Google, GitLab)
exported as `SIGSTORE_ID_TOKEN`. GitHub Actions ambient tokens are
not directly supported in v1; GHA users fetch a compatible token via
a helper action. Full GHA-ambient support deferred to a follow-up
milestone. Details at
[`docs/sigstore-trust-keys.md`](docs/sigstore-trust-keys.md) +
[`specs/222-sigstore-keyless-signing/quickstart.md`](specs/222-sigstore-keyless-signing/quickstart.md).

Referenced downstream by EU CRA (Regulation (EU) 2024/2847) Article
3, BSI TR-03183-2, CERT-In's Technical Guidelines on Bill of
Materials, and the equivalent guidance from every CISA 2026
co-authoring national cyber agency.

## Supported ecosystems

Fifteen production ecosystem readers plus a generic binary scanner.
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
| **pants (Python)** *(223)* | —                          | Pex lockfile at `3rdparty/python/*.lock` (default) or `pants.toml`-declared path; multi-resolve scope-tagging via name allowlist (mypy, pytest, black, …) | Full via `requires_dists` (PEP 508) |
| **pants (JVM)** *(224)* | —                             | Coursier lockfile at `3rdparty/jvm/*.lock` (default) or `pants.toml` `[jvm.resolves]`-declared path; multi-resolve scope-tagging via JVM dev-tool allowlist (junit, scalatest, ktlint, …); Pants-header discriminator vs standalone coursier | Full via `dependencies[]` coord-string edges |
| **pants (shell)** *(225)* | —                           | `BUILD` files declaring `shell_source` / `shell_sources` / `shunit2_test` / `shunit2_tests` targets; `pants.toml` `[shellcheck]` / `[shfmt]` / `[shunit2]` version pins → design-tier tool components | — (shell scripts have no dep graph) |
| **pants (Go)** *(226)* | —                              | `BUILD` files declaring `go_binary` / `go_package` / `go_third_party_package` / `go_mod` targets → post-scan enrichment of existing `pkg:golang/*` components with `waybill:pants-target` annotation; `pants.toml` `[golang] expected_version` → design-tier `pkg:generic/go@<version>` | Full via the existing Go reader (m053+m055+m160+m161); enrichment adds Pants attribution only |
| **vcpkg** *(102)* | —                                   | `vcpkg.json` (manifest mode, `dependencies[]` + `overrides[]`)              | Direct only                          |
| **conan** *(102)* | —                                   | `conanfile.txt` (`[requires]`/`[tool_requires]`) + `conanfile.py` (literal `requires=[...]`) | Direct only                          |
| *generic binary*  | —                                   | ELF / Mach-O / PE headers (`DT_NEEDED`, `LC_LOAD_DYLIB`, PE IMPORT)         | Linkage only                        |

C/C++ build-system manifests (Bazel `MODULE.bazel` / `WORKSPACE.bazel`,
CMake `FetchContent_Declare` / `ExternalProject_Add`) ship in
milestone 102's second PR — alpha-status spec at
[`specs/102-cpp-bazel-cmake-readers/`](specs/102-cpp-bazel-cmake-readers/).

Maven fat-jars built with the shade-plugin are emitted as nested
`components[].components[]` with a `waybill:shade-relocation = true`
property, gated on bytecode-presence verification so
declared-but-not-relocated ancestors do not inflate the SBOM. See
[`specs/009-maven-shade-deps/spec.md`](specs/009-maven-shade-deps/spec.md).

**Go tip:** run `go build` before scanning — with the binary present,
modules the linker didn't embed get `waybill:not-linked = true`,
giving both the broad lockfile view and a strict "what shipped"
filter in one SBOM. Workflow details in
[`docs/ecosystems.md`](docs/ecosystems.md#golang).

## Usage

```bash
# Scan a source tree (any host, no privileges; manifest scope by default)
waybill sbom scan --path ./my-project --output project.cdx.json

# Scan a container image (artifact scope; local docker cache first, registry fallback)
waybill sbom scan --image alpine:3.19 --output alpine.cdx.json

# Verify a signed DSSE attestation
waybill sbom verify some.dsse.json --public-key signer.pub
```

Many more recipes — authenticated registries, the OCI Referrers API
(`--sbom-source`), package-cache scans, Maven fat-jar shade analysis,
`sbom enrich` JSON patches, in-toto layouts, Kubernetes workload
tagging — in
[`docs/user-guide/quickstart.md`](docs/user-guide/quickstart.md).
Per-flag reference:
[`docs/user-guide/cli-reference.md`](docs/user-guide/cli-reference.md).

## Cross-tier correlation

When the same software produces multiple SBOMs across its lifecycle —
source, build, and image — Waybill ships the identity plumbing to tie
them together:

- **Stable identifiers**, auto-detected (`repo:` from the git remote,
  `git:<url>#<sha>` at build time, `image:<ref>@<digest>` on image
  scans) and rideable by any operator-defined scheme. Credentials in
  remote URLs are stripped by default. See
  [`docs/reference/identifiers.md`](docs/reference/identifiers.md).
- **Content-addressed handshake** — the build SBOM carries
  `subject:sha256:X` for its output binary; the image SBOM's digest
  matches by plain string. No Waybill-side resolver needed.
- **Explicit binding** — `--bind-to-source` embeds a content-hashed
  reference to the source SBOM; `sbom verify-binding` /
  `sbom trace-binding` re-derive and walk the chain. See
  [`docs/reference/cross-tier-binding.md`](docs/reference/cross-tier-binding.md).
- **Per-component IDs** — attach internal asset identifiers with
  `--component-id <PURL>=<scheme>:<value>`.

## Experimental: build-time trace (Linux only)

eBPF-based capture that binds an SBOM + signed attestation to a
specific build event. Requires CAP_BPF + CAP_PERFMON; ~2-3x wall-clock
overhead. For most SBOM use cases prefer the scan pipeline above.

```bash
waybill trace run \
  --signing-key ./signing.key \
  --sbom-output ripgrep.cdx.json \
  --attestation-output ripgrep.attestation.dsse.json \
  -- cargo install ripgrep

# Verify from anywhere (verify is pure-scan, works on macOS):
waybill sbom verify ripgrep.attestation.dsse.json --public-key ./signing.pub
```

Keyless (Fulcio/Rekor) flows, policy layouts, and the witness-v0.1
format:
[`docs/architecture/signing.md`](docs/architecture/signing.md),
[`docs/architecture/attestations.md`](docs/architecture/attestations.md).

## Documentation

- **[User guide](docs/user-guide/)** — installation, quickstart, CLI
  reference, configuration.
- **[Architecture](docs/architecture/)** — four-stage pipeline
  (scan → resolve → enrich → generate), PURL & CPE emission rules,
  license resolution, in-toto attestation schema.
- **[Ecosystems](docs/ecosystems.md)** — per-ecosystem coverage
  matrix (authoritative).
- **[SBOM scopes reference](docs/reference/sbom-scopes.md)** —
  artifact vs manifest scope, lifecycle tiers, terminology bridge
  to other scanners.
- **[Binary identity reference](docs/reference/binary-identity.md)** —
  ELF/Mach-O/PE identity annotations, Go VCS provenance,
  cargo-auditable closures, embedded-version-string detection.
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
- **[Project discovery reference](docs/reference/project-discovery.md)**
  — `--project-discovery=<mode>`: the three modes
  (`all` / `root-only` / `strict`), the per-ecosystem
  workspace-member detection matrix, and the interaction matrix
  vs `--split`.
- **[Conformance harness guide](docs/reference/conformance-harness-guide.md)**
  — for external implementers writing cross-format conformance suites.
- **[Design notes](docs/design-notes.md)** — living architectural
  decisions at the cross-cutting level.
- **[Changelog](CHANGELOG.md)** — what shipped in which release.
- **[Specs](specs/)** — per-milestone planning specs
  (001 build-trace pipeline → 076 subject + per-component identifiers).

## Workspace layout

```text
waybill-cli/      User-space CLI: scan, resolve, enrich, generate, verify, trace
waybill-common/   Shared types: PURL, attestation schema, resolution types
waybill-ebpf/     Kernel-side eBPF probes (uprobe on libssl, kprobe on file ops)
xtask/            Workspace build/dev tooling
docs/             User guide, architecture, ecosystems, design notes
specs/            Per-milestone planning specs
tests/fixtures/   Real + synthetic fixtures consumed by integration tests
```

## Reporting issues and contributing

Open an issue or PR at
[github.com/kusari-sandbox/waybill](https://github.com/kusari-sandbox/waybill).
CI enforces `cargo +stable clippy --workspace --all-targets` and
`cargo +stable test --workspace` on every PR; run both locally before
opening one.

## License

Apache-2.0. See the workspace [`Cargo.toml`](Cargo.toml) for the
declared `license` field.
