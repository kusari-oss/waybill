# Phase 0 Research

All findings verified against `main` at `09805cad` with a binary built
from that commit. Line references are to that tree.

---

## R1 — Where the unbacked edges are created

**Decision**: There are **two** sites, not one.

> **Correction, found at implementation time (T005).** R1 originally
> claimed a single origin. Reading the surrounding code while building
> the index found a second: the m233 `replace`-directive block at
> `legacy.rs:~2063` adds an edge from a main module to a sibling main
> module whenever a `replace` points at that sibling's directory —
> **without checking for a matching `require`**. A `replace` redirects a
> path; it does not declare a dependency.
>
> Measured: `k8s.io/api`'s `go.mod` carries
> `replace k8s.io/streaming => ../streaming` and **no**
> `require k8s.io/streaming`, yet waybill emits
> `k8s.io/api -> pkg:golang/k8s.io/streaming@v0.0.0-unknown`.
>
> `contracts/edge-backing.md` C-3 already named this exact counter-case;
> the research pass did not. Both sites must be fixed for FR-001 to hold.

**Site 1** — the go.sum fallback augment loop at
`waybill-cli/src/scan_fs/package_db/golang/legacy.rs:~2020–2065`.

`build_main_module_entry` produces a main-module entry whose `depends`
holds the go.mod-declared requires. The loop then appends
`graph_map.gosum_fallback_paths_for(sums)` — every module that
resolution steps 1–3 failed to reach — to that same `depends` list.

`ModuleGraphMap::gosum_fallback_paths_for`
(`golang/graph_resolver.rs:334`) returns modules whose
`source == ResolutionStep::GoSumFallback`, filtered to those present in
this project's own `go.sum`.

**Rationale for locating it here**: downstream, `scan_fs/mod.rs:952–966`
turns every `entry.depends` string into a `DependsOn` relationship
without distinguishing which came from go.mod and which were appended.
By that point the two are indistinguishable, so the fix belongs at the
point of augmentation, not at emission.

**This was deliberate, not accidental.** The code comment states the
intent plainly: augment depends "so the SBOM includes flat root →
transitive edges recovering the ~110 transitive edges trivy captures
from go.sum content alone" (m091, refined by m233 to scope per
main-module). This milestone reverses that tradeoff — see R6.

---

## R2 — The provenance field already exists, and already lies

**Decision**: `Relationship.provenance` needs no new type. It needs to
be populated honestly and then emitted.

`waybill_common::resolution::Relationship` (`resolution.rs:401–408`)
already carries:

```rust
pub provenance: EnrichmentProvenance,   // "Where this relationship was discovered."
```

and `EnrichmentProvenance` (`resolution.rs:577`) is
`{ source: String, data_type: String }`.

Two facts make this the pivot of the whole feature:

1. **It is never emitted.** `grep '\.provenance' waybill-cli/src/generate/`
   returns nothing. The field is carried through the whole pipeline and
   discarded at serialization.
2. **For Go main-module edges it currently asserts something false.**
   `scan_fs/mod.rs:962` sets `source: entry.source_path`, and
   `legacy.rs` sets the main-module entry's `source_path` to the
   `go.mod` path. Every fallback-appended edge therefore claims **go.mod
   as its source** — and go.mod does not contain it.

**Consequence for the plan**: FR-002a is not "add a new claim". The
claim is already there, already wrong, and merely invisible. FR-001
makes it true and FR-002a makes it visible. That ordering matters: were
provenance emitted before the unbacked edges are removed, the product
would start publishing a falsehood it currently only holds in memory.

**Alternatives considered**: a new per-edge struct carrying
`Vec<DeclaringSource>` for edges declared in multiple manifests.
Rejected for now — no measured case of one edge backed by two distinct
manifests, and `source: String` can hold a path today. Revisit only if
FR-014's measurement surfaces one.

---

## R3 — Native-construct audit for edge provenance (Constitution Principle V)

**Decision**: native `comment` on the relationship for both SPDX
formats; a parity-bridging `waybill:*` property for CycloneDX, which has
no per-edge slot.

