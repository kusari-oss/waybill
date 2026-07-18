# Feature Specification: Fix `--no-deps-dev` Flag UX — Aggregate Disable

**Feature Branch**: `207-no-deps-dev-aggregate`
**Created**: 2026-07-17
**Status**: Draft
**Input**: Issue #596 — `--no-deps-dev` flag misleading: doesn't disable deps.dev dep-graph enrichment, only license enrichment. Reporter's symptom: emitted SBOM contains `mikebom:source-files: ["deps.dev"]` components even after passing `--no-deps-dev --no-clearly-defined`.

## Background

Today mikebom has three flags controlling deps.dev enrichment:

- `--no-deps-dev` — currently disables the deps.dev *license* enrichment path only. Keeps the dep-graph enrichment active.
- `--no-deps-dev-graph` — currently disables the deps.dev *transitive dep-graph* enrichment path only. Keeps the license enrichment active.
- `--offline` — disables ALL outbound network calls (all enrichment paths).

The naming reads to operators as "no deps.dev, period," but the current semantic is "no deps.dev license lookup specifically." This mismatch surfaces when the operator inspects the emitted SBOM and finds components tagged `mikebom:source-files: ["deps.dev"]` — provenance markers correctly identifying components discovered through the deps.dev dep-graph enrichment that they thought they'd disabled.

Reporter's exact invocation:

```
mikebom sbom scan --warm-go-cache=per-workspace --no-deps-dev --no-clearly-defined --path <target> --output out.cdx.json
```

Expected result: no components sourced from deps.dev.
Observed: components with `mikebom:source-files: ["deps.dev"]` still present.

The scanner is not misbehaving. The dep-graph enrichment path IS running (as documented) and correctly stamping its provenance. The bug is CLI-side naming: `--no-deps-dev` doesn't do what its name suggests.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Aggregate disable "just works" (Priority: P1)

An operator who wants no deps.dev interaction whatsoever passes `--no-deps-dev`. Post-fix, the emitted SBOM contains ZERO components with `mikebom:source-files: ["deps.dev"]` provenance (and ZERO deps.dev-sourced license fields). The single flag disables both the license and dep-graph enrichment paths.

**Why this priority**: This is the reporter's actual pain point and the highest-leverage fix — the vast majority of operators using `--no-deps-dev` want "no deps.dev, please" and were surprised by the current behavior. Making the name match the semantic eliminates the confusion for every future operator hitting this. P1 because it's the operator-facing UX correctness issue driving the report.

**Independent Test**: Reproduce the reporter's invocation against any small project. Assert (a) scan exits 0, (b) `.components[]` contains ZERO entries with `.properties[]` matching `mikebom:source-files` = `["deps.dev"]`, (c) no license values on any component carry deps.dev provenance markers.

**Acceptance Scenarios**:

1. **Given** a project scanned with `--no-deps-dev`, **When** the operator inspects the emitted SBOM, **Then** no components carry `mikebom:source-files: ["deps.dev"]` provenance AND no license fields carry deps.dev-source markers.
2. **Given** the same project scanned WITHOUT `--no-deps-dev`, **When** the operator inspects the emitted SBOM, **Then** deps.dev-sourced components MAY appear (baseline behavior unchanged for the non-suppressed case).

---

### User Story 2 - Fine-grained sub-flags still work (Priority: P2)

An operator who wants to disable ONLY the deps.dev license enrichment (but keep the dep-graph enrichment) can still do so via a fine-grained flag. Same for the reverse case. The current `--no-deps-dev-graph` flag continues to disable only the dep-graph path; a new flag (or the existing granularity via `--enrich-sources`) covers the "license only" case.

**Why this priority**: Some operators legitimately want fine-grained control — e.g., they trust deps.dev's license data (fast, high-quality) but skip the graph fetch because it's large. Preserving fine-grained control avoids regressing existing workflows that depend on the current `--no-deps-dev` semantic. P2 because it's smaller than the P1 use case AND has a workaround (`--enrich-sources`) — but preserving it keeps this fix drama-free for existing users.

**Independent Test**: Pass the new fine-grained "no license enrichment only" flag OR `--enrich-sources deps-dev-graph,clearly-defined`. Assert (a) deps.dev-sourced dep-graph components ARE emitted, (b) license fields do NOT carry deps.dev-source markers.

**Acceptance Scenarios**:

1. **Given** an operator using the pre-fix invocation `--no-deps-dev` (old semantic: disable license only), **When** they run the new mikebom, **Then** they either (a) get the new aggregate behavior automatically with a clear WARN in stderr telling them how to restore the old semantic, OR (b) get their old semantic via a documented alias flag that continues to work.
2. **Given** an operator wants to disable deps.dev license enrichment but keep dep-graph, **When** they use the documented fine-grained flag (or `--enrich-sources`), **Then** the emitted SBOM shows license fields without deps.dev provenance but dep-graph components sourced from deps.dev.

---

### Edge Cases

