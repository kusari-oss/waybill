# Measurements for #853 (2026-10-03)

Host: macOS, residential network. waybill `815e6157`, debug build. Every number below
was observed; none is derived. Predictions built from them live in the spec
and are labelled as such.

## 1. Per-fetch latency, proxy.golang.org — `probe_latency.sh https://proxy.golang.org 30`

Raw: `latency_serial.raw.txt`. 30 samples per class, serial.

| class | HTTP | p50 | p90 | max |
|---|---|---:|---:|---:|
| module exists | 200 | 56 ms | 62 ms | 77 ms |
| path does not exist | 404 | 1293 ms | 1407 ms | 1606 ms |
| version does not exist | 404 | 1283 ms | 1928 ms | 2031 ms |

## 2. Negative cache and 16-way scaling — `probe_repeat_and_scale.sh`

Raw: `repeat_and_scale.txt`.

- **Negative cache:** the same missing module took 1.27 s the first time, then ~0.05 s on each of 4 repeats.
- **16-way, distinct missing modules:** N=16 → 1.4 s, N=64 → 5.9 s, N=256 → 21.9 s, i.e. ~1.4 s per wave of 16.

## 3. End to end — `probe_e2e.sh <waybill> <N> <GOPROXY> <label>`

Raw: `e2e.txt`. `go` is hidden from PATH, `GOMODCACHE` is empty, and the scan
runs with `--no-deps-dev --no-go-mod-why`, so proxy fetch is the only network
activity.

| GOPROXY | N | wall |
|---|---:|---:|
| (nothing to fetch) | 0 | 0.2 s |
| https://proxy.golang.org | 16 | 1.5 s |
| https://proxy.golang.org | 64 | 7.0 s |
| http://10.255.255.1 (unreachable; connect timeout) | 16 | 10.2 s |
| http://10.255.255.1 | 32 | 20.1 s |
| local server that accepts and never answers (`hang_server.py`) | 16 | 30.4 s |

**Coverage under the unreachable proxy (N=16):**
- `waybill:go-transitive-coverage = complete`
- `waybill:go-transitive-fallback-count = 16`
- all 16 modules have `waybill:go-transitive-source = go-sum-fallback`

So today the document reports complete coverage even though no fetch succeeded.

Re-run (Ctrl-C the server afterwards):

    python3 hang_server.py 18853 &
    ./probe_e2e.sh target/debug/waybill 16 http://127.0.0.1:18853 hanging-proxy
