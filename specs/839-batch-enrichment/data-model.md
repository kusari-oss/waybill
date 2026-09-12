# Data Model: Batched, observable dependency enrichment

Feature: `839-batch-enrichment` · Spec: [spec.md](./spec.md)

All state is per-scan and in-process, with one exception: the disk cache,
which is the first persistent state this enrichment path has ever had.
That is the entity to read carefully.

---

## Enrichment request

The identity of one package version for which metadata is sought.

| Field | Description |
|---|---|
| system | deps.dev system enum — `CARGO`, `NPM`, `PYPI`, `MAVEN`, `NUGET`, `GO`, `RUBYGEMS` |
| name | Package name **as deps.dev expects it** for that ecosystem |
| version | Version string |

**Validation**
- Name normalisation is ecosystem-specific and deps.dev documents it:
  PyPI per PEP 503, NuGet lowercased, Maven as `group:artifact`. Getting
  this wrong yields a response entry with no `version` — indistinguishable
  from "package genuinely unknown", so a normalisation bug degrades
  silently into apparent absence of data.
- A request is the cache key. It must therefore be constructed identically
  on the batch path and the per-component path, or the two paths will
  populate and miss different cache entries for the same package.

---

## Batch page

One request/response round-trip against `POST /v3alpha/versionbatch`.

| Field | Description |
|---|---|
| requests | **100** (FR-005a) — the observed response page size. Hard service ceiling is **5000**; above that the service returns HTTP 400, but sizes between 100 and 5000 are worse than 100 because they paginate. |
| page_token | Empty on the first page; thereafter the previous response's `next_page_token` |
| responses | One entry per result, each echoing its originating request |
| next_page_token | **Empty string when exhausted — not absent** |

**Validation**
- `next_page_token` is the trap. The service returns `""` rather than
  omitting the field, so `Option<String>` deserialises to `Some("")` and
  a presence check loops forever. Termination MUST test emptiness.
- Paging requires every other request field to be unchanged from the
  initial request, per the API docs. A page-2 request that re-derives its
  body is a correctness bug even when it looks equivalent.
- Three numbers, easily conflated. **5000** is the service's hard limit.
  **100** is the observed page size and waybill's chosen batch size.
  Anything in between is the worst of both: accepted by the service, then
  silently split into serial pages. Measured, batch=500 is ~3× slower than
  batch=100 and batch=5000 ~4× slower, for identical coverage.
- The page size is **observed, not documented**. Nothing in the deps.dev
  API reference states it; it was found by probing (101 in → 100 out plus
  a token). It may change, which is why pagination stays implemented.
- Chunking must hold for inputs far larger than any currently tested — a
  repository is free to have 60,000 components.

---

## Enrichment record

The metadata retrieved for one package version.

| Field | Description |
|---|---|
| licenses | Licence expressions |
| links | Labelled URLs — `SOURCE_REPO`, `HOMEPAGE`, `DOCUMENTATION`, … |

**Validation**
- These two fields are the entire consumed surface. `advisoryKeys` is
  deserialised today and used nowhere; it is removed (FR-016a).
- The service returns considerably more than this — `licenseDetails`,
  `purl`, `cooldown`, `upstreamIdentifiers`, `attestations`,
  `slsaProvenances` — and offers no field mask, so the extra data arrives
  whether or not it is wanted. The obligation is to not *retain* it.
- Absence of a record is not an error. A response entry with its request
  echoed and no `version` means deps.dev has nothing; that component is
  left unenriched and the scan continues (US1 scenario 3).

---

## Cache entry

A stored enrichment record. **The only persistent state in this feature.**

| Field | Description |
|---|---|
| key | Enrichment request identity, hashed |
| record | The enrichment record, **or a recorded absence** |
| retrieved_at | When it was fetched |
| max_age | The freshness bound that applied at fetch time |

**Validation**
- **Not immutable.** The package version is pinned; deps.dev's record
  about it is re-derived continuously. An entry is usable only while
  `now - retrieved_at < max_age`.
- `max_age` is stored per entry rather than applied globally, because it
  comes from the response that produced the entry. If deps.dev changes
  its policy, old entries keep the bound they were fetched under and new
  ones pick up the new bound, with no migration.
- A recorded absence is cached like a record, and expires like one. This
  is what stops every scan re-requesting the long tail of packages
  deps.dev does not carry — on a large repository that tail is most of
  the request volume the feature exists to remove.
- Corrupt, truncated or unreadable entries are misses, never errors
  (FR-013). A cache that can fail a scan is worse than no cache.
- Writes are best-effort. A full disk degrades speed, not correctness.

**State transitions**

```
absent ──fetch──▶ fresh ──max_age elapses──▶ stale ──read──▶ treated as absent
                    │                                              │
                    └──────────── corrupt / unreadable ────────────┘
```

There is deliberately no "stale but serve anyway" edge. That edge is
FR-012b's extend flag, which changes `max_age` at fetch time rather than
overriding the check at read time — so an operator's choice to accept
staleness is recorded in the entry itself, not applied invisibly later.

---

## Progress report

Completed and total enrichment work at a point in time.

| Field | Description |
|---|---|
| completed | Components resolved so far, cache hits included |
| total | Components in the phase |
| elapsed | Time since the phase began |

**Validation**
- Emission is driven by `elapsed`, not by `completed` (FR-009). The
  defect being fixed is an operator watching silence; a small component
  count behind a slow endpoint produces the same silence as a large one.
- `total` is known before the phase starts, so progress can be reported
  as a fraction from the first line rather than as a bare running count.
- Zero enrichable components emits nothing at all (FR-010) — not a
  "0/0 complete" line, which is noise asserting that nothing happened.
