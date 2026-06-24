# Quickstart — milestone 138 PHP/Composer reader

Operator-facing walkthrough of the scenarios this milestone surfaces.

## Scenario 1 — Scan a Laravel app project (US1 / SC-001)

A PHP backend developer's Laravel app source tree with `composer.json` + `composer.lock`:

```bash
mikebom --offline sbom scan --path . --output /tmp/app.cdx.json
```

Inspect main-module + deps:

```bash
# Main-module (the app itself):
jq '.metadata.component' /tmp/app.cdx.json
# {"bom-ref": "pkg:composer/acme/my-app@1.2.3",
#  "name": "acme/my-app", "version": "1.2.3",
#  "purl": "pkg:composer/acme/my-app@1.2.3", ...
#  "properties": [
#    {"name": "mikebom:component-role", "value": "main-module"},
#    {"name": "mikebom:source-type", "value": "composer-main-module"}
#  ]}

# Direct + transitive deps:
jq '.components[] | select(.purl | startswith("pkg:composer/")) | .purl' /tmp/app.cdx.json | sort | head -10
# "pkg:composer/guzzlehttp/guzzle@7.8.1"
# "pkg:composer/monolog/monolog@3.5.0"
# "pkg:composer/psr/log@3.0.0"
# "pkg:composer/symfony/console@v7.0.4"
# "pkg:composer/symfony/polyfill-mbstring@v1.28.0"
# ...
```

Count check against `composer show`:

```bash
composer show --format=json | jq '.installed | length'
# 87

jq '[.components[] | select(.purl | startswith("pkg:composer/")) | select(.properties[]? | .name == "mikebom:source-type" and .value != "composer-main-module")] | length' /tmp/app.cdx.json
# 87  ✓
```

## Scenario 2 — Source discriminator distinction (US2 / SC-002)

A Laravel app whose lockfile mixes Packagist + VCS + path + composer-plugin:

```bash
mikebom --offline sbom scan --path . --output /tmp/mixed.cdx.json
```

Filter by source type:

```bash
# Packagist deps only:
jq '.components[] | select(.properties[]? | .name == "mikebom:source-type" and .value == "composer-packagist") | .purl' /tmp/mixed.cdx.json
# "pkg:composer/symfony/console@v7.0.4"
# "pkg:composer/monolog/monolog@3.5.0"

# VCS deps only:
jq '.components[] | select(.properties[]? | .name == "mikebom:source-type" and .value == "composer-vcs") | .purl' /tmp/mixed.cdx.json
# "pkg:composer/acme/my-fork@dev-main?vcs_url=git+https://github.com/acme/my-fork.git"

# Path deps (operator's monorepo-local libs):
jq '.components[] | select(.properties[]? | .name == "mikebom:source-type" and .value == "composer-path") | .purl' /tmp/mixed.cdx.json
# "pkg:generic/acme-local-lib@0.1.0"

# Composer plugins (filter out of "runtime app deps" views):
jq '.components[] | select(.properties[]? | .name == "mikebom:source-type" and .value == "composer-plugin") | .purl' /tmp/mixed.cdx.json
# "pkg:composer/composer/installers@v2.3.0"
```

## Scenario 3 — Self-hosted Packagist mirror

A project pulling from a private Composer mirror at `https://repo.acme.example.com`:

```bash
jq '.components[] | select(.purl | contains("repository_url=")) | .purl' /tmp/private.cdx.json
# "pkg:composer/acme/internal_lib@2.0.0?repository_url=https://repo.acme.example.com"
```

Default-Packagist deps MUST NOT carry the qualifier:

```bash
jq '.components[] | select(.purl | startswith("pkg:composer/symfony/console@")) | .purl' /tmp/private.cdx.json
# "pkg:composer/symfony/console@v7.0.4"     ← no ?repository_url= qualifier
```

## Scenario 4 — PHP library project, design-tier mode (US3 / SC-003)

A PHP library with `composer.json` but no `composer.lock`:

```bash
mikebom --offline sbom scan --path . --output /tmp/lib.cdx.json

# Components are design-tier (constraint preserved, not pinned):
jq '.components[] | {name, purl, props: .properties}' /tmp/lib.cdx.json | head -30
# {
#   "name": "symfony/console",
#   "purl": "pkg:composer/symfony/console@^7.0",          ← constraint not pinned
#   "props": [
#     {"name": "mikebom:sbom-tier", "value": "design"},
#     {"name": "mikebom:requirement-range", "value": "^7.0"},
#     {"name": "mikebom:evidence-kind", "value": "composer-json"},
#     {"name": "mikebom:source-type", "value": "composer-packagist"}
#   ]
# }
```

