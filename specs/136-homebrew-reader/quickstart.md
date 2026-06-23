# Quickstart — milestone 136 Homebrew (brew + Linuxbrew) reader

Operator-facing walkthrough of the scenarios this milestone surfaces.

## Scenario 1 — Scan a macOS Apple Silicon developer machine (US1 / SC-001)

The default install prefix on Apple Silicon Macs is `/opt/homebrew`. Running mikebom against your home dev machine emits one component per `brew install`-ed formula.

```bash
mikebom --offline sbom scan --path / --output /tmp/laptop.cdx.json
```

Inspect just the Homebrew components:

```bash
jq '
  .components[]
  | select(.purl != null and (.purl | startswith("pkg:brew/")))
  | .purl
' /tmp/laptop.cdx.json | sort | head -20
```

Expected output (excerpt):

```
"pkg:brew/curl@8.5.0"
"pkg:brew/git@2.43.0"
"pkg:brew/jq@1.7.1"
"pkg:brew/node@20.10.0"
"pkg:brew/openssl@3@3.4.0"
"pkg:brew/python@3.12@3.12.7"
...
```

Count check against `brew list --formula`:

```bash
brew list --formula | wc -l
# 87

jq '[.components[] | select(.purl != null and (.purl | startswith("pkg:brew/")) and (.purl | contains("type=cask") | not))] | length' /tmp/laptop.cdx.json
# 87   ✓
```

Steady-state expectation: exact match. Mismatch by 1–2 usually means a formula was installed mid-scan.

## Scenario 2 — Inspect a formula's dep edges

```bash
jq '
  .components[]
  | select(.purl == "pkg:brew/openssl@3@3.4.0")
  | .bom-ref
' /tmp/laptop.cdx.json
# "pkg:brew/openssl@3@3.4.0"

jq '
  .dependencies[]
  | select(.ref == "pkg:brew/curl@8.5.0")
  | .dependsOn
' /tmp/laptop.cdx.json
# [
#   "pkg:brew/openssl@3@3.4.0",
#   "pkg:brew/brotli@1.1.0",
#   ...
# ]
```

The dependsOn relationships come straight from each formula's `runtime_dependencies` array in `INSTALL_RECEIPT.json`. They target real bom-refs in the emitted SBOM.

## Scenario 3 — Scan an Intel macOS install (US2 / SC-002)

Intel macOS defaults to `/usr/local` as the Homebrew prefix. The reader detects this independently via `/usr/local/Cellar/` existence.

```bash
mikebom --offline sbom scan --path / --output /tmp/intel.cdx.json
jq '.components[] | select(.purl | startswith("pkg:brew/")) | .purl' /tmp/intel.cdx.json | head -5
```

Critically, the emitted PURLs are IDENTICAL between Apple Silicon and Intel for the same formula — the install-prefix does NOT leak into the PURL identity. A `curl@8.5.0` component on Apple Silicon and on Intel both emit as `pkg:brew/curl@8.5.0`.

## Scenario 4 — Scan a Linuxbrew install on a Debian rootfs (US2)

Linuxbrew installs at `/home/linuxbrew/.linuxbrew/`. Coexists with the system's dpkg / apk / rpm — both sets emit cooperatively.

```bash
mikebom --offline sbom scan --path / --output /tmp/linuxbrew.cdx.json

# Distro packages (from dpkg):
jq '[.components[] | select(.purl | startswith("pkg:deb/"))] | length' /tmp/linuxbrew.cdx.json
# 412

# Linuxbrew formulae:
jq '[.components[] | select(.purl | startswith("pkg:brew/"))] | length' /tmp/linuxbrew.cdx.json
# 27

# Both surface — Homebrew supplements the underlying distro packages, never replaces them.
```

## Scenario 5 — Third-party tap formulae (SC-007)

A formula installed from a non-default tap (e.g., `hashicorp/tap/terraform`) carries a `tap=` qualifier:

```bash
brew tap hashicorp/tap
brew install hashicorp/tap/terraform

mikebom --offline sbom scan --path /opt/homebrew --output /tmp/tap.cdx.json
jq '.components[] | select(.name == "terraform") | .purl' /tmp/tap.cdx.json
# "pkg:brew/terraform@1.10.0?tap=hashicorp/tap"
```

Default-tap formulae (`homebrew/core`) MUST NOT carry the qualifier:

```bash
jq '.components[] | select(.name == "curl") | .purl' /tmp/tap.cdx.json
# "pkg:brew/curl@8.5.0"     ← no ?tap= qualifier (core formula)
```

## Scenario 6 — macOS Casks (US3 / SC-003)

Casks (GUI app installers) live under `<prefix>/Caskroom/`. Modern Homebrew 4.0+ writes JSON metadata; older `.rb`-only casks warn-and-skip.

```bash
brew install --cask visual-studio-code firefox

mikebom --offline sbom scan --path / --output /tmp/casks.cdx.json
jq '.components[] | select(.purl | contains("type=cask")) | .purl' /tmp/casks.cdx.json
# "pkg:brew/visual-studio-code@1.95.3?type=cask"
# "pkg:brew/firefox@121.0?type=cask"
```

Casks have no dep edges (FR-005) — `depends_on.formula` is rarely populated in real-world casks; v1 doesn't surface what little exists.

## Scenario 7 — No-op on non-Homebrew rootfs (SC-004 regression invariant)

A pure Linux server (no `/opt/homebrew/`, no `/usr/local/Cellar/`, no `/home/linuxbrew/`) produces zero brew components and zero warnings:

