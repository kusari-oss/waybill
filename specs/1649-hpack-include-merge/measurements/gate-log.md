# T023: the pre-PR gate

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with default features and no `+toolchain`. Three runs:

| Run | Commit | Result | Totals | Cause |
|---|---|---|---|---|
| 1 | `5cbd509c` | fail | 11,130 passed, 1 failed | `haskell_hpack_us1::m1273_us1_unmodelled_construct_is_announced` asserted milestone 1273's rule that an `!include` is unmodelled. Its fixture's include is resolvable, so under this spec the manifest reads in full. Fixed in `d4a683b9`: the announcement arm now uses an unresolvable include (`include-unresolved`), and `s1649_m1273_include_fixture_now_reads_fully` pins the new behaviour |
| 2 | `d4a683b9` (after `cargo clean`: `target/debug/deps` held 57,794 entries) | fail | 11,131 passed, 1 failed | `gradle_ladder::sc005_subprocess_timeout_degrades_gracefully` took 15.09 s against a 10 s bound. This is #1492, open: "took the mock's full 15s sleep under full-suite load". Alone it passed 8 of 8 at 3.17–3.22 s. Gradle code is untouched by this spec |
| 3 | `d4a683b9` | **pass** | 432 `test result` lines, all `ok`; **11,132 passed, 0 failed**; 0 failure blocks | — |

Run 3's log ends with:

```
GATE_NAME=prepr
GATE_EXIT=0
GATE_RESULT=pass
```

and contains `>>> all pre-PR checks passed.` once.

`git diff ea149065 -- Cargo.lock '*Cargo.toml'` is empty (0 lines): no new dependencies.
