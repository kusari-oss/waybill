# Implementation Plan: Bound the Go proxy-fetch tier

**Branch**: `1065-go-proxy-fetch-bounds` | **Date**: 2026-10-03 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1065-go-proxy-fetch-bounds/spec.md`

## Summary

Bound step 3 of the Go resolution ladder (proxy `.mod` fetch) two ways:

- **Per-entry circuit breaker** (R2): it trips after 16 consecutive network-level failures from an entry that has never answered, and then skips that entry for the rest of the scan.
- **Per-scan time budget** (R3): 60 s. It is derived from measurement, with a test-only override matching m771.

Skipped modules fall through to the existing go.sum fallback unchanged. Per scan, the outcome merges into C110/C111 using two new C111 codes (`proxy-unreachable` → `unknown`, `proxy-fetch-budget-exhausted` → `partial`). Scans where no bound trips are byte-identical.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml`. No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only: `reqwest` (blocking client, unchanged), `std::sync::{Arc, Mutex, atomic}`, `std::time::Instant`, `tracing`. Dev: existing `wiremock = "0.6"` and `tempfile`. **Zero new Cargo dependencies.**
**Storage**: N/A. All state is in-process per scan (data-model.md).
**Testing**: Unit tests in `graph_resolver.rs` / `proxy_fetch.rs` with `GraphResolverConfig` overrides and local servers (wiremock with a delay; closed port for connection-refused; a TCP accept-and-hang listener). Integration via the binary with `GOPROXY` at a closed local port (instant connection-refused). `./scripts/pre-pr.sh`. Corpus regression via CI.
**Target Platform**: Linux, macOS, Windows. No platform-specific code.
**Project Type**: CLI (`waybill-cli`).
**Performance Goals**:
- Dead proxy: the proxy step costs about one timeout regardless of N (SC-001/002, ratio ≤ 1.5× between 64 and 16 modules).
- Budget: ≤ 60 s + one total timeout (SC-003).
- Healthy path: no measurable change (an extra atomic check per fetch).
**Constraints**: FR-009 byte-identity when no bound trips. No new CLI flag (Q2). No credentials in any new output (R6).
**Scale/Scope**: 3 source files (`graph_resolver.rs`, `proxy_fetch.rs`, `legacy.rs`), 1 catalogue row text, CHANGELOG, about 3 test files.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Pure Rust | ✅ No new crates. |
| III. Fail Closed | ➖ Scoped to trace mode (#987 tracks scan-mode scoping). In scan mode this feature *adds* disclosure where today a dead proxy reports `complete`. |
| IV. Type-driven | ✅ Entry health and trip class are typed; reasons reuse `ErrorClass`. |
| VII. Test isolation | ✅ All tests use local servers or closed ports, never the live proxy. The live probes are measurement tooling, not tests. |
| VIII / IX. Completeness / Accuracy | ✅ No component lost (FR-005). Coverage stops claiming `complete` when the proxy was never asked. |
| X. Transparency | ✅ The point of FR-006: the cut short appears in C110/C111 in all formats. |
| XII.3 External source unavailability | ✅ Graceful degradation with transparency annotation, which is exactly the rule. |
| Measurement rule (CLAUDE.md) | ✅ Budget and threshold trace to `measurements/`. SCs are ratios against a harness baseline. |

No violations.

## Project Structure

### Documentation (this feature)

```text
specs/1065-go-proxy-fetch-bounds/
├── spec.md
├── plan.md            # this file
├── research.md        # R1–R7
├── data-model.md      # EntryHealth, ProxyFetchBudget, BoundOutcome
├── quickstart.md
├── contracts/coverage-reason.md
├── measurements/      # probes + raw results (committed per the measurement rule)
└── checklists/requirements.md
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/golang/
├── graph_resolver.rs   # config: proxy_fetch_budget; per-scan state on GraphResolver;
│                       # parallel_fetch: budget check before each request;
│                       # BoundOutcome accessor
├── proxy_fetch.rs      # fetch_module_mod: consult/update EntryHealth per chain entry;
│                       # skip a tripped entry as a network failure of its trip class;
│                       # entry_label() = scheme://host[:port]
└── legacy.rs           # after the workspace loop: merge BoundOutcome into
                        # signals.go_transitive_coverage; FR-008 warnings
docs/reference/sbom-format-mapping.md   # C111: two new codes
CHANGELOG.md
waybill-cli/tests/go_proxy_fetch_bounds.rs   # binary-level: closed-port GOPROXY → unknown + reason
```

**Structure Decision**: confined to the Go resolver. No emitter changes: C110/C111 already reach all three formats through `signals.go_transitive_coverage`.

## Delivery

One PR. US1 (breaker) and US2 (budget) share the state, the reporting path and the C111 change, so splitting them would mean touching the same lines twice. US3 is the invariant both must hold.

## Complexity Tracking

None.
