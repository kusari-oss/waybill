# T043 — `--exclude-scope` below the propagation pass

**Date**: 2026-10-26
**Host**: darwin 25.5.0, `target/debug/waybill` built from this branch
**Scripts**: `probe-exclude-scope.sh`, `exclude-scope-survivors.py`
(both committed beside this file)
**Reproduce**:

```sh
cargo build --bin waybill
./specs/1269-transitive-dev-scope/measurements/probe-exclude-scope.sh \
    go-opentelemetry test after
python3 ./specs/1269-transitive-dev-scope/measurements/exclude-scope-survivors.py \
    ./target/measurements/exclude-scope/after test   # exits 0 iff 0 survivors
```

## What was changed

The `--exclude-scope` filter was an inline block in `scan_cmd.rs`
immediately after the m191 second-pass reconciler, about 670 lines
**above** the #1209 propagation pass. It is now an extracted function,
`apply_exclude_scope_filter`, called immediately **below** the file-tier
walker and immediately above the m167 orphan-reason classifier. The
call site's own comment enumerates the four passes it must follow and
why each one is a position rather than a preference.

## Why a count of drops cannot answer this

The obvious check — "does anything scoped `test` survive
`--exclude-scope test`?" — passes in both worlds, and this is the trap
the measurement is built around.

With the filter above the pass, excluding a scope drops the
**declared** members first. Those members are exactly the seeds Phase B
propagates from, so the pass then finds nothing to walk and their
exclusive transitives come out carrying **no scope at all**. A missing
`waybill:lifecycle-scope` property is byte-identical on the wire to a
runtime dependency. So the old order did not merely fail to drop those
components: it destroyed the evidence that they should have been
dropped, and then produced a document in which nothing is scoped `test`
— which is what a correct run also looks like.

The separating signal is the **per-derivation survivor count**: how
many components that the *baseline* (no-flag) run scoped `test`, broken
down by what assigned the scope, are still present in the excluded run.
`exclude-scope-survivors.py` reports that and exits non-zero if any
survive.

The same distinction is available at runtime. The extracted function
logs `dropped_inferred` — drops carrying C62
`waybill:lifecycle-scope-derivation = transitive` — alongside the
total. Move the call back above the pass and that field reads 0 on every
input, because the marker does not exist yet:

```
before : (no such field)
after  : applied --exclude-scope filter dropped=50 dropped_inferred=33 exclude_scope=[Test]
```

## `go-opentelemetry`, `--exclude-scope test`

| | before | after |
|---|---|---|
| baseline components / edges | 334 / 1558 | 334 / 1558 |
| excluded components / edges | 317 / 1465 | **284 / 761** |
| baseline scoped `test` | 50 | 50 |
| — reader-declared, survived | 0 / 7 | 0 / 7 |
| — C62 `test-only-closure`, survived | 0 / 8 | 0 / 8 |
| — C62 `go-mod-why`, survived | 0 / 2 | 0 / 2 |
| — **C62 `transitive`, survived** | **33 / 33** | **0 / 33** |
| analyser exit status | 1 | 0 |

All 33 are ordinary modules — `golang.org/x/crypto`,
`google.golang.org/grpc`, `github.com/prometheus/client_golang`,
`gopkg.in/yaml.v3` and so on. An operator asking for "no test
dependencies" received every one of them, presented as a runtime
dependency, and 704 edges that exist only to reach them.

## `haskell-language-server`, `--exclude-scope dev`

| | before | after |
|---|---|---|
| baseline components / edges | 399 / 2404 | 399 / 2404 |
| excluded components / edges | 364 / 2164 | **362 / 2145** |
| unscoped components in the excluded run | 356 | **354** |
| baseline scoped `development` | 37 | 37 |
| — reader-declared, survived | 0 / 11 | 0 / 11 |
| — C62 `transitive`, survived | 0 / 26 | 0 / 26 |
| analyser exit status | 0 | 0 |

