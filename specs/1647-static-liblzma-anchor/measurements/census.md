# T009: census at the log-only commit

## Runs

- **Log-only commit**: `7c78ac6c52d414e409cc94900600f19aa7ba03a4` (T008). Output equals `main`'s (`control.md`).
- **TheRock and the quality corpus**: `census-versions.yml` run 45201916920 in kusari-sandbox/waybill-fixture-therock-sdk, dispatched `--ref main -f waybill_ref=7c78ac6c…`.
  - The fixture repository was at `87bc1e6`, which adds the `liblzma_static` summary and the static control.
  - 20 of 20 jobs succeeded, and all 20 `refs.txt` files name `7c78ac6c`.
- **Public corpus**: `public-corpus.yml` run 45201924694, dispatched with `-f branch=1647-static-liblzma-anchor`.
  - The "Resolve the scanned commit" step reads `scanned commit: 7c78ac6c52d414e409cc94900600f19aa7ba03a4`.
  - `test result: ok. 166 passed; 0 failed`.
  - The harness keeps no census log, so FR-001 is measured there as output (as amended): **unchanged at the log-only commit**.

## Positive control: a real static liblzma on the runner

The `control` job built `census/static_lzma/a.c` (this spec's `probe/a.c`) with `gcc -O2 -static -llzma` against `liblzma-dev 5.6.1+really5.4.5-1ubuntu0.4`. It scanned one unstripped copy and one stripped copy, and the job's assertion passed: `spec 1647 control ok: log-only 5.4.5 [] stripped event`.

| File | ELF type | Anchors | `current` | Rule A | Rule B |
|---|---|---|---|---|---|
| `waybill-fixture-xzstatic` (unstripped) | exec | `lzma_version_number`, `lzma_version_string`, both `symtab`, `global` | none | liblzma `versioned` `5.4.5` | — |
| `waybill-fixture-xzstripped` | exec | none; magic present | none | — | liblzma `versioned` `5.4.5` |

Both rules fire on a real static build. A zero elsewhere therefore means no such file, not a broken probe.

The job's other controls still pass in their wired states: spec 1644's `control ok`, spec 1645's `identity control ok`, and `spec 1646 control ok: wired`.

## TheRock SDK

**Rule A: no file.** No ELF file in the SDK defines `lzma_version_string` or `lzma_version_number` in `.symtab` without also defining it in `.dynsym`. Every liblzma counted by spec 1644 (3 shared libraries) is unchanged by this spec. Reading `.symtab` therefore changes nothing on TheRock.

**Rule B: 5 files hold the xz header magic and no anchor symbol.** For all 5, rule B's verdict is liblzma `no-candidate`, so it would version none of them:

| File | ELF type | Rule B |
|---|---|---|
| `lib/librocm-dbgapi.so`, `lib/librocm-dbgapi.so.0`, `lib/librocm-dbgapi.so.0.80.0` | dyn | liblzma `no-candidate` |
| `lib/rocm_sysdeps/lib/libdw.so`, `lib/rocm_sysdeps/lib/librocm_sysdeps_dw.so.1` | dyn | liblzma `no-candidate` |

## Quality corpus

18 targets: **0 `liblzma_static` events.** No file defines a `.symtab`-only anchor, and no file holds the xz header magic without one.

## The Ubuntu false-positive population (local, not TheRock)

These are the 13 files from `magic-population-2026-11-23.md`, extracted from `ubuntu:24.04` with the same packages. Each was scanned alone with the log-only build (`7c78ac6c`'s code) and the census on. Versions: `7zip=23.01+dfsg-11`, `busybox-static=1:1.36.1-6ubuntu3.1`, `kmod=31+20240202-2ubuntu7.2`, `libarchive13t64=3.7.2-2ubuntu0.9`, `libdw1t64=0.190-1.1ubuntu0.1`, `liblzma5=5.6.1+really5.4.5-1ubuntu0.4`, `libmagic1t64=1:5.45-3build1`, `libxml2=2.9.14+dfsg-1.3ubuntu3.10`, `tar=1.35+dfsg-3ubuntu0.4`, `xz-utils=5.6.1+really5.4.5-1ubuntu0.4`.

| File | Event | Rule B verdict (liblzma) |
|---|---|---|
| `liblzma.so.5.4.5` | none: anchored by `.dynsym` | — (production gives `liblzma@5.4.5`, the positive control) |
| `7zr`, `7za` | rule B | **`versioned` `13.2.0`** (candidate `13.2.0`) |
| `libarchive.so.13.7.2` | rule B | `ambiguous` (`5.4.5`, `1.9.4`, `1.5.5`) |
| `busybox`, `kmod`, `tar`, `xz`, `7z.so`, `libdw-0.190.so`, `libkmod.so.2.4.1`, `libmagic.so.1.0.0`, `libxml2.so.2.9.14` | rule B | `no-candidate` |

Of the 12 files that are not liblzma, rule B would give 2 a false version, `liblzma@13.2.0`. That is 7-Zip's compiler version, from a program with its own xz implementation and no liblzma (`magic-population-2026-11-23.md`).

## Checkpoint (T010)

- **FR-004: rejected.** Its second condition, no false version in the false-positive population, fails on `7zr` and `7za`, measured with waybill's own verdict code. One failed condition suffices, so the build matrix for the first condition is not built. Spec FR-004, contracts/wire.md S8 and the data model are amended.
- **FR-002: confirmed.** Rule A versions the runner's real static build, and no counted file contradicts it: no local-binding, relocatable-object or non-liblzma case appeared. It changes nothing on TheRock or the quality corpus.
- **The scope is stated plainly.** This spec does not change the ROCm SDK's output. It versions unstripped static liblzma wherever that appears; none of the trees counted here contains one.
