# Research: Batched, observable dependency enrichment

Feature: `839-batch-enrichment` · Spec: [spec.md](./spec.md) · Issue #766

Every finding below was measured or read from a primary source on
2026-09-12, not inferred. Where a claim was checked and turned out
false, the false version is recorded too — a research note that hides
its own corrections invites the next reader to re-derive them.

---

## R1 — The upstream record for a pinned version is mutable

**Decision**: Cache entries expire. The freshness bound is read from the
response's own `Cache-Control: max-age`, defaulting to one hour.

**Evidence**:

```
$ curl -sS -D- -o/dev/null \
    https://api.deps.dev/v3/systems/cargo/packages/serde/versions/1.0.197
cache-control: public, max-age=3600
```

No `ETag`. No `Last-Modified`. Change is therefore undetectable without
re-fetching — there is no conditional-request path to make revalidation
cheap.

The deps.dev FAQ states the service "re-scans each package at a constant
rate to catch any updates that might be missed", that data is fresh "to
within an hour or so" for common packages and **staler for quiescent
ones**, and that "there is no mechanism for users to trigger an update".
It also documents that Go licences are derived with the `licensecheck`
package — a scanner, so a pinned module's licence string can change when
the scanner changes, with no change to the module.

**Rejected**: permanent entries keyed on "a pinned version is immutable".
The artefact is immutable; the record about it is not. This was the
spec's original premise and it was wrong. Because deps.dev offers no
`ETag`, nothing would ever have surfaced the resulting staleness: licence
data would have frozen at first-scan time, per machine, and the SBOM
would have looked entirely normal.

**Rejected**: a 7-day TTL mirroring `clearly_defined_disk_cache.rs:58-61`.
One policy across both caches is attractive, but it serves data deps.dev
considers stale by roughly 168×, and unlike ClearlyDefined the upstream
here publishes an explicit bound we can simply obey.

**Cost accepted**: a one-hour bound gives a nightly CI job a ~0% cache
hit rate. SC-005 is unaffected — it tests an immediate repeat scan, well
inside the hour — but US3's practical value is now mostly the local dev
loop. FR-012b's extend flag exists for operators who choose otherwise.

---

## R2 — Batch endpoint shape, limits and pagination

**Decision**: `POST /v3alpha/versionbatch`, chunked at ~500 (well under
the 5000 ceiling — see FR-005a), paged until `nextPageToken` is empty,
matched by echoed request key.

**Evidence** (v3alpha API reference + live probe):

| Property | Value | Source |
|---|---|---|
| Max entries per batch | **5000**; more returns HTTP 400 | docs, explicit |
| Page request field | `pageToken` | docs |
| Page response field | `nextPageToken` | docs |
| Paging constraint | "All other request fields must be the same as in the initial request" | docs |
| Identity handle | `responses[].request.versionKey`, echoing the **uncanonicalized** request | docs + probe |
| Missing data | `responses[]` entry present, `version` key **absent** | probe |
| Cache-Control | `public, max-age=3600`, same as v3 | probe |

**Trap 1 — `nextPageToken` is `""`, not absent, on the last page.**

```
$ curl -sS -X POST .../v3alpha/versionbatch -d '{"requests":[…one…]}'
top-level keys: ['responses', 'nextPageToken']
nextPageToken repr: ''
```

Deserialised as `Option<String>` this is `Some("")`, so `if let Some(t)`
loops forever. FR-006's termination test MUST be non-empty, not present.
Contract C-2 pins this and a test asserts it.

**Trap 2 — matching must use the echoed request, not a canonical form.**
The docs say the echo is *uncanonicalized*. Rebuilding a key from our own
canonicalisation and matching on that will silently mismatch for any
ecosystem where deps.dev normalises differently (PyPI PEP 503, NuGet
lowercasing, Maven `group:artifact` — all documented as normalised).

**Probe of a nonexistent package** confirms US1 scenario 3 is satisfiable:
the entry comes back with its `request` echoed and no `version`, so
"enriched / not enriched / neither fails" is directly expressible.

---

## R3 — There is no field mask, so FR-016 is not achievable as written

**Decision**: Satisfy FR-016a by not *retaining* `advisoryKeys`; raise
FR-016's wording as a spec defect rather than silently reinterpreting it.

