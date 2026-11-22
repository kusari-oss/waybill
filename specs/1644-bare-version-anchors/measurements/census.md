# T010: census (FR-001, SC-001)

## Census 1, at the first log-only commit

- **Run**: 45035417841 of `census-versions.yml` in the TheRock SDK fixture repository, at fixture commit `68d84f9`. Dispatched `--ref main -f waybill_ref=62198549b9c110664788767ead62e8fc8bc0c6c2`.
- **Jobs**: 20 of 20 succeeded: `control`, `sdk`, and 18 `corpus` jobs.
- **Commit check**: every job's `refs.txt` names waybill `62198549`.
- **SDK tarball**: sha256 `efeb6b07211030d0dea0f89e15017fe269f80c8811e22b61799bfb32b0c8bc5a`, the pinned tarball.
- **Positive control**: the `control` job scanned `census/anchored_control.py`'s synthetic tree and asserts its result in the workflow. It saw exactly bzip2 `versioned 1.0.8`, gmp `versioned 6.3.0` and liblzma `versioned 5.8.1`, and nothing for the library with no anchor. A zero elsewhere therefore means no anchored binary, not a broken probe. `pants-backend-ai` still reports its 4 spec 1643 pattern events, so the corpus jobs read the events as before.
- **Public corpus**: run 45035420298, dispatched with `-f branch=1644-bare-version-anchors`. Its summary reads `Scanned commit: 62198549…`, and the suite passed 166 of 166. That suite compares every target with its committed golden, so the log-only commit changes no public-corpus output.

### Anchored binaries

| Target | Anchored binaries | Verdicts |
|---|---|---|
| therock-sdk | 7 | bzip2 versioned 2, gmp no-candidate 2, liblzma versioned 3 |
| control (synthetic) | 3 | versioned 3 |
| the 18 quality-corpus targets | 0 | — |

TheRock, by binary (under the SDK root):

| Binary | Library | Anchor(s) | Candidates | Verdict |
|---|---|---|---|---|
| `lib/rocm_sysdeps/lib/libbz2.so` | bzip2 | text `bzip2/libbzip2` | `1.0.8, 13-Jul-2019` | versioned 1.0.8 |
| `lib/rocm_sysdeps/lib/librocm_sysdeps_bz2.so` | bzip2 | same | same | versioned 1.0.8 |
| `lib/rocm_sysdeps/lib/libgmp.so` | gmp | text `GNU MP:` | none | no-candidate |
| `lib/rocm_sysdeps/lib/librocm_sysdeps_gmp.so.10` | gmp | same | none | no-candidate |
| `lib/rocm_sysdeps/lib/liblzma.so` | liblzma | symbols `lzma_version_string`, `lzma_version_number` | `5.8.1` | versioned 5.8.1 |
| `lib/rocm_sysdeps/lib/librocm_sysdeps_liblzma.so.5` | liblzma | same | same | versioned 5.8.1 |
| `lib/rocm_sysdeps/lib/librocm_sysdeps_liblzma.so.5.8.1` | liblzma | same | same | versioned 5.8.1 |

- **Ambiguous**: 0. **Ambiguous-shared**: 0.
- **bzip2's date form without its anchor (FR-006)**: 0 binaries in any of the 19 targets.

### What main's document already names (FR-001, third bullet)

These are read from each target's own document at the log-only commit, whose output equals `main`'s.

| Target | Component | Source files |
|---|---|---|
| therock-sdk | `pkg:generic/bzip2?file-sha256=adea0650…` (no version) | `lib/rocm_sysdeps/bin/bzip2`, the executable |
| therock-sdk | `pkg:generic/gmp@6.3.0` | `lib/libgmp.so`, `lib/pkgconfig/gmp.pc` |
| therock-sdk | `pkg:generic/liblzma@5.8.1` | `lib/cmake/liblzma/liblzma-config-version.cmake`, `lib/liblzma.so`, `lib/librocm_sysdeps_liblzma.so.5.8.1`, `lib/pkgconfig/liblzma.pc` |
| python-pytorch | `pkg:github/tukaani-project/xz@v5.8.1` | `cmake/External/aotriton.cmake`, a source fetch with no binary |

TheRock ships `gmp.pc` and `liblzma.pc` under upstream's names, so FR-007's slugs `gmp` and `liblzma` land on the existing components. TheRock ships no `bzip2.pc`. The only bzip2 component is the unversioned file-level one for the executable, which is a different file from the libraries, so bzip2 gains a component rather than a duplicate. The final trace (T018) confirms all of this.

### FR-003: a text anchor for xz

The SDK job saved the strings of each liblzma-anchored binary (`lzma-strings/`). The three files are copies of one build: their strings are identical. Its only text naming the library is error messages from the API added in 5.4.0, such as "Unexpected NULL pointer argument(s) to lzma_str_to_filters()". That is one build, and an API older liblzma lacks. The symbol anchor already versions all three files, so **FR-003 keeps the symbol anchor alone**. A text anchor for statically linked liblzma, whose symbols are hidden, is a follow-up.

### GMP: no candidate, explained

Both GMP binaries carry the anchor but no candidate. #1641 saw `6.3.0` with `strings -a`, so where the bytes sit was inspected rather than guessed. Run 45037988231 of the fixture repository's read-only `inspect-bytes.yml` (fixture commit `75ef06d`, same tarball) extracted the two members: `libgmp.so` is a symlink to `librocm_sysdeps_gmp.so.10`. It located every `6.3.0`:

```
== lib/rocm_sysdeps/lib/librocm_sysdeps_gmp.so.10 (545688 bytes)
   section .rodata   type=1 flags=0x2 off=0x60000 size=0xd360
   at 0x60300 in .rodata: before='…\xcb\xed\xd7Y\xc3\xe5\x0f\x11;]\xc7I3U\xff'
                          from  ='6.3.0\x00\x00\x00…'
   occurrences: 1
```

The one `6.3.0` is in `.rodata`, at a 16-byte-aligned offset, NUL-terminated, and preceded by `0xff`, the last byte of a constant table. That is #1641's ncurses layout. FR-004 as first written required a NUL before the candidate, so it could never accept GMP here.

**Decision: FR-004 amended** (see the spec). A candidate may also follow a byte that is not printable text. A version inside text is still rejected: `requires 6.3.0 or later` and `x6.3.0` are each one printable string. The wider start could add candidates in the anchored binaries and turn a verdict ambiguous, so it is counted by census 2 before anything is wired.

## Census 2, with FR-004 amended

Pending: dispatched at the commit that carries the amended rule.
