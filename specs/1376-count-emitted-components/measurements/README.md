# Milestone 1376 — measurements

Host: aarch64-darwin. Date: 2026-11-07. Issue: #1385.

## T001 / T002 — the defect, before the change

Scanned `waybill-cli/tests/fixtures/pnpm_v9_scope` in all three formats
without `--root-name`:

```
row                       : 10
CDX .components|length    : 9   <- the documented expression, short by 1
SPDX 2.3 .packages|length : 10
SPDX 3 Package + File     : 10 + 0 = 10
root: 'pkg:npm/waybill-fixture-pnpm-v9-scope@1.0.0'  real=True  in_components=False
```

**The counts were never wrong.** All three formats emit the same ten
components and the row says ten. What was wrong was the sentence in the
C209 row claiming a consumer could recover that ten with
`.components | length`. In CycloneDX they cannot, whenever the scan finds
exactly one main module.

This is why #1385 was filed as a counting defect and is not one. It was
retitled.

## T003 — the three root shapes

| shape | produced by | root is real? | root in `components[]`? | CDX adjustment |
|---|---|---|---|---|
| synthetic | `--root-name <s>`, or no main module found | no (carries no `waybill:*` properties) | n/a | **+0** — a placeholder is not a resolved component |
| promoted | exactly one main module | yes | **no** — milestone 127 promotes it to `metadata.component` | **+1** |
| duplicated | several main modules, one at the scan root | yes | yes — the ladder elects it and it stays | **+0** |

The real/synthetic test is the presence of `waybill:*` properties; the
in-array test is `bom-ref` identity. Both are needed: neither alone
separates all three shapes.

A note for whoever writes the next fixture: a `go.work` listing `./a` and
`./b` yields a **synthetic** root, not a duplicated one. One module has to
sit at the scan root for the ladder to elect a real component.

## T005 — the test can go red

`cdx_component_total` mutated to the naive `comps`, reverted with `cp`
from a backup (never `git checkout` — on an uncommitted file that deletes
the work under test):

```
m1376_a_promoted_root_is_counted_by_the_documented_expression ... FAILED
m1376_a_synthetic_root_is_not_counted ........................... ok
m1376_a_duplicated_root_is_not_counted_twice .................... ok
m1376_the_spdx_expressions_agree_with_the_same_row .............. ok
test result: FAILED. 3 passed; 1 failed
```

Exactly one test separates the old expression from the new, and it is the
promoted case — which is the defect. The other three stay green because
`+0` is what both expressions do there. That is the correct shape, not a
weakness: it is also why the defect survived.

## T013 — byte-identity, structurally rather than by sampling

Every changed line in `waybill-cli/src/cli/scan_cmd.rs` is a comment:

```
git diff -U0 -- waybill-cli/src/cli/scan_cmd.rs \
  | grep -E '^[+-]' | grep -vE '^(\+\+\+|---)' \
  | sed -E 's/^[+-][[:space:]]*//' | grep -vE '^(//|$)'
  -> (empty)
```

The other two changed files are a Markdown document and a new test file.
No emitted byte can differ, which is a stronger statement than any single
re-emit comparison would have been.

## T015 — the corrected expression against the 16 committed corpus goldens

| | |
|---|---|
| agree | **15** |
| disagree | **1** — `pants-example-javascript` |
| no C209 | 1 — `image-postgres16` |

`pants-example-javascript`'s golden holds 303 components, **all of them
`pkg:npm`**, against a row of 304. That golden is JS-filtered per #760 /
feature 675 FR-008: it is deliberately not the whole document, so it is a
known structural exception and not a counter-example. The one missing
component is the non-npm one the filter removed.

`image-postgres16` carries 11 metadata properties and none is C209 — the
golden predates the row. Tracked separately as corpus drift; out of scope
here.

### The corroboration that matters

**All 17 corpus targets are the `synthetic` shape**, because the harness
passes `--root-name` to every one of them. The promoted case — the only
shape on which the old expression is wrong — is one CI structurally cannot
produce. That is why the false sentence survived from milestone 1367 to
now, and it is the reason the new tests are hermetic integration tests
that synthesize their own fixtures rather than corpus assertions.

## T016 — mutation check: is every clause load-bearing?

Each clause of `cdx_component_total` was removed in turn, the suite run,
and the file restored with `cp` from a backup (verified byte-identical
afterwards with `diff -q`).

| mutation | promoted | synthetic | duplicated | SPDX |
|---|---|---|---|---|
| none (as delivered) | ok | ok | ok | ok |
| drop the whole adjustment (T005) | **FAILED** | ok | ok | ok |
| M1 — drop the `waybill:*` test, so every root looks real | ok | **FAILED** | ok | ok |
| M2 — drop the `bom-ref` test, so the array is never consulted | ok | ok | **FAILED** | ok |
| M3 — make the adjustment unconditional | ok | **FAILED** | **FAILED** | ok |

Each of the three shapes has exactly one mutation that kills it and no
other. No clause is decoration, and no test is passing for a reason other
than the one it claims.

The SPDX test is green under all four because it does not use the CDX
expression — correct, and worth stating so a later reader does not mistake
its stability for insensitivity.

## T017 — the gate

```
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' m1376-prepr -- ./scripts/pre-pr.sh
  exit:   0
  result: pass
```

From `target/gate/m1376-prepr.log`, read for positive signals rather than
for the absence of the word FAILED:

- 371 targets reporting `test result: ok.`, **0 reporting anything else**
- **0** `---- … stdout ----` failure blocks
- 7,473 tests passed in total

The four new tests, enumerated rather than inferred from the total:

```
test m1376_a_duplicated_root_is_not_counted_twice ... ok
test m1376_a_promoted_root_is_counted_by_the_documented_expression ... ok
test m1376_a_synthetic_root_is_not_counted ... ok
test m1376_the_spdx_expressions_agree_with_the_same_row ... ok
```

## T018 — no golden moved

```
tracked goldens modified: 0
untracked *.actual.json diagnostics: 0
```

Five files changed, none of them a golden: the C209 row, three spec files,
`scan_cmd.rs` (comments only) and the test file. This is what the T013
diff analysis predicted, now confirmed against a full gate run rather than
assumed from it.
