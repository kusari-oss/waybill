# Implementation Plan: Kernel-side trace-noise filter for file_ops kprobes

**Branch**: `213-kernel-noise-filter` | **Date**: 2026-07-21 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/213-kernel-noise-filter/spec.md`

## Summary

Port `mikebom-cli/src/trace/compiler_pipeline.rs::classify_filter_category`
(userspace) *kernel-side* into `mikebom-ebpf/src/programs/file_ops.rs`. Add a
new `path_matches_filter_category(&path) -> Option<FilterCategoryTag>` helper
in the eBPF programs and short-circuit `try_do_filp_open` + `try_openat2`
BEFORE the `FILE_EVENTS.reserve()` call: when the classifier returns
`Some(cat)`, increment a per-CPU `FILTER_CATEGORY_HITS[cat]` counter and
return early — the event never enters the ring buffer, and the drop
counter (`FILE_EVENT_DROPS` from milestone 212) is also NOT touched
(intentional-drop ≠ overflow-drop). Userspace-side, extend the m212
`counters.rs` module to *additionally* read the 4-entry
`FILTER_CATEGORY_HITS` per-CPU map at trace-end and populate a new
`trace_integrity.filter_categories_applied[]` field with the sorted-
deduplicated set of category names whose count > 0.

Verifier-safety recipe reuses m211's fixed-size u64-array pattern (proven
on Colima aarch64 6.8): each filter pattern is stored as a
`[u8; 32]` fixed-size array + `len: u8`, and the compare loop is a
`#[inline(always)]` word-wide `u64` compare, not a byte loop. Total
new kernel-side instructions: ~800 per open path (estimated from the
compiler-exec whitelist compare pattern established in m210). Well
inside the verifier's 1M-instruction budget headroom.

The widening flag (`--include-system-reads` per FR-010) is already
threaded from `scan.rs::ScanArgs` into loader-time config. It becomes
a boolean written to a new 1-entry `PerCpuArray<u8>` config map
(`FILTER_WIDEN`) that the classifier reads at every open; when
`FILTER_WIDEN[0] == 1`, the `System` category compare is short-
circuited to `None`.

Two dedicated new fields on the wire: `FilterCategoryTag` enum
(u8 discriminant, 4 variants) in `mikebom-common` for kernel↔user
transport (via the new hits map), and `filter_categories_applied:
Vec<String>` on `TraceIntegrity` for the emitted attestation. Neither
change touches `FileEvent`'s on-wire shape (FR-005 hard requirement).

## Technical Context

**Language/Version**: Rust nightly (eBPF target via `aya-ebpf` 0.1.1) + Rust stable (workspace toolchain inherited from milestones 001–212; no new nightly features required beyond what m020 pins). No MSRV change.

**Primary Dependencies**: Existing only — `aya-ebpf = 0.1.1` (kernel-side; `PerCpuArray<u64>` writer already proven at `mikebom-ebpf/src/maps.rs:52-71` per m212 precedent; `bpf_probe_read_kernel_str_bytes` for path extraction already used at `mikebom-ebpf/src/programs/file_ops.rs:2`), `aya = 0.13` (user-space `PerCpuArray` reader — m212 pattern from `mikebom-cli/src/trace/counters.rs::read_percpu_sum`), `mikebom-common` for the new `FilterCategoryTag` enum (u8-repr, `Pod` + `Zeroable` via `bytemuck` — already a workspace dep for `FileEvent`), `serde`/`serde_json` (wire encoding for the new `filter_categories_applied` field), `tracing` (INFO log at trace-end summarizing filter hits per FR-006), `anyhow`/`thiserror` (error propagation). **Zero new Cargo dependencies at any layer.**

**Storage**: N/A — all filter-hit state lives in per-CPU u64 slots in-kernel; userspace reads them once at trace-end and drops the map handle. Nothing persists past a single trace invocation. Matches the m212 counter pattern verbatim.

**Testing**:
- Rust unit tests in `mikebom-cli/src/trace/counters.rs::tests` (SC-005) — extend the m212 `RingBufferDropsSummary` test suite with a parallel `FilterCategoryHitsSummary` test proving (a) `total()` sums correctly across the 4 categories, (b) the `applied_categories()` accessor emits sorted-deduplicated names, (c) empty hits produce `[]` not `null` (FR-009).
- Rust unit tests in `mikebom-common/src/attestation/integrity.rs::tests` (SC-005) — extend the existing `trace_integrity_serde_populated_counter_and_attach_failures` (m212) with a new case populating `filter_categories_applied` and asserting round-trip byte-identity via `serde_json::to_value` equality (m212 R4 pattern).
- Container integration test (SC-001 + SC-002 + SC-003) — extend `scripts/ebpf-integration-test.sh` (already asserts m210 compiler_pipeline + m212 ring_buffer_overflows > 100) with three new jq assertions: (a) `filter_categories_applied[]` is a JSON array containing `"CargoFingerprint"`; (b) `ring_buffer_overflows <= 10`; (c) at least one entry in `file_access.operations[]` has `comm == "rustc"`.
- Manual smoke test on `mikebom trace capture -- true` — asserts `filter_categories_applied[] == []` on a zero-syscall command (FR-009 counterpart).

