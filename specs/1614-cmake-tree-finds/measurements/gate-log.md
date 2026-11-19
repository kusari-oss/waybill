# T031 gate

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, default features, no `+toolchain`.

| Commit | GATE_RESULT | ok lines | passed / failed | failure blocks | walker audit | Cause |
|---|---|---|---|---|---|---|
| `d927de78` | fail | 0 | — | — | OK | clippy `type_complexity` on `searches::parse`; no tests ran |
| `68f74005` (type alias) | fail | 412 (3 non-ok) | 9,291 / 0 in the ok binaries | 3 | OK | the CMake ecosystem goldens (`cdx_regression_cmake`, `cmake_byte_identity` ×2) gained C237 |
| `7d04a8aa` (goldens) | **pass** | 415 | 9,324 / 0 | 0 | OK | — |

**The goldens change is additions only:** 4, 6 and 8 lines across CycloneDX, SPDX 2.3 and SPDX 3, all the new `waybill:cmake-find-coverage` record. No existing line changed (`git diff --numstat`: `4 0`, `6 0`, `8 0`).

**FR-013:** `git diff main -- Cargo.toml '*/Cargo.toml' Cargo.lock` is empty. Walker audit: `OK (12 entries)`.

## Re-runs after the CI rounds

| Commit | GATE_RESULT | ok lines | passed / failed | failure blocks |
|---|---|---|---|---|
| `4d2038d8` (one parse; variable components) | pass | 415 | 9,324 / 0 | 0 |
| `99e6e2f5` (evidence after merge; module-path includes) | pass | 415 | 9,327 / 0 | 0 |

`5a6fb4c1` changes only `xtask/corpus/quality-corpus.toml` (pytorch's bounds); the quality corpus at it is green (run 44480610178).
