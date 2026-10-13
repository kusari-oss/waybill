# Implementation Plan: Close-time facts for compiler read and write sets

**Branch**: `1144-compiler-set-close-time` | **Date**: 2026-10-12 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1072-compiler-set-close-time/spec.md`

> **Numbering**: the spec directory is `1072-` (milestone sequence); the
> branch is `1144-` (issue number). The repo uses two number spaces and
> `create-new-feature.sh` conflates them, so `setup-plan.sh` must be run as
> `SPECIFY_FEATURE=1072-compiler-set-close-time .specify/scripts/bash/setup-plan.sh --json`
> or it creates a stray `specs/1144-…` directory.

## Summary

Issue #1144: the compiler read/write sets carry two fields that have never
held a true value — every `sha256` is 64 zeros, and every
`survived_trace_window` is `true`, including rustc temporaries that are
deleted before the build ends. A false statement in a signed attestation.

The fix is not to compute the missing hashes into the existing fields. The
kernel cannot hash (all probes are entry kprobes; the verifier forbids
unbounded loops), so close-time is the only moment a hash can exist — and
for a **read** set, a close-time hash is a different fact from the bytes the
compiler consumed. Writing it into a field named `sha256` would trade an
obvious lie for a subtle one.

So each entry gains two structurally distinct tiers, following SBOMit's
separation of verifiable in-toto content from its unverified mutation layer:
an **observation tier** (path, kind, rename chain — checkable against the
same attestation's raw `file_access` events) and a **close-time tier**
(`existed_at_close`, `sha256_at_close` — separately named so a verifier
cannot mistake one for the other). A budgeted userspace pass between
aggregator finalize and attestation build fills the close-time tier, hashing
each **distinct** path once and disclosing its coverage in
`trace_integrity`.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml`. `waybill-ebpf` keeps its dated nightly pin and is
**untouched** — no kernel-side change in this milestone.
**Primary Dependencies**: existing only — `sha2` via the established
`waybill-cli/src/trace/hasher.rs`, `serde`/`serde_json`, `tracing`,
`anyhow`, `clap` (one new `Args`-derive flag). Dev: `tempfile` (already a
dev-dep). **Zero new Cargo dependencies** (FR-020).
**Storage**: N/A — the pass is a single in-process traversal over the
finalized `CompilerPipelineData`; nothing persists beyond the emitted
attestation.
**Testing**: `cargo +stable test --workspace --no-fail-fast` for the pass
and wire shapes (unprivileged, no eBPF — see R6); the privileged container
harness `scripts/ebpf-integration-test.sh` under `Dockerfile.ebpf-test` for
SC-001 – SC-006, which is where m210/m212/m213 put their kernel assertions
and what Principle VII requires (the feature-on `cargo test` lane runs
unprivileged, so a cfg-only gate is not a privilege check).
**Target Platform**: the pass compiles on every host; its call site is Linux
+ `--features ebpf-tracing`, off by default everywhere.
**Project Type**: single Rust workspace, three crates (Principle VI).
**Performance Goals**: the close-time pass stays within **5% of total trace
wall-clock** — SC-008's ceiling, committed before the measurement so the
criterion can fail. The *baseline* it is a ratio against is what T034/T035
establish. The **byte** budget's default constant is the separate number
that may not be quoted before measurement (`contracts/cli-budget.md`
C-1072-9); no absolute byte figure may enter this plan.
**Constraints**: byte-identical pre-signature payloads across re-runs of a
reproducible build (feature 006 US2) — this is why the budget is in
cumulative bytes rather than wall-clock; exit zero on truncation with
structured disclosure; no false statement in a signed attestation.
**Scale/Scope**: 7 files, ~6 entities, 2 breaking field removals and 1
rename inside a feature-gated optional sub-object. On the measured fixture,
412-ish distinct paths across 7 rustc invocations.

## Constitution Check

*GATE: evaluated before Phase 0 and re-evaluated after Phase 1 design.*

