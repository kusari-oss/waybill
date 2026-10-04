# Research: Bound the Go proxy-fetch tier

Every number here is measured. Raw data and the probes that produced it are in
`measurements/`. Predictions are labelled.

## R1 — Where the cost comes from

**Measured** (`measurements/README.md` §1–3):

| case | cost per batch of 16 | 16 modules end to end |
|---|---:|---:|
| healthy proxy, module exists | 56 ms per fetch (p50) | — |
| healthy proxy, module missing (cold) | ~1.4 s | 1.5 s |
| healthy proxy, module missing (warm negative cache) | ~50 ms per fetch | — |
| unreachable proxy (connect timeout) | 10 s | 10.2 s |
| proxy accepts and never answers (total timeout) | 30 s | 30.4 s |

- **Where the timeouts come from:** `fetch_connect_timeout` = 10 s and `fetch_total_timeout` = 30 s (`graph_resolver.rs:570-571`). Concurrency is 16 (`fetch_concurrency`).
- **Why a dead proxy repeats the cost:** `parallel_fetch` (`graph_resolver.rs:1054`) is a 16-worker pool pulling from one queue, and each worker calls `fetch_module_mod` independently. Nothing records proxy health, so every module pays its own timeout.

**Decision:** two bounds, as clarified: a per-entry breaker (R2) and a per-scan budget (R3).

## R2 — Breaker trip rule

**Decision:**
- **Trip:** a chain entry trips when it has failed at the network level for `fetch_concurrency` (16) attempts in a row and has never returned an HTTP response in this scan. Network level means `ErrorClass::{Connection, Timeout, Dns, Tls}` (`proxy_fetch.rs:211`).
- **Reachable for good:** any HTTP response, whatever its status, marks the entry reachable for the rest of the scan, and a reachable entry never trips.
- **Check point:** each worker checks the entry's state before sending a request.

**Rationale:**
- **The threshold costs nothing extra.** The pool starts the first 16 requests at once, and they fail together at the timeout, so 16 is the number of failures waybill sees anyway. A lower threshold would not trip any earlier in wall time. A higher one would cost another full timeout per extra batch.
- **Resulting cost:** about one timeout regardless of N, which is SC-001/SC-002's ratio.
- **Under 16 modules** the breaker never trips. That costs at most one batch, the same bound.
- **Why 'never responded':** an intermittent proxy (some responses, some timeouts) is reachable, so tripping it would discard answers it can give. The budget covers it (spec edge case).

**Alternatives considered:**
- Trip on a failure *ratio*: needs a window and a tuning knob, and gains nothing here because the measured cases are all-or-nothing.
- Trip per module host: Go proxies are single hosts per entry, so per-entry is the right grain.

## R3 — Budget value

**Measured** (`measurements/healthy_real.txt`, probe 4):
- **Healthy proxy, large real set:** kubernetes v1.31.0 has 788 go.sum lines. Its 197 selected modules all went through the proxy step (empty cache, `go` hidden, isolated `HOME`), and the whole scan took 0.4–0.5 s.

**Decision:** the default budget is **60 s**.

**Derivation:**
- **Healthy-path headroom:** 60 s ≥ 100× the healthy cost of a 197-module real tree, so FR-009 (no trip on a healthy scan) holds by two orders of magnitude.
- **Cold missing modules:** at the measured ~1.4 s per batch of 16, 60 s covers ~43 batches ≈ **685 cold missing modules** before it trips.
- **Precedent:** same value and pattern as m771's `go mod why` budget (`mod_why.rs:519`, `BudgetTracker`), so the scan's two network-bound Go steps share one convention.

**Overshoot:** requests in flight at exhaustion finish within their own total timeout, so the worst case is 60 s + 30 s (SC-003).

**Test hook:** `WAYBILL_GO_PROXY_FETCH_BUDGET_MS`, integer milliseconds and test-only, exactly as `WAYBILL_GO_MOD_WHY_BUDGET_MS`. It is not documented in `--help` or the CLI reference. That is consistent with clarification Q2 (no operator control), as it was for m771. Without it, testing the budget would take a minute per test.

## R4 — Where the state lives

**Decision:** keep per-scan state in `GraphResolver`, which `legacy.rs:1853` builds once per scan and reuses for every workspace in the sequential loop at `legacy.rs:1915`:
- breaker state: one record per chain entry, keyed by the entry's index in `ProxyChain`;
- the budget's start time and exhaustion flag;
- skip counters.

The state sits behind `Arc` + atomics/`Mutex`, because the 16 fetch workers run on threads and `resolve` takes `&self`.

**Rationale:** spec edge case "per scan, not per workspace". A proxy found dead in workspace 1 is skipped immediately in workspace 2.

## R5 — Reporting

**Decision:**
- **Per-workspace coverage is untouched.** `compute_coverage` is unchanged, which keeps FR-009 byte-identity trivially.
- **Bound outcome merged once per scan.** After the workspace loop in `legacy.rs`, the scan-level bound outcome is merged into `signals.go_transitive_coverage` with the existing `merge_coverage` (Unknown > Partial > Complete; reasons joined with `; `):
  - breaker tripped → merge `Unknown("proxy-unreachable: …")`;
  - budget exhausted → merge `Partial("proxy-fetch-budget-exhausted: …")`.
- **Counts are per scan:**
  - **breaker:** modules whose outcome came from the tripped entry. That is the network-level failures that tripped it plus the modules skipped afterwards. All of them resolved from go.sum only because the proxy was unreachable.
  - **budget:** modules never attempted.
- **New log lines (FR-008):** one `warn!` per tripped entry, and one for budget exhaustion. Both go out at trip time.

**Rationale:**
- Merging at scan level gives one count per bound, which the spec requires ("the number of modules the bound covered"), instead of one fragment per workspace.
- `merge_coverage` already orders `Unknown` before `Partial`, which is exactly clarification Q1's rule.

## R6 — Naming a proxy entry without leaking credentials

**Found while planning:**
- `GOPROXY` URLs may carry userinfo (`https://user:token@proxy.corp/`).
- The existing per-module log line (`proxy_fetch.rs`, `format!("{target} from {url}: …")`) formats the `Url` with `Display`, which includes the userinfo.

**Decision:**
- The new warning and the C111 detail name an entry as `scheme://host[:port]` only: no userinfo, path or query.
- The existing log line is out of scope (FR-010 keeps ordinary per-module failure reporting unchanged). It is filed as #1110, because it concerns logs, not SBOM output.

## R7 — Blast radius

- **Corpus and in-repo goldens:** no target uses a dead proxy or exceeds 60 s in the proxy step. CI corpus scans run with real network, and the Go targets resolve mostly via step 1/2. Expected `no semantic change` for every target (SC-005).
- **Catalogue:** C111's row gains two codes. No new row, no parity-catalogue change (C111 is already `SymmetricEqual`).
- **Code:** `graph_resolver.rs` (state, pool checks, outcome), `proxy_fetch.rs` (entry-level skip before sending), `legacy.rs` (scan-level merge). No emitter changes: C110/C111 already flow through all three formats.
