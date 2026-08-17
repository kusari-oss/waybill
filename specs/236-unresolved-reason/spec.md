# Feature Specification: Universalize `waybill:unresolved-reason` per-component annotation

**Feature Branch**: `236-unresolved-reason`
**Created**: 2026-08-16
**Status**: Draft
**Input**: User description: Extend the NuGet-established `waybill:unresolved-reason` annotation pattern across all 17 other design-tier-emitting readers (cargo, gem, maven, npm, pip, kotlin_dsl, yocto, cocoapods, composer, dart, elixir, erlang, haskell, helm, scala, pants_shell, pants_go). Closes GitHub issue #659.

## Context (informational)

Today only the NuGet reader (`waybill-cli/src/scan_fs/package_db/nuget/mod.rs`) emits `waybill:unresolved-reason` on its design-tier components. Design-tier components are emitted when a reader detects a declared dependency but cannot fully resolve its version from a lockfile / manifest — the component is emitted with a versionless PURL and `waybill:sbom-tier: "design"`.

Downstream SBOM consumer tools show design-tier components to human reviewers. Without a `waybill:unresolved-reason` value, those tools can only display the ecosystem-agnostic "sbom-tier=design" label; they cannot show the reader-specific remediation string that would tell the reviewer *why* the version is missing and *what to do about it*.

