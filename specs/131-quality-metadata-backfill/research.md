# Research: Quality metadata backfill (milestone 131)

## R1. ECMA-335 §II.22.10 — `CustomAttribute` table (US1)

**Decision**: Walk the `CustomAttribute` table (token 0x0C) after Assembly row 0 extraction. Each
row's columns are:

- `Parent` — HasCustomAttribute coded index (5-bit tag); identifies the metadata element the
  attribute is attached to. For an `AssemblyInformationalVersionAttribute`, the parent is the
  Assembly itself (tag 14, row 1).
- `Type` — CustomAttributeType coded index (3-bit tag); resolves to `MethodDef` (tag 2) or
  `MemberRef` (tag 3) — virtually always the latter for assembly-attributes since the
  attribute's `.ctor` lives in an external assembly (`System.Reflection.dll`).
- `Value` — `#Blob` heap reference; the serialized attribute argument.

**Rationale**: ECMA-335 §II.22.10 documents this table layout verbatim. The CustomAttribute table
is present in virtually every managed assembly because the C# compiler emits at least
`AssemblyCompanyAttribute` + `AssemblyDescriptionAttribute` + `AssemblyVersionAttribute` per
standard build. For our scope (extracting Informational + File versions), we filter rows by
resolving the `Type` column's TypeRef name through the `#Strings` heap.

**Filter implementation**: For each CustomAttribute row whose `Type` tag is 3 (MemberRef):
1. Decode the row-index portion to get the MemberRef table row number.
2. Read that MemberRef row's `Class` column (a MemberRefParent coded index — usually tag 1 for
   TypeRef) and `Name` column (`#Strings` index, expected to be `".ctor"`).
3. Resolve the TypeRef row's `TypeName` (`#Strings` index) and compare to
   `"AssemblyInformationalVersionAttribute"` / `"AssemblyFileVersionAttribute"`.

## R2. ECMA-335 §II.23.3 — Custom-attribute blob serialization (US1)

**Decision**: For each filtered CustomAttribute row, decode its Value blob:

1. Read the blob length prefix (`#Blob` heap entries are length-prefixed; the length itself is
   encoded as the ECMA-335 §II.24.2.4 compressed-integer format — 1, 2, or 4 bytes).
2. The blob payload starts with a 2-byte prolog `0x0001` (`u16` little-endian).
3. After the prolog, the fixed arguments are serialized inline. For
   `AssemblyInformationalVersionAttribute(string)` / `AssemblyFileVersionAttribute(string)`, this
   is a single `SerString` argument:
   - Length prefix (compressed integer, ECMA-335 §II.24.2.4 — 1 byte for length <128, 2 bytes for
     128≤length<16384, 4 bytes for ≥16384).
   - UTF-8 bytes of the string.
   - Special case: a length prefix of `0xFF` denotes a null string (treat as `None`).
4. After the fixed argument(s), the named-argument count follows (2 bytes), then each named
   argument. For our purposes, all named arguments are ignored.

**Rationale**: The blob format is rigidly defined in §II.23.3 ("Custom attributes"). Real-world
`AssemblyInformationalVersionAttribute` values are short strings (under 100 chars), so the
1-byte length-prefix case dominates; we still handle all three width cases for correctness.

## R3. Coded-index width derivation (US1)

**Decision**: Use the milestone-130 Phase A row-width computation verbatim. For the
CustomAttribute table specifically, three coded-indices need width derivation:
- `HasCustomAttribute` (5-bit tag, 21 referenced tables) — almost always 4 bytes on real assemblies
  with >100 rows in any of the referenced tables.
- `CustomAttributeType` (3-bit tag, 5 referenced tables — Method, MemberRef, etc.) — usually
  2 bytes.
- The `#Blob` heap index width (`heap_sizes & 0x04` bit) — 2 or 4 bytes.

**Rationale**: Inherit milestone-130 US3's `compute_row_size` function with the existing
`coded_idx_width` helper. The Phase A sanity-filter caveat carries forward — for some assemblies
the row offsets misalign and we get an unparseable `Value` blob length. The fail-closed behavior
(Principle III) handles this: parse failure → fall through to the next-lower ladder rung silently.

## R4. License-file probe paths (US2a)

**Decision**: For each emitted PE/CLR `pkg:nuget` component, probe the assembly's parent directory
walking up to 3 levels, looking for case-insensitive matches against:
- `LICENSE`
- `LICENSE.txt`
- `LICENSE.md`
- `COPYING`
- `COPYING.txt`

The .NET runtime store convention places these at the package's version-directory root (e.g.
`/usr/share/dotnet/packs/Microsoft.AspNetCore.App.Ref/8.0.27/LICENSE.TXT`). The 3-level walk
upward from the `.dll`'s parent dir covers this layout AND the nested `ref/net8.0/` subdirectory
pattern.

