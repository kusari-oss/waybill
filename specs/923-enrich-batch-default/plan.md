# Implementation Plan: Enrichment is fast by default

**Branch**: `923-enrich-batch-default` | **Date**: 2026-09-20 | **Spec**: [spec.md](./spec.md)
**Issue**: [#927](https://github.com/kusari-oss/waybill/issues/927)

## Summary

Enrichment is the slowest thing waybill does by two orders of magnitude, and
the fast path already exists behind an opt-in flag. Measured: 1302s
per-component against 8.5s batched, for output with zero differing package
identities, zero differing licence values and an identical edge count.

The work is a default flip plus the guards that make it defensible: a circuit
breaker so a persistent upstream failure costs one wasted attempt, a log line
so an operator can see the fast path stopped working, and a standing check for
the day the `v3alpha` premise expires.

Phase 0 changed one thing materially.

**The concurrency the batch loop appears to have does not exist.** Chunks are
grouped by `CONCURRENT_REQUESTS` and then awaited one at a time; there is no
`join_all`, `FuturesUnordered` or `spawn` in the module. Timing agrees — ~0.27s
per chunk across 23 chunks is one round-trip each. That makes the circuit
breaker *exact* (one wasted attempt, not one group's worth), and it retires a
clarification that had weakened the guarantee to accommodate concurrency that
was never there. Filed separately as **#929**; deliberately not fixed here.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain inherited. No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `clap` (flag surface), `tracing` (the log line). **Zero new Cargo dependencies.**
**Storage**: unchanged. The batched path populates and reads the same enrichment cache.
**Testing**: `cargo +stable test --workspace`, extending the existing `MockServer`-based enrichment tests, which are already parameterised on the path.
**Target Platform**: unchanged.
**Project Type**: CLI default change plus a failure-path guard.
**Performance Goals**: the point of the feature — enrichment in seconds rather than minutes on a repository of ~2,000 packages.
**Constraints**: output equivalence is non-negotiable (FR-002); enrichment-disabled scans must be byte-identical (FR-008/009); the existing opt-in flag must keep working (FR-004).
**Scale/Scope**: one flag inversion, one circuit breaker, one log line, one standing check, and tests for both paths.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | No dependencies added. **Pass.** |
| **II. eBPF-Only Observation** | Not engaged. **N/A.** |
| **III. Fail Closed** | Engaged and satisfied in the shape the principle intends: a batch failure degrades to a slower path that produces the *same* content, and the degradation is recorded rather than swallowed. The scan does not proceed as though nothing happened, and it does not fail a scan that can still succeed. **Pass.** |
| **IV. Type-Driven Correctness** | Light engagement — the circuit-breaker state is a bool on an existing struct rather than a new domain type. Modelling it further would be ceremony. **Pass.** |
| **V. Standards-native first** | Not engaged: no new `waybill:*` field. The degradation record (C158) already exists and is unchanged. **N/A** — and worth stating, because a reflexive "audit performed" here would be noise. |
| **VI. Three-Crate Architecture** | No crate boundary moves. **Pass.** |
| **VII. Test Isolation** | Enrichment tests use a local `MockServer`; no network in the suite. **Pass.** |
| **VIII. Completeness** | **The principle this feature is downstream of.** A default that takes 22 minutes is a default operators turn off, and enrichment-off means no licences and no source provenance. The fast default is what makes completeness affordable. **Pass.** |
| **IX. Accuracy** | Protected by FR-002: the two paths must produce equivalent documents, asserted rather than assumed. If they ever diverge, the faster one is not a valid default. **Pass.** |
| **X. Transparency** | Directly served by FR-007/007b: an operator whose scan got slow learns why, in the log while it happens and in the document afterwards. **Pass.** |
| **XI / XII. Enrichment** | Engaged. The enrichment principles govern provenance and degradation reporting; this feature changes the request shape, not what is claimed about the data. **Pass.** |

**No violations. Complexity Tracking omitted.**

### One thing recorded rather than buried

This feature **accepts a risk rather than eliminating one**. `GetVersionBatch`
is on a surface its publisher documents as liable to change incompatibly, and
waiting does not make it stable. The position is that a fallback costing speed
rather than coverage is adequate mitigation — and FR-007c exists so that
position is re-examined when its premise changes, instead of being inherited
by people who never saw the trade.

## Project Structure

```text
specs/923-enrich-batch-default/
├── plan.md            # This file
├── spec.md            # 13 FRs, 11 SCs, 2 clarifications (one withdrawn on evidence)
├── research.md        # Phase 0 — R1..R6
├── data-model.md      # Phase 1
├── contracts/
│   └── enrichment-default.md
├── quickstart.md      # Phase 1 — verification per SC
└── checklists/requirements.md
```

```text
waybill-cli/src/
├── cli/scan_cmd.rs              # flag inversion; keep the old flag accepted
└── enrich/
    ├── depsdev_source.rs        # circuit breaker + log line; both-path tests
    └── deps_dev_batch.rs        # endpoint constant (the v3alpha pin)

.github/workflows/               # the standing v3alpha graduation check
docs/                            # the rationale FR-010 requires to survive
```

**Structure Decision**: the change is small and concentrated. The risk is not
in the amount of code but in two places where a plausible mistake is invisible
— an opt-out that silently does nothing, and a circuit breaker that never
trips — so both need tests that would fail if the code did nothing.

## Phase ordering

1. **The equivalence guard first.** FR-002 is what licenses the whole change;
   it should be asserted before the default moves, not after.
2. **The flag inversion.** Default flips, old flag stays accepted.
3. **The circuit breaker and its log line.** Exact one-attempt guarantee,
   available because execution is sequential (R2).
4. **The standing `v3alpha` check.**
5. **Documentation** carrying the rationale forward (FR-010).

## Post-design Constitution re-check

Re-evaluated after Phase 1. No new violations.

Principle V is the one worth noting for its *absence*: no new annotation, so
no audit. Several recent milestones have carried a Principle V section because
they introduced a `waybill:*` field; this one does not, and saying "audited,
nothing found" where nothing was proposed would devalue the rows where the
audit did real work.

Principle VIII is the substantive one. The argument for this feature is not
"faster is nicer" — it is that a 22-minute default is one operators disable,
and a disabled enrichment path means no licences and no provenance in the
document. Speed here buys completeness.