| Format | Native carrier for per-edge provenance | Verdict |
|---|---|---|
| SPDX 2.3 | `relationships[].comment` — **already in the struct** at `generate/spdx/relationships.rs:79`, `Option<String>`, currently set to `None` at every construction site (`:161`, `:311`, `:347`) | **Native exists. Use it.** |
| SPDX 3.0.1 | `Relationship` is an `Element`, so it inherits `comment` | **Native exists. Use it.** |
| CycloneDX 1.6 | `dependencies[]` entries carry only `ref`, `dependsOn` and (1.6) `provides`. No `properties`, no `evidence`, no annotation target for an edge — document-level `annotations[].subjects` reference components and services, not dependency entries | **No native. Parity bridge required.** |

This is precisely the carve-out Principle V describes: "to bridge a
parity gap when one format has the native field but another doesn't (in
which case the parity-bridging `waybill:*` annotation MUST be documented
in `docs/reference/sbom-format-mapping.md` with a justification clause
naming the missing native field)."

**Precedent for structured data inside an SPDX comment**: the m071
`MikebomAnnotationCommentV1` envelope already encodes JSON in comment
fields throughout the SPDX emitters, so this is an established pattern
rather than a new one.

**Catalogue obligation**: any new C-row requires a matching entry in
`parity/extractors/mod.rs::EXTRACTORS` in the same change, or
`parity::extractors::tests::every_catalog_row_has_an_extractor` and
`holistic_parity` both fail.

**Existing rows that are adjacent but do NOT satisfy FR-002a** — all
three are per-*component*, and FR-002a is per-*edge*:

- **C48 `waybill:resolver-step`** — which ladder step claimed the
  component (`go-sum-fallback`).
- **C108 `waybill:go-transitive-source`** — which of the 5 ladder steps
  resolved the module's transitive requires.
- **C45 `waybill:orphan-reason`** — includes a `flat-attached-fallback`
  value per C60's cross-reference.

A component reached by a backed edge from one parent and an unbacked
edge from another cannot be described by any of them. They stay; they do
not discharge FR-002a.

---

## R4 — Why the completeness signal cannot currently fire

**Decision**: no change to the BFS pass is needed for the Go case; the
fix cascades from R1. FR-003 (consult coverage signals) remains a
separate, deliberate belt-and-braces requirement.

The verdict at `generate/graph_completeness/mod.rs:315–322` is
`Complete` iff `reason_codes.is_empty() && orphan_count == 0`.

Both conditions are satisfied today for the wrong reason:

- `orphan_count == 0` because the unbacked edges attach every component.
  Measured on kubernetes: `reachable_count=494 total_count=494
  orphan_count=0`.
- No reason code, because the only classifier that could fire,
  `classify_transitive_edges_unresolvable`, keys on components at
  `design` or `analyzed` tier. Every go.sum-fallback component is
  `sbom_tier = source` (confirmed in the `go-cobra` golden: all 8
  components report `waybill:sbom-tier = source`). `affected_ecosystems`
  is therefore always empty and the classifier returns `None`.

**Rationale for not rewriting the BFS**: once R1 removes the unbacked
edges, the stranded components have no incoming edge, become genuine BFS
orphans, and the existing `OrphanedComponentsDetected` classifier fires
on its own. The reachability metric is not wrong in principle — it was
measuring a graph that had been pre-filled with invented edges.

**Why FR-003 is still required**: reachability is a weak proxy in
general. A future fallback that attaches to something real but wrong
would pass BFS again. Consulting the ecosystem coverage signal is an
independent check that does not depend on graph shape, and it is the
invariant #829 asked for.

---

## R5 — Ordering: the fix cascades

**Decision**: US1 (remove unbacked edges) must land before or with US2
(completeness). Never US2 alone.

Removing the edges converts the false `complete` into a correct
`partial` through machinery that already exists. Implementing US2 first
would mean writing a coverage-signal check whose only job is to override
a verdict that is about to stop being produced.

Conversely US1 alone leaves the document declaring `complete` over a
graph whose edges were just removed — strictly worse for a consumer than
today, which is why the spec ranks both stories P1.

