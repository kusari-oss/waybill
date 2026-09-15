# Measurements

Every figure in `../spec.md` comes from here. Re-run these when the
behaviour changes — and note that this already happened once: see
"A warning about stale evidence" below.

Both probes deliberately share no code with waybill. A self-report
cannot be validated by the thing doing the reporting, and a
normalisation bug shared between instrument and subject would cancel out
in exactly the comparison that matters.

## Environment

Measured against `main` at `09805cad` with a binary built from that
commit (T001; `waybill 0.7.0`, built 2026-09-13).

### Recorded baselines (T002-T004)

| target | cache | golang edges | backed | stdlib | unbacked | probe |
|---|---|---:|---:|---:|---:|---|
| `go-cobra` | cold | 7 | 4 | 1 | **2** | exit 1 |
| `go-cobra` | warm | 5 | 4 | 1 | **0** | exit 0 |
| `go-kubernetes` | cold | 2984 | 2392 | 38 | **554** | exit 1 |

`go-kubernetes` cold also reports `components=824`,
`reachable_count=494 total_count=494 orphan_count=0`,
`graph-completeness=complete`, `go-transitive-coverage=unknown`.

### Environment isolation

All three env vars are required when simulating a clean runner —
module-cache discovery falls back `$GOMODCACHE` → `$GOPATH` →
`$HOME/go/pkg/mod`, so a warm cache in any one of them masks the defect:

```bash
E=$(mktemp -d)
env GOMODCACHE=$E GOPATH=$E HOME=$E waybill --offline sbom scan ...
```

## `probe_edge_truth.py` — the primary probe

Reads the scanned tree's `go.mod` files itself and checks every emitted
Go dependency edge against them. A CycloneDX `dependsOn` entry states a
*direct* dependency; this asks whether waybill ever read that.

```bash
./probe_edge_truth.py <scanned-repo-path> <cdx.json>     # exit 1 if any edge is unbacked
```

Counts `// indirect` entries as backed — a manifest entry is something
waybill read, however it is annotated. Only edges with no manifest
backing anywhere in the tree are flagged, so the count is conservative.

### Results

`spf13/cobra` @ `a655097faf7d54f78933a815984b9919d51a05d2` (v1.9.1),
whose `go.mod` declares four direct requires:

```
                 golang edges   backed   stdlib   UNBACKED
warm cache (control)        5        4        1          0     exit 0
cold cache                  7        4        1          2     exit 1
```

The two unbacked: `cobra → blackfriday` and `cobra → check.v1`. Neither
is in cobra's `go.mod`; both are transitive. Warm, they attach to their
real parents.

`kubernetes/kubernetes` @ `157e582fcc3ebba3c22b16721f49d6890f784c1f`
(v1.37.0), cold, 39 `go.mod` files:

```
emitted golang edges                   2984
  backed by a declared require         2392
  synthetic stdlib node                  38
  NOT backed — asserted, never read     554   (18.6%)
```

Hand-verified sample: `k8s.io/api`'s `go.mod` declares none of
`go-difflib`, `testify`, `go.yaml.in/yaml/v3`, `gopkg.in/yaml.v3` or
`k8s.io/streaming` (the last appears only under `replace`), yet all five
are emitted as its direct dependencies.

### The warm-cache control matters

Running only the cold case proves nothing — a probe that flags
everything is worthless. The warm control returns **0 unbacked, exit 0**
on the same repository and the same commit. Reproduce it with:

```bash
W=$(mktemp -d)
(cd <repo> && GOMODCACHE=$W GOFLAGS=-mod=mod go mod download all)
env GOMODCACHE=$W waybill sbom scan --path <repo> ...   # note: no --offline
```

## `probe_completeness.py` — the secondary probe

Finds documents that contradict themselves, and cross-tabulates an
independently-measured shape against the document's self-report.

```bash
./probe_completeness.py doc    <cdx.json> [...]          # exit 1 on contradiction
./probe_completeness.py corpus <quality-corpus run-*.json>
```

Both Go targets emit `graph-completeness = complete` alongside
`go-transitive-coverage = unknown`. kubernetes additionally reports
`reachable_count=494 total_count=494 orphan_count=0` — every component
reachable, *because* the unbacked edges attach them.

Across the 11 committed public-corpus goldens the `doc` mode flags 2
(`go-cobra`, `pants-example-golang`) and passes 9, four of which
legitimately declare `complete`. So it is not vacuously failing
everything.