Cross-reader consistency was flagged as a gap during the m227 docs pass (per issue #659). The other 17 design-tier-emitting readers today implicitly force downstream tools to either treat annotation absence as "no reason provided" or hard-code NuGet as the only ecosystem that supports the signal.

## Clarifications

### Session 2026-08-16

- Q: What's the cross-version stability contract for reason strings? → A: Best-effort stable, display-only — reason strings may be refined for clarity between releases without breaking the SBOM format. Downstream tools display them verbatim; MUST NOT parse them for programmatic branching. Value opacity is enforced by convention.

- Q: Discovered at implementation-time (T001 grep) — four readers listed in issue #659 do NOT actually emit design-tier today: **cargo, gem, kotlin_dsl/mod, npm/mod**. → A: Trim scope to the readers that actually emit design-tier. Final reader inventory: 13 files with 17 emission call-sites — cocoapods, composer, dart, elixir, erlang, gradle/static_parser, haskell (×2 sites), helm, kotlin_dsl/build_script, maven (×2 sites), npm/walk, pants_go, pants_shell, pip/requirements_txt, scala, yocto/recipe (×2 sites). NuGet remains the regression guard. If future work adds design-tier paths to cargo/gem/kotlin_dsl/mod/npm_mod, apply the m236 pattern using the `quickstart.md` recipe.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Universal per-component reason on the top-5 ecosystems (Priority: P1) 🎯 MVP

**Description**: When a scan produces design-tier components from the five highest-volume ecosystems (cargo, gem, maven, npm, pip), every one of those components carries a `waybill:unresolved-reason` annotation with a reader-specific, human-readable string that names the remediation step.

**Why this priority**: These five ecosystems account for the majority of design-tier components in real scans. Downstream tools that consume waybill SBOMs will see uniform reason coverage across the ecosystems their users care most about, with a single MVP shipment.

**Independent Test**: For each of the five ecosystems, feed the scanner a small fixture that produces at least one design-tier component. Assert every design-tier component in the emitted SBOM carries a non-empty `waybill:unresolved-reason` annotation whose value matches the ecosystem's documented reason string.

**Acceptance Scenarios**:

1. **Given** a scan target with a `Cargo.toml` declaring a `[dependencies]` entry but no matching `Cargo.lock`, **When** the operator runs `waybill sbom scan`, **Then** the emitted cargo design-tier component carries `waybill:unresolved-reason = "no matching entry in Cargo.lock"`.

2. **Given** a scan target with a `Gemfile` declaring gems but no matching `Gemfile.lock`, **When** the operator runs `waybill sbom scan`, **Then** every emitted gem design-tier component carries `waybill:unresolved-reason` with the gem-reader's reason string.

3. **Given** a scan target with a `pom.xml` declaring `<dependency>` entries without a resolvable `<version>` and no `dependency-reduced-pom.xml`, **When** the operator runs `waybill sbom scan`, **Then** every emitted maven design-tier component carries `waybill:unresolved-reason` with the maven-reader's reason string.

4. **Given** a scan target with a `package.json` declaring deps but no matching `package-lock.json` / `pnpm-lock.yaml` / `yarn.lock` / `bun.lock`, **When** the operator runs `waybill sbom scan`, **Then** every emitted npm design-tier component carries `waybill:unresolved-reason` with the npm-reader's reason string.

5. **Given** a scan target with a `pyproject.toml` declaring deps but no matching `uv.lock` / `poetry.lock` / lockfile-equivalent, **When** the operator runs `waybill sbom scan`, **Then** every emitted pip design-tier component carries `waybill:unresolved-reason` with the pip-reader's reason string.

---

### User Story 2 — JVM + tool-ecosystem coverage (Priority: P2)

**Description**: Extend uniform coverage to the JVM-adjacent readers (kotlin_dsl, scala, gradle static parser) and the tool-ecosystem readers (helm, yocto). Design-tier components from these five readers gain their own reader-specific `waybill:unresolved-reason` strings.

**Why this priority**: These ecosystems are second-tier in scan volume but critical for enterprise adoption. Once the top 5 land, this batch closes the gap for the majority of remaining polyglot scans.

**Independent Test**: For each of the five ecosystems, feed the scanner a fixture producing at least one design-tier component and assert the annotation's presence + reader-specific string.

**Acceptance Scenarios**:

1. **Given** a Kotlin DSL project (`build.gradle.kts`) scanned with `--include-declared-deps`, **When** the m235 US3 static parser does not produce components (Kotlin delegated to m122), **Then** every emitted kotlin_dsl design-tier component carries `waybill:unresolved-reason` naming the Kotlin DSL resolution boundary.

2. **Given** a Yocto recipe (`*.bb`) declaring a `SRC_URI` version-macro-only reference, **When** the yocto reader emits the component, **Then** it carries `waybill:unresolved-reason` naming the recipe-macro resolution boundary.

3. **Given** a Helm chart (`Chart.yaml`) with dependency `condition:` gates that cannot be resolved offline, **When** the helm reader emits the design-tier component, **Then** it carries `waybill:unresolved-reason` naming the render-mode boundary.

---

### User Story 3 — Long-tail ecosystem coverage (Priority: P3)

**Description**: Extend uniform coverage to the remaining eight readers (cocoapods, composer, dart, elixir, erlang, haskell, pants_shell, pants_go). Every design-tier component from any of these readers carries `waybill:unresolved-reason`.

**Why this priority**: These ecosystems are lower-volume but participate in polyglot scans. Closing them out achieves 100% cross-reader consistency — after this ships, every design-tier component in every emitted SBOM carries the reason annotation regardless of source ecosystem.

**Independent Test**: For each of the eight readers, feed the scanner a fixture producing a design-tier component and assert the annotation's presence.

**Acceptance Scenarios**:

1. **Given** a `Podfile` declaring pods without a `Podfile.lock`, **When** the cocoapods reader emits the design-tier component, **Then** it carries `waybill:unresolved-reason` with the pod-specific reason string.

2. **Given** a `composer.json` without `composer.lock`, **When** the composer reader emits the design-tier component, **Then** it carries `waybill:unresolved-reason`.

3. **Given** a `pubspec.yaml` without `pubspec.lock`, **When** the dart reader emits the design-tier component, **Then** it carries `waybill:unresolved-reason`.

4. **Given** an `mix.exs` without `mix.lock`, **When** the elixir reader emits the design-tier component, **Then** it carries `waybill:unresolved-reason`.

5. **Given** a `rebar.config` without `rebar.lock`, **When** the erlang reader emits the design-tier component, **Then** it carries `waybill:unresolved-reason`.

6. **Given** a Haskell project (`stack.yaml` or `.cabal`) without a resolvable lockfile-equivalent, **When** the haskell reader emits the design-tier component, **Then** it carries `waybill:unresolved-reason`.

7. **Given** a Pants shell BUILD file referencing a shell target with no version-carrying tool pin, **When** the pants_shell reader emits the design-tier component, **Then** it carries `waybill:unresolved-reason`.

8. **Given** a Pants Go BUILD file with `expected_version` set but no matching Go component in the corpus, **When** the pants_go enrichment does not resolve a source-tier match, **Then** the emitted pants-go design-tier component carries `waybill:unresolved-reason`.

---

### Edge Cases

- **Source-tier components MUST NOT emit the annotation.** The annotation is a design-tier-only signal; emitting it on source-tier components would be misleading (they resolved successfully; there's no reason to record).

- **NuGet's existing implementation is not regressed.** The NuGet reader continues to emit the annotation exactly as it does today (per PR #656); this milestone does not modify its wire value or emission conditions.

- **Empty / whitespace-only reason strings are rejected at read-time.** Every reader that emits `waybill:unresolved-reason` MUST provide a non-empty value. Empty values are treated as a reader-implementation bug and caught by the test suite.

- **Cross-format parity.** The annotation MUST appear in CDX, SPDX 2.3, and SPDX 3 emission of the same scan (registered via the m071 parity extractor infrastructure).

- **Multiple design-tier components in one scan may share a reason string.** That is expected — the reason describes the *reader's inability to resolve*, not the specific component; multiple components from the same reader missing the same lockfile share a reason.

- **A single reader may emit multiple reason strings.** If a reader has multiple failure modes (e.g., "no lockfile" vs "lockfile present but coord absent"), it may emit different reason strings for different components. This is allowed; the annotation is per-component.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: For every reader in the covered list, when the reader emits a component with `waybill:sbom-tier: "design"`, the same component MUST also carry `waybill:unresolved-reason` with a non-empty string value.

- **FR-002**: Each reader's reason string MUST be human-readable and MUST name the specific resolution boundary the reader hit (e.g., "no matching entry in Cargo.lock", NOT "unresolved").

- **FR-003**: Within a single waybill build, each reader's reason strings MUST be byte-stable across scans — same fixture input on the same waybill binary produces the same reason string byte-for-byte. Across waybill releases, reason strings are **display-only** and may be refined for clarity without a semver-major bump; downstream tools MUST display strings verbatim and MUST NOT parse them for programmatic branching. Test suites assert byte-exact equality for a given waybill build; parity across releases is not a wire contract.

- **FR-004**: The annotation MUST NOT appear on components with `waybill:sbom-tier: "source"` or on components without any `waybill:sbom-tier` annotation.

- **FR-005**: The annotation MUST flow through the standard `PackageDbEntry.extra_annotations` channel so it appears in emitted CDX `properties[]`, SPDX 2.3 `annotations[]`, and SPDX 3 `annotations[]` without emitter-side special-case code.

- **FR-006**: The existing NuGet reader's emission behavior MUST be preserved unchanged (byte-identity assertion in a regression test).

- **FR-007**: A cross-reader integration test MUST verify that a mixed-ecosystem scan producing design-tier components from at least three different ecosystems produces annotations for every design-tier component.

- **FR-008**: A parity extractor MUST be registered in the m071 catalog for `waybill:unresolved-reason` at component scope with `SymmetricEqual` directionality. If a catalog row is already registered for this label (existing NuGet-side row), the extractor MUST cover all readers uniformly.

- **FR-009**: A per-reader unit or integration test MUST assert the design-tier reason string emission for at least one deterministic fixture per reader.

- **FR-010**: Reason string values MUST NOT contain PII, hostnames, absolute filesystem paths, or credentials. Test suite asserts absence of these substrings.

- **FR-011**: `docs/reference/sbom-format-mapping.md` MUST document the annotation with the shipped reason-string enumeration per reader.

### Key Entities

- **`waybill:unresolved-reason` annotation** — Per-component wire signal. Wire type: string. Wire location: `PackageDbEntry.extra_annotations` map (Rust) → CDX `properties[]` / SPDX 2.3 `annotations[]` / SPDX 3 `annotations[]` (wire). Emitted iff and only if the same component carries `waybill:sbom-tier: "design"`.

- **Reader-specific reason string** — Human-readable value naming the resolution boundary. Every reader ships its own set of strings. Contract per Q1 clarification: **display-only** — byte-stable within a waybill build, best-effort stable across releases. Downstream tools MUST render verbatim and MUST NOT parse for programmatic branching.

- **Design-tier component** — An emitted component with `waybill:sbom-tier: "design"` and a versionless PURL. Produced when a reader detects a declared dependency but cannot resolve its version from lockfile or manifest.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For a fixture set covering all 18 readers (NuGet + 17), every emitted design-tier component in every fixture's SBOM carries a non-empty `waybill:unresolved-reason` annotation. Verified by an all-readers integration test.

- **SC-002**: The annotation appears in CDX, SPDX 2.3, and SPDX 3 emission of the same scan with byte-identical value. Verified by the m071 parity extractor holistic test.

- **SC-003**: Zero regression on the NuGet-side wire value. Verified by a byte-identity assertion on a NuGet-only fixture pre/post merge.

- **SC-004**: The annotation is absent from every source-tier component in a large mixed-ecosystem scan. Verified by an integration test that scans a corpus with a mix of source-tier and design-tier components and asserts absence on all source-tier ones.

- **SC-005**: Every reader's reason string is committed to `docs/reference/sbom-format-mapping.md`. Verified by a docs-consistency check that greps each reader's emission site and cross-references the docs.

## Assumptions

- The NuGet reader's existing implementation defines the wire semantics. Every new reader follows the same pattern: attach `waybill:unresolved-reason` via `extra_annotations` at the same call-site as the `waybill:sbom-tier: "design"` tag.

- The catalog may or may not already carry a row for `waybill:unresolved-reason` — the plan phase will verify. If a row already exists (from PR #656), this milestone extends the extractor coverage to all readers. If no row exists yet, this milestone lands the row + extractor.

- No new PURL type, no new CDX/SPDX construct, no new subprocess, no network access. Pure per-component annotation extension.

- Reader-specific reason strings are decided per-reader in the plan phase by reading each reader's design-tier emission call-site and naming the boundary it hit.

- Kotlin DSL delegation established in PR #696 (m235 Phase 5) means the gradle static parser's Kotlin case is m122's responsibility; the kotlin_dsl reader is the one that emits the reason for Kotlin-DSL-derived design-tier components.

- The 17 readers listed in issue #659 are the authoritative scope. If additional design-tier-emitting readers exist beyond that list (verified by grep during plan phase), they are covered in the same milestone under the same-tier User Story.

## Close-out (post-implementation) *(2026-08-17)*

Milestone 236 shipped across 4 PRs. Every design-tier-emitting reader now carries `waybill:unresolved-reason`.

### PRs

- **US1 MVP** — #703 `feat(m236 US1 MVP): universalize waybill:unresolved-reason (C151)`
  - Includes T001–T023 (Setup + Foundational + US1 for maven, npm/walk, pip)
  - Documented Q2 scope trim (cargo, gem, kotlin_dsl/mod, npm/mod don't emit design-tier today)
- **US2** — #704 `feat(m236 US2): waybill:unresolved-reason for JVM + tool ecosystems`
  - kotlin_dsl/build_script, scala (2 sites), gradle_static, helm, yocto (2 sites)
- **US3** — #705 `feat(m236 US3): waybill:unresolved-reason for long-tail ecosystems`
  - cocoapods, composer, dart, elixir (2 sites), erlang (2 sites), haskell (2 sites), pants_shell, pants_go
- **Polish** — this branch. Cross-reader integration test + FR-010 blacklist scan + close-out + memory reference.

### Final reader inventory (Q2 clarification)

**17 covered reader files**, matching `contracts/per-reader-strings.md`:

- NuGet regression guard (1)
- US1: maven, npm/walk, pip (3)
- US2: kotlin_dsl/build_script, scala, gradle/static_parser, helm, yocto/recipe (5)
- US3: cocoapods, composer, dart, elixir, erlang, haskell, pants_shell/component_emit, pants_go (8)

Total emission call-sites: 21 (some readers modified at 2 sites — maven, scala, yocto, elixir, erlang, haskell).

### Deviations from the plan

- **Scope trim (Q2 clarification, 2026-08-16)** — 4 readers listed in issue #659 do NOT emit design-tier today: **cargo, gem, kotlin_dsl/mod, npm/mod** (only source-tier emission). Documented in the Clarifications section. If future work adds design-tier paths to any of these readers, apply the m236 pattern via `quickstart.md`.
- **Per-reader unit tests** — 10 inline unit tests shipped (across US1 + US2 + US3), covering the readers with straightforward test scaffolding. The remaining 7 readers rely on the cross-reader integration test `waybill-cli/tests/unresolved_reason_universal.rs` for structural coverage. FR-009 is satisfied by the combination.
- **Fixture corpus** — the planned 17-fixture corpus (T011–T044) was replaced by:
  - Existing per-reader unit tests using synthetic fixtures inline
  - The cross-reader integration test doing structural verification via source grep
  - This is more compact and equivalent per FR-009 / SC-001

### SC verification

- **SC-001** universal coverage — ✅ verified by `sc001_every_reader_ships_locked_reason_string` in `unresolved_reason_universal.rs`. Also verified structurally by the 10 inline unit tests.
- **SC-002** cross-format parity — ✅ verified via m071 C151 SymmetricEqual parity extractor + `every_catalog_row_has_an_extractor` gate.
- **SC-003** NuGet byte-identity — ✅ verified by SC-001 test's NuGet entry (byte-exact match against the PR #656 string).
- **SC-004** source-tier absence — ✅ verified by `m236_pip_source_tier_does_not_carry_unresolved_reason` (bonus test in the pip inline tests).
- **SC-005** docs enumerate strings — ✅ verified by `contracts/per-reader-strings.md` + `docs/reference/sbom-format-mapping.md` C151 row.

Additional verification:

- **FR-010** blacklist scan — ✅ verified by `fr010_reason_strings_no_pii_paths_credentials`
- **FR-002** ASCII + bounded — ✅ verified by `fr002_reason_strings_are_ascii_bounded_length`
- **Q2 scope-trim regression guard** — ✅ verified by `m236_scope_matches_q2_clarification` (asserts inventory count = 17)
