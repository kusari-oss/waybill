# Quickstart — Kotlin + Swift Ecosystem Readers

**Feature**: 122-kotlin-swift-readers
**Audience**: operators scanning Kotlin / Android / Swift / KMP projects post-merge.

## Operator workflows

### Scan a Swift Package Manager project

Pre-condition: the project root has a `Package.swift` AND a `Package.resolved` (run `swift package resolve` if the lockfile is missing).

```bash
mikebom sbom scan --path /path/to/swift-project --output sbom.cdx.json
```

The emitted CDX will contain one `pkg:swift/<host>/<namespace>/<name>@<version>` component per `pins[]` entry from `Package.resolved`. Commit-pinned entries (no `state.version`) carry the FULL 40-char SHA on the PURL version segment + a `mikebom:source-type = "git"` annotation + a redundant `mikebom:source-revision` annotation for grep convenience.

If the project lacks `Package.resolved` (fresh checkout pre-resolve), mikebom emits a `tracing::warn!` naming the path and emits ZERO Swift components. Run `swift package resolve` first to populate the lockfile.

### Scan an Android / Kotlin project

Pre-condition: the project root has at least one `build.gradle.kts` (the default Android Studio / IntelliJ shape).

```bash
mikebom sbom scan --path /path/to/android-project --output sbom.cdx.json
```

The emitted SBOM contains one `pkg:maven/<group>/<name>@<version>` component per dep declared in any `build.gradle.kts` under the scan tree. Deps using `libs.<alias>` references resolve against `gradle/libs.versions.toml`.

The `build.gradle.kts`-discovered components are `mikebom:sbom-tier = "design"` (gated by `--include-declared-deps`, which is auto-on for `--path` scans). If the project also has a `gradle.lockfile` (milestone-106 territory), the lockfile-locked version wins on tier (`source` or `installed`) and the `build.gradle.kts` discovery is deduplicated away.

Multi-module workspaces (`settings.gradle.kts` with `include(...)`) emit a synthetic workspace-root component `pkg:generic/<rootProject.name>@0.0.0` plus one main-module component per included module.

### Scan a Kotlin Multiplatform polyglot monorepo

Pre-condition: the project root has Gradle (Kotlin DSL) orchestration AND at least one module declares `Package.swift` for iOS-side deps.

```bash
mikebom sbom scan --path /path/to/kmp-monorepo --output sbom.cdx.json
```

The emitted SBOM contains BOTH `pkg:maven/...` components (Android side) AND `pkg:swift/...` components (iOS side) without cross-ecosystem dedup collapse. Components declared via KMP `kotlin { sourceSets { ... } }` blocks carry the `mikebom:kmp-source-set` annotation as a JSON-encoded array of source-set names.

Filter the resulting SBOM to a single target via downstream tooling — `jq` example pulling only Android-side common+Android deps:

```bash
jq '.components[] | select(
    .properties // [] | map(.name == "mikebom:kmp-source-set" and (.value | fromjson | any(.[]; . == "commonMain" or . == "androidMain"))) | any
)' sbom.cdx.json
```

## Negative-test runbook (for contributors)

Each acceptance scenario in the spec has a corresponding negative test that verifies fail-closed behavior. Run these as part of the milestone's integration suite:

| Test name | Setup | Expected behavior |
|---|---|---|
| `swift_malformed_package_resolved_warns_and_continues` | Fixture with a syntactically-broken `Package.resolved` (e.g., trailing comma) | `tracing::warn!` line names the path + parse error; mikebom exits 0; SBOM has ZERO `pkg:swift/...` components from that file |
| `swift_unknown_schema_version_warns_and_continues` | Fixture with `"version": 4` | `tracing::warn!` line names the unknown version; ZERO components from that file |
| `swift_ssh_url_emits_via_ssh_host` | Fixture with `git@gitlab.acme.com:internal/lib.git` | Component PURL is `pkg:swift/gitlab.acme.com/internal/lib@<ver>` (user dropped, host preserved) |
| `swift_commit_pinned_uses_full_sha` | Fixture with a `pins[]` entry missing `state.version` | Component PURL version segment is the FULL 40-char SHA; `mikebom:source-type = "git"` annotation present |
| `kotlin_dsl_unparseable_build_script_warns_and_continues` | Fixture with a `build.gradle.kts` containing only meta-programmed deps (no regex match) | `tracing::debug!` line names the file; ZERO components emitted; OTHER files in the project still emit normally |
| `kotlin_dsl_missing_catalog_alias_warns_and_drops` | Fixture with a `libs.<alias>` reference that doesn't exist in `libs.versions.toml` | `tracing::warn!` line names the alias; THAT specific dep drops; other deps in the same file emit normally |
| `kotlin_dsl_kmp_source_set_aggregates_into_json_array` | Fixture with one dep declared in BOTH `commonMain` and `jvmMain` | Single component with `mikebom:kmp-source-set` value `"[\"commonMain\",\"jvmMain\"]"` (lex-sorted) |
| `kotlin_dsl_workspace_root_emits_pkg_generic` | Fixture with `settings.gradle.kts` declaring `rootProject.name = "my-kmp-lib"` + `include(":app", ":lib")` | Synthetic workspace-root component `pkg:generic/my-kmp-lib@0.0.0` + two module components |
| `kmp_polyglot_no_cross_ecosystem_collapse` | KMP fixture with Android-side `pkg:maven/io.example/lib@1.0.0` AND iOS-side `pkg:swift/github.com/example/lib@1.0.0` declared independently | TWO distinct components emerge (no dedup; PURLs are in different ecosystems) |
| `no_swift_no_kotlin_byte_identical_emission` | Pure cargo project (no Package.resolved, no build.gradle.kts) | Emitted SBOM is byte-identical to pre-feature mikebom build (modulo random `serialNumber` + timestamp). Confirms SC-007. |

## Constitution Principle V audit for C68

When opening the PR, ensure `docs/reference/sbom-format-mapping.md` carries the new C68 row described in `contracts/kmp-source-set-annotation.md`. The Principle V audit narrative MUST cite:

1. The CDX 1.6 native-field gap (`evidence.identity[].methods[]` covers identification, not source-set provenance).
2. The SPDX 2.3 native-field gap (`primaryPackagePurpose` is a taxonomy).
3. The SPDX 3.0.1 native-field gap (no stable source-set field; the evidence-profile model would express this via a future profile).
4. Constitution Principle X (Transparency) as the carve-out justification.
5. The C64 + C67 storage-shape precedent.

The parity-catalog test `every_catalog_row_has_an_extractor` at `mikebom-cli/src/parity/extractors/mod.rs:425` MUST pass on first run — the three extractors (`c68_cdx`, `c68_spdx23`, `c68_spdx3`) MUST register in the table at the same time the C-row is added to the markdown catalog.

## Cross-platform sanity check

Per the milestone-101 Windows CI lane + FR-013 (no `#[cfg(unix)]` gating), the new readers MUST pass:

- `cargo +stable test --workspace` on Linux x86_64
- `cargo +stable test --workspace` on macOS aarch64
- `cargo +stable test --workspace` on Windows x86_64

The fixtures use UTF-8 + LF line endings (NEVER CRLF) so they parse identically across platforms. The integration tests use forward-slash paths in assertions to avoid Windows backslash drift.
