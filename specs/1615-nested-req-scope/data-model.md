# Data model

No new types on the wire and no new catalog rows (FR-008).

## Requirements file scope (C155)

Unchanged value set: `dev`, `test`, `docs`, `ci`, or absent (runtime). Internally the existing `RequirementsScope` enum (`Main` | `Optional(&'static str)`).

## Labelling signals, in order

| # | Signal | Source | Change |
|---|---|---|---|
| 1 | the immediate parent directory | `docs`, `doc`, `documentation` / `test`, `tests` / `ci`, `.ci` | none |
| 2 | the name | `requirements-<word>*` or `<word>-requirements*`, word from R1's sets | word sets widened (FR-001, FR-006) |
| 3 | the nearest ancestor below the scan root | a listed dot-directory, **or** `docs`/`doc`/`documentation` (docs) or `test`/`tests` (test) | FR-002's names added to the same search (FR-003) |

**Invariant:** a later signal is consulted only when every earlier one gives nothing. Signal 3 sees only the path relative to the scan root. For a path outside the root, signal 3 is the dot-directory search alone, as today (R3).

## Scope word sets (R1)

| C155 | words |
|---|---|
| `dev` | `dev`, `development` |
| `test` | `test`, `tests`, `testing` |
| `docs` | `doc`, `docs`, `documentation` |
| `ci` | `ci` |
