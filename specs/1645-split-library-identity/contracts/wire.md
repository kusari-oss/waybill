# Wire contract: spec 1645

## Census events (DEBUG, target `waybill::version_census`)

Each event is emitted only when `tracing::enabled!` is true for that target and level. Every value is one JSON object with sorted keys.

### `shared_file`

One event per file claimed by a curated entry and by at least one record entry with a different PURL.

```json
{"curated":{"library":"ncurses","version":"6.6.20251230"},
 "file":"lib/rocm_sysdeps/lib/libncursesw.so",
 "records":[{"mechanism":"pkg-config","name":"ncursesw","name_qualifies":true,"version":"6.6.20251230","version_agrees":true}],
 "verdict":"merge"}
```

`verdict` is one of `merge`, `version-disagrees`, `name-unrelated` or `ambiguous` (data-model). `records` is sorted by `(mechanism, name, version)`.

### `curated_version_change`

Log-only commit only (R8). One event per curated LLVM or ncurses match whose version FR-002 or FR-013 would change:

```json
{"file":"…","library":"llvm","new":"23.0.0git","old":"23.0.0"}
```

### `cpe_form_change`

Log-only commit only (R8). One event per `pkg:generic/ncurses` component whose CPE FR-008 would change:

```json
{"new":"cpe:2.3:a:invisible-island:ncurses:6.4:20221231:*:*:*:*:*:*","old":"cpe:2.3:a:invisible-island:ncurses:6.4.20221231:*:*:*:*:*:*:*","purl":"pkg:generic/ncurses@6.4.20221231"}
```

## Document changes

- **No new annotation, catalog row or field.** The merged component is an ordinary component: the record's mechanism and tier, and the union of C18 `waybill:source-files` (R7). The curated evidence kind is not kept (#1665).
- **The CPE value form changes for ncurses only** (FR-008). The field is unchanged.

## Cases (`tests/library_identity.rs`)

All fixtures are synthetic: a minimal ELF shared library (`elf_shared_lib`) with the banner in `.rodata`, named `libwaybill-fixture-*` where the name is not under test. The `.pc` and CMake files are written by the test.

| Case | Tree | Expected |
|---|---|---|
| M-1 name variant | `lib/libncursesw.so.6` with `ncurses 6.6.20251230\0`, and `lib/pkgconfig/ncursesw.pc` (`Version: 6.6.20251230`, `Libs: -lncursesw`) | exactly one `pkg:generic/ncurses@6.6.20251230`, with C18 containing the library and `ncursesw.pc`, `waybill:source-mechanism` = `pkg-config`, CPE candidates `invisible-island`, `ncurses_project` and `gnu` `:ncurses:6.6:20251230`, and no `ncursesw` component |
| M-2 version suffix | `lib/libLLVM.so.23.0git` with `LLVM version 23.0.0git\0`, and `lib/cmake/llvm/LLVMConfigVersion.cmake` (`23.0.0git`) with exported targets naming the library | exactly one `pkg:generic/llvm@23.0.0git` listing both |
| M-3 prefix | `lib/libexpat.so.1` with `expat_2.7.5\0`, and `lib/pkgconfig/expat.pc` (`Version: 2.7.5.1`). The record attaches the file by its name (`match_binaries`). The curated version is a dotted prefix of the record's | one `pkg:generic/expat@2.7.5.1` listing both; no `expat@2.7.5` |
| N-1 disagreement | M-1 with `Version: 6.5.20240427` | two components, `ncurses@6.6.20251230` and `ncursesw@6.5.20240427`; census `version-disagrees` |
| N-2 bundled | `libwaybill-fixture-foo.so` with `deflate 1.3.1 Copyright…`, and `waybill-fixture-foo.pc` `Version: 1.3.1` | two components; census `name-unrelated` |
| N-3 string prefix | M-3 with `Version: 2.7.50`: a plain string prefix, but not at a `.` | two components; census `version-disagrees` |
| N-4 two records | **Unit test on `reconcile`**: one curated ncurses entry, and two record entries named `ncursesw` and `ncursestw` at `6.6.20251230` that both list the curated file. A record attaches a file by name only when the file is `lib<name>` (`names_library`), so two qualifying names reach one file only through exported targets or embedded paths. The integration form is M-1 plus a CMake package `ncursestw` whose exported target's `IMPORTED_LOCATION` is the same file. It is attempted, and if the reader never attaches both, T-census records that the state is unreachable from real readers. | no merge for that file; census `ambiguous` |
| E-1 edges | M-1 plus `formw.pc` with `Requires: ncursesw` and its own `libformw.so` | the `formw` component's edge points to `pkg:generic/ncurses@6.6.20251230`; no dangling reference |
| C-1 census | M-1 with `waybill::version_census=debug` | exactly one `shared_file` event with verdict `merge` |
| C-2 no record | B-1's ncurses library alone | no `shared_file` event |
| P-1 parity | M-1, M-2, N-1 | `waybill sbom parity-check` adds no gap |
| B-1 | spec 1643's tree | regenerated golden; the diff is the ncurses component's version and CPE only |

Unit tests in `scan_fs/binary/version_strings.rs`:
- `parse_llvm_version`: `17.0.6\0`; `23.0.0git\0`; `18.1.0rc1 (`; `23.0.0` at the region's end; `23.0.0-rc1` rejected;
- `parse_ncurses_version`: keeps the patch date, and keeps its existing rejections;
- `names` and `from_slug` round-trip every variant.

Unit tests in `generate/cpe.rs`: the ncurses update split for `X.Y.YYYYMMDD`; `X.Y` unchanged; other libraries unchanged.
