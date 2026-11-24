# Spec 1653: after the change

## T014: nothing changes without `--nix-closure` (FR-007, SC-005)

**What was scanned:** milestone 966's flake-pinned Haskell project (`tests/fixtures/nix_closure_boot_versions/project`).
- **Binaries:** `main` at `91d84d1` (built in a worktree) and this branch.
- **How:** through the same stub `nix`, using `NixStub`'s binary override from a temporary, uncommitted test, in three formats.
- **Masking:** UUIDs and timestamps, with sorted keys; then `diff -r`.

| case | differing lines |
|---|---|
| without `--nix-closure` | **0** |
| with `--nix-closure` (positive control) | 116 |

**The 116 lines are only:**
- the new `dependsOn` entries (`process` and `stm` → `base`);
- four C245 properties;
- C105, gaining `boot-library-edges-incomplete: 4 hackage boot librar(ies), 4 with dependencies not in the document, 0 without dependency data` after the existing `transitive-edges-unresolvable: hackage`.

**Not demonstrated:** the non-closure document carries no C218 at all. So it does not exercise the `matches!(nix_closure, Some(Ok(_)))` gate in `scan_cmd`, which stops the pass if milestone 926 ever stamps C218 without the closure. No input available here produces that, and the gate is kept as defence.

## T017: mutations (SC-004)

**How each mutation was run:**
1. Applied from a `cp` backup.
2. Ran `cargo test -p waybill --test boot_library_edges`, plus the named unit tests.
3. Restored with `cp`, then rebuilt.

| # | Mutation | Killed | Integration tests that fail | Unit tests that fail |
|---|---|---|---|---|
| M1 | `depends()` returns `None` | yes | `base_depends_on_ghc_prim_in_every_format`, `every_boot_library_carries_c245` | 4 in `boot_library_edges` |
| M2 | compiler key ignored (names matched across compilers) | yes | `edges_stay_within_a_compiler` | `edges_stay_within_a_compiler`, `a_dependency_absent_from_the_document_is_listed_not_synthesised` |
| M3 | classifier never pushes | yes | `incomplete_boot_library_edges_are_reported` | `boot_library_edges_incomplete_counts_absent_and_unknown` |
| M4 | `compositions` arm removed | yes | — (hackage is already degraded by `transitive-edges-unresolvable` on this fixture) | `incomplete_boot_library_edges_degrade_hackage` |
| M5 | C245 not emitted in SPDX 3 | yes, after the parity fix below | 5 of 6 | — |
| M6 | `absent` never filled | yes | `every_boot_library_carries_c245`, `incomplete_boot_library_edges_are_reported` | 2 |
| M7 | the pass reads a present store output | yes | `a_present_store_output_changes_nothing` | — |

### M5's first run survived, and why

**What happened:** removing C245 from SPDX 3 alone failed no test, although every scan runs `waybill sbom parity-check`.

**Why:**
- The tests' parity helper, copied from `tests/closure_resolved_tier.rs`, asserted on each row's `universal_parity` field.
- That field says whether the catalog *expects* a row in every format. It is not the result.
- The verdict exists only as `summary.parity_gaps`.

**Measured with that SPDX 3 document:** C245 reads `cdx present 4, spdx23 present 4, spdx3 absent`, with `universal_parity: true`, and `parity_gaps` goes from 3 to 4.

**This repository's helper now:**
- requires every universal row that any format carries to have the same presence and count in all three;
- allows only the three gaps `main` already has on closure scans of this fixture:
  - B1, dependency edges: 9 / 12 / 12 on `main`, and 11 / 14 / 14 here, so this spec adds 2 edges to every format alike;
  - B2: 4 / 1 / 1;
  - E1: CycloneDX absent, SPDX 1 / 1;
- bounds `summary.parity_gaps` by that count.

M5 is then killed by 5 of 6 tests.

**Beyond this spec:** 13 test files use the same helper, so their parity assertions cannot fail. They include the ones for specs 1650, 1651 and 1652. That and the three gaps on closure scans are filed separately.
