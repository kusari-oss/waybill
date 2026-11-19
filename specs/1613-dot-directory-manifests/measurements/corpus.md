# T022–T023 corpora

## Public corpus (SC-003)

`public-corpus.yml` with `repeat=2`:

| Run | Ref | Result |
|---|---|---|
| 44426770187 | branch `b8b840fe` | success, 21 `corpus_*` target tests ok (166 tests in the suite) |
| 44412926370 | `main` | success (control) |

Every golden is unchanged, as predicted, including after R9–R11. Earlier branch rounds were also green: 44412913809 (`bc13f385`), 44418697169 (`38c4babb`) and 44423014538 (`bbeb4c5c`).

## Quality corpus (SC-006)

| Run | Ref |
|---|---|
| 44426781926 | branch `b8b840fe`, success |
| 44412953808 | `main`, control |

17 of 18 targets are identical to `main` in score, pkgs, files, edges, depth and completeness.

**python-pytorch:** pkgs 160 → 261, edges 110 → 211, sbomqs 5.72 → 5.93. Traced component by component with the harness's exact invocation (research R9, R11):
- **+101 components** from `.ci/`, `.github/` and `.devcontainer/`. All are non-runtime, except 6 that root requirements files also declare.
- **+1:** `.ci/lumen_cli`'s runtime `uv@unresolved`, labelled `Development` / `ci-directory` (R11).
- **−1:** pyproject's optional `opt-einsum@unresolved`, covered by the `.ci` pin.
- **+101 edges:** 96 from `torch`, which owns the new requirements files (spec 1537); 5 from `lumen-ci`; 1 from the document root to `lumen-ci`; less the `opt-einsum` retarget.

The first branch round (44412940263 at `bc13f385`, 44418707871 at `38c4babb`) violated pytorch's bounds. That is how R9 was found. The bounds are re-centred at ±10% of 260 and 211 (`xtask/corpus/quality-corpus.toml`); 261 is inside them.
