# Research: Fix walk_registry test flake — Phase 0 outputs

## R1: Fix shape — per-test-owned sink threaded via existing state slot

**Decision**: Each test creates its own `Arc<Mutex<Vec<String>>>` on the test's stack frame. The Arc is passed as `ReaderRegistration.state = Some(sink.clone() as Arc<dyn Any + Send + Sync>)`. Each test's file-callback fn hardcodes its own unique `ReaderId::new("visitor-...")` and looks up the sink via `ctx.state::<Mutex<Vec<String>>>(reader_id)`. Test's post-walker assertions read from its own `Arc` clone (held on the stack).

**Rationale**:
- **Reuses m664 contract C4 verbatim.** `ReaderRegistration.state: Option<Arc<dyn Any + Send + Sync>>` (defined at `waybill-cli/src/scan_fs/walk_registry/mod.rs:378-385`) and `SharedWalkerContext::state::<T>(reader_id)` (defined at `walk_context.rs:53`) are the exact mechanism every production reader already uses to thread per-scan state through the walker. 10+ production sites (dart, cocoapods, go_binary, cmake, cargo, vcpkg, conan, composer, erlang, scala) grep-verified using this pattern. Tests joining the same pattern makes them discoverable and pattern-consistent with production code.
- **Genuine parallelism-safe.** Each test owns its own `Arc`; sinks are never shared across tests. Cargo can schedule the three tests concurrently without any race — each writes to its own `Mutex<Vec<String>>`.
- **Zero API surface change** — satisfies FR-006. The `state` slot type stays `Option<Arc<dyn Any + Send + Sync>>`; nothing added to `ReaderRegistration`, `SharedWalker`, or `SharedWalkerContext`.
- **Zero new deps** — satisfies FR-005. `Arc`, `Mutex`, `Vec<String>` are stdlib. `Any + Send + Sync` are stdlib traits.
- **Panic-safe for FR edge-case cleanup**: `Arc<Mutex<Vec<String>>>` on a test's stack frame drops when the test's stack unwinds. If a test panics mid-run, its sink is destroyed with the stack frame; sibling tests observe no residue because they never had a reference to it. (Contrast with `static SEMANTICS_LOG` where a panicking test could leave stale entries visible to sibling tests.)

**Alternatives considered**:

- **File-scoped `SERIALIZE: Mutex<()>` static + retain `SEMANTICS_LOG`.** Fixes the flake by disallowing interleaving. Rejected: violates FR-002 ("MUST NOT rely on serializing the tests via a global lock or by disabling cargo's parallel test execution"). Even a per-file serialize-lock is a global lock at the scope where the race lives; it also slows total wall-time linearly in test count.
- **`serial_test` crate `#[serial]` attribute.** Standard idiomatic solution. Rejected: adds a dev-dependency the workspace does not currently use (violates FR-005's spirit even in the "test-only dev-dep" carve-out — the crate's Purpose does not stop at m664 walker tests; adopting it globally would be a larger surface). Also violates FR-002 (it's a serialization mechanism, not a genuine-parallelism fix).
- **mpsc channel-based collector: `let (tx, rx) = mpsc::channel(); ... state: Some(Arc::new(tx))`; test asserts on `rx.iter().collect()`.** Rejected: `mpsc::Sender<T>` is not `Sync` in older Rust versions (though `Sync` since 1.72; usable today). But the Sender-in-Arc requires a wrapper because `Sender` can't be shared across threads even under Arc — you'd need a `Mutex<Sender>` which regresses to the current problem shape (Mutex-guarded shared state, just wrapped differently). Not simpler than the direct Arc-Mutex-Vec approach.
- **Compile-time isolation via `PhantomData` or lifetimes.** Rejected: static analysis can't distinguish "shared static Vec" from "per-test Vec" without a language-level branding scheme; overengineered for a 3-test surface.
- **Introduce a small `waybill-cli/src/testing/walker_test_support.rs` module holding the sink helper.** Rejected: violates SC-005 discoverability-in-one-file-read. The 3 tests + their sink type + their record_visit callbacks all fit in ~30 lines of the existing test module; extracting them to a helper file adds indirection for negative value.

## R2: Callback-fn shape — one shared helper + three per-test wrappers

**Decision**: One shared helper `push_visit_to_sink(path, ctx, reader_id)` performs the state lookup + push. Each of the three tests defines a 3-line callback wrapper (`record_visit_loop`, `record_visit_exclude`, `record_visit_noise`) that calls the helper with the test's own `ReaderId`. The wrappers are unavoidable because `FileCallback: fn(&Path, &SharedWalkerContext)` (bare fn pointer — no captures, no per-test binding).

**Rationale**:
- **`FileCallback` signature is a bare `fn` pointer** (`waybill-cli/src/scan_fs/walk_registry/mod.rs:360`), NOT a `Fn` closure. A closure that captured the reader_id from a test's local scope would violate the type. So the reader_id must be baked into the callback at compile time — via a per-test wrapper fn.
- **The shared helper factors out the "look up state → downcast → lock → push" logic** so the three wrappers are one-line body each. Total: 3 tests × 1 wrapper × 3 lines = 9 lines of ceremony, plus 4 lines of helper. Readable in one file-scroll.
- **FR-008 generalization**: adding a 4th test requires copying the 3-line wrapper template + choosing a unique `ReaderId::new(...)`. No new pattern to invent.

**Alternatives considered**:

- **Change `FileCallback` to accept the dispatching reader's `reader_id` as a 3rd param.** Would eliminate per-test wrappers (a single generic `record_visit_via_state(path, ctx, reader_id)` could be the callback for every test). Rejected: violates FR-006 (extends the walker's public API surface with a slot used only by tests). Also would require modifying every existing production reader's callback signature — 20+ sites — for zero production benefit.
- **Use `std::sync::LazyLock` / thread-local to hold the "current reader_id" during dispatch.** Rejected: adds hidden state to the walker's execution model; violates FR-006 spirit; substantially more complex than three 3-line wrappers.

