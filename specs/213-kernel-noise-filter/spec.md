# Feature Specification: Kernel-side trace-noise filter for file_ops kprobes

**Feature Branch**: `213-kernel-noise-filter`
**Created**: 2026-07-21
**Status**: Draft
**Input**: User description: "Kernel-side trace-noise filter for file_ops kprobes — drop cargo fingerprint spam before ring buffer (issue #616, follow-up to #614)"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Cargo builds no longer lose real compiler events to fingerprint spam (Priority: P1)

An operator traces a `cargo build --release` on a real Rust workspace to
attest which source files rustc read and which artifacts the linker
produced. Today, the emitted attestation contains zero rustc file
events and zero linker file events — cargo's fingerprint-check step
opens ~167,000 files under `target/*/fingerprint/`, `target/*/deps/`,
and `target/incremental/`, saturating the file-events ring buffer
before rustc has even started. Every subsequent event, including all
the ones the operator actually wanted, is silently dropped.

With the kernel-side noise filter in place, the fingerprint-check
opens are discarded before they reach the ring buffer, freeing capacity
for the rustc and linker events. The operator now sees actual compiler
inputs and linker outputs in the attestation.

**Why this priority**: This is the reason the feature exists. Milestone
212 (issue #615) made the drop bug *visible*, but the attestation is
still 100% noise + 0% signal on any non-trivial cargo build. Without
this fix the entire compiler-pipeline capture path (milestone 210)
produces useless output on the primary Rust workflow.

**Independent Test**: Trace a cargo build of a 4-crate Rust workspace
(the existing m212 SC-001 fixture `two_binaries_diverge`) and verify
the emitted attestation's `file_access` section contains at least one
rustc-authored event AND at least one linker-authored event. Pre-fix
baseline: 0 of each.

**Acceptance Scenarios**:

1. **Given** a cargo build that opens ~167K fingerprint-check files, **When** the operator runs a trace with the kernel-side filter enabled by default, **Then** the emitted attestation's `file_access` section contains at least one rustc file event and at least one linker file event.
2. **Given** the same trace, **When** the operator inspects `trace_integrity.ring_buffer_overflows`, **Then** the value is at most 10 (down from ≥100 pre-fix per the m212 SC-001 fixture assertion).
3. **Given** the same trace, **When** the operator inspects `file_access.operations`, **Then** no operation references a path under `target/*/fingerprint/`, `target/*/deps/`, or `target/incremental/`.

---

### User Story 2 - Operator can see which noise categories the filter suppressed (Priority: P2)

An operator receiving an attestation from a colleague wants to know
what the trace *didn't* capture — not the specific paths (which would
leak host filesystem structure), but the *categories* of noise the
filter dropped. This lets a downstream reviewer judge attestation
completeness without needing to re-run the trace.

The filter emits a compact aggregate field on `trace_integrity`
listing which of the four defined categories (System, UserCache,
Ephemeral, CargoFingerprint) actually fired during the trace.

**Why this priority**: Operators today have no way to distinguish
"the trace saw no `/etc/` reads because the process didn't do any"
from "the trace saw no `/etc/` reads because the filter dropped them
all." This ambiguity blocks any downstream reviewer from judging
attestation completeness. Splitting it out from US1 keeps the P1 MVP
focused on the actual signal-recovery win.

**Independent Test**: Inspect an attestation from a cargo build and
verify `trace_integrity.filter_categories_applied[]` is present, is a
sorted-deduplicated list of category names, and contains at least
`CargoFingerprint`.

**Acceptance Scenarios**:

1. **Given** a completed trace of a cargo build, **When** the reviewer queries `trace_integrity.filter_categories_applied[]`, **Then** the result is a non-empty JSON array of strings drawn from the closed set {`System`, `UserCache`, `Ephemeral`, `CargoFingerprint`}.
2. **Given** the same trace, **When** the reviewer parses that array, **Then** no entry is a filesystem path or a substring of a filesystem path (privacy — categories only, no specific paths).
3. **Given** a trace of a non-cargo build (e.g., a plain `gcc hello.c -o hello`), **When** the reviewer queries the same field, **Then** the array contains at most the categories that actually fired for that build (may omit `CargoFingerprint`).

---

### User Story 3 - Operator can opt out of System-category filtering when they need full coverage (Priority: P3)

An operator investigating a specific bug wants to see every file the
traced process touched, including reads under `/etc/`, `/proc/`, and
`/sys/`. They pass an existing widening flag (equivalent semantics to
the current `--include-system-reads`) and the kernel-side filter
disables the System category for that trace only. The other three
categories (UserCache, Ephemeral, CargoFingerprint) remain filtered
by default because their noise-to-signal ratio is universally poor.

**Why this priority**: The widening path is already established at
the userspace layer (m210 Phase 6). Porting the same knob to the
kernel-side filter is a straightforward extension but not required
for the fix that unblocks US1. Deferring to P3 keeps the initial
implementation narrow.

**Independent Test**: Run the same fixture as US1 twice — once
with default settings, once with the widening flag — and verify
`trace_integrity.filter_categories_applied[]` includes `System` in
the first run but not the second.