## Reproducing the scope-boundary finding

The two targets #829 asked about are regenerable rather than committed
(same treatment as the m165 / m168 audit artefacts):

```bash
E=$(mktemp -d); W=$(mktemp -d); cd "$W"
clone() { mkdir -p "$1" && cd "$1" && git init -q . \
  && git remote add origin "$2" && git fetch -q --depth 1 origin "$3" \
  && git checkout -q FETCH_HEAD && cd ..; }

clone cmake https://github.com/nlohmann/json                 65ee68451d8eb2b5f3a30b410476ab83deb3289b
clone uv    https://github.com/meilisearch/meilisearch-python 8147a9dcff97da126663360588229079ab8400c8

for t in cmake uv; do
  env GOMODCACHE=$E GOPATH=$E HOME=$E waybill --offline sbom scan \
    --path "$W/$t" --format cyclonedx-json --output cyclonedx-json="$W/$t.json" \
    --root-name "$t" --root-version test
done
```

Expected: both report `complete`, both have zero components marked with
an orphan reason, neither emits an ecosystem coverage signal, and
neither has any Go edges to fabricate. That places them outside this
feature's scope. Their flatness is genuine: the cmake target reads
scattered cmake/bazel declarations and the uv target reads
`Pipfile.lock` (not `uv.lock`, despite the target name), and neither
format carries parent-child topology.

## Is removing the backfill safe? (guac, warm cache)

The #251 backfill existed to stop `// indirect` requires being emitted
as orphans. Removing it could in principle re-open that issue, so it was
measured rather than assumed.

`guacsec/guac` @ `bf8c0bab` (current HEAD; the `ebb808e` #251 cited is
no longer fetchable), warm module cache so `go mod graph` resolves,
backfill removed. 1188 components, 13647 relationships. Each resulting
orphan checked against `go mod why -m`:

```
orphans after removing the backfill                                  17
  main module genuinely does not need them                           16
  reachable only via a *test* package, already carrying
    waybill:build-inclusion = not-needed (derivation: go-mod-why)      1
  genuine attribution failures                                        0
```

#251 originally reported **161 of 660 (24%)**. The resolver work since
alpha.36 closed it. The backfill has since been attaching edges to 17
modules that are *correctly* orphaned, so removing it improves the
output rather than regressing it. Recorded on #251.

### Two traps when re-running this

Both produce a convincing false positive:

1. `go mod why -m` prints `# <module>` as a header before its answer.
   Parsing that as the result reports every module reachable *via
   itself* — an obviously wrong output that is easy to skim past.
   The real signal is the literal `(main module does not need module …)`
   line.
2. `go mod why -m` counts paths through `.test` packages. A module
   reachable only that way is not a build dependency; waybill discounts
   it deliberately (catalogue row C62). Counting it manufactures an
   attribution failure that is actually correct behaviour.

## A warning about stale evidence

The figures in #857 and #829 — 490 edges, depth 1, `flat = true` — were
measured before m860 (`359d2f44`, 2026-09-13 14:01) changed root
anchoring and main-module retention. The most recent quality-corpus run
available when this spec was drafted (`be7aa6b3`, 2026-09-12 21:53)
predates that commit, and so did the `target/release/waybill` binary
sitting on disk.

Re-measuring changed the finding materially: the graph is depth 2 rather
than flat, cobra emits the *same* seven non-root edges cold and warm
rather than losing 72% of them, and the components carrying
`waybill:orphan-reason` are all attached rather than dangling. The
defect survived m860 by changing shape while every check kept passing.

**Rebuild the binary and re-run both probes before quoting any number
here.** A figure describing a defect goes stale the moment anything near
it moves.

## A warning about measuring this

Three earlier attempts returned a clean result for the wrong reason and
would each have supported a false conclusion:

- Comparing file-tier component `name` against package `occurrences`
  never matches: a file-tier component's `name` is a basename while its
  paths live in a property.
- Looking up `waybill:component-tier` returns nothing, because the
  property is `waybill:sbom-tier`.
- Reading `waybill:orphan-reason` as a reachability claim: it is a
  provenance marker. In cobra it sits on six components, of which four
  are on perfectly good edges, and it does not identify either unbacked
  edge.

Any check added here must be run against a known-bad input and
**observed to fail** before its passing result is believed.

---

## T041 — what the corpus regeneration turned up

Regenerating the public-corpus goldens is where the fix met eleven real
projects instead of two, and it surfaced one defect the local suites
could not see.

