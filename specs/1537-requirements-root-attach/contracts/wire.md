# Wire contract: root edges and declared requirements

Every case is checked in CycloneDX 1.6, SPDX 2.3 and SPDX 3.0.1.

**"The root depends on X" means:**
- **CycloneDX:** X's `bom-ref` is in the `dependsOn` of `metadata.component`.
- **SPDX 2.3:** `root DEPENDS_ON X`, or the scope-typed reverse (`X DEV_DEPENDENCY_OF root`, and the others).
- **SPDX 3:** a `dependsOn` relationship from `rootElement` to X, carrying the same scope.

| # | Tree (synthetic `waybill-fixture-*`) | Root's direct targets, identical in all three formats | Completeness |
|---|---|---|---|
| C-1 | `requirements.txt` pinning `a`, `b` | `a`, `b` | `complete` |
| C-2 | C-1 plus one submodule `dep` | `dep`, `a`, `b` | `complete` |
| C-3 | `pyproject.toml` (`app`, declaring `c`) plus `requirements.txt` pinning `a`, `b`; the root is `app` | `c`, `a`, `b` | `complete` |
| C-4 | `requirements.txt` (`a`) plus `tools/requirements.txt` (`t`); no project anywhere | `a`, `t` | `complete` |
| C-5 | C-4 plus `tools/package.json` (an npm main module `tools-pkg`) | `a`, `tools-pkg`. `tools-pkg` depends on `t` | `complete` |
| C-6 | Submodule at `third_party/x` with its own `pyproject.toml` (`x-py`) and `requirements.txt` (`r`) | `x`; `x-py` attached by the fallback, or reached through `x`; `x-py` depends on `r` | `complete` |
| C-7 | `a` pinned in both `requirements.txt` and `tools/requirements.txt`, with `tools/package.json` | `a`, `tools-pkg`. `tools-pkg` depends on `a`. C18 `waybill:source-files` lists both files | `complete` |
| C-8 | `requirements-dev.txt` pinning `d`, plus a submodule | `dep` plus `d` with development scope: CycloneDX `scope: excluded`, SPDX 2.3 `DEV_DEPENDENCY_OF`, SPDX 3 `dependsOn` with development scope | `complete` |
| C-9 | An image-style scan, with OS packages only and a placeholder root | identical to `main`, except where an FR-005 parity correction applies; each difference names its correction | unchanged |
| C-10 | C-1 plus `--supplement-cdx` whose subject declares `s` | `s`, `a`, `b`. The supplement anchor no longer switches the fallback off in CycloneDX and SPDX 2.3 | `complete` |
| C-11 | `requirements.txt` (`a`) plus `vendor/x/requirements.txt` (`v`) | `a`, plus `v` if and only if the reader reads it today (T001 records which) | `complete` |
| C-12 | `requirements.txt` (`a`) plus `requirements-constraints.txt` (`k`) | set by T001's measurement and the spec's edge case. If `k` is read as a declaration, it is either attached like `a` or excluded with an orphan reason; the choice is recorded in research before T013 | per that choice |
| C-13 | C-2 plus a root `CMakeLists.txt` whose `FetchContent_Declare` names the submodule's repository at its pinned commit, so the two readers merge into one component (spec 1530, FR-009) | `dep` exactly once, alongside `a` and `b`: one component, one root edge | `complete` |

**C-6 is not fully determined yet.** Whether `x-py` hangs off the fallback or off the submodule depends on the fallback gate for that scan. T002 records the observed shape, and the contract is updated to it before implementation. Either way, `r` is attached to `x-py`, not to the root.

## Invariants (every fixture and every corpus document)

- **I-1, FR-006:** the root's direct-target set is the same in all three formats.
- **I-2, FR-007 and SC-003:** the verdict matches the emitted graph. Tests never recompute the classifier's seed set, because that would model the tool under test:
  - **Fixtures:** each C-case's verdict equals the expected value in the table above.
  - **Classifier:** a unit test in `graph_completeness` asserts that the root's adjacency the classifier walks equals `root_targets` for the same inputs. That makes the verdict match the emitters by construction.
  - **Corpus:** documents marked `complete` that have a component unreachable from the root alone come from T021's probe. Each must have a recorded explanation: the component is reachable from a main-module component in the same document.
- **I-3, FR-008 and research R7:** no component declared by a `requirements*.txt` is unreachable from the seed set.
- **I-4, FR-004:** a root edge exists only to one of three things:
  - a relationship target;
  - an anchor;
  - a graph-top, and only when the root has no dependency edge of its own.

## Unchanged

- **The submodule reader's components and anchors** (#1515, FR-009).
- **Scans with no declarations and a synthetic root**, such as images and OS packages (FR-005).
- **No new wire rows.** `waybill:graph-completeness`, `waybill:orphan-reason` and C18 keep their shapes.
