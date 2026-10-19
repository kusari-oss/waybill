# Data model: every unreachable component says why it has no edge

**Feature**: `1218-explain-every-orphan` | **Date**: 2026-10-19
**Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

## Three words this feature keeps distinct

| Term | Means | Lives in |
|---|---|---|
| **mechanism** | one of the six measured classes, labelled A1, A2, B, C, D, E. There is no bare "A"; A1 and A2 are two distinct deferred defects that an earlier per-target reading had merged. | spec.md |
| **population** | the set of components a mechanism covers — "mechanism B's population is ten" | spec.md, measurements |
| **arm** | the `match` branch that implements a mechanism's predicate | data-model.md, tasks.md, the code |

A mechanism is a class, a population is its members, an arm is the code. The
three are not interchangeable and the documents use them that way.

No new type is introduced. Three variants are added to one existing enum,
three fields to one existing counter struct, and three predicates to one
existing classification site.

---

## E1 — `OrphanReasonCode` (existing enum, extended)

`waybill-cli/src/generate/orphan_reason.rs:71-91`. Five variants today,
eight after this feature. `as_str()` is the wire contract and its existing
five strings are frozen; the three new ones are frozen by
[contracts/orphan-reason-vocabulary.md](./contracts/orphan-reason-vocabulary.md)
before any emission task runs.

| Variant | Mechanism | Population | Status |
|---|---|---|---|
| `FlatAttachedFallback` | — | Go reader-time backfill | unchanged, never overwritten |
| `StaleGoSumEntry` | — | Go, reachable sibling | unchanged |
| `DeadLockfileEntry` | **B** | npm **or pypi**, lockfile fork with reachable sibling | **domain widens**, assignments unchanged |
| `HoistedUnused` | — | npm, no sibling | unchanged |
| `UnresolvedIndirectRequire` | — | Go, no sibling | unchanged |
| *new* | **E** | toolchain pin, `design` tier | added |
| *new* | **C** | foreign build system's manifest | added |
| *new* | **D** | lockfile node nothing references | added |

`DeadLockfileEntry` is the only existing variant whose domain changes. Per
FR-004 that is permitted and assignments are not: every component carrying
it today still carries it, because widening a domain un-assigns nobody.
SC-016 verifies this by diffing the per-component value across all
seventeen goldens.

---

## E2 — `OrphanReasonCounts` (existing struct, extended)

`orphan_reason.rs:96-121`. One `usize` per code plus `tally()`. Three new
fields and three new `tally()` arms, mechanically paired with E1 — the
compiler enumerates the work, which is Principle IV doing its job.

Consumed by the call site's log line (FR-012a). Today the whole struct is
discarded: `let _m167_orphan_reason_counts = …` at `scan_cmd.rs:4939`.

---

## E3 — The predicate inputs

Read-only. Every one is already on the component when the classifier runs
(`scan_cmd.rs:4940`, after `tag_components_with_workspace_member` at
`:4308`). Nothing new is plumbed.

| Input | Access | Shape | Caution |
|---|---|---|---|
| ecosystem | `c.purl.ecosystem()` | `&str` | — |
| versionless identity | `versionless_identity(c.purl.as_str())` | `&str` | strips at first `@`; ecosystem-neutral already (m866) |
| reachability | `reachable_set.contains(purl)` | `bool` | computed by m158's BFS, passed in |
| tier | `c.sbom_tier` | `Option<String>` | **struct field**, `resolution.rs:103` — not an annotation |
| source paths | `c.evidence.source_file_paths` | `Vec<String>` | **raw**; see the caution below |
| workspace member | `c.extra_annotations["waybill:workspace-member"]` | JSON-array-encoded **string** | `"[\"examples/celery\"]"`, not an array — written `scan_fs/mod.rs:1663` |
| source mechanism | `c.extra_annotations["waybill:source-mechanism"]` | plain string | absent on pip today; FR-002a fixes that |
| existing reason | `c.extra_annotations["waybill:orphan-reason"]` | plain string | the m061 preserve-guard reads this |

**The path caution, which is load-bearing.** `waybill:source-files` is not
an annotation. It is derived at emit time in
`generate/cyclonedx/builder.rs:1500-1509` by passing
`evidence.source_file_paths` through `sbom_path::emitted_source_files`,
which relativizes against the scan roots (`sbom_path.rs:122-124`). The
classifier therefore sees raw paths — possibly absolute, possibly
`path+file://<abs>` — while every path in this feature's measurements is the
emitted spelling. A predicate that string-matches
`"examples/celery/requirements.txt"` against the raw value will silently
match nothing.