**This target refutes the reasoning that motivated the change, while
confirming the change.** The gap had been asserted twice from code
position — "an operator excluding `development` still receives the
transitive closure of a declared dev member" — and on the target that
claim would most naturally have been checked against, it is false: the
old order already dropped all 37. Not because the filter was placed
correctly, but because removing the 11 declared dev members starves the
nixpkgs-haskell runtime-closure resolver that runs *below* the old call
site, so the 26 transitives were never produced in the first place.
Two filters' worth of correct output from an incorrect order, by
accident.

What the old order did leak here is smaller and was invisible to the
headline count: **2 components**, `pkg:hackage/QuickCheck@2.15.0.1` and
`pkg:hackage/network-uri@2.6.4.2`, emitted unscoped under
`--exclude-scope dev`. The unscoped total tells the story — 356 before
against a baseline of 354, i.e. the excluded run contained *more*
unscoped components than the document it was filtering. After the move
it is exactly 354 and the arithmetic closes: 399 − 37 = 362.

Same flag, same binary, opposite failure mode per ecosystem. Neither
was predictable from reading the call order; both took one afternoon of
probing, which is the standing rule in `CLAUDE.md § Measure external
behaviour before designing around it` applied to our own pipeline.

## No-flag output is unchanged

Every scan that does not pass `--exclude-scope` must be byte-identical,
since the filter early-returns on an empty list. Verified directly on
both targets rather than inferred from the early return: the baseline
documents from the before and after runs are identical once
`serialNumber` and `timestamp` are masked (the two fields the corpus
harness also masks), and their component PURL multisets are equal.

```
go-opentelemetry        masked baseline documents equal: True
haskell-language-server masked baseline documents equal: True
                        baseline purl multiset identical: True
```

The corpus lane is the stronger statement of the same thing: 109
assertions across 17 targets against the committed goldens, none of
which pass `--exclude-scope`.

## What is NOT covered, and why

There is **no hermetic end-to-end regression test** for this ordering,
and the reason is a measured property of the flag rather than an
omission. Reproducing the defect in a synthetic fixture needs an
ecosystem where the pass assigns a scope that `--exclude-scope` can
name. Five candidates were built and scanned:

| fixture | parent scope | transitive | why it cannot carry the test |
|---|---|---|---|
| uv (`[dependency-groups]`) | `optional` | `optional`, C62 `transitive` | `ExcludeScopeArg` has no `optional` variant — see below |
| yarn v1 | none | none | reader does not apply `package.json` `devDependencies` to lock entries |
| pnpm v9 | none | none | same, at the importer level |
| gem, composer | `development` | `development`, **no** marker | reader already scopes the transitive; the pass has nothing to do |
| go (`_test.go` + fixture-local `GOMODCACHE`) | `test` | `test`, C62 `test-only-closure` | the Go reader's own closure tags it at this size; the pass only fires on the multi-module shape `go-opentelemetry` has |

The uv fixture is the one that has the right shape and the wrong
vocabulary. `LifecycleScope::Optional` has existed since m179 and
`--exclude-scope` still accepts only `dev`, `build` and `test`, so the
one scope this feature demonstrably infers on a local fixture cannot be
excluded at all. That is a gap in the flag, filed separately rather
than widened here.

Until it is closed, the ordering is held by three things and not by an
integration test: the five unit tests on `apply_exclude_scope_filter`
(mutation-checked, below), the `dropped_inferred` log field, and this
measurement with its two committed scripts.

## The file-tier constraint, and the limit of what was shown

Constraint 2 at the call site — the filter must run *below* the
file-tier walker, because `DedupeIndex::build(&components)` reads the
component set, so filtering first would shrink the index and let files
covered by a dropped package reappear as file-tier entries.

What was measured: on the current tree, file-tier output is identical
with and without the flag. On `go-opentelemetry` the tier histogram is
`{source: 333, file: 1}` with no flag and `{source: 283, file: 1}` with
`--exclude-scope test`. On a synthetic Go fixture run in
`--file-inventory=source-tree` mode, the file-tier rows are row-for-row
identical:

```
no flag              {'source': 5, 'file': 2}
--exclude-scope test {'source': 3, 'file': 2}
file-tier rows IDENTICAL: True
```

