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
