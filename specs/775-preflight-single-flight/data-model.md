# Phase 1 Data Model — m775 single-flight preflight + go.work directive tolerance

**Feature**: 775-preflight-single-flight
**Status**: Complete
**Date**: 2026-09-05

Per-scan in-process types. No persistence, no wire representation, no cross-scan cache.

## Modified entities — US1

### `SharedPreflightCache` (existing — one field added)

```rust
pub struct SharedPreflightCache {
    /// UNCHANGED (m771): completed outcomes, keyed by scope root dir.
    /// Serves fast, non-blocking reads once a scope's preflight is done.
    pub entries: HashMap<PathBuf, PreflightOutcome>,

    /// NEW (m775): per-scope single-flight cells. The first worker to
    /// claim a scope holds ITS cell across the `go list all` subprocess;
    /// concurrent workers for the SAME scope block on that cell and
    /// read the memoized outcome. Distinct scopes hold distinct cells,
    /// so this never serializes unrelated preflights (FR-003).
    pub inflight: HashMap<PathBuf, Arc<Mutex<Option<PreflightOutcome>>>>,
}
```

**Validation rules**:
- A cell present in `inflight` with an inner value of `None` means "claimed, in flight, not yet complete".
- A cell whose inner value is `Some(outcome)` MUST agree with `entries[scope]` if the latter is present — both are written by the same worker before releasing the cell.
- `entries` remains the authority for the fast path; `inflight` exists solely to serialize the slow path.

**Lifecycle**: created empty per scan, dropped when the classifier returns. No cross-scan reuse (spec edge case: "repeated scans in one process").

---

### `PreflightOutcome` (existing — no shape change)

```rust
pub enum PreflightOutcome {
    Ok,
    Skipped(SkipReason),
}
```

**Milestone change**: NONE to the type. It must satisfy `Clone` (it already does) so a cell's memoized value can be handed to each waiter.

---

### `MainModuleAnalysis` (existing — one field added)

```rust
pub struct MainModuleAnalysis {
    pub verdicts: HashMap<String, GoModWhyVerdict>,   // UNCHANGED
    pub skip_reason: Option<SkipReason>,              // UNCHANGED
    pub workspace_active: bool,                       // UNCHANGED (m231)

    /// NEW (m775 FR-015): true iff THIS call performed an actual
    /// `go list all` subprocess spawn. False when the outcome came
    /// from the cache fast path OR from waiting on another worker's
    /// in-flight cell. Aggregated scan-level into the FR-015 counter.
    ///
    /// Counting spawns rather than requests is what makes SC-003
    /// meaningful — a request counter would read 39 both before and
    /// after the fix (research R3).
    pub preflight_spawned: bool,
}
```

**Validation rules**:
- Exactly one `MainModuleAnalysis` per scope may report `preflight_spawned == true` for a given scope within a scan (this IS the FR-001 invariant, and is what the counter observes).
- Loose workspaces (no governing `go.work`) always report `true` — they run their own preflight per FR-006.

---

### `GoModWhyOutcome` (existing — one field added)

```rust
struct GoModWhyOutcome {
    classified: HashSet<String>,        // UNCHANGED
    go_workspaces_found: bool,          // UNCHANGED
    analyzed: usize,                    // UNCHANGED
    prod: usize,                        // UNCHANGED
    test: usize,                        // UNCHANGED
    not_needed: usize,                  // UNCHANGED
    unresolved: usize,                  // UNCHANGED
    skipped: Option<&'static str>,      // UNCHANGED
    elapsed_ms: u128,                   // UNCHANGED
    workspace_modules: usize,           // UNCHANGED (m231)

    /// NEW (m775 FR-015): count of actual preflight subprocess spawns
    /// this scan. Reported as one additional field on the FR-013
    /// summary line. Aggregation mirrors `workspace_modules` verbatim
    /// (research R3).
    preflight_invocations: usize,
}
```

**Validation rules**: `preflight_invocations` ≤ (distinct scopes) + (loose workspaces). This is SC-003 stated as a type-level invariant; the automated test asserts it on a multi-workspace fixture.

---

## New entity — US2

### `go.work` directive vocabulary

The single shared definition of the directive keywords the `go.work` format defines. Consulted by both parsers per FR-014 (Clarifications Q2).

```
go | toolchain | godebug | use | replace
```

