# T020: the pre-PR gate

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` at `7af8fae3925f5cc622f439d497da5b934bc24feb`, after `cargo clean`, with default features and no `+toolchain`:

| Run | Result | Totals |
|---|---|---|
| 1 | **pass** | 434 `test result` lines, all `ok`; **11,244 passed, 0 failed**; 0 failure blocks |

The log ends with `GATE_EXIT=0` and `GATE_RESULT=pass`, and contains `>>> all pre-PR checks passed.` once.

- `git diff f0c6706 -- Cargo.lock '*Cargo.toml'` is empty: no new dependencies.
- `git diff --stat f0c6706 -- waybill-cli/tests/fixtures/public_corpus` is empty: no corpus golden changed.

The corpus golden tests are `#[ignore]`d locally, and the aggregator refactor (T003) sits on the scan's emission path, so a `public-corpus.yml` verification dispatch (no regeneration) runs at the final commit too. Its result is recorded in the PR.
