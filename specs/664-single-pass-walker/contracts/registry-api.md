# Contract: Reader-Registry Rust API

**Feature**: 664-single-pass-walker
**Date**: 2026-08-21

The reader-registry is an internal Rust API within `waybill-cli`, not an external contract. This document is the reference for reader-migration authors and the FR-006 / FR-011 gatekeeper.

## Public API surface (`waybill-cli/src/scan_fs/walk_registry/mod.rs`)

```rust
/// Reader identity — the key into per-reader output aggregation and
/// the value that appears in FR-009 diagnostic logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReaderId(&'static str);

impl ReaderId {
    /// New instances declared per migrated reader via `pub const`.
    pub const HASKELL: ReaderId;
    pub const IPK_FILE: ReaderId;
    pub const PANTS_COMMON: ReaderId;
    pub const SCALA: ReaderId;
    pub const ERLANG: ReaderId;
    pub const RPM_FILE: ReaderId;
    pub const YOCTO_RECIPE: ReaderId;
    // ... one per migrated reader
}

/// Per-file callback signature.
pub type FileCallback = fn(&Path, &SharedWalkerContext<'_>);

/// Per-directory callback signature. Fires once per directory the shared
/// walker descends into, AFTER the directory's contents are indexed.
pub type DirCallback = fn(&Path, &SharedWalkerContext<'_>);

/// Reader interest declaration.
pub struct ReaderRegistration {
    pub reader_id: ReaderId,
    pub patterns: globset::GlobSet,
    pub on_file: Option<FileCallback>,
    pub on_dir: Option<DirCallback>,
}

/// Builder for `ReaderRegistry`.
pub struct ReaderRegistryBuilder { /* private */ }

impl ReaderRegistryBuilder {
    pub fn new() -> Self;
    pub fn register(self, r: ReaderRegistration) -> Self;
    pub fn build(self) -> Result<ReaderRegistry, ReaderRegistryError>;
}

/// The registry, ready for walking.
pub struct ReaderRegistry { /* private */ }

/// The one-pass walker.
pub struct SharedWalker<'reg, 'ex> { /* private */ }

impl<'reg, 'ex> SharedWalker<'reg, 'ex> {
    pub fn new(rootfs: &Path, registry: &'reg ReaderRegistry, exclude_set: &'ex ExclusionSet) -> Self;
    pub fn with_max_depth(mut self, depth: usize) -> Self;
    pub fn run(&mut self);
    pub fn finish(self) -> HashMap<ReaderId, Vec<PackageDbEntry>>;
}

/// Reader-facing handle passed to every callback.
pub struct SharedWalkerContext<'w> { /* private */ }

impl<'w> SharedWalkerContext<'w> {
    pub fn dir_index(&self) -> &DirIndex;
    pub fn exclude_set(&self) -> &ExclusionSet;
    pub fn push(&self, reader_id: ReaderId, entry: PackageDbEntry);
}

/// Sibling-lookup index — the FR-003 "no extra syscalls" contract.
pub struct DirIndex { /* private */ }

impl DirIndex {
    /// The Arc-shared, sorted list of filenames in the same directory
    /// as `file`. Returns `None` if the walker did not visit that
    /// directory (e.g., it was excluded or beyond max_depth).
    pub fn siblings_of(&self, file: &Path) -> Option<Arc<Vec<OsString>>>;

    /// True iff the walker's index shows `dir` contains a file named
    /// `filename`. Cheap check used by two-phase readers.
    pub fn contains(&self, dir: &Path, filename: &OsStr) -> bool;
}
```

## Contract clauses

### C1. Dispatch order determinism (from R8)

For a given file that matches multiple registered patterns, callbacks MUST fire in **registration order** — the order in which `ReaderRegistryBuilder::register()` was called at scan init.

**Verification**: `registry_dispatches_in_registration_order` unit test in `walk_registry/tests.rs`.

### C2. Sibling-lookup index freshness (from Q1 + R4)

`DirIndex::siblings_of(file)` MUST return exactly the filenames that were present in the file's parent directory at the moment the shared walker read that directory. The returned list is IMMUTABLE for the duration of the scan (`Arc<Vec<OsString>>` — no interior mutation).

Sibling-lookup MUST NOT trigger a `read_dir()` syscall on the target directory.

**Verification**: `sibling_lookup_reads_from_index_not_disk` unit test — mock the filesystem, populate the index directly, remove the underlying dir, assert `siblings_of` still returns.

### C3. Sorted-filename invariant (from R4)

Every `Vec<OsString>` value stored in `DirIndex` MUST be sorted per `OsString`'s lexicographic ordering before insertion. This is the sole mechanism protecting FR-006 byte-identity from macOS vs Linux `readdir` ordering differences.

**Verification**: `DirIndex::insert` `debug_assert!(sorted_filenames.is_sorted())`, plus `dir_index_sorts_by_construction` integration test on a mixed-case-filename fixture.

### C4. Panic isolation (from R5)

If a reader's `on_file` or `on_dir` callback panics, the shared walker MUST:

1. Catch the panic via `catch_unwind` with `AssertUnwindSafe`.
2. Log a `tracing::warn!` with the reader ID, file path, and unwind payload if convertible to string.
3. Continue dispatching to remaining readers for the current file/dir.
4. Continue the walker's traversal.

**Verification**: `panicking_reader_does_not_abort_walker` integration test — register a reader whose callback panics on a specific fixture path, assert that other readers still receive dispatches for the same and subsequent paths.

### C5. FR-002 `WalkConfig` semantic preservation

The shared walker MUST honor:

