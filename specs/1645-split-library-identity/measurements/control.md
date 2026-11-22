# Control: spec 1645 at the log-only commit (T001, T008, T009)

## Binaries

- **`main`**: `bb9d6cc485aa13043891c39c50af70669dd40106`, a debug build saved as the scratchpad's `waybill-main-bb9d6cc4` (`waybill 0.17.0`). The branch's code was identical to `bb9d6cc4` when it was built: `git diff bb9d6cc4 -- waybill-cli waybill-common Cargo.toml Cargo.lock` gave 0 lines.
- **Branch**: the log-only commit. It adds:
  - `CuratedLibrary::names` and `from_slug`;
  - the two stated-version parsers, not yet used by the scanner;
  - `library_identity::reconcile` with `log_only = true`;
  - the ncurses CPE split, not yet used;
  - the three census events.

  Nothing is emitted from any of them.

## Every case, both binaries

`cargo test -p waybill --test library_identity -- --include-ignored`. For the `main` column, `WAYBILL_LIBRARY_IDENTITY_BIN` points at `main`'s binary.

Totals:
- `main`: 2 passed, 8 failed. After N-4 was strengthened (below): 1 passed, 9 failed.
- Branch: 6 passed, 4 failed. The 4 are exactly the planned red cases.

| Case | `main` | log-only branch | Meaning |
|---|---|---|---|
| m1_name_variant_is_one_component | FAILED (`["pkg:generic/ncurses@6.6"]` beside `ncursesw`) | FAILED (same) | red until T014 |
| m2_version_suffix_is_one_component | FAILED (`llvm@23.0.0`, `llvm@23.0.0git`) | FAILED (same) | red until T011 |
| m3_dotted_prefix_takes_the_record_version | FAILED (`expat@2.7.5`, `expat@2.7.5.1`) | FAILED (same) | red until T014 |
| e1_requires_follows_the_merge | FAILED | FAILED | red until T014 |
| c1_census_reports_the_shared_file | FAILED (no event) | ok | positive control: the census fires, verdict `merge` |
| n1_disagreeing_version_keeps_both | FAILED (no verdict) | ok | guard; the verdict is read from the census |
| n2_bundled_library_is_not_merged | FAILED (no verdict) | ok | as above, `name-unrelated` |
| n3_string_prefix_does_not_agree | FAILED (no verdict) | ok | as above, `version-disagrees` |
| n4_two_qualifying_records | FAILED once strengthened | ok | as above, `ambiguous` |
| c2_no_record_no_event | ok | ok | negative control |

The N cases' component assertions hold on `main` too, because nothing merges there. Their verdict assertions are what fail on `main`; their teeth against a merge come from the mutations (T016).

## N-4 is reachable from real readers

N-4's integration tree combines `ncursesw.pc`, which names `libncursesw.so.6`, with a CMake package `ncursestw` whose exported target's `IMPORTED_LOCATION_RELEASE` is the same file. Both records attached the file: the first branch run printed `N-4 integration verdicts: ["ambiguous"]`.

So the state the spec's edge case describes is reachable, not only in the unit test. The test was then made unconditional: it asserts `["ambiguous"]`, and that both `ncursesw` and `ncursestw` remain. Strengthened that way, it fails against `main`, which has no census, and passes on the branch.

## A test that was wrong after the new event

The first gate at this commit failed two of spec 1643's tests, `version_string_boundary.rs::c1_census_reports_a_non_identifier_byte` and `c2_census_reports_whitespace_separately`, each with `left: 2, right: 1`.

They counted every `waybill::version_census` line containing `"library":"ncurses"`. Spec 1645's `curated_version_change` for the same binary also names `ncurses`, so each test saw a second line. Their helper now matches ` hit=` with its leading space, the same fix spec 1644 made for `unanchored=`. The scanner was right; the tests' matcher was too broad. `version_string_boundary`: 154 passed, 0 failed.

## Identity control, run locally before dispatch

The fixture repository's `census/anchored_control.py OUT IDENTITY` tree was scanned with this commit's debug build, and its log was read by the extended `census/versions.py`. The result is exactly what the workflow's control step asserts:
- `shared_file`: `[("llvm", "version-disagrees"), ("ncurses", "merge")]`;
- `curated_version_change`: `[("llvm", "23.0.0", "23.0.0git"), ("ncurses", "6.6", "6.6.20251230")]`;
- `cpe_form_change`: `["pkg:generic/ncurses@6.4.20221231"]`.

## Unit tests at this commit

`cargo test -p waybill --bins s1645`: 13 passed, 0 failed.
- `version_strings`: names and slugs round-trip, the LLVM suffix, the ncurses date, `version_changes`.
- `library_identity`: dotted agreement, each verdict, what is not a finding, the merge rewrite with edges, the prefix merge, no rewrite without a merge, census JSON.
- `cpe`: the release/date split and the split CPEs.

## Gate at the log-only commit

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, after the test fix above:
- `GATE_EXIT=0`, `GATE_RESULT=pass`, and the expected line present;
- 428 `test result: ok.` lines, 0 `FAILED`, and no failure block;
- 11,028 passed, 0 failed, 51 ignored. The 51 are spec 1644's final 47 plus this spec's 4 planned red cases (M-1, M-2, M-3, E-1).