**Target Platform**: Linux only (eBPF is Linux-native). Per m211 Clarification Q1's support matrix: Colima aarch64 Linux 6.8 (dev) + Ubuntu 22.04/24.04 amd64 Linux 6.5+ (CI). Kernel 5.15 LTS (SC-003 lower bound) — best-effort per verifier acceptance on that kernel; container harness runs each PR through the full 5.15/6.1/6.6/6.8 matrix.

**Project Type**: eBPF-instrumented CLI tool. Three-crate architecture (Constitution Principle VI) untouched — this milestone only touches:
- `mikebom-ebpf` (new classifier helper + new `PerCpuArray<u64>` hits map + new `PerCpuArray<u8>` widen-flag config map + short-circuit branches in 2 kprobes)
- `mikebom-cli` (new module extension in `trace/counters.rs` for reading the hits map + wiring `filter_categories_applied` into `TraceIntegrity` builder)
- `mikebom-common` (new `FilterCategoryTag` enum + one new `Vec<String>` field on `TraceIntegrity`)

**Performance Goals**:
- FR-013: kernel-side classifier per-open overhead < 5 µs on kernel 5.15+; the 4-category compare is O(prefix_length) with fixed-size loops the verifier proves bounded. Expected: 200–400 ns per open, well inside the observed background per-open latency.
- Userspace-side aggregation at trace end: one-time O(num_cpus × 4 categories) — for 4 categories on a 128-core host that's ~512 map lookups total, well under 5 ms.
- SC-001/SC-002 empirical target: `ring_buffer_overflows` on the SC-001 fixture drops from ≥100 (m212 baseline) to ≤10 (a 10× improvement), and rustc/linker events appear (baseline 0).

