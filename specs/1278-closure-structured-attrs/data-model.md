# Data Model: 1278-closure-structured-attrs

In-process per scan. Nothing persists; the only durable output is what reaches
the emitted document.

## Derivation record

One closure member as `nix derivation show` reports it. Already exists; gains
one field and a second source for four reads.

| Field | Shape | Change |
|---|---|---|
| `name` | optional string | unchanged |
| `env` | map of string → string | unchanged |
| `structured_attrs` | optional JSON object | **new** — the typed carrier, read as `serde_json::Value` because its members are heterogeneous (strings, lists, booleans, nested objects) |
| `outputs` | map of name → output | unchanged |

**Validation rules**

- A field is resolved from `env` first and from `structured_attrs` only when
  `env` does not carry it (FR-006 precedence).
- Both present and disagreeing → `env` wins **and the disagreement is
  reported**. Measured: does not occur on either closure examined, so this is
  a defined default, not an observed preference.
- A typed value that is neither a string nor a list of strings yields no paths
  and must not fail the scan (FR-004). Total over `Value`, so there is no
  shape that panics.
- An empty list yields no paths, identically to an absent field (FR-005).
  Load-bearing: `buildInputs: []` is present on all 56 `source` derivations in
  the pinned closure, and crediting it would invent references.

## Closure format version

The `version` field of the top-level document. Parsed and dropped today, which
is how the v3 → v4 move went unnoticed.

- Captured, carried to emission, recorded in the disclosure (FR-012).
- **Never gates resolution** (FR-013). An unseen version is read exactly as a
  known one.
- Observed value on the pinned closure: `4`.

## Dependency field

A named attribute whose value is a list of store paths.

- Seven decide a role: three artifact fields (`buildInputs`,
  `propagatedBuildInputs`, `depsHostHost`) and four tooling fields
  (`nativeBuildInputs`, `depsBuildBuild`, `depsBuildHost`,
  `nativeCheckInputs`). **This set does not change** — the census in SC-004
  must stay attributable to the shape fix alone.
- `patches` resolves the same way and is not a dependency field: it feeds
  patch evidence, not roles.
- Any other field holding store paths is *reported, never classified*
  (FR-014). Its population is this milestone's output, not its input.

## Store-path join

Unchanged, and restated because it is the join that fails silently: output
paths arrive **without** the `/nix/store/` prefix while the fields referencing
them arrive **with** it. Comparison is on the basename. Comparing raw strings
matches nothing, without erroring, and yields a classification in which every
member is unreferenced — which once read as a finding rather than a bug.

The widening does not change the join. It changes only where the referencing
side's strings come from, which is why a typed-form fixture must assert a
resolved reference and not merely a parsed field.

## Read outcome

**New.** What the reader saw, assembled once per closure and consumed by
emission.

| Field | Shape | Meaning |
|---|---|---|
| `format_version` | integer | the output shape the closure was read from |
| `fields_unconsulted` | sorted set of field names | every store-path field declared but not consulted by classification (FR-014) |
| `declarations_unconsulted` | count | how many such declarations were seen |
| `derivations_affected` | count | how many derivations carried at least one |

**Validation rules**

- Sets are sorted and counts are integers, so the value is byte-stable across
  runs. A map with unordered keys would make the goldens non-deterministic.
- "Nothing unconsulted" is representable and distinct from "no closure": the
  disclosure is **absent** in the first case, and C190 already covers the
  second. A zero-valued disclosure would claim a closure was read where none
  was.
- Purely observational. Changing it must not change any member's role
  (SC-014), which is what makes the SC-004 census attributable.

## Derivation role

Unchanged in definition and in the union rule used to merge build variants.
Its *inputs* widen, so its values move: 256 / 101 / 39 / 1,353 →
270 / 118 / 42 / 1,319 on the pinned closure, with no component moving toward
`unreferenced`.

## Relationships

```text
Closure document
  ├── format version ─────────────────┐
  └── Derivation record (×N)          │
        ├── env ──────────┐           │
        ├── structuredAttrs ──┐       │
        │                 ├── resolved attribute
        │                 │     ├── dependency field → role → native scope marker
        │                 │     ├── patches          → patch evidence
        │                 │     └── pname/version    → component identity
        │                 └── unconsulted field ─────┐
        └── outputs → store-path join                │
                                                     ▼
                                              Read outcome ──→ native completeness
                                                           └─→ new annotation
```

The read outcome is the only new node, and it is a leaf: everything reads from
it and nothing reads back into classification. That is the property SC-014
asserts.
