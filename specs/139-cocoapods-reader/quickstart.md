# Quickstart — milestone 139 CocoaPods reader

Operator-facing walkthrough of the scenarios this milestone surfaces.

## Scenario 1 — Scan an iOS app project (US1 / SC-001)

An iOS developer's app source tree with `Podfile` + `Podfile.lock`:

```bash
mikebom --offline sbom scan --path . --output /tmp/app.cdx.json
```

Inspect main-module + pods:

```bash
# Main-module (derived from `target 'MyApp' do` block):
jq '.metadata.component' /tmp/app.cdx.json
# {"bom-ref": "pkg:cocoapods/MyApp@0.0.0-unknown",
#  "name": "MyApp", "version": "0.0.0-unknown",
#  "purl": "pkg:cocoapods/MyApp@0.0.0-unknown", ...
#  "properties": [
#    {"name": "mikebom:component-role", "value": "main-module"},
#    {"name": "mikebom:source-type", "value": "cocoapods-main-module"}
#  ]}

# Direct + transitive pods:
jq '.components[] | select(.purl | startswith("pkg:cocoapods/")) | .purl' /tmp/app.cdx.json | sort | head -10
# "pkg:cocoapods/AFNetworking@4.0.1"
# "pkg:cocoapods/Firebase@10.20.0#Auth"           ← subspec subpath form
# "pkg:cocoapods/Firebase@10.20.0#Core"           ← subspec subpath form
# "pkg:cocoapods/FirebaseCore@10.20.0"
# "pkg:cocoapods/FirebaseInstallations@10.20.0"
# "pkg:cocoapods/GoogleUtilities@7.13.0#Environment"
# "pkg:cocoapods/SDWebImage@5.18.10"
# ...
```

Count check against `pod outdated`:

```bash
pod outdated --no-repo-update | grep '^  - ' | wc -l
# 0 (lockfile is current)

# Count pinned pods:
jq '[.components[] | select(.purl | startswith("pkg:cocoapods/")) | select(.properties[]? | .name == "mikebom:source-type" and .value != "cocoapods-main-module")] | length' /tmp/app.cdx.json
# 47  (matches Podfile.lock PODS: count exactly per SC-001)
```

## Scenario 2 — Source discriminator distinction (US2 / SC-002)

An iOS app whose Podfile.lock mixes trunk + git + path + subspec:

```bash
mikebom --offline sbom scan --path . --output /tmp/mixed.cdx.json
```

Filter by source type:

```bash
# Trunk pods only:
jq '.components[] | select(.properties[]? | .name == "mikebom:source-type" and .value == "cocoapods-trunk") | .purl' /tmp/mixed.cdx.json
# "pkg:cocoapods/AFNetworking@4.0.1"
# "pkg:cocoapods/Firebase@10.20.0#Core"
# "pkg:cocoapods/Firebase@10.20.0#Auth"

# Git-source pods:
jq '.components[] | select(.properties[]? | .name == "mikebom:source-type" and .value == "cocoapods-git") | .purl' /tmp/mixed.cdx.json
# "pkg:cocoapods/MyFork@1.5.0?vcs_url=git+https://github.com/foo/my-fork.git"

# Path pods (development pods in monorepo):
jq '.components[] | select(.properties[]? | .name == "mikebom:source-type" and .value == "cocoapods-path") | .purl' /tmp/mixed.cdx.json
# "pkg:generic/LocalLib@0.1.0"

# Subspecs only (filter by # in PURL):
jq '.components[] | select(.purl | contains("#")) | .purl' /tmp/mixed.cdx.json
# "pkg:cocoapods/Firebase@10.20.0#Core"
# "pkg:cocoapods/Firebase@10.20.0#Auth"
```

## Scenario 3 — Git-source pod with resolved SHA (US2 / Q2)

A pod declared `pod 'MyFork', :git => '...', :branch => 'main'` resolves to a specific commit at `pod install` time:

```bash
jq '.components[] | select(.purl | contains("vcs_url"))' /tmp/mixed.cdx.json
# {
#   "name": "MyFork",
#   "version": "1.5.0",
#   "purl": "pkg:cocoapods/MyFork@1.5.0?vcs_url=git+https://github.com/foo/my-fork.git",
#   "properties": [
#     {"name": "mikebom:source-type", "value": "cocoapods-git"},
#     {"name": "mikebom:vcs-ref", "value": "eb39649a76b87e8451baf75d10ce82ca3a3d5601"},       ← from CHECKOUT OPTIONS
#     {"name": "mikebom:vcs-declared-ref", "value": "main"}                                  ← from EXTERNAL SOURCES
#   ]
# }
```