Dev deps tagged with lifecycle-scope (US3 acceptance scenario 3):

```bash
jq '.components[] | select(.properties[]? | .name == "mikebom:lifecycle-scope" and .value == "development") | .name' /tmp/lib.cdx.json
# "phpunit/phpunit"
# "vimeo/psalm"
# "friendsofphp/php-cs-fixer"
```

`--exclude-scope dev` filtering works:

```bash
mikebom --offline --exclude-scope dev sbom scan --path . --output /tmp/lib-prod.cdx.json
jq '.components[] | select(.properties[]? | .name == "mikebom:lifecycle-scope" and .value == "development")' /tmp/lib-prod.cdx.json
# (empty — dev deps filtered)
```

## Scenario 5 — Deployed-tier container scan (US3 / SC-009)

A built PHP container image where the build stage shipped only `vendor/` (no `composer.json` / `composer.lock` in the final image):

```bash
mikebom --offline sbom scan --path /mnt/container-rootfs --output /tmp/container.cdx.json

# Components emit with sbom-tier=deployed:
jq '.components[] | select(.properties[]? | .name == "mikebom:sbom-tier" and .value == "deployed") | .name' /tmp/container.cdx.json | head -10
# "symfony/console"
# "monolog/monolog"
# "psr/log"
# ...

# dev-package-names entries from installed.json carry lifecycle-scope=development:
jq '.components[] | select(.properties[]? | .name == "mikebom:lifecycle-scope" and .value == "development") | .name' /tmp/container.cdx.json
# (dev tools that survived build, if any)
```

## Scenario 6 — Lockfile-vs-disk drift detection (SC-010)

A project where `composer.lock` is committed but a developer ran `composer require foo/bar` after CI captured the lockfile (without committing the updated lockfile):

```bash
mikebom --offline sbom scan --path . --output /tmp/drift.cdx.json

# Find the orphan installed.json entries (drift signal):
jq '.components[] | select(.properties[]? | .name == "mikebom:lockfile-orphan" and .value == true) | .purl' /tmp/drift.cdx.json
# "pkg:composer/foo/bar@1.5.2"     ← installed but not in lockfile

# Lockfile-pinned packages emit normally WITHOUT the orphan annotation:
jq '.components[] | select(.purl | startswith("pkg:composer/symfony/console@")) | .properties' /tmp/drift.cdx.json
# (lockfile-orphan property absent)
```

## Scenario 7 — Composer monorepo (FR-010)

A monorepo with multiple packages, each with its own `composer.json`:

```text
my_workspace/
├── packages/
│   ├── app/composer.json + composer.lock
│   ├── lib_a/composer.json + composer.lock
│   └── lib_b/composer.json + composer.lock
└── composer.json + composer.lock        ← top-level (optional)
```

```bash
mikebom --offline sbom scan --path my_workspace --output /tmp/workspace.cdx.json

# One main-module per composer.json — monorepo structure is invisible:
jq '.components[] | select(.properties[]? | .name == "mikebom:component-role" and .value == "main-module") | .purl' /tmp/workspace.cdx.json
# "pkg:composer/acme/app@1.0.0"
# "pkg:composer/acme/lib_a@0.5.0"
# "pkg:composer/acme/lib_b@0.3.0"
```

No synthetic monorepo-root component emits (per FR-010). Same-PURL deps across the lockfiles collapse via standard dedup.

## Scenario 8 — No-op on non-PHP rootfs (SC-004 regression invariant)

A pure Linux server rootfs (no `composer.json`, no `composer.lock`, no `vendor/composer/installed.json`):

```bash
mikebom --offline sbom scan --path /mnt/server-rootfs --output /tmp/server.cdx.json 2>/tmp/scan.log

jq '[.components[] | select(.purl | startswith("pkg:composer/") or startswith("pkg:generic/"))] | length' /tmp/server.cdx.json
# (matches pre-feature baseline — Composer contributes zero)

grep -c 'composer\|installed.json' /tmp/scan.log
# 0
```

SBOM bytes are identical (modulo timestamps + serial numbers) to a pre-milestone-138 baseline for the same rootfs.

## Scenario 9 — Malformed lockfile graceful degradation (SC-005)

