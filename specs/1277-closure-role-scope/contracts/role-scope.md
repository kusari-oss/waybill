# Contract — the closure role → scope mapping

The complete mapping, with its one exemption. Every row is derived from the
**unioned** role.

| C182 role | compiler-supplied? | CDX `scope` | SPDX 2.3 | SPDX 3.0.1 | FR |
|---|---|---|---|---|---|
| `artifact-input` | either | **unset by this feature** | unchanged | unchanged | FR-002 |
| `both` | either | **unset by this feature** | unchanged | unchanged | FR-002 |
| `build-tooling` | no | `excluded` | `BUILD_TOOL_OF` | `LifecycleScopedRelationship`, scope `build` | FR-001, FR-012, FR-013 |
| `build-tooling` | **yes** | **unset** | **none added** | **none added** | FR-005, FR-014 |
| `unreferenced` | no | `excluded` | `BUILD_TOOL_OF` | `LifecycleScopedRelationship`, scope `build` | FR-004, FR-012, FR-013 |
| `unreferenced` | **yes** | **unset** | **none added** | **none added** | FR-005, FR-014 |
| absent (no role) | either | **unset** | unchanged | unchanged | FR-007 |

Every row names a **native** construct. No row is carried by a `waybill:*`
annotation, because all three formats have one for this semantic and
Principle V's parity-bridge carve-out applies only where a format lacks it
(research R6). The one `waybill:*` row this feature adds — C203 — carries
the classification's **provenance**, which no format expresses.

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

## T040 — Principle V self-audit, row by row

Principle V: a standards-native construct takes precedence over a
`waybill:` property, and the parity-bridge carve-out applies only "when one
format has the native field but another doesn't". Each row of the table
above is audited against that, and against the refreshed
`nix-closure-moat` goldens rather than against the emission code.

| Row | Native construct used | Annotation used instead? | Verdict |
|---|---|---|---|
| `artifact-input`, either | none — nothing is asserted | no | **compliant.** The absence is the claim; FR-002 forbids asserting `required`, and no annotation substitutes for it. Measured: 231 components, 9 of them scoped by the propagation, all unchanged. |
| `both`, either | none — nothing is asserted | no | **compliant**, same reasoning. Measured: 34 components, 1 propagation-scoped, all unchanged. |
| `build-tooling`, not compiler-supplied | CDX `component.scope = "excluded"`; SPDX 2.3 `BUILD_TOOL_OF`; SPDX 3.0.1 `LifecycleScopedRelationship` + `usesTool`, scope `build` | no | **compliant.** All three formats carry it natively. Measured: 80. |
| `build-tooling`, compiler-supplied | none — the exemption suppresses the claim | C203 `exempt-compiler-supplied` | **compliant, and the carve-out does not apply.** No format has a native way to say "a rule deliberately declined to classify this", so there is no native field in any format to prefer. The annotation records the *provenance of a non-claim*, which is why it is not a parity bridge around a missing field. Corpus population 0 (P9); covered by fixture `roletoolboot`. |
| `unreferenced`, not compiler-supplied | as `build-tooling` above | no | **compliant.** Measured: 17. |
| `unreferenced`, compiler-supplied | as `build-tooling` above | C203 | **compliant**, same reasoning. Corpus population 0. |
| absent (no role) | none | no | **compliant.** FR-007's fail-closed arm classifies nothing and emits nothing, so there is no claim to carry. Measured: 25 components with no role — 24 unscoped and the `nixpkgs` component, which keeps the `excluded` it already had. |

**The one annotation this milestone adds**, C203
`waybill:scope-classification-source`, was audited against the vendored
schemas in `research.md` R5 and recorded as **KEEP-NO-NATIVE** in
`docs/reference/sbom-format-mapping.md`. The classification itself is
native in all three formats and is deliberately *not* in that row; what has
no native carrier is which analysis concluded it. CDX `component.scope`'s
`meta:enum` defines its three values as claims about runtime reachability
with no provenance slot, SPDX 2.3 `relationshipType` has no provenance
member, and SPDX 3.0.1's `LifecycleScopeType` likewise carries the
classification rather than its origin.

**Measured parity across the three native carriers** (refreshed goldens):
the CDX components carrying C203, the `BUILD_TOOL_OF` subjects, and the
`usesTool` targets are the same set of 97, with exactly one root on each
side (1 distinct `relatedSpdxElement`, 1 distinct `from`). So no format
carries the classification for a component another format omits.
