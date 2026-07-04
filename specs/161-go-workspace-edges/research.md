# Research: Milestone 161 (Go workspace-mode false dep-graph edges)

**Date**: 2026-07-04
**Feature**: [spec.md](./spec.md)
**Plan**: [plan.md](./plan.md)

Phase-0 outline of unknowns + design decisions. Three ambiguities were resolved in `/speckit-clarify` (Q1–Q3, see spec §Clarifications). This research resolves the remaining plan-time technical questions.

## R1 — Semantic distinction of `mikebom:go-workspace-mode` (C112) vs existing doc-scope annotations

**Decision**: **Distinct.** C112 answers a different consumer question than any existing document-scope annotation:

- **`mikebom:graph-completeness` (C104, milestone 158)** — Document-scope. "Did we successfully build a top-level component graph in this scan?" Ecosystem-neutral.
- **`mikebom:go-transitive-coverage` (C110, milestone 160)** — Document-scope. "For the Go modules we DID discover, what fraction had their transitive requires resolved via the milestone-055 ladder?" Go-specific ladder-attribution.
- **`mikebom:go-workspace-mode` (C112, this milestone)** — Document-scope. "Did the scanned Go repo use go.work workspace mode, and how many `use`d modules did we discover?" Go-specific workspace-detection.

Concrete non-overlap example: a `test-kubernetes` scan post-161 could emit:
- C104 = `complete` (graph built successfully)
- C110 = `complete` (all Go modules had their transitive edges resolved)
- C112 = `detected: 47 use-modules` (workspace-mode was detected with 47 use members)

All three annotations carry independent information and would each be checked by a different consumer switch statement.

**Rationale**: Consumers gating on workspace-mode detection today have no signal. Providing C112 as a distinct annotation preserves backward compatibility for C104/C110 (their semantics don't change) while adding the new signal.

**Alternatives considered**:

- **A. Extend C110 with workspace-mode information**: rejected — would conflate ladder-attribution with workspace-detection; consumers with pinned reason-code allowlists would break.
- **B. Overload C104 with workspace-mode-related reason codes**: rejected — C104 is the top-level graph existence signal; adding workspace-specific reasons is off-axis.
- **C. Introduce C112 (chosen)**: additive, backward-compatible, semantically distinct.

## R2 — `go.work` file grammar + parser design

**Decision**: Line-based Rust parser in `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs`, stdlib-only. Mirrors the existing `parse_go_mod` structure at `legacy.rs:200`.

Grammar (per `go help work` and Go 1.18+ documentation):

```text
go_work        ::= go_line? (use_directive | replace_directive | comment)*
go_line        ::= "go" WS <go-version>
use_directive  ::= "use" WS use_paths
use_paths      ::= use_path                                       # single-line form
                 | "(" WS_MULTILINE use_path* WS_MULTILINE ")"    # block form
use_path       ::= <path string> [comment]
replace_directive ::= "replace" WS <old_path>[@<old_ver>] "=>" <new_path>[@<new_ver>]
comment        ::= "//" <text-to-EOL>
```

Parser state machine:

- Line-by-line iteration with a `mode` variable tracking `Toplevel | InUseBlock | InReplaceBlock`.
- Comments (`//` to EOL) stripped before token analysis.
- Empty lines ignored.
- On malformed line: emit a `WorkspaceMode::Malformed(reason)` value with the first-error message; continue parsing to accumulate all use directives (fail-transparent, matching the milestone-055 pattern).

**Rationale**: The go.work grammar is small (~4 directive types + comment handling). A stdlib parser is ~150 lines. Zero new deps.

**Alternatives considered**:

- **A. Use an existing Go-toolchain-compatible parser crate**: no such Rust crate exists as of 2026-07-04.
- **B. Shell out to `go env GOWORK` + `go list -m -json` for workspace membership**: rejected — requires the `go` binary at scan time, defeats the milestone-055 offline-mode support (mikebom already parses `go.mod` in-process).
- **C. Line-based stdlib parser (chosen)**: matches the existing `parse_go_mod` pattern; testable in isolation; zero deps.

## R3 — Per-`use`d-module isolation via `GOWORK=off`

**Decision**: When workspace-mode is detected, mikebom invokes `go mod graph` in each `use`d module's directory with `GOWORK=off` set in the subprocess environment. This tells the Go toolchain to treat that module as a standalone unit rather than participating in workspace-mode graph merging.

Concrete subprocess invocation:

```rust
std::process::Command::new("go")
    .args(["mod", "graph"])
    .current_dir(&use_module_dir)
    .env("GOWORK", "off")
    .output()
```

Reference: Go's `go help environment` documents `GOWORK=off` as the explicit override for workspace-mode disabling. Setting it via `Command::env` overrides any parent-process env value.

**Rationale**: This is the documented Go-toolchain-supported mechanism for isolating a single module's dep graph in a workspace context. No new subprocess patterns needed — extends the existing milestone-055 `run_go_mod_graph` at `go_mod_graph.rs:81` with an env-override flag.

**Alternatives considered**:

- **A. Parse `use`d modules' `go.mod` files directly (no subprocess)**: rejected as primary path — mikebom's step 1 (`go mod graph`) is milestone-055's authoritative resolution mechanism; short-circuiting it would lose MVS resolution semantics. However, this becomes the FALLBACK path when `go` binary is unavailable (`--offline` mode).
- **B. Delete the workspace's `go.work` file before scanning**: rejected — mutates the scanned filesystem, violates Constitution principles.
- **C. `GOWORK=off` env-override (chosen)**: standards-compliant, non-mutating, existing subprocess pattern.

## R4 — Q1 hybrid disposition implementation

**Decision**: Post-resolution sweep inside `legacy::read`, after the per-project-root `ModuleGraphMap` is built. For each candidate edge whose target has version `v0.0.0-unknown`:

1. Look up the target module path in the workspace's `use_modules_map: HashMap<String, PathBuf>` (populated at go.work parse time).
2. If target is workspace-internal AND source's own go.mod's require block names the target module path:
   - Parse the sibling `use`d module's `go.mod` to find the target's declared version.
   - Rewrite the edge's target from `<target>@v0.0.0-unknown` to `<target>@<declared-version>` (typically `v0.0.0` per Go's default for workspace-internal modules).
