# Quickstart: milestone 1272

**Spec**: [spec.md](./spec.md) | **Research**: [research.md](./research.md) | **Contract**: [contracts/reader-optional-axis.md](./contracts/reader-optional-axis.md)

## What this milestone changes

Four npm-ecosystem readers answer "not optional" for a dependency the
project declared optional. One reader — npm's — answers correctly. This
makes the other four agree with it.

## Re-run the measurements before trusting any of this

Lockfile formats are external behaviour and can move without notice.
That is the defect: pnpm moved the optional marker between v7 and v9.

```sh
cd specs/1272-npm-optional-scope/measurements
./optional-axis-probe.sh      # where each manager records the axis
./cancellation-probe.sh       # what the axis means (C1 vs C2)
```

Needs node, npm, pnpm, yarn, corepack and bun on `PATH`. Each writes
only inside its own scratch directory and prints where it left the tree.

**Reading the cancellation probe**: its two cases MUST return different
answers. If C1 and C2 agree, the harness is broken — suspect it before
concluding a manager changed.

## The four arms, cheapest first

| arm | change | needs a closure? |
|---|---|---|
| pnpm v9 (P1) | read `snapshots.<id>.optional` instead of the `packages:` entry | no — pnpm publishes it closed |
| bun (P2) | seed from `workspaces.<p>.optionalDependencies`, then close | yes |
| yarn Berry (P3) | add the lockfile's per-entry `dependenciesMeta` as a source alongside the manifest, then close | yes |
| yarn v1 (P4) | root from `package.json` only, then close over the existing per-entry sub-blocks | yes |

Each is independently shippable. pnpm first because it is one source
expression and delivers the transitive answer for free.

## Where the code is

```
waybill-cli/src/scan_fs/package_db/npm/
├── pnpm_lock.rs    build_snapshots_lookup:141   discards the flag
│                   is_optional:384              reads the wrong section
│                   classifier:464-490           already correct, never fires
├── bun_lock.rs     bun_workspace_dev_scopes:200 the dev-axis analogue
│                   parent loop:690-806          tracker excludes workspace shapes
│                   finalize:808-860             any_hard is depth-1, not transitive
├── yarn_lock.rs    comment:127-129              asserts the opposite of measurement
│                   berry optional_names:133     manifest-only source
│                   v1 optional_children:241     has the edges already
│                   v1 subtraction:372           one level, needs closure
└── peer_optional.rs  is_peer_optional           unchanged, must keep winning
```

## The trap that makes this not a two-line change

`v9_importer_scopes` (`pnpm_lock.rs:200`) merges `dependencies` **and**
`optionalDependencies` into one `protected` set. Harmless for the
development axis — both are "not dev". Fatal for the optional axis: the
cancelling set would contain the packages being classified, and FR-006
would subtract a set from itself.

Any arm reusing those seeds must split them, and FR-010 requires the
development result stay byte-identical while doing so.

## Verifying a change

```sh
# the mandatory gate — no +toolchain argument (#1189)
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

Assert on the positive signal. A check that did not run looks identical
to one that passed.

Two gates the local run does **not** cover:

- **Corpus goldens** are all `#[ignore]`d locally and the PR lane skips
  them, so a reader change surfaces as a red nightly. See
  `feedback_green_local_gate_excludes_corpus_goldens`. Never generate
  them locally.
- **Parity extractors**: adding a `waybill:optional-derivation` value
  without its extractor entry fails
  `parity::extractors::tests::every_catalog_row_has_an_extractor`.

## Proving a test is load-bearing

SC-006 requires that deleting any arm's logic fails a test. Check it the
way the project learned to: delete the classification line, confirm a
named test fails, restore. A test that passes without the code it covers
is not evidence — `feedback_a_passing_test_may_exercise_nothing`.

## Fixtures

Manager-produced manifests and lockfiles (FR-013), **synthetic package
names** (FR-015 — real coordinates trip Kusari Inspector's advisory
scan). The probes use real coordinates deliberately and are not
fixtures; their headers say so.

The Berry arm specifically needs a fixture whose manifest has **no**
`dependenciesMeta` key — the realistic shape, and the one the current
tests do not have.
