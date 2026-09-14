# Contract: Edge backing

The rule every emitted dependency relationship must satisfy, and the
observable consequences of it.

## C-1 — The backing rule

> Every emitted dependency relationship MUST truthfully state where it
> came from. A relationship whose recorded provenance names a source
> that does not contain it MUST NOT be emitted.

A file that enumerates components without attributing them to a
requirer cannot back a relationship attributed to it. This is not a
Go-specific rule (FR-013); Go is where it is enforced and gated in this
milestone.

### C-1a — locality is not the test

The rule is about **truthful attribution**, not about where the source
lives. An edge sourced from outside the scanned tree is legitimate when
it says so.

| edge | stated source | does that source contain it? | verdict |
|---|---|---|---|
| `cobra -> blackfriday` (go.sum fallback) | `go.mod` | **no** | forbidden — the defect |
| `k8s.io/api -> k8s.io/streaming` (replace-only) | `go.mod` | **no** | forbidden |
| maven transitive from `enrich/deps_dev_graph.rs` | `deps.dev` | yes | **permitted** |

The deps.dev dep-graph enricher (Maven-only, on by default) records
`EnrichmentProvenance { source: "deps.dev", data_type:
"dependency-graph" }` on every edge it contributes. That claim is true,
so the edges are sound.

Measured on `transitive_parity/maven` with an empty `~/.m2`: the maven
reader alone emits **zero** transitive edges — the parity baseline is
literally `EXPECTED_WAYBILL_EDGE_COUNT = 0` — while enrichment supplies
50. Forbidding non-local edges would therefore delete the only
dependency graph a cold-cache Maven scan has.

Two guards make this safe, and both already exist:

- deps.dev is authoritative for topology, **never** for versions. If it
  says `A -> B@1.0` and the scan observed `B@1.5`, the edge targets the
  observed `B@1.5`.
- a coord deps.dev names that was never seen on disk is emitted as
  `declared-not-cached`, keeping "observed" separable from
  "declared-but-unseen".

## C-2 — What backs an edge, for Go

| Source | Backs an edge? | Why |
|---|---|---|
| `go.mod` `require` block entry | **yes** | states requirer → required |
| `go.mod` single-line `require x v1` | **yes** | same |
| `go.mod` entry marked `// indirect` | **NOT a direct edge** | it records that the module graph needs the module, not that *this* module imports it. A CycloneDX `dependsOn` / SPDX `DEPENDS_ON` edge states a DIRECT dependency, so emitting one for an indirect entry misstates directness. `build_main_module_entry` has filtered these out since m059; the backfill was silently undoing that. Measured: 1841 of kubernetes' 2984 emitted edges were main-module → `// indirect` |
| `go.mod` `replace` directive | **as redirection only** | rewrites a path; does not itself declare a requirement |
| `vendor/modules.txt` | **yes** when the tree is vendored | records the resolved graph |
| `go.sum` | **NO** | a hash list. It names modules without saying who requires them — the precise reason the fallback had nothing to attribute and filled the gap by inventing edges |

A requirement declared in **any** module's `go.mod` in the tree backs an
edge from **that** module. It does not license an edge from a different
module.

## C-3 — Multi-module workspaces

A `replace` MUST be resolved on both sides: a requirement declared
against a pre-replace path backs an edge emitted against the
post-replace path, and vice versa.

Measured case: `k8s.io/api`'s `go.mod` declares
`require k8s.io/apimachinery v0.0.0` and
`replace k8s.io/apimachinery => ../apimachinery`. The edge
`k8s.io/api → k8s.io/apimachinery` is backed.

Counter-case from the same file: `k8s.io/streaming` appears **only**
under `replace`, never under `require`. An edge
`k8s.io/api → k8s.io/streaming` is therefore **not** backed, and is one
of the 554 this feature removes.

## C-4 — Observable consequences

| Before | After |
|---|---|
| `go-cobra` cold: 7 golang edges, 2 unbacked | 5 golang edges, 0 unbacked |
| `go-cobra` warm: 5 edges, 0 unbacked | **unchanged** — the warm path is already correct and is the control |
| `kubernetes` cold: 2984 edges, 554 unbacked | **602 edges, 0 unbacked** (551 direct requires + 13 `tool` directives + 38 stdlib) |
| `blackfriday`, `check.v1` attached to `cobra` | present in inventory, no incoming edge |

The warm-cache row is the one that must not move. If it does, the change
has broken correct resolution rather than removed invented edges.

The kubernetes drop is larger than the unbacked count alone because two
distinct classes go away. Measured:

```
                  direct   // indirect   fabricated   stdlib
before               551          1841          554       38
after                551            13*           0       38
```

\* the 13 remaining are backed by Go 1.24+ `tool` directives, which
declare a build-time dependency even though Go records the module as
`// indirect`.

Every genuinely-direct edge survives. Nothing legitimate was lost.

## C-5 — Components are never dropped

Removing an edge MUST NOT remove a component. `go-cobra` emits 8
components before and 8 after; only the relationship count changes.

No relationship may be synthesised to the document root, or anywhere
else, to make a stranded component reachable. Reachability is an outcome
of the graph, never a goal to be manufactured — manufacturing it is the
defect being fixed.

## C-6 — Provenance

Every emitted relationship carries the manifest path that backs it,
sufficient to locate the declaration without re-running the scan.

Carriers per research R3 — native where one exists:

| Format | Carrier |
|---|---|
| SPDX 2.3 | `relationships[].comment` (native) |
| SPDX 3.0.1 | `Relationship.comment` (native) |
| CycloneDX 1.6 | parity-bridging `waybill:*` property — no native per-edge slot exists |

**Ordering constraint**: `Relationship.provenance.source` today names
`go.mod` for the fallback-appended edges, which do not appear in
`go.mod`. It is currently wrong and merely unemitted. C-1 must be
satisfied **before** provenance is emitted, or the product starts
publishing a falsehood it presently only holds in memory.

## C-7 — Verification

```bash
# Any Go target, cold cache. Exit 1 means an unbacked edge survived.
probe_edge_truth.py <scanned-repo> <cdx.json>

# Control: the same repo with a warm cache must stay at 0.
probe_edge_truth.py <scanned-repo> <cdx-warm.json>
```

A probe run only against the cold case proves nothing — a check that
flags everything is worthless. Both arms are required.
