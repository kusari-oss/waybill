# Contract: what the operator is told

Derived from one enumerated per-member outcome rather than scattered
conditionals, so the set below is exhaustive by construction.

## Emitted (warn level, per R6, matching `yarn_lock.rs:52`)

| condition | requirement | says |
|---|---|---|
| yarn root manifest absent or unparseable | FR-006 | the reader, the path, and that development classification was skipped |
| a member manifest unreadable while siblings are fine | FR-010 | the member, and that only that member contributed nothing |
| a member under an operator-excluded path | FR-011 | the member, and that the exclusion is why it was not read |

Each is pinned by a test that fails if the diagnostic is removed (SC-006,
SC-007). A diagnostic nobody asserts is one that disappears in a refactor.

## Silent, deliberately

| condition | why |
|---|---|
| a manifest that declares no `devDependencies` | an empty result is not a degradation (FR-006) |
| a `workspaces` pattern matching a directory with no manifest | normal monorepo shape (FR-010) |
| a declaration resolving to no locked package | normal operation (FR-012), counted rather than announced — see below |

## The FR-012 count

Per research R4: the FR-006 diagnostic carries a count of declarations
skipped at step 2 of the classification contract. A log line inside an
existing surface, not a new output field, so FR-008 is intact. It exists so
that a `patch:`-wrapped development dependency surviving
`--exclude-scope dev` is a visible number rather than silence — the shape of
complaint #1297 makes.

## Not a diagnostic

bun requires no manifest on disk (FR-004), so none of the three warn
conditions can arise there. A bun project emits nothing from this contract,
and a test asserting that is worth more than it costs: it pins that the yarn
evidence path did not leak into the bun reader.
