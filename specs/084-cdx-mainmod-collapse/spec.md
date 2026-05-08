# Feature Specification: CDX 1.6 main-module super-root collapse

**Feature Branch**: `084-cdx-mainmod-collapse`
**Created**: 2026-05-08
**Status**: Draft
**Input**: User description: "let's fix this." — referring to the orphan-ref residue surfaced by side-by-side analysis of mikebom CDX 1.6 emission against a real Go project (`hosted-guac-mgmt @ 156756e`), where the legacy short-name super-root ref persists in `dependencies[]` and `compositions[]` alongside the milestone-053 main-module PURL now used in `metadata.component.bom-ref`.

## Overview

When mikebom emits CDX 1.6 against a project that has a recognizable main-module (Go via milestone 053; cargo / npm / pip / gem / maven via milestones 064–070), the resulting document carries **two competing identifiers for the same root component**:

1. **The promoted main-module PURL** — `pkg:<ecosystem>/<full-module-path>@<version>` — correctly placed in `metadata.component.bom-ref` AND used as the `ref` of a `dependencies[]` entry that lists ~15 direct deps.
2. **A legacy short-name ref** — `<basename>@0.0.0`, e.g. `hosted-guac-mgmt@0.0.0` — left over from the pre-053 super-root code path. Appears as the `ref` of an additional `dependencies[]` entry whose `dependsOn = [<main-module PURL>]`, and as the identifier in two `compositions[]` entries (`incomplete_first_party_only` assemblies + `complete` dependencies).

The legacy short-name ref is dangling: it is **not** in `components[]`, **not** equal to `metadata.component.bom-ref`, and **not** assigned to any `bom-ref` field anywhere in the document. The single non-PURL string in an otherwise PURL-keyed graph.

Empirical observation against a real Go project SBOM (`hosted-guac-mgmt @ 156756e`):

- 48 components, all PURL-shaped bom-refs ✅
- 50 `dependencies[]` entries: 49 PURL-keyed refs + 1 orphan (`hosted-guac-mgmt@0.0.0`) ❌
- `compositions[]`: 3 entries; 2 reference the orphan ref ❌
- `metadata.component.bom-ref` = the PURL ✅

## Why this matters — spec contract violated

The CDX 1.6 schema defines `refLinkType` (the type used by `dependencies[].ref`, `dependsOn[]`, and `compositions[].assemblies[]`) verbatim as:

> "Descriptor for an element identified by the attribute 'bom-ref' in the same BOM document."

And the field-level descriptions:

- `dependencies[].ref`: "References a component or service by its bom-ref attribute"
- `dependencies[].dependsOn[]`: "The bom-ref identifiers of the components or services..."
- `compositions[].assemblies[]`: "The bom-ref identifiers of the components or services being described..."

The orphan's value (`hosted-guac-mgmt@0.0.0`) is not assigned as a `bom-ref` to any component, service, or `metadata.component`. The full universe of declared bom-refs in the document is `{metadata.component.bom-ref} ∪ components[].bom-ref` — 49 entries, all PURLs. The orphan is in none of them.

The schema does not enforce closure (no JSON-Schema-level "must resolve" rule), so the document parses. But every reference-field description is a contract that the value identifies an in-document element. The orphan violates `refLinkType` and three field-description contracts simultaneously.

The CycloneDX use-case page (`cyclonedx.org/use-cases/compositions-components/`) shows the canonical pattern: `metadata.component.bom-ref = "acme-application"` is used directly as a `compositions[].assemblies[]` target — single coherent identifier across `metadata.component`, `dependencies[]`, and `compositions[]`. That is the pattern mikebom should converge to.

## Why this matters — semantic impact on consumers

Four downstream-consumer patterns and what each does with the orphan today:

