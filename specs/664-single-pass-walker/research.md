# Phase 0 Research: Single-Pass Walker with Reader-Registry Dispatch

**Feature**: 664-single-pass-walker
**Date**: 2026-08-21

## R1: Filename-pattern matching mechanism

**Decision**: Use `globset = "0.4"` (already a direct workspace dependency since m113 `--exclude-path` + m118 exclude-path polish + m113's `ExclusionSet` at `waybill-cli/src/scan_fs/package_db/exclude_path.rs`). Readers register `GlobSet`-compiled patterns; the registry maps `GlobMatcher` → `Vec<ReaderId>`.

**Rationale**:
- Zero new Cargo dependency per FR-010 (globset is already in the closure).
- `GlobSet::matches` is O(patterns) per file lookup, well under the SC-005 100 µs budget for the pattern counts we're dealing with (~40 patterns across ~28 readers).
- Compatible with the m113 exclude-path patterns operators already know.
- Supports the natural expressiveness readers need: `**/*.cabal`, `**/Cargo.toml`, `**/build.gradle{,.kts}`, `**/requirements*.txt`. Exact-filename patterns work as degenerate cases (`**/Cargo.lock` == "any depth, exact filename").

**Alternatives considered**:
- Hand-rolled prefix/suffix/exact-name matcher (a `HashMap<&'static str, Vec<ReaderId>>` for exact filenames + separate suffix table). Rejected: two lookup tables split the API surface; every reader that wants glob-like matching would need a case-analysis dance. Complexity outweighs the marginal cycles saved.
- `regex` alone. Rejected: readers naturally express interest as globs, not regex; forcing regex-form for every pattern hurts readability and forces every reader author to double-check regex-escaping of dots/braces.

## R2: Registry dispatch model + callback signature

**Decision**: Two callback types, both taking a `&SharedWalkerContext<'_>` handle:

```rust
// Per-file callback — fires when the shared walker visits a file whose
// filename matches one of the reader's registered patterns.
type FileCallback = fn(&Path, &SharedWalkerContext<'_>);

// Optional per-directory callback — fires once per directory the shared
// walker descends into, AFTER its full contents are indexed. Reader can
// use this for two-phase logic that must fire per-project-root rather
// than per-file (e.g., pip's "for each project root, read siblings").
type DirCallback = fn(&Path, &SharedWalkerContext<'_>);
```

Readers register a `ReaderRegistration { patterns: GlobSet, on_file: Option<FileCallback>, on_dir: Option<DirCallback>, reader_id: ReaderId }`. The dispatch loop invokes `on_file` if the pattern matches, and `on_dir` unconditionally if registered.

`SharedWalkerContext` exposes:
- `.dir_index()` → the (dir → filenames) map for sibling lookup (per m664 clarify Q1).
- `.exclude_set()` → the m113 `ExclusionSet` handle (readers may still consult it).
- `.push(reader_id, PackageDbEntry)` — the reader's output sink; the registry aggregates per-reader `Vec<PackageDbEntry>` and returns them to `read_all`.

**Rationale**:
- Function-pointer callbacks (not `Box<dyn Fn>`) keep the registry `Send`/`Sync` without RC/Arc overhead — the whole registry is a stack-allocated struct built at scan init.
- The reader-facing `&SharedWalkerContext<'_>` bundle is the ONE shared type that every reader touches; keeping it slim keeps the reader-migration diff small (see quickstart.md).
- Splitting file + dir callbacks lets two-phase readers avoid opaque state machines — pip's per-project-root logic naturally lives in `on_dir`.

**Alternatives considered**:
- Trait-based `dyn Reader` registration. Rejected: adds vtable indirection, forces `Box<dyn Reader>` at every dispatch, and the callback signature is more natural expressed as a plain function pointer. If a reader needs to own state, it holds a `thread_local!` or a `OnceLock<Mutex<T>>` — fine for the scope of a single scan.
- Registry as a `Vec<(GlobMatcher, ReaderId, FileCallback)>` scanned linearly per file. Rejected: O(readers × files) = O(N²)-ish in the worst case; the `GlobSet` composite matcher is O(patterns) already.

## R3: `ReaderId` newtype design (Principle IV compliance)

**Decision**: `ReaderId(&'static str)` — a newtype around a compile-time string constant declared per reader module.

```rust
// waybill-cli/src/scan_fs/walk_registry/mod.rs
pub struct ReaderId(&'static str);

impl ReaderId {
    pub const HASKELL: ReaderId = ReaderId("haskell");
    pub const IPK_FILE: ReaderId = ReaderId("ipk_file");
    pub const PIP: ReaderId = ReaderId("pip");
    // ... one per migrated reader
}
```

Each `ReaderId` constant is co-declared with the reader module that owns it (public via re-export from the registry) and appears in the FR-009 diagnostic log payload.

**Rationale**:
- `&'static str` avoids allocation on every dispatch.
- Enumerating constants keeps the FR-008 regression-guard trivially auditable: `git grep "ReaderId(\"" waybill-cli/` lists every reader that has migrated.
- Compile-time uniqueness is not enforced by the Rust type system for `&'static str` newtype values, but the `pub const` declaration + regression-guard grep catch every practical drift case.

**Alternatives considered**:
- `enum ReaderId { Haskell, IpkFile, ... }` in `waybill-common`. Rejected: forcing every reader to edit a central enum creates merge-conflict friction during the coexistence window, and the FR-009 log line's `Debug` format for enum variants is uglier than the string form.
- Hash-of-module-path (like `TypeId`). Rejected: opaque log values; regression-guard grep loses its target.

## R4: Directory-index representation

**Decision**: `HashMap<PathBuf, Arc<Vec<OsString>>>` — the map key is the canonicalized absolute path of the directory; the value is the sorted list of `OsString` filenames in that directory. `Arc` so the shared walker can hand the same slice to multiple readers' `on_file` callbacks without cloning the vector for each.

Sorting the filename list is essential for FR-006 byte-identity: readers observing sibling filenames may push entries in the order they iterate the list; unsorted output creates order-dependence and breaks goldens on macOS vs Linux (which have different `readdir` orderings by default).

**Rationale**:
- `PathBuf` key preserves platform-native path separators for macOS vs Linux vs Windows without normalization drift.
- `Arc<Vec<OsString>>` shares one heap allocation across all readers that touch the same directory during dispatch.
- Sorting is cheap (typical dir has < 100 entries; `sort_unstable_by` on `OsString` is nanoseconds).

**Alternatives considered**:
- `HashMap<PathBuf, Vec<OsString>>` (owned). Rejected: extra clones when multiple readers query the same dir's siblings.
- `HashMap<PathBuf, Vec<DirEntry>>`. Rejected: `DirEntry` is not `Send`+`Sync` on macOS and holds an open file descriptor; storing many in a scan-wide map risks fd exhaustion on large trees.
- `Vec<(PathBuf, Vec<OsString>)>` (sorted). Rejected: `O(log N)` binary search per lookup adds up over the ~500-directory ansible baseline; `HashMap` gives O(1) amortized.

## R5: Reader-callback panic isolation

**Decision**: Wrap each reader's `on_file` / `on_dir` invocation in `std::panic::catch_unwind` with `AssertUnwindSafe`. On panic, log via `tracing::warn!` with the reader ID and file path, then continue the dispatch loop. Panics do not abort the shared walker or any other reader's callbacks.

Matches the m209 resolver-chain `catch_unwind` pattern at `waybill-cli/src/resolve/resolver_chain.rs` (per project memory).

**Rationale**:
- Reader isolation preserves scan output for the healthy readers even if one panics on a malformed manifest.
- `AssertUnwindSafe` is safe here because the shared walker's mutable state (dir-index, per-reader entry vectors) is not observed by the callback except through the `&SharedWalkerContext<'_>` handle, and the context's interior mutability is protected (Mutex around per-reader output vecs — see R7).

**Alternatives considered**:
- Let panics abort the whole scan. Rejected: today's per-reader design already isolates panics via the sequential dispatch; the new model must not regress that.
- Return `Result` from callbacks. Rejected: `Result` for expected errors is fine, but panics happen in reader code that dereferences `Option::unwrap` or hits an unexpected file shape; wrapping them at the callback boundary is more surgical than plumbing `Result` through 40+ callback signatures.

## R6: US1 pilot-reader selection

**Decision** (revised 2026-08-21 during Phase-3 audit): US1 migrates **5 clean-shape readers** covering 11 walker call sites, drawn from the m664 profile's top-hotness list. Two originally-slated readers (pants_common, yocto/recipe) were deferred to US2 bundle migrations after the audit surfaced structural coupling.

| Reader | Walker sites (from m664 sample) | Legacy walker cost saved on ansible | US1 fit |
|---|---|---|---|
| haskell | 2 (`discover_cabal_files` + `discover_by_filename` × 4) | ~287 ms | ✓ clean-shape |
| scala | 4 (`discover_build_properties` + `discover_sbt_locks` + `discover_build_sbts` + `discover_dependencies_scala`) | ~203 ms | ✓ clean-shape |
| erlang | 3 (`discover_app_src_files` + `discover_rebar_configs` + `discover_rebar_locks`) | ~151 ms | ✓ clean-shape |
| ipk_file | 1 (`discover_ipk_files`) | ~97 ms | ✓ via `ReaderRegistration.state` extension (needed for `IpkReaderConfig` + per-scan `distro_tag`) |
| rpm_file | 1 (`discover_rpm_files`) | ~52 ms | ✓ via `ReaderRegistration.state` extension (needed for `RpmReaderConfig` + `distro_version`) |
| ~~pants_common~~ | ~~1 (`discover_build_files`)~~ | ~~~91 ms~~ | **DEFERRED to US2 pants bundle** — pants_common is a shared helper for pants_go + pants_shell + pants_jvm; migrating it alone doesn't reduce walker cost |
| ~~yocto/recipe~~ | ~~1 (`recipe::read` walker)~~ | ~~~35 ms~~ | **DEFERRED to US2 yocto bundle** — structurally coupled to `layer_conf::build_index()` + `bbappend::build_from_walk()` (US2 T059); half-migration leaves 2/3 of yocto walker cost as legacy |

**Revised total legacy cost saved: ~790 ms on the ansible baseline** (down from the originally-scoped ~916 ms).

After paying the ~120 ms shared-walker floor tax (m664 `--no-package-db` measurement), the net improvement is **~670 ms → 4.10s wall dropping to ≈3.43s**. This is a partial improvement (~17%) — the headline SC-001 (≤ 1.2s, ~3.4×) still lands at US2 when every walker-using reader has migrated.

**Also required**: the per-reader-state API extension (`ReaderRegistration.state: Option<Arc<dyn Any + Send + Sync>>` + `SharedWalkerContext::state::<T>()` accessor) shipped as a Phase-2 hotfix during the 2026-08-21 audit. Without this, ipk_file + rpm_file couldn't fit into US1.

**Rationale for the trimmed pilot**:
- The 5 clean-shape readers give ~86% of the originally-scoped pilot's savings with substantially lower migration complexity + zero cross-reader coordination.
- Deferring pants_common + yocto/recipe avoids "half-migrated" states where a reader's walker migrates but its sibling walkers (owned by helper modules) don't.
- The US2 bundles that pants_common and yocto/recipe move into (pants trio + yocto trio) are natural units — migrating them together is less total work than doing 2 half-migrations plus a US2 catchup.

**Alternatives considered**:
- "Just haskell as the sole US1 pilot." Rejected: 287 ms saved − 120 ms tax = 167 ms net improvement (4.10s → 3.93s), only 4% better than baseline. Doesn't validate the win convincingly.
- "All 20+ walker-using readers in US1." Rejected: that's the entire migration; US1 becomes the whole milestone with no incremental validation opportunity.
- "Original 7-reader pilot as planned." Rejected during Phase-3 audit: 2 of the 7 have hidden structural dependencies that make single-reader migration counterproductive.

## R7: Per-reader output sink (interior mutability)

**Decision**: `SharedWalkerContext` holds a `HashMap<ReaderId, Mutex<Vec<PackageDbEntry>>>` — one output vector per registered reader, guarded by a `Mutex` so readers can push during dispatch without cross-reader coordination. Post-walk, `SharedWalker::finish()` consumes the map and returns `HashMap<ReaderId, Vec<PackageDbEntry>>` to `read_all`.

Since FR-012 forbids reader parallelism in this milestone, contention on the `Mutex` is zero (only the main thread pushes). The `Mutex` is a future-proofing mechanism for when FR-012 gets lifted in a follow-on; it costs ~10 ns per lock/unlock, well under the SC-005 budget.

**Rationale**:
- Isolating each reader's output vector by `ReaderId` gives natural per-reader aggregation without an "envelope" data type.
- The `Mutex` is a placeholder for future parallelism per the follow-on note in FR-012; costs nothing today.

**Alternatives considered**:
- `RefCell<Vec<PackageDbEntry>>`. Rejected: `RefCell` panics on concurrent borrow; the moment FR-012 is lifted this becomes a bug.
- Return callbacks that yield `Vec<PackageDbEntry>` from `on_file`. Rejected: many readers accumulate across multiple files (e.g., haskell walks 5+ paths and unions the results) — a per-callback return value forces a stateful aggregator anyway.

## R8: Dispatch order determinism

**Decision**: Reader dispatch order for a given file is **deterministic** and follows registration order. When two readers register overlapping patterns and both match the same file, the reader that registered first is dispatched first.

Registration order is fixed by `read_all`'s explicit `registry.register(...)` call sequence — a stable list matching the existing `read_all` reader call ordering in `waybill-cli/src/scan_fs/package_db/mod.rs`.

**Rationale**:
- Byte-identity of goldens (FR-006) is guaranteed by per-reader output aggregation, but downstream code paths could observe shared state (e.g., the dedup helpers in `scan_fs/mod.rs`); stable dispatch order removes an entire class of "worked in dev, broke in CI" bugs.
- Zero runtime cost — the registry stores registrations in a `Vec` and iterates them in insertion order.

**Alternatives considered**:
- `HashMap` iteration order (non-deterministic across runs). Rejected: even with FR-006 goldens catching output drift, silent order-dependence in intermediate state is a debugging nightmare.
- Alphabetical order by `ReaderId`. Rejected: doesn't align with the existing `read_all` ordering; churn without value.

## R9: FR-009 diagnostic log shape

**Decision**: One INFO-level log line, emitted at the end of `SharedWalker::run()`:

```
2026-08-21T20:46:29Z INFO waybill::scan_fs::walk_registry: shared walker completed
  passes=1
  files_visited=5793
  dirs_visited=487
  registered_readers=7
  per_reader_dispatch_counts={"haskell": 12, "scala": 8, "erlang": 6, "ipk_file": 0, "pants_common": 3, "rpm_file": 0, "yocto/recipe": 15}
  wall_ms=127
```

The per-reader dispatch counts double as the FR-008 regression-guard signal: a new reader that walks outside the registry would show up as `registered_readers` unchanged but `wall_ms` inflated on baseline suites.

**Rationale**:
- One line, structured, `tracing::info!` — matches the m055 / m112 / m160 / m173 diagnostic-log pattern.
- Field ordering is stable so operator scripts / CI regression tests can grep-and-parse without JSON dependencies.

**Alternatives considered**:
- Emit the same data as a per-reader `debug!` line each. Rejected: noisy; also harder to write regression assertions against.
- Structured JSON via `tracing_serde`. Rejected: adds `tracing-serde` as a dep (violates FR-010); the readable format above is enough.

## R10: FR-008 regression-guard enforcement mechanism

**Decision**: Shell-based CI grep guard, matching the m117 line-stable-allowlist precedent.

At `.github/workflows/ci.yml` add a small step that runs:

```bash
git grep -nE 'safe_walk\(' waybill-cli/src/scan_fs/package_db/ \
  | sort > .actual-safe-walk-callers.txt
diff .actual-safe-walk-callers.txt \
  waybill-cli/src/scan_fs/walk.audit-allowlist.txt \
  || (echo "New safe_walk caller detected outside shared registry. See specs/664-single-pass-walker/spec.md FR-008." && exit 1)
```

The allowlist file `waybill-cli/src/scan_fs/walk.audit-allowlist.txt` already exists per m117 (project memory `feedback_walker_audit_local_check`). This milestone extends it with the coexistence-window entries (readers not yet migrated in US1/US2) plus the FR-005 npm-inner permanent entry.

**Rationale**:
- Zero new tools installed on CI runners (grep, sort, diff are POSIX).
- The allowlist is a git-tracked plain-text file; adding a reader before the shared registry lands requires an allowlist edit, making the intent visible in code review.
- Per m117, this pattern already works locally and in CI without flake.

**Alternatives considered**:
- Rust-level macro-based deny (a `#[deprecated_since_664]` attribute on `safe_walk`). Rejected: `#[deprecated]` warns but doesn't fail CI without `-D warnings` at the workspace level (which m211 already turns on for clippy but not for build — expanding that scope is out of the milestone's scope).
- Runtime assertion (log a warn if `safe_walk` is called outside the registry). Rejected: runtime detection is too late; the guard needs to fire at CI time.

## R11: Interaction with the m113 `ExclusionSet`

**Decision**: The shared walker consults the same `ExclusionSet` handle that individual readers use today. Every reader migration MUST verify that its previous `WalkConfig { exclude_set, ... }` is threaded through into the registration call unchanged. The `SharedWalkerContext::exclude_set()` accessor exists so callbacks can still consult exclusions if they need to make finer-grained decisions on non-matching-but-still-visited paths.

**Rationale**:
- Preserves the m113 operator contract: a path excluded via `--exclude-path` is not visited, whether by the shared walker or any legacy walker.
- Byte-identity of exclusion-touching goldens (there are ~5) is protected.

**Alternatives considered**:
- Have the shared walker own its own exclusion list separate from the reader-level exclusions. Rejected: two configuration sources for the same operator intent; recipe for drift.

## R12: FR-011 CLI/SBOM-field invariance verification

**Decision**: The `cargo +stable test --workspace` gate is the FR-011 enforcement mechanism — every golden SBOM test MUST pass before each per-reader migration PR merges. Additionally, the CI walker-audit step (R10) verifies no new CLI-flag drift.

No new CLI flags added by this milestone. No new SBOM fields, annotations, or schema changes. FR-009 log line is `tracing::info!`, external to the SBOM.

**Rationale**:
- The existing ~1000+ golden SBOM tests are the authoritative FR-011 check.
- Adding a new spec-level assertion is unnecessary; the existing test surface catches it.

**Alternatives considered**:
- Explicit "no new CLI flag" grep in CI. Rejected: overkill; a new flag would show up in `waybill sbom scan --help` and be caught by human review + clippy's unused-argument warnings.

## Consolidated open items

None. All 12 research items resolved; no `[NEEDS CLARIFICATION]` markers remain.

## Cross-references

- Constitution: Principles I, IV, V, VI, VII, VIII, IX, X all evaluated in `plan.md` → Constitution Check.
- m664 diagnostic session baselines: ansible 4.10s / pytorch 4.30s / mongodb 15.68s (all offline mode; macOS APFS release build; measurements captured on 2026-08-21).
- Existing precedents this plan follows: m054 safe_walk foundation; m113 `ExclusionSet`; m117 walker-audit CI guard; m209 `catch_unwind` reader panic isolation; m211 walk-audit allowlist.
