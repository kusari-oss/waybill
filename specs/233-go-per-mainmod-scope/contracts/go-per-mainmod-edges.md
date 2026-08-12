# Contract: Per-main-module `dependsOn` edge shape (Go)

**Feature**: 233-go-per-mainmod-scope
**Phase**: 1
**Audience**: Downstream SBOM consumers (vuln scanners, license auditors) + waybill contributors extending the Go reader.

Records the wire-observable shape of `dependencies[]` edges emitted from Go main-modules post-milestone-233. This is the authoritative contract for any future refactor of the Go graph resolver.

## Per-main-module edge invariants

For any Go main-module component `M` (identified by `waybill:component-role: "main-module"`), the emitted SBOM's `dependencies[]` MUST satisfy:

### Invariant 1 — Own-manifest scoping (FR-001)

`M.dependsOn` MAY contain a PURL `P` if and only if AT LEAST ONE of the following holds:
- `P`'s module path is directly named in `M`'s own `go.mod`'s `require` block AND the version matches what that `require` line declares (post-`replace` resolution).
- `P`'s module path appears in a `dependsOn` list of another module `Q` where `Q ∈ M.dependsOn` (transitive; the graph is walked from `M` via `M`'s own transitive closure).
- `P.name == "stdlib"` AND `P.version` matches the `go <version>` directive in `M`'s own `go.mod` (per FR-008).

Consequences:
- If module `A` requires `x/text v1.0.0` and module `B` requires `x/text v2.0.0`, then `A.dependsOn` MUST include `pkg:golang/x/text@v1.0.0` and MUST NOT include `pkg:golang/x/text@v2.0.0`. `B.dependsOn` MUST include v2.0.0 and MUST NOT include v1.0.0.
- If module `A` requires `foo v1.0.0` which requires `bar v1.0.0`, then `A.dependsOn` MUST include both `foo@1.0.0` and (via BFS from A) `bar@1.0.0`.

### Invariant 2 — Cross-main-module edges only via `replace` (FR-002)

`M.dependsOn` MUST NOT include another main-module `N`'s PURL UNLESS:
- `M`'s own `go.mod` has a `require` line naming `N`'s module path (rare — indicates M treats N as a published dependency), OR
- `M`'s own `go.mod` has a `replace` directive pointing at `N`'s project_root's filesystem path.

Consequences:
- In a 4-module workspace with no cross-`replace`s, no main-module points at any other main-module. Every main-module's `dependsOn` list contains only non-main-module Go dependencies (plus stdlib).

### Invariant 3 — Per-Go-version stdlib (FR-008)

Every emitted `pkg:golang/stdlib@<version>` component MUST have `<version>` matching a `go <version>` directive declared by at least one main-module in the scan. The union of all Go versions declared across the scan is the exact set of stdlib versions emitted.

Each main-module `M.dependsOn` MUST include exactly one stdlib component: `pkg:golang/stdlib@<M's own go-version>`.

Consequences:
- 4-module workspace with 3 modules on `go 1.24.0` and 1 on `go 1.22.5` emits 2 stdlib components (`@v1.24.0`, `@v1.22.5`).
- Each of the 3 `go 1.24.0` modules `dependsOn stdlib@v1.24.0`. The `go 1.22.5` module `dependsOn stdlib@v1.22.5`. Neither points at the other's stdlib.

### Invariant 4 — Workspace-member union on shared components (FR-004)

If a Go component `C` (not a main-module) is contributed to `M1.dependsOn` AND `M2.dependsOn` at the same version, `C.properties.waybill:workspace-member` MUST be a sorted deduplicated union of `[dir(M1), dir(M2)]`.

Consequences:
- If `hack/` and `tools/` both require `mikebomfixture/text v0.29.0`, one component is emitted with `waybill:workspace-member: ["hack", "tools"]`.

## Invariance across `--project-discovery` modes (FR-005)

For any `M` that survives project-discovery's filtering in a given mode:
- Invariants 1, 2, 3, 4 hold identically across `--project-discovery=all`, `root-only`, and `strict`.
- project-discovery may DROP main-modules from the emitted SBOM entirely; it MUST NOT rewrite an emitted main-module's edges.

## Invariance under `--offline` (FR-006)

The invariants hold identically when `--offline` is set:
- The resolver may fail to reach a transitive dep beyond `M`'s go.sum; that dep gets marked "unresolved" per m112's existing skip-reason mechanism.
- The resolver MUST NOT substitute a wrong version from a sibling module's manifests to fill the gap.

## Pre-milestone-233 violations (regression signatures)

The four invariants above are violated by the pre-233 code in the following observed ways (each is a regression signature future contributors MUST watch for):

| Invariant | Pre-233 violation | Detection |
|---|---|---|
| 1 | Every main-module's `dependsOn` for `x/text` names the LAST-processed version, regardless of which module actually declares it | Reporter's 4-module fixture: all four modules point at `x/text@v0.25.0` |
| 2 | Every main-module's `dependsOn` includes every OTHER main-module | Same fixture: root points at deepthing/hack/tools; hack points at tools; tools points at root; deepthing points at hack |
| 3 | Single `stdlib` component per scan regardless of mixed Go versions | Would surface only in mixed-version fixtures; not observed in reporter's uniformly-`1.24.0` repro |
| 4 | (Untested pre-233; may or may not hold) | Requires two modules requiring the same version to test |