### The SPDX 3 root-attachment divergence

With the unbacked Go edges gone, `go-cobra`'s SPDX 3 document asserted
two dependency edges that CycloneDX and SPDX 2.3 did not:

```
REL go-cobra -[dependsOn]-> github.com/russross/blackfriday/v2
REL go-cobra -[dependsOn]-> gopkg.in/check.v1
```

Those are the same two modules the *same document* reports as
`orphaned-components-detected: 2 component(s) not reachable from root`.
The document contradicted itself, and it did so by fabricating a direct
dependency — the exact falsehood this milestone exists to remove,
displaced one level up and into one format.

**Why it was latent.** Both stranded modules used to appear in some
relationship's `to` position, courtesy of the fabricated
`main-module -> <every go.sum module>` edges. The issue-#236 fallback
skips anything already depended on, so it never touched them. Removing
those edges promoted both to graph roots and handed them straight to the
fallback.

**Why only SPDX 3.** All three emitters carry a copy of that fallback.
CycloneDX gates it on `target_has_no_edges`; SPDX 2.3 gates it on
"synth_id has no outgoing edges" — a gate its comment records as having
been added after an earlier over-attachment bug. The SPDX 3 copy never
got one. It could not have been gated as written, either: SPDX 3 was
also dropping the `root -> <retained main module>` edge that m860
anchors, because `build_dependency_relationships` resolves endpoints
through `package_iri_by_purl` and the root's ref is an operator-supplied
name, not a PURL. With no entry, the edge was silently discarded and the
root genuinely had no outgoing edges — so the fallback fired, and fired
unfiltered.

Two changes, both narrowing SPDX 3 onto what the other two formats
already did:

1. alias the root's own ref onto the synthesized root IRI, so m860's
   anchor edge survives. Deliberately narrower than the issue-#229 alias
   that m860 removed — that one mapped *every* dropped main-module PURL
   onto the root and collapsed N modules' annotations onto one subject.
2. gate the fallback on the root already having an outgoing edge,
   mirroring SPDX 2.3.

After both, all three formats agree:

```
root      -> github.com/spf13/cobra        (and nothing else)
cobra     -> go-md2man, mousetrap, pflag, yaml.v3, stdlib
blackfriday, check.v1: present, no incoming edge
```

**Teeth-checked.** `spdx3_synth_root_does_not_attach_stranded_components`
(in `spdx/document.rs`, beside its two SPDX 2.3 siblings) was observed to
fail against the pre-fix emitter — reporting both root targets including
the stranded one — before it was trusted to pass. SPDX 3 had no test
module at all, which is the whole reason the gap survived; the two
SPDX 2.3 tests guarding this exact behaviour had no mirror.

### Attribution of the golden drift

Seven of eleven targets drifted. Every category traced to a named commit:

