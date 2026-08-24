# Phase 1 Data Model: Single-Pass Walker with Reader-Registry Dispatch

**Feature**: 664-single-pass-walker
**Date**: 2026-08-21

Entity definitions for the shared-walker + reader-registry subsystem. All entities live in-process for the duration of a single `scan_path` invocation; nothing persists to disk.

## `ReaderId`

**Purpose**: Stable, allocation-free identifier for a package-db reader that has migrated to the shared registry.

**Shape**:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReaderId(&'static str);

impl ReaderId {
    // One `pub const` per migrated reader. Added in the reader's migration PR.
    pub const HASKELL: ReaderId = ReaderId("haskell");
    pub const IPK_FILE: ReaderId = ReaderId("ipk_file");
    // ... one per migrated reader
}
```

**Validation rules**:
- The `&'static str` value MUST be unique across all `pub const` declarations. Enforced by `waybill-cli/src/scan_fs/walk_registry/tests.rs::all_reader_ids_are_unique` (a compile-time-list check that iterates the constants and asserts distinctness).
- The value MUST match the reader's module name (e.g., `ReaderId::HASKELL` in `haskell.rs`). Convention only; not enforced.

**Relationships**:
- Owned by: the reader module.
- Referenced by: `ReaderRegistration` (as the identity), `SharedWalkerContext::push` (as the sink key), `WalkerMetrics::per_reader_dispatch_counts` (as the log-line key).

**Lifecycle**: `'static` (embedded in the binary).

## `ReaderRegistration`

**Purpose**: The unit of interest a reader declares to the shared registry: "when the walker visits a file (or a directory) matching these patterns, call me back."

**Shape**:

```rust
pub struct ReaderRegistration {
    pub reader_id: ReaderId,
    pub patterns: globset::GlobSet,   // filename patterns; matched against file basename OR path suffix
    pub on_file: Option<FileCallback>,
    pub on_dir: Option<DirCallback>,
}

pub type FileCallback = fn(&Path, &SharedWalkerContext<'_>);
pub type DirCallback = fn(&Path, &SharedWalkerContext<'_>);
```

**Validation rules**:
- At least one of `on_file` / `on_dir` MUST be `Some`. A registration with both `None` is rejected at `ReaderRegistry::register` with a clippy-friendly `debug_assert!` (unreachable in practice; catches API misuse in test).
- `patterns` MUST compile successfully at scan init. `globset` returns `Result` from its builder; the registration site propagates the error via `anyhow` (this is scan-init failure, not runtime).

**Relationships**:
- Owned by: `ReaderRegistry` (in insertion order — R8 determinism).
- Points to: `ReaderId` (identity), `FileCallback` / `DirCallback` (behavior).

**Lifecycle**: Created at scan init, dropped at `SharedWalker::finish()`.

## `ReaderRegistry`

**Purpose**: The collection of all `ReaderRegistration`s for a scan. Owns the dispatch fast-path (composite `GlobSet` for O(patterns) match).

**Shape**:

```rust
pub struct ReaderRegistry {
    registrations: Vec<ReaderRegistration>,   // insertion order preserved
    composite_matcher: globset::GlobSet,       // union of every registration's patterns
    // Index from composite matcher index → registration index; O(1) lookup post-match.
    match_index: Vec<usize>,
}
```

**Validation rules**:
- `composite_matcher.matches(path)` returns indices into `match_index`, which then map to the actual registration for dispatch.
- Empty registry (no readers migrated) is a valid state — the shared walker still runs to build the dir-index but every dispatch is a no-op. Used during US1 pre-first-migration and by tests.

**Relationships**:
- Consumed by: `SharedWalker::run()`.
- References: `Vec<ReaderRegistration>` (owned).

**Lifecycle**: Built at scan init from `ReaderRegistry::builder()`; consumed by the shared walker; dropped at `SharedWalker::finish()`.

## `SharedWalker`

**Purpose**: The single-pass filesystem walker that traverses the scan tree once, populates the `DirIndex`, and dispatches per-file + per-dir callbacks to the `ReaderRegistry`.

**Shape**:

```rust
pub struct SharedWalker<'reg, 'ex> {
    rootfs: &'reg Path,
    registry: &'reg ReaderRegistry,
    exclude_set: &'ex ExclusionSet,     // m113 handle
    max_depth: usize,                    // defaults to unbounded per current safe_walk semantics
    visited: HashSet<PathBuf>,           // m054 symlink-loop protection
    dir_index: DirIndex,                 // populated during descent
    metrics: WalkerMetrics,              // FR-009 aggregator
    output: HashMap<ReaderId, Mutex<Vec<PackageDbEntry>>>,
}
```

**Validation rules**:
- FR-002 semantic preservation: `max_depth`, `exclude_set`, `visited` all match today's `safe_walk` behavior.
- FR-006 byte-identity: `read_dir` results MUST be sorted by filename before dispatch — see `DirIndex` below.

**Relationships**:
- Consumes: `ReaderRegistry`, `ExclusionSet`.
- Produces: `DirIndex`, `WalkerMetrics`, per-reader `Vec<PackageDbEntry>`.

**Lifecycle**: One instance per `read_all` invocation. Instantiated in `read_all` post-registration, dropped at `finish()` call.

## `SharedWalkerContext<'w>`

**Purpose**: The reader-facing handle passed to every callback. Provides scoped access to the walker's shared state without exposing the walker's internals.

**Shape**:

```rust
pub struct SharedWalkerContext<'w> {
    walker: &'w SharedWalker<'_, '_>,
}

impl<'w> SharedWalkerContext<'w> {
    pub fn dir_index(&self) -> &DirIndex { ... }
    pub fn exclude_set(&self) -> &ExclusionSet { ... }
    pub fn push(&self, reader_id: ReaderId, entry: PackageDbEntry) { ... }
    // `push` acquires the per-reader `Mutex` briefly.
}
```

**Validation rules**:
- `push` panics-in-debug if `reader_id` is not a registered reader (indicates a reader pushing under someone else's ID — a bug).

**Relationships**:
- Held by: the reader's `on_file` / `on_dir` callback for the duration of the callback.
- Wraps: `&SharedWalker`.

**Lifecycle**: Ephemeral — created per callback invocation.

## `DirIndex`

**Purpose**: The in-memory (directory → filenames) map that satisfies FR-003's "sibling lookup without extra syscalls" contract (Clarify Q1).

**Shape**:

```rust
pub struct DirIndex {
    entries: HashMap<PathBuf, Arc<Vec<OsString>>>,
}

impl DirIndex {
    pub fn siblings_of(&self, file: &Path) -> Option<Arc<Vec<OsString>>>;
    pub fn contains(&self, dir: &Path, filename: &OsStr) -> bool;
    pub fn insert(&mut self, dir: PathBuf, sorted_filenames: Vec<OsString>);
}
```

**Validation rules**:
- `insert` requires the filename vector to be pre-sorted (FR-006 byte-identity). Debug assertion.
- Keys are canonicalized absolute paths — matches the m054 canonicalize-then-key convention.

**Relationships**:
- Owned by: `SharedWalker`.
- Read by: reader callbacks via `SharedWalkerContext::dir_index()`.

**Lifecycle**: Built incrementally as the walker descends. Dropped at `finish()`.

## `WalkerMetrics`

**Purpose**: FR-009 diagnostic aggregator — counts populated during the walk, emitted as one INFO log line at `SharedWalker::run()` completion.

**Shape**:

```rust
pub struct WalkerMetrics {
    passes: u32,                             // baseline: 1
    files_visited: u64,
    dirs_visited: u64,
    per_reader_dispatch_counts: BTreeMap<ReaderId, u64>,  // BTreeMap for stable log ordering
    started_at: std::time::Instant,
}

impl WalkerMetrics {
    pub fn tick_file(&mut self, dispatched_to: &[ReaderId]);
    pub fn tick_dir(&mut self);
    pub fn emit(&self) { /* tracing::info!(...) with the R9 shape */ }
}
```

**Validation rules**:
- `per_reader_dispatch_counts` MUST include every registered reader, even those with zero dispatches (so operators can see "reader X was registered but got zero hits" — helpful for validating pilot-set sizing).

**Relationships**:
- Owned by: `SharedWalker`.
- Updated by: dispatch loop.

**Lifecycle**: Instantiated at `SharedWalker::run()` start, consumed at `emit()`.

## `WalkAuditAllowlist` (test fixture, not a runtime entity)

**Purpose**: The plain-text list of `safe_walk(` callers that are permitted to remain OUTSIDE the shared registry — either because they haven't migrated yet (coexistence window) or because they're FR-005 permanent escape hatches (npm inner `node_modules/**`).

**Shape**: One `<file>:<content>` line per allowed occurrence, LF-terminated, LC_ALL=C-sorted.

**Validation rules**:
- Existing m117 walker-audit CI step compares `git grep`-produced actual list against this file; any diff fails CI.
- Every reader migration PR removes the reader's line(s) from this file in the same commit as the code migration — a coupled diff.

**Relationships**:
- File at: `waybill-cli/src/scan_fs/walk.audit-allowlist.txt` (extended from m117).
- Enforced by: `.github/workflows/ci.yml` walker-audit step (from m115/m117).

**Lifecycle**: Git-tracked; shrinks with each per-reader migration until only FR-005 permanent entries remain post-US3.

## Entity relationships (summary)

```
ReaderRegistry
    ├── registrations: Vec<ReaderRegistration>
    │       ├── reader_id: ReaderId
    │       ├── patterns: globset::GlobSet
    │       ├── on_file: Option<FileCallback>
    │       └── on_dir: Option<DirCallback>
    └── composite_matcher: globset::GlobSet (union)

SharedWalker
    ├── registry: &ReaderRegistry
    ├── exclude_set: &ExclusionSet (m113)
    ├── dir_index: DirIndex
    ├── metrics: WalkerMetrics
    └── output: HashMap<ReaderId, Mutex<Vec<PackageDbEntry>>>

SharedWalkerContext (ephemeral, per-callback)
    └── walker: &SharedWalker (accessors: dir_index, exclude_set, push)
```
