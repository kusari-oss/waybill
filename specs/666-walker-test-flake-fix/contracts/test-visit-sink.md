# Contract: Test-visit-sink pattern for walk_registry unit tests

## Interface

### `VisitSink` type alias (test-scope only)

```rust
type VisitSink = std::sync::Arc<std::sync::Mutex<Vec<String>>>;
```

**Scope**: `#[cfg(test)] mod tests { ... }` inside `waybill-cli/src/scan_fs/walk_registry/walker.rs`.

**Ownership contract**: each `#[test]` fn that needs to observe walker file visits MUST construct its own `VisitSink` on its stack frame. Sinks MUST NOT be `static`, MUST NOT be `thread_local!`, and MUST NOT be reused across tests.

### `push_visit_to_sink` helper

```rust
fn push_visit_to_sink(path: &Path, ctx: &SharedWalkerContext<'_>, reader_id: ReaderId);
```

**Behavior**: fetches the sink from `ctx.state::<Mutex<Vec<String>>>(reader_id)`. If the sink is present, pushes `path.file_name().unwrap().to_string_lossy().into_owned()` onto the guarded Vec. If the sink is absent (reader_id mismatch, no state populated, downcast failure), returns silently — a defensive no-op that keeps the walker's dispatch loop unblocked.

**Signature MUST match** `FileCallback`'s expected shape for the wrapper fn pointers to satisfy the walker's callback typedef.

### Per-test callback wrapper family

Every `#[test]` fn that consumes the sink MUST register a distinct fn-pointer callback that hardcodes its own `ReaderId` at compile time:

```rust
fn record_visit_<test-slug>(path: &Path, ctx: &SharedWalkerContext<'_>) {
    push_visit_to_sink(path, ctx, ReaderId::new("visitor-<test-slug>"));
}
```

Where `<test-slug>` is unique per test (`loop` / `exclude` / `noise` for the three existing tests). The `ReaderId::new(...)` string MUST match the registration's `reader_id` in the same `#[test]` body.

## Behavioral contract

### C1: Isolation guarantee

Two tests running concurrently MUST observe:
- Their own visit-log entries only.
- Zero visits from sibling tests.
- Zero effects from sibling tests' `sink.lock()` calls.

Enforced by construction: each test's Arc points at a distinct `Mutex<Vec<String>>` allocation. There is no path through which one test's writes reach another test's sink.

### C2: Panic safety

