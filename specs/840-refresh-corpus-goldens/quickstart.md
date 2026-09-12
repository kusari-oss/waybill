# Quickstart: refreshing the public-corpus goldens

Feature `840-refresh-corpus-goldens` · drafted here, published to
`docs/development/refreshing-corpus-goldens.md` per FR-014.

Read this before regenerating. The two most recent nightly-lane
outages in this repo were both caused by generating an expected-value
artifact in the wrong place, not by any defect in the code under test.

---

## Rule zero: never generate goldens locally

Goldens capture environment-dependent content. Generate them where the
gate runs — `ubuntu-latest`, via CI dispatch — or you will produce
goldens that pass only on your machine.

This is not hypothetical:

- **#818** — a perf baseline recorded on an arm64 macOS laptop and
  compared against Linux CI. Nine days of failures across five release
  tags before anyone read it as a machine mismatch rather than a code
  regression.
- **#832** — a corpus fixture whose content depended on whether the
  generating host had `git-lfs` installed. Three nights of failures, and
  two wrong diagnoses (first "macOS vs Linux", then "arm64 vs x86_64")
  before the real cause surfaced.

Both were the same mistake in different clothes.

---

## Procedure

### 1. Confirm what is actually failing

Dispatch the lane read-only against your branch. Do not assume the
failing set from a previous run; it moves.

### 2. Regenerate through CI

Dispatch `public-corpus.yml` against your branch with
`regen_goldens: true`. The workflow sets the update flag, the harness
overwrites the goldens, and the job uploads the fixture tree as the
`corpus-goldens-regen` artifact.

Download that artifact. Do not run the regeneration locally, even to
"check something quickly" — a local run that overwrites goldens is
indistinguishable afterwards from a CI-generated one.

### 3. Read every diff, normalised

```
cargo run -p xtask -- corpus-diff --target <name> --old-ref HEAD
```

Goldens are stored already masked, so timestamps, per-scan document
identifiers and embedded content hashes are already neutralised. What
the normaliser adds is array-ordering stability — without it, SPDX 3
`@graph` reordering presents as every element changing and buries
whatever really changed.

### 4. Attribute every category

For each repeated shape of change, name the merge that caused it. Work
against the log since the goldens were last written.

A change that appears exactly once is **not** a category. It needs its
own explanation. Folding singletons into a category is how a regression
enters a golden unnoticed.

If you cannot attribute something: stop. Do not accept that target's
golden until it is explained or raised as a defect.

### 5. Handle non-drift failures separately

If a target is failing for a reason other than accumulated drift, do
**not** regenerate its golden — that would encode the fault as expected
output. Either fix it here, or remove it from the lane's gating set with
a tracked issue, and make the removal visible in lane output.

Reaching 100% by removing targets must not look like reaching 100% by
fixing them.

### 6. Commit all targets together

One change, not per-target increments. The lane goes red-to-green once,
and a reviewer can compare deltas across targets — which is how a target
whose delta pattern is unlike its peers becomes visible.

Put the attribution in the PR description. Do not commit it as a
document: it describes one moment and will read as current long after it
is not. Reference the PR from the commit message so it stays reachable
from `git log`.

### 7. Prove the lane can still fail

Change something in emission deliberately. Confirm the lane fails and
names the target and format. Revert.

Skipping this is how a refresh becomes indistinguishable from a disable.
A green lane is not evidence of a working lane — this repo has shipped a
schema gate that passed because its `$ref`s resolved to stubs and it
validated nothing.

### 8. Confirm reproducibility

Regenerate a second time against the same tree. The goldens must be
identical. If they are not, something non-deterministic is unmasked, and
the fix belongs in the harness's masking — where the gate will honour it
— not in the review tool, where only humans would.
