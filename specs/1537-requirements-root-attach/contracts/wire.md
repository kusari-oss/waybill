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
| C-3 | `pyproject.toml` (`app`, declaring `c`) plus `requirements.txt` pinning `a`, `b`; the root is `app` | `c`, `a`, `b` | `partial`, reason `transitive-edges-unresolvable: pypi` (`c` is declared without a lockfile, so `c@unresolved`); never an orphan reason |
| C-4 | `requirements.txt` (`a`) plus `tools/requirements.txt` (`t`); no project anywhere | `a`, `t` | `complete` |
| C-5 | C-4 plus `tools/package.json` (an npm main module `tools-pkg`) plus a scan-root `pyproject.toml` (`app`, no dependencies); the root is `app` | `a`, `b`, `tools-pkg`. `tools-pkg` depends on `t`. File-tier entries for bare manifests are ignored | `complete` |
| C-6 | Submodule at `third_party/x` holding its own `pyproject.toml` (`xpy`) and `requirements.txt` (`r`), plus a scan-root `pyproject.toml` (`app`); the root is `app` | `x`, `xpy`, `a`, `b`. `xpy` is a root edge, as a nested project, not reached through `x`. `xpy` depends on `r` | `complete` |
| C-7 | `a` pinned in both `requirements.txt` and `tools/requirements.txt`, with `tools/package.json` and a scan-root `pyproject.toml` (`app`) | `a`, `b`, `tools-pkg`. `tools-pkg` depends on `a`. C18 `waybill:source-files` lists both files | `complete` |
| C-8 | `requirements-dev.txt` pinning `d`, plus a submodule | `dep` plus `d` with the optional scope the reader already assigns (`classify_requirements_scope` maps `-dev`, `-test`, `-docs` and `-ci` to optional): CycloneDX `scope: excluded`, SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`, SPDX 3 `dependsOn` with the same scope | `complete` |
| C-8b | `tools/requirements-dev.txt` (`d`) with `tools/package.json` and a scan-root `pyproject.toml` | `tools-pkg` depends on `d`, typed optional (SPDX 2.3 `d OPTIONAL_DEPENDENCY_OF tools-pkg`): an owner edge keeps the file's scope, which requires the pass to run before lifecycle typing (FR-010) | `complete` |
| C-9 | An image-style scan, with OS packages only and a placeholder root | identical to `main`, except where an FR-005 parity correction applies; each difference names its correction | unchanged |
| C-10 | C-1 plus `--supplement-cdx` whose subject declares `s` | `s`, `a`, `b`. The supplement anchor no longer switches the fallback off in CycloneDX and SPDX 2.3 | `complete` |
| C-11 | `requirements.txt` (`a`, `b`) plus `vendor/x/requirements.txt` (`v`) | `a`, `b`. `v` is not a component, because the vendored file is not read (T001, research R14) | `complete` |
| C-12 | `requirements.txt` (`a`, `b`) plus `requirements-constraints.txt` (`k`) | `a`, `b`, `k`. A file named like a constraints file is read as a declaration and attached like any other (research R14) | `complete` |
| C-13 | C-2 plus a root `CMakeLists.txt` whose `FetchContent_Declare` names the submodule's repository at its pinned commit, so the two readers merge into one component (spec 1530, FR-009) | `dep` exactly once, alongside `a` and `b`: one component, one root edge | `complete` |
| C-14 | Scan-root `pyproject.toml` (`app`), `requirements.txt` (`a`, `b`) and `docs/requirements.txt` (`d`), scanned with `--root-name` (the corpus shape) | `app` only. `app` depends on `a`, `b` and `d`. Neither `a` nor `b` is a root edge (FR-002, corrected) | `complete` |
| C-15 | C-14's `app` plus a compiled `requirements.txt`: `a` via `app (pyproject.toml)`, `b` via `a`, `c` via `a` and `b` | `app` only. `app` depends on `a`; `a` on `b`, `c`; `b` on `c` (FR-002a) | `complete` |
| C-16 | `requirements.txt` (`a`, `b`) at the scan root with no scan-root project, plus `tools/package.json` (`tools-pkg`), scanned with `--root-name`. The retained `tools-pkg` is a real root edge, so the fallback gate is closed | `tools-pkg`, `a`, `b`. `a` and `b` reach the root only as declared anchors (mutation M11) | `complete` |

**C-5 to C-7 give the repository a scan-root project (T002).** With a single main module anywhere in the scan, the root selector promotes it to document subject however deep it is. At `main`, `tools/package.json` became the root of C-5, and the submodule's own `pyproject.toml` became the root of C-6. That is root selection, not root edges, and it is recorded in `measurements/repro.md`. The observed shape of C-6 is that `app` has root edges to its nested projects; at `main`, those edges close the fallback gate (the C-3 mechanism).

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
