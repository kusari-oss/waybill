# Data model: spec 1653

## `ghc_boot_dependencies.tsv` (new committed data)

**The header** is lines starting with `#`, carrying:
- `# observed: <date>`;
- `# source: https://downloads.haskell.org/~ghc/<v>/ghc-<v>-x86_64-deb10-linux.tar.xz`;
- `# layouts: package.conf.d (9.4+), package-data.mk (9.0, 9.2)`;
- `# regenerate: python3 -I -B scripts/ghc-boot-dependencies.py > …/ghc_boot_dependencies.tsv`;
- `# releases: 36; rows: N`.

**The columns** are tab-separated, with one row per (release, library):

| column | example | rule |
|---|---|---|
| release | `9.10.3` | exact `X.Y.Z`; the same set as `ghc_boot_libraries.tsv` |
| library | `base` | package name |
| version | `4.20.2.0` | must equal milestone 966's version for the same release and library (`rts` under make: may be empty) |
| depends | `ghc-internal ghc-prim` | space-separated names, sorted; `-` for none |
| layout | `package.conf.d` | `package.conf.d` or `package-data.mk` |

## `BootDependencyTable` (new, `boot_dependencies.rs`)

- `table() -> &'static BootDependencyTable`, parsed once with `OnceLock`, as `boot_table::table()` is.
- `observed() -> &str`.
- `releases() -> BTreeSet<&str>`.
- `depends(release, library) -> Option<&[String]>`: `None` when the release or library is absent, `Some(&[])` for a known leaf.

## C247 `waybill:boot-library-dependencies` (new, per boot library)

A canonical JSON string, with sorted keys. It is present on every component carrying C218 in a closure scan.

| case | value |
|---|---|
| every dependency is in the document | `{"source":"ghc-boot-dependencies/<observed>"}` |
| some dependency is absent | `{"absent":["rts"],"source":"ghc-boot-dependencies/<observed>"}` |
| a known leaf (`rts`) | `{"source":"ghc-boot-dependencies/<observed>"}` |
| the compiler's release is not in the table | `{"reason":"release-not-in-table"}` |
| the library is not in the release | `{"reason":"library-not-in-table"}` |

## `ReasonCode::BootLibraryEdgesIncomplete { incomplete: usize, unknown: usize }` (new)

- **Fires** when `incomplete + unknown > 0`.
  - `incomplete` counts C247 values with a non-empty `absent`.
  - `unknown` counts values with a `reason`.
- **Renders:** `boot-library-edges-incomplete: <N> hackage boot librar(ies), <incomplete> with dependencies not in the document, <unknown> without dependency data`.
- **In `degraded_ecosystems`:** it adds `hackage`.

## Edges

**Type:** `RelationshipType::DependsOn`, with provenance `{source: "ghc-boot-dependencies", data_type: "dependency"}`.

**Endpoints:** both endpoints carry the same C218 value. The target is the unique component of that compiler with the dependency's name. An edge that already exists is not duplicated.
