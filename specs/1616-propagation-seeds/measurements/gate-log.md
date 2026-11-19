# T020: gate

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, default features, no `+toolchain`.

| Commit | GATE_RESULT | ok lines | non-ok lines | passed / failed | failure blocks |
|---|---|---|---|---|---|
| `5f8aa537` | pass | 417 | 0 | 9,628 / 0 | 0 |

- **FR-009:** `git diff --stat main -- '*Cargo.toml' Cargo.lock` is empty.
- **Goldens:** no repository golden changed. Every ecosystem golden has at most one main module per ecosystem, or no dev seed that reaches another project's runtime dependency.
