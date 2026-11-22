# T019: TheRock measure-source (SC-004, SC-005)

## Run

Run 44975676604 of `measure-source` in the TheRock SDK fixture repository, dispatched `--ref main -f waybill_ref=1474a600… -f base_ref=79b3350b…`. It has one job, `measure`, which succeeded. `refs.txt`:

```
base 79b3350bbf848c5a28236c64cbdf5cb71cb98823
branch 1474a600bc444cfcc1a78350a38afa5636773c5d
therock b1cae390cc745283dce25581415353f9bc906162
```

## Time (SC-005)

| side | run 0 | run 1 | run 2 | median |
|---|---|---|---|---|
| `main` | 33.633 s | 33.663 s | 33.667 s | 33.663 s |
| branch | 33.630 s | 33.666 s | 33.711 s | 33.666 s |

The ratio of the medians, branch over `main`, is **1.000**, within the 1.05 ceiling. The warm-ups (77.8 s for `main`, the cold cache, and 34.0 s for the branch) are excluded. The no-submodule scan takes 0.958 s on `main` and 0.956 s on the branch.

## Component by component (SC-004)

Each side's CycloneDX document was compared component by component, including `metadata.component`: every property, the scope, and every dependency edge.

| mode | components (`main` / branch) | same `bom-ref` set | changed components | edges −/+ |
|---|---|---|---|---|
| no submodules | 266 / 266 | yes | 0 | 0 / 0 |
| submodules | 4,215 / 4,215 | yes | 10 | 35 / 32 |

The submodule result equals the census document comparison at the same commits (`corpus.md`). The changes, each traced:

**FR-001: a lockfile-merged main module records its own directory.**
- `pkg:pypi/tensile@5.0.0`: C18 gains `…/tensilelite`.
- `pkg:pypi/rocisa@0.1.0`: C18 gains `…/tensilelite/rocisa`, and C120 gains that directory too.
- `pkg:pypi/nccl4py@0.0.0-unknown`: C18 gains `rccl/bindings/nccl4py`.
- `pkg:pypi/llvm-testing-tools@23.0.0`: C18 and C120 gain `llvm/utils/llvm-testing-tools`. Its existing entry was a requirements entry, not a lockfile entry: `compiler/amd-llvm/libcxx/test/requirements.txt` names the package. R2 applies to any non-installed existing entry.

**FR-001: ownership follows (the edges).**
- **`tensilelite`, #1640's shape on TheRock.** The dev-group package `rocisa` claimed `rocm-libraries/projects/hipblaslt/tensilelite` from its `uv.lock` and owned it by name order. Now `tensile` does.
  - 27 edges from `rocisa` move to `tensile`: requirements packages, CMake searches and fetches declared in `tensilelite`.
  - 3 more (`filelock@3.25.2`, `hip-python`, `syrupy`) are dropped from `rocisa` with no addition, because `tensile` already had them on `main` (checked).
  - `rocisa` now owns `tensilelite/rocisa`, so the CMake project there (`pkg:generic/rocisa`) gives its 2 edges (`python`, `nanobind`) to `pkg:pypi/rocisa`, which outranks it (spec 1618). Its edge to `hip` goes to `tensile`. `rocisa` keeps 4 edges.
- **`llvm-testing-tools`** claimed `libcxx/test` through that requirements entry. It now owns its own directory, so `libcxx/test/requirements.txt` goes to the nearest enclosing owner, `pkg:pypi/llvm@0.0.0-unknown`. That moves 2 edges (`clang`, `psutil`).

**FR-005 and FR-001: scope.** Six components change scope:
- `attrs@25.4.0`, `numpy@1.24.2`, `python-dateutil@2.8.2` and `six@1.16.0` go from `excluded` (C42 `optional`, C62 `transitive`) to required. Each is declared by a runtime requirements file with no owning project, a declared root anchor, which is now a seed (FR-005). These are the census's four `anchor_immunity` components exactly. `attrs` also loses C155 `docs`: spec 1613's `ci_scope` removes that label from any component that ends runtime. It is also declared by `docs/sphinx/requirements.txt`.
- `tox` goes from `excluded` (C62 `transitive`) to required. The Optional path reached it through `rocisa`, and it is now reached from `tensile`, which is runtime (FR-001).
- `mysql-connector-python` goes from C42 `optional` to `test` (C62 `transitive`, `excluded` on both sides). The Optional path through `rocisa` is gone, and a Test path remains.

**#1640's report.** #1640 described five root-declared packages emitted `excluded`/`transitive` in a large tree with this layout. Here five packages change from `excluded`/`transitive` to required: `attrs`, `numpy`, `python-dateutil`, `six` and `tox`. The issue did not name its five, so whether these are the same five is **not verified**. Its "nine more excluded although an in-scope component depends on them" were not traced separately.

## The formats agree (FR-006)

Counted in each document (CycloneDX / SPDX 2.3 / SPDX 3):

| row | submodules, `main` → branch | no submodules |
|---|---|---|
| C62 `transitive` | 38 → 33, in all three formats | 21 → 21 |
| C42 `optional` | 548 → 542, in all three formats | 19 → 19 |

C62 falls by the 5 components that became required, and C42 `optional` falls by those 5 plus `mysql-connector-python`.

`waybill sbom parity-check` on each side's three documents gives identical summaries: 257 of 260 universal rows passing with submodules (3 gaps), and 258 of 260 without (2 gaps). The only non-universal row is A5, format-restricted. The branch adds no gap.

## Orphans

From `report.md`, unreachable components without an orphan reason: 0 on both sides, in both modes. Root targets with a PURL: 630 with submodules and 46 without, the same in every format and on both sides.
