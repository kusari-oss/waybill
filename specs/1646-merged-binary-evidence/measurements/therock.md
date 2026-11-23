# T014: quality corpus and TheRock at the final code commit (US3, SC-001, SC-005, SC-007)

The final code commit is `491068473a0851f2798814890f5886d23d228222`. The docs commit after it (`8f7f328b`) changes only `docs/`. `main` is T001's control, `e114881803f149c8ef58884a63b21428464c3690`.

## Runs

| Run | Workflow (TheRock SDK fixture repository) | Dispatch | Jobs | Commit check |
|---|---|---|---|---|
| 45118967051 | `measure-full` | `--ref main`, base `e1148818…`, branch `49106847…` | 1 of 1 succeeded | the log names `BASE_REF: e1148818…` and `WAYBILL_REF: 49106847…` |
| 45118970005 | `census-versions.yml` | `--ref main`, `waybill_ref=49106847…` | 20 of 20 succeeded | all 20 `refs.txt` name `49106847` |

The `control` job reports:
- `spec 1646 control ok: wired ['pkg:generic/llvm@23.0.0git', 'pkg:generic/ncurses@6.6.20251230'] []`;
- `identity control ok: [('ncurses', 'merge')] [] []`;
- spec 1644's `control ok`.

## TheRock: every changed component

From `measure-full`'s `base` and `branch` documents:

| Measure | `main` | Branch |
|---|---|---|
| Components, recursive (PURL, CPE, name, version) | 3,282 | 3,282, `diff` 0 lines |
| CycloneDX dependency edges | 3,901 | 3,901, `diff` 0 lines |

Identity methods (`evidence.identity[0].methods[].technique`), every component that changed:

| Component | `main` | Branch | Traced to |
|---|---|---|---|
| `expat@2.7.5`, `gmp@6.3.0`, `liblzma@5.8.1`, `llvm@23.0.0git`, `ncurses@6.6.20251230`, `zlib@1.3.2` | `manifest-analysis` | `manifest-analysis`, `binary-analysis` | FR-002: each is a pkg-config or CMake record that absorbed the binary match (census: exactly these 6 PURLs merged with a record) |
| `bzip2@1.0.8` | `manifest-analysis` | `binary-analysis` | FR-003: an unmerged banner match |
| `pkg:generic/sqlite` | `manifest-analysis` | `binary-analysis` | FR-003: an unmerged symbol-fingerprint match (census T009) |

Nothing else changed. In both SPDX formats, exactly these 6 merged components' D1 payload gains `"methods": [{"technique": "package-database", "confidence": 0.85}, {"technique": "binary-analysis", "confidence": 0.85}]`, and keeps the flat `package-database` keys.

## parity-check

`waybill sbom parity-check --format json` on each arm's three documents. Both arms report 260 universal rows, 258 passing, and the same 2 by-design gaps.

The only changed row is D1 (evidence identity, presence-only): 2/2/2 distinct values on `main` and 4/4/4 on the branch, equal in all three formats. That is the two new payload shapes, the binary method alone and the two-method payload.

## Quality corpus

Each target's CycloneDX components were compared between the log-only census (run 45116219040, whose output equals `main`'s) and this run: PURL, CPE, version, every property, and the identity methods.

- 17 targets: no change.
- `pants-backend-ai` (373 components): `openssl@3.5.4` and `zlib@1.3.1` change from `manifest-analysis` to `binary-analysis`. With methods excluded, the same comparison shows 0 differing lines. Both are unmerged banner matches (census T009).

## SC-007: timing

TheRock SDK scan, wall-clock seconds, from `timing.json`. Each arm had one warm-up, and the runs were interleaved as: base warm-up, branch warm-up, base, branch, branch, base, base, branch.

| Arm | min | median | max |
|---|---|---|---|
| `main` `e1148818` | 19.95 | 19.97 | 20.30 |
| branch `49106847` | 19.92 | 19.93 | 19.97 |

**Median ratio: 19.93 / 19.97 = 0.998**, within 1.05.
