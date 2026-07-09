# Feature Specification: Graph-completeness reachability signal for downstream analysis tools

**Feature Branch**: `177-graph-reachability-signal`
**Created**: 2026-07-09
**Status**: Draft
**Input**: User description: "graph-completeness reason code for transitive-edge unresolvability so downstream reachability tools know when their analysis will produce false negatives"

## Clarifications

### Session 2026-07-09

- Q: FR-002 predicate granularity — per-package or per-ecosystem? → A: **Per-package**. Fires whenever ANY design-tier component lacks a source-tier-or-higher counterpart of the same package. Matches the Edge Case resolution and is the technically correct signal for reachability consumers (transitive closure past a single unresolved dep is unwalkable regardless of neighbors). Per-ecosystem coarser semantics would silently mask real reachability gaps in mixed-tier ecosystems.
- Q: What's the "safe" tier boundary for reachability trust — analyzed-tier and higher, or source-tier and higher? → A: **Source-tier or higher** (analyzed-tier is NOT safe; treated same as design-tier for reachability purposes). Rationale: mikebom's analyzed-tier resolves component identity via hash-match against deps.dev but does NOT reliably emit the deps.dev-fetched transitive edges into the CDX `dependencies[]` graph today. Marking analyzed-tier as reachability-unreliable is honest (Constitution Principle IX) and avoids silent false negatives. This means the reason code fires for ANY design-tier OR analyzed-tier component lacking a same-package source-tier-or-higher (source/deployed/build) counterpart.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Reachability tool refuses to run against unreliable graphs (Priority: P1)

A downstream reachability tool (e.g., a "does this CVE actually reach my application code via the dep graph?" analyzer) consumes a mikebom SBOM. Before it walks the graph, it checks `mikebom:graph-completeness`. If the value is `"partial"` AND the `mikebom:graph-completeness-reason` contains a code indicating transitive-edge unresolvability, the tool either (a) refuses to run and reports "graph is not reliable for reachability analysis — the scan input needs remediation," (b) runs but flags results as low-confidence with a documented caveat, or (c) treats the affected ecosystems as opaque and only reachability-analyzes the fully-resolved ecosystems.

**Why this priority**: this is the load-bearing use case. A reachability tool that runs against a graph with silently-missing transitive edges produces silent false negatives (nothing reaches anything because nothing is connected). That's worse than no reachability answer at all — it produces confident-wrong output. Ranked P1 because it's the direct consumer-safety signal.

**Independent Test**: an operator scans a Python project with only `requirements.txt` and no lockfile. A test reachability consumer reads the emitted SBOM. Assert (a) `mikebom:graph-completeness = "partial"`, (b) `mikebom:graph-completeness-reason` value contains the transitive-edge-unresolvability code with the pypi ecosystem named. A pre-177 mikebom on the same scan input emits `"Complete"` — the difference is the milestone deliverable.

**Acceptance Scenarios**:

1. **Given** a scan target with only `requirements.txt` (no lockfile, no venv), **When** the reachability tool checks `mikebom:graph-completeness`, **Then** it reads `"partial"` and finds a reason code naming transitive-edge unresolvability + the pypi ecosystem.
2. **Given** the same scan target, **When** the reachability tool decides whether to proceed, **Then** it can machine-check the reason in one jq call: `jq -r '.metadata.properties[]? | select(.name == "mikebom:graph-completeness-reason") | .value' scan.cdx.json | grep -q 'transitive-edges-unresolvable'`.
3. **Given** a scan target where cargo is fully lockfile-resolved AND pip is constraint-only, **When** the reachability tool inspects the reason value, **Then** it can determine that cargo-scoped reachability analysis is safe but pip-scoped is not.

---

### User Story 2 — Constraint-only scans emit accurate graph-completeness signal (Priority: P1)

An operator's SBOM contains 48 design-tier pypi components (from `requirements.txt` alone). Pre-177 mikebom marks the graph `"Complete"` — technically true only in the "we didn't drop any edges we could have resolved" sense. Post-177, mikebom recognizes that entire chunks of the transitive-edge closure are unresolvable BY DESIGN (no lockfile → nothing to resolve past declaration-tier) and marks the graph `"partial"` with the appropriate reason code + affected ecosystem list.

**Why this priority**: this is the mikebom-side behavior change that enables US1. Without it, US1's reachability tool has nothing to key off. Ranked P1 alongside US1 because the two are load-bearing pair.

**Independent Test**: run mikebom against a `requirements.txt`-only fixture. Assert the emitted CDX SBOM's `mikebom:graph-completeness` value is `"partial"` (not `"Complete"`). Assert `mikebom:graph-completeness-reason` contains the transitive-edge-unresolvability code.

**Acceptance Scenarios**:

