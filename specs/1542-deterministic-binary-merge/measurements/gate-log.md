# Pre-PR gate (T020)

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, run alone, no `+toolchain`, at `5a1c1d82`.

- **Result:** `GATE_EXIT=0` and `GATE_RESULT=pass`; the expected line is present.
- **Counts:** 412 `test result: ok` lines; **8,819 passed, 0 failed**; 0 failure blocks.
- **Cargo files:** unchanged against `main`; `git diff --quiet main -- Cargo.toml Cargo.lock '*/Cargo.toml'` exits 0 (FR-008).

**The first attempt failed once**, in `gradle_ladder::sc005_subprocess_timeout_degrades_gracefully`. It took 15.1 s against its 13 s bound: the stub Gradle subprocess ran its full 15 s sleep, so the 3 s timeout did not take effect.
- It is **#1492**, open, with exactly this symptom under full-suite load.
- In isolation it passed 5 of 5.
- This branch changes only `resolve/deduplicator.rs` and `generate/graph_completeness/mod.rs`, which the Gradle subprocess path does not reach.
- The rerun above passed with no test failing.