**Rationale**: Empirically, the .NET runtime store and SDK pack layouts place LICENSE files at
the package-version root. A 3-level walk balances coverage with avoiding accidentally adopting a
license file from a sibling package. The 4 KB read cap (FR-013) prevents pathological
license-file-as-DDoS attacks while accommodating realistic license texts (MIT ~1 KB, Apache-2.0
~10 KB — the latter gets truncated at 4 KB with the prefix being self-identifying).

## R5. Nested-JAR `<licenses>` extraction (US2b)

**Decision**: Plumb the existing `parse_pom_xml` output's `licenses` field through milestone-130's
nested walker.

**Rationale**: The function already extracts `<licenses>` for top-level JAR usage. The
milestone-130 nested walker discards this output in favor of just `dependencies`. The fix is
~20 LOC: add a `nested_licenses` field to the emit path or use the existing
`PackageDbEntry.licenses` field directly. SpdxExpression construction reuses the existing
`SpdxExpression::try_canonical` helper from the top-level path.

## R6. cargo-auditable license-source annotation (US2c)

**Decision**: At the cargo-auditable per-crate emission site (`binary/entry.rs::cargo_auditable_packages_to_entries`), for each `packages[]` entry whose `source ==
"crates-io"` (or starts with `"registry+https://"` — both forms are observed in real-world
manifests), populate `PackageDbEntry.extra_annotations.insert("mikebom:license-source",
"registry-required")`.

**Rationale**: Constitution Principle XII permits external-source enrichment but doesn't require
it. Marking these components with `registry-required` signals downstream tooling (a future
deps.dev milestone) where to look without consulting the registry from inside this milestone.
For local-path / git deps, no annotation is added (we don't know where the license is).

## R7. Supplier URL conventions (US3)

**Decision**: Extend `scan_fs/mod.rs::supplier_from_purl` with three new heuristic patterns:

| PURL type | Synthesized URL |
|---|---|
| `pkg:cargo/<name>@<version>` | `https://crates.io/crates/<name>/<version>` (type=website) |
| `pkg:nuget/<name>@<version>` | `https://www.nuget.org/packages/<name>/<version>` (type=website) |
| `pkg:maven/<g>/<a>@<v>` | `https://search.maven.org/artifact/<g>/<a>/<v>/jar` (type=website) |

For the maven case, gate the synthesis on the existing `mikebom:source-mechanism` annotation
being `"maven-jar-nested"` — top-level JARs may already get a sidecar-derived URL elsewhere; we
don't want to clobber.

For the cargo case, ADDITIONALLY parse the `.dep-v0` `source` field. If it matches
`^git\+(https?://[^#]+?)(\.git)?(#[a-f0-9]+)?$`, emit a `vcs`-type ExternalReference with the
captured URL (sans trailing `.git`, sans the `#<rev>` fragment). The cargo-auditable source field
flows through `binary/entry.rs::cargo_auditable_packages_to_entries`; the parsed VCS URL goes into
`PackageDbEntry.extra_annotations` or directly into a new `ExternalReference` on the entry.

**Rationale**: All three URL patterns are documented registry conventions (crates.io's REST API
documents the URL shape; nuget.org's package page URL is stable; search.maven.org's per-artifact
JAR-download URL is stable). The VCS pattern matches the upstream cargo-auditable spec example.

## R8. Constitutional fail-closed behavior (cross-cutting)

**Decision**: Every new parser failure (CustomAttribute decode, license-file read, source-field
regex mismatch) emits a single `warn`-level log and falls through to the next-lower behavior:
US1 → Phase A 4-tuple ladder rung; US2 → `mikebom:license-source = "package-dir-no-license"`;
US3 → no external-reference emitted for that component. No silent omission; no `unwrap()`;
no Constitution Principle III violation.

## Decisions summary

| ID | Topic | Decision | Status |
|---|---|---|---|
| R1 | CustomAttribute table layout | Walk via §II.22.10; filter by resolved TypeRef name | Decided |
| R2 | Blob serialization | Decode prolog `01 00` + compressed-int + UTF-8 string | Decided |
| R3 | Coded-index widths | Reuse milestone-130 `compute_row_size` + `coded_idx_width` | Decided |
| R4 | License-file probe paths | 3-level upward walk; 5 filename variants; 4 KB read cap | Decided |
| R5 | Nested-JAR licenses | Plumb `parse_pom_xml.licenses` through milestone-130 walker | Decided |
| R6 | cargo-auditable license-source | `"registry-required"` annotation for crates-io entries | Decided |
| R7 | Supplier URL templates | crates.io / nuget.org / search.maven.org + git+ parser | Decided |
| R8 | Fail-closed behavior | Single warn + fall through; no silent omission | Decided |
