# Feature Specification: Bound the Go proxy-fetch tier

**Feature Branch**: `1065-go-proxy-fetch-bounds`
**Created**: 2026-10-03
**Status**: Draft
**Input**: Issue #853, "Go proxy-fetch tier has no budget: doomed fetches scale unbounded with broken-module count", plus the research posted on it on 2026-10-03 (`measurements/`).

## Background (measured, not assumed)

When waybill resolves a Go project's transitive dependencies, one step asks a Go module proxy for each module's `go.mod` file. It asks 16 at a time and waits up to 10 s to connect and 30 s in total per request. All figures below are in `measurements/README.md`.

- **A missing module costs ~1.3 s, not ~29 ms.** proxy.golang.org takes p50 1.29 s to answer 404 for a module it has never seen. Repeats are ~50 ms because the proxy caches the miss. The ~29 ms in #850 was a warm rescan.
- **Missing modules cost ~1.4 s per batch of 16.** 64 such modules took 7.0 s end to end.
- **A dead or hanging proxy is far worse.** Every module waits out its own full timeout, because nothing notices the proxy is gone: 16 modules took 10.2 s against an unreachable proxy and 30.4 s against one that never answers.
- **Predictions, not measurements, for 500 modules:** ~45 s with a cold healthy proxy, ~5 minutes with an unreachable proxy, ~16 minutes with a hanging one.
- **The document hides it.** After a scan in which every fetch failed, it reports `waybill:go-transitive-coverage = complete`. Only `waybill:go-transitive-fallback-count` hints that the dependency graph came from go.sum's flat list and not from the modules' own `go.mod` files.

A missing module (HTTP 404/410) is the proxy answering a question about one module, and waybill has to ask to find out. A connection failure or timeout says the proxy itself is unusable, and that is already known after the first batch.

## Clarifications

### Session 2026-10-03

