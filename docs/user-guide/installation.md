# Installation

Waybill has two modes with different runtime requirements.

| Mode | Subcommands | Needs |
|---|---|---|
| **Scanning** | `waybill sbom scan`, `waybill sbom verify`, `waybill sbom enrich`, `waybill sbom verify-binding`, `waybill sbom trace-binding`, `waybill policy init` | Any OS Rust runs on. No privilege. No eBPF. |
| **Tracing** | `waybill trace capture`, `waybill trace run` | Linux kernel ≥ 5.8, eBPF privilege (`--privileged` container, root, or CAP_BPF + CAP_PERFMON) |

If you only need the scanning surface, Waybill runs natively on macOS,
Linux, or Windows (the Windows binary is 🧪 [experimental](https://github.com/kusari-sandbox/waybill/issues/210); WSL2 also works for both scanning and tracing). `trace` requires Linux with eBPF.

## Pre-built binaries (recommended)

Every Waybill release ships per-platform tarballs as GitHub Release assets.

Discover the latest release:

```bash
gh release list -R kusari-sandbox/waybill --limit 1
```

Download the asset matching your platform:

```bash
TAG=$(gh release list -R kusari-sandbox/waybill --limit 1 --json tagName --jq '.[0].tagName')
gh release download "$TAG" -R kusari-sandbox/waybill -p "waybill-${TAG}-*-$(uname -m)-*.tar.gz"
tar -xzf waybill-*.tar.gz
sudo install -m 0755 waybill /usr/local/bin/waybill
waybill --version
```

Or browse releases manually at
<https://github.com/kusari-sandbox/waybill/releases> and pick the asset
that matches your platform.

## Windows install (experimental)

> 🧪 **Experimental.** Windows builds are available as of milestone
> 100, but are not feature-equivalent to Linux/macOS yet. Known gaps:
> Linux-only OS package readers (dpkg/rpm/apk), HOME-env-var-derived
> cache paths, OCI image cache atomic-rename, path-resolver pattern
> matcher, and Python stdlib collapse. Full Windows runtime test
> parity + production-code fixes are tracked in
> [#210](https://github.com/kusari-sandbox/waybill/issues/210).
> Do not rely on the Windows binary for production SBOM workflows
> until #210 closes.

Download `waybill-v<version>-x86_64-pc-windows-msvc.zip` from the
[latest release](https://github.com/kusari-sandbox/waybill/releases),
extract `waybill.exe`, and place it on your `PATH`.

```powershell
waybill.exe sbom scan --path C:\Users\dev\my-project --output out.cdx.json
```

The Windows build covers the cross-platform readers (cargo, npm, pip,
gem, maven, golang) and the cross-format binary scanner (ELF, Mach-O,
PE). Linux-specific OS-package readers (dpkg, rpm, apk), Docker-daemon
scanning, and eBPF tracing are unavailable on Windows hosts; they no-op
silently. Path strings in emitted SBOMs are forward-slash-normalized
regardless of host OS so the same input produces the same SBOM bytes
on Linux, macOS, and Windows.

## Build from source

Stable Rust, standard workspace build:

```bash
cargo build --release
```

The binary lands at `./target/release/waybill`. Add it to `$PATH` or invoke
it directly.

```bash
./target/release/waybill --help
```

For a system-wide install from source:

```bash
cargo install --path waybill-cli
```

The workspace has three crates (`waybill-cli`, `waybill-common`,
`waybill-ebpf`) plus an `xtask` crate. A single `cargo build --release`
from the repo root produces the CLI binary.

## Production container image (multi-arch)

Each `v0.1.0-alpha.*` release publishes a multi-arch distroless container image to GitHub Container Registry. The image is signed with cosign keyless via Sigstore (matches Waybill's existing attestation philosophy) and is suitable for Kubernetes Pod Security Standards "restricted" profiles — runs as non-root user `65532`, no shell, no package manager.

**Pull:**

```bash
docker pull ghcr.io/kusari-sandbox/waybill:v0.1.0-alpha.35
# or:
docker pull ghcr.io/kusari-sandbox/waybill:0.1.0-alpha.35
# or the latest alpha release:
docker pull ghcr.io/kusari-sandbox/waybill:latest
```

**Tags published per release:**

- `ghcr.io/kusari-sandbox/waybill:v0.1.0-alpha.35` — full git tag form
- `ghcr.io/kusari-sandbox/waybill:0.1.0-alpha.35` — version without `v` prefix
- `ghcr.io/kusari-sandbox/waybill:latest` — moves with every alpha release

**Platforms:** `linux/amd64`, `linux/arm64`. The binary inside is byte-identical to the corresponding release tarball — the image is built from those artifacts, not re-compiled.

**Verify the signature (optional but recommended for supply-chain scenarios):**

```bash
cosign verify \
  --certificate-identity-regexp 'https://github.com/kusari-sandbox/waybill/.+' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  ghcr.io/kusari-sandbox/waybill:v0.1.0-alpha.35
```

**Scan a directory mounted into the container** (the common case — `sbom scan` doesn't need any special privileges):

```bash
docker run --rm \
  -v "$PWD:/scan:ro" \
  ghcr.io/kusari-sandbox/waybill:latest \
  sbom scan --offline --path /scan --output /scan/sbom.cdx.json
```

**Pull from a private registry** (`waybill sbom scan --image …` with auth) — see [`--registry-credentials-dir`](cli-reference.md#--registry-credentials-dir-path) for the K8s `imagePullSecrets`-mount pattern.

**Trace mode in a container** is supported but uncommon. The image ships the eBPF object at the loader's expected relative path; you'd need to run with `CAP_BPF` + `CAP_PERFMON` (Linux only) and the appropriate host-mount setup. Most users want `sbom scan` or `sbom generate`, neither of which needs extra privileges.

## Development container (Linux eBPF, macOS, Windows)

The tracing subcommands need a privileged Linux host. On macOS, Windows,
or when you don't want to build toolchain dependencies locally, use the
provided dev container — it ships a compatible kernel and the BPF
toolchain.

```bash
docker build -t waybill-dev -f Dockerfile.dev .
docker run --rm --privileged \
  -v "$PWD:/waybill-src:ro" \
  waybill-dev \
  bash -c "cd /waybill-src && cargo build --release"
```

`--privileged` is required: eBPF probe attachment uses capabilities that
rootless Docker and unprivileged containers don't expose.

## Lima VM (macOS)

For an interactive Linux shell on macOS without Docker, the repo ships a
[`lima.yaml`](../../lima.yaml) recipe. Provision with:

```bash
limactl start --name=waybill lima.yaml
limactl shell waybill
```

Inside the VM, `cargo build --release` and `trace`/`scan` subcommands work
as on any Linux host.

## Verify the install

```bash
waybill --version
waybill --help
waybill sbom --help
waybill trace --help
```

If `waybill --help` shows the top-level `trace` / `sbom` / `attestation` /
`policy` nouns and the global flags (`--offline`, `--exclude-scope`,
`--include-legacy-rpmdb`), the install is ready. Move on to the
[quickstart](quickstart.md).

## What's next

- [Quickstart](quickstart.md) — operator onboarding recipes.
- [CLI reference](cli-reference.md) — every flag with type, default, and
  example.
- [Configuration](configuration.md) — global flags and environment variables.