---

## R6 — The tradeoff being reversed

**Decision**: accept emitting fewer edges than trivy in offline mode,
and say so.

m091's stated goal was parity with trivy's go.sum-derived edge count.
That is a real capability loss: after this change a cold-cache scan of
`go-cobra` emits 5 backed edges where it now emits 7, and kubernetes
loses up to 554.

**Rationale**: the 554 are not edges waybill found and trivy also found.
They are edges *neither* tool read from a manifest — go.sum cannot
supply them, because it records hashes, not requirers. Matching a
competitor's count by asserting relationships that no input states is a
bad trade for a tool whose value proposition is that its claims are
checkable.

**Mitigation, not compensation**: the remedy is real and already ships —
a resolvable module graph (warm cache, or `--warm-go-cache` from m173)
recovers the true topology. The warm-cache control measured 0 unbacked
edges and a correct `complete`. FR-008 requires the document to point at
this.

**This must be stated in the PR and the milestone record**, because a
reviewer comparing edge counts against trivy will otherwise read the
drop as a regression.

---

## R7 — Method for the FR-014 cross-ecosystem measurement

**Decision**: extend the probe per-ecosystem rather than generalise it
prematurely; report the result whichever way it comes out.

`measurements/probe_edge_truth.py` parses `go.mod` and so can only judge
Go edges. For each other ecosystem in the corpus the same question needs
its own manifest reader (`package.json` + lockfile, `pom.xml`,
`Cargo.toml`, `pyproject.toml`/`Pipfile.lock`, …).

**Cheap discriminator to prioritise**: the Go defect arises from an
input that supplies a module *set* with no requirer. Most lockfiles
(`package-lock.json`, `Cargo.lock`, `pnpm-lock.yaml`, `poetry.lock`)
record requirer→required directly, so they have nothing to fabricate
from. Readers to check first are those with a documented fallback path:
`nuget/mod.rs:1700` ("unlocked fallback: `<PackageReference>` feeds
main-module depends") and `gradle/mod.rs:273` (`fallback_history`).

**Explicitly not assumed clean.** Both of the above look like they read
real manifests, which would make their edges backed — but that is
inspection, not measurement, and the spec's SC-006a records the current
state as *unknown*.

---

## R8 — Corpus expectations that must be re-authored

**Decision**: re-author after the change, measured on an isolated
environment, in the same PR as the behaviour change.

- **`xtask/corpus/quality-corpus.toml`** — `go-cobra` and
  `go-kubernetes` `edges` and `max_depth` bounds are authored around
  current counts and will exceed their floors downward.
- **Public-corpus goldens** — `go-cobra` and `pants-example-golang` are
  the two committed goldens carrying the defect; both change.

**Non-negotiable method** (the #830 lesson, recorded in the corpus
config itself): re-measure with `$GOMODCACHE`, `$GOPATH` **and** `$HOME`
all pointed at an empty directory. Module-cache discovery falls back
through all three, so a warm cache in any one of them yields bounds no
clean runner can reproduce. That is exactly how the original bounds came
to describe a graph CI could never produce.

**Golden regeneration is CI-dispatch only**, per
`docs/development/refreshing-corpus-goldens.md`. Freeze the fix set
before regenerating: any further emission-affecting change invalidates
the run.

---

## Resolved unknowns

| Unknown from Technical Context | Resolution |
|---|---|
| Where unbacked edges originate | R1 — single site, `legacy.rs:~2036` |
| Whether a new provenance type is needed | R2 — no; the field exists and is unemitted |
| Native carrier for edge provenance | R3 — SPDX both native; CDX needs a parity bridge |
| Whether the BFS pass needs rewriting | R4 — no; the fix cascades |
| Story ordering | R5 — US1 before/with US2, never US2 alone |
| Scale of the edge-count drop | R6 — cobra 7→5; kubernetes up to −554 |
| How to measure other ecosystems | R7 — per-ecosystem readers; NuGet and Gradle first |
| Which expectations change | R8 — 2 corpus bounds, 2 goldens |

No unresolved NEEDS CLARIFICATION remain.
