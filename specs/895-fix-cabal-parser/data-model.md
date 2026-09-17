# Phase 1 Data Model: Trustworthy `.cabal` dependency parsing

The feature adds no persistent state. What follows are the in-process shapes
the parsing path works with, and the rules each must satisfy.

---

## FieldBlock

A bounded region of a `.cabal` file introduced by a field name and containing
that field's value.

| Attribute | Meaning |
|---|---|
| `field_name` | The field introducing the block (`build-depends`, `build-tool-depends`) |
| `field_indent` | Column at which the field name begins — **the reference point for termination** |
| `body` | The lines belonging to the block, comments removed |

**Rules**

- **FB-1**: The block ends at the first subsequent non-blank line whose
  indentation is `<= field_indent` (R1). Not at a blank line, not at column 0,
  not at the end of the file alone — those are the current rule and are the
  defect.
- **FB-2**: Blank lines inside a block do not terminate it; they are skipped
  when looking for the terminator.
- **FB-3**: Text appearing on the field line after the colon is the block's
  first content (Layout B in R1), not a separate case.
- **FB-4**: A line whose first non-whitespace characters are `--` contributes
  nothing. At indentation `<= field_indent` it also terminates the block by
  FB-1; deeper, it is skipped.
- **FB-5**: `field_indent` is measured in characters. A file mixing tabs and
  spaces is not required to parse *correctly*, but MUST NOT produce a
  fabricated component — falling back to emitting nothing for the block
  satisfies this.

---

## DeclaredDependency

One entry within a `FieldBlock`.

| Attribute | Meaning |
|---|---|
| `name` | The package name |
| `constraint` | The version constraint as written, if any |
| `kind` | `Library` (from `build-depends`) or `BuildTool` (from `build-tool-depends`) |
| `executable` | For `BuildTool` only: the executable named after the colon |

**Rules**

- **DD-1**: `name` is drawn only from text inside its own `FieldBlock`
  (FR-003). This is the invariant the four observed defects all violate.
- **DD-2**: `constraint` is recorded verbatim, neither normalised nor
  evaluated.
- **DD-3**: An entry that cannot be read yields no `DeclaredDependency` and
  increments the skipped count. Its siblings are unaffected (FR-012).
- **DD-4**: For `kind = BuildTool`, a `package:executable` declaration splits
  into `name = package` and `executable = executable`. The combined string is
  never a `name` (FR-010a) — it is not a package that exists.
- **DD-5**: Entries are separated by `,`, which may lead or trail a line.

---

## EmittedComponent

What a `DeclaredDependency` becomes in the SBOM.

**Rules**

- **EC-1**: Identifier is `pkg:hackage/<name>` with **no version segment**
  (FR-005, clarification 1). The constraint never appears in it.
- **EC-2**: The constraint is carried in the `waybill:requirement-ranges`
  record, catalogued as C20 (R3).
- **EC-3**: A package declared more than once in one file yields **one**
  component carrying every declared constraint (FR-007). Since EC-1 makes the
  identifier independent of the constraint, the two declarations collide on
  the same identifier by construction rather than by an explicit merge step —
  worth an assertion, because it is a property of the design rather than of
  any one line of code.
- **EC-4**: `kind = BuildTool` yields a component marked build-time using the
  lifecycle vocabulary already in the codebase (FR-010), with `executable`
  recoverable from the document (FR-010b).
- **EC-5**: A package declared as **both** a build tool and a library
  dependency is not marked exclusively build-time (US3 scenario 4). The
  library declaration is the weaker claim about scope and wins, on the same
  asymmetry the project applies elsewhere: over-reporting a runtime
  dependency is recoverable, hiding one from a runtime filter is not.

---

## SkippedEntryCount

A per-scan tally of entries that could not be read.

**Rules**

- **SEC-1**: Emitted whenever the Haskell reader ran, **including when zero**
  (FR-012b), so "the file was fully readable" stays distinguishable from "the
  count is missing".
- **SEC-2**: Absent entirely when no `.cabal` file was read, preserving
  byte-identical output for projects with no Haskell content.
- **SEC-3**: Counts entries, not files and not bytes — matching the existing
  `LegacyShapeCounter` convention in the Pants reader, which counts files and
  documents that it does so.

---

## Out of scope

These shapes are named to mark the boundary, not to be changed:

- **Lockfile-resolved components** (`stack.yaml.lock`, `cabal.project.freeze`)
  carry real resolved versions and are untouched.
- **Resolver / toolchain placeholders** (`pkg:generic/ghc-<v>@unspecified`)
  keep their existing shape (R2). They are not cabal-declared dependencies
  and the clarification about versionless identifiers does not reach them.
