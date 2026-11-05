# Data Model: 1279-closure-dep-field-coverage

In-process per scan. Nothing persists; the only durable output is what reaches
the emitted document.

## Dependency attribute

A named attribute of a derivation whose value is a list of store paths.
nixpkgs names them by convention — `*Inputs`, and `deps<Host><Target>` with an
optional `Propagated` suffix — and that convention is the contract (#1348
established why a pure shape test cannot serve: nearly every attribute of a
derivation mentions a store path and almost none declares a dependency).

Fourteen are declared on the corpus closure. They fall into **four** states,
not two, and the fourth is the one this milestone adds:

| state | example | count (linux) | behaviour |
|---|---|---|---|
| consulted, populated | `buildInputs` | 5 | credits a role |
| consulted, never declared | `depsBuildHost`, `nativeCheckInputs` | 2 | nothing; not a gap |
| declared, empty | `depsHostHostPropagated` | 4 | nothing; not a gap (FR-005) |
| **declared, populated, not consulted** | `defaultNativeBuildInputs` | 2 after this milestone | **marks its members (FR-002)** |

**Validation rules**

- A field consulted but never declared must not appear as a gap. The
  disclosure iterates what each derivation declares, not the field list, so
  this already holds — asserted rather than assumed.
- An empty list credits nothing and is not a gap. Load-bearing: four
  attributes are declared on all 1,253 derivations of the darwin closure and
  hold zero store paths.
- A value that is not a string or list of strings yields no paths. Unchanged
  from #1348 and must stay total over `Value`.

## Consulted set

The attributes classification reads, as two constants. Widened by this
milestone:

| | before | added | after |
|---|---|---|---|
| artifact-side | `buildInputs`, `propagatedBuildInputs`, `depsHostHost` | `depsTargetTarget`, `depsTargetTargetPropagated` | 5 |
| tooling-side | `nativeBuildInputs`, `depsBuildBuild`, `depsBuildHost`, `nativeCheckInputs` | `depsBuildTarget`, `propagatedNativeBuildInputs` | 6 |

**The complement is the disclosure.** `consulted_fields()` already derives
from these two constants and `read_outcome` takes it as an argument, so
widening the constants narrows C204's `fields-unconsulted` automatically. SC-009
asserts the two agree; it is not left to inspection, because a hand-maintained
second copy is exactly the drift that would make the disclosure lie.

**Deliberately excluded**: `defaultNativeBuildInputs` and
`defaultBuildInputs`. They are stdenv's own machinery, they carry most of the
unread declarations, and FR-012 keeps them out so the census moves only for
the four above. Their members get the marker instead.

## Unclassified reference

**New.** The record that a member was named by an attribute classification
does not consult.

| field | shape | meaning |
|---|---|---|
| member | derivation key | which closure member was referenced |
| attributes | sorted set of names | **through which attributes** — FR-013 |

**Validation rules**

- Sorted, so the emitted value is byte-stable; an unordered collection makes
  goldens non-deterministic.
- Keyed per member, not per declaring derivation: a consumer asks "why is this
  component here", not "what did that derivation declare".
- Recorded for a member **whatever its role**, but *emitted* only where it
  changes the reading — see the marker's gating in
  `contracts/unclassified-marker.md`.
- Purely observational. It must not influence any role, which is what keeps
  SC-011's "zero census movement from the stdenv pair" checkable.

## Closure role

Unchanged in definition, in its four wire values, and in the union rule that
merges build variants. Only its *inputs* widen.

Measured movement on the pinned x86_64-linux closure:

| role | before | after |
|---|---|---|
| artifact-input | 270 | **268** |
| build-tooling | 118 | 121 |
| both | 42 | 45 |
| unreferenced | 1,319 | 1,315 |

`artifact-input` **falls**, because three of its members gain a tooling
reference and become `both` while only one arrives. Stated here because a
reader checking the arithmetic will otherwise read it as an error.

## Relationships

```text
Derivation
  ├── declares attribute ──┬── consulted ──────→ credits a role ──→ native scope marker
  │                        └── not consulted ──→ Unclassified reference ──→ marker
  └── outputs ──→ store-path join (basename; unchanged)

Unclassified reference ──(observational only)──→ never reaches a role
```

The one-way arrow at the bottom is the property SC-011 asserts. If the marker
ever feeds a role, this milestone's census is no longer attributable to its
four attributes and the measurement loses its meaning.
