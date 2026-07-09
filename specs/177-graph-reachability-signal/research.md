# Phase 0 Research: Graph-completeness reachability signal (m177)

**Feature**: 177-graph-reachability-signal
**Date**: 2026-07-09

Five research questions resolved by inspection of the existing m158/m167 graph-completeness code + code-search for `sbom_tier` assignment sites + review of the `MultiEcosystemPartialRoot` precedent shape. Every question was answerable without spawning subagents — the pattern-matches all trace to existing code.

---

## R1 — What's the correct "same-package identity" key for the FR-002 predicate?

**Decision**: **PURL type + name tuple** (`(purl.ecosystem(), purl.name())`), version ignored.

**Rationale**:
- Design-tier components have empty `version` by definition (per m005-era spec + reinforced by m175). A design-tier `pkg:pypi/pyyaml` (empty version) MUST match a source-tier `pkg:pypi/pyyaml@6.0.2` (with version) as "same package" for the FR-002 predicate to work at all.
- `Purl::ecosystem()` returns the PURL type string (e.g., `"pypi"`, `"cargo"`, `"npm"`) — already used by the existing `MultiEcosystemPartialRoot` classifier at `graph_completeness/mod.rs:243`.
- `Purl::name()` returns the package name (e.g., `"pyyaml"`). Already exposed on the `Purl` newtype in `mikebom-common`.
- Neither method requires normalization for cross-tier matching — the same package emitted at design-tier vs source-tier will have byte-identical `(ecosystem, name)` tuples by construction.

**Alternatives considered**:
- **Full PURL equality**: rejected — design-tier version is empty, source-tier version is populated, so full-PURL equality never matches, defeating the predicate.
- **PURL type + name + arch qualifier**: rejected — design-tier components emitted from `requirements.txt` don't carry arch qualifiers; introducing arch would make the match too strict.
- **Just PURL name (no type)**: rejected — a `pkg:pypi/foo` and a `pkg:cargo/foo` are semantically different packages; they must not cross-match.

---

## R2 — How does the "source-tier or higher" tier boundary check work?

**Decision**: **membership test in a closed set of tier strings** — `{"source", "deployed", "build"}`.

**Rationale**:
- `sbom_tier` is `Option<String>` on `ResolvedComponent` (per `mikebom-common/src/resolution.rs:98`). Free-form string, not an enum.
- Valid tier values today (per grep of assignment sites): `"design"` (m175 spec + 11 readers), `"source"` (m005 lockfile readers), `"analyzed"` (m005 hash-match + m055 binary readers), `"deployed"` (m055 dpkg/apk/rpm/venv readers), `"build"` (m001 eBPF trace readers).
- Q2 clarification: `"source"`, `"deployed"`, `"build"` are safe; `"design"` and `"analyzed"` are NOT.
- Implementation: `matches!(c.sbom_tier.as_deref(), Some("source") | Some("deployed") | Some("build"))`. Fast, exhaustive, no allocation.

**Alternatives considered**:
- **Introduce an `SbomTier` enum** in `mikebom-common`: rejected — larger refactor than warranted for this milestone. The string-based check is stable and matches the existing m175 pattern (`c.sbom_tier.as_deref() == Some("design")`).
- **Match all NOT-design-NOT-analyzed values** (i.e., negative predicate): rejected — a future milestone might add a new tier value (e.g., `"virtual"` for synthetic components) that shouldn't automatically count as safe. Positive membership is safer.

---

## R3 — Should the reason detail include a count of affected components?

**Decision**: **NO count. Ecosystem list only.**