**Consumers**:
- Strict validator (`gowork.rs`) — decides whether an unrecognized leading token yields the `unknown-directive` reason.
- Lenient member-extractor (`mod_why.rs`) — confirms a non-`use` line it skips is a directive it legitimately ignores.

**Validation rules**:
- `toolchain` and `godebug` are accept-and-ignore: recognized as valid, contributing nothing to `GoWorkDocument` (research R6). Repeated `godebug` lines are valid — Go permits them, so no duplicate-detection applies (unlike `use` paths, which retain `duplicate-use-path`).
- Extending the vocabulary for a future Go release MUST be a one-place edit. A test asserts both parsers agree across a fixture corpus, so a single-parser edit fails CI.

---

## Unchanged surfaces

- **`GoWorkDocument`** — no new fields. `toolchain`/`godebug` are ignored, not captured (research R6).
- **Malformed-reason vocabulary** — `invalid-use-path`, `duplicate-use-path`, `invalid-replace-syntax`, `unknown-directive` all preserved verbatim (FR-013).
- **`GoWorkScope`**, **`SkipReason`**, **`GoModWhyVerdict`** — untouched.
- **C112 `waybill:go-workspace-mode`** — the annotation's name, catalog row, and cross-format parity treatment are unchanged. Only the *value* changes, and only on repositories whose `go.work` previously misparsed (FR-007, research R7).

---

## Diagram — US1 coordination

```text
            worker N pops a member workspace of scope S
                              │
                              ▼
        ┌─────────────────────────────────────────────┐
        │ acquire CACHE mutex (short, never across a  │
        │ spawn — FR-004)                             │
        │   • entries[S] present?  ─── yes ──▶ clone  │
        │                                     outcome │
        │   • else: clone-or-create inflight[S] cell  │
        │ release CACHE mutex                         │
        └─────────────────────────────────────────────┘
                    │                        │
         cache hit  │                        │ cache miss
                    ▼                        ▼
            return outcome        ┌──────────────────────────┐
            (no spawn;            │ acquire THIS SCOPE's     │
             preflight_spawned    │ cell mutex               │
             = false)             │                          │
                                  │  inner == None?          │
                                  │   ├─ yes: FIRST CLAIMANT │
                                  │   │   run `go list all`  │
                                  │   │   write outcome into │
                                  │   │   cell + entries     │
                                  │   │   preflight_spawned  │
                                  │   │     = true           │
                                  │   └─ no:  WAITER         │
                                  │       (blocked until the │
                                  │        claimant released)│
                                  │       read memoized      │
                                  │       outcome            │
                                  │       preflight_spawned  │
                                  │         = false          │
                                  │ release cell mutex       │
                                  └──────────────────────────┘

  Scope S′ ≠ S holds a DIFFERENT cell → proceeds fully concurrently (FR-003).
```

**Coordination invariants**:
- At most one worker per scope per scan observes `inner == None` and spawns (FR-001).
- Every waiter observes the identical outcome the claimant produced, success or failure (FR-002, FR-005).
- The cache mutex is held only for map operations, never across a spawn (FR-004).
- A panicking claimant poisons only its own scope's cell; waiters for that scope fail fast rather than hanging (NFR-001, NFR-002, research R2).

---

## Transition table

| Pre-milestone state | Post-milestone state | Trigger |
|---|---|---|
| Cache miss → release lock → spawn → re-acquire → insert (all 18 workers do this simultaneously) | Cache miss → claim per-scope cell → exactly one spawns, others block and reuse | FR-001, FR-002, R1 |
| One scope's preflight does not block another (accidentally true — nothing was serialized) | One scope's preflight still does not block another (deliberately true — distinct cells) | FR-003 |
| Cache mutex released before the spawn (correct, but the reason the stampede exists) | Cache mutex still released before the spawn; serialization moved to the per-scope cell | FR-004 |
| Preflight spawn count unobservable in-product | Reported on the FR-013 summary line and asserted in CI | FR-015, SC-003 |
| `godebug`/`toolchain` ⇒ `unknown-directive` ⇒ annotation reads `malformed` | Recognized via the shared vocabulary; annotation reports detected + member count | FR-011, FR-012, R5 |
| Two parsers with independent directive knowledge | Two parsers consulting one shared vocabulary | FR-014, R5 |

Every column-1 → column-2 transition preserves `GoWorkDocument`'s shape, the malformed-reason vocabulary, the m771 FR-007 skip semantics, the FR-013 summary line's existing fields, and byte-identity of every emitted document except the US2 annotation value.
