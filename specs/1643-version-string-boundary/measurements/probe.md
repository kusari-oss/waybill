# R1: #1568 and #1641 re-verified at the current `main`

**Binary:** waybill built at `83657af2` (`main` after #1659; `version_strings.rs` unchanged since `5e4e36b8`, #1526).
**Inputs:** local Nix store copies, not committed:
- `/nix/store/04jn5rzj03wm6nv227fbvnxqx2pidins-zlib-1.3.2/lib/libz.1.3.2.dylib`
- `/nix/store/382l3srb7dy2s7hs1xvkrf359vckp668-ncurses-6.6/lib/libncursesw.6.dylib`

**Command:** `measurements/probe.sh <waybill> <libz> <libncursesw> <workdir>`. Each copy differs from the original in one byte, the byte before the literal; each copy is scanned alone, three times.

| library | byte before the literal | offset | run 1 | run 2 | run 3 |
|---|---|---|---|---|---|
| zlib | `0x00` (unmodified) | `0x12567` | `zlib@1.3.2` | `zlib@1.3.2` | `zlib@1.3.2` |
| zlib | `0x2d` `-` | `0x12567` | `zlib` | `zlib` | `zlib` |
| zlib | `0xff` | `0x12567` | `zlib` | `zlib` | `zlib` |
| zlib | `0x78` `x` | `0x12567` | `zlib` | `zlib` | `zlib` |
| ncurses | `0x00` (unmodified) | `0x4b845` | `ncurses@6.6` | `ncurses@6.6` | `ncurses@6.6` |
| ncurses | `0x2d` `-` | `0x4b845` | none | none | none |
| ncurses | `0xff` | `0x4b845` | none | none | none |
| ncurses | `0x78` `x` | `0x4b845` | none | none | none |

All PURLs are `pkg:generic/…`. The unversioned `pkg:generic/zlib` is the symbol fingerprint; the embedded version is what is lost. Both issues reproduce as reported. The `x` rows are what FR-002 must keep: under the new rule they stay as they are.

**At the wired commit (T-later)**, the expected result is `-` and `0xff` versioned like `0x00`, and `x` unchanged.
