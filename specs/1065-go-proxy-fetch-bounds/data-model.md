# Data model: Bound the Go proxy-fetch tier

All state is in-process for one scan. Nothing is persisted.

## EntryHealth (one per `ProxyChain` entry, per scan)

| field | meaning |
|---|---|
| `responded` | set once any HTTP response (any status) arrives from this entry |
| `consecutive_network_failures` | count of `Connection`/`Timeout`/`Dns`/`Tls` failures since the last response; reset never needed (a response sets `responded` permanently) |
| `in_flight` | requests sent and not yet finished. While `!responded && failures > 0 && in_flight > 0`, new requests wait (half-open hold, research R2 correction) |
| `tripped` | set when `!responded && consecutive_network_failures >= fetch_concurrency` |
| `trip_class` | the failure class of the attempt that tripped it (for FR-008 / C111 detail) |
| `affected` | modules whose outcome came from this tripped entry: the failures that tripped it plus every later skip |

States: `unknown → reachable` (on any response; terminal) and `unknown → tripped` (on threshold; terminal).

## ProxyFetchBudget (one per scan)

| field | meaning |
|---|---|
| `started` | set on the first proxy request of the scan, not at resolver construction, so time spent in other steps is not charged |
| `budget` | 60 s default; test-only override `WAYBILL_GO_PROXY_FETCH_BUDGET_MS` |
| `exhausted` | set the first time a worker finds no time left before starting a request |
| `not_attempted` | modules a worker declined to start because the budget was exhausted |

## BoundOutcome (derived once per scan, after the workspace loop)

- `breaker: Vec<(entry_label, trip_class, affected)>`: one item per tripped entry, in chain order.
- `budget: Option<(budget, not_attempted)>`.

`entry_label` is `scheme://host[:port]` (R6).

## Mapping to the document (FR-006)

| outcome | C110 | C111 code |
|---|---|---|
| no bound tripped | unchanged | unchanged |
| breaker tripped | `unknown` | `proxy-unreachable` |
| budget exhausted only | `partial` (unless another reason already made it `unknown`) | `proxy-fetch-budget-exhausted` |
| both | `unknown` | both codes, breaker first |

Merged into the per-workspace aggregate with the existing `merge_coverage` precedence (Unknown > Partial > Complete).
