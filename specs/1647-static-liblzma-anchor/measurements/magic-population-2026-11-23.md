# The xz header magic on an Ubuntu 24.04 install (research R2)

Byte-level facts only, from `readelf` and `strings`. These scripts do not apply waybill's candidate rule; the log-only census does that with waybill's own code (FR-001).

- **Image**: `ubuntu:24.04`, x86_64 (`--platform linux/amd64`), with `binutils file libarchive-tools busybox-static kmod systemd python3 7zip zstd rpm squashfs-tools xz-utils libarchive13t64` installed.
- **Versions**: `7zip=23.01+dfsg-11 busybox-static=1:1.36.1-6ubuntu3.1 kmod=31+20240202-2ubuntu7.2 libarchive13t64=3.7.2-2ubuntu0.9 liblzma5=5.6.1+really5.4.5-1ubuntu0.4 rpm=4.18.2+dfsg-2.1build2 squashfs-tools=1:4.6.1-1build1 systemd=255.4-1ubuntu8.19 xz-utils=5.6.1+really5.4.5-1ubuntu0.4`.

## Which ELF files hold `FD 37 7A 58 5A 00` (`probe/magic-pop.sh`)

Every regular file on the root filesystem that is executable or named `*.so*` and starts with `\x7fELF`: **1,042 files, 13 with the magic.**

| File | defines `lzma_version_*` (`.dynsym`) | imports `lzma_*` |
|---|---|---|
| `/usr/lib/x86_64-linux-gnu/liblzma.so.5.4.5` | 2 | 0 |
| `/usr/bin/xz` | 0 | 37 |
| `/usr/lib/x86_64-linux-gnu/libarchive.so.13.7.2` | 0 | 16 |
| `/usr/bin/kmod`, `libkmod.so.2.4.1` | 0 | 3 each |
| `libxml2.so.2.9.14` | 0 | 4 |
| `libmagic.so.1.0.0` | 0 | 3 |
| `libdw-0.190.so` | 0 | 3 |
| `/usr/bin/tar` | 0 | 0 |
| `/usr/bin/busybox` | 0 | 0 |
| `/usr/lib/7zip/7zr`, `7za`, `7z.so` | 0 | 0 |

One of the 13 is liblzma. The other 12 detect or decode xz themselves, or call liblzma and test the magic first.

## Their bare `X.Y.Z` strings (`probe/magic-cands.sh`, `probe/sect.sh`)

`strings -a -n 5 | grep -E -x '[0-9]+\.[0-9]+\.[0-9]+(alpha|beta)?'` is an approximation of a whole-string candidate, not waybill's rule:

| File | bare `X.Y.Z` strings |
|---|---|
| `liblzma.so.5.4.5` | `5.4.5` |
| `libarchive.so.13.7.2` | `1.5.5`, `1.9.4`, `5.4.5` |
| `7zr`, `7za` | `13.2.0` |
| the other 10 | none |

Located with `readelf -W -p .rodata`:

- **`7zr`**: `13.2.0` is in `.rodata` at `0x1fc0`, a NUL-terminated string directly after `Compiler: ` (ending at `0x1fbf`) and before ` GCC `. It is the compiler's version from 7-Zip's own banner, not liblzma's. 7-Zip implements xz itself and links no liblzma.
- **`libarchive.so.13.7.2`**: `5.4.5`, `1.9.4` and `1.5.5` are consecutive `.rodata` strings at `0x2b8b`, `0x2b91` and `0x2b97`: the liblzma, lz4 and zstd versions libarchive was built against, beside ` zlib/`. libarchive imports liblzma; it contains no copy.

## What this means for FR-004 (inference, confirmed by the census)

FR-004's second condition requires that no file in the false-positive population gets a version it does not contain. With the magic as an anchor, `7zr` and `7za` would have one candidate, `13.2.0`, and would be emitted as `pkg:generic/liblzma@13.2.0`. libarchive would have three candidates and be ambiguous, so it would get no version.

If waybill's census at the log-only commit agrees about `7zr`, the second condition fails, and FR-004 rejects the magic without needing the build matrix for its first condition.
