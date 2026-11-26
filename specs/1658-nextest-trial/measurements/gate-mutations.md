# Gate mutations (T014)

Each mutation applied with an exact replacement, the nextest gate tests run, then reverted with `cp` and checked with `cmp`.

## M1: drop the summary --expect (`if expect {` → `if false {`)
```
        PASS  the_documented_gate_invocation_uses_this_summary_pattern
        PASS  pre_pr_refuses_to_run_without_nextest
        FAIL  a_build_that_does_not_compile_fails_the_summary_gate
        PASS  forced_colour_does_not_break_the_summary_gate
        FAIL  a_run_of_zero_tests_fails_the_summary_gate
         Summary [   0.000s] 0 tests run: 0 passed, 3 skipped
        FAIL  a_failing_test_fails_the_summary_gate_even_with_exit_zero
            PASS [   0.007s] (1/2) gate-probe a
            FAIL [   0.007s] (2/2) gate-probe b
         Summary [   0.007s] 2 tests run: 1 passed, 1 failed, 0 skipped
            FAIL [   0.007s] (2/2) gate-probe b
        PASS  a_passing_nextest_run_passes_the_summary_gate
     Summary [   0.550s] 7 tests run: 4 passed, 3 failed, 24 skipped
        FAIL  a_build_that_does_not_compile_fails_the_summary_gate
        FAIL  a_run_of_zero_tests_fails_the_summary_gate
        FAIL  a_failing_test_fails_the_summary_gate_even_with_exit_zero
```
reverted: cmp ok

## M2: drop `--color never` from the probe command
```
        PASS  the_documented_gate_invocation_uses_this_summary_pattern
        PASS  pre_pr_refuses_to_run_without_nextest
        PASS  a_build_that_does_not_compile_fails_the_summary_gate
        PASS  a_run_of_zero_tests_fails_the_summary_gate
        FAIL  forced_colour_does_not_break_the_summary_gate
            PASS [   0.007s] (1/2) gate-probe a
            PASS [   0.007s] (2/2) gate-probe b
         Summary [   0.007s] 2 tests run: 2 passed, 1 skipped
    GATE_MISSING_EXPECTED=^ +Summary \[ *[0-9.]+s\] [1-9][0-9]* tests? run: [1-9][0-9]* passed(, [0-9]+ skipped)?$
        PASS  a_failing_test_fails_the_summary_gate_even_with_exit_zero
        PASS  a_passing_nextest_run_passes_the_summary_gate
     Summary [   0.712s] 7 tests run: 6 passed, 1 failed, 24 skipped
        FAIL  forced_colour_does_not_break_the_summary_gate
```
reverted: cmp ok

## M3: remove pre-pr.sh's missing-nextest exit
```
        PASS  the_documented_gate_invocation_uses_this_summary_pattern
        PASS  pre_pr_refuses_to_run_without_nextest
        PASS  a_build_that_does_not_compile_fails_the_summary_gate
        PASS  forced_colour_does_not_break_the_summary_gate
        PASS  a_run_of_zero_tests_fails_the_summary_gate
        PASS  a_passing_nextest_run_passes_the_summary_gate
        PASS  a_failing_test_fails_the_summary_gate_even_with_exit_zero
     Summary [   0.539s] 7 tests run: 7 passed, 24 skipped
```
reverted: cmp ok

## Control: unmutated
```
        PASS  the_documented_gate_invocation_uses_this_summary_pattern
        PASS  pre_pr_refuses_to_run_without_nextest
        PASS  a_build_that_does_not_compile_fails_the_summary_gate
        PASS  a_passing_nextest_run_passes_the_summary_gate
        PASS  forced_colour_does_not_break_the_summary_gate
        PASS  a_run_of_zero_tests_fails_the_summary_gate
        PASS  a_failing_test_fails_the_summary_gate_even_with_exit_zero
     Summary [   0.551s] 7 tests run: 7 passed, 24 skipped
```

## M3, after tightening the test (it must also see that no `>>>` step started)
```
        FAIL  pre_pr_refuses_to_run_without_nextest
     Summary [   0.065s] 1 test run: 0 passed, 1 failed, 30 skipped
        FAIL  pre_pr_refuses_to_run_without_nextest
```
reverted: cmp ok

Control after revert:
```
        PASS  pre_pr_refuses_to_run_without_nextest
     Summary [   0.055s] 1 test run: 1 passed, 30 skipped
```

M3 first survived because the mutated script then failed at its first step for lack of `python3`, so both original assertions held. The test now also asserts that no step started.

## M4: the pre-CI pattern, without `( \([0-9]+ slow\))?`

```
            PASS [   0.005s] (1/2) gate-probe a
            PASS [   1.516s] (2/2) gate-probe b
        FAIL  a_slow_test_still_passes_the_summary_gate
        FAIL  the_documented_gate_invocation_uses_this_summary_pattern
```
reverted: cmp ok. The slow-test case fails, and so does the documented-pattern check (CLAUDE.md now carries the new pattern).
