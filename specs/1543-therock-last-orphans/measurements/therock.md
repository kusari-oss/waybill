# TheRock (T022, T023)

Measured in CI only, in `kusari-sandbox/waybill-fixture-therock-sdk`, branch `75dd0922` against `main` `262712227403`, TheRock `b1cae390`, 2026-11-17. Documents probed locally with the corpus harness's `probe_i7_on_document` and `probe_compare_documents`.

## Full source tree (`measure-source` run 44231264444, `sub` mode, `--root-name TheRock --root-version 10.3.0`)

| | `main` | branch |
|---|---|---|
| unreachable components (dependency edges and containment) | 1,677 | 1,675 |
| explained | 1,675 | 1,675 |
| **unexplained** | **2** (`pkg:generic/tensile`, `pkg:generic/wkmi@1.0.0`) | **0** |
| C105 orphan count | 410 | 408 |
| formats agree on C45 / C104 / C105 | yes / yes / yes | yes / yes / yes |
| SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` relationships | 1,170 | 1,171 |

- `TheRock@10.3.0 → pkg:generic/tensile` (R1 anchor).
- `pkg:github/rocm/rocm-systems@c1ab6df5… → pkg:generic/wkmi@1.0.0` (owner: the `rocm-systems` submodule). `wkmi`: CycloneDX `scope: excluded`, C122 `cmake-conditional-link`, C235 `["WIN_SDK"]`; C235 is `["WIN_SDK"]` in SPDX 2.3 and SPDX 3 too (SC-003).
- **Every other difference from `main`: none.** A normalised, `LC_ALL=C`-sorted diff of every component (PURL, scope, orphan reason, C122, C235) and every edge shows exactly: `wkmi`'s scope and two annotations, and the two edges above. 2,271 component lines and the subject (`TheRock@10.3.0`) unchanged.
- `nosub` mode (no submodules): component and edge sets identical to `main`.
- **Time** (3 runs per side after a warm-up): `main` 101.3 / 100.6 / 99.1 s, median 100.6; branch 98.0 / 102.1 / 99.5 s, median 99.5. **Ratio 0.990×** (SC-006 ≤ 1.1×).
- Repeatability on the source tree is not measured by this workflow (it keeps one document per side); it is covered for the SDK below and for the public corpus (`audit.md`).

## Built SDK (`measure-full` run 44231267202)

- Branch document equal to `main`'s after masking (`probe_compare_documents`: `same`). The SDK has no workspace root and no vendored link, as expected.
- The 4 branch scans (warm-up, run0–2) are identical after masking.
- Time, 3 runs per side: `main` 38.3 / 42.0 / 39.6 s (median 39.6), branch 43.7 / 44.6 / 41.8 s (median 43.7), ratio 1.105×; warm-ups 53.2 (`main`) and 39.9 (branch). One sample; re-measured below.