A monorepo where one `composer.lock` has corrupted JSON alongside three valid project subdirs:

```bash
mikebom --offline sbom scan --path /tmp/corrupted-php --output /tmp/corrupted.cdx.json 2>/tmp/scan.log

# Scan exit code:
echo $?
# 0  (scan succeeded — partial output preserved)

# Components from the three valid projects emit:
jq '[.components[] | select(.purl | startswith("pkg:composer/"))] | length' /tmp/corrupted.cdx.json
# (sum of three valid projects' deps + corrupted project's design-tier fallback from composer.json)

# Warn for the broken lockfile:
grep 'composer:.*parse failed\|composer.lock' /tmp/scan.log
# WARN mikebom::scan_fs::package_db::composer: composer: failed to parse composer.lock, falling back to design-tier from composer.json path=/tmp/corrupted-php/broken/composer.lock
```

## Verification commands

End-to-end SC validations:

```bash
# SC-001 — Laravel app baseline + dep edges
cargo test -p mikebom --test composer_laravel_baseline

# SC-002 — Source discriminator distinction
cargo test -p mikebom --test composer_source_discriminators

# SC-003 — Design-tier emission (no lockfile)
cargo test -p mikebom --test composer_tier_fallbacks -- design_tier

# SC-004 — Non-PHP byte-identity invariant
cargo test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression

# SC-005 — Malformed-lockfile graceful degradation
cargo test -p mikebom --test composer_edge_cases -- malformed_lockfile

# SC-006 — Standard PURL filter usability
mikebom --offline sbom scan --path <project> --format cyclonedx-json --output /tmp/out.cdx.json
jq '.components[] | select(.purl | startswith("pkg:composer/"))' /tmp/out.cdx.json

# SC-007 — Dev-scope filterability
cargo test -p mikebom --test composer_laravel_baseline -- dev_scope

# SC-008 — Main-module emission
cargo test -p mikebom --test composer_laravel_baseline -- main_module

# SC-009 — Deployed-tier installed.json emission
cargo test -p mikebom --test composer_tier_fallbacks -- deployed_tier

# SC-010 — Lockfile-vs-disk drift detection
cargo test -p mikebom --test composer_tier_fallbacks -- lockfile_orphan
```

## Cross-format byte-equivalence check

Same scan, all three formats — Composer components must agree:

```bash
mikebom --offline sbom scan --path my_laravel_app \
  --format cyclonedx-json --format spdx-2.3-json --format spdx-3-json \
  --output cyclonedx-json=/tmp/comp.cdx.json \
  --output spdx-2.3-json=/tmp/comp.spdx.json \
  --output spdx-3-json=/tmp/comp.spdx3.json

jq '[.components[] | select(.purl | startswith("pkg:composer/")) | .purl] | sort' /tmp/comp.cdx.json > /tmp/cdx-comp.txt
jq '[.packages[].externalRefs[]? | select(.referenceType == "purl") | .referenceLocator | select(startswith("pkg:composer/"))] | sort' /tmp/comp.spdx.json > /tmp/spdx-comp.txt
jq '[.["@graph"][] | select(.software_packageUrl? | tostring | startswith("pkg:composer/")) | .software_packageUrl] | sort' /tmp/comp.spdx3.json > /tmp/spdx3-comp.txt

diff /tmp/cdx-comp.txt /tmp/spdx-comp.txt
diff /tmp/cdx-comp.txt /tmp/spdx3-comp.txt
# (no output = success)
```

## Known deferrals (documented in spec Out-of-Scope)

- **License emission**: `composer.lock` carries `license:` array per entry, but plumbing through `PackageDbEntry.licenses` end-to-end + verifying SPDX-expression canonicalization is a cross-reader follow-up (parallels milestone-135 FR-012 + milestone-136 FR-011 + milestone-137 deferrals).
- **Transitive dep edges**: v1 emits main-module → direct deps; transitive components surface but their inter-package edges are deferred to v1.1.
- **`composer.json::autoload.psr-4` / `psr-0`** namespace emission: spec Out-of-Scope explicitly.
- **Composer 1 lockfile / installed.json formats**: warn-and-skip on detection (rare in 2026).
- **WordPress plugin/theme via `wp-content/`** (non-Composer-managed): tracked separately as #437.
- **Packagist API enrichment** (license/homepage/author from upstream): explicitly out of scope; lockfile is the sole source of truth.
