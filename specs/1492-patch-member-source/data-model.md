# Data Model: Locally built patches are sources, not fragments

No new entity, field or annotation. Two existing rules change.

## C206 member role: evidence order (FR-010)

For a closure member that declares no `pname`, the first matching rule wins:

| # | Evidence | Role | Changed |
|---|---|---|---|
| 1 | content hash (`outputs.*.hash` or `env.outputHash`) | `source` | no |
| 2 | output listed in the `patches` attribute of a derivation in the closure | `source` | **new position**; previously checked only inside rule 4 |
| 3 | `env.text` | `scaffolding` | no |
| 4 | version-shaped name → `srcOnly` phases or a source-tree name → `source`; a check name → `check`; else `package` | `source` / `check` / `package` | `patches` clause moved to rule 2 |
| 5 | none | `fragment` | no |

The classifier stays total: every member gets exactly one of the five frozen values (FR-003).

## Edge endpoint

A member is an edge endpoint iff its role is `source`, `package` or `check`. That is unchanged.

What changes is that the endpoint answer is computed **with the closure's patch set**. Today it uses an empty one. Both the projection's stopping test and the emitter's endpoint filter use the same set, built once per closure, so they cannot disagree with each other or with the emitted C206 value (research R2).

## Effects on the pinned moat closure (expected from R1; verified by the tasks)

| | before | after |
|---|---|---|
| members `fragment` | 245 | 240 |
| members `source` (no-`pname`) | 553 | 558 |
| `fragment` members listed in `patches` | 5 | 0 |
| C184 `edge-ineligible-components` | 512 | 507 |
| C184 `edges` | 6,875 | measured (R7) |
