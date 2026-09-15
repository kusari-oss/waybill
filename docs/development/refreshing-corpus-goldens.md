# Refreshing the public-corpus goldens

`waybill-cli/tests/fixtures/public_corpus/` is what the
`Public corpus regression` lane compares every run against. This is the
procedure for replacing those files, and the sibling of
`docs/perf/refreshing-the-baseline.md` — same hazard class, same shelf.

Budget an afternoon. A lane run takes 2–5 minutes warm and up to ~15
cold, and this procedure needs at least four of them.

## Before you start

- `gh` authenticated, with permission to dispatch workflows on this repo.
- A Rust toolchain — you will build `xtask` locally.
- Your branch pushed. The workflow checks out the branch you *name*, not
  your working tree.

Where things live, because this procedure refers to all of them:

| thing | path |
|-------|------|
| the goldens | `waybill-cli/tests/fixtures/public_corpus/<target>/{cdx,spdx-2.3,spdx-3}.json` |
| the gating set (which targets run) | `waybill-cli/tests/corpus_harness_195/manifest.rs`, `TARGETS` |
| the gate's masking | `waybill-cli/tests/corpus_harness_195/layer2_golden.rs`, `mask_nondeterministic` |
| the review tool | `xtask/src/corpus_diff/mod.rs` |
| the lane | `.github/workflows/public-corpus.yml` |

## Rule zero: never generate goldens locally

Goldens capture environment-dependent content. Generate them where the
gate runs — `ubuntu-latest`, via CI dispatch — or you will produce
goldens that pass only on your machine.

Concretely, the thing never to do locally is set
**`WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1`**. That is the flag the
harness checks to write goldens instead of comparing them. Running the
corpus tests locally *without* it only compares, and is fine. Building
and running `xtask corpus-diff` locally is fine — steps below require it.

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

## What the gate compares — read this before trusting a clean diff

The gate masks known-volatile fields and then compares the result
**byte for byte** (`layer2_golden.rs`: `golden_bytes == masked_bytes`).
It does **not** tolerate array reordering.

The review tool in step 3 *does* sort arrays, deliberately, so that a
reordered `@graph` does not present as every element having changed.

So the two do not agree, and the direction matters: **a diff that the
review tool reports as clean can still fail the lane.** If the lane is
red and your normalised diffs are empty, you are looking at ordering
instability, and the fix belongs in `mask_nondeterministic` — where the
gate will honour it — not in the review tool, where only humans would
see it.

---

## Procedure

### 1. Confirm what is actually failing

Do not assume the failing set from a previous run; it moves.

```bash
gh workflow run "Public corpus regression" \
  -f branch=<your-branch> -f regen_goldens=false
```

Getting the run id is fiddly and it matters — see the warning under
step 2. Take it from the dispatch you just made:

```bash
gh run list --workflow "Public corpus regression" --limit 5 \
  --json databaseId,event,createdAt,conclusion
gh run watch <run-id>
gh run view <run-id> --log-failed
```

### 2. Regenerate through CI

```bash
gh workflow run "Public corpus regression" \
  -f branch=<your-branch> -f regen_goldens=true
```

The workflow sets the update flag, the harness overwrites the goldens in
the CI workspace, and the job uploads the fixture tree as the
`corpus-goldens-regen` artifact. Nothing is written to your repository
by the workflow — you install the goldens yourself, in step 8.

> **Dispatch one at a time.** The lane's concurrency group is keyed on
> `github.ref`, and every `workflow_dispatch` resolves to the same ref
> whatever `branch` you pass — so all your dispatches share one group.
> `cancel-in-progress: false` protects the *running* job, but GitHub
> keeps only one *pending* run per group: dispatching a second
> regeneration while the first is still queued cancels the first, and it
> shows up as a `cancelled` run you did not cancel. Wait for each run to
> finish before dispatching the next. This bites hardest at step 7,
> where the whole point is to get two artifacts from two runs.

> **Pick the run id deliberately.** This lane also runs nightly on cron,
> so `gh run list` interleaves cron runs with your dispatches, and
> "the most recent one" is frequently not yours. By the end of this
> procedure you will have dispatched it four or five times, several with
> the same artifact name. Filter by `event` and check `createdAt`:
>
> ```bash
> gh run list --workflow "Public corpus regression" --limit 10 \
>   --json databaseId,event,createdAt,conclusion \
>   --jq '.[] | select(.event=="workflow_dispatch")'
> ```
>
> Getting this wrong in step 7 is the expensive case: comparing an
> artifact against itself passes, and a check that always passes is
> exactly what step 9 exists to prevent.

Download into a **fresh, empty** directory. `gh run download` merges
into an existing one, so re-using a directory leaves files from the
previous artifact in place and any comparison silently ignores them.

```bash
rm -rf /tmp/regen && gh run download <run-id> -n corpus-goldens-regen -D /tmp/regen
```

The artifact unpacks with one directory per target at the top level
(`/tmp/regen/<target>/cdx.json`, …). It also carries any non-JSON files
that live in a target's golden directory — at time of writing,
`pants-example-javascript/README.md`.

### 3. Read every diff, normalised

The artifact is not in git, so compare it against the committed goldens
by path:

```bash
cargo build -p xtask
G=waybill-cli/tests/fixtures/public_corpus
for t in $(ls /tmp/regen); do
  for f in cdx spdx-2.3 spdx-3; do
    ./target/debug/xtask corpus-diff \
      --old "$G/$t/$f.json" --new "/tmp/regen/$t/$f.json" --format "$f"
  done
done | tee /tmp/corpus-diffs.txt
```

Capture the output — it is one diff per target per format, and it
scrolls.

