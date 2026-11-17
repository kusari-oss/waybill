# Pre-PR gate (T018)

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, run alone, no `+toolchain`, at `0d8d10ff`.

- **Result:** `GATE_EXIT=0` and `GATE_RESULT=pass`; the expected line `>>> all pre-PR checks passed.` is present.
- **Counts:** 411 `test result: ok` lines; 8,658 passed, 0 failed; 0 `---- … stdout ----` failure blocks.
- **Contract suite:** `orphan_explanations` 145/145 (16 contract cases plus the corpus harness's own tests, which the file includes by `#[path]`).

The first attempt was terminated by an external SIGTERM (exit 143) three minutes in, during clippy, before any check failed. It was rerun unchanged.

## Final

Run alone at `e961902b`, which holds the final code (`2aecdf6e`), the regenerated goldens and the harness updates (`2be4f95a`):
- `GATE_EXIT=0` and `GATE_RESULT=pass`; the expected line is present;
- 411 `test result: ok` lines; **8,665 passed, 0 failed**; 0 failure blocks.

Cargo files are unchanged against `main` (FR-010):
`git diff --quiet main -- Cargo.toml Cargo.lock '*/Cargo.toml'` exits 0.
