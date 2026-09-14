# Quickstart

How to see the defect, and how to know when it is fixed. ~10 minutes.

## Prerequisites

- a `waybill` release binary built from the commit under test —
  **rebuild it**; a stale binary is how the first draft of this spec
  came to describe a product state that no longer existed
- `git`, `python3`
- `go` (only for the warm-cache control)

## 1. Reproduce the defect

```bash
W=$(mktemp -d); cd "$W"
git init -q cobra && cd cobra
git remote add origin https://github.com/spf13/cobra
git fetch -q --depth 1 origin a655097faf7d54f78933a815984b9919d51a05d2
git checkout -q FETCH_HEAD && cd ..

# All three env vars are required. Module-cache discovery falls back
# $GOMODCACHE -> $GOPATH -> $HOME/go/pkg/mod, so a warm cache in any
# one of them masks the defect entirely.
E=$(mktemp -d)
env GOMODCACHE=$E GOPATH=$E HOME=$E waybill --offline sbom scan \
  --path "$W/cobra" --format cyclonedx-json \
  --output cyclonedx-json="$W/cold.json" \
  --root-name go-cobra --root-version a655097
```

Check the edges:

```bash
probe_edge_truth.py "$W/cobra" "$W/cold.json"
```

Before the fix:

```
emitted golang edges       : 7
  backed by a declared require : 4
  synthetic stdlib node        : 1
  NOT backed (asserted, never read): 2

  github.com/spf13/cobra -> github.com/russross/blackfriday/v2
  github.com/spf13/cobra -> gopkg.in/check.v1
```

Neither is in cobra's `go.mod`, which declares exactly four requires:
`go-md2man`, `mousetrap`, `pflag`, `yaml.v3`.

Check the declaration:

```bash
probe_completeness.py doc "$W/cold.json"
```

Before the fix: `graph-completeness = complete` alongside
`go-transitive-coverage = unknown`.

## 2. Run the control — this is the part people skip

A probe that flags everything proves nothing. The same repository with a
resolvable module graph must come back clean.

```bash
C=$(mktemp -d)
(cd "$W/cobra" && GOMODCACHE=$C GOFLAGS=-mod=mod go mod download all)

# note: no --offline, and only GOMODCACHE is redirected
env GOMODCACHE=$C waybill sbom scan \
  --path "$W/cobra" --format cyclonedx-json \
  --output cyclonedx-json="$W/warm.json" \
  --root-name go-cobra --root-version a655097

probe_edge_truth.py "$W/cobra" "$W/warm.json"    # expect: 0 unbacked, exit 0
```

Warm emits 5 golang edges, all backed, and correctly declares
`complete`. Cold emits 7 — the *same* total non-root edge count, with
two attached to the wrong parent. Cold does not lose edges. It
misattributes them.

## 3. What "fixed" looks like

| | before | after |
|---|---|---|
| `probe_edge_truth.py` on cold | exit 1, 2 unbacked | **exit 0**, 0 unbacked |
| `probe_edge_truth.py` on warm | exit 0 | **exit 0** — must not move |
| cold golang edges | 7 | 5 |
| cold components | 8 | **8** — none dropped |
| `blackfriday`, `check.v1` | attached to cobra | present, no incoming edge |
| cold `graph-completeness` | `complete` | not `complete`, with a reason |
| warm `graph-completeness` | `complete` | **`complete`** — must not move |

The two rows marked "must not move" are the ones that catch a fix that
went too far.

## 4. At scale

```bash
git init -q k8s && cd k8s
git remote add origin https://github.com/kubernetes/kubernetes
git fetch -q --depth 1 origin 157e582fcc3ebba3c22b16721f49d6890f784c1f
git checkout -q FETCH_HEAD && cd ..
# ~400 MB clone, ~15 s scan, 39 go.mod files
```

Before: 2984 golang edges, 554 unbacked (18.6%), `complete`,
`orphan_count=0`.

Spot-check one by hand rather than trusting the aggregate —
`k8s.io/api`'s `go.mod` declares none of `go-difflib`, `testify`,
`go.yaml.in/yaml/v3`, `gopkg.in/yaml.v3` or `k8s.io/streaming` (that
last appears only under `replace`), yet all five are emitted as its
direct dependencies.

## 5. Before regenerating any corpus expectation

Edge counts **fall** — that is the intended outcome, not a regression.

Re-measure the m770 bounds with `$GOMODCACHE`, `$GOPATH` **and** `$HOME`
all isolated. Bounds authored on a machine with a populated module cache
describe edges no clean runner can resolve; that is exactly how the
original bounds came to be wrong, and #830 had to correct them.

Public-corpus goldens regenerate by CI dispatch only — see
`docs/development/refreshing-corpus-goldens.md`. Freeze the fix set
first: any further emission-affecting change invalidates the run.

## Traps

- **A stale binary.** Rebuild before measuring. The first draft of this
  spec was written against a pre-m860 build and got three claims wrong.
- **A warm cache leaking in.** Set all three env vars. One is not
  enough.
- **Trusting a clean result.** Every check here must be run against a
  known-bad input and *observed to fail* before its passing result means
  anything. Three earlier attempts at measuring this returned "0
  problems" for the wrong reason — wrong property name, wrong field,
  wrong semantics.
- **Reading `waybill:orphan-reason` as "unattached".** It is a
  provenance marker. In `go-cobra` it sits on six components, four of
  which are on perfectly good edges, and it identifies neither unbacked
  edge.
