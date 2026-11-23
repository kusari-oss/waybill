# Static liblzma probe (2026-11-23, `main` `b6ad3c26`)

Two programs linked statically against a distribution liblzma, each kept unstripped and stripped, then classified with `readelf` and `strings` and scanned with waybill.

- `a.c` calls only `lzma_easy_buffer_encode`.
- `b.c` also calls `lzma_version_string`.
- `probe.sh` builds and classifies them. It expects the two sources in `/w`.

## Re-running

```sh
tar -cf - a.c b.c probe.sh | docker run --rm -i --platform linux/amd64 ubuntu:24.04 \
  bash -c 'mkdir /w && tar -xf - -C /w && bash /w/probe.sh'
```

`ver=` in `probe.sh` is the package's upstream version. Ubuntu 24.04's `liblzma-dev` is `5.6.1+really5.4.5-1ubuntu0.4`, which is xz 5.4.5. A first run that searched for `5.6.1` found no whole string, which was the search's error, not the binary's.

## Results

`probe.sh` output, x86_64 container on an aarch64 host:

```
liblzma-dev 5.6.1+really5.4.5-1ubuntu0.4
a.static dynsym_def=0 symtab_def=2 whole_string_5.4.5=1 lzma_text_lines=171
a.stripped dynsym_def=0 symtab_def=0 whole_string_5.4.5=1 lzma_text_lines=0
a.shared dynsym_def=0 symtab_def=0 whole_string_5.4.5=0 lzma_text_lines=2
b.static dynsym_def=0 symtab_def=2 whole_string_5.4.5=1 lzma_text_lines=171
b.stripped dynsym_def=0 symtab_def=0 whole_string_5.4.5=1 lzma_text_lines=0
b.shared dynsym_def=0 symtab_def=0 whole_string_5.4.5=0 lzma_text_lines=4
```

- `lzma_text_lines` counts `strings -a` lines containing `lzma_`. In the unstripped static builds they are symbol names from `.strtab`.
- In the stripped builds, `strings -a -n 4 | grep -iE 'lzma|xz|tukaani|lasse'` gives three lines: `7zXZ`, `XZ1J` and `XZM4P?`. The first is the xz stream header magic `FD 37 7A 58 5A 00`. The other two are not text (inference from their shape; not traced to a symbol).
- In `a.stripped`, `5.4.5` sits between `%d %zu` (the program's own format string) and `../sysdeps/x86/dl-cacheinfo.h` (glibc).

waybill at `b6ad3c26` (debug build), `waybill sbom scan --path <dir> --offline` on each binary alone, with `RUST_LOG=waybill::version_census=debug`:

| Binary | liblzma component | `version_census` lines |
|---|---|---|
| `a.static`, `b.static` | none | 0 |
| `a.stripped`, `b.stripped` | none | 0 |
| `a.shared` | only `pkg:generic/liblzma.so.5`, the file-level dynamic-linkage component | 2 |
| the package's shared `liblzma.so.5` (positive control) | `pkg:generic/liblzma@5.4.5` | 2 |

The built binaries are not committed: they contain real liblzma and glibc code.
