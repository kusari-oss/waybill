# Quickstart — milestone 135 Arch Linux pacman/alpm reader

Operator-facing walkthrough of the scenarios this milestone surfaces.

## Scenario 1 — Scan a stock Arch container (US1 / SC-001)

```bash
docker pull archlinux:latest
docker save archlinux:latest -o /tmp/arch.tar
mikebom --offline sbom scan --image-tar /tmp/arch.tar --output /tmp/arch.cdx.json
```

Inspect:

```bash
jq '
  .components[]
  | select(.purl != null and (.purl | startswith("pkg:alpm/")))
  | .purl
' /tmp/arch.cdx.json | sort | head -20
```

Expected output (excerpt):

```
"pkg:alpm/arch/acl@2.3.2-1?arch=x86_64"
"pkg:alpm/arch/argon2@20190702-6?arch=x86_64"
"pkg:alpm/arch/attr@2.5.2-1?arch=x86_64"
"pkg:alpm/arch/audit@4.0.2-1?arch=x86_64"
"pkg:alpm/arch/bash@5.2.026-1?arch=x86_64"
...
```

Count check against `pacman -Q`:

```bash
# in-container:
docker run --rm archlinux:latest pacman -Q | wc -l
# 178

# emitted SBOM:
jq '[.components[] | select(.purl != null and (.purl | startswith("pkg:alpm/")))] | length' /tmp/arch.cdx.json
# 178   ✓
```

The counts should match exactly on a frozen image.

## Scenario 2 — Scan a SteamOS rootfs (US2)

Setup: a SteamOS rootfs has `/etc/os-release` declaring `ID=steamos` and `VERSION_ID=3.5.7`.

```bash
mikebom --offline sbom scan --path /mnt/steamos-rootfs --output /tmp/steamos.cdx.json
```

Inspect:

```bash
jq '.components[] | select(.purl != null and (.purl | contains("steamos"))) | .purl' /tmp/steamos.cdx.json | head -5
```

Expected:

```
"pkg:alpm/steamos/bash@5.2.026-1?arch=x86_64&distro=steamos-3.5.7"
"pkg:alpm/steamos/glibc@2.40-1?arch=x86_64&distro=steamos-3.5.7"
"pkg:alpm/steamos/curl@8.5.0-1?arch=x86_64&distro=steamos-3.5.7"
...
```

Note the `distro=steamos-3.5.7` qualifier (present because `VERSION_ID` is declared) and the `steamos` namespace (NOT `arch`).

## Scenario 3 — No-op on a non-Arch rootfs (SC-003 regression invariant)

A Debian rootfs MUST produce zero alpm components and zero warnings:

```bash
mikebom --offline sbom scan --path /mnt/debian-rootfs --output /tmp/debian.cdx.json 2>/tmp/scan.log

jq '[.components[] | select(.purl != null and (.purl | startswith("pkg:alpm/")))] | length' /tmp/debian.cdx.json
# 0

grep -c 'pacman\|alpm' /tmp/scan.log
# 0  (no warnings, no debug noise mentioning pacman/alpm)
```

The emitted SBOM is byte-identical (modulo timestamps + serial numbers) to a pre-milestone-135 baseline for the same Debian rootfs.

## Scenario 4 — File-claim dedup (US3 / SC-004)

Setup: a synthetic Arch rootfs where pacman owns `/usr/bin/bash`, and `bash` is a real ELF at that path.

```bash
mikebom --offline sbom scan --path /mnt/arch-rootfs --output /tmp/arch.cdx.json
```

Inspect: ensure exactly one `bash` component (the alpm one), no `pkg:generic/bash` duplicate:

```bash
jq '.components[] | select(.name == "bash") | .purl' /tmp/arch.cdx.json
# "pkg:alpm/arch/bash@5.2.026-1?arch=x86_64"
# (single line — no second pkg:generic/bash entry)
```

## Scenario 5 — Multi-OS-DB hybrid rootfs (R6)

A test fixture deliberately combining a Debian rootfs with an Alpine chroot + an Arch chroot — all three OS DBs present.

```bash
mikebom --offline sbom scan --path /mnt/hybrid-rootfs --output /tmp/hybrid.cdx.json

jq '.components[] | select(.purl != null) | .purl | sub("@.*"; "")' /tmp/hybrid.cdx.json | sort -u | head -10
```

