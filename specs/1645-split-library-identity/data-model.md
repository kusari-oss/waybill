# Data model: spec 1645

Everything is in process, per scan. Nothing persists beyond the emitted document.

## Curated entry

A `PackageDbEntry` with `evidence_kind = "embedded-version-string"`, `name` = the slug, `version` = the curated version, and `source_path` = the binary it was read from.

## Record entry

A `PackageDbEntry` from `installed_lib::resolve`:
- `waybill:source-mechanism` is `pkg-config` or `cmake-package-version`;
- `name` is the lowercased record name;
- the **record's own library files** are `extra_source_paths`, from name matching (#1531), exported targets (#1563) and embedded paths (#1534).

## CuratedLibrary (extended)

- `names(self) -> &'static [&'static str]`: an exhaustive match (R3). The slug comes first. Ncurses has `["ncurses", "ncursesw", "ncursest", "ncursestw"]`; every other library has `[slug]`.
- `from_slug(&str) -> Option<CuratedLibrary>`: an exhaustive match.

## Shared-file verdict (per file F that a curated entry C was read from)

The candidates are the record entries R whose library files include F and whose PURL differs from C's.

| Verdict | Condition | Effect |
|---|---|---|
| `merge` | exactly one candidate has `R.name ∈ C.library.names()` **and** an agreeing version (below) | rewrite (next section) |
| `version-disagrees` | every candidate with a qualifying name has a version that does not agree | none |
| `name-unrelated` | no candidate has a qualifying name | none |
| `ambiguous` | two or more candidates have a qualifying name and an agreeing version | none |

If both `version-disagrees` and `name-unrelated` apply to a file, the census records `version-disagrees`, the closer of the two.

**Versions agree** when `vc == vr`, or when `vr` starts with `vc` followed by `.`.

## Rewrite on `merge`

- **Surviving PURL**: `pkg:generic/<slug>@<vr>`.
- **Record entry R**: `purl` becomes the surviving PURL and `name` becomes the slug. Its files, mechanism and tier are kept.
- **Curated entry C**: `purl` and `version` become the surviving PURL and `vr`. When `vc == vr`, nothing changes.
- **Every entry's `depends`**: each string equal to R's old PURL is replaced by the surviving PURL (R2).

The deduplicator then merges R and C, which are now equal PURLs, in the usual way: R comes first, so R's tier and mechanism are kept, and C56 records the other mechanism.

A curated entry on a file that no record claims is not rewritten. With FR-002 and FR-013 its PURL already equals the surviving one on TheRock (`librocm_sysdeps_ncursesw.so.6`), so it merges by PURL.

## Version forms after this spec

| Library | Banner | `main` | Spec 1645 |
|---|---|---|---|
| LLVM | `LLVM version 23.0.0git\0` | `23.0.0` | `23.0.0git` |
| LLVM | `LLVM version 17.0.6\0` | `17.0.6` | `17.0.6` |
| ncurses | `ncurses 6.6.20251230\0` | `6.6` | `6.6.20251230` |

## ncurses CPE (FR-008)

| Component version | Version field | Update field |
|---|---|---|
| `X.Y.YYYYMMDD` | `X.Y` | `YYYYMMDD` |
| `X.Y` | `X.Y` | `*` (unchanged) |
| anything else | as `main` | as `main` |
