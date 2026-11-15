# Gate log (spec 1532)

## T017

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`
was run on the branch at `83a80010`.

**Result:** `GATE_RESULT=pass`, 402 `test result: ok.` lines, and 0
`---- … stdout ----` failure blocks. The log contains `>>> all pre-PR checks
passed.`

**Re-run at `00f3dd0f`,** after the location fixes M8 and M9:
`GATE_RESULT=pass`, 402 `test result: ok.` lines, 0 failure blocks.
