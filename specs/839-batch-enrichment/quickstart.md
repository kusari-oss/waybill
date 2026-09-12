# Quickstart: batched, observable enrichment

Feature: `839-batch-enrichment` · Issue #766

Operator-facing. What changed, how to use it, and how to measure whether
it helped.

---

## What changed

A scan with enrichment enabled no longer goes silent. Three things moved:

- **Progress**, on by default. Enrichment reports completed/total once
  the phase passes ten seconds, then at ten-second intervals. A phase
  that finishes faster prints nothing.
- **Concurrency**, on by default. The per-component path issues requests
  concurrently instead of one at a time.
- **Batching**, opt-in. Groups lookups into requests of up to 5000.

Enrichment content is unchanged. Same licences, same source links.

---

## Using it

```bash
# Default: concurrent, with progress. No flag needed.
waybill scan ./repo --output sbom.cdx.json

# Opt into batching.
waybill scan ./repo --output sbom.cdx.json --enrich-batch
```

Progress goes to **stderr**, the SBOM to `--output`, so redirecting one
does not disturb the other:

```bash
waybill scan ./repo --output sbom.cdx.json 2>progress.log
```

To see nothing at all, the existing log controls apply:

```bash
RUST_LOG=warn waybill scan ./repo --output sbom.cdx.json
```

---

## The cache

Enrichment results are cached under `~/.cache/waybill/deps-dev/`.

**It expires after an hour by default, and that is deliberate.** deps.dev
serves a pinned version record with `cache-control: max-age=3600` and
states it re-scans packages continuously — licence data gets corrected,
source links get added, and for Go the licence comes from a scanner whose
output moves when the scanner does. A cache that never expired would
freeze licence data at first-scan time and nothing would ever reveal it,
because deps.dev offers no `ETag` to check against.

Practically: a repeated scan within the hour is fast; a nightly CI job
will mostly miss. If that trade is wrong for you and you would rather
have speed than currency, say so explicitly:

```bash
# Accept entries up to a day old.
waybill scan ./repo --enrich-cache-max-age 86400
```

That decision is recorded in each entry it writes, so entries fetched
under a longer bound keep it and others are unaffected.

Cold measurement, and clearing:

```bash
waybill scan ./repo --enrich-no-cache      # bypass, read and write
rm -rf ~/.cache/waybill/deps-dev/          # clear
```

---

## Measuring the change

The baseline is the **sequential** path as it behaved before this work —
about 17 minutes for ~7,500 components. Compare against that, not against
the improved per-component path, or the batch gain will look far smaller
than it is.

```bash
rm -rf ~/.cache/waybill/deps-dev/
time waybill scan ./big-repo --output /tmp/a.json                  # concurrent
rm -rf ~/.cache/waybill/deps-dev/
time waybill scan ./big-repo --output /tmp/a.json --enrich-batch   # batched
```

Clear the cache between runs or the second one measures the cache, not
the change.

To confirm enrichment content is identical across paths rather than
assuming it:

```bash
jq '[.components[] | select(.licenses) ] | length' /tmp/a.json
jq '[.components[].externalReferences // [] | .[]] | length' /tmp/a.json
```

Both counts must match between the batch and per-component runs. A batch
path that silently dropped a page would show up here as a lower count and
nowhere else.

---

## When enrichment degrades

Enrichment never fails a scan. If deps.dev is unreachable, throttling, or
the `v3alpha` batch endpoint has changed — its own documentation warns it
"may change in incompatible ways from time to time" — the scan completes
and emits an SBOM with less enrichment.

Batch failure falls back to the concurrent per-component path, not to the
old sequential one, so a degraded run is slower than batching but not a
return to the seventeen-minute behaviour.

Under `--offline` no request of any kind is issued. Cached entries are
still read; expired ones are simply misses and those components go
unenriched.