- Q: Which bounds does this feature ship: a proxy circuit breaker (stop using a proxy that fails at the network level), a time budget for the whole fetch step, or both? → A: Both. The breaker caps a dead or hanging proxy at about one batch regardless of module count. The budget caps the slow many-404 case. Neither alone covers both failure shapes.
- Q: Can an operator change the time budget? → A: No. The budget is a fixed default with no new flag. `--no-go-proxy-fetch` already gives operators a full opt-out, and adding flags waits on the CLI rethink (#1042).
- Q: What does `waybill:go-transitive-coverage` (C110) say when a bound trips? → A: Breaker trip → `unknown`; budget exhausted → `partial`. If both trip, `unknown` wins, matching C110's existing priority order (could-not-measure before ran-but-incomplete).
- Q: Do modules skipped by a bound get a per-component marker? → A: No. The document-level C110/C111 reason and count, plus the existing per-component `go-sum-fallback` source (C108), carry it. No new catalogue row; one can be added later without breaking anything.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A dead or hanging Go proxy does not stall the scan (Priority: P1)

An engineer scans a Go repository on a machine whose configured module proxy is unreachable or hangs, for example a corporate proxy off-VPN or a firewall that drops traffic. Today the scan stalls for minutes, in proportion to the number of modules, and finishes with no clue why it was slow. With this feature, waybill notices after the first batch that the proxy is unusable, stops asking it, and takes the rest of the graph from go.sum.

**Why this priority**: this is the only case that runs to minutes, and it is also the commonest misconfiguration: off-VPN laptops and CI runners behind egress firewalls.

**Independent Test**: scan a synthetic repository with 64 go.sum modules and `GOPROXY` pointed at an unreachable address. The proxy step finishes in about one connect timeout instead of four, and the document says why its graph is degraded.

**Acceptance Scenarios**:

1. **Given** a Go repository with 64 modules not in the local cache and an unreachable `GOPROXY`, **When** it is scanned, **Then** the proxy step takes no more than about one connect timeout (see SC-001) rather than one per batch of 16.
2. **Given** a proxy that accepts connections and never answers, **When** the same repository is scanned, **Then** the proxy step takes no more than about one total-request timeout.
3. **Given** a breaker trip in either case, **When** the document is read, **Then** every Go module is still present with edges from go.sum, and the document-level coverage says the proxy was unreachable and how many modules it covered.

---

### User Story 2 - Many missing modules have a known upper bound on time (Priority: P2)

An engineer scans a monorepo with hundreds of modules the proxy cannot serve, for example stale `replace` targets, private modules without credentials on this host, or renamed repositories. The proxy answers every request, slowly. With this feature the proxy step stops starting new requests at a known time limit and leaves the rest to go.sum, and the document says it did.

**Why this priority**: it is bounded already (proxy.golang.org answers in ~1.3 s), so the risk is a long scan rather than a stalled one. But nothing caps it, and it is invisible today.

**Independent Test**: scan a synthetic repository whose go.sum lists more missing modules than the budget can cover. The proxy step ends near the budget, and the document records how many modules were left to go.sum because time ran out.

**Acceptance Scenarios**:

1. **Given** more missing modules than fit in the budget, **When** scanned, **Then** no new proxy request starts once the budget is spent. The step ends within the budget plus at most one request timeout.
2. **Given** the budget ran out, **When** the document is read, **Then** coverage is not `complete`, and its reason names the budget and the number of modules not attempted.

---

### User Story 3 - Scans that never hit a limit are unchanged (Priority: P1)

A healthy proxy that serves everything, or a scan that finishes within the budget with no breaker trip, produces the same document as before. This includes scans whose fetches fail one by one with 404s.

**Why this priority**: the bounds must only ever take over from a failing tier, never alter one that works.

**Independent Test**: every public-corpus target and every in-repo golden is byte-identical before and after.

**Acceptance Scenarios**:

1. **Given** any scan that neither trips the breaker nor exhausts the budget, **When** scanned, **Then** the output is byte-identical to the output before this feature.

### Edge Cases

- **Intermittent proxy** (some requests succeed, some time out): any HTTP response, including 404, proves the proxy is reachable, so the breaker does not trip; the budget still applies.
- **Multi-entry `GOPROXY` chain**:
  - `|`-separated: a tripped entry is skipped and the next entry is still tried.
  - `,`-separated: Go already does not fall through on network errors, so a trip ends the fetch step for the remaining modules.
- **Multiple Go workspaces in one scan**: breaker state and budget are per scan, not per workspace. A proxy found dead in one workspace is not retried in the next.
- **Unchanged behavior:**
  - `GOPRIVATE` modules are never fetched.
  - `--offline`, `GOPROXY=off` and `--no-go-proxy-fetch` behave as today.
  - The bounds only govern fetches that would otherwise happen.
- **Requests already in flight** when a bound trips are allowed to finish within their own timeout. A result that arrives is used, not discarded.
- **HTTP 5xx responses** do not trip the breaker. The proxy answered, so the time cost is bounded by response latency, which the budget covers.

## Requirements *(mandatory)*

### Functional Requirements

**Circuit breaker:**

- **FR-001**: When every attempt in the first full batch against a proxy chain entry fails at the network level (connection refused or unreachable, timeout, DNS failure, TLS failure), waybill MUST stop sending requests to that entry for the rest of the scan. Any HTTP response from the entry, whatever its status, MUST keep it in use.
- **FR-002**: A tripped entry MUST be treated as if it had failed for every module not yet attempted, following the chain's existing fall-through rules (`|` falls through, `,` does not).

**Time budget:**

- **FR-003**: The proxy-fetch step MUST have a per-scan time budget. Once it is spent, no new proxy request may start. Requests already in flight MUST be allowed to finish within their own timeout.
- **FR-004**: The default budget MUST be derived from the measured per-batch cost in `measurements/`, not chosen by feel. The plan MUST record the derivation and the module count it covers on a cold healthy proxy.

**Outcome and reporting:**

- **FR-005**: Modules not fetched because of either bound MUST still be resolved by the go.sum fallback exactly as a failed fetch is today, and MUST appear in the document with their go.sum edges. No Go component may be lost.
- **FR-006**: When either bound trips:
  - `waybill:go-transitive-coverage` (C110) MUST be `unknown` after a breaker trip (waybill could not ask the proxy, as with `--offline`). It MUST be `partial` after budget exhaustion alone (waybill asked and ran out of time). If both trip, `unknown` wins;
  - `waybill:go-transitive-coverage-reason` (C111) MUST name the cause, by extending C111's closed-but-extensible code vocabulary with one code per bound;
  - the reason MUST state the number of modules the bound covered.
- **FR-006a**: Modules skipped by a bound MUST carry the same per-component annotations as a module whose fetch failed today (C108 `go-sum-fallback`). This feature MUST NOT add a per-component annotation.
- **FR-007**: The C111 reason MUST be identical across CycloneDX, SPDX 2.3 and SPDX 3 (existing parity row).
- **FR-008**: A scan that trips the breaker MUST emit one warning naming the proxy entry, the failure class and the number of modules handed to the go.sum fallback.
- **FR-009**: A scan in which no bound trips MUST produce byte-identical output to the output before this feature (US3).
- **FR-010**: This feature MUST NOT change how ordinary per-module failures (404/410, 4xx, 5xx) are reported when no bound trips. Today such scans report `complete` with a fallback count; that stays as it is (see Assumptions).

### Key Entities

- **Proxy chain entry health**: per entry, per scan. Its states are *unknown*, *reachable* (any HTTP response seen) and *tripped*.
- **Proxy-fetch budget**: per scan. It records the time spent and whether it was exhausted, and is shared across workspaces.
- **Bound outcome**: per scan, at most one record per bound: which bound tripped, at which entry (for the breaker), the failure class, and the module count left to the go.sum fallback. This record drives FR-006 and FR-008.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With an unreachable proxy, scanning 64 modules spends no more than 1.5× the time a 16-module scan spends in the proxy step. The baseline is measured with the `probe_e2e.sh` harness: today 64 modules is a predicted ~40 s against a measured 10.2 s for 16.
- **SC-002**: With a proxy that never answers, the same 64-vs-16 ratio holds against a measured 30.4 s baseline for 16.
- **SC-003**: With more missing modules than the default budget covers, the proxy step ends within the budget plus one total-request timeout.
- **SC-004**: In every scan where a bound trips, 100% of the Go modules in go.sum appear in the document, and the coverage reason names the bound and its module count in all three formats.
- **SC-005**: All public-corpus targets and in-repo goldens are byte-identical (no scan in either set trips a bound), so a CI corpus run shows `no semantic change` for every target.
- **SC-006**: The per-batch costs measured in `measurements/` are reproducible by re-running the committed probes. They are kept so the default can be re-derived if the proxy's behaviour changes.

## Assumptions

- **Scope:** only the proxy-fetch step changes. `go mod graph`, the module-cache walk, the go.sum fallback itself, and per-module 404 handling are unchanged.
- **Accepted inaccuracy left alone:** today a scan where every module 404s individually reports `complete` coverage. Correcting that is a separate decision about what `complete` means when topology came from go.sum. This feature only stops the *new* cut-short case from also hiding behind `complete`.
- **Breaker trip threshold:** the first full batch, matching the existing 16-way concurrency. The plan may adjust this from measurement (FR-004 applies by analogy).
- **No operator control of the budget** (clarification Q2). If one is needed later, it belongs in #1042's flag rethink.
- **Default budget:** chosen in the plan from measurement. The spec fixes the method, not the number, per the repository rule that numbers describing external behaviour must trace to observation.
- **Mechanism:** reuse the existing shared-budget pattern from the `go mod why` step (m771) rather than inventing one. This is a plan concern; noted so the plan looks there first.
