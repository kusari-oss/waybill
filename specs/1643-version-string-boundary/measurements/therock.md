# T018: TheRock SDK and the quality corpus (SC-005, SC-006)

## Quality corpus

`census-versions` run 44995039054 at `4caee6a1` (the wired commit): 19 of 19 jobs succeeded, and every `refs.txt` names `4caee6a19ecbcaeb5434f1028d0009b8e2575016`. Each target's CycloneDX document was compared with the log-only control's (run 44990809831 at `d3dcd676`), by component `bom-ref` and by dependency entry:

- **All 18 quality-corpus targets:** identical components and dependencies. Only `metadata.timestamp` and `serialNumber` differ.
- **TheRock SDK** (the same tarball, sha256 `efeb6b07…`): the changes below.

## TheRock SDK: every change, traced

Component count: 3,282 → 3,282 (one removed, one added).

| change | component | cause |
|---|---|---|
| removed | `pkg:generic/zlib` (unversioned; `waybill:evidence-kind = symbol-fingerprint`, the 6 zlib files) | the 6 files now carry the curated `zlib@1.3.2` match (census: 6 zlib behind `-`) and merge into the existing versioned component |
| changed | `pkg:generic/zlib@1.3.2` | gains the 6 `.so` files in C18 `waybill:source-files`, `waybill:fingerprint-symbols-matched = 10/10`, and C120 `waybill:workspace-member` gains `lib/rocm_sysdeps/lib` and `lib/rdc/grpc/lib/rocm_sysdeps/lib` |
| added | `pkg:generic/ncurses@6.6` (`embedded-version-string`, `heuristic`, CPE `cpe:2.3:a:invisible-island:ncurses:6.6`) | the 2 ncurses files (census: behind `0xff`) |
| edge | root `sdk@0.0.0`: `-pkg:generic/zlib`, `+pkg:generic/ncurses@6.6` | follows the two component changes |
| document | `waybill:workspaces-detected` gains `lib/rdc/grpc/lib/rocm_sysdeps/lib` | from zlib's C120 |
| document | `waybill:cpe-coverage`: `unaddressable` 3133 → 3132, `with_candidates` 25 → 26 | the unversioned zlib removed; `ncurses@6.6` carries CPE candidates |

Every change comes from the 8 matches the census predicted. Nothing else moved.

**Finding: ncurses is reported twice.** `libncursesw.so` is also named by `pkg:generic/ncursesw@6.6.20251230` (pkg-config, `ncursesw.pc`, no CPE), which remains. A synthetic tree reproduces the pair with a NUL-preceded banner, which this spec does not change: `libncursesw.so.6` holding `\0ncurses 6.6.20251230\0` and `pkgconfig/ncursesw.pc` give both `ncurses@6.6` and `ncursesw@6.6.20251230`. It comes from how the curated scanner (#1526: slug `ncurses`, version `X.Y`) and the pkg-config reader (#1531: the `.pc` name and full version) name the same library. This spec exposes it on two more files. It is a follow-up, not part of this spec.

## SC-006: scan time

### First measurement: failed

`measure-full` run 44995037088: base `83657af2` (`main`), branch `4caee6a1`, read from the "Build both" lines. One warm-up per arm, then 3 interleaved runs per arm.

| arm | runs (s) | median |
|---|---|---|
| base `83657af2` | 25.635, 25.499, 26.165 | 25.635 |
| branch `4caee6a1` | 27.953, 27.998, 28.047 | 27.998 |

**Ratio 1.092: fails SC-006 (≤ 1.05).** The ranges do not overlap.

**Cause, measured.** `match_prefix` had become `pattern_at` followed by the rule, so every byte position compared all 15 prefixes. `main` returned at once for any position not preceded by NUL, which is most positions. A temporary ignored test (not committed) timed `scan` over a deterministic 16 MiB `.rodata`-shaped region: printable strings and binary constants, NUL-separated. Medians of 7 runs, release build:

| scanner | min | median | max |
|---|---|---|---|
| `main` `83657af2` | 0.0279 | 0.0279 | 0.0281 |
| branch `4caee6a1` | 0.4396 | 0.4961 | 0.5434 |
| fixed (below), 3 repetitions | 0.0368–0.0369 | 0.0370–0.0374 | 0.0374–0.0377 |

**Fix.** `match_prefix` now narrows the patterns by the preceding byte before comparing any prefix:
- after NUL, or at the region's start: every pattern;
- after an identifier byte: none;
- after any other byte: only the anchored patterns, plus zlib after a space.

That is equivalent to trying every pattern and then applying the rule. `s1643_census_agrees_with_the_scanner` checks this for every variant behind all 256 preceding bytes and at the region's start, since the census does try every pattern. The remaining 1.33× is in the string scan alone, against the 18× that cost 2.36 s. Whether the whole scan is within 1.05 is measured below, not inferred.

### Second measurement, after the fix: passes

`measure-full` run 45000670082: base `83657af2`, branch `1586d74b` (the "Build both" lines). One warm-up per arm, then 3 interleaved runs per arm.

| arm | runs (s) | median |
|---|---|---|
| base `83657af2` | 25.774, 25.793, 25.922 | 25.793 |
| branch `1586d74b` | 25.647, 25.692, 25.823 | 25.692 |

**Ratio 0.996: passes SC-006.** The branch's range sits inside the base's.

**The fix changes no output.** The branch documents at `1586d74b` were compared with those at `4caee6a1` (run 44995037088):
- **CycloneDX:** identical components and dependencies; only `metadata.timestamp` and `serialNumber` differ.
- **SPDX 2.3:** 25,260 differing values, all timestamps (25,259 `annotationDate`, 1 `created`).
- **SPDX 3:** identical with timestamps masked.

**Base against branch (CycloneDX)** shows exactly the change traced above: `-zlib`, `+ncurses@6.6`, `~zlib@1.3.2`, and the root edge.

**Format parity.** `waybill sbom parity-check` gives the same summary for the base and branch documents: 258 of 260 universal rows passing, 2 gaps, with A5 (package author absent in CycloneDX and SPDX 2.3, `restricted` in SPDX 3) the one marked non-universal. The change adds no parity gap.