**Acceptance Scenarios**:

1. **Given** an operator invocation with the widening flag set, **When** the trace completes, **Then** `trace_integrity.filter_categories_applied[]` does NOT contain `System` (because the filter was disabled for that category).
2. **Given** the same invocation, **When** the reviewer parses `file_access.operations`, **Then** at least one operation references a path under `/etc/`, `/proc/`, `/sys/`, or `/dev/` (if the traced process performed any such reads).
3. **Given** an invocation without the flag, **When** the reviewer parses the same field, **Then** no operation references a path under `/etc/`, `/proc/`, `/sys/`, or `/dev/`.

---

### Edge Cases

- **Verifier budget exhaustion**: The prefix-compare loops must fit inside the eBPF verifier's 1M instruction budget on every supported kernel (5.15, 6.1, 6.6, 6.8). Milestone 211 established that fixed-size arrays and word-wide compares survive; the plan must reuse those patterns and prove verifier acceptance via the existing Colima 6.8 harness before merge.
- **Long paths**: The kernel scratch buffer used for path extraction is 256 bytes today (matches the `FileEvent.path` on-wire field). Paths longer than that are truncated before the prefix compare — the filter must treat truncated paths as "unknown category" and let them through (so a malicious actor can't hide a `target/*/fingerprint/` file by prepending 256 bytes of padding, and so legitimate long paths that don't match any pattern are never dropped by accident).
- **Symlinks and bind mounts**: The filter operates on the path passed to the `open()` syscall, not the resolved inode. If the operator has bind-mounted `/proc/` at some other location, the filter will not catch reads through that alternate path. This is acceptable for v1 — bind-mount-hiding is an adversarial case, not a build-tool-noise case.
- **Non-cargo Rust builds**: A build that invokes rustc directly (bazel, buck2, custom scripts) does not emit fingerprint-check noise, so the CargoFingerprint category simply doesn't fire. The other three categories still apply and behave identically.
- **Non-Rust builds**: Any traced build (make, ninja, gradle, npm, bazel) benefits from the System, UserCache, and Ephemeral filters. CargoFingerprint is cargo-specific and simply doesn't match anything else; this is expected.
- **Empty categories in the aggregate**: If no path matched any category during a trace (e.g., an extremely short traced command), `filter_categories_applied[]` MUST be an empty array `[]`, never `null` or the field being absent — the field's presence is the operator-visible signal that the filter ran.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST drop file-open events whose path matches the **System** category prefix set (`/etc/`, `/proc/`, `/sys/`, `/dev/`) kernel-side, before those events enter the file-events ring buffer, when the widening flag is not set.
- **FR-002**: System MUST drop file-open events whose path matches the **UserCache** category pattern set (paths containing `/.cache/` or `/.local/share/` as a directory component) kernel-side, before those events enter the file-events ring buffer.
- **FR-003**: System MUST drop file-open events whose path matches the **Ephemeral** category prefix set (`/tmp/`, `/var/tmp/`) kernel-side, before those events enter the file-events ring buffer.
- **FR-004**: System MUST drop file-open events whose path matches the **CargoFingerprint** category pattern set (paths containing `/fingerprint/`, `/deps/`, or `/incremental/` as a directory component beneath a `target/` ancestor) kernel-side, before those events enter the file-events ring buffer.
- **FR-005**: System MUST preserve the on-wire `FileEvent` shape unchanged — no new fields, no rename, no reordering. The filter drops events before they are ever constructed; consumers of the file-events stream see fewer events, not different events.
- **FR-006**: System MUST emit a `filter_categories_applied` field on `trace_integrity` in every emitted attestation. The field's value MUST be a sorted, deduplicated JSON array of strings.
- **FR-007**: `filter_categories_applied` values MUST be drawn from the closed set `{"System", "UserCache", "Ephemeral", "CargoFingerprint"}`. The string values MUST match the userspace `ClassifyFilterCategory` enum variant names verbatim so extractor tooling can join across the two layers.
- **FR-008**: `filter_categories_applied` MUST NOT contain any filesystem path, path fragment, or path-derived value. Category names only — no path leakage.
- **FR-009**: `filter_categories_applied` MUST be an empty array (`[]`) rather than absent or `null` when the filter ran but no category fired. The field's presence is the observable signal that the filter was active.
- **FR-010**: When the operator sets the widening flag (semantics equivalent to the existing userspace `--include-system-reads`), the **System** category filter MUST be disabled for that trace. The other three categories (UserCache, Ephemeral, CargoFingerprint) MUST remain active.
- **FR-011**: When the widening flag is set and the trace produced any file event under `/etc/`, `/proc/`, `/sys/`, or `/dev/`, `filter_categories_applied[]` MUST NOT contain `"System"` (because the filter did not fire for that category).
- **FR-012**: The filter MUST NOT drop file events whose path does not match any defined category prefix or pattern. Non-matching paths flow to the ring buffer unchanged.
- **FR-013**: The filter MUST NOT increase the eBPF program's verifier-instruction cost beyond the 1M budget on any of the supported kernel versions (5.15, 6.1, 6.6, 6.8). Verifier rejection at load-time is a merge-blocker.
- **FR-014**: On the m212 SC-001 fixture (`two_binaries_diverge` 4-crate cargo build), the reported `trace_integrity.ring_buffer_overflows` count MUST fall to at most 10 (down from the ≥100 baseline that m212 SC-001 currently asserts as a lower bound).
- **FR-015**: On the same fixture, the emitted attestation's `file_access.operations` array MUST contain at least one operation whose owning process is rustc AND at least one whose owning process is a linker (ld, ld.lld, mold). Pre-fix baseline: zero of each.
- **FR-016**: Paths longer than the eBPF program's path-extraction scratch buffer (256 bytes today, matching `FileEvent.path`) MUST be treated as "unknown category" and forwarded to the ring buffer unfiltered — prevents both (a) an adversary bypassing the filter by prepending padding and (b) accidentally dropping legitimate long paths that don't match any pattern.

