# T017: gate

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, default features, no `+toolchain`.

| Commit | GATE_RESULT | ok lines | non-ok lines | passed / failed | failure blocks |
|---|---|---|---|---|---|
| `fc035dfb` | pass | 416 | 0 | 9,480 / 0 | 0 |

- **FR-011:** `git diff --stat main -- '*Cargo.toml' Cargo.lock` is empty.
- **Goldens:** no repository golden changed. None of the ecosystem goldens has a requirements file the new rules label.
