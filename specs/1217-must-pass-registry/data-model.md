# Data Model — must-pass registry (feature 1217)

Test-harness types only. Nothing here is emitted into an SBOM, and the
shipped `waybill` binary is untouched.

## `MustPass` — one recorded judgement

```rust
pub struct MustPass {
    pub invariant: String,
    pub target: String,
    pub reason: String,
    pub issue: Option<u32>,
}
```

Field for field the shape of `declarations::Declaration` **minus** `kind` and
`count_at_declaration`. Those two pin the failure a declaration accepts, and
this file accepts no failure — carrying them would invite a reader to think
the registry pins something about an outcome it merely requires.

| field | rule |
|---|---|
| `invariant` | names a live entry in `invariants::REGISTRY` |
| `target` | present in `manifest::TARGETS`, and not in `goldens::GOLDENS_ARE_FILTERED` |
| `reason` | non-empty after trimming; answers "what would a `NotApplicable` here mean?" **for this target** (FR-003, FR-013a) |
| `issue` | optional; the issue whose fix made the cell pass |

Identity is `(invariant, target)`. A cell appears at most once, and in at
most one of the two files.

## `MustPassFile` — the committed registry

```rust
pub struct MustPassFile {
    pub must_pass: Vec<MustPass>,
}
```

`#[serde(default)]` on the field, so an empty file deserializes to an empty
`Vec` rather than failing — which is what makes "empty is valid, missing is
not" expressible (FR-011).

Order is part of the model: entries ascend by `(invariant, target)`, and
validation rejects any other order. Sorting is what keeps an unrelated change
from moving an entry (FR-015).

## `Unmet` — one entry the run found unsatisfied

```rust
pub struct Unmet {
    pub invariant: String,
    pub target: String,
    pub observed: CellOutcome,
    pub reason: String,
}
```

A record rather than a formatted string, because two consumers need it
differently: `lane_failures()` renders one line per entry, and the grid needs
to know *which cells* are registered in order to mark them. A `Vec<String>`
would force the renderer to parse its own message back.

## State table — an entry against its cell

| cell outcome | in registry | result |
|---|---|---|
| `Pass` | yes | satisfied, cell marked in the grid |
| `NotApplicable` | yes | **lane fails** — the entire point |
| `Fail` | yes | lane fails (also `UndeclaredFailure` from the verdict) |
| `Error` | yes | lane fails (also `Verdict::Error`) |
| target not evaluated this run | yes | skipped (FR-009) |
| anything | no | unchanged from today |

Two failures for one cell is accepted by the spec: suppressing either risks
suppressing the wrong one.

## Validation — seven rules, all before any comparison

| # | rule | message names |
|---|---|---|
| 1 | `invariant` is live | the invariant id |
| 2 | `target` is in the manifest | the target |
| 3 | `target` is evaluable by the always-run lane | the target and that it has filtered goldens |
| 4 | `reason` non-empty after trimming | the cell |
| 5 | no duplicate `(invariant, target)` | the cell |
| 6 | entries sorted ascending | the ordering rule |
| 7 | no cell in both this file and the declarations | the cell and both files |

`validate(&MustPassFile, &DeclarationsFile) -> Vec<String>`; a non-empty
return fails the lane before any cell outcome is read (FR-004).

## Relationships to existing types

| existing | relationship |
|---|---|
| `Cell { invariant, target, outcome }` | read-only input to `unmet` |
| `CellOutcome` | unchanged; the registry reads it and changes none of its four states |
| `verdict(outcome, declared)` | **unchanged**, two inputs, eight rows (FR-008) |
| `declarations::DeclarationsFile` | second input to `validate`, for rule 7 |
| `unfalsifiable_invariants(cells)` | the precedent `unmet` mirrors: a corpus-level pass contributing its own failures |
| `report::MatrixReport` | gains the registry as a constructor input and the unmet records as a field |
| `goldens::GOLDENS_ARE_FILTERED` | the universe rule 3 checks against |

## State transitions

An entry has no lifecycle of its own. It is added by the change that makes a
cell's `Pass` load-bearing, and removed by a change that makes the cell
legitimately stop applying — which must then say why in its own diff. The
file is the state.
