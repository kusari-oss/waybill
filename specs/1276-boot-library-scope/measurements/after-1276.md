# After: US1 + US2, measured against a verified baseline

**Tasks**: T026, T027, T033 | **Date**: 2026-11-03
**Baseline**: `6dd75678` (the commit before US1), built in a detached worktree
**Control**: the baseline's `scope_propagation.rs` contains **0** occurrences
of `compiler_supplied`. A baseline that cannot be shown to lack the feature is
not a baseline — milestone 1275's first attempt at this comparison used
`git stash`, which reverts to HEAD, and HEAD already held the feature.

## `slack-web`, the subject project, through both binaries

Same command line, same pinned revision, same cached nixpkgs:

```
  [baseline]  optional in B but not A (9): [clock, deque, extra, microlens,
                microlens-th, multistate, pretty, strict-list, unsafe]
  [after]     optional in B but not A (8): [clock, deque, extra, microlens,
                microlens-th, multistate, strict-list, unsafe]
```

| criterion | required | measured |
|---|---|---|
| **SC-001** | `pretty` carries no non-runtime scope | **met** — it leaves the set |
| **SC-003** | the 8 non-boot components keep theirs | **met** — all 8 remain |

The two lines differ by exactly one element, and it is the boot library. That
is the whole feature on its subject project: one component corrected, eight
untouched.

## T026 — the CLI surface

`waybill sbom scan --help` is byte-identical to the baseline's, 765 lines.
(C1, FR-013)

## T027 — the corpus-scale control cannot be verified locally, and that is a task defect

T027 asks for the 569-of-579 survivor figure by running
`scope-always-present.py` and diffing against T001's baseline. **That probe
reads the committed goldens**, which this feature does not regenerate — so it
will report 579 and 10 before and after, unchanged, no matter whether the code
is correct.

This is the same shape as findings F3: the corpus layer reads fixtures, and
fixtures can only be refreshed through `public-corpus.yml`. What differs is
that F3's case *failed* visibly (I9 flagged the stale cells) while this one
would have *passed* vacuously — a control that cannot move is not a control,
and reporting "569 survived" from an unchanged golden would have been a false
claim.

**What is actually verified of FR-003 / SC-004, and how:**

| level | check | status |
|---|---|---|
| predicate | `m1276_a_non_boot_component_still_receives_the_scope` | **passes**, and mutant M2 (ecosystem check dropped) is caught by it among four |
| predicate | `m1276_a_same_named_package_in_another_ecosystem_is_not` | **passes** — the npm/Go/cargo name collisions |
| real project | the 8 non-boot controls on `slack-web` | **measured above** |
| corpus, 569 of 579 | — | **deferred to the golden refresh** |

The corpus figure is a prediction for the refresh diff, not a result: expect
**2 targets, scope only, 10 components**, and the two I9 declarations removed.
Anything else is a finding.