If a test panics mid-run (assertion failure, walker callback panic caught by `dispatch_file`'s `catch_unwind`, tempfile teardown panic, etc.), that test's sink drops with its stack frame. Sibling tests observe no residue and no cross-test lock poisoning because they hold different `Mutex` identities.

### C3: Reader_id uniqueness

Every `#[test]` that consumes a sink MUST use a `ReaderId::new(...)` string distinct from the other tests' `ReaderId::new(...)` strings. Two tests sharing a reader_id would produce a scenario where a callback resolves to the WRONG test's sink (both registrations sit in the same registration slice; `SharedWalkerContext::state` picks the first match). While the tests would still run in isolation of sink writes (they hold separate Arcs), the LOOKUP would silently misroute — a latent bug for anyone extending the pattern.

Regression guard: `grep -c 'ReaderId::new("visitor-' waybill-cli/src/scan_fs/walk_registry/walker.rs` should return exactly the count of test wrappers, and `sort -u` on the extracted strings should equal that count (no dupes).

### C4: No walker API surface change

The fix MUST NOT:
- Extend `ReaderRegistration` with new fields.
- Extend `SharedWalker` with new methods.
- Extend `SharedWalkerContext` with new accessors.
- Change the `FileCallback` typedef signature.

Every extension point used by the fix (`ReaderRegistration.state`, `SharedWalkerContext::state::<T>()`, `FileCallback`) exists at the pre-fix code (grep-verified at `walk_registry/mod.rs:378`, `walk_context.rs:53`, `walk_registry/mod.rs:360` respectively).

### C5: Zero production code changes

The fix MUST NOT modify any code outside `#[cfg(test)]` blocks in `walk_registry/walker.rs`. No other file in the workspace changes.

### C6: Assertion-shape preservation

The three tests' post-walker assertions MUST retain the same logical shape as pre-fix:

- `walker_survives_symlink_loop`: `assert_eq!(log.iter().filter(|s| s.as_str() == "target.marker").count(), 1, ...)`
- `walker_respects_exclusion_set`: `assert!(!log.iter().any(|s| s == "in_excluded.marker"), ...); assert!(log.iter().any(|s| s == "in_kept.marker"), ...);`
- `walker_skips_default_noise_dirs`: `assert!(!log.iter().any(|s| s == "in_git.marker"), ...); assert!(!log.iter().any(|s| s == "in_nm.marker"), ...); assert!(log.iter().any(|s| s == "top.marker"), ...);`

The `.iter()` / `.any()` / `.filter()` / `.count()` shape stays. Only the `log` binding source shifts from `SEMANTICS_LOG.lock().unwrap()` to `sink.lock().unwrap()`.

## Test-authoring rules

### T1: Adding a fourth test

To add a `walker_XYZ` test that observes visits, the maintainer:

1. Chooses a unique reader_id string: `"visitor-xyz"`.
2. Writes a callback wrapper immediately after the existing three:
   ```rust
   fn record_visit_xyz(path: &Path, ctx: &SharedWalkerContext<'_>) {
       push_visit_to_sink(path, ctx, ReaderId::new("visitor-xyz"));
   }
   ```
3. In the test body, constructs the sink:
   ```rust
   let sink: VisitSink = Arc::new(Mutex::new(Vec::new()));
   ```
4. Registers the walker with the sink threaded via the `state` slot:
   ```rust
   .register(ReaderRegistration {
       reader_id: ReaderId::new("visitor-xyz"),
       state: Some(sink.clone()),   // implicit Arc<T> → Arc<dyn Any + Send + Sync> coercion
       // ... patterns, on_file: Some(record_visit_xyz), etc.
   })
   ```
5. After `walker.run()`, asserts via `let log = sink.lock().unwrap(); assert!(...)`.

Time-to-add: ~10 lines. No new module, no memory to reference.

### T2: Constructor pattern for the state Arc coercion

The registration's `state: Some(sink.clone())` line relies on Rust's `CoerceUnsized` to convert `Arc<Mutex<Vec<String>>>` to `Arc<dyn Any + Send + Sync>` at the field-assignment site. This works because `Mutex<Vec<String>>: Any + Send + Sync` (all three traits derived structurally from the type's contents). If a future test needs a sink type that's NOT `Any + Send + Sync` (unlikely but possible), the coercion fails at compile time — surfacing the constraint immediately.

### T3: 100-iteration verification harness (implementation-time only, not shipped)

At implementation time, the maintainer verifies FR-001/SC-001 via:

```bash
for i in $(seq 1 100); do
  cargo +stable test -p waybill --lib -- \
      scan_fs::walk_registry::walker::tests::walker_survives_symlink_loop \
      scan_fs::walk_registry::walker::tests::walker_respects_exclusion_set \
      scan_fs::walk_registry::walker::tests::walker_skips_default_noise_dirs \
      --test-threads=8 --nocapture 2>&1 | grep -q "test result: ok. 3 passed" || { echo "FAIL at iter $i"; exit 1; }
done
echo "PASS: 100 iterations, 0 failures"
```

Not shipped as a persistent CI harness (per research R3): the fix's correctness is verified once at implementation time; ongoing regression detection relies on cargo's standard parallel scheduler catching any future re-introduction of shared state.

## Non-contracts

- **`walker_survives_symlink_loop` remains `#[cfg(unix)]`-gated.** The fix does not restore that test on Windows lanes; that's a separate concern.
- **`empty_registry_produces_empty_output` is unchanged.** That test doesn't touch `SEMANTICS_LOG` (it verifies the walker's zero-registration path), so it needs no changes.
- **The `record_visit` fn from pre-fix code is removed, not renamed.** Any grep for `fn record_visit(` in the codebase should return zero hits post-fix.
- **`SEMANTICS_LOG` static is removed, not renamed.** Any grep for `SEMANTICS_LOG` should return zero hits post-fix.
