# Research: Unified Optional-Dependency Classification (m179)

**Date**: 2026-07-09
**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Decision 1 — Ecosystem Survey Table

Each row: `ecosystem | reader source | manifest construct for "optional" | lockfile construct for "optional" | verdict for m179`.

Verdict values:
- **Implement in m179 (US N)** — construct exists; wire the reader to populate `LifecycleScope::Optional` + emit the derivation-specific annotation value.
- **Defer** — construct exists but scoping notes make implementation unwise (e.g., requires build-model resolution beyond current reader).
- **No equivalent** — ecosystem has no "optional" concept in the sense of "declared but may not be present in the production build".

### Language / Registry Ecosystems

| Ecosystem | Reader source | Manifest construct | Lockfile construct | Verdict |
|-----------|---------------|-------------------|-------------------|---------|
| **Cargo** (Rust) | `cargo.rs` | `[dependencies]` entry with `optional = true` (activated by `[features]` sections referencing `dep:<name>`) | `Cargo.lock` doesn't carry the optional flag — it lists what WAS resolved for a given feature set | Implement in m179 (US3). Derivation value: `cargo-optional-true`. |
| **npm** | `npm/mod.rs` + `package_lock.rs` | Top-level `optionalDependencies` in `package.json` (installer skips fetch failures) | `package-lock.json` has `packages.<path>.optional: true` | Implement in m179 (US4). Derivation value: `npm-optional-dependencies`. **m178 peer-precedence guard**: `peerDependenciesMeta.<name>.optional = true` marks a PEER-and-optional dep — m178's `PROVIDED_DEPENDENCY_OF` wins; do NOT reclassify to Optional. |
| **Yarn (v1 + Berry)** | `npm/yarn_lock.rs` | Reads `package.json` (same as npm above) | `yarn.lock` v1 carries `optionalDependencies` sections; Berry (v2/3) uses `dependenciesMeta.<name>.optional` | Implement in m179 (US4 sibling). Same derivation value `npm-optional-dependencies` (Yarn wraps npm's manifest). |
| **pnpm** | `npm/pnpm_lock.rs` | Reads `package.json` (same as npm) | `pnpm-lock.yaml` carries `optionalDependencies` block + `optional: true` on package entries | Implement in m179 (US4 sibling). Same derivation value. |
| **pip / PEP 621 (pyproject.toml)** | `pip/pyproject.rs` | `[project.optional-dependencies.<extra>]` section (each key is an extra name) | `poetry.lock` / `uv.lock` / `pdm.lock` carry `extras` markers | Implement in m179 (US5). Derivation value: `pip-extras-require`. **Note**: any dep in `[project.optional-dependencies.<extra>]` is "optional" in the sense that it's only installed when the extra is requested (`pip install foo[dev]`). Maps cleanly to `LifecycleScope::Optional`. |
| **pip / setup.py** | `pip/setup_py.rs` | `extras_require = {"dev": [...]}` in `setuptools.setup(...)` | (same as pyproject) | Implement in m179 (US5 sibling). Same derivation value. |
| **pip / setup.cfg** | `pip/setup_cfg.rs` | `[options.extras_require]` section | (same) | Implement in m179 (US5 sibling). Same derivation value. |
| **uv** | `pip/uv_lock.rs` | Reads pyproject.toml (above) | `uv.lock` carries `optional` markers per-package | Implement in m179 (US5 sibling). Same derivation value. |
| **Poetry** | (part of pip readers) | Reads pyproject.toml `[tool.poetry.dependencies]` with `optional = true` | `poetry.lock` | Implement in m179 (US5 sibling). Same derivation value `pip-extras-require` (poetry uses the extras concept). |
| **Maven** | `maven.rs` | `<dependency>` with `<optional>true</optional>` element (marks dep as not-transitively-propagated) | (no separate lockfile in classic Maven; `dependencies-list.txt` in newer builds) | Implement in m179 (US6). Derivation value: `maven-optional-element`. |
| **Gradle** | `gradle/` reader | `compileOnly`, `annotationProcessor`, and `compileOnlyApi` configurations declare deps that are NOT packaged at runtime | Gradle doesn't have a canonical lockfile shape; some projects use `gradle.lockfile` | Implement in m179 (US6 sibling). Derivation value: `gradle-compile-only`. **Scoping caveat**: pin implementation to `compileOnly` for m179; `annotationProcessor` and other build-only configurations are already covered by m052's `LifecycleScope::Build`. |
| **Gem (Bundler)** | `gem.rs` | `gem "foo", require: false` (does NOT mean "optional" — means "don't autoload"); no true "optional" concept in Gemfile | `Gemfile.lock` no shape for it either | **No equivalent**. Bundler doesn't have "conditionally-included" deps in the same sense; groups (`gem "foo", group: :prod`) are group-filter, not optional-per-scan. Row present for audit completeness. |
| **Composer (PHP)** | `composer.rs` | `require-dev` is dev-scope (already m052 `Development`); no true "optional" — `suggest` is weaker (advisory metadata, no install action) | `composer.lock` | **No equivalent for m179**. `suggest` deps are never installed, so they're not even discovered by the trace; not "optional in the production build" sense. Row present for audit completeness. |
| **CocoaPods** | `cocoapods.rs` | No "optional" concept — `Podfile` has `pod "Foo", :configurations => ['Debug']` which is configuration-scope, not optional-per-build | `Podfile.lock` | **No equivalent for m179**. Configuration-scoped deps are debug/release build-mode filters. Row present for audit completeness. |
| **Elixir (mix)** | `elixir.rs` | `{:foo, "~> 1.0", optional: true}` in `mix.exs` — this DOES exist in Elixir | `mix.lock` | **Defer**. Reader currently does not parse the `optional:` key. Follow-up milestone. |
| **Erlang (rebar)** | `erlang.rs` | `.app.src` has `optional_applications` list (per m141's `mikebom:erlang-app-dep-kind = optional`) | `rebar.lock` | Implement in m179 (US7). Derivation value: `erlang-optional-applications`. **Normalization** — the signal already exists internally via the mikebom annotation; m179 elevates it to native `LifecycleScope::Optional`. |
| **Scala (sbt)** | `scala.rs` | `libraryDependencies` entries with `% "optional"` scope (rare, but valid) | `*.sbt.lock` | **Defer**. Rare in practice; reader currently doesn't parse the scope suffix. Follow-up milestone. |
| **Haskell (cabal + stack)** | `haskell.rs` | Conditional stanza `if flag(foo) build-depends: ...` gates deps on flag activation. Semantically optional. | `cabal.project.freeze` / `stack.yaml.lock` | **Defer**. Complex conditional evaluation would be needed; scope too big for m179. Follow-up milestone. |
| **Dart (pub)** | `dart.rs` | No true "optional" — `dev_dependencies` is dev-scope (already m052) | `pubspec.lock` | **No equivalent for m179**. Row present for audit completeness. |
| **Swift Package Manager** | `swift/` | Conditional target compilation via `condition:` on `.target(...)` deps | `Package.resolved` | **Defer**. Conditional compilation similar to Haskell flags; scope too big for m179. Follow-up milestone. |
| **Kotlin DSL** | `kotlin_dsl/` | Wraps Gradle — same `compileOnly` construct via Gradle Kotlin DSL | Same as Gradle | Covered by US6 (Gradle). |
| **Bazel** | `bazel.rs` | `select({...})` conditional deps + `tags = ["optional"]` on `*_library` rules | `MODULE.bazel.lock` / `WORKSPACE.bazel.lock` | **Defer**. Bazel's `select()` conditional model needs build-model resolution to determine which deps are "actually optional in production"; not implementable via manifest-only reading. Follow-up milestone. |
| **CMake** | `cmake.rs` | `find_package(Foo REQUIRED)` vs. `find_package(Foo QUIET)` — QUIET means "don't fail if missing" but not "optional in production build" | (no lockfile) | **No equivalent for m179**. CMake `QUIET`/`REQUIRED` is a build-configuration signal, not a component-classification signal. Row present for audit completeness. |
| **Conan** | `conan.rs` | `[options] optional_foo=True` in `conanfile.txt`; also `requires` vs. `tool_requires` (tool_requires already ≈ m052 `Build`) | `conan.lock` | **Defer**. Options-based classification is complex to map to a "was this dep production-optional?" question. Follow-up milestone. |
| **vcpkg** | `vcpkg.rs` | Feature-based `vcpkg.json` `features` map (features can be conditionally activated) | (no dedicated lockfile in classic mode) | **Defer**. Same complexity as Conan features. Follow-up milestone. |
| **west (Zephyr)** | `west/` | No true "optional" — `west.yml` groups map to build modes | (no lockfile) | **No equivalent for m179**. Row present for audit completeness. |
| **Go** | `golang/` + m112 | `go.mod` doesn't have "optional"; m112's `go mod why` = NotNeeded signal is the closest analog (test-only-transitive detection) | `go.sum` | **Special case**: US1 flagship. Route `build_inclusion = NotNeeded` (when `lifecycle_scope = None`) → SPDX `TEST_DEPENDENCY_OF` per user's Q1 answer. NOT a new `LifecycleScope::Optional` classification. |
| **NuGet** | `nuget/` | `<PackageReference>` with `PrivateAssets="all"` (build-only, not runtime) — closer to m052 `Build` than "optional" | `packages.lock.json` | **No equivalent for m179**. `PrivateAssets="all"` maps to m052 `Build`, not optional. Row present for audit completeness. |

### OS Package Ecosystems

| Ecosystem | Reader source | Optional construct | Verdict |
|-----------|---------------|-------------------|---------|
| **Homebrew** | `brew.rs` | No "optional" — casks/formulae have `depends_on :optional` in classic Ruby DSL, but Homebrew 4.0+ deprecates it and current formulae rarely use it | **No equivalent for m179**. Deprecated construct. Row present for audit completeness. |
| **alpm (Arch)** | `alpm.rs` | `optdepends` field in package `desc` file | **Defer**. Reader currently doesn't parse `optdepends`; would be a new emission path. Follow-up milestone. |
| **dpkg (Debian)** | `dpkg.rs` | `Recommends:` and `Suggests:` fields in `control` (deb weakness relationships) | **No equivalent for m179**. Debian's Recommends/Suggests are weaker than "optional in the sense of possibly-installed"; they're auto-installed by default under `apt`. Row present for audit completeness. |
| **apk (Alpine)** | `apk.rs` | No "optional" — `depends` field is authoritative | **No equivalent for m179**. Row present for audit completeness. |
| **rpm** | `rpm.rs` / `rpm_file.rs` | `Recommends:` (weak dep, similar to Debian) | **No equivalent for m179**. Row present for audit completeness. |
| **ipk (OpenWrt)** | `ipk_file.rs` | No "optional" | **No equivalent for m179**. |
| **opkg** | `opkg.rs` | Same as ipk | **No equivalent for m179**. |
| **Yocto (recipes)** | `yocto/` | `RRECOMMENDS_${PN}` recommendation dependencies | **No equivalent for m179**. Same rationale as dpkg. |

### Survey Summary

- **Implement in m179 (5 user stories, 6 readers)**: Cargo (US3), npm/yarn/pnpm (US4 x3 lockfile parsers), pip/setup.py/setup.cfg/uv/Poetry (US5 x4 readers), Maven + Gradle (US6 x2), Erlang normalization (US7).
- **Defer to future milestones (7 ecosystems)**: Elixir, Scala, Haskell, Swift, Bazel, Conan, vcpkg, alpm.
- **No equivalent (11 ecosystems)**: Gem, Composer, CocoaPods, Dart, CMake, west, NuGet, Homebrew, dpkg, apk, rpm, ipk, opkg, Yocto.
- **Special case (1 ecosystem)**: Go — routes through the `build_inclusion = NotNeeded` path (US1 flagship), not `LifecycleScope::Optional`.

The survey satisfies SC-007's coverage gate — every ecosystem in mikebom's reader dispatch has a row and a verdict.

## Decision 2 — Precedence Rules for Multi-Signal Components

The classifier at `mikebom-cli/src/scan_fs/mod.rs:1261` (`apply_lifecycle_scope_to_edges`) MUST check the target component's fields in this exact order and return the first match:

| Priority | Target signal | Internal `RelationshipType` | SPDX 2.3 (Full mode) | SPDX 2.3 (Basic mode) |
|----------|--------------|----------------------------|---------------------|----------------------|
| 1 | `lifecycle_scope = Some(Test)` | `TestDependsOn` | `TEST_DEPENDENCY_OF` (reversed) | `DEPENDS_ON` (natural) |
| 2 | `lifecycle_scope = Some(Development)` | `DevDependsOn` | `DEV_DEPENDENCY_OF` (reversed) | `DEPENDS_ON` (natural) |
| 3 | `lifecycle_scope = Some(Build)` | `BuildDependsOn` | `BUILD_DEPENDENCY_OF` (reversed) | `DEPENDS_ON` (natural) |
| 4 | `lifecycle_scope = Some(Optional)` (NEW) | `OptionalDependsOn` (NEW) | `OPTIONAL_DEPENDENCY_OF` (reversed) (NEW) | `DEPENDS_ON` (natural) |
| 5 | `build_inclusion = Some(NotNeeded)` AND `lifecycle_scope = None` (NEW) | `TestDependsOn` (mapped; US1 flagship) | `TEST_DEPENDENCY_OF` (reversed) | `DEPENDS_ON` (natural) |
| 6 | (all else) | `DependsOn` (unchanged) | `DEPENDS_ON` (natural) | `DEPENDS_ON` (natural) |

**Precedence rationale**:

- Rows 1-3 preserve m052's existing behavior byte-identically. No m052-touched fixture drifts.
- Row 4 (NEW `LifecycleScope::Optional`) sits BEFORE Row 5 (m112 `NotNeeded`). Rationale: a manifest declaration of `optional = true` is a more authoritative classification than a build-graph inference. If both are set on the same target, the manifest wins (FR-014).
- Row 5 handles the pico flagship case: Go's m112 `go mod why` = NotNeeded transitive gets emitted as `TEST_DEPENDENCY_OF`, since m112 doesn't currently carry a "why NotNeeded" reason code (per spec.md Assumption 8). This is deliberate semantic overloading of `TEST_DEPENDENCY_OF` — a follow-up milestone MAY refine it once m112 carries granular reason codes.
- Row 5's guard requires `lifecycle_scope = None`. If both `lifecycle_scope = Some(Test)` AND `build_inclusion = Some(NotNeeded)` are set (should not happen per m112's "never downgrade an existing test tag" invariant, but defense-in-depth), Row 1 wins (m052 semantic).

**The `--spdx2-relationship-compat=basic` column** confirms that Rows 1-5 ALL collapse to natural-direction `DEPENDS_ON` in basic mode, honoring m228's contract per FR-003 + SC-006.

## Decision 3 — Derivation Annotation Emission Sites

The new `mikebom:optional-derivation` annotation MUST round-trip through all three format emitters with byte-identical value per FR-019 + SC-008. Per m112's `mikebom:build-inclusion-derivation` precedent, the annotation is a **component-level** annotation (not document-level, not relationship-level).

**CDX 1.6** (`mikebom-cli/src/generate/cyclonedx/builder.rs`):
- Emit as `component.properties[]` entry: `{"name": "mikebom:optional-derivation", "value": "cargo-optional-true"}`.
- Site: the block right after m112's `mikebom:build-inclusion` property emission at `builder.rs:842-857`.

**SPDX 2.3** (`mikebom-cli/src/generate/spdx/annotations.rs`):
- Emit as a `Package.annotations[]` entry with the `MikebomAnnotationCommentV1` envelope shape (matching m147's peer-edge-targets and m112's build-inclusion-derivation):
  ```json
  {
    "annotationDate": "<scan-emission-time>",
    "annotationType": "OTHER",
    "annotator": "Tool: mikebom",
    "annotationComment": "{\"schema\":\"mikebom.annotation.v1\",\"name\":\"mikebom:optional-derivation\",\"value\":\"cargo-optional-true\"}"
  }
  ```
- Site: the annotation loop at `spdx/annotations.rs:243+` (m112's `mikebom:build-inclusion-derivation` emission).

**SPDX 3.0.1** (`mikebom-cli/src/generate/spdx/v3_annotations.rs`):
- Emit as a `spdx:Annotation` node with `spdx:statement` payload wrapping the same `MikebomAnnotationCommentV1` envelope:
  ```json
  {
    "type": "Annotation",
    "spdxId": "spdx:<derived>",
    "subject": "spdx:<package-spdx-id>",
    "annotationType": "other",
    "statement": "{\"schema\":\"mikebom.annotation.v1\",\"name\":\"mikebom:optional-derivation\",\"value\":\"cargo-optional-true\"}",
    "creationInfo": "...",
    "created_by": ["<mikebom-tool-agent-node>"]
  }
  ```
- Site: `v3_annotations.rs:250+` (m112's `mikebom:build-inclusion-derivation` emission).

**Parity extractor** (`mikebom-cli/src/parity/extractors/`):
- Register `mikebom:optional-derivation` in the catalog as `Directionality::SymmetricEqual` (byte-identity across all three formats).
- Colocate with the existing `mikebom:build-inclusion-derivation` catalog row.

## Decision 4 — Delivery Cadence

Per plan.md's Phase 0 recommendation:

- **m179 (this milestone)**: US1 (pico Go fix) + US2 (research artifact — THIS file + spec) + US3 (Cargo) + core-model change + one SPDX 2.3 emitter arm. Estimated 15-20 tasks.
- **m180**: US4 (npm/yarn/pnpm — biggest per-unit user impact after Cargo). Estimated 8-10 tasks.
- **m181**: US5 (pip/pyproject/setup.py/setup.cfg/uv/Poetry). Estimated 6-8 tasks.
- **m182**: US6 (Maven + Gradle `compileOnly`). Estimated 6-8 tasks.
- **m183**: US7 (Erlang normalization). Estimated 3-5 tasks. Small — the signal already exists internally.

Alternative delivery (single-PR bundle): if `/speckit-tasks` finds the per-ecosystem work is smaller than estimated, the whole thing MAY ship as one m179 PR. Left as a `/speckit-tasks`-phase decision.

**Rationale for the split**: (a) US1 closes the reported pico bug — the flagship user-facing fix — with the smallest possible change surface. (b) US2 delivers the design foundation. (c) US3 (Cargo) is the second-most-scanned ecosystem after Go and has the cleanest test signal (well-known feature-flag pattern, easy fixture). Delivering US1+US3 together validates the core-model design against two ecosystems (Go via NotNeeded path + Cargo via Optional path) before extending to more. (d) Each subsequent milestone (m180-m183) is a per-ecosystem-family delivery that can regress-test independently.

## Alternatives Considered (Not Adopted)

- **Alternative 1** — Ship all 7 user stories as one m179 PR. Rejected because the ecosystem-coverage scope is large enough that a single-PR bundle would sit in review for weeks; incremental delivery lets each user-story slice ship + get feedback separately.
- **Alternative 2** — Split the core-model change into its own PR before touching any reader. Considered but rejected: the enum extensions are tiny (~5 lines total), and shipping them without an initial concrete reader consumer leaves the code dead weight in the tree. Bundling US1 (which uses the new dispatch table without touching the new enum) + US3 (which populates the new enum from Cargo) means m179 exercises both the classifier extension AND the enum extension in one PR.
- **Alternative 3** — Use `is_optional: bool` on ResolvedComponent instead of extending the enum (Q1 Option B). Rejected via Q1 answer: the enum extension has better ergonomics with the existing `is_non_runtime()` helper + m052 precedence table.
- **Alternative 4** — Emit BOTH `TEST_DEPENDENCY_OF` and `OPTIONAL_DEPENDENCY_OF` for m112 NotNeeded (hybrid), waiting on m112 to grow a reason code. Rejected as out-of-scope for m179 — the reported user bug can be closed with a single-type emission today, and hybrid classification is a legitimate follow-up milestone once m112 carries granular reasons.

## Open Questions

None. Q1 and Q2 are answered. Delivery cadence is a recommendation, not a hard constraint — `/speckit-tasks` may adjust.
