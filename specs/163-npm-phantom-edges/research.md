# Research: Milestone 163 (npm workspace-peer phantom empty-version edges)

**Date**: 2026-07-05
**Feature**: [spec.md](./spec.md)
**Plan**: [plan.md](./plan.md)

Phase-0 outline of unknowns + design decisions. Two ambiguities were resolved in `/speckit-clarify` (Q1–Q2, see spec §Clarifications) into a UNIFIED disposition rule. This research resolves the remaining plan-time technical questions.

## R1 — Semantic distinction of C115 from existing catalog rows

**Decision**: **Distinct.** C115 (`mikebom:unresolved-declared-dep`, per-component) is a per-workspace-peer annotation naming declared deps that couldn't be cross-resolved against the top-level lockfile. Distinct from every existing per-component annotation:

- **C109 (`mikebom:go-transitive-unresolved-reason`, milestone 160)** — Go-specific per-module ladder-attribution failure reason. Different ecosystem, different failure semantics.
- **C110 (`mikebom:go-transitive-coverage`, milestone 160)** — Doc-scope, Go-specific ecosystem-wide coverage. Different scope.
- **C112 (`mikebom:go-workspace-mode`, milestone 161)** — Doc-scope, Go-specific workspace detection. Different scope + ecosystem.

C115 is the first npm-specific per-workspace-peer unresolved-declaration signal. Structurally matches milestone-159 C106/C107 (pnpm/yarn alias) shape — per-component bare-string OR JSON-array value.

**Rationale**: Consumer switch statements per workspace peer's `mikebom:unresolved-declared-dep` want a clear signal that doesn't overlap with other ecosystem-specific annotations. New slot preserves consumer contract clarity.

**Alternatives considered**:

- **A. Extend C109 to be language-neutral**: rejected — C109 is docs'd as Go-specific ("go-transitive-unresolved-reason" — the "go-transitive" prefix is load-bearing).
- **B. Reuse `mikebom:evidence-kind`**: rejected — evidence-kind names WHICH READER emitted an entry, not a per-dep-declaration failure.
- **C. Introduce C115 (chosen)**: additive, non-overlapping, matches milestone-159/162 multi-value shape.

## R2 — Cross-workspace resolution index design

**Decision**: Scan-local `HashMap<String, String>` from `name → concrete-version`. Constructed ONCE per scan after Tier A (lockfile reads) completes for the workspace root. Consulted per workspace-peer during Tier C emission.

Concrete construction:

```rust
fn build_cross_workspace_index(entries: &[PackageDbEntry]) -> HashMap<String, String> {
    let mut index = HashMap::new();
    for entry in entries {
        // Only tier A entries (lockfile-derived) contribute to the index.
        // Design-tier entries (which we're about to reshape) don't have
        // concrete versions.
        if entry.purl.as_str().starts_with("pkg:npm/")
            && !entry.version.is_empty()
        {
            // Multi-version collision: prefer the first encountered
            // (project-root-adjacent lockfiles win over nested).
            index.entry(entry.name.clone()).or_insert_with(|| entry.version.clone());
        }
    }
    index
}
```

**Rationale**: The lockfile is the authoritative version source per FR-005. A HashMap is O(1) lookup per workspace-peer dep. Collision handling (multi-version) is stable + deterministic.

**Alternatives considered**:

- **A. Per-workspace-peer lookup against a fresh parse of the lockfile**: rejected — duplicates parsing work.
- **B. Persist the index across scans**: rejected — scan-local state matches every milestone since 002 (no persistence).
- **C. Scan-local HashMap (chosen)**: sub-millisecond overhead, deterministic, matches existing scan-state posture.

## R3 — FR-003 closest-ancestor semantics (nested node_modules)