| Consumer pattern | Impact |
|---|---|
| Walk down from `metadata.component` via `dependsOn[]` | Works — orphan never visited |
| Reverse-impact analysis ("what depends on X?") | Walks `leaf → main-module-PURL → orphan → dead-end` and renders a phantom node above the real project, OR errors trying to look the orphan up |
| Topological sort from "nodes with no incoming edges" | Picks the orphan as a root candidate (no incoming edges to it), tries to look it up, fails or falls back |
| Strict CDX validators (e.g. `cyclonedx-cli validate`, Dependency-Track import) | Reports the dangling ref; pipeline gates fail |

The first pattern is forgiving and is why the bug stayed hidden. The other three break in a way that is observable in the field today (this is what surfaced the bug).

## Why this matters — root cause

Two files, one mismatch:

- `mikebom-cli/src/generate/cyclonedx/metadata.rs:391-409` — milestone-053 aware. When `main_module.is_some()` and no override is active, `metadata.component.bom-ref` becomes the PURL.
- `mikebom-cli/src/generate/cyclonedx/builder.rs:255, 297-300, 304` — pre-053. Always builds `target_ref = format!("{}@{}", effective_target_name, effective_target_version)` with `effective_target_version` falling back to a hardcoded `"0.0.0"` literal. Feeds this short-form ref into `build_dependencies` (becomes `dependencies[0].ref`) and `build_compositions` (becomes the orphan in two of three composition entries).

`metadata.rs` got the milestone-053 promotion. `builder.rs` didn't. They carry two different identities for the same node.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Strict CDX consumer sees zero dangling refs (Priority: P1)

A regulatory-compliance ingestion pipeline (or any CDX-consuming tool that runs strict ref-resolution validation: `cyclonedx-cli validate`, Dependency-Track's import path, custom ingestion systems like the team's pico) consumes a mikebom-emitted CDX 1.6 document for a Go project. Today it reports a dangling-ref warning or error on `<short-name>@0.0.0`. Post-milestone, the validation passes with zero ref-resolution warnings.

**Why this priority**: Headline operator pain — surfaced by the team's pico ingestion failing on slack-notifier output. Unblocks every strict consumer in the field today via a single-file code change.

**Independent Test**: Generate a CDX 1.6 document for any post-053 ecosystem (Go, cargo, npm, pip, gem, maven). Compute the union `S = components[].bom-ref ∪ {metadata.component.bom-ref}`. Assert `dependencies[].ref ⊆ S`, `dependencies[].dependsOn[] ⊆ S`, `compositions[].assemblies[] ⊆ S`, `compositions[].dependencies[] ⊆ S`. Equivalently: run `cyclonedx-cli validate` (already wired into CI under format-parity); assert exit 0 with no ref-resolution warnings.

**Acceptance Scenarios**:

1. **Given** a mikebom CDX 1.6 emission against a Go project with a `go.mod` declaring `module github.com/<org>/<repo>`, **When** `dependencies[].ref` set is checked against `S`, **Then** every ref resolves; zero orphans.
2. **Given** the same emission, **When** `compositions[].assemblies[]` and `compositions[].dependencies[]` arrays are checked against `S`, **Then** every entry resolves.
3. **Given** the same scenario across each of cargo, npm, pip-poetry, pip-pipfile, pip-plain, gem, maven (one fixture per ecosystem from `mikebom-cli/tests/fixtures/`), **When** the same checks run, **Then** all pass.
4. **Given** the existing format-parity gate (milestone 013), **When** it runs over regenerated goldens, **Then** parity holds.

---

### User Story 2 — Reverse-impact analysis terminates cleanly at the real root (Priority: P1)

A consumer running reverse-impact analysis ("what depends on `logrus`?") on a mikebom CDX 1.6 document needs the chain to terminate at the project root, identified by a single canonical bom-ref. Today the chain hits the orphan one step before the real PURL root and either dead-ends or renders a phantom. Post-milestone, the chain terminates at `metadata.component.bom-ref` (the main-module PURL), reachable as a `dependencies[].ref` entry — a single canonical identity.

