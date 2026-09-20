# Quickstart — verifying #927

Each section maps to a success criterion and states what the answer is
**today**, so a reader can tell a real pass from a vacuous one.

## 0. See the cost first

```sh
# today's default — per-component
time waybill sbom scan --path <repo-with-~2000-packages> --output /tmp/a.cdx.json
# ~1302s on the reference repository

# the path this feature makes default
time waybill sbom scan --path <same> --enrich-batch --enrich-no-cache --output /tmp/b.cdx.json
# ~8.5s
```

**Use `--enrich-no-cache` on the second run.** The first populates a cache;
without it the comparison flatters the batched path for the wrong reason.

## 1. SC-002 — the two paths agree

```sh
jq -S '[.components[] | {purl, licenses}] | sort' /tmp/a.cdx.json > /tmp/a.norm
jq -S '[.components[] | {purl, licenses}] | sort' /tmp/b.cdx.json > /tmp/b.norm
diff /tmp/a.norm /tmp/b.norm && echo "equivalent"
```

Measured on the reference repository: 2291 vs 2291 purls, 2283 vs 2283
licences, 2331 vs 2331 edges — **zero** differences. This is the check that
licenses the whole feature; if it ever fails, the faster path is not a valid
default regardless of its speed.

## 2. SC-001 — the default is fast

```sh
time waybill sbom scan --path <repo> --output /tmp/c.cdx.json     # no flags
```

Seconds, not minutes. Today this is the 1302s path.

## 3. SC-004 / C-8 — disabled enrichment is untouched

```sh
waybill sbom scan --path <repo> --offline --output /tmp/off-after.cdx.json
diff /tmp/off-before.cdx.json /tmp/off-after.cdx.json && echo "byte-identical"
```

Capture the "before" with the pre-change binary. The default flip must not
reach a scan that does no enrichment.

## 4. SC-005a / C-6 — one wasted attempt, not one per chunk

Point the client at a failing endpoint and count batch requests.

```
expected: 1 batch attempt, then the per-component path for everything
today:    one attempt per chunk — ~23 on a 2,000-package repository
```

**This is the test that would silently pass if the breaker were never wired
in**, because content is unaffected either way. Count attempts, not output.

## 5. SC-005b / C-7 — the operator can see it

```sh
waybill sbom scan --path <repo> 2>&1 | grep -i "batch"
```

A line naming the failure and saying the batched path was abandoned. Then
confirm the document also records it:

```sh
jq -r '.metadata.properties[] | select(.name=="waybill:enrichment-degraded")' /tmp/c.cdx.json
```

Both, because the operator watching a slow scan and the consumer reading the
document a week later are different people.

## 6. SC-007 / C-4 — the old flag still works

```sh
waybill sbom scan --path <repo> --enrich-batch --output /tmp/d.cdx.json && echo "accepted"
```

Must succeed, not error on an unknown argument.

## 7. SC-006 / C-3 — both paths are exercised

```sh
cargo +stable test --workspace -- deps_dev
```

Both the batched and per-component paths covered. The per-component path is
the fallback the default rests on; if only the fast path is tested, the safety
argument is untested.

## 8. C-9 — the `v3alpha` watch exists

The standing check is present and fires when a non-alpha batch endpoint
appears. Nothing to observe day to day, which is exactly why it needs to
exist — the premise this feature accepts expires silently otherwise.

## What "done" looks like

- A default scan enriches in seconds, with identical output to the slow path.
- A persistent upstream failure costs one wasted attempt and says so, twice.
- Enrichment-disabled scans are byte-identical.
- Both paths tested; the old flag still accepted.
- Something will tell us when `v3alpha` stops being alpha.
