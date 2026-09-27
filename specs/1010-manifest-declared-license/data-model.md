# Data Model: declared licenses from manifests

**Feature**: `1010-manifest-declared-license` · **Date**: 2026-09-26

No new persistent entity is introduced. This feature populates a field that
already exists and adds one transient value type inside the readers.

## Existing entities (unchanged shape, newly populated)

### `PackageDbEntry.licenses: Vec<SpdxExpression>`

Declared at `scan_fs/package_db/mod.rs:142`. Populated by 13 production
main-module sites that currently construct it empty. Its doc comment already
describes the intent ("`package.json::license:`"), so no field documentation needs
inventing — only correcting where it claims detection is deferred.

**Validation**: every element is an `SpdxExpression`, so construction is the
validation boundary. Two constructors, and which one is used is the whole design:

| Constructor | Behaviour | Used when |
|---|---|---|
| `SpdxExpression::try_canonical` | Runs the real expression parser; stores the canonical form; errors on failure | First attempt, always |
| `SpdxExpression::new` | Lenient — accepts any non-empty, control-character-free string verbatim | Only after `try_canonical` fails, to preserve the raw declaration |

**Cardinality after this feature**: at most **one** element per main-module
component. A reader that finds several declared licenses combines them into a
single expression before construction (FR-010b). This is a deliberate narrowing:
pushing several elements would hand the combining decision to
`reduce_license_vec`, whose unconditional conjunction cannot be correct for every
ecosystem.

### `ResolvedComponent.licenses` / `.concluded_licenses`

Declared at `waybill-common/src/resolution.rs:18` and `:29`. Unchanged. The
declared slot receives manifest values; the concluded slot continues to receive
enrichment values. The two never contend, which is why no precedence rule exists.

## New transient type

### `DeclaredLicense` (reader-internal)

The outcome of resolving one manifest's license declaration. Not persisted, not
serialised, not crossing a crate boundary — it exists so a reader cannot
accidentally emit an unverified string as though it were canonical.

| State | Meaning | Emission consequence |
|---|---|---|
| `Canonical(SpdxExpression)` | Parsed and canonicalised | Emitted as a recognised license identifier |
| `Preserved(SpdxExpression)` | Did not canonicalise; raw text retained | Emission mints a non-listed license reference plus its extracted-text record |
| `Absent` | No declaration, or an unresolvable inheritance | No license emitted; no warning |

**State transitions**: one-way, decided once at extraction. A `Preserved` value is
never retried as `Canonical`, and a `Canonical` value is never downgraded. There is
no path from `Absent` to either — an absent declaration is not inferred from
anything else, which is what keeps this feature distinct from LICENSE-file
detection.

**Why a type rather than `Option<SpdxExpression>`**: `Option` cannot distinguish
"canonical" from "preserved raw text", yet the two must be emitted differently and
only one may be presented as an identifier. Collapsing them to `Option` would make
FR-004c unenforceable at compile time, and Principle IV asks for the distinction to
live in the type.

## Relationships

```text
manifest file
   └── (reader, ecosystem-specific extraction)
         └── raw declaration: &str  ─┬─ one value
                                     └─ several values ──> joined by the reader
                                                            using its ecosystem's
                                                            operator (FR-010)
         └── (shared resolution ladder)
               └── DeclaredLicense
                     ├── Canonical  ──> PackageDbEntry.licenses (1 element)
                     ├── Preserved  ──> PackageDbEntry.licenses (1 element, raw)
                     └── Absent     ──> PackageDbEntry.licenses (empty)

PackageDbEntry.licenses
   └── (existing resolution pipeline, unchanged)
         └── ResolvedComponent.licenses   [declared attribution]

scan-root component
   └── (post-reader pass, FR-016)
         └── inherits iff exactly one main-module carries a license
```

## Inheritance inputs

Not a new entity — a second lookup against a table the reader already reaches.

| Ecosystem | Declaration of inheritance | Root to resolve against |
|---|---|---|
| cargo | `license.workspace = true` | `[workspace.package]` in the workspace-root manifest |
| maven | absence of `<licenses>` in the child | parent POM (`licenses` is an inherited element) |

The cargo case reuses the traversal already implemented for
`version.workspace = true`; only the key differs.
