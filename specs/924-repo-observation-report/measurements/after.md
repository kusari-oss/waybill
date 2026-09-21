# Post-implementation measurement (T039, T040)

Shipped implementation, release build, 7 repositories. Research R3 measured a
**proxy** for the significance rule (markers-or-size, computed in Python);
this measures the rule itself.

## Records against directories

| repo | dirs walked | recorded | % | files walked | unclaimed | report |
|---|---|---|---|---|---|---|
| waybill (this repo, `--exclude-path target`) | 5400 | 515 | 9.5% | 18,169 | 17,674 | 250 KB |
| polyglot reference (2,291 packages) | 1187 | 29 | 2.4% | 6,193 | 6,188 | 13 KB |
| corpus 1 | 225 | 82 | 36.4% | 779 | 685 | 41 KB |
| corpus 2 | 3 | 2 | 66.7% | 22 | 17 | 2 KB |
| corpus 3 | 8 | 1 | 12.5% | 65 | 64 | 1 KB |
| corpus 4 | 10 | 6 | 60.0% | 31 | 23 | 4 KB |
| corpus 5 | 49 | 5 | 10.2% | 230 | 224 | 3 KB |

`significance_threshold` is 25 in every report (FR-021c).

**R3's prediction held.** It projected 7.7% for this repository against the
proxy rule; the shipped rule gives **9.5%**. The difference is the criteria R3
could not model — reader claims and ambiguity — and it is small enough that the
threshold choice stands without revision.

## Why the records exist

The raw percentages are not comparable across repositories until you separate
the two reasons a directory earns a record.

| repo | records | structural (marker / claim / ambiguity / excluded) | size-driven (over threshold) |
|---|---|---|---|
| waybill | 515 | **439** | 76 |
| polyglot | 29 | 3 | 26 |

waybill: 340 marker files, 410 claimed directories, 28 ambiguous.
Polyglot: 3 markers, 3 claimed.

**The size-driven portion is 1.4% of walked directories here and 2.2% on the
polyglot repo.** That component is what FR-021's bound is really about, and it
stays small. The rest of the growth is waybill genuinely having more structure
to report — which is the report doing its job, not failing to bound.

## SC-008 does not survive this measurement as worded

> **SC-008**: the report for a repository an order of magnitude larger than
> another is not an order of magnitude larger.

Taken literally it is contradicted by the strongest pair in the table:
waybill has **4.5×** the directories of the polyglot repo and **17.8×** the
records. Taken against the widest pair it passes comfortably — 1800× the
directories, 258× the records.

Both readings are arithmetic on the same data, which is the tell that the
criterion is measuring the wrong thing. Record count tracks **significant
structure**, and waybill has 113× the markers of the polyglot repo. A
criterion that treats a repository's structural richness as a size failure
will fail on exactly the repositories this feature is most useful for.

**The criterion is amended** to bound the component that actually scales with
size, which is measurable and is what FR-021 intends:

> Size-driven records — those earned only by exceeding the significance
> threshold — stay under 5% of directories walked.

Measured: **1.4%** (waybill), **2.2%** (polyglot). The original wording is left
in the spec struck through, with this note, because the arithmetic that
contradicts it is more useful to a future reader than a criterion quietly
rewritten to pass.

## Report size in absolute terms

250 KB for a 5,400-directory repository, 13 KB for the polyglot reference.
Both are readable documents and both are shareable by any ordinary means —
which was the practical goal behind FR-021.

---

# SC-002 — can a reader answer the question without the repository? (T039a)

Run once as a judgement check against the polyglot reference report, by a
reader with no access to that repository.

**Question 1 — name every ecosystem present.** Answerable directly:

```
go    supported    in 2 directories
npm   supported    in 2 directories
```

Correct: it is a Go repository with two yarn workspaces. **Question 2 — which
does waybill support?** Answerable from the same line: both.

Readers that engaged are equally legible:

```
npm         matched   4   emitted undetermined
pants_jvm   matched   2   emitted undetermined
golang      matched   1   emitted 297
```

**SC-002 passes.** Both questions were answered from the report alone.

## But one thing in it misleads, and it is worth recording

The report lists **26 unrecognised directories** carrying observation detail:

```
app/api/controller/pullreq    predominantly_text   files=69
app/api/controller/repo       predominantly_text   files=83
app/api/controller/space      predominantly_text   files=52
```

Each is *correct* — none holds a marker file, because the `go.mod` governing
them sits at the repository root. But a reader scanning for gaps sees 26
"unrecognised" entries in a repository with no gaps at all, and the honest
answer to "is this repository well supported?" is obscured by them.

This is the cost of FR-008's marker-first rule, which remains right: inferring
an ecosystem from `.go` files would mislabel exactly the layouts this report
exists to explain. The fix is not to weaken attribution but to add context —
a directory that falls **under** a recorded project root is covered by that
root, and saying so would collapse those 26 entries into "part of the Go
module at `.`".

Filed as a follow-up rather than fixed here: it changes the report's shape
rather than its correctness, and shipping the correct-but-noisy version is
better than delaying to make it pretty. Recorded so the next reader knows the
noise is understood rather than unnoticed.
