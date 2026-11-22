# Control: spec 1644 at the log-only commit (T001, T008, T009)

## Binaries

- **`main`**: `110d1a5895cf5fc10bfd42c78f5f36d09be33093`, a debug build saved as the scratchpad's `waybill-main-110d1a58` (`waybill 0.17.0`). The branch's code was identical to `110d1a58` when it was built (`git diff --stat 110d1a58 -- waybill-cli waybill-common Cargo.toml Cargo.lock`: 0 lines).
- **Branch**: the log-only commit. It adds the three variants, `anchor_symbols`, `anchored_verdicts` and the census event. Nothing is emitted from them yet.

## Every case, both binaries

`cargo test -p waybill --test version_string_anchored -- --include-ignored`, with `WAYBILL_VERSION_ANCHORED_BIN` set to `main`'s binary for the first column. Totals: `main` 8 passed and 14 failed; the branch 14 passed and 8 failed.

| Case | `main` | log-only branch | Meaning |
|---|---|---|---|
| a1_bzip2 | FAILED | FAILED | red until T011 |
| a2_gmp | FAILED | FAILED | red until T011 |
| a3_liblzma | FAILED | FAILED | red until T011 |
| a4_liblzma_prerelease | FAILED | FAILED | red until T011 |
| a5_liblzma_macho | FAILED | FAILED | red until T011 |
| a_cpe_candidates | FAILED | FAILED | red until T012 |
| p1_agreeing_pc_file_gives_one_component | FAILED | FAILED | red until T016 |
| p2_disagreeing_pc_file_keeps_both_versions | FAILED | FAILED | red until T016 |
| c1_census_reports_an_anchored_library | FAILED | ok | positive control: the census fires |
| defined_anchor_symbol_anchors_elf_and_macho | FAILED | ok | a defined symbol anchors, ELF and Mach-O |
| n4_gmp_two_candidates_is_ambiguous | FAILED | ok | the verdict is read from the census |
| n5_shared_candidate_versions_neither | FAILED | ok | as above |
| n6_version_inside_a_longer_string | FAILED | ok | as above |
| n7_liblzma_snapshot_suffix | FAILED | ok | as above |
| c2_no_anchor_no_census_event | ok | ok | negative control |
| helpers_mark_symbols_defined_and_undefined | ok | ok | the fixture helpers write what they claim |
| imported_or_unlisted_symbol_anchors_nothing | ok | ok | guard; teeth from mutation M3 |
| n1, n2, n3, n8, n9 | ok | ok | guards: no component on `main` because no pattern exists; teeth from mutations M1–M7 |

## Three cases that were wrong as first written, and one added

- **P-1 passed on the branch before the change.** The `.pc` file alone gives exactly one `pkg:generic/liblzma@5.8.1`, so "exactly one component" held without any embedded match. P-1 now also asserts that this component carries the library file, `libwaybill-fixture-lzma.so.5`. It fails on both binaries until T016.
- **N-6 passed against `main` vacuously.** It asserted that *every* `gmp` census event said `no-candidate`, and `main` logs none. It now asserts exactly one event, so it fails against `main` and passes on the branch.

- **C-2 failed in the gate after the census-only `unanchored` event was added** (`version_string_anchored.rs:280`, `left: 1, right: 0`). The helpers counted lines containing `anchored=`, which `unanchored=` also contains, and C-2's fixture holds the bzip2 date form with no anchor. Both helpers now match ` anchored=` with its leading space. The scanner was right; the test's matcher was not.
- **C-3** (added with the `unanchored` event) checks that bzip2's date form in a binary without the anchor is reported as `unanchored` and never emitted.

## Gate at the log-only commit

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`: `GATE_RESULT=pass`, with the expected line present. 427 `test result: ok.` lines, 0 `FAILED`; 10,998 passed, 0 failed, 55 ignored. The 55 are the 47 ignored on spec 1643's last gate plus this spec's 8 planned red cases.

## B-1

B-1 (a library with no anchor is byte-identical to `main`) is spec 1643's `version_string_boundary.rs::b1_nul_preceded_banners_are_byte_identical_to_main`, against its committed golden. It is unchanged and runs in the gate (`gate-log.md`).

## The CPE coverage test at this commit

`cpe.rs::mappings_cover_all_curated_libraries` requires a table row for every curated slug. `bzip2` has one. `gmp` and `liblzma` are on the test's `OMITTED` allowlist **for this commit only**. A row applies to every `pkg:generic/<slug>` component, whatever reader emitted it, so adding the rows here would change the CPEs of `gmp` or `liblzma` components that pkg-config or the closure tier already emit, and the census commit would no longer be output-neutral. T012 adds the rows and removes them from the allowlist.
