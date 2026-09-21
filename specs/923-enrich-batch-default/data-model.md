# Phase 1 — Data Model

**Feature**: `923-enrich-batch-default` (#927)

This feature adds no emitted data. It changes which request shape is used and
when one stops being used. The entities below are runtime state and CLI
surface, not document content.

## Entity: Enrichment path selection

| | |
|---|---|
| **Values** | batched (default) \| per-component |
| **Source** | absent flag → batched; opt-out flag → per-component |
| **Inert when** | `--offline`, deps.dev disabled, or enrichment sources exclude deps.dev |
| **Observable in output** | no — the two paths produce equivalent documents (FR-002) |

The last row is the load-bearing one. If the selection were observable in the
document, flipping the default would be a wire change and every consumer would
have to care.

## Entity: Batch circuit state

| | |
|---|---|
| **Shape** | open (batched allowed) \| tripped (batched abandoned for this scan) |
| **Transition** | open → tripped on the first batch failure; never back |
| **Scope** | one scan. Not persisted, not shared between runs |
| **On trip** | a log line naming the failure (FR-007b), and the existing document-scope degradation record (FR-007) |

**One-way by design.** A breaker that reopened would retry a surface that just
failed, which is the per-chunk retry this feature is replacing.

**Exactly one wasted attempt**, because batch requests are issued sequentially
(research R2) — the failure is observed before the next request goes out. This
is a property of today's execution, not a law: FR-007a records that if
concurrency is ever added the guarantee becomes "at most one concurrency
group", and that the weakening must be deliberate.

## Entity: Degradation record *(existing, unchanged)*

`BatchUnavailable`, catalogue row C158, document scope. Already built in
milestone 839; its own documentation states the invariant this feature relies
on — *enrichment content is unaffected; this costs speed, not coverage*.

**What changes is when it can appear.** With the batched path off by default,
no default scan could emit it. After the flip, one can. That is confined to
the failure path and is strictly more information than before.

## Entity: Opt-in flag *(existing, becomes inert)*

The current `--enrich-batch` keeps being accepted and stops meaning anything,
because batching is the default. Removing it would break scripts for no
benefit; erroring on it would break them louder.

## Relationships

```
flags ──select──> enrichment path
batch failure ──trips──> circuit state ──suppresses──> further batch attempts
                      └──emits──> log line (operator, during)
                      └──records──> degradation (consumer, after)
```

The two outputs of a trip have different audiences and different lifetimes,
which is why both exist rather than one.

## What this does not change

- Emitted document content on a successful scan.
- The enrichment cache's shape, location or freshness rules.
- Any `waybill:*` field, existing or new.
- Behaviour when enrichment is disabled.