The `mikebom:vcs-ref` is the authoritative resolved 40-char SHA (per Q2 clarification — CHECKOUT OPTIONS section); `mikebom:vcs-declared-ref` preserves the operator's pin (`:branch` / `:tag` / `:commit`).

## Scenario 4 — Lockfile-only commit (Q1 dir-basename fallback)

Some iOS projects `.gitignore` the Podfile itself (treating it as developer-local) and commit only the lockfile:

```bash
ls /tmp/lockfile-only-project/
# Podfile.lock     (no Podfile)

mikebom --offline sbom scan --path /tmp/lockfile-only-project --output /tmp/lo.cdx.json

# Main-module derived from parent-dir basename per Q1:
jq '.metadata.component' /tmp/lo.cdx.json
# {"name": "lockfile-only-project", "version": "0.0.0-unknown",
#  "purl": "pkg:cocoapods/lockfile-only-project@0.0.0-unknown", ...
#  "properties": [
#    {"name": "mikebom:component-role", "value": "main-module"}
#  ]}
```

## Scenario 5 — iOS library project, design-tier (US3 / SC-003)

An iOS library with `Podfile` but no `Podfile.lock`:

```bash
mikebom --offline sbom scan --path . --output /tmp/lib.cdx.json

# Components are design-tier (constraint preserved, not pinned):
jq '.components[] | {name, purl, props: .properties}' /tmp/lib.cdx.json | head -20
# {
#   "name": "AFNetworking",
#   "purl": "pkg:cocoapods/AFNetworking@~> 4.0",           ← constraint not pinned
#   "props": [
#     {"name": "mikebom:sbom-tier", "value": "design"},
#     {"name": "mikebom:requirement-range", "value": "~> 4.0"},
#     {"name": "mikebom:evidence-kind", "value": "cocoapods-podfile"},
#     {"name": "mikebom:source-type", "value": "cocoapods-trunk"}
#   ]
# }
```

## Scenario 6 — Deployed-tier container scan (US3 / Q3)

A built iOS container layer that shipped only `Pods/Manifest.lock` (no source-tree Podfile.lock):

```bash
ls /tmp/built-ipa-rootfs/Pods/
# Manifest.lock   (no Podfile.lock at project root)

mikebom --offline sbom scan --path /tmp/built-ipa-rootfs --output /tmp/built.cdx.json

# Components emit with sbom-tier=deployed per Q3:
jq '.components[] | select(.properties[]? | .name == "mikebom:sbom-tier" and .value == "deployed") | .name' /tmp/built.cdx.json | head -5
# "AFNetworking"
# "Firebase/Core"
# "SDWebImage"

# Evidence-kind distinguishes the install-time source:
jq '.components[] | select(.properties[]? | .name == "mikebom:evidence-kind" and .value == "cocoapods-manifest-lock") | .name' /tmp/built.cdx.json | wc -l
# (matches the Manifest.lock PODS count)
```

## Scenario 7 — SHA-1 hash emission (SC-007 / FR-008)

Pods carry SHA-1 hashes from `SPEC CHECKSUMS:` (root-keyed):

```bash
# Standard pod:
jq '.components[] | select(.name == "AFNetworking") | .hashes' /tmp/app.cdx.json
# [{"alg": "SHA-1", "content": "abc123...40hex"}]

# Subspec components share the root pod's SHA-1:
jq '.components[] | select(.purl | startswith("pkg:cocoapods/Firebase@")) | {purl, hash: .hashes[0].content}' /tmp/app.cdx.json
# {"purl": "pkg:cocoapods/Firebase@10.20.0#Core",  "hash": "def456..."}
# {"purl": "pkg:cocoapods/Firebase@10.20.0#Auth",  "hash": "def456..."}    ← SAME hash (root-keyed per FR-008)
```

## Scenario 8 — No-op on non-iOS rootfs (SC-004 regression invariant)

```bash
mikebom --offline sbom scan --path /mnt/server-rootfs --output /tmp/server.cdx.json 2>/tmp/scan.log

jq '[.components[] | select(.purl | startswith("pkg:cocoapods/"))] | length' /tmp/server.cdx.json
# (matches pre-feature baseline — CocoaPods contributes zero)

grep -c 'cocoapods\|Podfile\|Manifest.lock' /tmp/scan.log
# 0
```

