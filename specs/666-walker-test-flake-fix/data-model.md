# Data Model: Fix walk_registry test flake

## Entities

### `VisitSink` (test-owned, per-test-instance)

Per-test observation buffer for the walker's file-visit callback.

```rust
type VisitSink = std::sync::Arc<std::sync::Mutex<Vec<String>>>;
```

**Ownership**: created on the test's stack frame at test-body entry; two `Arc` clones exist for the duration of the walker's run — one held by the test (for post-run assertions), one held by the `ReaderRegistration.state` slot (for the walker's callback lookup). Both drop when the test exits.

**Lifecycle**:
1. Test body: `let sink: VisitSink = Arc::new(Mutex::new(Vec::new()));`
2. Registration: `state: Some(sink.clone() as Arc<dyn Any + Send + Sync>)` — Rust's `CoerceUnsized` implicitly coerces `Arc<Mutex<Vec<String>>>` to `Arc<dyn Any + Send + Sync>` at the field-assignment site.
3. Walker run: dispatches file events to the test's callback fn, which fetches `ctx.state::<Mutex<Vec<String>>>(reader_id)` and pushes visited filenames.
4. Test assertions: `let log = sink.lock().unwrap(); assert!(log.iter().any(|s| s == "expected"));`
5. Test exit: both Arc clones drop, sink deallocates.

**No cross-test sharing**: each test constructs its own sink. Two tests running concurrently hold two DIFFERENT `Arc`s pointing at two DIFFERENT `Mutex<Vec<String>>` allocations.

### `record_visit_*` callback family (test-scope fn pointers)

Three near-identical fn pointers, one per test. Each hardcodes the test's own `ReaderId` and dispatches to a shared helper.

```rust
fn push_visit_to_sink(path: &Path, ctx: &SharedWalkerContext, reader_id: ReaderId) {
    let Some(sink) = ctx.state::<Mutex<Vec<String>>>(reader_id) else { return };
    sink.lock().unwrap().push(path.file_name().unwrap().to_string_lossy().into_owned());
}

fn record_visit_loop(p: &Path, ctx: &SharedWalkerContext) {
    push_visit_to_sink(p, ctx, ReaderId::new("visitor-loop"));
}
fn record_visit_exclude(p: &Path, ctx: &SharedWalkerContext) {
    push_visit_to_sink(p, ctx, ReaderId::new("visitor-exclude"));
}
fn record_visit_noise(p: &Path, ctx: &SharedWalkerContext) {
    push_visit_to_sink(p, ctx, ReaderId::new("visitor-noise"));
}
```

**Why three separate callbacks and not one shared callback**: `FileCallback` is a bare `fn(&Path, &SharedWalkerContext)` pointer (defined at `walk_registry/mod.rs:360`). A callback cannot capture the current reader's id at runtime — the walker's dispatch loop invokes the pointer without passing the reader_id (`dispatch.rs:60`, `on_file(path_ref, ctx_ref)`). So the reader_id must be baked in at compile time via distinct fn pointers.

**Registration → callback binding table** (per-test):

| Test | reader_id | callback | Sink identity |
|------|-----------|----------|---------------|
| `walker_survives_symlink_loop` | `"visitor-loop"` | `record_visit_loop` | test-local `Arc` |
| `walker_respects_exclusion_set` | `"visitor-exclude"` | `record_visit_exclude` | test-local `Arc` |
| `walker_skips_default_noise_dirs` | `"visitor-noise"` | `record_visit_noise` | test-local `Arc` |

### `ReaderRegistration.state` slot (existing entity, unchanged)

The m664 contract C4 mechanism (`walk_registry/mod.rs:378-385`):

```rust
pub struct ReaderRegistration {
    pub reader_id: ReaderId,
    pub patterns: globset::GlobSet,
    pub state: Option<Arc<dyn Any + Send + Sync>>,   // ← THE FIX'S EXTENSION POINT
    pub on_file: Option<FileCallback>,
    pub on_dir: Option<DirCallback>,
    pub descend_into: Option<globset::GlobSet>,
}
```

**Not modified by this fix.** The tests just populate the `state` field where they previously left it `None`.

### `SharedWalkerContext::state` accessor (existing entity, unchanged)

Type-erased downcast lookup (`walk_context.rs:53-59`):

```rust
pub fn state<T: 'static>(&self, reader_id: ReaderId) -> Option<&T> {
    self.registrations
        .iter()
        .find(|r| r.reader_id == reader_id)
        .and_then(|r| r.state.as_ref())
        .and_then(|arc| arc.downcast_ref::<T>())
}
```

**Not modified by this fix.** Test callbacks use it with `T = Mutex<Vec<String>>`. The Arc peels via `downcast_ref` — you get `&Mutex<Vec<String>>` from `Arc<dyn Any>::downcast_ref::<Mutex<Vec<String>>>()`.

## Removed Entities

### `static SEMANTICS_LOG: Mutex<Vec<String>>` (REMOVED)

Was: file-scoped shared mutable state at `walker.rs:477`. Populated by `record_visit` (fn ptr registered by all three tests) and asserted against by each test after clearing it at test-body entry.

**Why removed**: root cause of the flake filed as #720. Three tests calling `.clear()` and `.iter()` on the same static Mutex race each other under cargo's parallel test scheduler.

### `fn record_visit(path, _ctx)` (REMOVED)

Was: single shared callback (`walker.rs:479`) that pushed to `SEMANTICS_LOG`.

**Why removed**: replaced by the per-test `record_visit_*` family (see above).

## Validation Rules

- **V1 — Sink ownership**: each test MUST create its own `VisitSink` via `Arc::new(Mutex::new(Vec::new()))`. Tests MUST NOT reuse sinks across `#[test]` fns. Enforced by code review; violations are visually obvious (a shared `static SINK: VisitSink` would restore the flake).
- **V2 — Unique reader_id per test**: each test MUST use a reader_id string distinct from the other tests. Reason: the `SharedWalkerContext::state` lookup keys on reader_id, so two tests reusing the same reader_id would produce a scenario where a callback resolves to the WRONG test's sink. Enforced at construction time — if two tests both use `"visitor-loop"`, one test's callback would find the other's sink and write to it, tripping cross-test assertions. Verified by grep for `ReaderId::new("visitor-...")` in the test module.
- **V3 — Callback fn pointer per test**: each test MUST register a callback fn pointer whose baked-in reader_id matches the test's registration reader_id. Violating this (test A's registration wired to test B's callback) would produce silent no-ops (`ctx.state::<T>(reader_id) = None` because the reader_id doesn't match A's registration). Enforced by the shape of the code — a maintainer would have to actively cross-wire.
- **V4 — Callback type parameter consistency**: every test's callback MUST call `ctx.state::<Mutex<Vec<String>>>(reader_id)` with the SAME `T`. Reason: the `state` slot stores the sink as `Mutex<Vec<String>>`; downcasting to any other type returns `None`. If the fix pattern generalizes to sinks of other types (e.g., `Mutex<Vec<PathBuf>>`), that would need a separate helper.
- **V5 — Sink drop-on-exit**: tests MUST NOT `mem::forget` or otherwise leak their sink. The two Arc clones MUST both drop at test-scope exit so cargo can wall-off the tests. Enforced by not calling `mem::forget`.

## State Transitions

Not applicable — the `VisitSink` has one state ("live within a test's stack frame"). Transitions are limited to `Arc::clone` (increments strong count) and `Arc::drop` (decrements).
