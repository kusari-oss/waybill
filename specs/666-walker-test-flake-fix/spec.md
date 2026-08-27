# Feature Specification: Fix walk_registry test flake

**Feature Branch**: `666-walker-test-flake-fix`
**Created**: 2026-08-26
**Status**: Draft
**Input**: User description: "Fix flake in walk_registry tests where shared SEMANTICS_LOG static causes cross-test race (closes issue #720)"
**Closes**: #720

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Walker unit tests never spuriously fail (Priority: P1)

A waybill maintainer pushes a PR that doesn't touch `waybill-cli/src/scan_fs/walk_registry/`. CI runs against the PR SHA. The three walker unit tests (`walker_survives_symlink_loop`, `walker_respects_exclusion_set`, `walker_skips_default_noise_dirs`) pass deterministically across every lane (linux-x86_64, macos-latest, windows-latest, ebpf lane) regardless of how cargo's parallel test-runner schedules them.

**Why this priority**: This is the sole user story — a flake-fix has exactly one behavior to preserve. Delivering the fix returns the maintainer to a state where a re-run is never needed to distinguish "real regression" from "shared-state race in a test." Every subsequent maintainer avoids the ~5 min debug + re-run tax that #720 observed on PR #719.

**Independent Test**: Run the walker test binary in isolation with `--test-threads=8` (or `nproc` on the CI runner) for **100 consecutive iterations** on both macOS and Linux; every iteration must show `3 passed / 0 failed` for the three affected tests. Pre-fix reproduction: the same loop against `main` at commit `dc8018d` will fail intermittently on macOS-arm64 (memory / CI cost of the observed flake).

**Acceptance Scenarios**:

1. **Given** a fresh clone of the repository, **When** the maintainer runs `cargo test -p waybill --test-threads=8 -- walker_survives_symlink_loop walker_respects_exclusion_set walker_skips_default_noise_dirs` 100 times in a row, **Then** every run reports `3 passed; 0 failed`.
2. **Given** the CI lane matrix on a PR, **When** the walker tests run under whatever parallelism the runner schedules, **Then** the tests never report a "log=[]" or "expected N entries, got 0" style assertion failure caused by shared-state interleaving.
3. **Given** a maintainer adds a *fourth* walker unit test that uses the same visit-log recording pattern, **When** they follow the pattern established by the fix, **Then** the new test also runs correctly in parallel with the other three (the pattern doesn't require future maintainers to remember a per-test serialization guard).

### Edge Cases

- What happens when the walker's registered `on_file` callback is invoked from multiple worker threads simultaneously (if the walker ever becomes multi-threaded)? The recording mechanism must not silently drop entries or panic in that case.
- What happens when a test panics mid-run and never restores or clears its recording state? Subsequent tests must not observe stale entries from the panicking test's aborted run.
- What happens on Windows lanes where `#[cfg(unix)]` guards exclude one of the three tests? The other two must still pass without depending on the excluded third for state cleanup.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The three unit tests `walker_survives_symlink_loop`, `walker_respects_exclusion_set`, and `walker_skips_default_noise_dirs` in `waybill-cli/src/scan_fs/walk_registry/walker.rs` MUST pass deterministically when run in parallel with each other.
- **FR-002**: The fix MUST NOT rely on serializing the tests via a global lock or by disabling cargo's parallel test execution — the three tests must be able to run genuinely in parallel without racing.
- **FR-003**: Each test's assertions MUST observe only the file visits produced by its own walker invocation — never entries left over from a sibling test's invocation, nor an empty log because a sibling test called `.clear()` at the wrong moment.
- **FR-004**: The fix MUST leave the tests' behavioral assertions unchanged (the noise-dir skip logic, the symlink-loop survivability check, and the exclusion-set filter check are all real invariants the tests exist to protect — the fix is scoped to how they observe the walker's output, not to what they check).
- **FR-005**: The fix MUST NOT add a new production-side dependency to `waybill-cli/Cargo.toml`. Test-only (dev-dep) additions are permitted only if no in-tree alternative exists.
- **FR-006**: The fix MUST NOT extend the walker's public API surface (`SharedWalker`, `ReaderRegistration`, `SharedWalkerContext`) with new fields or methods used only by tests — if the fix threads test-owned state through the walker, it must use existing extension points (e.g., the `state: Option<Box<dyn Any + Send + Sync>>` slot that `ReaderRegistration` already exposes per m664 contract C4).
- **FR-007**: The fix MUST leave the walker's runtime behavior byte-identical to pre-fix on non-test code paths — no production emission changes, no golden updates, no dependency-graph changes.
- **FR-008**: A future maintainer adding a fourth walker unit test with the same visit-recording need MUST be able to follow the established pattern without introducing a new shared-state race — the fix should generalize, not paper over the three specific sites.