**Evidence**: The v3alpha reference's "Data parameters" section covers
purl and versionKey encoding — how to pass parameters — not field
selection. No `fields`/`mask` parameter is documented on `GetVersionBatch`
or `GetVersion`. The live probe returns the full record either way, and
v3alpha returns *more* than v3: `licenseDetails`, `purl`, `cooldown`,
`upstreamIdentifiers` on top of the v3 set.

FR-016 says waybill "MUST NOT request upstream fields it does not
consume". Against this API that is unachievable: the server decides what
it sends. The achievable and useful half is FR-016a — stop deserialising
and storing `advisoryKeys`, which today is parsed at
`deps_dev_client.rs:21` and referenced nowhere outside test fixtures.
This matters more once caching exists: it is the field most obviously
mutable after publication, so persisting it would create staleness risk
for data that reaches no output at all.

**Resolved** (clarify session 2026-09-12, Q6): FR-016 now reads "MUST NOT
retain or persist upstream fields it does not consume". The plan flagged
this rather than applying it, because changing a requirement is not a
plan's call; it was then decided and the spec amended.

---

## R4 — Concurrency mechanism

**Decision**: `chunks(N)` + `tokio::task::JoinSet`, N = 8, mirroring
`deps_dev_graph.rs:128-144`.

**Evidence**: `grep -rn 'Semaphore' waybill-cli/src/` returns **nothing** —
there is no semaphore anywhere in the tree. An earlier claim during
clarification that `graph_resolver.rs` runs a semaphore-bounded fetcher
was wrong on two counts: that module uses `std::thread` + `mpsc`
(`graph_resolver.rs:1028-1088`) and is not async at all.

The correct precedent is in the same module this feature touches:

```rust
// deps_dev_graph.rs:43
const CONCURRENT_REQUESTS: usize = 8;
// :135
for chunk in seed_coords.chunks(CONCURRENT_REQUESTS) {
    // :144
    while let Some(result) = set.join_next().await { … }
}
```

This is already the project's self-imposed ceiling for concurrent
deps.dev traffic, in an async context, against the same host. Reusing the
constant rather than introducing a second one means there is one number
to change if deps.dev ever objects.

**Rejected**: raising N to chase SC-001. deps.dev publishes **no** rate
limit and **no** documented 429 semantics anywhere in its API docs, so
there is no advertised allowance to tune against. FR-003b makes the
conservative ceiling normative.

---

## R5 — Cache location and format

**Decision**: `$HOME/.cache/waybill/deps-dev/`, sibling to
`clearly_defined_disk_cache.rs`'s `$HOME/.cache/waybill/clearly-defined/`
(`:10-11`).

The sibling cache is the working reference for the mechanics this feature
needs, and every one of them is already solved there: recording a
confirmed negative (`definition: null` for a 404, `:30`), corrupt-entry
handling (`:113`), a key-hash collision guard (`:131`), expiry-on-read
(`:138`), and best-effort writes that never fail a scan (`:149`). FR-013
and FR-014 are ports, not designs.

Per the spec's standing assumption, the on-disk shape should be one a
future project-operated cache could serve, since that is recorded as
anticipated future work — not built now, but not designed against either.

---

## R6 — Progress output surface

**Decision**: `tracing` at INFO, which lands on stderr.

**Evidence**: `main.rs:288-290` configures `tracing_subscriber::fmt()`
with `.with_writer(std::io::stderr)`. The SBOM goes to `--output`. US2
scenario 3 — "redirected output does not corrupt or interleave badly" —
is therefore satisfied by construction: the two streams are already
separate and no new writer is introduced.

A time-triggered emitter (FR-009/009a) needs a clock in the enrichment
loop, not a counter, and must keep emitting while work is in flight —
which with `JoinSet` means emitting from the join loop rather than after
it.

---

## R7 — Where this plugs in

`enrich_components(&source, &mut components)` at `depsdev_source.rs:273`
is the sequential loop: `for component in components.iter_mut()` with the
`.await` at `:295`. It is called from `scan_cmd.rs:3526`. The in-scan memo
cache at `:43` already dedupes repeat lookups within one scan and stays —
the disk cache sits behind it, not in place of it.

FR-015's flag is `--offline`; `--no-deps-dev`, `--no-deps-dev-license`
and `--no-deps-dev-graph` are the narrower opt-outs, all documented as
subordinate to `--offline` (`scan_cmd.rs:972-1056`).