1. **Given** a scan target with ≥1 design-tier component in an ecosystem AND zero source-tier or higher components in the same ecosystem, **When** the scan completes, **Then** `mikebom:graph-completeness = "partial"` and the reason list contains a transitive-edge-unresolvability code naming that ecosystem.
2. **Given** a scan target where every design-tier component has a co-existing source-tier or higher counterpart in the same ecosystem (mixed-tier resolution succeeded), **When** the scan completes, **Then** the transitive-edge-unresolvability code IS NOT emitted for that ecosystem.
3. **Given** a fully-resolved scan (no design-tier components anywhere), **When** the scan completes, **Then** `mikebom:graph-completeness = "Complete"` and NO transitive-edge-unresolvability code appears in the reason list.
4. **Given** a scan invoked with `--offline`, **When** the scan produces design-tier components, **Then** the reason code IS still emitted — the semantic is orthogonal to offline mode (the graph is unreliable regardless of network state).

---

### User Story 3 — Mixed-tier scans surface per-ecosystem gap information (Priority: P2)

An operator scans a polyglot project where cargo has a `Cargo.lock` (fully resolved) but pip has only `requirements.txt` (constraint-only). A downstream reachability tool consuming the SBOM wants to know: which ecosystems are safe to reachability-analyze, which are opaque? The reason value's ecosystem list tells them: cargo is fine; pip is unresolvable.

**Why this priority**: enables partial reachability analysis on polyglot scans — a reachability tool can proceed against the safe ecosystems and skip/flag the unresolvable ones. Ranked P2 because the primary US1/US2 flow works with a single-ecosystem list too; the multi-ecosystem case is a refinement.

**Independent Test**: scan a polyglot fixture (cargo + lockfile AND pip + requirements.txt only). Assert the emitted `mikebom:graph-completeness-reason` value contains a transitive-edge-unresolvability code whose ecosystem detail names `pypi` but NOT `cargo`.

**Acceptance Scenarios**:

1. **Given** a polyglot scan where cargo is source-tier-resolved and pip is design-tier-only, **When** the scan completes, **Then** the reason code's ecosystem list names exactly `pypi` (or the mikebom-canonical name for the affected ecosystem, e.g., `pypi` matching the PURL type).
2. **Given** a polyglot scan where both cargo AND pip are constraint-only, **When** the scan completes, **Then** the ecosystem list names BOTH ecosystems (alphabetically sorted, deduplicated).
3. **Given** a downstream tool inspecting the reason value, **When** it parses the ecosystem list, **Then** it can safely reachability-analyze any ecosystem NOT named in the list.

---

### Edge Cases