**Why this priority**: Same code change as US1; this is the second-most-common downstream usage pattern after the forgiving "walk down from metadata.component" pattern.

**Independent Test**: Build the reverse adjacency map from `dependencies[]` (for each `(ref, dependsOn[])`, emit `dependsOn[i] -> ref` edges). Walk from any leaf component upward. Assert the walk terminates at exactly one node, and that node equals `metadata.component.bom-ref`.

**Acceptance Scenarios**:

1. **Given** a mikebom CDX 1.6 emission, **When** reverse-walk-to-root runs from any leaf component, **Then** the walk terminates at the main-module PURL (i.e., `metadata.component.bom-ref`).
2. **Given** the same emission, **When** the reverse-walk visits intermediate nodes, **Then** every intermediate ref resolves to a real component or to `metadata.component`.

---

### User Story 3 — Compositions semantics preserved on the real root (Priority: P1)

The two `compositions[]` entries that today reference the orphan ref carry meaningful CDX 1.6 claims about the project:

- `aggregate: incomplete_first_party_only` with `assemblies: [<orphan>]` says "the document's first-party assembly is incompletely enumerated (mikebom does not list internal source files)."
- `aggregate: complete` with `dependencies: [<orphan>]` says "for this root, the outgoing dependency graph is fully enumerated."

These claims are TRUE about the project. They should not be deleted — they should be **retargeted onto the real PURL ref** so consumers can correlate them with `metadata.component`.

**Why this priority**: Without this, fixing US1/US2 by simply deleting the orphan would lose the compositions semantics. P1 because it's part of the same fix and is the difference between "fix the dangling ref by deleting" (information loss) and "fix the dangling ref by collapsing onto the real root" (semantic-preserving).

**Independent Test**: Inspect `compositions[]` in a post-fix mikebom CDX 1.6 document. Assert: (a) every entry's `assemblies[]` and `dependencies[]` arrays reference real refs from `S`; (b) the `incomplete_first_party_only` aggregate's assembly equals `metadata.component.bom-ref`; (c) one `complete` aggregate's `dependencies[]` set equals the inventory of dependency components (today's first composition entry, unchanged); (d) the second `complete` aggregate's `dependencies[]` references `metadata.component.bom-ref`.

**Acceptance Scenarios**:

1. **Given** a post-fix CDX 1.6 emission, **When** `compositions[]` is inspected, **Then** the `incomplete_first_party_only` assembly references the main-module PURL.
2. **Given** the same emission, **When** the second `complete` composition is inspected, **Then** its `dependencies[]` references the main-module PURL.
3. **Given** the same emission, **When** any composition entry's refs are checked against `S`, **Then** every ref resolves.

---

### User Story 4 — Non-main-module fallback path preserved unchanged (Priority: P2)

A scan against a directory with no recognizable manifest (no `go.mod`, no `Cargo.toml` at the root, etc.) still produces a synthetic super-root today. Operators in this fallback path expect the existing `<short-name>@0.0.0` super-root behavior to keep working — the fix MUST NOT regress this path.

**Why this priority**: P2 because the fallback path is rare in real operator usage (most projects mikebom is run against have at least one detectable manifest), but the fixtures exercising it must continue to pass. The fix is conditional on `main_module.is_some()`; when it's `None`, behavior is unchanged.

**Independent Test**: Run mikebom against a fixture that has dep-database content but no main-module manifest (e.g., a bare directory with only OS package data, or a synthetic test fixture explicitly designed for the no-manifest fallback). Assert `metadata.component.bom-ref`, `dependencies[].ref` set, and `compositions[]` are all byte-identical to alpha.23 output.

**Acceptance Scenarios**:

1. **Given** a fixture with no main-module manifest, **When** mikebom emits CDX 1.6, **Then** the resulting document is byte-identical to alpha.23 output (no regression).
2. **Given** the existing `--root-component-name <name>` operator override (milestone 077), **When** an operator passes the override, **Then** the override semantics are preserved (override wins over both auto-detected main-module AND legacy super-root fallback).