**What that does NOT show.** Neither case puts a file in the scan tree
that is *covered by* a package the filter drops, which is the only
input that could distinguish the two orderings — and for Go it cannot
exist, because a module's sources live in `$GOMODCACHE` rather than in
the scanned tree. So the measurement confirms no regression on the
inputs available and is not a positive control for the constraint
itself. The constraint rests on reading
`waybill-cli/src/cli/scan_cmd.rs`'s `DedupeIndex::build(&components)`
call, with the measurement as corroboration. Said plainly because the
alternative — a table of equal numbers presented as proof — is the
shape of a check that inspected nothing.

## Invariant I7 holds under the flag, checked rather than argued

Constraint 3 at the call site — the filter must run *above* the m167
orphan-reason classifier, because dropping a component can orphan its
neighbours and #1218 requires every orphan to carry a reason — was
verified rather than reasoned. `i7.py` (committed beside this file)
replicates the corpus harness's I7 check, including all five of
`orphan_explained::is_explained`'s accepted markers, and was run over
every probe document:

| document | components | unreachable | unexplained |
|---|---|---|---|
| `go-opentelemetry` baseline, before | 334 | 3 | 0 |
| `go-opentelemetry` excluded, before | 317 | 3 | 0 |
| `go-opentelemetry` baseline, after | 334 | 3 | 0 |
| `go-opentelemetry` excluded, after | 284 | 3 | 0 |
| `haskell-language-server` baseline, after | 399 | 35 | 0 |
| `haskell-language-server` excluded, after | 362 | 35 | 0 |

A column of zeros is the shape a checker that inspects nothing also
produces, so `i7.py` was given a positive control: the same
`go-opentelemetry` excluded document with all five explainer properties
stripped reports **3 unexplained** and names them. The zeros above are
therefore the checker working, not the checker idling.

## A second pass had to move with it, and only measurement found it

Moving the filter down falsified a claim in a comment 300 lines above
it. The m776 source-provenance summary log says it is emitted "after
every pass that can add or drop components", because m776's SC-009a
forbids that count from overcounting the emitted document — and it
derives the count by walking the final component set rather than
tallying during enrichment, precisely so the two cannot drift. Putting
a component-dropping filter below it broke that.

Three runs of the same probe, one binary apart each time:

| tree | `--exclude-scope test` log says | document holds | |
|---|---|---|---|
| filter above the pass (before) | 244 / 236 | 244 / 236 | match |
| filter moved, summary not moved | 244 | **224** | **mismatch** |
| summary moved below the filter | 224 | 224 | match |

(The first row is baseline / excluded; the other two are the excluded
run.) The summary block now sits immediately below
`apply_exclude_scope_filter`, which makes its claim true of more passes
than when it was written — the nixpkgs-haskell and m1035 nix-closure
producers are above this point too, and were not when the comment was
authored.

It is worth being precise about what caught this. Not a test: no test
asserts that log line, and SC-009a was verified by hand at m776 time.
Not review: the comment that stated the invariant is 40 lines long and
says so in its first sentence, and the relocation was reviewed against
component and edge counts. It was caught by reading the INFO log of a
probe run that existed for a different question, noticing 244 where the
document had 224, and checking. The generalisation is the one the
project already has in `CLAUDE.md`: a check that did not run looks
identical to a check that passed, and so does an invariant nothing
asserts.

## Mutation check on the unit tests

Each mutation was applied to the extracted function alone, the five
tests run, and the file restored.

| mutation | expected to fail | observed |
|---|---|---|
| `inferred += 1` → `+= 0` | `..._counts_the_scopes_the_propagation_pass_inferred` | that test, alone |
| edge retain drops the `r.from` check | `..._drops_edges_on_either_endpoint` | that test, alone |
| derivation match tests key presence, not the value `transitive` | `..._does_not_count_the_go_readers_own_derivations` | that test, alone |

The third mutation is the one worth keeping: C62's enum is open and
shared, and the Go readers write `test-only-closure` / `go-mod-why`
into the same key. A `contains_key` check would make `dropped_inferred`
non-zero on any Go scan regardless of where the call site sits —
turning the ordering signal into a proxy that reads positive when
nothing was verified.
