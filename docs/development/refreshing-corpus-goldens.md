# Refreshing the public-corpus goldens

`waybill-cli/tests/fixtures/public_corpus/` is what the
`Public corpus regression` lane compares every run against. This is the
procedure for replacing those files, and the sibling of
`docs/perf/refreshing-the-baseline.md` — same hazard class, same shelf.

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

Push your branch first — the workflow checks out the branch you name,
not your working tree.

```bash
gh workflow run "Public corpus regression" \
  -f branch=<your-branch> -f regen_goldens=true
```

The workflow sets the update flag, the harness overwrites the goldens in
the CI workspace, and the job uploads the fixture tree as the
`corpus-goldens-regen` artifact. Nothing is written to your repository.

```bash
gh run download <run-id> -n corpus-goldens-regen -D /tmp/regen
```

Do not run the regeneration locally, even to "check something quickly" —
a local run that overwrites goldens is indistinguishable afterwards from
a CI-generated one.

### 3. Read every diff, normalised

The artifact is not in git, so compare it against the committed goldens
by path. All thirty-three at once:

```bash
cargo build -p xtask
G=waybill-cli/tests/fixtures/public_corpus
for t in $(ls /tmp/regen); do
  for f in cdx spdx-2.3 spdx-3; do
    ./target/debug/xtask corpus-diff \
      --old "$G/$t/$f.json" --new "/tmp/regen/$t/$f.json" --format "$f"
  done
done
```

The other invocation form — `--target <name> --old-ref <ref>` — compares
a target's *committed* goldens against a git ref. That is for looking at
history, not for reviewing a regenerated artifact, which by definition
is not committed yet. Reaching for it here produces a diff of the wrong
pair, or an empty one, and an empty diff is easy to misread as "no
drift".

Goldens are stored already masked, so timestamps, per-scan document
identifiers and embedded content hashes are already neutralised. What
the normaliser adds is array-ordering stability — without it, SPDX 3
`@graph` reordering presents as every element changing and buries
whatever really changed.

It also pairs elements across arrays that changed length, keyed on
identity rather than position. Do not switch that key to `spdxId`,
`SPDXID` or a file-tier `bom-ref`: those are hashes of the content whose
change you are trying to read, so keying on them pairs nothing and
reports every element as both added and removed.

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

### 5b. Freeze the fix set before the final regeneration

Any emission-affecting fix invalidates the regenerated artifact and the
attribution built from it. If step 5 turns up several defects, decide
which are in scope for this refresh, fix exactly those, and regenerate
once afterwards.

Defects you are not fixing should be raised as issues and named in the
PR, not quietly regenerated over. Milestone 840 fixed one (#854) and
raised three (#855, #856, #857) on that basis.

### 6. Commit all targets together

One change, not per-target increments. The lane goes red-to-green once,
and a reviewer can compare deltas across targets — which is how a target
whose delta pattern is unlike its peers becomes visible.

Put the attribution in the PR description. Do not commit it as a
document: it describes one moment and will read as current long after it
is not. Reference the PR from the commit message so it stays reachable
from `git log`.

### 7. Prove the lane can still fail

Change something in emission deliberately. Confirm the lane fails, and
that the failure names the target **and all three formats**. Revert.

Skipping this is how a refresh becomes indistinguishable from a disable.
A green lane is not evidence of a working lane — this repo has shipped a
schema gate that passed because its `$ref`s resolved to stubs and it
validated nothing.

Check all three formats, not just that the lane went red. When this was
first run during milestone 840, a deliberate mutation failed all eleven
targets while naming only `cdx.json` in every failure: the layer-2 loop
panicked on the first failing format, and CDX is compared first, so the
SPDX 2.3 and SPDX 3 comparisons never executed. A red lane was not
evidence that they worked either. The loop now compares every format and
reports the failures together — you should see
`3 of 3 formats drifted for <target>` — but verify it rather than
assume it.

Compile the mutation locally before dispatching. The first attempt at
this failed at build time and proved nothing, which costs a full lane
run to discover.

### 8. Confirm reproducibility

Regenerate a second time against the same tree. The goldens must be
identical. If they are not, something non-deterministic is unmasked, and
the fix belongs in the harness's masking — where the gate will honour it
— not in the review tool, where only humans would.