### Key Entities

- **`SEMANTICS_LOG` (current)**: file-scoped `static Mutex<Vec<String>>` in `walk_registry/walker.rs`, populated by a `record_visit` callback and asserted against by three tests. The problem entity — removed or replaced by the fix.
- **Per-test visit log (post-fix shape)**: each test's own visit-recording sink, isolated from sibling tests. Its concrete representation (e.g., a heap-allocated `Vec` threaded through the walker's per-registration `state` slot, a channel-based mpsc collector, a per-test `Arc<Mutex<Vec<String>>>` owned by the test's stack frame) is an implementation choice deferred to the plan phase; the spec constrains only the observable property: no cross-test interference.
- **The three affected tests**: `walker_survives_symlink_loop` (Unix-only), `walker_respects_exclusion_set`, `walker_skips_default_noise_dirs`. All three follow the same shape (register a walker, run it, assert against a visit log) and receive the same fix.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100 consecutive local test runs (`--test-threads=8`) on macOS-arm64 pass all three walker tests. Pre-fix: fails intermittently at a rate observed to be at least 1 in ~20 CI runs (basis: the observed failure on PR #719's first CI run, which triggered #720).
- **SC-002**: Zero pre-existing tests regress. The full workspace test-suite passes at the same count as pre-fix (baseline verified on the 666-walker-test-flake-fix branch before implementation begins).
- **SC-003**: Zero net change to golden files (`waybill-cli/tests/fixtures/golden/**`) — the fix is unit-test-only and does not touch the emission pipeline.
- **SC-004**: `./scripts/pre-pr.sh` passes without any workaround or `--test-threads=1` fallback. Post-fix, the three walker tests survive parallel scheduling under both the default `nproc` thread count AND `--test-threads=1` (deterministic serial) AND `--test-threads=8` (aggressive parallelism).
- **SC-005**: A maintainer reading the post-fix code for the first time can identify the visit-recording pattern in one file-read of `walk_registry/walker.rs`'s test module — the pattern is discoverable without cross-referencing a memory, a spec, or a separate helper module. (Verified by: the fix touches only `walk_registry/walker.rs` unless a new pattern requires an accompanying test-support module — in which case that module ships with a clear docblock explaining the isolation guarantee.)
- **SC-006**: Issue #720 auto-closes on merge via `Closes #720` in the PR body.

## Assumptions

- **Cargo's parallel test runner is the sole source of the observed race**. No other mechanism (e.g., a hypothetical async runtime spawning off the walker's work) contributes to the flake. This matches the root-cause analysis in the issue body and the `dc8018d` code state; a change to the walker's threading model in a future milestone would require re-evaluating.
- **The three tests' current behavioral assertions are correct**. The fix is scoped to solving the observation-race; if any of the three tests contains a latent logic bug in its walker-invariant check, that bug is out of scope for this feature and files as its own issue.
- **Local reproduction is achievable on macOS-arm64 or Linux-x86_64 within a 100-iteration loop**. The observed flake rate (at least 1-in-20 on CI macOS-arm64) is high enough that 100 iterations provides high-confidence reproduction pre-fix. If the flake proves harder to reproduce than the issue implies, SC-001's methodology may need to move to a 1000-iteration harness before deciding the fix is verified.
- **The `state: Option<Box<dyn Any + Send + Sync>>` slot on `ReaderRegistration` is the intended extension mechanism for per-registration test-owned state per m664 contract C4**. If the plan phase determines this slot has a hidden restriction that prevents the natural fix shape, the plan is free to propose an alternative (per-test `Arc<Mutex<Vec<String>>>` owned outside the walker, or an mpsc channel) — but should document why the existing slot didn't fit.
- **The fix will land as a standalone PR against `main`**, not bundled with unrelated work. The scope is small enough (single file, three tests) that bundling would obscure the fix.
