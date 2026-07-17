# Quickstart: Podman Source Implementation

**Date**: 2026-07-17
**Audience**: mikebom maintainer implementing or reviewing m206.

## Prerequisites

- Working mikebom checkout on branch `206-podman-source`.
- `cargo +stable` toolchain (workspace default).
- **Linux** for Reproducers 1-3 (podman-source is Linux-only per spec Assumption 1). Reproducer 4 (byte-identity guard) works on any platform.
- Reproducer 1-2 require `podman` on PATH + at least one image cached.

## Reproducer 1 — US1 rootless podman scan (SC-001, SC-002)

```bash
podman pull alpine:3.19
mikebom sbom scan --image alpine:3.19 --image-src podman \
  --format cyclonedx-json --output /tmp/alpine.cdx.json --no-deep-hash

# Verify: apk components detected
jq '.components[] | select(.purl | startswith("pkg:apk/")) | .purl' /tmp/alpine.cdx.json | head

# Verify: mikebom:image-source annotation present with value "podman"
jq '.metadata.properties[] | select(.name == "mikebom:image-source")' /tmp/alpine.cdx.json
```

**Expected post-m206**:
- ≥ 10 `pkg:apk/` components (alpine base pkgs — musl, busybox, apk-tools, etc.).
- `.metadata.properties[]` contains `{name: "mikebom:image-source", value: "podman"}`.

## Reproducer 2 — US3 auto-detection default order

```bash
# Ensure docker cache empty for alpine (only podman has it).
docker rmi alpine:3.19 2>/dev/null || true
podman pull alpine:3.19  # if not already cached

# Run mikebom WITHOUT --image-src flag → uses default order.
mikebom sbom scan --image alpine:3.19 \
  --format cyclonedx-json --output /tmp/alpine-auto.cdx.json --no-deep-hash 2>&1 | \
  grep -E "docker source failed|podman|remote"
```

**Expected post-m206**:
- Default order `docker,podman,remote` tries docker first, fails cleanly, falls back to podman, succeeds.
- Emitted CDX still has `mikebom:image-source: "podman"` (winning source).

## Reproducer 3 — FR-007 fallback ladder (podman unavailable)

```bash
# Simulate podman-not-installed by using a scan target that isn't cached in podman.
mikebom sbom scan --image nonexistent-image:latest --image-src docker,podman,remote \
  --format cyclonedx-json --output /tmp/none.cdx.json 2>&1 | \
  grep -E "docker source failed|podman source failed|remote source failed"
```

**Expected post-m206**: WARN lines for each source that fails; scan ultimately errors non-zero naming all sources tried. No corruption of prior scan state.

## Reproducer 4 — FR-005 byte-identity for non-podman scans

```bash
# Scan a non-image target (a local directory). Pre-m206 output MUST equal
# post-m206 output — mikebom:image-source annotation is absent because
# image_source is None.
git worktree add /tmp/m206-baseline main
cd /tmp/m206-baseline && cargo build -p mikebom --release
cd -
cargo build -p mikebom --release

/tmp/m206-baseline/target/release/mikebom --offline sbom scan --path mikebom-cli/tests/fixtures/public_corpus/npm-express \
  --format cyclonedx-json --output /tmp/baseline.cdx.json --no-deep-hash

target/release/mikebom --offline sbom scan --path mikebom-cli/tests/fixtures/public_corpus/npm-express \
  --format cyclonedx-json --output /tmp/postfix.cdx.json --no-deep-hash

diff /tmp/baseline.cdx.json /tmp/postfix.cdx.json && echo "byte-identical (FR-005 verified)"
```

**Expected post-m206**: `byte-identical (FR-005 verified)`. If diff shows drift on a non-image scan, the C124 annotation gate leaked.

## Reproducer 5 — Storage-driver detection (vfs/btrfs fallback + WARN)

If your podman is configured for a non-overlay driver (rare):

```bash
# Check driver.
grep -E "^driver" ~/.config/containers/storage.conf 2>/dev/null || echo "driver: overlay (default)"

# If driver != overlay, run mikebom and confirm WARN + fallback.
mikebom sbom scan --image alpine:3.19 --image-src podman,remote 2>&1 | grep -E "storage driver.*not supported|remote"
```

**Expected post-m206 (if non-overlay driver)**: WARN log names the driver + falls back to `remote` per FR-007. Scan succeeds via remote pull.

## Pre-PR gate

```bash
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets` (zero errors, zero warnings) AND `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`) MUST pass green.

## Empirical re-verification at implement time (m199-m205 lesson)

Per `feedback_verify_research_empirical_claims` memory: before finalizing tasks.md, re-run:

```bash
# Confirm the line-numbers cited in plan.md / data-model.md are still valid.
grep -n "pub enum ImageSource\|Docker,\|Remote,\|default_value = \"docker,remote\"" mikebom-cli/src/cli/scan_cmd.rs | head
grep -n "assemble_docker_save_tarball" mikebom-cli/src/scan_fs/oci_pull/tarball.rs
grep -n "fn extract" mikebom-cli/src/scan_fs/docker_image.rs | head
grep -oE 'row_id: "C1[0-9]+"' mikebom-cli/src/parity/extractors/mod.rs | sort -u | tail  # confirm C123 highest; C124 free.
```

**Expected**: enum def near line 54-62; default value at 234; assembler helper at tarball.rs:66; extract fn at docker_image.rs:96; highest catalog row C123.

If any drift → update tasks.md instructions accordingly before implementing.
