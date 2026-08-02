# Quickstart — Scanning a Pants JVM repo with waybill

**Feature**: 224-pants-coursier-jvm
**Audience**: platform teams running waybill against Pants JVM
monorepos (Java, Scala, Kotlin), compliance stakeholders auditing
JVM inventory from Pants-built artifacts.

---

## Prerequisites

- waybill built with feature 224 landed (see `waybill --version`)
- A Pants JVM project with at least one coursier lockfile generated
  (`pants generate-lockfiles` has been run at least once)

---

## 1. Basic scan (default lockfile layout)

If your Pants repo puts lockfiles under `3rdparty/jvm/*.lock` (the
default convention), no additional configuration is needed:

```bash
waybill sbom scan \
    --path ~/src/my-pants-jvm-repo \
    --format cyclonedx-json \
    --output my-repo.cdx.json
```

waybill discovers every `.lock` file under `3rdparty/jvm/` that
carries the `# --- BEGIN PANTS LOCKFILE METADATA` header, parses it
as TOML, and emits one component per locked distribution. Grep for
`"pkg:maven/"` in the output to verify JVM coverage:

```bash
jq '.components[] | select(.purl | startswith("pkg:maven/")) | .name' my-repo.cdx.json | wc -l
```

That count should match the number of locked distributions across
your coursier lockfiles.

## 2. Verify the FR-010 diagnostic

waybill logs one summary line per scan reporting what it found:

```bash
RUST_LOG=info waybill sbom scan --path ~/src/my-pants-jvm-repo --format cyclonedx-json --output out.cdx.json 2>&1 | grep 'pants-coursier-jvm reader complete'
```

Expected output:

```text
INFO waybill::scan_fs::package_db::pants_jvm: pants-coursier-jvm reader complete
  lockfiles_discovered=3
  lockfiles_parsed_ok=3
  lockfiles_skipped_corrupt=0
  lockfiles_skipped_non_pants=0
  components_emitted=147
```

If `lockfiles_discovered=0`: your Pants repo either has no
`3rdparty/jvm/*.lock` files OR uses a custom `pants.toml`
`[jvm.resolves]` path that also doesn't exist. See §3.

If `lockfiles_skipped_non_pants >= 1`: waybill found a `.lock` file
that isn't Pants-generated (missing the metadata header). Common
causes: standalone coursier output committed to the same directory,
or a legacy build system that also uses `.lock` extension. Grep the
INFO log for the specific file paths.

## 3. Custom lockfile path via `pants.toml`

Some Pants repos declare non-default lockfile locations:

```toml
# pants.toml
[jvm]
default_resolve = "prod"

[jvm.resolves]
prod = "build-support/jvm/prod.lock"
junit = "3rdparty/jvm/junit.lock"
scalatest = "3rdparty/jvm/scalatest.lock"
```

waybill honors this automatically — no additional flag needed. The
FR-010 log line reflects all discovered paths:

```text
lockfiles_discovered=3  # picked up prod + junit + scalatest via pants.toml + glob
```

## 4. Multi-resolve repositories

Pants JVM supports multiple named resolves (default + junit +
scalatest + ktlint + etc.). Every discovered lockfile is scanned;
each component carries a `waybill:pants-resolve` annotation naming
its source resolve:

```bash
# Group JVM components by resolve:
jq -r '.components[] |
    select(.purl | startswith("pkg:maven/")) |
    "\((.properties[]? | select(.name == "waybill:pants-resolve") | .value) // "unknown")\t\(.name)"' \
    my-repo.cdx.json | sort | uniq -c | head -20
```

waybill also tags components from known JVM dev-tool resolves
(`scalatest`, `junit`, `testng`, `mockito`, `assertj`, `scalafmt`,
`scalastyle`, `scalafix`, `checkstyle`, `spotbugs`, `pmd`,
`errorprone`, `jacoco`, `dokka`, `ktlint`, `detekt`, plus generics
`lint`, `test`, `dev`, `ci`, `check`, `tools`, `docs`) with
`lifecycle_scope=Development`, so downstream security tooling can
filter them out of production dependency inventories.

Grep for dev-scope components:

