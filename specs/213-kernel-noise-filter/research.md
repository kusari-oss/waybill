# Phase 0 Research: Kernel-side trace-noise filter

**Feature**: 213-kernel-noise-filter
**Date**: 2026-07-21

## R1 — Filter-category set (which 4 categories, and why exactly these)

**Decision**: Ship v1 with exactly `System`, `UserCache`, `Ephemeral`, `CargoFingerprint`. Deliberately EXCLUDE the m210 userspace-side `SecretsAdjacent` category.

**Rationale**: The m210 userspace `classify_filter_category` at
`mikebom-cli/src/trace/compiler_pipeline.rs:368` classifies FIVE categories,
not four. The fifth — `SecretsAdjacent` — is used to **alert** the operator
when the trace touched a `.ssh/`, `.aws/credentials`, or `.env`-style path,
not to filter it out. Moving `SecretsAdjacent` kernel-side and DROPPING
those events would (a) hide a security-relevant signal from the attestation
and (b) violate Principle IX (Accuracy — flagging vs. silently dropping is a
one-way door).

The four categories that *should* be filtered are the ones whose noise-to-
signal ratio is empirically >99% on real cargo/gcc/gradle/npm builds:

| Category         | Empirical noise ratio (m212 SC-001 fixture) | Why safe to drop kernel-side              |
|------------------|-----|-------------------------------------|
| System           | ~9K opens; 0 known consumer downstream | Kernel meta-fs; not part of any build graph |
| UserCache        | ~4K opens; 0 known consumer downstream | Per-user cache dirs; opaque to build |
| Ephemeral        | ~1.5K opens; 0 known consumer downstream | Compiler-generated scratch; already ephemeral |
| CargoFingerprint | ~167K opens (per issue #616 investigation); 0 signal | Cargo-internal timestamp bookkeeping |

**Alternatives considered**:
- (a) Drop `SecretsAdjacent` too — REJECTED per rationale above.
- (b) Add per-ecosystem noise categories (npm `node_modules/`, gradle `.gradle/caches/`, bazel `bazel-*/`) — REJECTED for v1 scope; deferred to a follow-up spec once similar 0%-signal traces are observed for those toolchains (per Assumptions).
- (c) Make the category set configurable via a `--filter-categories=...` operator flag — REJECTED for v1 scope; the four defaults cover the primary pain point (cargo builds on any Rust workspace).

## R2 — Kernel-side path-matching primitive

**Decision**: Fixed-size `[u8; 32]` pattern arrays + `len: u8`, word-wide `u64` compare inside `#[inline(always)]` `path_matches_prefix(pattern: &[u8; 32], plen: u8, path: &[u8; 256]) -> bool`.

**Rationale**: Milestone 211 (issue #611) established that the eBPF verifier
tolerates fixed-size u64-word-wide compares but explodes on byte-by-byte
loops when combined with slice indexing (verifier state-explosion via loop
unrolling). The m211 compiler-exec whitelist compare at
`mikebom-ebpf/src/programs/compiler_exec.rs:169` is proven working on
Colima aarch64 6.8 and Ubuntu 22.04 amd64 6.5. Reusing that exact pattern
verbatim.

Pattern shape:

```rust
#[repr(C)]
struct PathPattern {
    bytes: [u8; 32],     // NUL-padded prefix, e.g., b"/etc/\0\0\0\0..."
    len: u8,             // effective prefix length (< 32)
    category: u8,        // FilterCategoryTag discriminant
    _pad: [u8; 6],       // align to 8 bytes for word compare
}

const PATTERNS: [PathPattern; 15] = [...];
```

Compare loop is unrolled 4×u64 iterations for max prefix length of 32
(covers the longest pattern `/.local/share/` at 14 bytes + slack). Each
iteration is 3 instructions (load, xor, jne). 4 iterations × 3 insns × 15
patterns = 180 insns per open on the slow path (path is shorter than
pattern) and ~50 on the fast path (mismatch in the first u64). Total
verifier cost: ~800 instructions per open, well within the 1M budget.

**Alternatives considered**:
- (a) Longest-prefix trie in a `#[map]` — REJECTED: overkill for 15
  patterns, and lookup traversal is verifier-hostile (unbounded pointer
  chains).
- (b) BPF LPM_TRIE map — REJECTED: LPM_TRIE is designed for IP addresses,
  not path prefixes, and the aya wrapper doesn't expose a clean API for
  byte-string keys yet.
- (c) `bpf_strncmp` helper — REJECTED: available only in kernel 6.1+; we
  need 5.15+ per SC-003.

## R3 — Widening flag transport (kernel↔user)

**Decision**: `PerCpuArray<u8>` with 1 slot (`FILTER_WIDEN`). Loader writes `1` when `ScanArgs.include_system_reads` is true, `0` otherwise, once at load time. Kernel-side classifier reads `FILTER_WIDEN[0]` at every open; when `1`, `System`-category matches short-circuit to `None`.

**Rationale**: Per-CPU array with a single entry gives every CPU its own
copy of the flag — no cross-CPU contention on the read, no atomic needed.
The write is loader-time (not per-event), so operator-facing latency is
zero. Read cost per open: 1 map lookup + 1 branch (~5 insns).

**Alternatives considered**:
- (a) Compile-time feature flag (recompile eBPF program with/without System
  filter) — REJECTED: forces operators to keep two binaries, incompatible
  with the one-binary-serves-all posture.
- (b) Global `AtomicU8` in `mikebom-ebpf/src/globals.rs` — REJECTED:
  `#[no_mangle] static mut` globals are technically supported by aya-ebpf
  but the pattern isn't proven in the codebase; per-CPU arrays ARE proven
  (m212 SCRATCH_BUF).
- (c) Encoded into the `SCRATCH_BUF` reserved bytes — REJECTED: overloads
  a scratch buffer with config semantics, bug-prone.

## R4 — On-wire aggregation shape

**Decision**: New field `filter_categories_applied: Vec<String>` on `TraceIntegrity`, populated at trace-end by summing across CPUs then filtering to categories with count > 0, then sorting + deduplicating the resulting strings.

**Rationale**: Consumers of the emitted attestation need a stable, sorted,
deduplicated list they can `jq` on. `Vec<String>` with human-readable
category names (`"System"`, `"UserCache"`, `"Ephemeral"`, `"CargoFingerprint"`)
matches the exact convention already established by m210's
`compiler_pipeline.filter_categories_applied` field — one vocabulary, two
sites, no divergence. FR-007 pins the strings to the enum variant names
verbatim so extractor tooling can join across the two layers with
byte-identity comparison.

The **kernel↔user** boundary carries u8 discriminants, not strings — the
Rust-to-JSON conversion happens exactly once, at trace-end, in the
`counters::read_filter_category_hits` reader, via a match statement that
returns `&'static str` per variant. No allocation on the eBPF side, no
string copy on the userspace side per-event.

**Alternatives considered**:
- (a) `HashMap<String, u64>` on the wire (per-category counts, not just
  names) — REJECTED: leaks host-scale information (a home lab vs. a
  production cluster's build has visibly different absolute counts, and
  the counts have no per-attestation-consumer value beyond "did the filter
  fire").
- (b) Bitfield `u8` (each bit = one category fired) — REJECTED: not
  human-readable in `jq`, breaks the m210 vocabulary consistency.
- (c) Reuse `compiler_pipeline.filter_categories_applied` at scope level —
  REJECTED: `compiler_pipeline` is a nested predicate that only exists when
  compiler events were captured (m210 SC-001); the trace-integrity signal
  needs to be present on every emitted attestation regardless of whether
  the compiler-pipeline nested predicate ran (per FR-009).

## R5 — Wire-shape byte-identity strategy for `TraceIntegrity`

**Decision**: The new `filter_categories_applied: Vec<String>` field is ADDITIVE — serde adds a new key at the end of the JSON object, deserialization back-compat is unchanged. Attestations emitted pre-m213 (missing the field) round-trip via `#[serde(default)]` to `Vec::new()`.

**Rationale**: The m212 `TraceIntegrity` shape freeze (FR-003) applied to
existing fields; new *additive* fields are permitted as long as:
- Default-value round-trip is stable (`serde(default)` = `Vec::new()`).
- Empty state serializes as `[]` (FR-009), not omitted.
- Field placement is at struct end so the pre-m213 JSON prefix is
  byte-identical.

The m212 `ring_buffer_overflows` field followed the same policy (it existed
pre-m212 as always-zero; m212 populated it with a real value). The m213
`filter_categories_applied` field is strictly additive with the same policy.

Round-trip test extension:

```rust
#[test]
fn trace_integrity_serde_populated_filter_categories_applied() {
    let integrity = TraceIntegrity {
        ring_buffer_overflows: 8,     // ≤10 target from SC-002
        kprobe_attach_failures: vec![],
        filter_categories_applied: vec![
            "CargoFingerprint".to_string(),
            "Ephemeral".to_string(),
            "System".to_string(),
        ],
        // ... other fields
    };
    let json = serde_json::to_string(&integrity).unwrap();
    let round_tripped: TraceIntegrity = serde_json::from_str(&json).unwrap();
    assert_eq!(
        serde_json::to_value(&integrity).unwrap(),
        serde_json::to_value(&round_tripped).unwrap(),
    );
}
```

**Alternatives considered**:
- (a) Nest under a new `filter: FilterMetadata { categories_applied: ... }`
  sub-struct — REJECTED: adds nesting depth for one field, breaks the flat
  shape convention every m212 field follows.
- (b) Use `Option<Vec<String>>` with `None` = filter didn't run — REJECTED:
  the filter ALWAYS runs (there's no way to disable it wholesale post-
  m213); `None` is impossible in practice, and FR-009 pins the empty state
  to `[]` explicitly.

## R6 — Kernel↔user hit-count aggregation semantics

**Decision**: Sum-across-CPUs at trace-end reads a single `[u64; num_cpus]` slice per category via `PerCpuArray::get(&0, 0)?` (matches m212 `read_percpu_sum`). Categories with `sum > 0` produce the corresponding string name in `filter_categories_applied`; categories with `sum == 0` are omitted.

**Rationale**: `PerCpuArray<u64>` gives lock-free per-CPU counters that the
verifier accepts. Sum happens exactly once, at trace-end, so per-event cost
is O(1) increment (no cross-CPU synchronization). Aggregation cost at trace
end: 4 categories × num_cpus lookups = ~512 map calls on a 128-core host,
<5 ms — well within acceptable trace-end latency.

The map's slot count is 4 (one per `FilterCategoryTag` variant); each slot
is a per-CPU u64. Total kernel memory: 4 × num_cpus × 8 bytes = 4 KB on
128-core host. Negligible.

**Alternatives considered**:
- (a) Single-slot per-CPU array with the category embedded in the value's
  high bits — REJECTED: harder to increment atomically, no memory savings
  worth the complexity.
- (b) HashMap<u8, u64> in-kernel — REJECTED: aya-ebpf `HashMap` requires
  KV updates via `insert()` which the verifier state-explodes on when
  called from a hot kprobe path.

## R7 — SecretsAdjacent handling (userspace-only, unchanged)

**Decision**: The m210 userspace `classify_filter_category` at `mikebom-cli/src/trace/compiler_pipeline.rs:368` continues to classify `SecretsAdjacent` events for its alerting purpose. These events now flow through the kernel-side filter untouched (they don't match System/UserCache/Ephemeral/CargoFingerprint prefixes), reach the ring buffer, are consumed by the userspace `EventAggregator` + `CompilerPipelineAggregator`, and the userspace classifier fires as before.

**Rationale**: SecretsAdjacent is orthogonal to noise filtering — it's a
signal-boost mechanism (add a WARN to the attestation when a suspicious
path is touched), not a signal-suppress mechanism. The kernel-side filter
targets noise; userspace SecretsAdjacent targets alerts. Both continue to
work in tandem.

**Alternatives considered**: n/a — this is a "keep working as-is" decision
rather than a change.

## R8 — Container-harness assertion strategy (SC-001 + SC-002 + SC-003)

**Decision**: Extend `scripts/ebpf-integration-test.sh` with three new jq blocks that assert:
1. `.predicate.trace_integrity.filter_categories_applied | index("CargoFingerprint")` is non-null (SC-003).
2. `.predicate.trace_integrity.ring_buffer_overflows <= 10` (SC-002).
3. `[.predicate.file_access.operations[] | select(.comm == "rustc")] | length >= 1` (SC-001).

**Rationale**: The m212 harness already runs `mikebom trace capture` on
the `two_binaries_diverge` fixture inside a --privileged Colima container.
Adding three jq assertions is <30 LOC and reuses the existing container
image + fixture + trace invocation.

The assertion 1 lower bound (`CargoFingerprint` MUST appear on any cargo
build) is a hard signal — if it's absent, the filter isn't wired.
Assertion 2 is the SC-002 empirical target. Assertion 3 is the SC-001
signal-recovery win — post-filter, rustc events MUST appear in
`file_access.operations`, whereas pre-filter their observation was 0.

**Alternatives considered**:
- (a) New standalone integration test binary — REJECTED: duplicates the
  m212 harness plumbing.
- (b) Rust `#[test]` gated by `MIKEBOM_EBPF_INTEGRATION=1` env var —
  REJECTED: the tests need privileged eBPF loading; shell script harness
  in Docker is the established pattern (m211/m212).

## R9 — Failure semantics if `FILTER_CATEGORY_HITS` map fails to attach

**Decision**: On attach failure of `FILTER_CATEGORY_HITS`, the classifier fails-open (returns `None` on every path) — no events are filtered. The failure is surfaced via `TraceIntegrity.kprobe_attach_failures[]` with the new entry `"filter_category_hits"` (matching m212's Q3 disambiguation pattern for `file_event_drops` etc.).

**Rationale**: Fail-open on the *filter* means "no signal recovery" but
also "no false-drop risk" — the trace continues to work as pre-m213. The
`kprobe_attach_failures[]` entry lets operators detect the degraded state.

**Alternatives considered**:
- (a) Fail-closed (bail the entire trace if the hits map fails) — REJECTED:
  the trace worked pre-m213 without this map; killing it because of a
  new-in-m213 dependency map would be a regression.
- (b) Silent fail-open with no attach_failures signal — REJECTED:
  operators lose the ability to diagnose why cargo-noise reappears in a
  post-m213 attestation.
