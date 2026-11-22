# Wire contract: fixture cases (`waybill-cli/tests/version_string_boundary.rs`)

Each case writes one synthetic ELF shared library (`elf_shared_lib`, bytes in `.rodata`) into its own temp directory. File names are synthetic (`libwaybill-fixture-z.so.1`, `libwaybill-fixture-ncurses.so.6`, `libwaybill-fixture-expat.so.1`); the curated slugs are the libraries' real names. Each case scans in all three formats and asserts:
- the curated component listed below;
- `waybill sbom parity-check` agreement;
- unique `bom-ref`s.

"main" is the result at `83657af2`, the control.

## Anchored patterns (User Story 1, User Story 2)

`P` is the preceding byte.

| case | `.rodata` bytes | P | expected | main |
|---|---|---|---|---|
| Z-1 | `P deflate 1.3.2 Copyright 1995-2026 …\0` | `\0` | `zlib@1.3.2` | same |
| Z-2 | same | `-` | `zlib@1.3.2` | no zlib version |
| Z-3 | same | `\xff` | `zlib@1.3.2` | no zlib version |
| Z-4 | same | `x` | no `zlib@1.3.2` | same |
| Z-5 | same | `9`, `_` | no `zlib@1.3.2` | same |
| Z-6 | `\0deflate 1.3.2 Copyright …\0` (no leading space) | – | `zlib@1.3.2` | same |
| Z-7 | `-deflate 1.3.2 Copyright …\0` (no leading space) | `-` | no `zlib@1.3.2` (the no-space form keeps the NUL rule) | same |
| N-1 | `P` + `ncurses 6.6.20251230\0` | `\0` | `ncurses@6.6` | same |
| N-2 | same | `-` | `ncurses@6.6` | none |
| N-3 | same | `\xff` | `ncurses@6.6` | none |
| N-4 | same | `l`, `9`, `_` | none | same |
| E-1 | `P` + `expat_2.7.5\0` | `\0` | `expat@2.7.5` | same |
| E-2 | same | `-`, `\xff` | `expat@2.7.5` | none |
| E-3 | same | `b`, `_` | none | same |

Without a curated version, zlib may still be reported by its symbol fingerprint. The synthetic library has no zlib symbols, so a case asserts the absence of the versioned PURL, not of every `zlib` component.

## NUL-only patterns (User Story 2)

| case | `.rodata` bytes | expected | main |
|---|---|---|---|
| U-1 | `-OpenSSL 3.0.11\0` | no `openssl` | same |
| U-2 | `\0OpenSSL 3.0.11\0` | `openssl@3.0.11` | same |

The other eleven NUL-only patterns are covered at unit level by the R3 iteration test, which runs each variant's sample banner behind `-` and behind NUL.

## Unchanged output (User Story 3)

| case | input | expected |
|---|---|---|
| B-1 | a library holding NUL-preceded zlib, expat and ncurses banners together | output byte-identical to `main`'s binary (golden captured from `main`, regenerated with `WAYBILL_UPDATE_VERSION_BOUNDARY_GOLDENS`) |
| B-2 | existing #1525/#1526 tests and `installed_lib_us3.rs` | unchanged |

## Census positive control

| case | input | expected |
|---|---|---|
| C-1 | `-ncurses 6.6.20251230\0`, scanned with `RUST_LOG=waybill::version_census=debug` | exactly one event: `library=ncurses`, `preceding=other_non_identifier`, `accepted_main=false`, `accepted_new=true` |
| C-2 | ` ncurses 6.6.20251230\0` (a space before it), same scan | exactly one event with `preceding=whitespace`, `accepted_new=true` |
