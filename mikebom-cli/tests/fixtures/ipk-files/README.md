# Vendored ipk fixtures — milestone 169 (issue #500)

Vendored `.ipk` files from the OpenWrt 23.05.5 x86_64 base release feed
(`https://downloads.openwrt.org/releases/23.05.5/packages/x86_64/base/`).
Used by the ipk archive-file reader unit tests + integration test at
`mikebom-cli/tests/ipk_reader.rs`.

## Provenance

Downloaded from OpenWrt's official release CDN on 2026-07-06. Each file
is a small representative sample — `all` (arch-independent) mixed with
`x86_64` (arch-specific), various license expressions, various dependency
patterns.

| File | Size | Description |
|---|---:|---|
| `6in4_28_all.ipk` | 2.5 KB | 6in4 tunnel support (GPL-2.0, arch=all, small; useful for well-formed baseline) |
| `6to4_13_all.ipk` | 1.9 KB | 6to4 tunnel support (GPL-2.0, arch=all, small) |
| `464xlat_13_x86_64.ipk` | 5.0 KB | 464XLAT stateful translator (GPL-2.0, arch=x86_64) |
| `adb_android.5.0.2_r1-3_x86_64.ipk` | 63 KB | Android Debug Bridge (Apache-2.0, arch=x86_64, larger; useful for size-cap tests) |
| `agetty_2.39-2_x86_64.ipk` | 24 KB | Alternative getty (GPL-2.0+ / BSD-4-Clause, arch=x86_64, mid-size) |

## Format (verified 2026-07-06)

All 5 files use the **gzipped tarball** outer envelope (`gzip( tar { debian-binary, control.tar.gz, data.tar.gz } )`), NOT the ar envelope the spec's initial draft assumed. This matches modern `opkg-utils/opkg-build` output. See spec.md Background + research.md §R2 for the format discovery.

## Reproducibility

Files are pinned to the 23.05.5 release. To re-fetch (e.g., after a fixtures-repo migration):

```bash
for pkg in 6in4_28_all 6to4_13_all 464xlat_13_x86_64 adb_android.5.0.2_r1-3_x86_64 agetty_2.39-2_x86_64; do
    curl -sSL -o "${pkg}.ipk" "https://downloads.openwrt.org/releases/23.05.5/packages/x86_64/base/${pkg}.ipk"
done
```

## Licensing

Each `.ipk` file's contents are governed by the license declared in its own `control` file (per-package: GPL-2.0, Apache-2.0, LGPL-2.1, etc.). Vendored here under the fair-use test-fixture exception common across SBOM tools' regression suites. No modification to the artifacts.
