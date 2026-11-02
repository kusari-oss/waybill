# Contract: the optional axis each npm-ecosystem reader must produce

**Milestone**: 1272 | **Spec**: [spec.md](../spec.md)

The interface this feature exposes is not a CLI surface or an API — it
is the classification contract four readers must satisfy, and which the
emitted SBOM is the observable form of. Stated as behaviour a test can
assert, per dialect, because the whole defect being fixed is four
readers answering differently about the same graph.

## C1 — A root optional declaration classifies that component optional

**Applies to**: pnpm v9, bun, yarn Berry, yarn v1 (all four arms).

Given a project whose manifest declares exactly one dependency under
`optionalDependencies` and one under `dependencies`:

- the optional one carries `LifecycleScope::Optional`
- the runtime one does not
- the optional one carries `waybill:optional-derivation`
- exactly **one** component is optional

Today all four answer zero (SC-001).

## C2 — Optionality propagates along optional edges

**Applies to**: all four arms (FR-007a).

Given a package reachable from an optional declaration only through
optional edges, that package is optional even though nothing declares
it so.

This is the contract Probe C's control case measured on npm and pnpm:
`ms`, declared optional nowhere, is optional because its only path runs
through an optional `debug`.

## C3 — Any non-optional path cancels

**Applies to**: all four arms (FR-006).

Given a package reachable by both an optional and a non-optional path,
that package is **not** optional — including when the root itself
declared it optional.

Probe C's contested case: npm writes `optional: false` and pnpm writes
no key for exactly this shape. A reader that marks it optional
contradicts both reference implementations.

## C4 — Peer-optional precedence is unchanged

**Applies to**: all four arms (FR-009).

A name satisfying the existing `is_peer_optional` predicate keeps its
peer classification. m178's `PROVIDED_DEPENDENCY_OF` continues to win
over `OPTIONAL_DEPENDENCY_OF`.

## C5 — The development axis is byte-identical

**Applies to**: all five dialects, including the two this feature does
not otherwise touch (FR-010, SC-004).

Being optional must not make a declaration development, and must
continue to disqualify it from being development. Every existing
fixture's development-axis output is unchanged.

This is the contract most at risk: three of the four arms need the
runtime seed *split* from the optional seed, and the existing
development-axis helper deliberately merges them (research R7).

## C6 — pnpm v6/v7 are frozen

**Applies to**: pnpm only (FR-002).

A v6 or v7 lockfile classifies exactly as before. The flag genuinely
lives in `packages:` for those dialects; only v9 moved it.

## C7 — An unrecognized format version warns and does not fail

**Applies to**: all four arms (FR-011a, SC-008).

Given a lockfile declaring a format version higher than any in research
R6's table:

- every location the reader knows is still attempted
- a diagnostic names the unrecognized version and says the optional
  axis may be incomplete
- the scan does not fail
- an absent axis is not reported as a confirmed absence

## C8 — A declaration naming an absent package is dropped with a diagnostic

**Applies to**: all four arms (FR-012).

No component is fabricated for a dependency declared but never
resolved.

## C9 — A malformed axis region leaves the axis unset

**Applies to**: all four arms (FR-011).

Parsing continues. The axis is unset, not guessed.

## Cross-dialect contract

**C10 — all five dialects agree** (SC-002, SC-003).

The same dependency graph expressed in npm, pnpm, yarn v1, yarn Berry
and bun produces the same optional classification. Today one of five is
right; the target is five of five. This is the contract the feature
exists for, and the only one no single-reader test can assert — it
needs the five-dialect comparison.

## Evidence standard for every contract above

**C11 — fixtures are manager-produced** (FR-013, SC-005).

Each arm is proven by at least one fixture whose manifest and lockfile
came from running the real manager, and the Berry arm by a manifest
containing no `dependenciesMeta` key. A fixture asserting a hand-written
shape is never the only evidence for an arm — that is how the Berry arm
came to have passing tests for a shape real projects do not produce.

**C12 — every arm's logic is load-bearing** (SC-006).

Deleting any one reader's classification logic fails at least one test.
A test that would pass without the code it covers is not evidence; see
`feedback_a_passing_test_may_exercise_nothing`.