## R3: Verification methodology — 100-iteration parallel harness

**Decision**: At implementation time (in the PR's development loop), run the three tests in a 100-iteration loop with `--test-threads=8` on macOS-arm64 and Linux-x86_64. Every iteration must show `3 passed / 0 failed`. Command shape:

```bash
for i in $(seq 1 100); do
  cargo +stable test -p waybill --lib -- \
      scan_fs::walk_registry::walker::tests::walker_survives_symlink_loop \
      scan_fs::walk_registry::walker::tests::walker_respects_exclusion_set \
      scan_fs::walk_registry::walker::tests::walker_skips_default_noise_dirs \
      --test-threads=8 --nocapture 2>&1 | grep "test result: ok. 3 passed" || { echo "FAIL at iter $i"; break; }
done
```

Pre-fix baseline verification: run the same loop against `main` at commit `dc8018d`. Should observe ≥1 intermittent failure within 100 iterations on macOS-arm64 (basis: the observed flake rate that filed #720).

**Rationale**:
- **Matches memory `feedback_dont_dismiss_test_failures`**: "verify reproducibility (50x loop + CI history) before calling anything a flake." 100 iterations comfortably exceeds the 50x floor.
- **Realistic parallelism**: `--test-threads=8` is aggressive enough to force interleaving; too few threads (=1 or =2) mask races. macOS-arm64 CI runners are 3-4 cores; 8 threads yields real preemption.
- **Loop shape catches "usually passes, occasionally fails"** better than a single run does. Even if the observed CI failure rate is 1-in-20, a 100-iter loop hits the fail with high confidence.

**Alternatives considered**:
- **Ship the loop as a `#[test] #[ignore]` stress-test in walker.rs, opt-in via `cargo test -- --ignored`.** Adds a durable regression signal but ~50 lines of test-support code that doesn't check any invariant beyond the pattern itself. Rejected as scope creep; if the fix regresses, the flake will surface in normal CI (the fix's whole point is that the tests reliably pass under normal `--test-threads`; a regression would show up on the next PR that runs them).
- **1000 iterations.** Rejected: 100 is high-enough-confidence per the flake-rate math; 1000 adds ~10× wall-time for no measurable additional confidence.

## R4: Assertion pattern — read via `sink.lock()` on stack-held Arc

**Decision**: After `walker.run()`, each test does `let log = sink.lock().unwrap();` on its own `Arc` clone (held in a test-local `let` binding). Subsequent `assert!(log.iter().any(...))` reads through the guard. Existing assertion phrasing is preserved verbatim except for the `SEMANTICS_LOG.lock().unwrap()` → `sink.lock().unwrap()` swap.

**Rationale**:
- **Preserves FR-004** ("assertions unchanged"). The three tests' behavioral checks (noise-dir skip, symlink-loop survivability, exclusion-set filtering) are unchanged — only the observation-plumbing changes.
- **Same `.unwrap()` posture as before.** Test code allowed to `.unwrap()` per `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention (see CLAUDE.md Pre-PR verification section).
- **Panic-safe**: if `sink.lock()` fails (poisoned mutex from a walker panic mid-callback), the test unwinds and reports the poisoning. Sibling tests unaffected because they hold different Arc identities.

**Alternatives considered**:
- **Convert `Arc<Mutex<Vec<String>>>` to `Arc<RwLock<Vec<String>>>`.** RwLock allows concurrent read-side lookups but the write-side (callback path) is unchanged. Rejected: no measurable benefit for a Vec that only appends; adds cognitive overhead.
- **Convert to `Arc<Mutex<HashSet<String>>>` for O(1) `contains` in assertions.** Rejected: the tests use `.iter().any(...)` and `.filter().count()`; assertion cost is not a bottleneck. Set semantics also lose insertion order which two of the three tests could theoretically care about.

## R5: Interaction with `#[cfg(unix)]` guard on `walker_survives_symlink_loop`

**Decision**: The `#[cfg(unix)]` guard on `walker_survives_symlink_loop` is preserved verbatim. On Windows, only two of the three tests compile + run; on Unix, all three. Each test still owns its own sink regardless of platform.

**Rationale**:
- **The pre-fix `SEMANTICS_LOG` was shared across all three tests regardless of platform** — Windows lanes ran two tests against it; both cleared it before their own runs. Post-fix each test owns its sink; Windows still runs two of them without racing.
- **No cross-test cleanup dependency**: the pre-fix pattern implicitly relied on each test's `.clear()` at start — a fragile invariant that Windows lanes accidentally honored because only two tests ever touched the log there. Post-fix the pattern is explicit: no shared cleanup needed.

**Alternatives considered**:
- **Remove the `#[cfg(unix)]` guard and stub out symlinks on Windows.** Out of scope; the guard's existence is orthogonal to the flake fix.

## Constitution re-check post-research

All Phase 0 decisions preserve every principle:
- **VII (Test Isolation)**: fix RESTORES conformance — tests now run reliably under standard CI parallelism without elevated privileges.
- **IV (Type-Driven Correctness)**: `.unwrap()` in test code remains permitted per existing convention; no new domain-value newtypes needed for test-visit-log entries.
- **VI (Three-Crate Architecture)**: zero crate structure change.

No unresolved `NEEDS CLARIFICATION`. Ready to proceed to Phase 1 design.
