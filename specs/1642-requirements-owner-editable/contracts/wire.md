# Wire contract: integration cases (`waybill-cli/tests/requirements_owner.rs`)

Every case builds a synthetic `waybill-fixture-*` tree in a temp directory and scans it in all three formats. Each case asserts:
- the stated edges, scope, C18, C120 and C62;
- `waybill sbom parity-check` agreement;
- CycloneDX validity against the vendored 1.6 schema;
- unique `bom-ref`s;
- 10 byte-identical scans.

"Control" is the result at the log-only commit, which is `main`'s output.

## Ownership (User Story 1)

| case | tree | expected | control |
|---|---|---|---|
| O-1 | #1640's tree (`devtool`) | `tool/requirements.txt`'s package has an edge from `waybill-fixture-tool@1.0.0`, none from the dev package | edge from the dev package |
| O-2 | O-1 with the dev package named `waybill-fixture-zdevtool` | same as O-1, up to the name | same as O-1 (passes on `main`; the pair proves order independence) |
| O-3 | O-1 | dev package C18 `["tool/devtool", "tool/uv.lock"]`, C120 `["tool/devtool"]`; tool C18 `["tool", "tool/uv.lock"]` | C18 `["tool/uv.lock"]`, C120 `["tool"]` |
| O-4 | the `zdevtool` order of O-1, plus `tool/devtool/requirements.txt` | its package attaches to `waybill-fixture-zdevtool` | attaches to `waybill-fixture-tool`: on `main` no candidate owns `tool/devtool`, and `tool/` goes to the tool by name order. The `devtool` order cannot be the control, because there the dev package already owns `tool/` on `main`. |
| O-5 | O-1 without `uv.lock` | unchanged from `main` | — |
| O-6 | a uv workspace (`[tool.uv.workspace]` with two members) | each member owns its own directory and keeps C40 `main-module` | — (unchanged expected) |
| O-7 | a poetry project with a same-PURL `poetry.lock` entry and a requirements file | owner is the project; C18 manifest directory first | C18 `["poetry.lock"]` first |
| O-8 | an installed `dist-info` plus the project's `pyproject.toml` (editable install) | C18 order unchanged from `main` | — |
| O-10 | O-1 with the tool's lock entry as `source = { virtual = "." }` | same as O-1 | edge from the dev package (as O-1) |
| O-11 | a uv.lock editable entry `editable = "../outside"` pointing outside the scan root, with no manifest in the tree | owns nothing; output unchanged from `main` | — (guard) |
| O-12 | a uv.lock editable entry whose `x/pyproject.toml` is absent | no main module for it, owns nothing; output unchanged from `main` | — (guard) |

FR-003's runtime step has no integration case: no scanned tree is known to produce two same-rank pip candidates after FR-001 (spec Edge Cases). It is tested at unit level in `declared_attach.rs` (T008), and that unit test kills mutation M4.

## Anchor immunity (User Story 2)

| case | tree | expected | control |
|---|---|---|---|
| A-1 | probe R4 (root `requirements.txt` declares `shared==2.0`; `app/uv.lock` dev-group `devlib` depends on it) | `shared` required, no C62 | `excluded`, C62 `transitive` |
| A-2 | A-1 with the requirements file in `app/` | `shared` required | required (passes on `main`) |
| A-3 | A-1 with the root file named `requirements-dev.txt` | `shared` Optional from the reader (spec 1537 FR-010); not a seed | Optional |
| A-4 | a top-level git submodule (a non-pip project) whose dependency a dev-scoped component also depends on | the dependency has no transitive non-runtime scope | transitive non-runtime |
| A-5 | a package reached only through a dev-group package | Optional, C62 `transitive`, as on `main` | — (unchanged expected) |
| A-6 | a runtime main module of another ecosystem with a direct dependency that a dev component also reaches | no transitive non-runtime scope | already holds on `main` (spec 1616) |

## Byte-identity (User Story 3)

| case | tree | expected |
|---|---|---|
| B-1 | one project per directory, no anchors | byte-identical to `main` |
| B-2 | spec 1537's and spec 1618's existing fixtures | unchanged |
