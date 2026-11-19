# T021: TheRock source tree

`kusari-sandbox/waybill-fixture-therock-sdk` measure-source runs 44600904259 and 44600923235: branch `c183fe3e` against `main` `1647ee01` (spec 1615 merged), TheRock `b1cae390` with submodules, CI only.

## Scan time (SC-005)

**0.998× and 0.997×** `main`'s median (3 interleaved runs after a warm-up). Budget ≤ 1.05×.

## Scopes (SC-001 to SC-003)

- **Counts.** 3,660 components and 3,692 edges on both sides. None added or removed.
- **SC-001: all 11 packages are required, with no C62:** pygithub@2.8.1, pynacl@1.6.1 and @1.6.2, numpy@1.26.4, certifi@2025.11.12, cryptography@46.0.3 and @46.0.5, pyjwt@2.14.0 and @2.9.0, requests@2.32.4, typing-extensions@4.13.2.
- **SC-002 and SC-003: 63 components change scope, and every one goes from C62 `transitive` (non-runtime) to required.** None gains a non-runtime scope.

| Before | Components |
|---|---|
| `transitive`, no C155 | 24 |
| `transitive`, C155 `docs` | 24 |
| `transitive`, C155 `ci` | 12 |
| `transitive`, C155 `test` | 3 |

The 39 with C155 are also declared in scoped requirements files. Now that they are runtime, spec 1613's R8 removes the label, as it does for any runtime component.

**Traced.** Each of the 63 is reached over runtime components from at least one runtime main module that is now a seed. By the branch document's graph, the seeds involved are (a component may have several):
- `llvm` 47;
- `nccl4py` 7;
- `origami`, `rocm-composable-kernel` and `rocprof-compute` 4 each;
- `stinkytofu`, `tensile` and `geko` 3 each;
- `hipfile`, `rocshmem4py`, `rocke-library`, `uhd-gen` and `rocke` 1 each.

By ecosystem, 62 are pypi and 1 is generic.

**The estimate in #1618 was wrong.** The issue's graph walk gave about 250 for `main` before spec 1615. Measured now, after spec 1615, it is 63.

## Formats, identity, repeats (SC-004)

- **`waybill sbom parity-check`:** the same count-row gaps as `main` (A12, B1, B2). C42, C62 and C155 agree across the formats.
- **Integrity:** 0 duplicate `bom-ref`s, and 0 unreachable components without an orphan reason, on both sides.
- **Runners:** CycloneDX, SPDX 2.3 and SPDX 3 are identical across the two runners once serial, timestamps and document namespace are removed.
