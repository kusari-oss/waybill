# Data model

No wire change: no new annotation, no new catalog row. C42 and C62 values move on existing components only.

## Seed sets

| Set | Members | Used by |
|---|---|---|
| `seeds` (unchanged) | primary root if it is a main module; one main module per other ecosystem | Phase A; also graph completeness (computed separately in `generate/`) |
| `added` (new) | every main module (`waybill:component-role` = `main-module`) not in `seeds`, whose `lifecycle_scope` is `None` or `Runtime`, and not all of whose evidence files lie under a listed dot-directory | Phase A only |
| `ecosystems_with_root` (unchanged) | ecosystems with an entry in `seeds` | Phase B's FR-012 gate |

## PropagationSummary

Adds one field:

| Field | Meaning |
|---|---|
| `immune_by_added_seeds: usize` | components Phase B reached, unscoped and not compiler-supplied, that are immune only through `added` |

## Invariant (R4)

`immune(seeds ∪ added) ⊇ immune(seeds)`. So `assigned(after) ⊆ assigned(before)`, with equal scopes on the intersection.
