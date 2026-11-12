# Data model: GHC boot-library versions

## GhcRelease

A compiler release parsed from a store path's basename.

| field | type | note |
|---|---|---|
| `version` | `String`, dotted digits | the table key (`9.10.3`) |
| `bootstrap` | `bool` | `true` for `ghc-binary-*` |
| `label` | `String` | as the closure names it, `ghc-9.10.3` or `ghc-binary-9.8.4`; this is C218's value and C219's key |

Parsed only by the shared helper behind `is_ghc_store_path` (research R2). A basename whose version part is not dotted digits (`ghc-9.10.3-doc`, `ghc-settings-edit`) yields `None`.

## BootLibraryTable

Committed TSV, loaded once.

| field | type |
|---|---|
| `observed` | date (header) |
| `rows` | `ghc version → library → BootRow` |

`BootRow { version: String, source: TableSource, platform: Platform }`

- `TableSource ∈ { ReleaseNotes, StackGlobalHints, GhcReleaseTag }`
- `Platform ∈ { All, Windows }`

**Validation**, all asserted by a unit test over the embedded file:
- every line parses, with no duplicate `(ghc, library)` pair;
- versions are dotted digits;
- every release has `base`;
- the 9.12.3 `process` row is `1.6.26.1` / `ghc-release-tag` (the known disagreement).

Queries:
- `ships(release, system) -> Option<BTreeSet<name>>`: the `All` rows, plus the `Windows` rows on a Windows `system`. `None` when the release is absent.
- `version_of(release, name, system) -> Option<&str>`: the same platform filter.

## ClosureCompilers

Derived from the `ClassifiedClosure` once, before `enrich`.

| field | type | note |
|---|---|---|
| `system` | `String` | the evaluated derivation's `env.system` (stdenv sets it). If that is absent, the `NixSystem` detected at `scan_cmd.rs:4556`, passed out of the closure block. Neither `ClassifiedClosure` nor `RawDerivation` carries a system field. |
| `project` | `Option<GhcRelease>` | research R2's rule |
| `by_member` | `(pname, version) → GhcRelease` | only members whose `nativeBuildInputs` name exactly one GHC |
| `seen` | `BTreeSet<GhcRelease>` | every distinct compiler found; C219's keys |

## Classification outcomes (per declared Haskell dependency, design tier)

| condition | outcome | annotations |
|---|---|---|
| project compiler known, in table, name in `ships` | **resolved**, version from table | C218 = project label; C170, C171 and C151 removed (m926's existing resolved path) |
| project compiler known, in table, name not in `ships` | not a boot library; ordinary m926 resolution | unchanged m926 behaviour (`os-string` on 9.6.6) |
| project compiler known, absent from table | m926 union rule | C170 `compiler-supplied` as today; C219 records `absent-from-table` |
| no project compiler | m926 union rule | as today; C219 `compilers: {}` when the closure ran |

## Declared-edge targets (closure tier, research R7)

For a confirmed member `m` and a declared target `t`:

1. A Haskell build of `t` is in the document (rank 2): bind to it, as today.
2. Else, if `m`'s compiler `g` is known and in the table, and `t ∈ ships(g)`: bind to `pkg:hackage/t@version_of(g, t)`, creating that component if absent (versioned, C218 = `g.label`).
3. Else: today's path (by-name `purl_of`, then FR-012c's versionless component or a counted omission).

## Invariants

- No document holds both `pkg:hackage/<n>` (versionless) and a C218-bearing `pkg:hackage/<n>@<v>` produced from the same compiler (FR-010).
- Every C218-bearing component's version equals `version_of(C218 release, name, system)`.
- C219 is present iff the closure tier ran **and** either m926 ran (C174) or the identity pass examined Haskell members (C201). The latter covers closure-only scans, where `enrich` declines. Its `compilers` keys equal the C218 values in the document, plus any `absent-from-table` compiler.
