# Wire contract: Read dependency manifests in CI and docs dot-directories

Every case is a synthetic `waybill-fixture-*` tree built at test time. Each is asserted in CycloneDX, SPDX 2.3 and SPDX 3, through the parity extractors for C155 and C62.

| Case | Tree | Expected |
|---|---|---|
| D-1 | `.ci/requirements.txt` declaring `waybill-fixture-ci-a==1.0` | component present; `Optional`; C155 `ci` |
| D-2 | `.ci/metrics/requirements.txt` (parent is not `.ci`) | present; C155 `ci` (ancestor rule) |
| D-3 | `.github/requirements-tool-ci.txt` | present; C155 `ci` |
| D-4 | `docs/.sphinx/requirements.txt` | present; C155 `docs` |
| D-5 | `.CI/requirements.txt` (case) | present; C155 `ci` |
| D-6 | requirements files under `.venv/`, `.tox/`, `.cache/` and `.upstream-tests/` | none of their packages present |
| D-7 | `.ci/.venv/requirements.txt` (nested cache) | not present |
| D-8 | `.ci/all_requirements.txt`, and `dev-requirements.txt` at the root | both read; C155 `ci` and `dev` |
| D-9 | `tox-requirements.txt` at the root | not read (known gap) |
| D-10 | `.github/actions/waybill-fixture-act/package.json` (dependency `waybill-fixture-npm-a`), with a committed `node_modules/` | dependency present; `Development`; C62 `ci-directory`; nothing from `node_modules/` |
| D-11 | `waybill-fixture-both==1.0` in `.ci/requirements.txt` and in the root `requirements.txt` | one component; the runtime scope from the root declaration, and no C155 (FR-006, research R8) |
| D-12a | root `requirements.txt`, plus manifests under `.cache/`, `.venv/` and `.tox/` | same document (masked) as without those directories |
| D-12b | any tree with no listed directory and no `<scope>-requirements*.txt` | byte-identical to `main` (existing goldens; public corpus, which holds 0 such names) |
| D-12c | file inventory on; `.github/workflows/ci.yml`, `.ci/script.sh` | no file-tier component under `.github` or `.ci` |

Wire values:
- **C155** `waybill:python-req-file-scope`: unchanged closed enum `dev`/`test`/`docs`/`ci`.
- **C62** `waybill:lifecycle-scope-derivation`: open enum, gaining `ci-directory` and `docs-directory`. The catalog row lists them; the existing extractor carries them.
- **Scope:**
  - CycloneDX: `scope: excluded` for `Optional` and `Development`.
  - SPDX 2.3: `OPTIONAL_DEPENDENCY_OF` / `DEV_DEPENDENCY_OF`.
  - SPDX 3: `LifecycleScopeType` `development` for `Development`; for `Optional`, the existing annotation path.