- `max_depth` — bounded descent per `SharedWalker::with_max_depth`.
- `ExclusionSet` from m113 — directories matching an exclusion pattern are skipped without descent, matching current `safe_walk` behavior.
- m114 permissive-on-error posture — `read_dir` failures are silently skipped with a `tracing::debug!` log; the walker does not abort.
- m054 symlink-loop protection — canonicalized paths already in the `visited` set are skipped.

**Verification**: Existing `safe_walk` behavior tests are ported to `walk_registry/tests.rs` with the shared walker as the SUT.

### C6. Zero new syscalls beyond descent (from R4 + R2)

The shared walker MUST NOT trigger `read_dir` or `stat` beyond what's necessary for the one-pass descent. In particular:

- Sibling lookup: zero extra syscalls (contract C2).
- Per-file dispatch: zero extra syscalls (globset match is in-memory).
- Per-dir dispatch: zero extra syscalls (dir contents already indexed).

**Verification**: `walker_syscall_budget` integration test using `strace`/`dtrace` counts on a Linux CI runner (post-US1 gate). If instrumenting syscall counts proves flaky, fall back to the SC-005 microbenchmark as the sole enforcement.

### C7. FR-006 byte-identity of golden SBOMs

Every reader-migration PR MUST pass `cargo test --workspace` with zero golden updates. If a golden diff appears, the migration is reverted and reworked. This is a hard blocker with no override mechanism.

**Verification**: The existing `MIKEBOM_UPDATE_*` / `WAYBILL_UPDATE_*` environment variables that permit intentional golden regeneration MUST NOT be set on the migration PR's CI run. Enforced by pre-existing CI conventions.

### C8. Reader registration is idempotent within a scan (from R2)

Calling `ReaderRegistryBuilder::register` twice with the same `ReaderId` is an ERROR at `build()` time. Rationale: the FR-008 regression guard depends on `ReaderId` uniqueness in the log line; duplicates would corrupt the per-reader dispatch counts.

**Verification**: `duplicate_reader_id_rejected_at_build` unit test.

### C9. `ReaderId` uniqueness is compile-time-visible

The list of every `pub const ReaderId::...` declaration is the authoritative reader-migration audit trail. A unit test `all_reader_ids_are_unique` iterates every declared const and asserts pairwise distinctness. Adding a duplicate `pub const` fails CI.

**Verification**: `all_reader_ids_are_unique` unit test in `walk_registry/tests.rs`.

### C10. `descend_into` scope preservation (m664 post-2026-08-23 API extension)

A reader MAY declare `descend_into: Option<globset::GlobSet>` in its `ReaderRegistration` to opt into descending directories the shared walker's default skip set would otherwise block (`target/`, `build/`, `dist/`, `out/`, `venv/`, etc.).

**Scoping guarantee**: when the walker descends into a normally-skipped directory via one or more registrations' `descend_into` override, dispatch under that subtree is RESTRICTED to the set of readers whose `descend_into` opened the door. Non-requesting readers do NOT receive dispatch under that subtree.

Rationale: preserves byte-identity for the 21 already-migrated readers. Before this API extension, e.g., `target/` was invisible to every reader; after `target/` opens for maven via `descend_into`, cargo (whose `Cargo.toml` pattern would otherwise match a stray file in `target/some-dep/`) must still not see files under `target/`. The intersection also applies transitively — a reader scoped out at an outer subtree cannot re-enter via a nested `descend_into`.

**Non-triggering case**: `descend_into: None` (the common case for the 21 already-migrated readers) means the walker uses its default skip set unchanged — no scoping ever engages for that reader.

**Verification**: unit tests `descend_into_allows_requesting_reader`, `descend_into_scopes_out_non_requesting_readers`, `descend_into_absent_preserves_default_behavior` in `walk_registry/walker.rs`.

## Non-contracts (things this API deliberately does NOT provide)

- **Reader parallelism**: FR-012. Dispatch is sequential in the shared walker. Future follow-on may add parallelism; the current API is designed to be extensible (the `Mutex` around per-reader output vectors is future-proofing).
- **Subtree-scoped walking**: readers that need a bounded / content-driven sub-walk (like npm's `node_modules/**`) MUST use their own `safe_walk` call site. The FR-005 allowlist tracks these.
- **Dynamic reader registration**: registration happens once, at `read_all` init. Adding a reader mid-scan is not supported.
- **Non-file entries**: symlinks, block devices, sockets — the shared walker skips them at descent time, matching current `safe_walk` semantics. Readers do not receive callbacks for non-files.

## Migration checklist (for reader-migration PRs)

A reader-migration PR touches:

1. `waybill-cli/src/scan_fs/walk_registry/mod.rs` → add `pub const READER_NAME: ReaderId`.
2. `waybill-cli/src/scan_fs/package_db/<reader>.rs` → replace `safe_walk(...)` call site(s) with:
   - Extract the `should_skip` predicate → convert to a `globset` glob (or a union of globs).
   - Extract the per-visit closure body → move into a top-level `fn on_file` / `fn on_dir`.
   - At the reader's `pub fn read()` entry, no longer walk — instead take the shared walker's output for this reader ID (`walker_output.get(&ReaderId::READER_NAME)`).
3. `waybill-cli/src/scan_fs/package_db/mod.rs::read_all` → wire the reader into the shared registry via `builder.register(reader_module::registration())` at the appropriate ordering slot (matching the today's reader-invocation order for R8 determinism).
4. `waybill-cli/src/scan_fs/walk.audit-allowlist.txt` → remove the reader's `safe_walk(` entries.
5. `waybill-cli/tests/walk_registry_integration.rs` → add a smoke test using an ecosystem-appropriate fixture.

Every touched golden SBOM MUST remain byte-identical (C7).
