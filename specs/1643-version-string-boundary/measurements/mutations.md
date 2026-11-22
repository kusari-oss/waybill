# T014: mutations (SC-003, research R8)

`measurements/mutate.py` applies each mutation to `version_strings.rs`, runs the named tests, classifies the result, and restores the file with `cp`. After every pass the source was compared with a pre-mutation copy (`cmp`: identical). A test command that runs no tests is reported as a harness error, never as survived or killed (added after pass 1).

## Final result: 7 of 7 killed

| id | mutation | killed by |
|---|---|---|
| M1 | anchored accepts only NUL (`main`'s rule) | unit `s1643_anchored_patterns_accept_a_non_identifier_byte`; integration `n2_ncurses_behind_dash` |
| M2 | zlib classed NUL-only | unit `s1643_every_variant_follows_its_class` |
| M3 | expat classed NUL-only | unit `s1643_every_variant_follows_its_class` |
| M4 | ncurses classed NUL-only | unit `s1643_every_variant_follows_its_class` |
| M5 | OpenSSL classed anchored | unit `s1643_every_variant_follows_its_class`; integration `u1_openssl_behind_dash_is_not_found` |
| M6 | identifier test removed (any preceding byte accepted) | unit `s1643_anchored_patterns_reject_an_identifier_byte`; integration `z4_zlib_behind_a_letter_is_not_versioned` |
| M7 | zlib tests the byte before `deflate`, not the byte before its space | unit `s1643_anchored_patterns_reject_an_identifier_byte`; integration `z4_zlib_behind_a_letter_is_not_versioned` |

The M1, M6 and M7 results are from pass 2. The M2–M5 results are from pass 3, after the test fix below. That fix touched only `s1643_every_variant_follows_its_class`, which none of M1, M6 or M7 relies on.

## How the passes went

1. **Pass 1: a harness error, not a result.** Each unit-test arm passed two name filters to `cargo test`, which cargo rejects (`error: unexpected argument`). The runner counted "no FAILED line" as survival.
   - M2–M4 were therefore reported as survivors without having run.
   - M1 and M5–M7 were reported as killed, on their integration tests alone.

   Fixed by giving `cargo test` one full-path filter, and by reporting any command with no `test result:` line as a harness error. A positive control then ran each unit filter on unmutated code: 1 test each.
2. **Pass 2: M2–M4 survived for real.** M5 was killed only by the integration test. The cause was `s1643_every_variant_follows_its_class`, which compared the scanner's result with `CuratedLibrary::boundary()`. The scanner reads `boundary()` itself, so moving a library to another class changed both sides and the test still passed.

   The test now takes its expectation from FR-001's table, written out as `ANCHORED`, and asserts `boundary()` against it as well.
3. **Pass 3: M2–M5 killed** by the corrected test.