```bash
jq '.components[] |
    select(.properties[]? | .name == "waybill:lifecycle-scope" and .value == "development") |
    .purl' my-repo.cdx.json
```

## 5. Coexistence with `pom.xml`

If your repo has BOTH a coursier lockfile AND a `pom.xml` (common in
repos migrating to Pants, or repos that keep a `pom.xml` for IDE
integration), waybill emits each Maven coordinate exactly once. The
coursier lockfile wins because it carries sha256 fingerprints; the
`pom.xml` path is recorded via the existing `waybill:source-files`
annotation for audit.

Verify no duplicates:

```bash
jq '.components[] | select(.purl | startswith("pkg:maven/")) | .purl' \
    my-repo.cdx.json | sort | uniq -d
```

Output should be empty. Any duplicate PURL indicates a dedup bug —
please file an issue with the offending fixture.

## 6. Non-default packaging + classifier PURL qualifiers

Maven artifacts with non-default `packaging` (`war`, `pom`, `aar`)
or non-empty `classifier` (`sources`, `javadoc`, platform-specific
tags like `linux-x86_64`) get PURL qualifiers per purl-spec's `maven`
type:

```bash
jq '.components[] |
    select(.purl | contains("?classifier=") or contains("?type=")) |
    .purl' my-repo.cdx.json
```

Example:
- `pkg:maven/com.example/native@1.0.0?classifier=linux-x86_64&type=so`
- `pkg:maven/com.example/webapp@1.0.0?type=war`

## 7. What if my Pants JVM repo isn't scanned correctly?

**Case A: `lockfiles_discovered=0` but you know your repo uses Pants JVM.**

Verify the lockfile path:

```bash
find ~/src/my-pants-jvm-repo -name '*.lock' -path '*jvm*' -not -path '*/node_modules/*'
```

If lockfiles exist at a non-default path, either:
- Move them to `3rdparty/jvm/*.lock` (Pants's default convention), OR
- Add a `[jvm.resolves]` table entry to your `pants.toml` so
  waybill can discover them.

**Case B: `lockfiles_skipped_non_pants >= 1`.**

The file at that path lacks the `# --- BEGIN PANTS LOCKFILE METADATA`
header. If it IS actually a Pants lockfile (generated by an older
Pants version, perhaps), regenerate with a current `pants
generate-lockfiles`. If it's a standalone coursier lockfile that
shouldn't be scanned by waybill, move it outside the
`3rdparty/jvm/` glob.

**Case C: `lockfiles_skipped_corrupt >= 1`.**

Grep for the WARN diagnostic — waybill names the offending file:

```bash
RUST_LOG=warn waybill sbom scan --path ~/src/my-pants-jvm-repo ... 2>&1 | grep pants-coursier-jvm
```

Common causes:
- `pants generate-lockfiles` was interrupted (partial TOML body).
- Manual edit corrupted the file. Run `pants generate-lockfiles`
  again to regenerate.
- Pants metadata `version` mismatch (e.g., Pants shipped v2 schema
  we don't support yet). File an issue naming the version.

**Case D: waybill's component count is much lower than your `pants peek --filter-target-type=jvm_artifact` output.**

`pants peek` reports design-tier declarations from `BUILD` files;
waybill parses source-tier lockfiles. Discrepancy is expected when
some `jvm_artifact` targets are declared but not actually pulled
in by any `scala_source` / `java_source` (unused declarations).
BUILD-file walker is a follow-up feature.

## What this feature does NOT change

- Repos with no Pants JVM config or coursier lockfiles: SBOM output
  is byte-identical to pre-feature-224 goldens per FR-007 / SC-003.
- The existing Maven reader (`pom.xml`, Gradle, `~/.m2/`, embedded
  `META-INF/maven/`) is unchanged; it runs alongside the new
  pants-coursier-jvm reader without conflict.
- No new CLI flags. No new subcommands.
- No new dependencies (coursier lockfile is TOML; `toml = "0.8"`
  already a workspace dep).
- No new parity-catalog rows or extractors — the m223-shipped
  `waybill:pants-resolve` (C143) + `waybill:source-url` (C144) are
  reused verbatim.
- `waybill trace` (the eBPF path) is untouched.