| Principle | Verdict | Reasoning |
|-----------|---------|-----------|
| **I. Pure Rust, static linkage** | PASS | No new deps, no C, no new linkage. |
| **II. eBPF-Only Observation** | PASS, and strengthened | II governs *discovery*; close-time hashing adds an attribute to an already-observed path and discovers nothing. C-1072-7 forbids the pass from adding, removing or reordering an entry, so a path no eBPF event witnessed can never reach a set. The two-tier split is II's "observed vs enriched" distinction made structural inside the predicate. |
| **III. Fail Closed** | NOT ENGAGED | III's triggers are the trace failing to attach, losing events, or observing zero dependency activity. A userspace post-pass budget causes none of them, and truncation loses no observation — every path, kind and rename chain is still emitted. Principle X governs instead. See the note below. |
| **IV. Type-Driven Correctness** | PASS | `Option<ContentHash>` (existing newtype) for the hash; `BudgetSource` as a two-variant enum rather than a bool; no `.unwrap()` in production; test modules carry `#[cfg_attr(test, allow(clippy::unwrap_used))]`. |
| **V. Specification Compliance** | PASS | `predicateType` stays `build-trace/v1` (FR-018); the pre-stable witness attestor bumps to `v0.2` (FR-017). No SBOM format shape changes — C130 stays dormant per FR-019. |
| **VI. Three-Crate Architecture** | PASS | Changes land in `waybill-common` (wire types) and `waybill-cli` (the pass). No new crate. |
| **VII. Test Isolation** | PASS, and improved | R6's decision to leave the pass module un-gated means FR-001 – FR-016 are unit-testable unprivileged in the default CI lane, with tempdirs rather than a mock filesystem. eBPF-dependent assertions stay in the gated lane. |
| **VIII. Completeness** | PASS | No entry is dropped. Truncation reduces *hash* coverage only, and discloses it. |
| **IX. Accuracy** | PASS — the point of the feature | IX's letter is about phantom components; a field asserting a hash that identifies nothing, and a `survived` flag that is hardcoded `true`, are the same class of harm at field level. This milestone removes both. |
| **X. Transparency** | PASS | `compiler_hash_coverage` is the structured disclosure: four distinct-path counts, a `truncated` flag and the budget source, with the partition invariant enforced. |
| **XI / XII. Enrichment** | N/A | No external data source; no network. |
| **Boundary 4 — no `.unwrap()` in production** | PASS | The pass propagates `io::Error` into `paths_skipped` plus a WARN; it never unwraps. |

**On Principle III.** The spec's clarification said fail-closed was
"considered and declined", which overstates the tension. Read literally,
III is scoped to trace capture failures, and none of its three triggers can
be caused by a hashing budget. Recording this precisely matters: a future
reader should not believe this milestone carved an exception out of III,
because it did not. The fail-open behaviour needs no justification under
III — it needs X's disclosure, which it has.

**Gate result: PASS. No violations to justify, so Complexity Tracking is
empty.**

## Project Structure

### Documentation (this feature)

```text
specs/1072-compiler-set-close-time/
├── spec.md                       # approved 2026-10-12
├── plan.md                       # this file
├── research.md                   # Phase 0 — R1..R10, two spec corrections
├── data-model.md                 # Phase 1 — E1..E6
├── contracts/
│   ├── read-write-set-shape.md   # wire contract; supersedes m210 C-1/C-2
│   └── cli-budget.md             # flag, default, exhaustion behaviour
├── quickstart.md                 # Phase 1 — dev loop + verification commands
├── measurements/
│   ├── probe.sh                  # probes 1-4, committed, reproducible
│   └── findings.md               # Q1 write sets, Q2 mechanisms, Q3 read sets
└── tasks.md                      # NOT created here — /speckit.tasks
```

### Source Code (repository root)

```text
waybill-common/src/attestation/
├── compiler_pipeline.rs    E1 ReadSetEntry, E2 WriteSetEntry, attestor const :216
├── integrity.rs            E3 CompilerHashCoverage, E4 TraceIntegrity field
└── witness.rs              attestor const :57 (the duplicate — R8)

waybill-cli/src/trace/
├── close_time.rs           NEW — E6 the pass, un-gated, tempdir unit tests
├── compiler_pipeline.rs    E5 in-flight maps, rename accumulation, stdin sentinel
└── hasher.rs               unchanged — reused as-is (FR: no new primitive)

waybill-cli/src/cli/scan.rs            call site (770..777), two `let mut`, the flag
waybill-cli/src/cli/run.rs             `ScanArgs` literal (:361) — flag forwarding
waybill-cli/src/generate/compiler_pipeline_annotation.rs   C130 payload (:129)

scripts/ebpf-integration-test.sh       the `m1072` kernel-assertion section
```

