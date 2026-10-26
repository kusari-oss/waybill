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

## T036's first gate run found two things, and recorded them rather than fixing them silently

`GATE_EXIT=101`, `GATE_RESULT=fail`,
`GATE_MISSING_EXPECTED=>>> all pre-PR checks passed\.`; 346 targets,
6608 passed, 2 failed. Clippy was clean.

### 1. The derivation key was already taken (fixed)

```
---- parity::extractors::tests::extractors_have_unique_labels stdout ----
duplicate label(s) in EXTRACTORS: [("waybill:lifecycle-scope-derivation", ["C62", "C199"])]
```

`waybill:lifecycle-scope-derivation` has been catalog row **C62** since
the Go test-closure work, with an open enum (`test-only-closure`,
`go-mod-why`) of exactly this semantic. Research R6 asked which row
*number* was free — C198 was the highest — and never asked whether the
*key* was taken. Corrected to a third value on C62's enum; no new row,
no new extractor. Full account in
`contracts/scope-derivation-annotation.md`.

Worth noting **which** check caught it. Both rows' extractors passed
their own tests, both catalog entries were well-formed, and
`every_catalog_row_has_an_extractor` passed — each row was consistent
with itself. Only `extractors_have_unique_labels`, which looks *across*
rows, could see it. The targeted suites run during implementation
(`parity::`, `holistic_parity`, the module's own tests) all passed with
the collision in place.

### 2. The declaration deletions and the goldens must land together

```
---- corpus_harness_195::invariants::tests::golden_matrix_is_evaluable_and_gated stdout ----
4 undeclared or stale cell(s) in the golden matrix:
  I3 / haskell-language-server: UndeclaredFailure — unmarked-non-runtime-reachable x26
  I3 / haskell-security-advisories: UndeclaredFailure — unmarked-non-runtime-reachable x13
  I3 / nix-closure-moat: UndeclaredFailure — unmarked-non-runtime-reachable x9
  I3 / python-flask: UndeclaredFailure — unmarked-non-runtime-reachable x51
```

**Not a regression — a sequencing error in the task list.** Milestone
1155 evaluates the invariant matrix in two places, and the task list
only accounted for one of them. The hermetic half runs on every
`cargo test` against the **committed goldens**, which are still the
pre-change documents; T035 deleted the declarations that excused their
I3 failures. The counts are identical to the deleted declarations
(26 / 13 / 9 / 51), which is what identifies this as stale evidence
rather than a new defect.

Note what the hermetic half *does* confirm: `python-flask`'s 51 is
real and present in the committed golden, and `rust-ripgrep`'s
retained declaration still reads `d` in the matrix — so the re-point
is consistent.

The order T034–T038 implies is wrong and the correct order is:

1. push the branch,
2. dispatch `public-corpus.yml` with `regen_goldens=true` (T037),
3. commit the regenerated goldens from the run's artifact,
4. **then** the four deletions are consistent and the local gate is green,
5. re-run the gate (T036) and read the lane (T038).

The memory note `feedback_green_local_gate_excludes_corpus_goldens`
covers the *golden comparison* being `#[ignore]`d locally. The
invariant matrix is not ignored, which is the half that bit here — a
declaration file edit is gated locally even though the goldens it
describes are not.

Goldens were **not** generated locally, and
`WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS` was never set here.
