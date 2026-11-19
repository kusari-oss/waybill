# Data Model: Read dependency manifests in CI and docs dot-directories

## Listed dot-directory

- **Set:** `.github`, `.ci`, `.gitlab`, `.circleci`, `.buildkite`, `.devcontainer`, `.sphinx`.
- **Matching:** against the directory name, case-insensitively, at any depth.
- **Class:** `ci` for every name except `.sphinx`, which is `docs`.
- **Where it is defined:** once, in `project_roots.rs`. It is used by:
  - the `descend_into` globs of the pip, npm and CMake registrations (R1, R2);
  - the pip scope classifier's ancestor rule (R4);
  - the R5 pass.

## Requirements file selection (pip)

A file in a project root is read if its name:
- starts with `requirements` and ends with `.txt` (as today);
- or matches `<scope>-requirements*.txt` with scope ∈ {`dev`, `test`, `docs`, `ci`} (new);
- or matches `*requirements*.txt` and lies under a listed directory (new).

The #1522 check applies to every file read. A project root is any directory holding such a file, or a `pyproject.toml`, `poetry.lock`, `Pipfile.lock` or `uv.lock` (as today).

## Requirements scope (pip, C155)

| Order | Signal | Result |
|---|---|---|
| 1 | Immediate parent directory | `docs`/`doc`/`documentation` → `docs`; `test`/`tests` → `test`; `ci`/`.ci` → `ci` |
| 2 | File name | `requirements-<scope>*` or `<scope>-requirements*` → that scope |
| 3 | New: nearest listed ancestor directory | its class (`ci` or `docs`) |
| 4 | None of the above | `Main` |

A classified file's packages get `LifecycleScope::Optional` and C155 = the scope, as today.

## CI or docs location (R5 pass, C62)

A component is in scope of the pass when:
- it has no lifecycle scope after deduplication;
- it has at least one evidence source file;
- every one of its evidence source files lies under a listed directory.

Effect:
- `lifecycle_scope = Development`;
- C62 `waybill:lifecycle-scope-derivation` = `ci-directory` if every file's nearest listed ancestor has class `ci`, else `docs-directory`.

States: a component either carries no scope (the pass may apply) or already carries one (the pass never changes it, FR-006).