Predicates MUST match on **file name** (`Path::file_name`, invariant under
relativization) or route through `emitted_source_files` first. A unit test
asserts an absolute and a relative input classify identically.

---

## E4 — Classification order

A total order, evaluated most-specific first. New arms sit strictly below
m167's so FR-004 holds by position rather than by case analysis.

```
0. existing `flat-attached-fallback`     → preserve, tally, stop   (m061 guard)
1. reachable                             → no marker, stop
2. already explained by another marker   → no marker, stop          (FR-008)
3. Go      + reachable sibling           → stale-go-sum-entry
4. lockfile fork + reachable sibling     → dead-lockfile-entry      (B)
5. npm     + no sibling                  → hoisted-unused
6. Go      + no sibling                  → unresolved-indirect-require
7. design tier + NO source files + NO member → <E>
8. foreign build system's manifest       → <C>
9. unreferenced lockfile node            → <D>
10. otherwise                            → no marker                (FR-011)
```

Arm 10 is the requirement, not the fallthrough: a component matching nothing
stays unexplained and keeps failing I7. There is no catch-all.

**Arm 4 is the dangerous one.** Deleting the ecosystem guards widens the
sibling index to every ecosystem, which mechanism B needs — and which also
lets `maven-guice`'s `guice@5.1.0` find its reachable `guice@7.0.0` sibling.
"Has a reachable sibling" is therefore **not sufficient** for arm 4; it must
also be a *lockfile resolution fork* rather than a *manifest-declared
coordinate*. The distinguishing inputs are measured and available:

| | python-flask (B, 10) | maven-guice (#1235, 3) |
|---|---|---|
| source | `uv.lock` | `examples/guice-demo/pom.xml`, `extensions/struts2/pom.xml` |
| `waybill:source-type` | absent | `workspace` |
| reachable sibling | yes | yes |

If arm 4 fires on the maven three, SC-001 reads better and #1235's cell
closes silently. SC-004 is the test that catches it.

**Arm 7 has the same shape of trap, and a worse failure mode.** `design`
tier is not a synonym for mechanism E: `haskell-language-server` holds six
unreachable `design`-tier components, of which only two are toolchain pins.
The other four — `docutils`, `myst-parser`, `sphinx`, `sphinx-rtd-theme`
from `docs/requirements.txt`, `workspace-member = ["docs"]` — are
documentation dependencies already explained by C151.

| | mechanism E (2) | the docs four (4) |
|---|---|---|
| `sbom_tier` | `design` | `design` |
| `source_file_paths` | empty | `docs/requirements.txt` |
| `waybill:workspace-member` | absent | `["docs"]` |
| already explained | no | **yes, by C151** |

Arm 7 therefore tests all three fields, not the tier. And because those four
are already explained they are **not** I7 offenders, so an arm that fires on
them moves no count: SC-001 through SC-004 all still pass while the document
asserts Sphinx is build tooling. SC-018 and T027a are the only checks that
would catch it.

---

## E5 — `I7 declaration` (existing committed data, edited)

`waybill-cli/tests/corpus_harness_195/invariant-declarations.toml`. Five
`[[declaration]]` tables with `invariant`, `target`, `kind`,
`count_at_declaration`, `reason`.

| target | before | after |
|---|---|---|
| `go-opentelemetry` | 1 | **deleted** |
| `haskell-language-server` | 2 | **deleted** |
| `haskell-security-advisories` | 3 | **deleted** |
| `python-flask` | 23 | `count_at_declaration = 13`, reason cites **#1234** |
| `maven-guice` | 5 | `count_at_declaration = 3`, reason cites **#1235** |

`kind` stays `"unreachable-and-unexplained"` on both survivors — the kind is
unchanged, only the count and the attribution. The lane fails on a stale
declaration, so an un-edited survivor breaks the build, which is the
mechanism that makes FR-014 self-enforcing.

---

## What this feature does not touch

- **`ResolvedComponent`** — read only. No field added, none changed.
- **Edges and reachability** — FR-009. SC-009 asserts component counts, edge
  counts and the reachable set are byte-identical in all seventeen goldens.
  The missing edges are #1234's work.
- **The catalog and the extractor set** — SC-007. C45's row *text* changes;
  no row is added, and C45's extractor already handles an arbitrary string
  value.
- **`is_explained`** — SC-014 pins the arm count at five. Every new value
  flows through the arm that already tests `waybill:orphan-reason`. A sixth
  arm means a new annotation was introduced and FR-001 was missed.
