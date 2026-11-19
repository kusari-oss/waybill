# Research: scope propagation protects every runtime project

Traced in the source at `main` `1647ee01`. TheRock figures come from waybill's own document of CI measure-source run 44550926162 (branch `fbb168ab`, equal to `main` after #1620).

## R1. Where the seed set comes from, and when

**Finding.**
- **The call.** `cli/scan_cmd.rs` (~5354–5398) selects the root (`select_root`), takes `(seeds, per_ecosystem_root)` from `graph_completeness::bfs::component_root_seeds`, and passes `seeds` and `ecosystems_with_root` (the keys of `per_ecosystem_root`) to `propagate_non_runtime_scope`.
- **What the seed set is.** `component_root_seeds` returns the primary root, when it is a main module, plus one main module per remaining ecosystem (`pick_ecosystem_top`). Main modules are recognised by `waybill:component-role` = `main-module`.
- **What runs later.** Milestone 149's demotion (`apply_main_module_drop_or_demote`) runs only in `generate/` while documents are built, after propagation. Spec 1613's `ci_scope::apply` and spec 1533's `test_scope::apply` run at ~5760, also after.

**Decision.** At propagation time every main module still carries `waybill:component-role` = `main-module`, with or without `--root-name`. The added seeds are read from that annotation.

## R2. Phase A's semantics, and a non-runtime seed

**Finding.** `runtime_reachable` puts every seed in the immune set, then walks edges (`adj`, which already leaves out the excluded edge types) without ever stepping *into* a component that has a non-runtime scope. A non-runtime seed would therefore be immune itself, and would immunise its runtime children.

**Decision (FR-002).** An added seed must be runtime when propagation runs:
- `lifecycle_scope` is `None` or `Runtime`;
- and not every one of its evidence source files lies under a listed CI or docs dot-directory (`project_roots::listed_ancestor_class`).

The second test anticipates `ci_scope::apply`, which runs later and makes such a main module `Development`. Without the test, a CI project would protect its dependencies from propagation and then become non-runtime itself.

**Measured on TheRock's document.**
- 64 main modules. 61 are required; 3 are already non-runtime from their reader, so FR-002 excludes them:
  - `llvm-testing-tools` (a `test` requirements file);
  - `rocisa` (a PEP 735 dev group);
  - `rocm-bootstrap` (pip optional dependencies).
- None lies under a dot-directory. The dot-directory test is for other trees, and has its own fixture and mutation.

## R3. The change

**Decision.**
- **Seeds.** `propagate_non_runtime_scope` gains a set of added seeds: the runtime main modules not already in `seeds` (R2). Phase A runs from `seeds ∪ added`.
- **Unchanged:**
  - `seeds` itself, `ecosystems_with_root` and spec 1269's FR-012 gate (an ecosystem with no root still receives nothing);
  - graph completeness, which computes its own roots in `generate/` (FR-004, clarified);
  - the edge exclusions (`adj` is built once and shared, FR-003).
- **Placement.** The added seeds are computed in `scan_cmd` next to the existing call. `component_root_seeds` is not touched, because graph completeness shares it.

**Alternative rejected:** widening `component_root_seeds`. It feeds graph completeness and orphan reasons, so the change would spread to every multi-project document (FR-004).

## R4. Why the change can only move scopes toward runtime

Phase A's immune set grows: a superset of seeds over the same edges gives a superset of reachable components. Phase B assigns only to components that are reached, unscoped and not immune. So:
- the set it assigns can only shrink;
- the scope it assigns to a component that is still assigned is unchanged, because the strongest-first iteration over the same seeds is untouched.

FR-005 and SC-003 follow by construction, and the corpora check them by measurement.

## R5. The summary count (FR-006)

**Decision.** `PropagationSummary` gains `immune_by_added_seeds`: the components Phase B reached that were unscoped and not compiler-supplied, and that are immune **only** through the added seeds. Phase A is computed twice, from `seeds` and from `seeds ∪ added`, and the components in the second set but not the first are what `added` protects. Phase B counts the ones it reaches.

The count goes in the existing INFO lines. No catalog row is added (spec FR-006).

**Cost.** One more BFS over the same adjacency, linear in edges. On TheRock that is about 3,700 edges.

## R6. Measurement plan

The population is unknown (spec Background), so it is measured three ways:
- **TheRock:** two measure-source runs against `main`. Diff C42 and C62 per component; every change must be a loss of C62 `transitive` (SC-002, SC-003); and the 11 packages must be required (SC-001).
- **Public corpus:** a `regen_goldens=true` dispatch at the branch. Every golden diff is traced the same way, then the goldens are committed.
- **Quality corpus:** CI against a `main` control. The documents are compared locally for the targets that are cached, as in spec 1615.

Each trace names the added seed that protects the component.
