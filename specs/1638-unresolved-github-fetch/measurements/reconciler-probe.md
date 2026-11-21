# T011: R5 reconciler probe

**Tree** (U-13b): one root `CMakeLists.txt` with two required declarations, both named `waybill-fixture-dep`, of `https://github.com/Waybill-Org/Waybill-Fixture-Dep.git`. One has `GIT_TAG v1`, the other `GIT_TAG ${WAYBILL_FIXTURE_UNSET}`.

**Decision rule, fixed in research R5 before the run:** one component means T012 is needed; two means no guard.

| Binary | Fetch-pass C231 `components` | Components emitted named `waybill-fixture-dep` |
|---|---|---|
| `main` `7f1a568a` | 2 | 2: `pkg:generic/waybill-fixture-dep` (design), `pkg:github/waybill-org/waybill-fixture-dep@v1` (source) |
| branch with T006 + T010 + T007, no reconciler change | 2 | **1**: `pkg:github/waybill-org/waybill-fixture-dep@v1` (source) |
| the same, plus T012's guard | 2 | 2: `pkg:github/waybill-org/waybill-fixture-dep` (design), `…@v1` (source) |

**Reading the results:**
- **The fetch pass emits two components in every row** (C231 `components` 2). The collapse happens later.
- **The guard is the only difference** between rows 2 and 3, and it restores both components. So the absorbing step is the design–source reconciler, as R5 predicted.
- **The survivor carried no reconciler annotation.** `waybill:source-manifests` and `waybill:requirement-ranges` were absent because the design-tier component had neither a manifest nor a range to pass on. The output alone could not have shown the cause; the toggle did.

**Outcome:** the tree collapses, so T012 is done (`resolve/reconciler.rs::is_versionless_github_fetch`). Mutation M5 stays in T016.