```bash
mikebom --offline sbom scan --path /mnt/server-rootfs --output /tmp/server.cdx.json 2>/tmp/scan.log

jq '[.components[] | select(.purl | startswith("pkg:brew/"))] | length' /tmp/server.cdx.json
# 0

grep -c 'brew\|homebrew' /tmp/scan.log
# 0  (no warnings; no debug noise)
```

The emitted SBOM is byte-identical (modulo timestamps + serial numbers) to a pre-milestone-136 baseline for the same rootfs.

## Scenario 8 — Malformed receipt graceful degradation (SC-005)

A fixture where one formula's `INSTALL_RECEIPT.json` is corrupted alongside three valid formulae:

```bash
mikebom --offline sbom scan --path /tmp/corrupted-brew --output /tmp/corrupted.cdx.json 2>/tmp/scan.log

# Scan exit code:
echo $?
# 0  (scan succeeded — partial output preserved)

# brew component count: 3 (the broken formula dropped, the three valid ones emit):
jq '[.components[] | select(.purl | startswith("pkg:brew/"))] | length' /tmp/corrupted.cdx.json
# 3

# Warn for the corrupted formula:
grep 'brew:.*failed\|brew:.*skip' /tmp/scan.log
# WARN mikebom::scan_fs::package_db::brew: brew: failed to parse INSTALL_RECEIPT.json, skipping formula path=/tmp/corrupted-brew/opt/homebrew/Cellar/broken/1.0
```

## Scenario 9 — Ruby-DSL casks warn-and-skip (R5)

A pre-4.0 cask install where only `Casks/<token>.rb` exists (no `.json`):

```bash
mikebom --offline sbom scan --path /mnt/old-macos --output /tmp/old.cdx.json 2>/tmp/scan.log

grep 'Ruby-DSL' /tmp/scan.log
# WARN mikebom::scan_fs::package_db::brew: brew: cask transmission at /mnt/old-macos/opt/homebrew/Caskroom/transmission/3.00 has only Ruby-DSL metadata (no Casks/transmission.json); skipping — Ruby parsing is out of scope per Constitution Principle I
```

Operator action: `brew reinstall transmission` to upgrade to API-backed JSON metadata. mikebom doesn't perform the reinstall — it emits a diagnostic and continues.

## Verification commands

End-to-end SC validations:

```bash
# SC-001 — Apple Silicon baseline + dep edges
cargo test -p mikebom --test brew_apple_silicon_baseline

# SC-002 — Intel + Linuxbrew prefix detection
cargo test -p mikebom --test brew_alternate_prefixes

# SC-003 — Cask emission with type=cask qualifier
cargo test -p mikebom --test brew_casks

# SC-004 — Non-Homebrew byte-identity invariant
cargo test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression

# SC-005 — Malformed-receipt graceful degradation
cargo test -p mikebom --test brew_edge_cases -- malformed_receipt

# SC-006 — Standard PURL filter usability (no brew-specific consumer code)
mikebom --offline sbom scan --path <rootfs> --format cyclonedx-json --output /tmp/out.cdx.json
jq '.components[] | select(.purl | startswith("pkg:brew/"))' /tmp/out.cdx.json
# Returns every brew-derived component without any custom filter

# SC-007 — Tap qualifier presence/absence
cargo test -p mikebom --test brew_apple_silicon_baseline -- tap_qualifier
```

## Cross-format byte-equivalence check

Same scan, all three formats — the brew components must agree:

```bash
mikebom --offline sbom scan --path /opt/homebrew \
  --format cyclonedx-json --format spdx-2.3-json --format spdx-3-json \
  --output cyclonedx-json=/tmp/brew.cdx.json \
  --output spdx-2.3-json=/tmp/brew.spdx.json \
  --output spdx-3-json=/tmp/brew.spdx3.json

jq '[.components[] | select(.purl | startswith("pkg:brew/")) | .purl] | sort' /tmp/brew.cdx.json > /tmp/cdx-brew.txt
jq '[.packages[].externalRefs[]? | select(.referenceType == "purl") | .referenceLocator | select(startswith("pkg:brew/"))] | sort' /tmp/brew.spdx.json > /tmp/spdx-brew.txt
jq '[.["@graph"][] | select(.software_packageUrl? | tostring | startswith("pkg:brew/")) | .software_packageUrl] | sort' /tmp/brew.spdx3.json > /tmp/spdx3-brew.txt

diff /tmp/cdx-brew.txt /tmp/spdx-brew.txt
diff /tmp/cdx-brew.txt /tmp/spdx3-brew.txt
# (no output = success)
```

The existing parity-test harness covers this automatically via the A1 (PURL) row — no new C-row needed because brew rides the native PURL identity per Constitution Principle V (informal type-name notwithstanding).

## Known soft regression — file-claim duplicates

Per spec Out-of-Scope and research §R5, the milestone-136 reader does NOT integrate with the binary-walker file-claim tracker. On rootfs scans where the binary walker fires (Linux scans, mostly), you may see:

```bash
jq '.components[] | select(.name == "curl") | .purl' /tmp/laptop.cdx.json
# "pkg:brew/curl@8.5.0"                                       ← from brew reader
# "pkg:generic/curl?file-sha256=abc..."                       ← from binary walker
```

This is acceptable for v1 — the two entries are filterable by source (`mikebom:source-type` annotation). A sibling follow-up will integrate file-claim with proper symlink resolution.