**Rationale**:
- Reachability consumers need to know WHICH ecosystems to filter/skip, not HOW MANY components are affected. The count is diagnostic noise for the reachability use case.
- Existing reason-code precedents diverge:
  - `EdgeResolutionDegraded { dropped_count }` — counts DROPPED edges (a resolver-side gap; count matters for resolver diagnostics).
  - `GoTransitiveCoverageDegraded { missing_count }` — counts missing transitive edges (specific to Go's coverage metric).
  - `OrphanedComponentsDetected { orphan_count }` — counts orphan components (for m167's orphan-reason classifier cross-reference).
  - `MultiEcosystemPartialRoot { ecosystems }` — NO count; ecosystem list only. **This is the correct precedent for m177.**
- Consistency with `MultiEcosystemPartialRoot` (same shape, same semantic layer — "ecosystem-scoped gap") argues strongly against adding a count.
- If a future milestone finds count is needed for a diagnostic use case, adding it is a spec amendment + backwards-compatible extension.

**Alternatives considered**:
- **Add count**: rejected per rationale above. Consistency + reachability-consumer utility both favor no-count.
- **Add unaffected-ecosystem count instead** (safe-ecosystem count): rejected — asymmetric with existing MultiEcosystemPartialRoot; reachability consumers can compute this from full ecosystem inventory if needed.

---

## R4 — Where does the classifier fit in the existing `compute_graph_completeness` flow?

**Decision**: **AFTER the existing `MultiEcosystemPartialRoot` + `OrphanedComponentsDetected` classifications, before the final `value` computation.**

**Rationale**:
- Verified via reading `mikebom-cli/src/generate/graph_completeness/mod.rs:223–276`. Current flow:
  1. Line 223: initialize empty `reason_codes: Vec<ReasonCode>`.
  2. Lines 229–267: classify BFS-orphan-driven gaps (Q3 `MultiEcosystemPartialRoot` + Q2 `OrphanedComponentsDetected`).
  3. Lines 276–279: compute final `value` from `reason_codes.is_empty()` + `orphan_count`.
- The m177 classifier is orthogonal to BFS-orphan classification — it inspects `sbom_tier` values, not graph-reachability. It composes cleanly by appending to `reason_codes` between step 2 and step 3.
- FR-004 (composability with existing codes) is structurally guaranteed by this placement — the `join_reason_codes` semicolon-join operates on the full `reason_codes` vector uniformly.
- The classifier does NOT modify `orphan_count`, `reachable_count`, `total_count`, or `reachable_set` — those are BFS-derived and continue to reflect graph-reachability, not tier-based reachability.

**Alternatives considered**:
- **Before BFS-orphan classification**: rejected — no ordering advantage, and it might make the flow harder to reason about (BFS-derived state comes first, sbom_tier-derived state comes second).
- **Inside the `if orphan_count > 0` block**: rejected — tier-based reachability gaps are ORTHOGONAL to BFS orphans. A graph could have zero orphans (all components reachable via BFS from roots) AND still have design-tier components whose transitive closure isn't walkable. The classifier MUST fire independently.

---

## R5 — Which existing golden fixtures will regenerate?

**Decision**: **at least the `pip.{cdx,spdx,spdx3}.json` triplet; probably `composer.*.json` too; possibly others**. Enumerated deterministically by re-running each golden with the new classifier.

**Rationale**:
- The classifier fires when the fixture's scan target produces ≥1 design-tier or analyzed-tier component without a same-package source-tier+ counterpart.
- Fixture inventory (`mikebom-cli/tests/fixtures/`):
  - `pip/` — has `requirements.txt` — HIGHLY LIKELY to fire (design-tier without lockfile).
  - `composer/` — need to check; if the fixture uses `composer.json` without `composer.lock`, WILL fire.
  - `cargo/` — has `Cargo.lock` — will NOT fire (source-tier everywhere).
  - `gem/` — has `Gemfile.lock` — will NOT fire.
  - `golang/` — has `go.mod` (design-tier at manifest level) BUT the m053-era resolver produces source-tier components. Might not fire, depends on fixture shape.
  - `npm/` — has lockfile — will NOT fire.
  - `maven/` — has lockfile — will NOT fire.
  - `apk/`, `deb/`, `rpm/` — deployed-tier (installed DB) — will NOT fire.
  - `bazel/`, `cmake/` — unclear tier; verify at regen time.
- The regeneration mechanism is deterministic via the m080 `MIKEBOM_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1` env vars per m080/m175 precedent.
- SC-006 gate: fully-resolved goldens (cargo, gem, npm, maven, apk, deb, rpm, plus SPDX/SPDX3 variants) MUST stay `"complete"`. Verified by diff inspection post-regen.
- SC-007 gate: goldens that flip to `"partial"` MUST show ONLY two deltas — `mikebom:graph-completeness` value + `mikebom:graph-completeness-reason` addition.

**Alternatives considered**:
- **Pre-enumerate which goldens will regenerate**: rejected — best done empirically at regen time. Overhead of maintaining a static list is greater than the value.
- **Skip goldens that will regenerate** by wrapping the classifier in `#[cfg(not(test))]` or similar: rejected — the tests SHOULD catch the classifier firing on real fixtures. Regeneration is the right response.

---

## Summary table

| ID | Question | Decision |
|---|---|---|
| R1 | Same-package identity key? | `(purl.ecosystem(), purl.name())` — version ignored (design-tier is empty version by def) |
| R2 | Tier boundary check? | Membership test in `{"source", "deployed", "build"}`; string-based (no `SbomTier` enum introduced) |
| R3 | Include component count in detail? | NO. Ecosystem list only. `MultiEcosystemPartialRoot` precedent, matches reachability-consumer use case |
| R4 | Classifier placement in `compute_graph_completeness`? | After BFS-orphan classification, before final `value` computation. Orthogonal, composable |
| R5 | Which goldens regenerate? | At least `pip.*`; probably `composer.*`; determined empirically at regen time. SC-006/SC-007 gates apply |
