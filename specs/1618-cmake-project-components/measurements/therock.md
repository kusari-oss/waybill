# T026 TheRock (CI only)

**Run.** `kusari-sandbox/waybill-fixture-therock-sdk` `measure-source` run 44773738337. Branch `62cfe74e`, base (`main`) `786091a8`, TheRock `b1cae390`, with and without submodules. The run's `--root-name` override applies to both sides. An earlier run at `a82d2f7b` (44756589960) is superseded by this head. It differed in two ways: projects declared in one directory enclosed each other (fixed, P-21), and C240 was set on 10 projects (dropped, `vendored-rule.md`).

## With submodules

| | base | branch |
|---|---|---|
| Components | 3,660 | 4,214 |
| Edges | 3,692 | 5,698 |
| Unreachable non-file components | 406 | 406 |
| … of which with no orphan reason | 3 | 3 (the same three PyPI components) |
| Duplicate `bom-ref`s | 0 | 0 |
| Root targets with a PURL, CycloneDX / SPDX 2.3 / SPDX 3 | 188 / 188 / 188 | 630 / 630 / 630 |

- **Added:** 554 `cmake-project` components. Nothing else is added and nothing is removed.
- **Scope:** 197 runtime, 233 Test (`source-test-directory`), 124 Development (`source-example-directory`).
- **Versions:** 166 versioned, 366 C197 `not-declared`, 22 C238 expressions.
- **C239:** `not-reached` on 528. TheRock adds subprojects through its own functions, as research R9 predicted.
- **C241:** `not-reached` 528, `submodule-boundary` 521, `licence-file` 92, `path-segment` 35. No C240 on any project.
- **C237:** `read` is 2,090 on both sides.
  - **In-tree names:** 81 → 23. None of the remaining 23 has a project component; they are config-template-only names.
  - **In-tree declarations:** 1,091 → 327.
  - **`paired.project`:** 764 declarations, which is SC-001.
- **Root targets:** 188 → 630. Under the override the root anchors every demoted main module, the CMake projects now included (m860, research R11).
- **Two-node cycles:** 3 → 49.
  - **3 on both sides:** npm (`@babel/core`, `eslint`, `browserslist`).
  - **46 added:** each a project enclosing a project below it that `find_package`s it back, such as `hipblas ⇄ hipblas-clients`. Accepted (spec Clarifications, 2026-11-20), follow-up #1635. None are same-directory siblings.

## Without submodules

| | base | branch |
|---|---|---|
| Components | 230 | 265 (+35 `cmake-project`) |
| Edges | 230 | 215 |
| Unreachable non-file / unexplained / duplicates | 0 / 0 / 0 | 0 / 0 / 0 |
| Root targets, three formats | 144 / 144 / 210 (formats disagree) | 46 / 46 / 46 (agree) |

Edges fall by 15 because the root fallback, which attached every top-level component to an override root with no edges of its own, no longer applies: the root now has edges to the projects. The three formats now agree on the root's targets.

## Scan time (SC-007)

Median of 3 after a warm-up, with submodules: base 24.271 s, branch 24.404 s, **ratio 1.005**. The earlier run at `a82d2f7b` measured 1.002.