**Structure Decision**: no new module tree. The pass is one new file in the
existing `waybill-cli/src/trace/` module beside `hasher.rs`, which it calls,
and `compiler_pipeline.rs`, whose output it consumes. The wire types stay
where they already are, so the three-crate split is untouched.

The one structural decision worth naming: **`close_time.rs` is not
cfg-gated.** It contains no eBPF, so gating it would make the bulk of this
feature invisible to a macOS `cargo check` and to the default CI lane — the
trap recorded in memory as the eBPF container test gap. Only the call site
in `scan.rs` stays gated. (Research R6.)

## Phase 0 — research

Complete: [research.md](./research.md). Ten findings, all read out of the
tree with `file:line`. Two contradicted the spec and are already corrected
in it:

1. **R2** — `sha256_file_hex` *refuses* files over `max_bytes`
   (`Err(InvalidData)`); it does not hash to the cap, and the existing
   `file_access` pass logs at debug and skips. The spec's edge case asserted
   the opposite of both halves. An over-cap path is now specified as
   skipped, counted in `paths_skipped`. Committed.
2. **R4** — `linkat` is not traced anywhere, so FR-009a (a hard link is not
   a rename) holds by construction. The plan carries a regression test and
   names the single place a future link probe must honour, instead of a
   guard against an event that cannot arrive. This also gives the proximate
   mechanical cause for the `release/<bin>` gap in findings Q2, which that
   file explained only via cargo's timing.

The one deliberately unresolved item, **R10**: the default budget number.
Not a clarification to chase — `contracts/cli-budget.md` C-1072-9 forbids an
unmeasured byte figure, so
`Budget` is a required parameter until the measurement task lands, and the
constant's definition site must cite `measurements/`.

## Phase 1 — design

Complete: [data-model.md](./data-model.md),
[contracts/](./contracts/), [quickstart.md](./quickstart.md).

Three design decisions beyond the spec's clarifications:

**The observation tier loses the ability to hold a hash.** E5 changes the
aggregator's in-flight maps from
`HashMap<PathBuf, (ContentHash, ReadKind)>` to `HashMap<PathBuf, ReadKind>`
and from `HashMap<PathBuf, (Option<ContentHash>, bool)>` to
`HashMap<PathBuf, PendingWrite>`. After this, the kernel's zero hash cannot
be threaded into either set, because neither map has a field for it. The
tier separation stops being a convention and becomes a type-level fact.

**`<stdin>` is never stat'd** (C-1072-2a). It is a marker, not a location;
stat'ing it would resolve against the working directory, so a build
containing a file named `<stdin>` would report the stdin entry as having
survived — the same false-statement class the milestone removes. It gets
`existed_at_close: false` directly and contributes to none of the coverage
counts.

**Hash and stat once per distinct path**, fanned out to every entry naming
it (FR-011, measured in findings Q3: one `.rlib` is read under three
distinct rustc invocations, and 213 paths in the fixture trace have more
than one reader). Per-entry charging would bill the same bytes three times
against the budget and would let two entries disagree about one path's
close-time state.

### Post-design Constitution re-check

Re-evaluated after the design above: **still PASS, with two verdicts
improved by it.** R6's un-gated module turns Principle VII from "satisfied
by gating integration tests" into "the majority of the feature is unit-
tested unprivileged in the default lane". C-1072-7's read-only rule turns
Principle II from a passive PASS into an enforced invariant with a test.
No new violations; Complexity Tracking stays empty.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified.

None. No gate failed and no principle needed a justification.

## What /speckit.tasks must sequence first

The measurement, before any default constant exists:

> Trace `two_binaries_diverge` on a real kernel with the pass present and a
> budget high enough not to truncate; record total trace wall-clock, pass
> wall-clock, bytes hashed and distinct-path count under `measurements/`.
> The default budget constant derives from that file, and so does the
> baseline SC-008's 5% is measured against — the 5% itself is fixed in
> `spec.md` ahead of the measurement.

Then: wire shapes (which break compilation at every consumer, usefully
enumerating the work) → aggregator maps and rename accumulation → the pass
with its tempdir tests → call site, flag, C130 payload → the two attestor
constants and their two tests.