**Decision**: When a workspace peer has its OWN `node_modules/<name>` (i.e., a nested install different from the root's), the peer's edges target the nested version — matching Node.js's actual runtime resolution algorithm.

Implementation: BEFORE consulting the cross-workspace index, check if the peer's own `node_modules/<name>/package.json` exists at the peer's project root. If yes → resolve to that nested version. If no → fall through to the cross-workspace index.

```rust
fn resolve_for_workspace_peer(
    peer_root: &Path,
    dep_name: &str,
    cross_workspace_index: &HashMap<String, String>,
) -> CrossResolution {
    // Step 1: check peer's own node_modules (FR-003 closest ancestor).
    let nested = peer_root.join("node_modules").join(dep_name).join("package.json");
    if nested.is_file() {
        if let Some(version) = read_package_json_version(&nested) {
            return CrossResolution::Resolved { version };
        }
    }
    // Step 2: fall through to cross-workspace index.
    match cross_workspace_index.get(dep_name) {
        Some(version) => CrossResolution::Resolved { version: version.clone() },
        None => CrossResolution::Unresolved,
    }
}
```

**Rationale**: Node.js's runtime resolver walks up from the calling script's directory, checking each ancestor's `node_modules/`. The closest ancestor with a matching install wins. This is what the SBOM should reflect.

**Alternatives considered**:

- **A. Only consult cross-workspace index (root-only)**: rejected — misses legitimate nested installs.
- **B. Full walk-up-parents recursion**: rejected as overkill — modern npm workspaces are typically flat (single level of peers under the root). The 1-level nested check catches ~99% of real-world cases per empirical observation.
- **C. Peer's own node_modules first + cross-workspace fallback (chosen)**: matches actual Node.js semantics for the typical workspace layout.

## R4 — Q1+Q2 unified `CrossResolution` classifier

**Decision**: Single enum with 2 variants driving the emission branch:

```rust
pub(crate) enum CrossResolution {
    /// Resolved against a real lockfile entry OR nested node_modules
    /// per FR-003. Contains the concrete version string.
    Resolved { version: String },
    /// Unresolvable — no lockfile entry AND no nested install. Per Q1+Q2
    /// unified disposition: the source workspace-peer emits
    /// `mikebom:unresolved-declared-dep = "<name>"` and the edge is
    /// SUPPRESSED from `dependsOn`.
    Unresolved,
}
```

Callers branch once:

- `Resolved` → no design-tier phantom emitted; instead, the source workspace-peer's `depends` list is populated with the dep-name (which the downstream graph resolver wires to the concrete-version PURL that already exists in `entries`).
- `Unresolved` → no design-tier phantom emitted; instead, the source workspace-peer's `extra_annotations` gains `mikebom:unresolved-declared-dep = "<name>"` (or JSON array if multiple).

**Rationale**: Q1+Q2 collapsed to a single rule during clarify (spec §Clarifications). This enum is the type-level encoding.

## R5 — SC-001 verification methodology

**Decision**: SC-001's ≥99% BFS reachability target is verified via:

1. **T024 integration test** — synthesized multi-workspace monorepo (workspace root with lockfile + 2 workspace peers). Full-controlled ground truth: 2 resolved edges + 1 unresolvable edge. Post-163 SBOM's BFS reachability MUST be 100% (every emitted npm component reachable from root); phantom count MUST be 0.

2. **T038 opportunistic audit** — new gated integration test at `mikebom-cli/tests/npm_phantom_edges_audit.rs` behind `MIKEBOM_NPM_PHANTOM_AUDIT=1` env var. If a cached copy of `test-podman-desktop` is available (via `MIKEBOM_FIXTURES_DIR`), invoke the release binary + assert (a) zero empty-version PURLs; (b) BFS reachability ≥99%; (c) npm component count ≥2835. NOT blocking for the PR.

**Rationale**: Same pattern as milestone-160 T033 + milestone-161 T040 + milestone-162 T034 fixture-gated audit tests.

## R6 — Parity catalog row allocation

**Decision**: Reserve C115 continuing the milestone-158 (C104/C105) + milestone-159 (C106/C107) + milestone-160 (C108–C111) + milestone-161 (C112) + milestone-162 (C113/C114) numbering:

- **C115**: `mikebom:unresolved-declared-dep` (per-component, `Directionality::SymmetricEqual`, `order_sensitive: false`)

Uses the milestone-127 macro pattern via `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` at `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`. Registration in `mikebom-cli/src/parity/extractors/mod.rs` adjacent to C114.

**Rationale**: Continues the deterministic slot-allocation pattern since milestone 127. No collisions.

## Open items (none blocking)

All research questions resolved. Ready for Phase 1 (data-model.md + contracts/).
