# Quickstart: Adding a walk_registry unit test after the flake fix

**Audience**: waybill maintainer writing a new `#[test]` inside `waybill-cli/src/scan_fs/walk_registry/walker.rs` that needs to observe the walker's file-visit callbacks.

## 5-step recipe

### Step 1 — Choose a unique reader_id string

Every existing sink-observing test uses a `"visitor-<slug>"` reader_id. Pick a slug that hasn't been used:

```bash
grep -oE 'ReaderId::new\("visitor-[^"]+"\)' waybill-cli/src/scan_fs/walk_registry/walker.rs | sort -u
```

Existing (as of the m666 fix): `visitor-loop`, `visitor-exclude`, `visitor-noise`. Pick `visitor-<your-slug>` where `<your-slug>` names what your test verifies (e.g., `visitor-max-depth` for a max-descent test).

### Step 2 — Add a per-test callback wrapper

Immediately after the existing callback family (right after `fn record_visit_noise`), add:

```rust
fn record_visit_<your_slug_snake_case>(path: &Path, ctx: &SharedWalkerContext<'_>) {
    push_visit_to_sink(path, ctx, ReaderId::new("visitor-<your-slug>"));
}
```

The `push_visit_to_sink` helper handles the state lookup, downcast, lock, and push.

### Step 3 — Construct a per-test sink in your test body

```rust
#[test]
fn walker_verifies_your_property() {
    let sink: VisitSink = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

    // ... your fixture setup (tempdir, files, symlinks, etc.) ...
```

The sink lives on the test's stack frame. Cargo's parallel scheduler cannot cause races because each test's sink is a distinct `Arc<Mutex<Vec<String>>>` allocation.

### Step 4 — Wire the walker with the sink threaded via the `state` slot

```rust
    let registry = ReaderRegistryBuilder::new()
        .register(ReaderRegistration {
            reader_id: ReaderId::new("visitor-<your-slug>"),
            state: Some(sink.clone()),   // Arc<Mutex<Vec<String>>> → Arc<dyn Any + Send + Sync> via CoerceUnsized
            patterns: globset_from_patterns(&["**/*.marker"]).unwrap(),
            on_file: Some(record_visit_<your_slug_snake_case>),
            on_dir: None,
            descend_into: None,
        })
        .build()
        .unwrap();

    let excludes = ExclusionSet::new_empty();
    let mut walker = SharedWalker::new(root, &registry, &excludes);
    walker.run();
    let _ = walker.finish();
```

The `sink.clone()` at the `state` field is a cheap Arc increment. The Rust compiler implicitly coerces `Arc<Mutex<Vec<String>>>` to `Arc<dyn Any + Send + Sync>` — no `as ...` needed.

### Step 5 — Assert against your own sink

```rust
    let log = sink.lock().unwrap();
    assert!(
        log.iter().any(|s| s == "expected.marker"),
        "your assertion; log={:?}",
        *log,
    );
}
```

That's it. Total ceremony: ~10 new lines including the wrapper fn.

## Verification recipe (implementation-time only)

After adding your test, verify it survives cargo's parallel scheduler by running the 100-iteration loop:

```bash
for i in $(seq 1 100); do
  cargo +stable test -p waybill --lib -- \
      scan_fs::walk_registry::walker::tests::walker_verifies_your_property \
      --test-threads=8 --nocapture 2>&1 | grep -q "1 passed; 0 failed" || { echo "FAIL at iter $i"; exit 1; }
done
echo "PASS: 100 iterations"
```

Then run the FULL walker test set to confirm your test doesn't accidentally race against a sibling:

```bash
for i in $(seq 1 100); do
  cargo +stable test -p waybill --lib -- \
      scan_fs::walk_registry::walker::tests:: \
      --test-threads=8 --nocapture 2>&1 | grep "test result: ok" || { echo "FAIL at iter $i"; exit 1; }
done
```

## Anti-patterns to avoid

### Don't: reintroduce shared static state

```rust
// ❌ WRONG — re-creates the exact flake #720 filed
static SHARED_SINK: Mutex<Vec<String>> = Mutex::new(Vec::new());
```

Each test MUST own its own sink. The whole point of the fix is that no `static` mutable state exists in the test module.

### Don't: reuse another test's reader_id

```rust
// ❌ WRONG — silently misroutes state lookup
#[test]
fn walker_verifies_your_property() {
    let sink: VisitSink = Arc::new(Mutex::new(Vec::new()));
    let registry = ReaderRegistryBuilder::new()
        .register(ReaderRegistration {
            reader_id: ReaderId::new("visitor-loop"),  // ← already used by walker_survives_symlink_loop
            state: Some(sink.clone()),
            // ...
        })
```

If two tests share a reader_id and both run concurrently, the walker's registration slice contains two entries with the same id. `SharedWalkerContext::state::<T>(reader_id)` returns the FIRST match — which may be the wrong test's sink. Silent misrouting. Always use a unique slug (see Step 1).

### Don't: capture the sink in a closure and try to use it as `on_file`

```rust
// ❌ WRONG — FileCallback is `fn`, not `Fn`
on_file: Some(|path, ctx| {
    sink.lock().unwrap().push(...);   // captures `sink` — closures don't fit fn-pointer type
}),
```

`FileCallback` is a bare `fn(&Path, &SharedWalkerContext)` pointer (defined at `walk_registry/mod.rs:360`), not a closure trait. Captures aren't allowed. Use the wrapper-fn pattern (Step 2) instead.

### Don't: extract the pattern to a separate `waybill-cli/src/testing/` helper module

The fix deliberately keeps everything in `walker.rs`'s test module (per SC-005 discoverability-in-one-file-read). If a maintainer reading the tests for the first time has to cross-reference an external helper, the pattern's readability drops. Add the ~10-line ceremony inline.

## Reference

- Spec: [`spec.md`](./spec.md)
- Plan: [`plan.md`](./plan.md)
- Research: [`research.md`](./research.md)
- Data model: [`data-model.md`](./data-model.md)
- Contract: [`contracts/test-visit-sink.md`](./contracts/test-visit-sink.md)
- Issue: [#720](https://github.com/kusari-oss/waybill/issues/720)
- m664 contract C4 (state slot) definition: `waybill-cli/src/scan_fs/walk_registry/mod.rs:378-385`
- `SharedWalkerContext::state::<T>()` accessor: `waybill-cli/src/scan_fs/walk_registry/walk_context.rs:53-59`
- Production reader precedents using the state slot: `dart.rs:155`, `cocoapods.rs:137`, `go_binary.rs:561`, `cargo.rs:224`, etc.