- **`--offline` remains the strongest hammer**: `--offline` disables ALL enrichment (deps.dev, ClearlyDefined, anything else). No behavior change — `--offline` was always strictly more powerful than any `--no-<source>` combination.
- **`--enrich-sources` allowlist**: When the operator passes `--enrich-sources <list>`, only the listed sources run — this overrides all `--no-*` flags per the current documented behavior. The fix does NOT change `--enrich-sources` semantics.
- **Old scripts using `--no-deps-dev` as the license-only flag**: their behavior changes post-fix (aggregate disable). The fix MUST provide a migration path (either automatic warning + fine-grained alias, or a documented `--enrich-sources` incantation) so their SBOMs don't silently gain fewer components without a signal to the operator.
- **Combining `--no-deps-dev` with `--no-deps-dev-graph`**: both flags together become equivalent to the new `--no-deps-dev` alone. No error; the intent is unambiguous.
- **`--no-clearly-defined` combined with `--no-deps-dev`**: covers both major enrichment sources. Post-fix, this common pairing does what operators expect: zero enrichment from either source.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `--no-deps-dev` MUST disable BOTH the deps.dev license enrichment path AND the deps.dev transitive dep-graph enrichment path. Post-fix invocation `mikebom sbom scan --no-deps-dev --path <target>` produces an SBOM with zero components carrying deps.dev provenance in either `mikebom:source-files` or license fields.
- **FR-002**: The fine-grained fill-in flag `--no-deps-dev-graph` MUST continue to work with its current semantic (disable dep-graph enrichment only, keep license enrichment).
- **FR-003**: A NEW fine-grained fill-in flag MUST exist to cover the old `--no-deps-dev` behavior (disable license enrichment only, keep dep-graph). This flag can be named `--no-deps-dev-license` OR the same effect can be reached via `--enrich-sources deps-dev-graph,clearly-defined`. Either or both mechanisms MUST be available so operators can achieve fine-grained control if they need it.
- **FR-004**: The change MUST NOT affect the semantics of `--offline` (all-off) OR `--enrich-sources <list>` (allowlist mode). Those flags are separately specified and continue to override `--no-*` flags per the current documented behavior.
- **FR-005**: Documentation (built-in `--help` text + `docs/`) MUST be updated to reflect the new semantic. The `--help` text for `--no-deps-dev` must explicitly state "disables ALL deps.dev enrichment paths" so no future operator has to reverse-engineer this by inspecting SBOMs.
- **FR-006**: Backward-compatibility migration signal: when mikebom detects an invocation that would have behaved differently pre-fix (e.g., `--no-deps-dev` alone), it SHOULD emit a one-time INFO log line explaining the new aggregate semantic + linking to the fine-grained escape hatch. Optional but recommended per Constitution Principle X (transparency).
- **FR-007**: The change MUST NOT introduce new failure modes. Scans that would have succeeded pre-fix MUST still succeed post-fix (with possibly fewer components in the SBOM due to the aggregate suppression, per FR-001).
- **FR-008**: The `--no-deps-dev` help text MUST clarify how it composes with `--enrich-sources` (allowlist wins).

### Key Entities *(include if feature involves data)*

- **`--no-deps-dev` flag**: operator-facing CLI toggle that suppresses deps.dev enrichment. Semantic changes from "no license enrichment" (pre-fix) to "no enrichment at all from deps.dev" (post-fix).
- **`--no-deps-dev-graph` flag**: continues to exist; unchanged.
- **New fine-grained flag OR documented `--enrich-sources` recipe**: covers the "license only" case per FR-003.
- **`mikebom:source-files: ["deps.dev"]` component provenance annotation**: the reporter-visible indicator that surfaced the bug. Unchanged shape; simply won't appear when `--no-deps-dev` is passed post-fix.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Reporter's exact invocation (`mikebom sbom scan --warm-go-cache=per-workspace --no-deps-dev --no-clearly-defined --path <target>`) produces an SBOM containing ZERO components with `mikebom:source-files: ["deps.dev"]` post-fix.
- **SC-002**: The single flag `--no-deps-dev` is now sufficient for "no deps.dev, period" — operators do NOT need to remember `--no-deps-dev-graph` separately.
- **SC-003**: `mikebom sbom scan --no-deps-dev --help` documentation clearly states "disables ALL deps.dev enrichment paths (both license lookups and transitive dep-graph)."
- **SC-004**: A regression test asserts FR-001 end-to-end: scan a fixture with `--no-deps-dev` and grep the emitted SBOM for `deps.dev` — MUST be absent from all component provenance surfaces.
- **SC-005**: The scan wall-clock time with `--no-deps-dev` is either the same OR faster than the current `--no-deps-dev` invocation (since the dep-graph fetch is now also skipped — should be strictly faster in the offline-cache case).
- **SC-006**: PR description references `Closes #596`.

## Assumptions

- **The current `--no-deps-dev-graph` flag stays with its existing semantic**. If the fix chose to remove or rename it, existing scripts would break. Preserving it is the safe choice.
- **The reporter's use case ("no deps.dev at all") is the majority case**. Very few operators appear to want the fine-grained "license only" split; the workaround via `--enrich-sources` is available for them. If review surfaces that fine-grained-license-only is a significant use case, FR-003's new named flag becomes P1 rather than a workaround.
- **No new `mikebom:*` annotations**. This is a CLI semantics fix — no wire-format or emitter change. All emitter code paths continue to operate identically; the change is purely in which enrichment paths run under a given flag combination.
- **No new Cargo dependencies**. Small CLI-side change.
- **Documentation update scope**: `--help` text (via clap doc-comment) + a note in `docs/` if there's a docs entry for the flag. Small change.
- **Migration signal**: FR-006 codifies a WARN log for the changed behavior. Consumers grepping mikebom's stderr for informational patterns get advance notice; the fix ships with a clear operator-facing signal explaining the new semantic + the fine-grained escape hatch (Constitution Principle X + Principle XI DX alignment).
