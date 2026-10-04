# Quickstart: verify the proxy-fetch bounds

Uses the committed probes in `measurements/`. Build first: `cargo build -p waybill`.
`B=target/debug/waybill`, `M=specs/1065-go-proxy-fetch-bounds/measurements`.

## 1. Breaker: unreachable proxy (US1, SC-001)

```sh
KEEP=/tmp/u64.cdx.json $M/probe_e2e.sh $B 64 http://10.255.255.1 unreachable
$M/probe_e2e.sh $B 16 http://10.255.255.1 unreachable
```

Expected:
- **Wall time:** 64 modules within 1.5× of 16 modules (~10 s each), down from a predicted ~40 s.
- **Document:**

```sh
jq -r '.metadata.properties[] | select(.name|test("go-transitive-coverage")) | "\(.name)=\(.value)"' /tmp/u64.cdx.json
# waybill:go-transitive-coverage=unknown
# waybill:go-transitive-coverage-reason=proxy-unreachable: http://10.255.255.1 failed at the network level (timeout); 64 modules resolved from go.sum only
```

## 2. Breaker: hanging proxy (SC-002)

```sh
python3 $M/hang_server.py 18853 &
$M/probe_e2e.sh $B 64 http://127.0.0.1:18853 hanging   # ~30 s, not ~120 s
kill %1
```

## 3. Budget (US2, SC-003)

```sh
WAYBILL_GO_PROXY_FETCH_BUDGET_MS=3000 KEEP=/tmp/b.cdx.json $M/probe_e2e.sh $B 256 https://proxy.golang.org budget
```

Expected:
- **Wall time:** ≤ 3 s + one request.
- **Document:** C110 is `partial` and C111 starts `proxy-fetch-budget-exhausted: 3s spent;`.

## 4. Healthy scans are unchanged (US3, SC-005)

```sh
$M/probe_healthy_real.sh $B v1.31.0
```

Expected: `failed-fetches=0` and `proxy-fetch=197`, with no C111 present. The CI corpus run shows `no semantic change` for every target.
