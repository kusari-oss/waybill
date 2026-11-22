# Contract: spec 1644

## Fixtures and expected components

All fixtures are synthetic shared libraries under `waybill-fixture-*` directory names. ELF ones are built by `elf_shared_lib` (string region only) or `elf_shared_lib_with_dynsym` (string region plus `.dynsym`); Mach-O ones (A-5, N-9) by `macho_dylib_with_symbols`. `\0` is NUL. "Defines S" and "imports S" refer to `.dynsym` entries.

| ID | String region | Symbols | Expected for the library under test |
|---|---|---|---|
| A-1 | `\0bzip2/libbzip2: internal error number %d.\0` … `\01.0.8, 13-Jul-2019\0` | none | `pkg:generic/bzip2@1.0.8` |
| A-2 | `\0GNU MP: Cannot allocate memory\0` … `\06.3.0\0` | none | `pkg:generic/gmp@6.3.0` |
| A-3 | `\05.8.1\0` | defines `lzma_version_string` | `pkg:generic/liblzma@5.8.1` |
| A-4 | `\05.7.1alpha\0` | defines `lzma_version_number` | `pkg:generic/liblzma@5.7.1alpha` |
| A-5 | Mach-O dylib (`macho_dylib_with_symbols`), `__cstring` `\05.8.1\0` | defines `_lzma_version_string` | `pkg:generic/liblzma@5.8.1` |
| G-1 | `\0GNU MP: …\0`, then binary bytes ending `\xff`, then `6.3.0\0` (TheRock's layout) | none | `pkg:generic/gmp@6.3.0`; census candidate `"preceding":"binary"` |
| N-1 | A-1 without `bzip2/libbzip2` | none | no bzip2 component |
| N-2 | A-2 without `GNU MP:` | none | no gmp component |
| N-3 | A-3's region | **imports** `lzma_version_string` | no liblzma component |
| N-9 | A-5's `__cstring` | **imports** `_lzma_version_string` | no liblzma component |
| N-4 | A-2 plus `\06.2.1\0` | none | no gmp component; census `ambiguous` with `6.2.1`, `6.3.0` |
| N-5 | `\0GNU MP:\0`, `\05.8.1\0` | defines `lzma_version_string` | neither gmp nor liblzma; census `ambiguous-shared` for both |
| N-6 | `\0GNU MP:\0`, `\0requires 6.3.0 or later\0` | none | no gmp component (not a whole string) |
| N-7 | `\04.999.8beta-21-g1d92\0` | defines `lzma_version_string` | no liblzma component; census `no-candidate` |
| N-8 | `\01.0.8\0`, `\0bzip2/libbzip2\0` | none | no bzip2 component (the date form is required) |
| P-1 | A-3 plus `lib/pkgconfig/liblzma.pc` with `Version: 5.8.1` | as A-3 | exactly one `pkg:generic/liblzma@5.8.1` |
| P-2 | P-1 with `Version: 5.8.0` | as A-3 | `pkg:generic/liblzma@5.8.1` and `pkg:generic/liblzma@5.8.0` |
| B-1 | a library with no anchor (the spec 1643 B-1 tree) | none | byte-identical to `main`; golden unchanged |

Every case's CycloneDX, SPDX 2.3 and SPDX 3 documents pass `waybill sbom parity-check`.

A versioned component carries:
- `waybill:evidence-kind = embedded-version-string`;
- `waybill:confidence = heuristic`;
- its CPE candidates from `cpe_table.toml`: `cpe:2.3:a:bzip:bzip2:<v>`, `cpe:2.3:a:gmplib:gmp:<v>`, and `cpe:2.3:a:tukaani:xz:<v>` plus `cpe:2.3:a:xz_project:xz:<v>`.

## Census event

Target `waybill::version_census`, level DEBUG, one per binary × anchored library:

```
path = <binary path>
anchored = {"library":"gmp",
            "anchors":[{"kind":"text","value":"GNU MP:","offset":4096}],
            "candidates":[{"value":"6.3.0","offset":8192}],
            "shared":[],
            "verdict":"versioned","version":"6.3.0"}
```

| Field | Values |
|---|---|
| `kind` | `text` (with `offset`) or `symbol` (with `value` only) |
| `verdict` | `versioned`, `ambiguous`, `ambiguous-shared` or `no-candidate` |
| `version` | present only for `versioned` |
| `shared` | the candidates removed by FR-005a, each with the other library that accepts it |
| `preceding` (each candidate) | `start`, `nul` or `binary` (FR-004 as amended) |

The census never changes the document, and is computed only when the target is enabled.
