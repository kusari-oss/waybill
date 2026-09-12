# Enrichment performance

waybill enriches components with licence and source-provenance data
from deps.dev. On a large repository that used to dominate scan time —
and, worse, did it silently. This page covers what changed, how to use
it, and how to measure it honestly.

Every number here was measured on 709 enrichable components. Your
numbers will differ; the method is the transferable part.

---

## What changed

| | enrichment phase |
|---|---|
| one request per component, in sequence | 30,140 ms |
| concurrent (default since this change) | 3,524 ms |
| `--enrich-batch` | **766 ms** |
| second scan, warm disk cache | **9 ms** |

Enrichment content is identical in every row — same components, same
licences, same external references. A speed-up that enriched less
would be a regression wearing a disguise, so the comparison always
checks coverage alongside wall time.

---

## Using it

```bash
# Concurrent by default. No flag needed.
waybill sbom scan --path ./repo --output sbom.cdx.json

# Bulk lookups. Opt-in.
waybill sbom scan --path ./repo --output sbom.cdx.json --enrich-batch
```

Batching is opt-in because `GetVersionBatch` lives on deps.dev's
`v3alpha` surface, which its own documentation says "may change in
incompatible ways from time to time". If a batch request fails, waybill
falls back to the concurrent per-component path — a batch failure costs
speed, never the scan or its content.

## Progress

Enrichment reports progress once the phase passes ten seconds, then at
ten-second intervals:

```
INFO enriching dependencies from deps.dev completed=339 total=709 elapsed_secs=20
```

The trigger is **elapsed time, not component count**. A few hundred
components behind a slow endpoint produces exactly the same silence as
a few thousand, and silence is what makes a running scan look like a
hung one. A phase that finishes inside ten seconds prints nothing.

Progress goes to stderr; the SBOM goes to `--output`. Redirecting one
does not disturb the other.

---

## The cache

Results are cached under `~/.cache/waybill/deps-dev/`. A repeat scan
inside the freshness window issues **zero** network requests.

**Entries expire after an hour by default, and that is deliberate.**
deps.dev serves a pinned version record with `cache-control:
max-age=3600` and re-scans packages continuously — licences get
corrected, source links get added, and for Go the licence comes from a
scanner whose output moves when the scanner is updated. deps.dev offers
no `ETag`, so a stale entry cannot be detected, only re-fetched. A
cache that never expired would freeze licence data at first-scan time
and nothing would ever reveal it.

Each entry stores the bound it was written under, so a change in
upstream policy needs no migration.

```bash
# Accept entries up to a day old. Applies to entries written from now
# on; entries already stored under a shorter bound keep it.
waybill sbom scan --path ./repo --enrich-cache-max-age 86400

# Cold measurement: neither read nor write.
waybill sbom scan --path ./repo --enrich-no-cache

# Clear it.
rm -rf ~/.cache/waybill/deps-dev/
```

Under `--offline` the cache is still **read** — a local file is not a
network request — but nothing is fetched. An entry the cache lacks
simply goes unenriched. That makes a pre-warmed cache useful for
air-gapped scanning: populate it on a connected machine, copy the
directory across.

---

## When enrichment degrades

Enrichment never fails a scan. If deps.dev is unreachable, or the
`v3alpha` batch endpoint changes, the scan completes and emits an SBOM
with less enrichment.

It also **says so**. A degraded run carries a document-scope
annotation in all three formats:

```json
{"name": "waybill:enrichment-degraded",
 "value": "wholly-unavailable;unenriched=709"}
```

```bash
# CycloneDX
jq '.metadata.properties[]? | select(.name=="waybill:enrichment-degraded")' sbom.cdx.json
```

Without it, a degraded run and a clean one differ only in licence
counts — and nobody checks a licence count against an expected number.
Modes are `batch-unavailable` (fell back; speed only, `unenriched=0`),
`wholly-unavailable`, and every mode that occurred is listed, not just
the first.

---

## Measuring it yourself

Two things will mislead you if you skip them.

**Read the phase timer, do not difference whole-scan runs.** A scan
does other network work whose cost is not stable, and subtracting it
produces confident nonsense. waybill logs the phase directly:

```bash
RUST_LOG=info waybill sbom scan --path ./repo --output /dev/null 2>&1 \
  | grep 'licence enrichment complete'
# elapsed_ms=766 attempted=709 network_lookups=709 cache_hits=0 enriched=561
```

`network_lookups` is the honest measure of cache effectiveness. Wall
time cannot distinguish a cache that worked from a fast network.

**Control the cache state on both sides.** deps.dev serves a one-hour
edge cache, so the same scan can be ~10× faster on its second run with
no code change at all. Comparing a cold arm against a warm one
manufactures an improvement out of nothing. Clear the local cache
between arms, and run both arms in the same session:

```bash
rm -rf ~/.cache/waybill/deps-dev/
RUST_LOG=info waybill sbom scan --path ./repo --output /dev/null 2>&1 | grep 'enrichment complete'
rm -rf ~/.cache/waybill/deps-dev/
RUST_LOG=info waybill sbom scan --path ./repo --output /dev/null --enrich-batch 2>&1 | grep 'enrichment complete'
```

---

## Scope

These figures cover the **licence** enrichment path. deps.dev's
dependency-graph enrichment is a separate call that this work does not
change, and it is not batchable — deps.dev publishes no batch method
for dependencies. It costs several seconds and, once licence lookups
are fast, becomes the dominant remaining cost. Disable it with
`--no-deps-dev-graph` if you do not need transitive components deps.dev
contributes.
