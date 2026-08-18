# Per-ecosystem cache-path shapes

**Milestone**: 663

Locked contract per research R3. Concrete example paths + expected PURLs.

## Maven (US1)

**Cache root**: `env::var_os("M2_HOME").map(|p| p.join("repository"))` else `dirs::home_dir()?.join(".m2/repository")`.

**Path shape**: `<root>/g1/g2/.../ga/version/artifact-version.<ext>` where `<ext>` ∈ `{jar, pom, war, aar}`.

**Extraction**:
- Path segments after `<root>` are: `[g1, g2, ..., gn, artifact, version, filename]`.
- `groupId = g1.g2.....gn` (dot-joined).
- `artifactId = <artifact>` (penultimate-parent).
- `version = <version>` (parent).
- PURL: `pkg:maven/<groupId>/<artifactId>@<version>`.

**Example**:
- Path: `<root>/org/apache/commons/commons-lang3/3.12.0/commons-lang3-3.12.0.jar`
- PURL: `pkg:maven/org.apache.commons/commons-lang3@3.12.0`

## Go (US1)

**Cache root**: `env::var_os("GOMODCACHE")` else `env::var_os("GOPATH").map(|p| p.join("pkg/mod"))` else `dirs::home_dir()?.join("go/pkg/mod")`.

**Path shape**: `<root>/<host>/<user>/<pkg>@<version>/...` where `<version>` matches `v[0-9].*`.

**Extraction**:
- Walk path segments from `<root>`. Find the segment matching `<name>@<version>`.
- `namespace = <host>/<user>/...` (join all pre-`@` segments).
- `name = <pkg>` (segment left of `@`).
- `version = v<version>`.
- PURL: `pkg:golang/<namespace>/<name>@<version>`.

**Example**:
- Path: `<root>/github.com/user/pkg@v1.2.3/main.go`
- PURL: `pkg:golang/github.com/user/pkg@v1.2.3`

## Cargo (US2)

**Cache root**: `env::var_os("CARGO_HOME").map(|p| p.join("registry"))` else `dirs::home_dir()?.join(".cargo/registry")`.

**Path shape (two variants)**:

- Variant A: `<root>/cache/<registry-hash>/<name>-<version>.crate`
- Variant B: `<root>/src/<registry-hash>/<name>-<version>/`

**Extraction**:
- Filename stem (or last dir segment for variant B) matches `<name>-<version>`.
- Split on the LAST `-` (cargo package names never contain trailing `-` before a version): everything before = `name`, everything after = `version`.
- PURL: `pkg:cargo/<name>@<version>`.

**Example**:
- Path: `<root>/cache/github.com-1ecc6299db9ec823/serde-1.0.100.crate`
- PURL: `pkg:cargo/serde@1.0.100`

**Edge case**: crate names with hyphens (e.g., `serde-json-1.0.100`). Split on the LAST `-` before a segment matching semver — regex: `-(\d+\.\d+\.\d+.*)$`.

## RubyGems (US2)

**Cache root** (two variants):

- Variant A (user-level cache): `env::var_os("GEM_HOME").map(|p| p.join("specs/rubygems.org%443"))` else `dirs::home_dir()?.join(".gem/specs/rubygems.org%443")`.
- Variant B (Bundler): any path containing segment `vendor/bundle/ruby/<x>/gems/`.

**Path shape**:
- Variant A: `<root>/<name>-<version>.gemspec`
- Variant B: `<...>/gems/<name>-<version>/`

**Extraction**:
- Same `<name>-<version>` split on last `-` before semver (mirrors Cargo).
- PURL: `pkg:gem/<name>@<version>`.

**Example**:
- Path: `<home>/.gem/specs/rubygems.org%443/waybill-fixture-gem-1.2.3.gemspec`
- PURL: `pkg:gem/waybill-fixture-gem@1.2.3`

## npm / pnpm (US3)

**Cache root** (two variants):

- Variant A (pnpm store): `env::var_os("PNPM_STORE_DIR")` else `dirs::home_dir()?.join(".local/share/pnpm/store")`. Content-addressed under a nested `v3/files/<hash>/` layout — this variant is DEFERRED to a follow-on because the coord isn't derivable from the path structure alone (requires a `.package-lock.json` cross-reference).
- Variant B (node_modules): any path matching `**/node_modules/<name>/package.json` OR `**/node_modules/@<scope>/<name>/package.json`.

**Extraction** (variant B only for MVP):
- `name` = the path segment(s) between `node_modules/` and `package.json`. Handle `@scope/name` compound.
- `version` = bounded metadata read on `package.json`, extract `"version"` string field. Per Q1: decline on read/parse failure.
- PURL: `pkg:npm/<name>@<version>` or `pkg:npm/%40<scope>/<name>@<version>` (scope-encoded per PURL spec).

**Example**:
- Path: `/proj/node_modules/waybill-fixture-npm/package.json`
- `package.json` contains `{"version": "1.0.0"}`
- PURL: `pkg:npm/waybill-fixture-npm@1.0.0`

**Deferred**: pnpm content-addressed store paths. Requires PR follow-on.

## Python (US3)

**Cache root** (two variants):

- Variant A (dist-info): any path ending in `<name>-<version>.dist-info/METADATA`.
- Variant B (wheel cache): `env::var_os("PIP_CACHE_DIR").map(|p| p.join("wheels"))` else `dirs::home_dir()?.join(".cache/pip/wheels")`. Path ends in `<name>-<version>-py3-none-any.whl` (or similar).

**Extraction**:
- Variant A: split the `.dist-info` parent-dir name on `-`. Cross-check with `METADATA`'s `Version:` header for authoritative version.
- Variant B: filename stem `<name>-<version>-<pyver>-<abi>-<platform>` — extract `<name>` and `<version>`.
- Both variants: PURL: `pkg:pypi/<name>@<version>` (name normalized to lowercase-hyphens per PyPI convention).

**Example**:
- Path: `<home>/.local/lib/python3.11/site-packages/waybill_fixture_pip-1.0.0.dist-info/METADATA`
- METADATA contains `Version: 1.0.0`
- PURL: `pkg:pypi/waybill-fixture-pip@1.0.0`

**Q1 decline**: if `METADATA` is unreadable OR missing `Version:` header for variant A, log warn + decline. Variant B (wheel filename) doesn't need a metadata read; if the filename stem doesn't split cleanly, decline.

## Verification

Every locked shape has:

- A per-ecosystem unit test with a synthetic fixture directory tree matching the shape.
- A cross-ecosystem integration test at `waybill-cli/tests/cache_probe_universal.rs` producing 6 components from one attestation.
- A parity extractor test verifying the `waybill:resolver-tier` annotation flows through CDX / SPDX 2.3 / SPDX 3.
