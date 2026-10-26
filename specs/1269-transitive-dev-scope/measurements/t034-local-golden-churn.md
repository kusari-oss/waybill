# T034 — local golden churn: none

**Date**: 2026-10-25 | **Feature**: `1269-transitive-dev-scope` (#1209)

## Result

**Zero local goldens moved.** Every one of the six golden-writing suites
passes unchanged against its committed goldens, and regenerating them
with all three update env vars set produces byte-identical files.

```
$ cargo test -p waybill --no-fail-fast \
    --test cdx_regression --test spdx_regression --test spdx3_regression \
    --test optional_dep_classification --test oci_pull_backward_compat \
    --test pkg_alias_binding_us1
test result: ok. 11 passed; 0 failed      (cdx_regression)
test result: ok.  2 passed; 0 failed      (spdx_regression)
test result: ok.  3 passed; 0 failed      (spdx3_regression)
test result: ok.  2 passed; 0 failed      (optional_dep_classification)
test result: ok. 11 passed; 0 failed      (oci_pull_backward_compat)
test result: ok. 11 passed; 0 failed      (pkg_alias_binding_us1)

$ WAYBILL_UPDATE_CDX_GOLDENS=1 WAYBILL_UPDATE_SPDX_GOLDENS=1 \
  WAYBILL_UPDATE_SPDX3_GOLDENS=1 cargo test -p waybill --no-fail-fast \
    --test cdx_regression ... (same six)
$ git status --short
(empty)
```

All six suites are listed because
`feedback_release_bump_regen_all_golden_tests` records that three of
them write goldens without `_regression` in the name, and a regen that
covers only the obvious three leaves the other three stale.

## Why both runs

The green comparison already proves byte-identity — that is what the
suites assert. The regeneration run was still done, because the two
failures look different and only one of them is a product finding: a
suite that compares and passes says the emission did not change; a
regeneration that writes different bytes while the comparison passes
would say the *golden writer* and the *comparator* disagree. The second
run rules that out. `git status --short` empty after a regen with all
three vars set is the positive signal.

`WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS` was **not** set, here or
anywhere. Corpus goldens are produced by the lane (T037), never
locally.

## Why there was nothing to move

None of the local fixtures has the shape the pass acts on: a component
with no scope, reachable along a declared edge from a component a reader
scoped non-runtime, in an ecosystem with a resolvable root. The npm
fixtures come pre-marked by their reader (the `npm-express` story at
corpus scale); the cargo fixtures hit the `rust-ripgrep` case; the rest
have no non-runtime member at all.

This is a fact about the local fixture set, not evidence that the pass
is inert — the five fixtures added under T031/T032 show it firing, and
`python-flask` shows it at corpus scale. It does mean **SC-007's
acceptance rests entirely on the corpus lane** (T038), since locally
there is no churn to explain.
