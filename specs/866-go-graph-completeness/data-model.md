# Phase 1 — Data Model

Only what this feature adds or changes. Existing types are described
just far enough to say what happens to them.

---

## DeclaringSourceIndex (new, in-process)

The authority for FR-001/FR-002: what the scanned tree actually
declares.

| Field | Meaning |
|---|---|
| `by_requirer` | requirer module path → set of `DeclaredRequirement` |
| `sources_parsed` | manifest paths read, for diagnostics and the FR-009 gate |

**Construction**: one pass over the scanned tree per Go workspace,
before edge emission. Inputs, per the FR-002 clarification:

- every `go.mod` in the tree — `require` blocks and single-line
  `require`. `// indirect` entries are **recorded but do not produce a
  direct edge** from the module that lists them: the marker records that
  the module graph needs that module, not that this module imports it,
  and `dependsOn` states a direct dependency. `build_main_module_entry`
  has filtered them since m059. Measured: 1841 of kubernetes' 2984
  emitted edges were main-module → `// indirect`.

**`replace` handling**: a `replace` directive redirects a module path.
The index MUST resolve both sides so that a requirement declared against
the pre-replace path still backs an edge emitted against the
post-replace path. Measured need: `k8s.io/api` declares
`k8s.io/apimachinery` and redirects it to `../apimachinery`.

**Lifetime**: per scan, dropped after edge construction. No cache, no
persistence — matching every reader milestone since 002.

---

## DeclaredRequirement (new)

| Field | Meaning |
|---|---|
| `requirer` | module path declaring the requirement |
| `required` | module path being required |
| `source_path` | manifest that declares it — the value FR-002a emits |
| `indirect` | whether the entry was marked `// indirect` |

`indirect` IS filtered on for direct-edge emission, and is also carried
so FR-007a can keep "declared indirectly" distinct from "parent
unknown". A Go 1.24+ `tool` directive overrides it: a module listed
under `tool` is a declared build-time dependency even though Go records
its `require` as `// indirect` (13 such edges on kubernetes).

---

## Relationship (existing — `waybill_common::resolution::Relationship`)

No shape change. Two behavioural changes:

| Field | Today | After |
|---|---|---|
| `from` / `to` / `relationship_type` | unchanged | unchanged |
| `provenance.source` | set to the declaring entry's `source_path`; for fallback-appended Go edges this names `go.mod`, **which does not contain them** (research R2) | names the manifest that genuinely declares the requirement |
| `provenance` emission | never serialized — `grep '\.provenance' generate/` returns nothing | emitted per FR-002a |

**The critical ordering constraint**: `provenance.source` is wrong today
for exactly the edges this feature removes. Emitting provenance before
removing them would publish a falsehood the product currently only holds
in memory. Phase 1 must precede Phase 3.

---

## EnrichmentProvenance (existing)

`{ source: String, data_type: String }`. No change. `source` carries the
declaring manifest path; `data_type` keeps its existing discriminator
(`package-database-depends`).

Rejected: a new `Vec<DeclaringSource>` for edges declared in multiple
manifests. No such case has been measured, and `String` holds a path
today. Revisit only if FR-014 surfaces one.

---

## Stranded component (a state, not a type)

A component with no incoming relationship after FR-001 removes unbacked
edges. Not a new type — it is what the existing orphan machinery already
detects once the invented edges stop masking it (research R4).

Measured instance: `go-cobra` strands `blackfriday` and `check.v1`.

FR-007a requires three states to stay distinguishable, which today
collapse into one:

| State | Meaning | Must not be confused with |
|---|---|---|
| parent unknown | resolution failed; nothing in the tree says where it belongs | a leaf |
| genuinely no dependencies | a real leaf; its manifest declares nothing | parent unknown |
| declared by a weaker tier | edge exists and is backed, but derived by a lower-fidelity resolution step | either of the above |

The third is what `waybill:resolver-step` (C48) and
`waybill:go-transitive-source` (C108) already record. The first is what
this feature must add without overloading those.

---

## Completeness verdict (existing — `GraphCompletenessResult`)

No shape change. `Complete` iff `reason_codes.is_empty() &&
orphan_count == 0` (`graph_completeness/mod.rs:315`).

What changes is the inputs:

- `orphan_count` becomes non-zero for cold-cache Go scans, because the
  stranded components are genuinely unreachable — so the existing
  `OrphanedComponentsDetected` classifier fires unaided
- FR-003 adds an independent check on the per-ecosystem coverage signal,
  which does **not** depend on graph shape and therefore still holds if
  a future fallback attaches to something real but wrong

`classify_transitive_edges_unresolvable` is left alone. It keys on
`design`/`analyzed` tiers and every go.sum-fallback component is
`source` tier, so it never fires for this case; making it fire would
duplicate what orphan detection now does correctly.

---

## Edge provenance carriers (per format)

From the Principle V audit in research R3.

| Format | Carrier | Native? |
|---|---|---|
| SPDX 2.3 | `relationships[].comment` — field already exists at `generate/spdx/relationships.rs:79`, currently `None` at every construction site | **yes** |
| SPDX 3.0.1 | `Relationship.comment` (inherited from `Element`) | **yes** |
| CycloneDX 1.6 | new parity-bridging `waybill:*` property; `dependencies[]` has no `properties`, no `evidence`, and is not an annotation subject | **no — bridge** |

Structured content inside an SPDX comment follows the existing m071
`MikebomAnnotationCommentV1` envelope rather than inventing a format.

**Catalogue obligation**: the CDX bridge needs a row in
`docs/reference/sbom-format-mapping.md` with a justification naming the
missing native field, **and** a matching entry in
`parity/extractors/mod.rs::EXTRACTORS` in the same change — otherwise
`every_catalog_row_has_an_extractor` and `holistic_parity` both fail.

---

## Validation rules

| Rule | Source | Enforced at |
|---|---|---|
| No emitted relationship without a `DeclaredRequirement` | FR-001 | emission + FR-009 build gate |
| `replace`-redirected requirements back edges; `// indirect` ones do not produce direct edges | FR-002 | index construction |
| `go.sum` never backs an edge | FR-002 | index construction |
| Every relationship carries locatable provenance | FR-002a | emission + SC-003a |
| Stranded components stay in inventory, unreachable | FR-007 | emission; SC-005 checks count before/after |
| No synthesised root edge replaces a removed one | FR-007 | SC-005 |
| `complete` never co-occurs with coverage `unknown` | FR-003 | verdict + FR-009 build gate |
| A fully-resolved graph can still earn `complete` | FR-006 | warm-cache control, SC-004 |
