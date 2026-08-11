# Feature Specification: `--tier=<mode>` output-filter flag

**Feature Branch**: `232-tier-filter-flag`
**Created**: 2026-08-10
**Status**: Draft
**Input**: User description: "CLI: add --tier=<mode> output-filter flag with modes all (default), source-only, design-only, source-and-binary. Filters the emitted SBOM's components + edges to the requested sbom_tier set. Document-scope annotations (graph-completeness, workspace_modules, etc.) reflect the FILTERED set, not the pre-filter set. Motivation: vulnerability scanners want source-only; compliance-attribution pipelines want design-only; today operators post-process with jq. Add integration test per mode. No new Cargo deps." (Closes #660.)

## Background

Every component waybill emits carries an `sbom_tier` marker: `source`, `design`, `binary`, `analyzed`, or `file`. Different downstream consumers care about different tier sets:

- **Vulnerability scanners** (Trivy, Grype, Snyk) want ONLY `source`-tier components with resolved versions. Design-tier entries carry versionless PURLs and false-positive as "unknown severity" or silent-miss the entire component.
- **Compliance/attribution pipelines** (SBOM registries, license auditors) want ONLY `design`-tier components — the developer-declared graph, without the resolver's binary-tier probes muddying the roll-up.
- **Binary-artifact consumers** (container SBOMs feeding SLSA verifiers) want `source` + `binary` (the two "actually shipped" tiers) but not `design`.

Today the only path is post-processing with `jq` per `docs/ecosystems.md` §3 recipes — the operator must run a second step, manage the intermediate file, and remember to also filter document-scope annotations (`waybill:graph-completeness`, `waybill:workspaces-detected`, and every other counter that summarizes across the component set). A native `--tier=<mode>` flag collapses this into the scan itself and guarantees the annotations reflect the filtered set consistently.

## Clarifications

### Session 2026-08-10

- Q: Under `--tier=source-only`, do `analyzed`-tier and `file`-tier components survive the filter? → A: No — strict literal match on `sbom_tier`. `--tier=source-only` keeps only `sbom_tier == "source"`. `analyzed`, `file`, `design`, and `binary` are all dropped. Simplest mental model; the flag name IS the filter. Follow-up milestones can add `--tier=analyzed-only` / `--tier=file-only` if operators ask.
- Q: Should `--tier` hard-error on any combination with other flags? → A: No mutual exclusions. `--tier` composes cleanly with every existing CLI flag. Degenerate combinations (e.g., filter drops the main-module referenced by `--sign-key`) produce a WARN log + potentially-empty output; the scan still exits 0. Rationale: hard-errors on flag combinations are decisions users regret later; WARN-and-continue leaves options open.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Vulnerability-scanner pipeline gets a clean source-tier SBOM (Priority: P1)

A CI operator running `waybill sbom scan --tier=source-only` produces an SBOM whose components are exclusively `sbom_tier: "source"`. Every dependency edge whose source OR target is a filtered-out component is dropped from the emitted `dependencies[]` array. The document-scope `waybill:graph-completeness` annotation re-evaluates against the filtered graph — so a scan that was "complete" pre-filter may report "partial" post-filter if design-tier orphans were the only components giving reachability to some source-tier subtree.

**Why this priority**: The vulnerability-scanner use case is the most concretely painful today. Every operator running waybill + Trivy in the same pipeline currently has to hand-craft `jq` recipes. Solving this first delivers the highest-leverage win.

**Independent Test**: Scan a fixture that mixes source- and design-tier NuGet or Go components (m230 / m655 shape). Assert (a) every emitted component has `sbom_tier: "source"` (or equivalent CDX property indicating source-tier); (b) no design-tier PURLs appear anywhere in the output; (c) every `dependencies[]` entry references only source-tier bom-refs; (d) the `waybill:graph-completeness` annotation reflects the source-tier-only reachability.

**Acceptance Scenarios**:

1. **Given** a scan that produced 10 source-tier + 5 design-tier components pre-filter, **When** the same scan is invoked with `--tier=source-only`, **Then** the emitted SBOM has 10 components, all source-tier, with dependency edges referencing only source-tier bom-refs.
2. **Given** the same scan, **When** invoked with the default (`--tier=all` or no flag), **Then** all 15 components appear (byte-identical to pre-232 emission).
3. **Given** a scan where every design-tier component is orphaned but a source-tier subgraph is fully connected, **When** invoked with `--tier=source-only`, **Then** `waybill:graph-completeness` reports `complete` on the filtered graph (design-tier orphans no longer contribute to the classifier's decision).

---

### User Story 2 — Compliance/attribution pipeline gets design-tier-only view (Priority: P2)

An operator running compliance analysis wants the developer-declared dependency graph (from manifests + Directory.Packages.props + go.mod requires) without the resolver's binary-detection noise. `--tier=design-only` produces this: only components tagged `sbom_tier: "design"` survive.

**Why this priority**: Second-most-common downstream ask. Not as widely painful as US1 because compliance auditors are more tolerant of noise; but a first-class flag removes the last friction point.

**Independent Test**: Scan a fixture with mixed tiers. Assert every emitted component has `sbom_tier: "design"` and every dependency edge references only design-tier bom-refs. Design-tier's typical low reachability rate is preserved: the `waybill:graph-completeness` annotation may report `partial` because design-tier graphs are naturally sparser.

**Acceptance Scenarios**:

1. **Given** the mixed fixture from US1, **When** invoked with `--tier=design-only`, **Then** the emitted SBOM contains exactly the 5 design-tier components (no source-tier entries).
2. **Given** a scan with zero design-tier components, **When** invoked with `--tier=design-only`, **Then** the emitted SBOM has zero components and the scan exits 0 with a WARN log line noting "0 components emitted after tier filter".

---

### User Story 3 — Container/artifact pipeline gets source-plus-binary view (Priority: P3)

An operator building container SBOMs wants `source` + `binary` (the two "actually shipped" tiers) but not `design`. `--tier=source-and-binary` collapses the design-tier before emission.

**Why this priority**: Narrower audience than US1/US2; SLSA verifiers today handle the mixed-tier output OK. Ship as convenience once US1+US2 land.

**Independent Test**: Scan a container fixture (image with a resolvable Go binary + a Cargo.toml). Assert every emitted component has `sbom_tier: "source"` OR `"binary"`; no design-tier components. Both source and binary components appear when present.

**Acceptance Scenarios**:

1. **Given** a fixture with 3 source + 2 binary + 4 design components, **When** invoked with `--tier=source-and-binary`, **Then** the emitted SBOM has 5 components (3 source + 2 binary), design components dropped.

---

### Edge Cases

- **`--tier=source-only` combined with `--sbom-type=<value>`** or other document-scope options: the tier filter runs LAST in the emission pipeline, so `--sbom-type` metadata reflects the filtered set (e.g., a scan that would emit sbom-type `application` may downgrade to `library` if the filter drops the top-level main-module component).
- **Zero-component result**: if the filter removes every component, the SBOM emits with an empty `components[]` array and empty `dependencies[]`. Not an error. WARN log line notes the outcome so operators can debug.
- **Analyzed-tier and file-tier**: Per Clarifications §1, the filter is a strict literal match on `sbom_tier`. Under `--tier=source-only`, `analyzed`-tier and `file`-tier components are dropped (they don't match the `"source"` string). Same treatment under `--tier=design-only` and `--tier=source-and-binary`. Follow-up milestones can add `--tier=analyzed-only` / `--tier=file-only` modes if operators need them; not in scope here.
- **Composite tags via `waybill:also-detected-via`**: A component can be primary-tier `source` but ALSO detected via a design-tier path. The filter uses the primary `sbom_tier` value only — secondary detection tier does NOT re-qualify a filtered-out component.
- **Multi-format output (CDX + SPDX 2.3 + SPDX 3)**: The tier filter applies identically across all three formats — same components dropped, same annotations re-evaluated.
- **`--split` combined with `--tier=...`**: Filter runs on each split boundary independently. A workspace where module-a has source deps and module-b has only design deps produces two split SBOMs — one with content, one with only the main-module scaffold.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The `waybill sbom scan` CLI MUST accept a new `--tier=<mode>` flag whose valid values are `all` (default), `source-only`, `design-only`, and `source-and-binary`.
- **FR-002**: When `--tier=all` is set (or the flag is omitted), the emitted SBOM MUST be byte-identical to the pre-232 emission for the same scan input across all three supported output formats (CDX 1.6, SPDX 2.3, SPDX 3.0.1). Verified via diff against pre-232 goldens.
- **FR-003**: When `--tier=source-only` is set, the emitted SBOM's `components[]` MUST contain exclusively components whose primary `sbom_tier` is `"source"`. Components with `sbom_tier` `"design"`, `"binary"`, `"analyzed"`, or `"file"` MUST be excluded.
- **FR-004**: When `--tier=design-only` is set, the emitted SBOM's `components[]` MUST contain exclusively components whose primary `sbom_tier` is `"design"`.
- **FR-005**: When `--tier=source-and-binary` is set, the emitted SBOM's `components[]` MUST contain components whose primary `sbom_tier` is `"source"` OR `"binary"`.
- **FR-006**: For every filter mode, the emitted SBOM's `dependencies[]` (CDX) / `relationships[]` (SPDX) MUST drop any entry whose source or target references a filtered-out component. No dangling bom-refs allowed.
- **FR-007**: Document-scope annotations that summarize across the component set MUST re-evaluate against the filtered set — including `waybill:graph-completeness`, `waybill:graph-completeness-reason`, `waybill:workspaces-detected`, `waybill:cisa-2026-lifecycle`, and any counter or classifier that iterates `components[]`.
- **FR-008**: When the filter results in zero surviving components, the scan MUST NOT error. The SBOM MUST emit with empty `components[]` and `dependencies[]`, a WARN log line MUST note the outcome, and the scan MUST exit 0.
- **FR-009**: The filter MUST apply identically across all three output formats. A CDX scan and an SPDX 2.3 scan of the same input with the same `--tier` mode MUST include the same set of underlying components (same PURLs; equivalent bom-refs).
- **FR-010**: The filter modes MUST use strict literal `sbom_tier` matching. `--tier=source-only` matches ONLY `sbom_tier == "source"`; `--tier=design-only` matches ONLY `sbom_tier == "design"`; `--tier=source-and-binary` matches `sbom_tier == "source"` OR `sbom_tier == "binary"`. Components tagged `analyzed` or `file` are dropped under all three modes (they don't match any of the three modes' string sets). This is intentional — the flag name IS the filter.
- **FR-011**: The flag MUST compose cleanly with every existing `waybill sbom scan` flag (`--split`, `--sbom-type`, `--sign`, `--sign-key`, `--offline`, `--supplement-cdx`, `--root-name`, etc.). No CLI-parse-level mutual exclusions are introduced by this milestone. Degenerate combinations (e.g., the filter drops the main-module a signature step depends on) MUST produce a WARN log line and continue; they MUST NOT cause the scan to exit non-zero.

### Key Entities

- **Tier filter mode**: One of `all` / `source-only` / `design-only` / `source-and-binary`. Encoded as a small enum in the CLI parser and threaded through the emission pipeline. `all` is the default and is a no-op filter (byte-parity guarantee).
- **Component `sbom_tier` marker**: The existing per-component `sbom_tier: Option<String>` field on `PackageDbEntry` → `ResolvedComponent`. This milestone reads it; it does not add new values.
- **Emitted SBOM document**: The final CDX/SPDX artifact written to the operator's `--output` path. This milestone filters what appears in it; it does not change the wire format schemas.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a fixture producing 10 source-tier + 5 design-tier components, `waybill sbom scan --tier=source-only` emits an SBOM with exactly 10 components, all `sbom_tier: "source"`, and zero design-tier PURLs in `components[]` or `dependencies[]`. Measured by grep + jq on the emitted SBOM.
- **SC-002**: Same fixture, `--tier=design-only` emits an SBOM with exactly 5 components, all design-tier. Measured identically.
- **SC-003**: Same fixture, `--tier=all` (or flag omitted) emits an SBOM byte-identical to the pre-232 emission (with content-addressed IDs, timestamps, and serial numbers masked per memory `feedback_verify_golden_churn_normalized`). Measured by normalized diff.
- **SC-004**: `waybill:graph-completeness-reason` on a scan where design-tier orphans dominate the graph reports the DIFFERENT reason before and after `--tier=source-only`. Pre-filter may say `orphaned-components-detected: N`; post-filter may drop that reason code because the orphans no longer exist in the filtered set. Measured by jq inspection of the annotation.
- **SC-005**: The three output formats (CDX 1.6, SPDX 2.3, SPDX 3.0.1) emitted from the same scan input with the same `--tier=source-only` mode contain the same set of PURLs when compared component-by-component. Measured by extracting PURLs from each format and asserting set-equality.

## Assumptions

- The `sbom_tier: Option<String>` field on `ResolvedComponent` (introduced in m005 and consumed pervasively since) is the source of truth for tier classification. This milestone reads that field; it does NOT invent new tier values. Existing values are: `source`, `design`, `binary`, `analyzed`, `file`.
- The tier filter runs LATE in the emission pipeline — after resolution + reconciliation + graph-completeness computation — because SC-004 requires the annotations to reflect the FILTERED set. Concrete ordering: (1) resolve components; (2) apply filter; (3) recompute document-scope annotations; (4) serialize. This ordering is a plan-phase design choice; the spec captures the outcome, not the mechanism.
- The three named filter modes plus `all` are the initial delivery. Future modes (`--tier=binary-only`, `--tier=source-and-analyzed`, etc.) can be added as follow-up milestones once the enum + pipeline plumbing exists; not scoped here.
- The `--tier` flag's default is `all` (no filter). Existing scan invocations MUST continue to produce the same output as before — enforced by SC-003.
- No new Cargo dependencies. This is a CLI-flag addition + a filter pass in the existing emission pipeline; both are standard extensions.
- The synthetic fixture for SC-001/SC-002 uses the existing NuGet `packages_lock_present` shape (m230-authored) or extends it with a design-tier component. Fixture module names use the `MikebomFixture.*` synthetic convention per memory `feedback_fixture_synthetic_package_names`.
- Grafana or other external repos are NOT needed for verification; the synthetic fixture is sufficient.
- The `--tier` flag composes with `--split` in the natural way: filter runs on each split boundary's component set. A split that ends up empty after filtering still emits its manifest entry (possibly with zero components in its manifest); no split-fragment error.