A pair with no differences prints `<format>: no semantic change after
normalisation`. That is what a genuine clean comparison looks like;
anything else is drift to attribute.

The other invocation form — `--target <name> --old-ref <ref>` — compares
a target's *committed* goldens against a git ref. That is for looking at
history, not for reviewing a regenerated artifact, which by definition is
not committed yet. Reaching for it here produces a diff of the wrong
pair, or an empty one, and an empty diff is easy to misread as "no
drift".

The tool pairs elements across arrays that changed length, keyed on
identity rather than position (`purl`, then `name`+version, then a
non-content-addressed `bom-ref`, then `type`). Do not switch that key to
`spdxId`, `SPDXID` or a file-tier `bom-ref`: those are hashes of the
content whose change you are trying to read, so keying on them pairs
nothing and reports every element as both added and removed.

### 4. Attribute every category

For each repeated shape of change, name the merge that caused it.

```bash
# when the goldens were last written
git log -1 --format='%h %ad' --date=short -- waybill-cli/tests/fixtures/public_corpus
# what landed since
git log <that-sha>..HEAD --oneline
# which merge introduced a given annotation key or field
git log <that-sha>..HEAD -S'<the string>' --oneline -- waybill-cli/src/
```

A change that appears exactly once **across the whole review** — not
once per file — is not a category. It needs its own explanation.
Folding singletons into a category is how a regression enters a golden
unnoticed.

"Expected churn" is not a cause. If you cannot attribute something:
stop. Do not accept that target's golden until it is explained or raised
as a defect.

Some drift originates upstream rather than in this repo — corpus targets
are pinned external repositories and images, and a pin bump moves
content. That still needs naming ("pin moved from X to Y"), but the
cause is the pin, not a waybill merge.

### 5. Handle non-drift failures separately

If a target is failing for a reason other than accumulated drift, do
**not** regenerate its golden — that would encode the fault as expected
output. Either fix it here, or remove it from `TARGETS` in
`waybill-cli/tests/corpus_harness_195/manifest.rs` with a tracked issue,
and say so in the PR body and the commit message.

Reaching 100% by removing targets must not look like reaching 100% by
fixing them.

### 6. Freeze the fix set

Any emission-affecting fix invalidates the regenerated artifact and the
attribution built from it. If step 5 turned up several defects, decide
which are in scope, fix exactly those, push, and **return to step 2** to
regenerate once afterwards.

Defects you are not fixing should be raised as issues and named in the
PR. Milestone 840 fixed one (#854) and raised three (#855, #856, #857)
on that basis.

### 7. Confirm reproducibility — before you commit anything

Dispatch a second regeneration against the same tree, download it to a
*different* fresh directory, and confirm the two artifacts are identical:

```bash
rm -rf /tmp/regen2 && gh run download <second-run-id> -n corpus-goldens-regen -D /tmp/regen2
diff -r /tmp/regen /tmp/regen2 && echo "reproducible"
```

If they differ, something non-deterministic is unmasked. Fix it in
`mask_nondeterministic` — where the gate honours it — not in the review
tool, and then start again from step 2.

This comes before the commit deliberately. Run it afterwards and a
difference means you have already committed goldens containing
nondeterminism.

### 8. Install the goldens

Nothing has written to your repository until this point. Copy the
artifact over the committed tree:

```bash
G=waybill-cli/tests/fixtures/public_corpus
rsync -a --delete /tmp/regen/ "$G/"
git status --short "$G"
```

`--delete` matters: a plain `cp -r` leaves a stale golden behind for any
target you removed under step 5, and a file that did not change is
invisible in review.

Then confirm you changed nothing else:

```bash
git diff --name-only | grep -v "^$G/" || echo "nothing outside the goldens"
```

The harness itself must be untouched. If `corpus_harness_195/` appears
there, you have modified the gate rather than its expected values.

### 9. Commit all targets together

One change, not per-target increments. The lane goes red-to-green once,
and a reviewer can compare deltas across targets — which is how a target
whose delta pattern is unlike its peers becomes visible.

Put the attribution in the PR description. Do not commit it as a
document: it describes one moment and will read as current long after it
is not. Reference the PR from the commit message so it stays reachable
from `git log`.

Then dispatch the lane read-only once more and confirm it is green
before moving on — step 10 is meaningless against a lane that is already
red for unrelated reasons.

### 10. Prove the lane can still fail

Change something in emission deliberately. Confirm the lane fails, and
that the failure names the target **and all three formats**. Revert.

Skipping this is how a refresh becomes indistinguishable from a disable.
A green lane is not evidence of a working lane — this repo has shipped a
schema gate that passed because its `$ref`s resolved to stubs and it
validated nothing.

Pick a mutation that reaches every target and every format. Milestone
840 used the version constant in `waybill-cli/build.rs`, which lands in
CDX `metadata.tools`, SPDX 2.3 `annotations[].annotator` and SPDX 3
`createdBy` — three different structures, so a format whose comparison
silently no-ops cannot hide behind the other two.

**Compile the mutation locally before dispatching**
(`cargo build -p waybill --bin waybill`). The first attempt at this in
milestone 840 failed at build time and proved nothing, costing a full
lane run to discover.

Then check what the failure actually says, rather than that it went red.
Expect `3 of 3 formats drifted for <target>`, once per affected target —
with the version-constant mutation above, all of them. When this was
first run, a deliberate mutation failed every target while naming only
`cdx.json`, because the layer-2 loop panicked on the first failing format
and CDX is compared first. A red lane was not evidence the SPDX
comparisons ran either.

The mutation has to reach the remote to be dispatched, so your branch
will carry a deliberate-breakage commit and its revert. That is expected;
leave both in the history and name the run ids in the PR.
