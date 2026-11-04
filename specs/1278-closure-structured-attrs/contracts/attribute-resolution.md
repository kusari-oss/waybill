# Contract: resolving a derivation's attributes

The reader-side contract. One rule, used by every consumer (FR-007), because
two consumers resolving differently would disagree about the same derivation.

## The rule

To resolve field `F` of a derivation:

1. If `env[F]` exists, the value is `env[F]`, split on whitespace for a path
   list. **This is today's behaviour, unchanged.**
2. Otherwise, if `structuredAttrs[F]` exists:
   - a JSON **array** → its string members, in order; non-string members are
     skipped individually, not fatally
   - a JSON **string** → split on whitespace, as in the flat form
   - **anything else** (boolean, number, object, null) → no paths
3. Otherwise → no paths.

Identity fields (`pname`, `version`) resolve by the same precedence, taking
the string value only; a non-string yields nothing, preserving the existing
distinction between a declared name and the fallback to the derivation's own
`name`.

## Properties this contract must hold

| # | Property | Why it is stated |
|---|---|---|
| P1 | A flat-form derivation resolves **byte-identically** to today. | The regression surface is every existing fixture and every non-nix golden. |
| P2 | A typed-form derivation resolves to the same paths its flat equivalent would. | The point of the feature. Assert on a matched pair, not on the typed form alone. |
| P3 | An empty array resolves to no paths. | `buildInputs: []` is on all 56 `source` derivations in the pinned closure. Crediting it would invent references. |
| P4 | A non-list, non-string value resolves to no paths and does not panic. | `doCheck` is a boolean, `env` a nested object, `outputChecks` a map — all co-resident with the dependency fields on real derivations. |
| P5 | Resolution never consults the format version. | FR-013. A version list is an allowlist that falls out of date. |
| P6 | A field in both shapes resolves from `env`, and the disagreement is reported. | FR-006. Unobserved on either closure measured, so the test engineers it. |

## The test that must exist, and why it is not the obvious one

The obvious test parses a typed-form fixture and asserts a field resolved.
That test passes while the feature is broken, because the store-path join is a
separate failure: output paths arrive without the `/nix/store/` prefix and the
referencing fields arrive with it, so comparing them unstripped resolves
nothing — silently, yielding a closure in which every member is unreferenced.

So the required assertion is on a **resolved reference**: a typed-form
derivation naming a member, and that member landing in `artifact-input` or
`build-tooling`. Parsing the field proves the field was read. Only the role
proves it was *used*.

Each typed-form test additionally carries the control the existing classifier
tests carry: that the fixture really holds the derivations it claims, and that
something is referenced — because a parse that produced nothing would satisfy
every "is not wrongly classified" assertion vacuously.

## What this contract does not change

- The seven fields that decide a role. Widening that set is #1346 and would
  move the census for two reasons at once.
- The store-path basename join.
- The union rule that merges build variants of one identity.
- Any behaviour of a scan without `--nix-closure`. The type this contract
  governs is constructed only from `nix derivation show` output.