Expected (excerpt — all three OS-package types coexist):

```
"pkg:alpm/arch/bash"
"pkg:apk/alpine/bash"
"pkg:deb/debian/bash"
"pkg:apk/alpine/busybox"
"pkg:deb/debian/curl"
"pkg:alpm/arch/curl"
"pkg:apk/alpine/curl"
...
```

All three readers run independently and emit cooperatively; file-claim accumulation prevents the binary walker from producing duplicate `pkg:generic/bash` entries.

## Scenario 6 — Malformed `desc` graceful degradation (SC-005)

A fixture where one `local/<pkg>-<ver>/desc` file is deliberately corrupted (missing the `%NAME%` block) alongside three valid packages.

```bash
mikebom --offline sbom scan --path /tmp/corrupted-arch --output /tmp/corrupted.cdx.json 2>/tmp/scan.log

# Scan exit code:
echo $?
# 0  (scan succeeded — partial output preserved)

# Alpm component count: 3 (the corrupted package dropped, the three valid ones emit):
jq '[.components[] | select(.purl != null and (.purl | startswith("pkg:alpm/")))] | length' /tmp/corrupted.cdx.json
# 3

# Warn for the corrupted package:
grep 'pacman:.*missing' /tmp/scan.log
# WARN mikebom::scan_fs::package_db::alpm: pacman: missing %NAME% in desc, skipping path=/var/lib/pacman/local/broken-1.0-1
```

## Verification commands

End-to-end SC validations:

```bash
# SC-001 — Arch container scan completeness
cargo test -p mikebom --test alpm_arch_baseline

# SC-002 — Derivative-distro namespace correctness (SteamOS / Manjaro / EndeavourOS / CachyOS)
cargo test -p mikebom --test alpm_derivative_distros

# SC-003 — Non-Arch byte-identity invariant
cargo test -p mikebom --test cdx_regression
cargo test -p mikebom --test spdx_regression
cargo test -p mikebom --test spdx3_regression

# SC-004 — Binary-walker file-claim dedup
cargo test -p mikebom --test alpm_file_claim_dedupe

# SC-005 — Malformed desc graceful degradation
cargo test -p mikebom --test alpm_edge_cases -- malformed_desc

# SC-006 — Standard PURL filter usability (no alpm-specific consumer code)
mikebom --offline sbom scan --path <rootfs> --format cyclonedx-json --output /tmp/out.cdx.json
jq '.components[] | select(.purl | startswith("pkg:alpm/"))' /tmp/out.cdx.json
# Should return every alpm-derived component without any custom filter
```

## Cross-format byte-equivalence check

Same scan, all three formats — the alpm components must agree:

```bash
mikebom --offline sbom scan --path /mnt/arch-rootfs \
  --format cyclonedx-json --format spdx-2.3-json --format spdx-3-json \
  --output cyclonedx-json=/tmp/arch.cdx.json \
  --output spdx-2.3-json=/tmp/arch.spdx.json \
  --output spdx-3-json=/tmp/arch.spdx3.json

# CDX
jq '[.components[] | select(.purl | startswith("pkg:alpm/")) | .purl] | sort' /tmp/arch.cdx.json > /tmp/cdx-alpm.txt
# SPDX 2.3
jq '[.packages[].externalRefs[]? | select(.referenceType == "purl") | .referenceLocator | select(startswith("pkg:alpm/"))] | sort' /tmp/arch.spdx.json > /tmp/spdx-alpm.txt
# SPDX 3
jq '[.["@graph"][] | select(.software_packageUrl? | tostring | startswith("pkg:alpm/")) | .software_packageUrl] | sort' /tmp/arch.spdx3.json > /tmp/spdx3-alpm.txt

# All three lists should be byte-identical:
diff /tmp/cdx-alpm.txt /tmp/spdx-alpm.txt
diff /tmp/cdx-alpm.txt /tmp/spdx3-alpm.txt
# (no output = success)
```

The existing parity-test harness covers this automatically via the A1 (PURL) row — no new C-row needed because alpm rides the native PURL identity per Constitution Principle V.