## Scenario 9 — Malformed lockfile graceful degradation (SC-005)

Monorepo where one Podfile.lock has corrupted YAML alongside three valid iOS project subdirs:

```bash
mikebom --offline sbom scan --path /tmp/corrupted-ios --output /tmp/corrupted.cdx.json 2>/tmp/scan.log

echo $?
# 0  (scan succeeded — partial output preserved)

# Components from valid projects emit; corrupted project falls back to Podfile design-tier:
jq '[.components[] | select(.purl | startswith("pkg:cocoapods/"))] | length' /tmp/corrupted.cdx.json
# (sum of valid projects + design-tier fallback from corrupted project's sibling Podfile)

# Warning fires for corrupted lockfile:
grep 'failed to parse Podfile.lock' /tmp/scan.log
# WARN mikebom::scan_fs::package_db::cocoapods: cocoapods: failed to parse Podfile.lock, falling back to design-tier from Podfile path=...
```

## Verification commands

```bash
# SC-001 — iOS app baseline
cargo test -p mikebom --test cocoapods_ios_app_baseline

# SC-002 — Source discriminator distinction
cargo test -p mikebom --test cocoapods_source_discriminators

# SC-003 + Q1 + Q3 — design + deployed + dir-basename fallback
cargo test -p mikebom --test cocoapods_tier_fallbacks

# SC-004 — Non-iOS byte-identity invariant
cargo test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression

# SC-005 + multi-target + CHECKOUT OPTIONS + SHA-1 + subspec multi-level
cargo test -p mikebom --test cocoapods_edge_cases

# SC-006 — standard PURL filter usability
mikebom --offline sbom scan --path <project> --format cyclonedx-json --output /tmp/out.cdx.json
jq '.components[] | select(.purl | startswith("pkg:cocoapods/"))' /tmp/out.cdx.json

# SC-007 — SHA-1 hash emission
cargo test -p mikebom --test cocoapods_ios_app_baseline -- sha1_hash

# SC-008 — main-module emission
cargo test -p mikebom --test cocoapods_ios_app_baseline -- main_module

# SC-009 — subspec PURL form
cargo test -p mikebom --test cocoapods_source_discriminators -- subspec_subpath
```

## Cross-format byte-equivalence check

Same scan, all three formats — CocoaPods PURL set MUST agree:

```bash
mikebom --offline sbom scan --path my_ios_app \
  --format cyclonedx-json --format spdx-2.3-json --format spdx-3-json \
  --output cyclonedx-json=/tmp/ios.cdx.json \
  --output spdx-2.3-json=/tmp/ios.spdx.json \
  --output spdx-3-json=/tmp/ios.spdx3.json

jq '[.components[] | select(.purl | startswith("pkg:cocoapods/")) | .purl] | sort' /tmp/ios.cdx.json > /tmp/cdx-pods.txt
jq '[.packages[].externalRefs[]? | select(.referenceType == "purl") | .referenceLocator | select(startswith("pkg:cocoapods/"))] | sort' /tmp/ios.spdx.json > /tmp/spdx-pods.txt
jq '[.["@graph"][] | select(.software_packageUrl? | tostring | startswith("pkg:cocoapods/")) | .software_packageUrl] | sort' /tmp/ios.spdx3.json > /tmp/spdx3-pods.txt

diff /tmp/cdx-pods.txt /tmp/spdx-pods.txt
diff /tmp/cdx-pods.txt /tmp/spdx3-pods.txt
# (no output = success)
```

## Known deferrals (documented in spec Out-of-Scope)

- **License emission**: `Podfile.lock` carries no license; lives in each pod's `Pods/<PodName>/<PodName>.podspec`. Cross-reader follow-up.
- **Transitive dep edges**: v1 emits main-module → direct deps only; transitive inter-pod edges deferred to v1.1.
- **Per-target attribution**: multi-target Podfiles emit each pinned pod once; per-target ownership deferred to v1.1.
- **Private CocoaPods spec repo provenance**: `SPEC REPOS:` section not consumed v1 (no `repository_url=` qualifier emitted for private mirrors).
- **`Pods/<pod>/` directory walking**: out of scope; lockfile is the v1 source of truth.
- **syft/trivy compatibility annotation** (`mikebom:also-known-as` with name-folded PURL): deferred to v1.1.
- **Pre-CocoaPods-1.0 lockfile format**: warn-and-skip on detection (exceptionally rare in 2026).
