# Quickstart — milestone 169 (ipk archive-file reader + opkg installed-DB hardening)

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Data model**: [data-model.md](./data-model.md)

How to reproduce SC-001 + SC-005b + SC-011 end-to-end.

## Prerequisites

- Rust stable (workspace toolchain — same as milestones 001–168)
- `git`, `cargo`
- Fresh checkout on branch `169-ipk-opkg-reader`
- Optional (for SC-011 empirical closure): a local Yocto build environment or an OpenWrt release feed download

## Step 1: Build

```bash
cargo +stable build --release -p mikebom
```

**Expected**: clean compile with the new `ipk_file.rs` module + edited `opkg.rs` + walker allowlist entry.

## Step 2: Unit tests

```bash
cargo +stable test -p mikebom scan_fs::package_db::ipk_file
cargo +stable test -p mikebom scan_fs::package_db::opkg
```

**Expected**: ≥ 12 tests pass across the two modules:

- Archive-file (US1) tests: well-formed → correct PURL; filename fallback → correct PURL + WARN; filename non-conforming → skip + WARN; License → SPDX canonical; Depends → correct edges; Provides → annotation; archive-size cap → filename-only + annotation.
- Installed-DB (US2) tests: `/var/lib/opkg/status` primary parse → `opkg-status-db` evidence-kind; `info/*.control` fallback → INFO log; `info/*.list` skip-set; mixed archive+installed-DB → dedup with installed-DB precedence.
- Shared tests: distro-qualifier propagation; alternative-list Q2 semantic.

## Step 3: Integration test

```bash
cargo +stable test -p mikebom --test ipk_reader
```

**Expected**: synthesized scan of a mixed-fixture directory (archive `.ipk` + `/var/lib/opkg/status` synthetic tree) emits the expected component counts across all 3 formats (CDX 1.6, SPDX 2.3, SPDX 3.0.1) and passes SPDX validation.

## Step 4: Full pre-PR gate

```bash
./scripts/pre-pr.sh
```

**Expected**: `>>> all pre-PR checks passed`. Zero clippy warnings; all workspace tests pass. Golden fixtures byte-identical on ecosystems other than ipk/opkg (SC-008).

## Step 5: Empirical SC-001 reproduction (PR-body attestation path per Q3)

Requires a Yocto scarthgap `core-image-minimal` build. If unavailable, use OpenWrt as a substitute per below.

### Path A — Yocto scarthgap core-image-minimal

Build the reference image per issue #500's coordinates:

```bash
# ~30-60 min build; consult Yocto docs for setup
source poky/oe-init-build-env
# Set MACHINE=qemux86-64 in local.conf
# Set PACKAGE_CLASSES = "package_ipk" in local.conf
bitbake core-image-minimal
```

Then scan:

```bash
./target/release/mikebom --offline sbom scan \
    --path tmp/deploy/ipk/ \
    --format cyclonedx-json \
    --output /tmp/yocto-ipk.cdx.json 2>&1 | grep -E 'shape_skipped|components='

jq '.components | length' /tmp/yocto-ipk.cdx.json
```

**Expected**: `shape_skipped=0` (ipk-file portion), component count ≥ 4580.

**Attach**: paste the walker-complete tracing line + the component count to the PR body per SC-011.

### Path B — OpenWrt release feed (fallback if Yocto unavailable)

```bash
mkdir /tmp/openwrt-ipks && cd /tmp/openwrt-ipks
# Download a subset of OpenWrt 23.05.5 x86_64 base packages
wget -r -np -nd -A '.ipk' \
    https://downloads.openwrt.org/releases/23.05.5/packages/x86_64/base/
cd -

./target/release/mikebom --offline sbom scan \
    --path /tmp/openwrt-ipks \
    --format cyclonedx-json \
    --output /tmp/openwrt.cdx.json

jq '.components | length' /tmp/openwrt.cdx.json
```

**Expected**: component count matches the downloaded `.ipk` count.

## Step 6: Empirical SC-005b installed-DB reproduction

Build or acquire a Yocto/OpenWrt runtime rootfs. Scan its root:

```bash
./target/release/mikebom --offline sbom scan \
    --path /path/to/rootfs \
    --format cyclonedx-json \
    --output /tmp/rootfs.cdx.json

# Count opkg-status-db emissions
jq '[.components[] | select(.properties[]?.name == "mikebom:evidence-kind" and .properties[].value == "opkg-status-db")] | length' /tmp/rootfs.cdx.json
```

**Expected**: matches `opkg list-installed` count on the same rootfs (36 for a `core-image-minimal` build per SC-005b anchor).

## Step 7: Observability check (FR-014 fallback)

Test the FR-014 installed-DB fallback when `/var/lib/opkg/status` is absent but `info/*.control` files are present:

```bash
# Synthetic fixture omitting status
mkdir -p /tmp/opkg-fallback/var/lib/opkg/info
cat > /tmp/opkg-fallback/var/lib/opkg/info/busybox.control <<EOF
Package: busybox
Version: 1.36.1-r0
Architecture: core2-64
License: GPL-2.0
Description: Multi-call binary for embedded systems
EOF

./target/release/mikebom -v --offline sbom scan --path /tmp/opkg-fallback \
    --format cyclonedx-json --output /dev/null 2>&1 | grep 'falling back to info'
```

**Expected**: `INFO opkg installed-DB: status file absent; falling back to info/*.control per FR-014`.

## Rollback / bail-out path

Milestone 169 is an additive ecosystem-coverage milestone. If a regression surfaces:

1. Revert the merge commit — restores pre-169 state. Pre-169 behavior on non-ipk scans is byte-identical (SC-008); pre-169 behavior on ipk scans returns to the 0-component cliff.
2. No wire-format breakage — annotation keys are additive; unmodified consumers ignore unknown properties per CDX/SPDX permissiveness.
3. No golden-file breakage on non-ipk ecosystems (SC-008 preserves this invariant).