---

### Edge Cases

- **`--root-component-name <name>` override active** (milestone 077): operator-supplied override wins. The override sets `metadata.component.bom-ref`; `target_ref` should follow the same identifier so dependencies/compositions remain anchored. The fix preserves existing override semantics — the conditional change is the `main_module.is_some() && override.is_none()` path only.
- **`go mod` with no `module` directive** (rare but valid): main-module detection fails, fallback path runs, US4 preserves behavior.
- **Workspace projects** (cargo workspaces, Go workspaces): the main-module-promotion code already picks one root per scan; this milestone does not change that selection — only the downstream wiring.
- **Hardcoded `0.0.0` version literal** at `builder.rs:255`: the literal is no longer reached in the main-module case (the PURL carries its own version, even if it's the `v0.0.0-unknown` Go sentinel). The literal stays for the fallback path.
- **Existing byte-identity goldens** (~27 per the alpha.10 release commit + drift through alpha.23): goldens for any post-053 ecosystem need regeneration. The diff each golden shows MUST be exclusively (a) one fewer `dependencies[]` entry (the bridge entry is gone), (b) two retargeted `compositions[]` entries — no other field changes.
- **SPDX 2.3 + SPDX 3 emission**: unaffected. SPDX uses different identifier conventions (SPDXIDs, IRIs) and a different document-subject convention (`SPDXRef-DOCUMENT` describes-relationship). Whether SPDX has analogous orphan-ref bugs is a separate audit (candidate follow-up milestone, not this one).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When `main_module.is_some()` AND no `--root-component-name` override is active, `target_ref` MUST equal `metadata.component.bom-ref` (the main-module PURL).
- **FR-002**: When `target_ref` is set per FR-001, the legacy `dependencies[]` entry that bridged the orphan-to-PURL relationship MUST NOT be emitted (deleting it eliminates the orphan; merging it would create a self-loop).
- **FR-003**: When `target_ref` is set per FR-001, the two `compositions[]` entries that previously referenced the orphan MUST be retargeted to the main-module PURL — preserving the `incomplete_first_party_only` assemblies semantics and the `complete` dependencies semantics.
- **FR-004**: When `main_module.is_none()` (no manifest detected, fallback path), the legacy super-root behavior (`<short-name>@0.0.0` as `target_ref`) MUST be preserved byte-identical to alpha.23.
- **FR-005**: When `--root-component-name` override is active (milestone 077), the override identity flows into `target_ref` (matching `metadata.component.bom-ref`), preserving override semantics; orphan refs do not appear.
- **FR-006**: Strict CDX 1.6 ref-resolution validation MUST pass on every post-fix golden: every value in `dependencies[].ref`, `dependencies[].dependsOn[]`, `compositions[].assemblies[]`, and `compositions[].dependencies[]` MUST be an element of `S = components[].bom-ref ∪ {metadata.component.bom-ref}`.
- **FR-007**: SPDX 2.3 + SPDX 3 emission MUST be unaffected by this milestone (the bug is CDX-only).
- **FR-008**: Existing milestone-053 / 064 / 066 / 068 / 069 / 070 main-module-edge tests MUST pass unmodified (the edges from main-module to direct deps stay correct; only the orphan goes away).
- **FR-009**: Existing milestone-013 format-parity tests (`mikebom parity-check`) MUST pass post-regen.
- **FR-010**: Existing milestone-077 override tests MUST pass unmodified.
- **FR-011**: A new regression test MUST exercise the closure invariant from FR-006 against at least one fixture per post-053 ecosystem (Go, cargo, npm, pip, gem, maven), so future milestones cannot reintroduce a dangling ref without test failure.

### Key Entities

- **`target_ref`** (existing internal): the bom-ref used as the project-root identifier across `dependencies[]` and `compositions[]`. Today: `<short-name>@0.0.0` (always). Post-milestone: equals `metadata.component.bom-ref` when `main_module.is_some()`; unchanged otherwise.
- **Closure set `S`**: the universe of declared bom-refs in the document, defined as `components[].bom-ref ∪ {metadata.component.bom-ref}`. Used by FR-006 to express the ref-resolution invariant in measurable terms.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Every mikebom-emitted CDX 1.6 document against any post-053 ecosystem fixture passes the closure invariant from FR-006: zero dangling refs.
- **SC-002**: Reverse-walk from any leaf component to the root terminates at exactly one node, equal to `metadata.component.bom-ref`.
- **SC-003**: Affected byte-identity goldens (post-053 ecosystem fixtures) regenerate cleanly with diffs containing only orphan-removal + composition retarget — no other field changes.
- **SC-004**: Pre-PR gate (`./scripts/pre-pr.sh`) clean: clippy zero warnings; `cargo test --workspace` 0 failed.
- **SC-005**: SPDX 2.3 + SPDX 3 goldens stay byte-identical pre/post merge (FR-007).
- **SC-006**: Strict-validator regression: when an operator emits CDX 1.6 against a Go project (e.g., `hosted-guac-mgmt @ 156756e`) and feeds it to `cyclonedx-cli validate`, the validator reports zero dangling-ref warnings on the post-fix output.
- **SC-007**: Operator-perceived improvement: the team's pico ingestion (or any equivalent strict consumer) processes the post-fix CDX 1.6 output without warnings or errors related to the project-root identifier.

## Assumptions

- The pre-053 super-root path is exercised exclusively by the no-manifest fallback today. Every Go / cargo / npm / pip / gem / maven fixture in `mikebom-cli/tests/fixtures/` exercises the main-module promotion path. Pre-PR gate confirms.
- Milestone 077 (`--root-component-name` override) already routes its identifier through `metadata.component.bom-ref`; the fix piggybacks on that path so override scenarios get the same fix for free.
- No new Cargo dependencies needed. The fix is internal refactoring of identifier-passing logic in `cyclonedx/builder.rs` plus new test code.
- Existing tests in `mikebom-cli/tests/cdx_regression.rs` cover the post-053 ecosystems sufficiently to catch behavioral regressions; FR-011 adds the explicit closure invariant.
- The fix is safe to ship in a single PR. No staged rollout, no feature flag — the orphan ref has no semantic value to consumers (it's a bug, not a feature anyone relies on).
- Milestone 083 (transitive correctness audit) is in flight on its own branch; this milestone is independent and can land before, after, or in parallel.

## Out of scope

- **SPDX 2.3 / SPDX 3 ref alignment**: SPDX uses different identifier conventions (SPDXIDs, IRIs) and a different document-subject convention. Whether SPDX has analogous orphan-ref bugs is a separate audit (candidate follow-up milestone).
- **The slack-notifier ingestion failure root cause**: this milestone fixes ONE candidate cause (dangling refs in CDX 1.6), but the working-vs-not-working SBOM divergence between commits 3640409 and 156756e is mostly explained by real code/dep changes between those commits, not by this bug. This milestone does not claim to fully resolve that ingestion issue — it eliminates one variable.
- **Renaming or removing the `target_ref` concept**: the fix retargets `target_ref`; refactoring its name or call sites is a code-quality concern outside scope.
- **Adding the project's own source files to `components[]` as first-party assemblies**: the `incomplete_first_party_only` aggregate captures that mikebom does not enumerate first-party internals. Promoting from `incomplete` to `complete` for first-party would require a source-file inventory pass that is well outside this milestone's scope.

## Dependencies

- Milestones 053, 064, 066, 068, 069, 070 (per-ecosystem main-module promotion) — all merged.
- Milestone 077 (`--root-component-name` override) — must continue to work; the fix routes through the same identifier.
- Milestone 013 format-parity gate — gates the regen.
- Milestone 016 clippy hygiene — pre-PR gate.
