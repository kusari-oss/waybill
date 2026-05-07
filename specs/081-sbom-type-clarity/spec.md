# Feature Specification: SBOM-type signaling clarity

**Feature Branch**: `081-sbom-type-clarity`
**Created**: 2026-05-07
**Status**: Draft
**Input**: User description: "I want us to really explore, potentially redevelop or harden our ability to understand what type of SBOM is being delivered. This could be just docs in the case we don't need to do anything since we support this, or exploring options for making this better"

## Overview

Downstream consumers of mikebom-emitted SBOMs need to know **what type of SBOM they're looking at**. The CISA "SBOM Types" guidance (April 2023, Software Supply Chain Working Group) defines six canonical types — Design, Source, Build, Analyzed, Deployed, Runtime — and downstream tooling (vulnerability scanners, regulatory pipelines, CISA-aligned compliance dashboards) increasingly classifies SBOMs by these types to apply correct policy. An operator running mikebom needs to be able to (1) determine which SBOM type a given mikebom-emitted document represents, (2) trust that the answer matches the actual data sources mikebom used, and (3) optionally assert the type explicitly when their pipeline knows better than auto-detection.

mikebom already does meaningful work in this space:

- **Per-component `mikebom:sbom-tier` annotation** (introduced milestone 047) tags every emitted component with one of `design / source / build / deployed / analyzed`. Five values, not six (missing CISA's `runtime`).
- **CDX 1.6 `metadata.lifecycles[]`** is aggregated natively from per-component tier values via `mikebom-cli/src/generate/lifecycle_phases.rs::aggregate_phases` and emitted as the standards-native CDX phase set (`design / pre-build / build / post-build / operations`).
- **SPDX 2.3 + SPDX 3** receive the aggregated phase set in document-level `comment` fields.
- **Five CDX phases vs six CISA types**: mikebom's mapping today is `source → pre-build`, `build → build`, `analyzed → post-build`, `deployed → operations`, `design → design`. The CISA `Runtime` type has no mikebom tier or CDX phase mapping today.

The operator-facing problem the milestone addresses: even with the above infrastructure, mikebom does NOT today document which SBOM type a given output represents in a way an operator can read without opening source code. There's no `docs/reference/sbom-types.md`. The `mikebom:sbom-tier` annotation is internal-vocab; CDX `metadata.lifecycles[]` is spec-vocab but uses different label strings; SPDX comment is free-text. An operator inspecting an mikebom SBOM has to derive the SBOM type by cross-referencing three different field positions across three formats with three different label conventions.

The milestone is deliberately **exploration-first**: the audit is the central deliverable. Implementation work is **conditional on what the audit finds** — if the audit reveals all the native fields are wired correctly and the only gap is documentation, the milestone ships docs only. If the audit reveals SPDX 3's `software_LifecycleScopeType` (or its document-level Sbom-class equivalent) isn't wired natively, OR the `runtime` tier needs to be added, OR an operator self-assert flag is missing, those become P2/P3 conditional follow-up stories within this milestone or split into separate milestones.

## Clarifications

### Session 2026-05-07

- Q: When `metadata.lifecycles[]` aggregates multiple tiers (e.g., `[pre-build, build]`), how does the operator-facing documentation tell consumers to interpret the SBOM type? → A: **"Mixed-type SBOM" — the docs say the SBOM spans multiple CISA types and lists each type from `lifecycles[]`.** Transparent and accurate to the underlying CDX 1.6 spec intent (the array IS multi-element when components span tiers; mikebom does not invent a "dominant tier" heuristic). Operators who need single-type assertion for downstream pipelines pass `--sbom-type` (US3); the docs explicitly point them there. mikebom does not infer a single SBOM type from a multi-tier aggregation.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator can identify the SBOM type from a mikebom-emitted document (Priority: P1)

A regulatory-compliance operator receives a mikebom-emitted SBOM (any of CDX 1.6 / SPDX 2.3 / SPDX 3) and needs to classify it as one of the six CISA SBOM Types so their downstream pipeline applies the correct policy. Today this requires reading source code or guessing from field contents. Post-milestone, the operator opens `docs/reference/sbom-types.md`, finds the per-format field-position table, runs a documented `jq` query, and gets a definitive answer in under 60 seconds.

**Why this priority**: This is the headline operator pain point — without it, the existing milestone-047 infrastructure is invisible to consumers. P1 because docs alone (no code change) close the dominant gap, and the audit work for the docs is also the dependency for any P2/P3 code changes.

**Independent Test**: Take a mikebom-emitted SBOM in any of the three formats; consult `docs/reference/sbom-types.md`; identify the SBOM type via the documented field-position + label-translation table; cross-check against the actual data sources mikebom used (e.g., source-tier scan → "Source SBOM"). Independent of any new code — verifiable against today's emission output.

**Acceptance Scenarios**:

1. **Given** a mikebom-emitted CDX 1.6 SBOM, **When** the operator follows the documented procedure in `docs/reference/sbom-types.md`, **Then** they identify the SBOM as one of {Design, Source, Build, Analyzed, Deployed, Runtime} (or "mixed-type" with a documented per-component breakdown if components span multiple tiers).
2. **Given** the same SBOM in SPDX 2.3, **When** the operator follows the equivalent procedure, **Then** they reach the same answer.
3. **Given** the same SBOM in SPDX 3, **When** the operator follows the equivalent procedure, **Then** they reach the same answer.
4. **Given** the documentation, **When** the operator searches for "CISA SBOM Types" or "SBOM type" or "lifecycle", **Then** the docs surface (`docs/reference/sbom-types.md`) is discoverable from the existing docs index + README.

---

### User Story 2 — Phase 0 audit produces a per-format native-field gap list (Priority: P1)

A maintainer evaluating whether mikebom satisfies Constitution Principle V's standards-native-precedence requirement for SBOM-type signaling needs an audit of the per-format native fields that exist for SBOM-type metadata. The audit must enumerate: (a) what mikebom currently emits in each format; (b) what each format's native vocabulary is for SBOM-type metadata (CDX `metadata.lifecycles[]`, SPDX 2.3 spec section X, SPDX 3 `software_Sbom.context` or equivalent); (c) the gap list — native fields that exist but aren't wired, or `mikebom:`-prefix fields that should be promoted to native equivalents.

**Why this priority**: This is the audit deliverable that gates US3 + US4. P1 because the audit is needed for both US1 (docs require accurate native-field references) and any P2/P3 code-change decisions. The audit itself is a docs/research artifact — output is `docs/reference/sbom-types.md` Phase 0 §1 + the milestone's research.md.

**Independent Test**: Inspect the audit output (research.md + the per-format-mapping table in docs); cross-reference against the actual format specs (CDX 1.6 schema, SPDX 2.3 spec, SPDX 3 model docs); confirm the audit's claims about native fields are accurate.

**Acceptance Scenarios**:

1. **Given** the audit deliverable, **When** a reviewer cross-checks one claim per format against the official spec, **Then** all three claims are accurate (no native field misnamed, no field-existence claim wrong).
2. **Given** the audit's gap list, **When** a maintainer evaluates each gap entry against Constitution Principle V, **Then** each gap either (a) gets a follow-up implementation task in US3 or US4, or (b) is documented as out-of-scope with rationale.

---

### User Story 3 — Operator-asserted SBOM type via `--sbom-type <type>` flag (Priority: P2)

An operator running mikebom in a context where they KNOW the resulting SBOM should be classified as a specific CISA type (e.g., a CI/CD pipeline scanning a freshly-built container image — the operator knows it's a "Build" SBOM regardless of mikebom's per-component auto-detection) needs to assert the type explicitly. Today there's no operator override; the SBOM type is purely a derived property of the per-component tier mix.

**Why this priority**: P2 because (a) most operators get correct auto-detection for single-tier scans and don't need the override, and (b) per the 2026-05-07 Q1 clarification, mixed-tier scans are presented transparently as "Mixed-type SBOM" — operators who NEED single-type assertion for downstream pipelines (regulatory dashboards expecting a single CISA type) reach for `--sbom-type` as the documented escape hatch. US1's audit deliverable will reveal whether this need is common enough to justify shipping in this milestone or whether it's a follow-up filing. If the audit reveals mixed-tier SBOMs are common in real operator usage AND downstream pipelines hard-fail on multi-type lifecycles, US3 becomes more essential and may promote to P1.

**Independent Test**: Run `mikebom sbom scan --sbom-type build --path .`; assert all three formats emit a single, unambiguous `build`-type signal at their respective native fields (CDX `metadata.lifecycles = [{phase: "build"}]`; SPDX 3 native field; SPDX 2.3 comment).

**Acceptance Scenarios**:

1. **Given** an operator passes `--sbom-type build`, **When** the SBOM is emitted in any of the three formats, **Then** the SBOM-type signal at the format-native field reflects `build` regardless of per-component tier auto-detection.
2. **Given** the operator passes `--sbom-type build` AND per-component auto-detection produces a different tier mix, **When** the SBOM is inspected, **Then** the document-level type is `build` (operator assertion wins) AND the per-component `mikebom:sbom-tier` annotations preserve the auto-detected per-component tiers (operator override is document-level only, not per-component).
3. **Given** an invalid type value (e.g., `--sbom-type foobar`), **When** the CLI parses the flag, **Then** the invocation fails with a clear "valid types are design/source/build/analyzed/deployed/runtime" error.

---

### User Story 4 — Add the `runtime` SBOM-type tier (Priority: P3)

The CISA SBOM Types document defines six types; mikebom's `mikebom:sbom-tier` vocabulary covers five (`design / source / build / deployed / analyzed`). The missing tier is `runtime` — an SBOM produced from observation of a running system (eBPF live-trace, runtime instrumentation, etc.). mikebom's eBPF trace path (when feature `ebpf-tracing` is enabled, per the project's existing infrastructure) arguably emits runtime data — making `runtime` a candidate tier value mikebom could legitimately use.

**Why this priority**: P3 because (a) it depends on US1's audit confirming the eBPF trace path actually represents Runtime semantics per CISA, and (b) the operator-facing impact is small unless downstream tooling specifically filters for "Runtime SBOMs" (which is uncommon today in the SBOM consumer ecosystem). If the audit reveals that adding `runtime` requires re-categorizing build-tier emission OR breaks existing per-component goldens, this becomes a deferred follow-up rather than a P2/P3 task in this milestone.

**Independent Test**: Run `mikebom trace run --feature ebpf-tracing` (or equivalent eBPF-enabled invocation); inspect the emitted SBOM; assert the per-component `mikebom:sbom-tier` and aggregated CDX `lifecycles[]` include `runtime` for components observed via eBPF.

**Acceptance Scenarios**:

1. **Given** the audit confirms eBPF-traced components map to CISA Runtime, **When** the milestone adds `runtime` to the `mikebom:sbom-tier` enum + `tier_to_phase` mapping at `lifecycle_phases.rs:33`, **Then** eBPF-traced SBOMs surface `runtime` at the per-component level + aggregate to a `runtime` CDX phase.
2. **Given** the addition of `runtime`, **When** existing milestone-047 byte-identity goldens are re-run, **Then** they stay byte-identical (because no existing test fixture exercises eBPF emission OR the goldens regen with the runtime addition documented as the milestone's expected operator-visible change).

---

### Edge Cases

- **Mixed-tier SBOM**: a polyglot scan may produce some components tagged `source` (e.g., from a manifest in the source tree) AND others tagged `build` (e.g., from artifacts in the build cache). Today CDX `metadata.lifecycles[]` aggregates BOTH phases (e.g., `[{phase: "pre-build"}, {phase: "build"}]`). Per the 2026-05-07 clarification, the docs surface presents this transparently as a **"Mixed-type SBOM"** spanning multiple CISA types (each type listed from `lifecycles[]`). mikebom does not invent a "dominant tier" heuristic. Operators who need a single-type assertion for downstream pipelines (regulatory dashboards, CISA-aligned compliance tools that expect a single SBOM type) pass `--sbom-type` (US3) — the docs explicitly point them there.
- **Empty SBOM** (no components emitted): mikebom may produce an SBOM with zero components (e.g., scan of an empty directory). Today `metadata.lifecycles[]` is omitted entirely (`metadata_omits_lifecycles_when_no_tiers_present` test at `cyclonedx/metadata.rs:808`). Document that empty-SBOM has no SBOM-type signal — operators interpreting the absence MUST know what it means.
- **mikebom version mismatch**: an operator may inspect an SBOM emitted by an older mikebom version that predates milestone 047's lifecycle aggregation. Document the version in which native-field SBOM-type signaling was introduced + how to detect missing-signal vs explicit-no-signal.
- **CISA naming case mismatch**: mikebom's vocab is lowercase (`source`, `build`); CISA uses Title Case (`Source`, `Build`). The docs MUST address whether to normalize on lowercase (mikebom's choice) or Title Case (CISA's choice) in the operator-facing tables — recommend lowercase to match mikebom emission, with a "CISA equivalent" column showing Title Case alongside.
- **CDX-phase vs CISA-type label mismatch**: CDX uses `pre-build` for what CISA calls `Source`; `post-build` for `Analyzed`; `operations` for `Deployed`. mikebom's `mikebom:sbom-tier` vocab matches CISA exactly. The docs MUST contain a clear three-column mapping table (mikebom tier → CISA type → CDX phase).
- **SPDX 3 emission of SBOM type at the document level**: SPDX 3 has multiple lifecycle-related fields (`software_LifecycleScopeType` at the per-component level; possibly `Sbom.context` or `software_Sbom.<field>` at the document level). The audit MUST identify which one (if any) is the document-level "this is a Build SBOM" slot, and whether mikebom emits it natively today. If a native field exists but isn't wired, US3 (or a new milestone) closes the gap per Constitution Principle V.
- **`--sbom-type` operator assertion vs auto-detected per-component tiers**: per US3 §2, document-level operator assertion overrides aggregation; per-component tiers preserve auto-detected values. The docs MUST clarify this layering so operators don't expect the override to back-propagate to per-component annotations.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A new operator-facing reference document `docs/reference/sbom-types.md` MUST be added to the project. The document MUST contain (a) an overview of the CISA SBOM Types framework with citation, (b) a per-format field-position table mapping CDX 1.6 / SPDX 2.3 / SPDX 3 fields to the SBOM type they signal, (c) a per-format `jq` recipe for extracting the SBOM type from an emitted document, (d) the mikebom-tier ↔ CISA-type ↔ CDX-phase three-column equivalence table, (e) explicit handling of the mixed-tier edge case per the 2026-05-07 clarification: a multi-element `metadata.lifecycles[]` is presented as a **"Mixed-type SBOM"** spanning multiple CISA types (each type listed); no dominant-tier heuristic is invented; operators wanting single-type assertion are pointed at `--sbom-type` (US3), (f) explicit handling of the empty-SBOM edge case (absence of signal).
- **FR-002**: The audit produced as the milestone's research.md MUST enumerate, per format (CDX 1.6 / SPDX 2.3 / SPDX 3): (a) what mikebom currently emits as SBOM-type signal at the document level, (b) what native fields the spec offers for the same semantic, (c) whether mikebom uses the native field or a `mikebom:` parity bridge or omits the signal entirely, (d) the recommended action (no-op / wire native field / promote bridge to native / etc.) per Constitution Principle V's standards-native-precedence rule.
- **FR-003** (conditional on US3): If the milestone ships the operator-assert flag, mikebom MUST accept `--sbom-type <type>` on `mikebom sbom scan` and `mikebom trace run`, where `<type>` ∈ `{design, source, build, analyzed, deployed, runtime}`. The flag MUST override the document-level SBOM-type signal in all three formats while preserving per-component `mikebom:sbom-tier` annotations from auto-detection.
- **FR-004** (conditional on US3): When `--sbom-type` is passed, the document-level signal lands at the format's native field: CDX `metadata.lifecycles[]` becomes a single-element array with the operator-asserted phase value (overriding aggregation); SPDX 2.3 + SPDX 3 use the format-native or aggregated `comment` field per the audit's recommendation.
- **FR-005** (conditional on US4): If the milestone ships the `runtime` tier, the `mikebom:sbom-tier` enum + `tier_to_phase` mapping at `lifecycle_phases.rs:33` MUST add a `runtime` variant. The CDX phase mapping for `runtime` MUST be researched against CDX 1.6's vocab (CDX 1.6 may have `operations` as the closest equivalent, OR a more-specific phase if added in a later spec point release).
- **FR-006**: All milestone-047 byte-identity goldens for CDX 1.6 + SPDX 2.3 + SPDX 3 MUST stay byte-identical UNLESS the milestone introduces operator-asserted tier values that change the goldens (in which case the regen is the expected operator-visible change of the milestone, documented per the milestone-077/078/079/080 pattern). For the docs-only US1+US2 path, no goldens regenerate.
- **FR-007**: The `docs/reference/sbom-types.md` document MUST link from `docs/reference/identifiers.md` (cross-reference) and from the project README's "What mikebom emits" section so operators discover it via the existing docs navigation surface.
- **FR-008**: Any code-change follow-ups (US3 + US4) introduced by this milestone MUST land at standards-native field positions per Constitution Principle V. If a native field doesn't exist for a given format, a `mikebom:` parity bridge MAY be used and MUST be documented in `docs/reference/sbom-format-mapping.md` per the Principle V escape clause (analogous to the milestone-080 audit-record pattern).

### Key Entities

- **SBOM type**: A CISA-defined classification (Design / Source / Build / Analyzed / Deployed / Runtime) for the data lineage of an SBOM document. mikebom maps internal `mikebom:sbom-tier` values to this vocabulary.
- **mikebom tier**: The internal per-component vocabulary mikebom uses today (`design / source / build / analyzed / deployed`; possibly `runtime` post-milestone). One-to-one mappable with CISA SBOM types.
- **CDX lifecycle phase**: The CDX 1.6 spec-defined vocabulary (`design / pre-build / build / post-build / operations / discovery / decommission`). Some entries map cleanly to CISA (e.g., `design`); others use different label conventions (`pre-build` ≡ Source).
- **SPDX 2.3 SBOM-type field**: To be determined by Phase 0 audit. Possibly only `creationInfo.comment` (no native enum) per the milestone-047 emission path.
- **SPDX 3 SBOM-type field**: To be determined by Phase 0 audit. Candidates include `Sbom.context`, `software_Sbom.<field>`, or per-component `software_LifecycleScopeType` aggregated to document-level.
- **Operator assertion**: An optional `--sbom-type` flag value that overrides per-component aggregation at the document level.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An operator who has never seen mikebom output before can determine the SBOM type of a mikebom-emitted document by following `docs/reference/sbom-types.md` in under 60 seconds. Verified by manual smoke against a representative SBOM during the milestone's polish phase.
- **SC-002**: 100% of the per-format field claims in `docs/reference/sbom-types.md` are verifiable against the actual format specs (CDX 1.6 schema, SPDX 2.3 spec, SPDX 3 model docs). Verified by reviewer cross-check during PR review.
- **SC-003**: The audit deliverable (research.md) explicitly cites Constitution Principle V's standards-native-precedence requirement and either (a) concludes no native-field gaps exist, or (b) lists each gap with a follow-up disposition (in-milestone US3/US4 OR documented out-of-scope OR new GitHub issue filed).
- **SC-004** (conditional on US3): An operator running `mikebom sbom scan --sbom-type build --path .` sees a single-element `lifecycles[{phase: "build"}]` in CDX, format-native equivalent in SPDX 2.3 + SPDX 3, regardless of per-component tier auto-detection.
- **SC-005** (conditional on US3): The same invocation preserves per-component `mikebom:sbom-tier` annotations from auto-detection (operator override is document-level only).
- **SC-006** (conditional on US3): An invalid `--sbom-type foobar` invocation fails with a clear error naming the valid type set.
- **SC-007**: This milestone accepts `--sbom-type runtime` as a vocab value for operator self-assertion (operators with their own runtime-instrumentation pipelines outside mikebom can assert it via the flag). **Auto-detection is DEFERRED** to a separate GitHub issue per research §3 — mikebom's eBPF observes the build process, not the runtime of artifacts; auto-tagging components with `runtime` requires a real runtime-observation feature mikebom doesn't have today. Verified by (a) `--sbom-type runtime` parses successfully + (b) the deferred-work follow-up issue filed by T001(b) captures the auto-detection scope for future work.
- **SC-008**: 100% of milestone-047 byte-identity goldens for CDX 1.6 + SPDX 2.3 + SPDX 3 stay byte-identical UNLESS US3 or US4 ships AND those flags are exercised by the goldens. For the docs-only baseline (US1 + US2 only), no goldens regenerate.
- **SC-009**: The new `docs/reference/sbom-types.md` is discoverable from `docs/reference/identifiers.md` (cross-reference link) and from the project README. Verified by manual inspection of both surfaces post-merge.

## Assumptions

- The CISA SBOM Types framework (https://www.cisa.gov/sites/default/files/2023-04/sbom-types-document-508c.pdf, April 2023) is the canonical reference for the six SBOM types this milestone aligns with. If a newer revision exists at audit time, the milestone aligns with the latest stable version.
- The mikebom-tier ↔ CISA-type ↔ CDX-phase mapping table per the Edge Cases section is an audit deliverable, not a clarification needed from the user. Phase 0 audit confirms each mapping entry against the actual format specs.
- Audit-first scope: US1 (docs) + US2 (audit deliverable) are P1 and definitely ship. US3 (operator-assert flag) and US4 (runtime tier) are P2/P3 and conditionally ship based on audit findings — if the audit reveals they're needed to close a real Principle V gap or operator pain point, they're scoped into the milestone; if not, they're filed as separate GitHub issues for future milestones.
- The `--sbom-type` flag's six valid values are exactly the CISA Types: `design / source / build / analyzed / deployed / runtime`. No additional vocab. Mismatched-case forms (`Build`, `BUILD`) fail parsing — operators normalize to lowercase for consistency with mikebom's existing `mikebom:sbom-tier` vocabulary.
- Operator self-assertion via `--sbom-type` is document-level only. Per-component `mikebom:sbom-tier` annotations preserve auto-detected values; the operator override does NOT back-propagate to per-component annotations. This layering is documented explicitly in both the flag's clap help text + `docs/reference/sbom-types.md`.
- The eBPF trace path's mapping to `runtime` is an audit decision. If audit reveals eBPF-observed components are more accurately `build`-tier (because eBPF is observing the BUILD process producing artifacts, not the runtime of those artifacts), the `runtime` tier doesn't get added in this milestone and the project documents the decision.
- The milestone deliberately ships as a single PR. The docs (US1 + US2) are tightly coupled — the operator-facing reference document IS the audit deliverable in operator-friendly form. If US3 + US4 conditionally ship, they extend the same PR rather than splitting (analogous to milestone-080's tight US-bundle pattern).
- Existing milestone-047 lifecycle aggregation infrastructure (`lifecycle_phases.rs::aggregate_phases` + the per-format wirings) is preserved. This milestone EXTENDS the infrastructure where audit reveals gaps; it does NOT rewrite it.
- Downstream operators currently consuming mikebom SBOMs by reading `mikebom:sbom-tier` annotations directly continue to work post-fix. The new docs make the existing fields discoverable; they do not deprecate or remove anything.
