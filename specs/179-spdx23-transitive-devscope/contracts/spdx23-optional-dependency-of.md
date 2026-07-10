# Contract: SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` Emission

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md)

## Wire-format contract

Under `--spdx2-relationship-compat=full` (default), every internal edge `(A) OptionalDependsOn (B)` MUST emit as an SPDX 2.3 relationship object:

```json
{
  "spdxElementId": "SPDXRef-<B>",
  "relationshipType": "OPTIONAL_DEPENDENCY_OF",
  "relatedSpdxElement": "SPDXRef-<A>"
}
```

**Direction reversal**: matches m052's convention for `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` — internal `(A) TypedDependsOn (B)` "A needs B for this scope" → SPDX `(B) TYPED_DEPENDENCY_OF (A)` "B is a typed dep of A". This matches the SPDX 2.3 spec's own semantic for the `*_DEPENDENCY_OF` verb family.

**SPDX 2.3 §11.1 citation** (the vocabulary entry for `OPTIONAL_DEPENDENCY_OF`):

> Is to be used when SPDXRef-A is an optional dependency of SPDXRef-B. Note this is a logical relationship to help understand the context, and it is a MAY relationship (as opposed to MUST) as a code base has a level of optional deps that may or may not be needed depending on the use case.

The mikebom internal `OptionalDependsOn` classification is generated ONLY by ecosystem readers that observe a first-class "optional-declared" manifest construct (Cargo `optional = true`, npm `optionalDependencies`, etc.). The classification is a MAY-semantic per SPDX 2.3 §11.1 — a downstream consumer knows the dep may or may not appear in a given production build.

## Basic-mode contract

Under `--spdx2-relationship-compat=basic` (m228 escape hatch), every internal `OptionalDependsOn` edge MUST fall through to natural-direction `DEPENDS_ON`:

```json
{
  "spdxElementId": "SPDXRef-<A>",
  "relationshipType": "DEPENDS_ON",
  "relatedSpdxElement": "SPDXRef-<B>"
}
```

Zero `OPTIONAL_DEPENDENCY_OF` edges emitted under basic mode. Same treatment as `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` / `PROVIDED_DEPENDENCY_OF` under basic mode.

## Golden emission example

Given a Rust project with `Cargo.toml`:

```toml
[package]
name = "my-app"
version = "0.1.0"

[dependencies]
serde = "1"
foo = { version = "1", optional = true }

[features]
foo-support = ["dep:foo"]
```

SPDX 2.3 output (under `--spdx2-relationship-compat=full`, relationship excerpts):

```json
{
  "relationships": [
    { "spdxElementId": "SPDXRef-DOCUMENT",       "relationshipType": "DESCRIBES",           "relatedSpdxElement": "SPDXRef-my-app" },
    { "spdxElementId": "SPDXRef-my-app",         "relationshipType": "DEPENDS_ON",          "relatedSpdxElement": "SPDXRef-serde" },
    { "spdxElementId": "SPDXRef-foo",            "relationshipType": "OPTIONAL_DEPENDENCY_OF", "relatedSpdxElement": "SPDXRef-my-app" }
  ]
}
```

Under `--spdx2-relationship-compat=basic`:

```json
{
  "relationships": [
    { "spdxElementId": "SPDXRef-DOCUMENT", "relationshipType": "DESCRIBES",  "relatedSpdxElement": "SPDXRef-my-app" },
    { "spdxElementId": "SPDXRef-my-app",   "relationshipType": "DEPENDS_ON", "relatedSpdxElement": "SPDXRef-serde" },
    { "spdxElementId": "SPDXRef-my-app",   "relationshipType": "DEPENDS_ON", "relatedSpdxElement": "SPDXRef-foo" }
  ]
}
```

## Preservation invariants (FR-005, FR-009)

- Fixtures that do NOT exercise the new `LifecycleScope::Optional` OR `build_inclusion = NotNeeded` path MUST emit byte-identical SPDX 2.3 output pre-vs-post m179 (verified by golden fixture diff).
- Existing `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` edge counts on any fixture MUST NOT decrease post-m179.