**Constraints**:
- **FR-005 wire byte-identity for `FileEvent`**: The filter drops events *before* `FILE_EVENTS.reserve()`; no `FileEvent` bytes change. On-wire compatibility with every downstream reader (aggregator, compiler-pipeline, m210+m211 patterns) is unaffected.
- **FR-013 verifier budget**: eBPF verifier's 1M-instruction budget on kernels 5.15/6.1/6.6/6.8. Reusing m211's word-wide u64 compare pattern keeps a 4-category × N-pattern loop compact (~800 insns estimated for 15 total patterns). Verified via `bpftool prog show` on Colima 6.8 before merge.
- **eBPF stack limit (512 bytes per program frame)**: The classifier reads the path from an existing `[u8; 256]` local (already allocated at `file_ops.rs:146`). No new stack allocations; the pattern arrays live in `.rodata` at `#[map]` scope. Total stack impact: 0 bytes.
- **Constitution Principle I** (Pure Rust): No C. New patterns arrays are Rust `const` values in the `mikebom-ebpf` `no_std` binary.
- **Constitution Principle II** (eBPF-Only Observation): CANONICAL — the noise filter operates ENTIRELY in eBPF-side observation code. No LD_PRELOAD, no ptrace, no static classifier. Enforced at architectural level (the classifier can't run anywhere else — the path bytes only exist kernel-side).
- **Constitution Principle IV** (Type-Driven Correctness): New `FilterCategoryTag` is a `#[repr(u8)]` enum with 4 variants (`System=0`, `UserCache=1`, `Ephemeral=2`, `CargoFingerprint=3`). Wire-transported as u8, deserialized via `TryFrom<u8>` at userspace boundary with error on unknown discriminants.
- **Constitution Principle V** (Specification Compliance — Standards-Native Fields Take Precedence): AUDITED. `trace_integrity` is a mikebom-native attestation predicate (not CDX/SPDX/PURL), so the standards-precedence rule doesn't apply to it — the `filter_categories_applied[]` field is a new bag on mikebom's own witness-schema-adjacent predicate. But: within `mikebom:` namespace we already established the m210 `filter_categories_applied` field on `compiler_pipeline`; this milestone reuses the **same field name** at the outer `trace_integrity` scope. No new vocabulary invented.

**Scale/Scope**:
- One new `PerCpuArray<u64>` map (4 entries — one slot per `FilterCategoryTag` variant) at `mikebom-ebpf/src/maps.rs` (~5 LOC).
- One new `PerCpuArray<u8>` config map (1 entry — the widen flag) at same location (~5 LOC).
- One new `path_matches_filter_category(&path) -> Option<FilterCategoryTag>` `#[inline(always)]` function in `mikebom-ebpf/src/programs/file_ops.rs` (~80 LOC including pattern arrays + widen check + word-wide compare).
- Two short-circuit branches in `try_do_filp_open` + `try_openat2` (~5 LOC each = 10 LOC total).
- One userspace helper `read_filter_category_hits(bpf: &mut aya::Ebpf) -> FilterCategoryHitsSummary` in `mikebom-cli/src/trace/counters.rs` (~40 LOC).
- One `#[repr(u8)]` enum + `TryFrom<u8>` impl in `mikebom-common/src/events.rs` or `mikebom-common/src/attestation.rs` (~30 LOC).
- One new `Vec<String>` field on `TraceIntegrity` + m212-pattern round-trip test extension (~40 LOC).
- Loader-time boolean write to the `FILTER_WIDEN` map from `mikebom-cli/src/trace/loader.rs` reading `ScanArgs.include_system_reads` (~15 LOC).
- Wiring at trace-end in `mikebom-cli/src/cli/scan.rs::execute_scan` — one call to `read_filter_category_hits` + populate `filter_categories_applied` on the emitted `TraceIntegrity` (~15 LOC).
- Container harness assertion extension in `scripts/ebpf-integration-test.sh` (~30 LOC — 3 new jq blocks per SC-001/SC-002/SC-003).
- Total estimated diff: **~250 LOC production + ~50 LOC test/harness**. Zero LOC deletions (the m210 userspace-side `classify_filter_category` stays — it's used by `compiler_pipeline.rs` for orthogonal purposes like SecretsAdjacent alerting).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C**: ✅ No C. New classifier is Rust in the `no_std` `mikebom-ebpf` crate, compiled via aya-ebpf + bpf-linker (already the workspace's kernel toolchain).
- **II. eBPF-Only Observation**: ✅ CANONICAL — the classifier moves observation-filter logic further *into* the kernel (from userspace-post-aggregation to kernel-pre-ringbuffer). Fewer bytes cross the kernel/user boundary; the trust boundary tightens.
- **III. Fail Closed**: ✅ On classifier / map-attach failure, the event flows through the filter unblocked (as if the filter were disabled) — the trace continues. The failure is surfaced via the existing `kprobe_attach_failures[]` mechanism (m212 pattern) with a new entry `filter_category_hits`, so operators can detect degraded filtering. This is fail-open on the *filter*, but the underlying trace remains complete (no events lost due to filter failure) — the failure mode is "more noise" not "less signal".
- **IV. Type-Driven Correctness**: ✅ `FilterCategoryTag` is a `#[repr(u8)]` enum with `TryFrom<u8>` for wire boundary. Match statements are exhaustive; unknown discriminants error at parse time rather than silently coerce. No `.unwrap()` in production code; test-only unwraps are `#[cfg_attr(test, allow(clippy::unwrap_used))]` per the existing `mikebom-cli/src/trace/counters.rs::tests` convention.
- **V. Specification Compliance**: ✅ Audited standards-native precedence: `trace_integrity` is mikebom's own predicate (attestation-collection wrapper around witness/v0.1), not CDX/SPDX/CISA/PURL. The `filter_categories_applied[]` field name is REUSED from the existing m210 `compiler_pipeline.filter_categories_applied` field — one vocabulary, two sites. No new `mikebom:` prefix introduced. `FileEvent` wire shape unchanged (FR-005) preserves compatibility with the m020 witness-v0.1 attestation-collection consumer.
- **VI. Three-Crate Architecture**: ✅ Only the three existing crates change. `mikebom-ebpf` gains a helper function + 2 maps; `mikebom-cli` gains a userspace reader function + wiring; `mikebom-common` gains one enum + one Vec<String> field. No new crates.
- **VII. Test Isolation**: ✅ Unit tests run in-process (no kernel needed) via in-memory `HashMap<FilterCategoryTag, u64>` mocks. Container harness (privileged Docker isolated from host) exercises the eBPF path end-to-end. `cargo test --workspace` on unprivileged CI is unaffected.
- **VIII. Completeness**: ⚠️ CRITICAL — this milestone deliberately DROPS events that would otherwise be recorded. Completeness Principle VIII says: "every network request and file-read event observed by the eBPF trace MUST be processed and represented in the output unless explicitly filtered by a user-specified exclusion rule." The kernel-side filter IS a user-specified exclusion (the operator chooses whether to run with the default filter enabled or the `--include-system-reads` widening flag). Per FR-006 the filter emits `filter_categories_applied[]` — a transparent record of which categories were filtered, so completeness gaps are auditable rather than silent. **This is Completeness-compliant IF AND ONLY IF the aggregate signal is emitted per FR-006/FR-007/FR-008/FR-009.** Skipping the signal emission would violate Principle VIII.
- **IX. Accuracy**: ✅ Filter drops NOISE (paths definitionally not part of the compilation graph — kernel meta-fs, per-user cache dirs, ephemeral scratch, cargo's out-of-band fingerprint bookkeeping). Fewer false positives, not more. This is an *accuracy* fix.
- **X. Transparency**: ✅ CANONICAL — the whole feature is transparency. `filter_categories_applied[]` on `trace_integrity` is spec-native (mikebom-native) metadata using the same JSON structure as m210's `compiler_pipeline.filter_categories_applied`.
- **XI. Enrichment**: N/A — no enrichment layer.
- **XII. External Data Source Enrichment**: N/A — no external data sources.

**Strict Boundaries**:
- 1. No lockfile-based discovery — N/A.
- 2. No MITM proxy — N/A; this is trace-side filtering, not observation.
- 3. No C code — enforced (all new code is Rust).
- 4. No `.unwrap()` in production — enforced (test-only unwraps guarded per convention).
- 5. No file-tier duplicates in default mode — N/A (milestone touches trace-side, not SBOM-side).

**Verdict**: ✅ Constitution check passes. Principle VIII deserves careful attention (the filter drops events by design); the mitigation is FR-006's `filter_categories_applied[]` transparent aggregate. Emitting the aggregate is a MERGE-BLOCKING requirement per the analysis above. No violations to justify.

## Project Structure

### Documentation (this feature)

```text
specs/213-kernel-noise-filter/
├── plan.md                          # This file (/speckit.plan command output)
├── research.md                      # Phase 0 — categorization decisions, verifier-cost analysis, kernel-side path-matching primitive, widening-flag transport, wire-shape strategy
├── data-model.md                    # Phase 1 — FilterCategoryTag enum, FILTER_CATEGORY_HITS map, FILTER_WIDEN map, TraceIntegrity.filter_categories_applied field, aggregate summary shape
├── quickstart.md                    # Phase 1 — end-to-end verification recipe (container harness + jq assertions + widening-flag toggle)
├── contracts/
│   ├── filter-category-tag.md       # Wire contract for the u8→string mapping
│   ├── filter-hits-map.md           # Kernel↔user PerCpuArray<u64> contract (5 slots — 4 categories + 1 reserved)
│   └── ebpf-verifier-notes.md       # Verifier acceptance contract for the classifier's word-wide compare pattern
├── checklists/
│   └── requirements.md              # (already exists from /speckit.specify)
└── tasks.md                         # Phase 2 output (/speckit.tasks — NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
mikebom-ebpf/
└── src/
    ├── maps.rs                                  # +2 maps: FILTER_CATEGORY_HITS (PerCpuArray<u64>, 4 slots), FILTER_WIDEN (PerCpuArray<u8>, 1 slot)
    ├── helpers.rs                               # +1 helper: increment_filter_category_hit(cat: u8) (mirrors m212's increment_drop_counter)
    └── programs/
        └── file_ops.rs                          # +path_matches_filter_category() classifier + 2 short-circuit branches in try_do_filp_open + try_openat2

mikebom-cli/
└── src/
    ├── cli/
    │   └── scan.rs                              # +at trace-end: call counters::read_filter_category_hits + populate TraceIntegrity.filter_categories_applied
    └── trace/
        ├── loader.rs                            # +at load-time: write ScanArgs.include_system_reads into FILTER_WIDEN[0]
        └── counters.rs                          # +read_filter_category_hits() + FilterCategoryHitsSummary struct + tests

mikebom-common/
└── src/
    ├── events.rs                                # +FilterCategoryTag u8-repr enum + TryFrom<u8> impl
    └── attestation/
        └── integrity.rs                         # +filter_categories_applied: Vec<String> field on TraceIntegrity + extend round-trip test

scripts/
└── ebpf-integration-test.sh                     # +3 jq assertions per SC-001/SC-002/SC-003 (rustc event present, ring_buffer_overflows ≤ 10, filter_categories_applied contains "CargoFingerprint")

Dockerfile.ebpf-test                             # NO CHANGES (harness reuses m212 container image as-is)
```

**Structure Decision**: single-milestone edit spanning all three workspace crates, mirroring the m212 shape (which also spanned all three). One new module extension in `mikebom-cli/src/trace/counters.rs` (an existing m212 module) keeps the aggregation logic co-located with the m212 ring_buffer_overflows reader. No new files, no new crates, no new modules. Kernel-side changes are additive (new maps + new helper + new classifier + early-return branches); no existing kprobe entry-point structure is disturbed.

## Complexity Tracking

> No Constitution violations. Complexity tracking section unused.