| targets | change | cause |
|---|---|---|
| `go-cobra`, `pants-example-golang` | edges removed; 4 and 2 false `waybill:orphan-reason` markers dropped | `e595bad1` (#880) |
| `go-cobra`, `pants-example-golang` | `graph-completeness` `complete` → `partial` + reason | `ceec02a2` (FR-003) |
| `rust-ripgrep`, `python-flask`, `maven-guice`, `go-cobra` | `compositions` dependency claim split off by reachability | `e595bad1` (#871) |
| `image-postgres16`, `npm-express`, `pants-example-golang` | same split, by degraded ecosystem | `6f926977` |
| `rust-ripgrep`, `maven-guice`, `python-flask`, `npm-express`, `pants-example-golang`, `go-cobra` | SPDX 3 root edges replaced | `61f8ba98` |
| all drifted targets | SPDX 3 `spdxId`/`statement`/`subject` churn | content-addressed IDs re-hashing over the above |

Two notes on reading that table:

- The removed `waybill:orphan-reason` markers are a correctness gain, not
  churn. On `go-cobra` they sat on `go-md2man`, `mousetrap`, `pflag` and
  `yaml.v3` — which are precisely the four modules cobra's `go.mod`
  declares. The marker now sits on the two genuinely stranded modules
  and nothing else.
- `image-postgres16` is the evidence that the degraded-ecosystem change
  withholds rather than blanket-demotes: its `deb` ecosystem (142
  components) keeps `aggregate: complete`, while only `generic` and
  `golang` (2 components) drop to `unknown`.

Component counts are unchanged on all eleven targets. Only `go-cobra`
loses edges: 8 → 6, i.e. 7 → 5 golang edges, matching the quickstart.

### What the SPDX 3 fix actually changed, per target

`go-cobra` is the smallest case and the one the spec is written around,
but it is not the most striking. The ungated fallback had been attaching
whatever happened to be a graph root, and on most targets that was
**files, not dependencies**:

| target | root edges before | root edges after |
|---|---|---|
| `rust-ripgrep` | `utils.sh`, `ubuntu-install-packages`, `copy-examples`, `benchsuite`, `sha256-releases`, `test-complete`, `build-and-publish-m2` | the nine real workspace crates (`grep`, `grep-cli`, `globset`, `ignore`, …) |
| `maven-guice` | `deploy-guice.sh`, `diff-jars.sh`, `google_bazel_common`, and `pom.xml` three times over | the sixteen real Maven modules |
| `pants-example-golang` | `get-pants.sh`, `pants_from_sources` | the main module |
| `npm-express` | `run` | the main module |
| `python-flask` | 24 packages nothing declared a dependency on | the four real ones |

After the fix every target's root out-edge count is identical across
CycloneDX, SPDX 2.3 and SPDX 3:

```
go-cobra 1/1/1   image-postgres16 201/201/201   maven-guice 16/16/16
npm-express 1/1/1   pants-example-django 21/21/21   pants-example-golang 1/1/1
pants-example-jvm 4/4/4   pants-example-python 4/4/4   python-flask 4/4/4
rust-ripgrep 10/10/10
```

### One pre-existing divergence, left alone

`pants-example-javascript` reports `cdx=0, spdx-2.3=1, spdx-3=0` root
out-edges — SPDX 2.3 emits `root -> demo`, CycloneDX emits nothing. This
is the opposite direction from the defect fixed here (CycloneDX is the
outlier) and it is **pre-existing**: all three of that target's goldens
are byte-identical before and after, so it did not drift and nothing was
regenerated for it. Filed separately rather than encoded.

### The normalised diff and the gate agree, and that is load-bearing

The review tool reported exactly 11 changed (target, format) pairs; the
lane failed exactly 11. Had the gate failed more than the tool reported,
the surplus would have been array-ordering instability, which belongs in
`mask_nondeterministic` rather than in a golden.

The tool's raw counts are not the finding, though. It reported `[90x]
changed $.@graph[].spdxId` and `[82x] changed $.@graph[].statement` on
`go-cobra`; comparing the documents semantically instead — by
`(type, subject-name, statement)` rather than by position — the real
change is four annotation removals, one completeness value, and two
edges. SPDX 3 `spdxId`s are content hashes, so the tool cannot key on
them and reports neighbours of an insertion as changed. Read those
figures as "something moved here", never as a magnitude.

### Reproducibility

Two independent regeneration runs at `6f926977`
(`34918022260`, `34918425387`) produced byte-identical artifacts.

### Operational note — regeneration dispatches must be serialized

`public-corpus.yml` sets `concurrency: { group: public-corpus-${{
github.ref }}, cancel-in-progress: false }`, and every `workflow_dispatch`
resolves to the same `github.ref` regardless of the `branch` input. GitHub
keeps only **one** pending run per group: dispatching a second regeneration
while the first is still queued cancels the first. Dispatch, wait for
completion, then dispatch the next.

### Step 10 — the lane was observed failing, without a synthetic mutation

The refresh procedure ends by requiring proof that the lane can still
fail, "named target and all three formats", because a refresh that
silently disabled the gate would look identical to one that fixed it.

This cycle produced that proof from a real emission change rather than a
deliberate one. The read-only run before the goldens were installed
(`34917982329`) failed with:

```
3 of 3 formats drifted for go-cobra
3 of 3 formats drifted for pants-example-golang
1 of 3 formats drifted for image-postgres16
1 of 3 formats drifted for maven-guice
1 of 3 formats drifted for npm-express
1 of 3 formats drifted for python-flask
1 of 3 formats drifted for rust-ripgrep
```

The `3 of 3` lines are the load-bearing part: they show the SPDX 2.3 and
SPDX 3 comparisons actually ran and reported independently. The failure
mode milestone 840 caught — the layer-2 loop panicking on the first
failing format, so a red lane named only `cdx.json` and proved nothing
about the other two — would have printed `1 of 3` everywhere.

The confirming read-only run after installation (`34920854544`) is green.
