# Feature Specification: Complete design-tier SBOM documentation in ecosystems.md

**Feature Branch**: `227-design-sbom-docs`
**Created**: 2026-08-05
**Status**: Draft
**Input**: User description: "can we update ecosystems.md to include info about design sboms, i.e., non-lock file sboms? we have some info, but I want more complete info there"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Operator predicts SBOM tier before running a scan (Priority: P1)

An operator preparing to run waybill against a source tree needs to understand ahead of time whether their scan will produce a **source-tier** SBOM (fully-resolved versions, transitive graph where lockfiles or registry data are available), a **design-tier** SBOM (declared inventory only, versionless where the version can't be resolved), or a mix. Today they have to either run the scan and inspect the output, or read the source code of each ecosystem reader. The documentation should let them predict the outcome from their project's contents alone.

**Why this priority**: This is the single most impactful piece of missing information. Operators choosing between waybill and other SBOM tools compare declared vs resolved coverage; without a clear "what you'll get" story per ecosystem, waybill's design-tier fallback (a genuine differentiator vs trivy/syft which give up when no lockfile is present) looks like a weakness rather than a strength.

**Independent Test**: A reader who has never used waybill can, given a description of a project (e.g., "a .NET repo with `.csproj` files but no `packages.lock.json`" or "a Cargo workspace with `Cargo.toml` but no `Cargo.lock`"), correctly predict from `docs/ecosystems.md` alone which components will appear at source-tier, which at design-tier, and which will be missing entirely.

**Acceptance Scenarios**:

1. **Given** an operator has a Ruby project with a `Gemfile` but no `Gemfile.lock`, **When** they read the gem section of `ecosystems.md`, **Then** they can determine that waybill will emit design-tier components with versionless PURLs (`pkg:gem/<name>`) rather than skipping the ecosystem entirely.
2. **Given** an operator has a mixed monorepo with a Cargo project (has `Cargo.lock`) and a Python project (no lockfile), **When** they read the tier-concept section of `ecosystems.md`, **Then** they understand that the same output SBOM will contain source-tier Cargo components AND design-tier Python components, with the mixed-tier state visible via the `waybill:sbom-tier` property on each component.
3. **Given** an operator is deciding whether waybill fits their compliance-attribution use case, **When** they read the "when design-tier is enough vs when it isn't" subsection, **Then** they can decide correctly without consulting engineering support.

---

### User Story 2 - Downstream consumer filters or acts on design-tier components (Priority: P2)

Downstream tools and human reviewers consuming waybill-emitted SBOMs need to distinguish design-tier from source-tier components at the JSON level. Design-tier is a signal that vulnerability scanners should NOT run exact-version CVE matches against these components (they'd be silent no-match false negatives on a versionless PURL). Attribution/compliance tools SHOULD treat them as authoritative declared-inventory records.

**Why this priority**: Downstream consumption is the point of emitting SBOMs. Without documented filtering recipes, consumers either mistake design-tier components as "missing data" and drop them, or run inappropriate vuln scans against them.

**Independent Test**: A person building a CI-integrated SBOM consumer can, using only `docs/ecosystems.md`, produce a working `jq` filter that extracts (a) only source-tier components, (b) only design-tier components, and (c) the `waybill:unresolved-reason` for each design-tier component — for both the CycloneDX and SPDX output formats.

**Acceptance Scenarios**:

1. **Given** a waybill CDX output with a mix of source-tier and design-tier components, **When** the consumer applies the documented `jq` filter for source-tier only, **Then** the filter correctly returns exactly the source-tier subset with no false positives or negatives.
2. **Given** a design-tier component in an emitted SBOM, **When** a consumer looks for a human-readable reason the version is unresolved, **Then** the documentation points them to a specific annotation field (`waybill:unresolved-reason`) with per-ecosystem example values.
3. **Given** a downstream tool that must decide whether to run a vulnerability scan against a component, **When** the tool implements the documented decision rule, **Then** it correctly skips exact-version CVE matches on design-tier components while still running them on source-tier components.

---

### User Story 3 - Contributor implementing a new ecosystem reader follows the design-tier convention (Priority: P3)

A waybill contributor adding a new ecosystem reader (e.g., a new build system or lockfile format) needs to know the cross-ecosystem convention for design-tier emission so their new reader behaves consistently: versionless PURL, `sbom_tier: "design"`, `waybill:unresolved-reason` annotation, and any per-ecosystem specifics (how to detect the fallback condition, what to say in the reason field).

**Why this priority**: This is a smaller audience than P1/P2 (contributors vs operators/consumers), and existing readers already follow the pattern by copy-paste from precedent. But documented conventions reduce onboarding friction and prevent drift — new contributors sometimes invent their own "unresolved" sentinel (as the NuGet reader did before #653) instead of following the established pattern.

**Independent Test**: A new contributor adding a hypothetical `pkg:foo/*` reader can, from `docs/ecosystems.md` alone, write correct design-tier emission code without consulting any existing reader's source.

**Acceptance Scenarios**:

1. **Given** a new reader implementation, **When** the contributor cannot resolve a component's version, **Then** the documentation describes the exact SBOM shape they must emit (PURL, tier, annotation) with copy-paste-ready field values.
2. **Given** a contributor is deciding what to put in the `waybill:unresolved-reason` string, **When** they consult the docs, **Then** they find a per-ecosystem catalog of existing reason strings they can pattern-match against.

---

### Edge Cases

- **Same scan produces mixed tiers**: An operator's scan discovers both fully-resolved components (via a lockfile) and unresolved ones (in a sibling project with no lockfile). Documentation must make clear these coexist in the same output SBOM and that the `waybill:sbom-tier` property distinguishes them per component.
- **Ecosystem has NO design-tier fallback**: Some readers (e.g., OS package readers like `dpkg`, `rpm`, `apk`) always emit source-tier because installed packages are always resolved by definition. Documentation must explicitly note "no design-tier for this ecosystem" rather than leaving readers to infer.
- **Design-tier emission is gated by a CLI flag**: Some ecosystems (e.g., Kotlin DSL v0.1 per milestone 122) require an explicit opt-in flag (`--include-declared-deps`) to emit design-tier components. Documentation must call out which ecosystems require opt-in vs which emit design-tier automatically.
- **Design-tier component gets re-classified when a sibling scan finds a lockfile**: Cross-tier reconciliation (milestone 191) can merge a design-tier component with a later-discovered source-tier sibling of the same PURL. Documentation should point to how this shows up in the output (single component, upgraded to source-tier).
- **Graph completeness at document scope**: The milestone-158 graph-completeness annotation summarizes whether the emitted SBOM has full-transitive coverage. Design-tier components inherently mean partial coverage. Documentation must connect the two concepts.
- **Vulnerability scanner silent-miss risk**: If a consumer forgets the tier distinction and runs an exact-version CVE match against a design-tier component, the match will produce false negatives (no version to match against) — silent misses, not loud errors. Documentation must call out this failure mode explicitly.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The updated `docs/ecosystems.md` MUST contain a dedicated top-level section that explains the three SBOM tiers (source, design, binary) as concepts — what each means, when waybill emits each, and how a consumer detects them in emitted SBOMs.

- **FR-002**: The tier section MUST include a complete per-ecosystem matrix (or a fully-populated column added to the existing coverage matrix at the top of the document) that answers, for every ecosystem waybill supports: (a) does this ecosystem have a design-tier fallback? (b) what trigger condition causes design-tier emission? (c) what PURL shape does design-tier produce? (d) does design-tier emission require an operator opt-in flag, and if so which flag?

- **FR-003**: The section MUST document a set of copy-pasteable `jq` recipes for downstream SBOM consumers that cover at minimum: filter to source-tier only, filter to design-tier only, extract `waybill:unresolved-reason` per component, and count components per tier. Recipes MUST be provided for both CycloneDX and SPDX output formats.

- **FR-004**: The section MUST document the `waybill:unresolved-reason` annotation field: where it appears in each output format (CDX/SPDX 2.3/SPDX 3), what values are populated per ecosystem (with concrete examples), and how a consumer should interpret the reason text.

- **FR-005**: Every existing per-ecosystem section in `docs/ecosystems.md` (currently ~30 sections) MUST either (a) briefly describe its design-tier fallback behavior in-place, or (b) contain an explicit link to the new tier section with a per-ecosystem anchor. Consumers reading a single ecosystem section MUST NOT have to hunt across the document to find the design-tier semantics for that ecosystem.

- **FR-006**: The section MUST include a "when to use design-tier as-is vs when to seek fuller resolution" guidance subsection that names concrete use cases waybill supports well with design-tier alone (compliance attribution, declared-inventory manifests, contract audits) AND concrete use cases where design-tier is insufficient (exact-version CVE scanning, transitive-graph analysis, license-conflict analysis on inherited deps).

- **FR-007**: The section MUST document how design-tier components affect the milestone-158 graph-completeness annotation at document scope — specifically that design-tier components imply partial-coverage classification, and how a consumer distinguishes "partial due to design-tier fallback" from "partial due to unreachable-from-any-root" (both classes exist).

- **FR-008**: The section MUST document existing options for upgrading design-tier to source-tier where the operator can supply additional context, at minimum: generating a lockfile in the appropriate ecosystem, using `--supplement-cdx` to overlay externally-known versions, or (per-ecosystem) opt-in resolver flags like `--warm-go-cache`.

- **FR-009**: The updated documentation MUST use only markdown that renders correctly on GitHub's `docs/` viewer without extra plugins. No proprietary rendering syntax, no fenced code with unusual highlighters. Tables MUST use standard pipe-delimited markdown.

- **FR-010**: Every claim about a specific reader's design-tier behavior in the new documentation MUST be verifiable against the current state of waybill's source code as of the merge time (i.e., no memory-based claims that reference behavior from prior states). This is a documentation-accuracy invariant; the memory `feedback-verify-research-empirical-claims` applies.

### Key Entities *(include if feature involves data)*

- **SBOM tier**: A per-component classification (`source`, `design`, `binary`) indicating the strength of provenance backing the component's version claim. Emitted as a property/annotation on each component in the output SBOM.
- **Design-tier fallback trigger**: A per-ecosystem condition that causes a reader to emit design-tier instead of source-tier — typically "no lockfile present AND version cannot be resolved from the manifest alone". Each ecosystem has its own specific condition documented in-reader today; consolidated in the new documentation.
- **Unresolved-reason annotation**: A per-component string field explaining WHY a design-tier component's version is unresolved. Enables downstream tools to display actionable remediation guidance to human reviewers.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A reader unfamiliar with waybill can, using only the updated `docs/ecosystems.md`, correctly predict for any of the ~30 supported ecosystems whether a source-tree scan without a lockfile will produce components (design-tier), skip the ecosystem entirely, or fail with an error. Prediction accuracy target: 100% across a representative panel of 5 ecosystems chosen at random.

- **SC-002**: A downstream SBOM consumer building a filter for their internal tooling can locate a working `jq` recipe from `docs/ecosystems.md` in under 60 seconds, and the recipe returns semantically correct output on a representative waybill CDX SBOM containing at least one source-tier component AND at least one design-tier component.

- **SC-003**: Every one of the current per-ecosystem sections in `docs/ecosystems.md` either describes its design-tier behavior in-line OR contains a working cross-reference link to the new tier section. Verification: manual count of section headings + cross-references, 100% coverage.

- **SC-004**: A contributor adding a hypothetical new ecosystem reader can, from `docs/ecosystems.md` alone, produce a correct design-tier emission code path — verified by writing pseudocode from the doc and comparing to an existing reader's actual behavior; correctness target: matching all 4 fields (PURL shape, `sbom_tier` value, `waybill:unresolved-reason` presence, `waybill:unresolved-reason` value shape).

- **SC-005**: The new tier section fits within a single continuous read of no more than 500 lines of markdown. A reader can consume it end-to-end without scrolling through the entire 1500-line document.

- **SC-006**: All documentation claims are verifiable against source: a spot-check of 5 randomly-selected reader-specific claims in the new section (e.g., "gem reader emits versionless PURL when Gemfile has no matching Gemfile.lock entry") successfully match the corresponding source code behavior on the merge commit.

## Assumptions

- **Scope is docs-only**: This feature touches `docs/ecosystems.md` (and possibly small related documentation files it cross-references, like `docs/reference/reading-a-waybill-sbom.md`). The waybill binary itself is not modified. No code paths, no test suite changes, no CLI surface changes.
- **The three-tier model is settled**: `source` / `design` / `binary` are the established tiers used across the codebase (see the `sbom_tier: Some("design")` pattern across 15+ readers). This feature documents the model as-is; it does not propose a new tier taxonomy.
- **Existing consumer-guide overlap is acceptable**: `docs/reference/reading-a-waybill-sbom.md` already covers some consumer-facing tier information for a general audience. `docs/ecosystems.md` is the per-ecosystem-reader reference. Some overlap between the two is expected; they serve different audiences (consumer vs operator+contributor). The new section MAY reference the consumer guide via link rather than duplicate content.
- **jq recipes target `jq 1.6+`**: Recipes will use syntax available in jq 1.6 (already used elsewhere in waybill docs per milestone 150-151 consumer guide precedent).
- **The 2026-08-04 NuGet audit's terminology stands**: the audit doc introduced the term "design-tier" for consumer-facing use; this feature aligns terminology with that doc rather than re-inventing labels.
- **No new user-facing waybill flags are proposed**: If gaps in operator ergonomics are identified during writing (e.g., "we should have a flag to filter design-tier from output"), they get filed as follow-up issues, not bundled into this docs feature.