### Key Entities

- **Filter category**: One of four named buckets used to classify file-open path prefixes. Each category has (a) a name (used both in code and in the on-wire aggregate) and (b) a set of matching patterns (used at kernel-side event-decision time).
- **Aggregate signal (`filter_categories_applied[]`)**: A sorted, deduplicated array of category names emitted on `trace_integrity`. Its presence tells reviewers the filter ran; its contents tell them which categories fired.
- **Widening flag**: An operator opt-in whose semantics match the existing userspace `--include-system-reads` behavior. When set, disables the System category filter for that trace only.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the m212 SC-001 fixture (`two_binaries_diverge`), tracing a `cargo build --release` and emitting the attestation results in ≥1 rustc file-access event and ≥1 linker file-access event (both currently 0 per issue #616's investigation).
- **SC-002**: On the same fixture, `trace_integrity.ring_buffer_overflows` reports a value ≤10 (down from the current m212 SC-001 assertion of >100).
- **SC-003**: The container-harness integration test at `scripts/ebpf-integration-test.sh` extends its m212 assertions to additionally require ≥1 rustc event and `ring_buffer_overflows ≤ 10`, and passes on every supported kernel (5.15, 6.1, 6.6, 6.8) tested via Colima.
- **SC-004**: The kernel-side eBPF programs load successfully (verifier accept) on each supported kernel version listed in SC-003 with the filter compiled in. Verifier rejection on any kernel is a merge-blocker.
- **SC-005**: A reviewer inspecting an emitted attestation can, without reading any filesystem path, determine which noise categories were suppressed during that trace, by reading `trace_integrity.filter_categories_applied[]` alone.
- **SC-006**: Running the same fixture with the widening flag set produces an attestation whose `filter_categories_applied[]` omits `"System"` AND whose `file_access.operations` contains ≥1 entry with a path under `/etc/`, `/proc/`, `/sys/`, or `/dev/`.

## Assumptions

- **Prerequisite #615 already landed**: Milestone 212 shipped the real `ring_buffer_overflows` counter, so SC-002 can be measured empirically rather than inferred. The `scripts/ebpf-integration-test.sh` harness already asserts the field is a JSON number > 100 pre-fix; the extension is trivial.
- **Widening flag already exists**: The userspace `--include-system-reads` flag was added in milestone 210 Phase 6 and its shape is stable. This spec assumes reuse of the same flag semantics for FR-010; no new flag surface is required.
- **UserCache detection is HOME-agnostic**: The kernel program cannot resolve the operator's `$HOME` directory. UserCache matching therefore uses trailing directory-component patterns (`/.cache/`, `/.local/share/`) rather than absolute HOME-anchored prefixes. This accepts a small false-positive risk (e.g., a project named `.cache/`) as the trade-off for portability across operator environments.
- **CargoFingerprint under any profile name**: Cargo lets users define custom profiles beyond `debug` and `release`. The `target/*/fingerprint/` pattern therefore matches on the sub-path components (`fingerprint`, `deps`, `incremental`) beneath any directory whose ancestor is named `target`, not on hard-coded profile literals.
- **Verifier-safe patterns are already established**: Milestone 211 established the recipe for kernel-side path comparisons that pass the verifier (fixed-size arrays, word-wide compares, no fat-pointer derefs). This spec assumes reuse; no new verifier-taming research is required.
- **Category-set is closed for v1**: The four categories (System, UserCache, Ephemeral, CargoFingerprint) cover the noise patterns identified in issue #614's investigation. Extension to other build-system-specific noise (npm `node_modules/`, gradle `.gradle/caches/`, bazel `bazel-*/`) is deferred to a follow-up spec once similar 0%-signal traces are observed for those toolchains.
- **Linux-only scope**: The kernel-side filter is Linux+eBPF-only, consistent with the entire trace pipeline. macOS and Windows dev builds continue to compile the filter code paths behind the existing `#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]` gates.
- **Post-filter userspace pipeline unchanged**: The userspace `EventAggregator`, `CompilerPipelineAggregator`, and attestation-builder code paths receive the same `FileEvent` shape as today and behave identically. The only observable difference is the reduced event volume and the new `filter_categories_applied[]` field on `trace_integrity`.