3. Else:
   - Drop the edge from the emitted `depends` list (SUPPRESS).

Code shape:

```rust
fn dispose_workspace_internal_edges(
    edges: &mut Vec<(ModuleId, ModuleId)>,
    source_go_mod: &GoModDocument,
    use_modules_map: &HashMap<String, PathBuf>,
    sibling_go_mods: &HashMap<PathBuf, GoModDocument>,
) {
    edges.retain(|(source, target)| {
        // Only classify edges to workspace-internal targets with v0.0.0-unknown.
        if !use_modules_map.contains_key(target.path()) || target.version() != "v0.0.0-unknown" {
            return true;
        }
        // Source's own require block must name the target (FR-002).
        source_go_mod.requires.iter().any(|r| r.path == target.path())
    });
    // Q1 resolution arm: rewrite version for edges we keep.
    for (_, target) in edges.iter_mut() {
        if let Some(sibling_path) = use_modules_map.get(target.path()) {
            if let Some(sibling_doc) = sibling_go_mods.get(sibling_path) {
                if let Some(declared_version) = sibling_doc.module_version() {
                    *target = ModuleId::new(target.path(), declared_version);
                }
            }
        }
    }
}
```

**Rationale**: Encodes Q1's hybrid rule directly. Testable in isolation via T024–T032 unit tests.

**Alternatives considered**:

- **A. Do the disposition at the graph_resolver level rather than post-resolution**: rejected — the resolver doesn't have visibility into workspace context (which modules are workspace-internal). Doing it in `legacy::read` where workspace context IS available keeps concerns separate.
- **B. Only suppress; never resolve**: rejected per Q1 clarification.

## R5 — SC-001 verification methodology (per Q3)

**Decision**: New gated integration test at `mikebom-cli/tests/go_workspace_edges_audit.rs`. Gated behind `MIKEBOM_WORKSPACE_EDGES_AUDIT=1` env var per the milestone-083/160 external-tool test convention.

Test flow:

1. Locate the `test-kubernetes` fixture via `MIKEBOM_FIXTURES_DIR/go/workspace-kubernetes/`.
2. Parse the fixture's `go.work` file to enumerate `use`d modules.
3. For each `use`d module M:
   - Shell out: `cd $M && GOWORK=off go mod graph`.
   - Parse output into `HashSet<(source, target)>` of direct edges from M.
4. Invoke the release binary with `mikebom sbom scan --path <fixture>` producing a CDX SBOM.
5. Extract mikebom's per-workspace-module edges from `dependencies[].dependsOn[]`.
6. For each `use`d module M:
   - Compute `wrong_edges = |mikebom_edges(M) \ go_mod_graph_edges(M)|` (edges mikebom emits but ground-truth doesn't).
   - Track total wrong edges across all workspace members.
7. Assert `total_wrong_edges / total_ground_truth_edges ≤ 0.05` (SC-001) with a diagnostic-friendly failure message listing 20 sample wrong edges.

**Rationale**: Per Q3, per-`use`d-module `GOWORK=off go mod graph` is the ground-truth generator. Direct shell-out matches milestone 160's audit pattern.

## R6 — FR-007 empirical investigation methodology

**Decision**: T014–T016 (forthcoming in tasks.md) will follow a scan-diff-hypothesize-fix loop matching milestone 160:

1. Scan `test-kubernetes` fixture; emit CDX; extract per-workspace-module `dependsOn` sets via jq.
2. For each `use`d module M, shell out `cd $M && GOWORK=off go mod graph`; diff edge sets.
3. For each wrong edge (mikebom-emit ∧ not-in-ground-truth):
   - Inspect: what is the source module in mikebom's emission? What version does the target have?
   - Check the 3 FR-007 candidate root causes in order:
     - **FR-007a** — Multi-`go.mod` walker not distinguishing workspace-root from `use`d modules. Inspect via `candidate_project_roots` output at scan time.
     - **FR-007b** — `v0.0.0-unknown` version-tell not being detected + acted upon. Inspect via edge-target version distribution.
     - **FR-007c** — Workspace-internal main-module components being emitted with synthetic versions. Inspect via component-list version audit.
4. Land the fix. Re-scan. Verify wrong-edge reduction.

Concrete anchoring: the 3 wrong edges from `k8s.io/api → kube-proxy` shape are the SC-002 spot-checks. All 3 must be suppressed post-161.

**Rationale**: Investigation-first when the exact fix isn't knowable at spec time (milestone-158/160 precedent).

## R7 — Parity catalog row allocation

**Decision**: Reserve C112 for the single new document-scope annotation, continuing the milestone-158 (C104/C105) + milestone-159 (C106/C107) + milestone-160 (C108–C111) numbering:

- **C112**: `mikebom:go-workspace-mode` (document-scope, `Directionality::SymmetricEqual`, `order_sensitive: false`)

Uses the milestone-127 macro pattern: `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` at `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`. Registration entry at `mikebom-cli/src/parity/extractors/mod.rs` in the same block as C110/C111.

**Rationale**: Continues the deterministic slot-allocation pattern established since milestone 127. No collisions with prior milestones.

## Open items (none blocking)

All research questions resolved. Ready for Phase 1 (data-model.md + contracts/).