- **Deployed-tier without source-tier**: an operator's `requirements.txt`-only project has a `.venv/` directory populated by `pip install`. The pip reader emits deployed-tier components (installed packages) from the venv. Does the reason code still fire? **Decision**: NO. Deployed-tier components carry resolved versions and the transitive-edge closure is walkable through installed metadata (`METADATA` / `RECORD` files). Deployed-tier is safe.
- **Analyzed-tier without source-tier**: a project with vendored artifacts on disk but no lockfile. mikebom emits analyzed-tier components (via hash-match against `deps.dev`). Does the reason code fire? **Decision (per Q2)**: YES — analyzed-tier is NOT considered safe for reachability trust. Hash-match resolves component identity but mikebom does NOT reliably emit the transitive-edge closure into `dependencies[]` for analyzed-tier components today. Marking these as reachability-unreliable is honest (Constitution Principle IX) and avoids silent false negatives. If a future milestone adds full deps.dev-driven edge emission for analyzed-tier, this decision can be revisited via spec amendment.
- **Single design-tier component in an otherwise-resolved ecosystem**: a pypi project has 47 source-tier components (from a lockfile) plus 1 design-tier "extras" line from a co-located `requirements-dev.txt`. Does the reason code fire? **Decision**: yes, per FR-002 — ANY design-tier component without a co-existing source-tier or higher of the SAME PURL in the same ecosystem triggers the signal. The reachability graph past that one component is unreliable even if the rest is fine.
- **Design-tier component that's actually pinned exactly** (edge case from m175): `requirements.txt` line `kaggle==1.7.5`. Still `sbom_tier = "design"` because the file is a constraint file, not a resolved lock. Does the code fire? **Decision**: yes — pin syntax does not upgrade tier per m175 spec; graph-completeness reflects tier, not pin form.
- **Empty scan target**: no components emitted at all. Does the reason code fire? **Decision**: no. The `mikebom:graph-completeness` annotation itself isn't emitted for empty scans; the reason cannot appear standalone.
- **Ecosystem-canonical naming**: the ecosystem list uses PURL-type names (`pypi`, `cargo`, `npm`, `gem`, `maven`, `composer`, `cocoapods`, `mix`, `rebar3`, etc.). Consistent with existing `mikebom:go-transitive-coverage` ecosystem-adjacent conventions.
- **Backwards compat for consumers**: existing consumers reading `mikebom:graph-completeness = "Complete"` on a design-tier-only scan currently get "Complete" (misleading). Post-177 they get "partial" + a reason. This is a semantic change — pre-177 consumers who assumed "Complete = safe for reachability" will get correct behavior; consumers who ignored the reason field will continue to (correctly, per Constitution Principle X).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The tool MUST introduce a new reason code in the closed `mikebom:graph-completeness-reason` vocabulary. The code identifies "transitive-edge unresolvability due to scan-input tier gap." The wire-code name is prose-level detail chosen at authoring time to fit the existing `kebab-case-name: detail-template` convention (e.g., `transitive-edges-unresolvable`).
- **FR-002**: The tool MUST emit `mikebom:graph-completeness = "partial"` when the scan produces ≥1 component at design-tier OR analyzed-tier (`sbom_tier ∈ {"design", "analyzed"}`) that lacks a **same-package** counterpart at source-tier or higher (`sbom_tier ∈ {"source", "deployed", "build"}`). Same-package identity is determined by PURL type + name (ignoring version, since design-tier's version is empty by definition — e.g., a design-tier `pkg:pypi/pyyaml` matches a source-tier `pkg:pypi/pyyaml@6.0.2`). Per-package granularity per Q1: a mixed-tier ecosystem with 47 source-tier + 1 unresolved design-tier component STILL fires the code, because the transitive closure past that one unresolved component is unreliable regardless of neighbors. The tier boundary is source-tier per Q2: analyzed-tier does NOT count as safe because mikebom's analyzed-tier resolves identity (hash-match against deps.dev) but does NOT reliably emit the transitive-edge closure. The affected-ecosystems list in the reason detail is derived from the union of PURL-type values across every triggering design-tier and analyzed-tier component (deduplicated, alphabetically sorted).
- **FR-003**: The reason code's detail portion MUST name the affected ecosystem(s) using PURL-type canonical names (e.g., `pypi`, `cargo`, `npm`), alphabetically sorted, deduplicated. Format precedent: the existing `MultiEcosystemPartialRoot` reason code's ecosystem-list format.
- **FR-004**: The new reason code MUST compose with existing reason codes. When multiple degradation causes co-exist (e.g., design-tier gap AND orphaned components), all applicable codes appear in the reason value, joined per the existing `join_reason_codes` semicolon convention.
- **FR-005**: The reason code emission MUST NOT be gated on `--offline`. The semantic is about SBOM output truthfulness for downstream reachability consumers; network state is orthogonal.
- **FR-006**: The `mikebom:graph-completeness` value transition semantics MUST be preserved: `"Complete" → "partial"` when this reason fires; NEVER "Complete" alongside a partial reason (existing invariant per m158/m167).
- **FR-007**: Documentation (`docs/reference/reading-a-mikebom-sbom.md`) MUST be updated to explicitly connect the graph-completeness signal to downstream reachability analysis: (a) reachability consumers should machine-check this annotation before running, (b) `"partial"` + this specific reason means "affected-ecosystem reachability will produce false negatives," (c) mixed-ecosystem scans permit partial-reachability analysis by filtering to safe ecosystems, (d) compose-with-m175 note explaining that the design-tier advisory log is the operator-UX signal while graph-completeness is the machine-attestation for downstream tools.
- **FR-008**: The `docs/reference/sbom-format-mapping.md` catalog row for `mikebom:graph-completeness-reason` (C111) MUST be updated to enumerate the new code as part of the closed vocabulary. The wire-format contract remains additive (existing consumers who don't recognize a new code should treat it as opaque diagnostic detail — Constitution Principle X-compatible).
- **FR-009**: Existing byte-identity golden regression fixtures MAY show `mikebom:graph-completeness` values flipping from `"Complete"` to `"partial"` on fixtures that contain design-tier components without a co-resolved source-tier counterpart. The delta MUST be limited to: (a) `mikebom:graph-completeness` value, (b) `mikebom:graph-completeness-reason` addition/extension. No other bytes drift. Regeneration is expected and bounded.
- **FR-010**: The new reason code MUST be added to the closed vocabulary contract in `mikebom-cli/src/generate/graph_completeness/reason_codes.rs`. Adding it counts as a spec/CHANGELOG event per the existing "closed vocabulary" governance in the m158 contract.

### Key Entities

- **Reachability graph reliability**: an attestation over the emitted dep graph that reflects whether a downstream reachability tool can walk it and produce trustworthy answers. Two states: `Complete` (walk is reliable across all ecosystems represented in the scan) or `partial` (some ecosystem has resolution gaps that will produce false negatives).
- **Ecosystem-scoped reason**: a variant of the reason-code vocabulary where the detail portion names one or more affected ecosystems. Enables downstream tools to filter their analysis scope by trustworthiness per ecosystem. Existing precedent: `MultiEcosystemPartialRoot`.
- **Traceability ladder projection into graph-completeness**: the mapping `sbom_tier` → graph-completeness contribution. Design-tier without a co-resolved higher-tier peer projects to `"partial"` because transitive-edge closure is unwalkable past design-tier components.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A downstream reachability tool checking `mikebom:graph-completeness` on a `requirements.txt`-only scan sees `"partial"` (not `"Complete"`) and the reason value contains the new transitive-edge-unresolvability code. Verified by integration test on a pip-constraint-only fixture.
- **SC-002**: A downstream tool checking `mikebom:graph-completeness` on a fully-lockfile-resolved scan sees `"Complete"` and the new reason code IS NOT present. Verified by integration test on a fully-resolved fixture (e.g., an existing cargo golden with `Cargo.lock`).
- **SC-003**: The new reason code's ecosystem-list detail correctly enumerates affected ecosystems on polyglot scans. Verified by integration test on a cargo-resolved-plus-pip-constraint fixture; the reason value names `pypi` but not `cargo`.
- **SC-004**: The new reason code composes with existing codes when multiple degradation causes co-exist. Verified by integration test on a scan where BOTH design-tier ecosystems AND orphaned components exist; both reason codes appear in the value.
- **SC-005**: The new reason code's wire-format string can be machine-detected via a single `grep -F` substring match (matching the m175 advisory-log grep-substring stability precedent). Verified by contract test asserting the substring appears verbatim in emitted `graph-completeness-reason` values.
- **SC-006**: Existing golden regression fixtures for fully-resolved scans (cargo, gem with lockfiles, etc.) stay byte-identical modulo the alpha.56→alpha.57 version bump. The `mikebom:graph-completeness` value on those goldens remains `"Complete"`. Verified by the existing byte-identity golden suite.
- **SC-007**: Existing golden regression fixtures for design-tier-containing scans (pip, composer without lockfiles) SHOW the value transition from `"Complete"` to `"partial"` + the new reason code appearing. Verified by post-regeneration diff review — the diff scope is bounded to these two annotations.
- **SC-008**: Documentation update in `reading-a-mikebom-sbom.md` §3.4 makes the reachability-consumer contract explicit — an SBOM consumer can read the subsection and correctly wire their reachability tool's gating logic within 5 minutes. Manual audit.

## Assumptions

- **Downstream reachability tools already respect `mikebom:graph-completeness`**: this milestone assumes the ecosystem of SBOM-consuming reachability tools that check `graph-completeness` before running is nonzero. If no such consumer exists, the value delivered by this milestone is preparatory (setting up the substrate for future tooling). Empirical evidence: SBOM-quality-reporter tools (sbomqs, etc.) already inspect completeness signals; reachability-specific tools are an emerging category.
- **The "safe" tier boundary is `source` OR higher (excluding analyzed-tier)**: source-tier / deployed-tier / build-tier provide resolved-version identity AND walkable transitive-edge closure (source-tier via parsed lockfile, deployed-tier via installed-metadata walks, build-tier via eBPF-traced edges). Design-tier and analyzed-tier are the specific gaps per Q2. If a future milestone extends analyzed-tier emission to include deps.dev-fetched transitive edges, the FR-002 predicate refines to include analyzed-tier as safe.
- **Ecosystem-scoped reason detail is downward-compatible**: pre-177 consumers reading `mikebom:graph-completeness-reason` who don't recognize the new code treat it as opaque diagnostic detail. Constitution Principle X-compatible per the existing "closed vocabulary is additive" governance from m158.
- **The advisory-log signal (m175) and the graph-completeness signal (m177) compose orthogonally**: m175 tells the operator "you should generate a lockfile"; m177 tells the downstream consumer "reachability analysis is unreliable on this SBOM." Both fire on the same scan condition (design-tier components exist) but serve different audiences. No consolidation is proposed.
- **Golden regeneration is expected and bounded**: SC-007 gate — the ONLY permitted deltas on affected goldens are `mikebom:graph-completeness` value + `mikebom:graph-completeness-reason` value. Consumers of the goldens (SBOM-quality benchmarks, cross-tool comparisons) receive an accurate signal upgrade — not a breaking change to unrelated fields.
- **`ecosystem` canonical naming aligns with PURL types**: `pypi`, `cargo`, `npm`, `gem`, `maven`, `composer`, `cocoapods`, `mix` (Elixir), `rebar3` (Erlang), `dart` (pub), `haskell`, etc. Matches the existing `MultiEcosystemPartialRoot` convention (verified in the m167 vocabulary expansion).
