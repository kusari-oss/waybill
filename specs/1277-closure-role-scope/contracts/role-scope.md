# Contract — the closure role → scope mapping

The complete mapping, with its one exemption. Every row is derived from the
**unioned** role.

| C182 role | compiler-supplied? | native CDX `scope` | FR |
|---|---|---|---|
| `artifact-input` | either | **unset by this feature** | FR-002 |
| `both` | either | **unset by this feature** | FR-002 |
| `build-tooling` | no | `excluded` | FR-001 |
| `build-tooling` | **yes** | **unset** | FR-005 |
| `unreferenced` | no | `excluded` | FR-004 |
| `unreferenced` | **yes** | **unset** | FR-005 |
| absent (no role) | either | **unset** | FR-007 |

"Unset by this feature" is not "unset". A member in an `artifact-input` role
may still carry `excluded` from the milestone-1269 lifecycle propagation —
nine do today, plus one `both` — and that value must survive byte-identical
(FR-002).

## Why no row ever says `required`

The schema reads an absent `scope` as `required`
(`cyclonedx-1.6.json`: `"default": "required"`), so leaving the field unset
is already the strongest claim this feature could make. It must never write
the value explicitly, because `ARTIFACT_FIELDS` is `buildInputs`,
`propagatedBuildInputs`, `depsHostHost` and nixpkgs' Haskell builder passes
test dependencies through `buildInputs` — so `artifact-input` means "nix
referenced it as an input to the thing being built", not "it is in the
artifact at runtime". `hspec` is the measured counterexample: role
`artifact-input`, and not in the shipped binary.

## The two conditions that must hold of the whole table

1. **No compiler-supplied component acquires a scope from this feature**, in any role. Invariant I9 asserts it, unamended, reading the emitted document.
2. **Every scan without `--nix-closure` is byte-identical.** No role exists, so no row applies.

## Composition with the lifecycle propagation

Both mechanisms only ever assign a non-runtime scope; neither asserts
`required`. Every combination is therefore idempotent and no precedence rule
is needed:

| | role says | propagation says | result |
|---|---|---|---|
| tooling, dev-only | `excluded` | `excluded` | `excluded` |
| tooling, runtime-reachable | `excluded` | nothing | `excluded` |
| artifact-input, dev-only | nothing | `excluded` | `excluded` |
| artifact-input, runtime-reachable | nothing | nothing | unset |

Should a future mechanism ever assert `required`, this table lapses and a
precedence rule becomes necessary. That is recorded in the spec's
Assumptions so the lapse is noticeable.
