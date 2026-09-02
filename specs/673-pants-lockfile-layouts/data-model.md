# Phase 1 Data Model — m673 Pants lockfile discovery layout extensions

**Date**: 2026-09-02

## Overview

Three concrete data-shape changes inside `waybill-cli/src/scan_fs/package_db/pants/`:

1. `DiscoverySource` enum (m672) gains TWO new variants: `RepoRootGlob` + `LockfilesGlob`.
2. `discover_lockfiles` in `mod.rs` gets two new discovery loops that produce these variants.
3. `lockfile.rs` gets a new pure function `is_pex_lockfile_content(bytes: &[u8]) -> bool` used as the content-detect gate for the wide-scope FR-001/FR-002 paths.

No SBOM wire-format changes. No parity-catalog changes. No new persistent state. Matches every reader milestone since m002.

---

## Enum 1: `DiscoverySource` (extended)

**File**: `waybill-cli/src/scan_fs/package_db/pants/mod.rs`

**Pre-m673 shape (from m672)**:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscoverySource {
    DefaultGlob,               // 3rdparty/python/*.lock (m223)
    PythonLockfileSingular,    // pants.toml [python].lockfile (m223)
    PythonResolvesMap,         // pants.toml [python.resolves] map (m672)
}
```

**Post-m673 shape**:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscoverySource {
    DefaultGlob,               // 3rdparty/python/*.lock (m223)
    PythonLockfileSingular,    // pants.toml [python].lockfile (m223)
    PythonResolvesMap,         // pants.toml [python.resolves] map (m672)
    /// Milestone 673: <repo-root>/*.lock file that passed the
    /// FR-003 content-detection gate. Resolve name derived from
    /// `path.file_stem()`. Wide-scope path — parse failures
    /// silent-skip per FR-004 (not m223 WARN-and-skip).
    RepoRootGlob,
    /// Milestone 673: <repo-root>/lockfiles/*.lock file that passed
    /// the FR-003 content-detection gate. Resolve name derived from
    /// `path.file_stem()`. Wide-scope path — parse failures
    /// silent-skip per FR-004.
    LockfilesGlob,
}
```

**Winner-selection rule** (extends m672's `dedup_by_canonical_path` verbatim per research.md §R4):

```
Precedence order (highest → lowest):
  PythonResolvesMap   > PythonLockfileSingular
                     > {DefaultGlob, RepoRootGlob, LockfilesGlob}   ← tied peers
```

When a collision group's members are all in the tied-peer set, the
existing lex-min `resolve_name` tie-breaker fires. In practice, the
three peer directories don't physically overlap (a file can't sit
under `3rdparty/python/` AND at repo-root AND under `lockfiles/`
simultaneously), so peer-vs-peer ties should be rare — but the
deterministic tie-break makes the code trivial to reason about.

---

## Enum 2: `WarnPolicy` (implicit — encoded by DiscoverySource pattern-match)

**File**: `waybill-cli/src/scan_fs/package_db/pants/mod.rs`

Per FR-004, per FR-003 clarification, per the 2026-09-02 clarify Q1:

- **`DefaultGlob` + `PythonLockfileSingular` + `PythonResolvesMap`** → m223 WARN-and-skip on parse failure (narrow-scope, Pants-owned locations).
- **`RepoRootGlob` + `LockfilesGlob`** → silent-skip on FR-003 content-detect failure (wide-scope, non-Pants files commonly present).

This is enforced by early-exit in the wide-scope discovery loops:

```rust
// (pseudocode inside discover_lockfiles)
for path in read_dir_lock_files(&repo_root_glob_dir) {
    let bytes = std::fs::read(&path).ok()?;
    if !is_pex_lockfile_content(&bytes) {
        continue;  // FR-004 silent-skip; no WARN, no counter increment
    }
    push_candidate(path, RepoRootGlob);
}
```

The content-detect gate runs BEFORE `push_candidate` — so the file
never reaches the m223 `lockfile::parse` → `serde_json::from_slice::<PexLockfile>`
codepath. This means:

- Non-PEX `.lock` files in FR-001/FR-002 paths trigger ZERO WARN
  log lines from the Pants reader.
- The FR-003 content-detect cost is paid once per candidate; the
  full-schema parse (m223's `serde_json::from_slice::<PexLockfile>`)
  runs only on files that already look PEX-shaped.

---

## Function 1: `is_pex_lockfile_content` (new pure function)

**File**: `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs`

```rust
/// Milestone 673 FR-003: content-detect a `.lock` file as a valid
/// PEX lockfile by checking for `pex_version: "2.x"` at the JSON
/// top level. Used as the gate for the wide-scope FR-001/FR-002
/// discovery paths per the 2026-09-02 clarify Q1 — narrow-scope
/// paths (m223 3rdparty/python/*.lock + m672 explicit overrides)
/// retain m223's attempt-full-parse-and-WARN semantics.
///
/// Steps:
/// 1. Strip `//`-frontmatter (m672 `strip_pants_frontmatter`).
/// 2. Parse as `serde_json::Value` (permissive top-level JSON).
/// 3. Return `obj["pex_version"].as_str().is_some_and(|s| s.starts_with("2."))`.
///
/// Returns `false` on any parse failure or missing/wrong-version
/// field — caller silent-skips (FR-004).
///
/// Pure function — no allocation beyond the parse-buffer, no error
/// path (returns `bool`), no persistent state.
///
/// Complexity: O(file-size) linear parse. Sub-millisecond on TOML/
/// non-JSON rejects (parse errors early); < 5 ms on real PEX
/// shapes (200 KB average).
pub(crate) fn is_pex_lockfile_content(bytes: &[u8]) -> bool {
    let body = strip_pants_frontmatter(bytes);
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return false;
    };
    value
        .get("pex_version")
        .and_then(|v| v.as_str())
        .is_some_and(|s| s.starts_with("2."))
}
```

**Test matrix** (from `contracts/content_detection.md`):

| Input | Expected `is_pex_lockfile_content` return | Contract clause |
|---|---|---|
| Valid PEX 2.10 clean-JSON lockfile | `true` | C1 accept |
| Valid PEX 2.10 with `//`-frontmatter | `true` | C1 + m672 stripper interop |
| Valid PEX 1.9 lockfile (`pex_version = "1.9.0"`) | `false` | C2 version-gate |
| Cargo lockfile (TOML) | `false` | C3 non-JSON reject |
| Poetry lockfile (TOML) | `false` | C3 non-JSON reject |
| Bun lockfile (JSONC — starts with valid JSON, has comments) | `false` (parse fails at comment) | C3 non-JSON reject |
| Empty file (`&[]`) | `false` | C4 empty-input reject |
| Valid JSON without `pex_version` field | `false` | C5 missing-field reject |
| Valid JSON with `pex_version: 2` (integer, not string) | `false` | C5 wrong-type reject |
| Valid JSON with `pex_version: null` | `false` | C5 wrong-type reject |

---

## Function 2: `discover_lockfiles` (extended)

**File**: `waybill-cli/src/scan_fs/package_db/pants/mod.rs`

**Pre-m673 shape (from m672)**: enumerates candidates from three sources:
1. `3rdparty/python/*.lock` (m223 default glob).
2. `pants.toml` `[python].lockfile` singular string.
3. `pants.toml` `[python.resolves]` bare-string map.
Then runs `dedup_by_canonical_path`.

**Post-m673 shape**: enumerates candidates from FIVE sources:
1. `3rdparty/python/*.lock` (m223 default glob).
2. `pants.toml` `[python].lockfile` singular string.
3. `pants.toml` `[python.resolves]` bare-string map.
4. **NEW**: `<repo-root>/*.lock` filtered by `is_pex_lockfile_content` — non-recursive.
5. **NEW**: `<repo-root>/lockfiles/*.lock` filtered by `is_pex_lockfile_content` — non-recursive.

The final `dedup_by_canonical_path` pass is unchanged from m672.

**Signal detection** (FR-006): the m672 `pants_signal_present`
computation at `pants/mod.rs::read` is extended to include two more
signals:
- `<repo-root>/lockfiles/` exists (regardless of contents).
- At least one repo-root `.lock` file passes content-detection.

The signal is used to gate the zero-discovered diagnostic INFO log
(m672 US3). Adding these signals means Pants monorepos using ONLY
the m673 layouts (without `pants.toml` OR `3rdparty/python/`) still
get a helpful hint when discovery finds nothing usable.

---

## Data-flow diagram

```
                   ┌───────────────────────────────────────┐
                   │        discover_lockfiles(root)       │
                   └───────────────────────────────────────┘
                                     │
        ┌────────────┬───────────────┼───────────────┬────────────┐
        ▼            ▼               ▼               ▼            ▼
   3rdparty/    [python]        [python.        NEW: root/    NEW: root/
   python/*.    .lockfile       resolves]       *.lock        lockfiles/
   lock         singular        map                           *.lock
        │            │               │               │            │
        │            │               │               ▼            ▼
        │            │               │        is_pex_lockfile_content?
        │            │               │           │           │
        │            │               │           yes         no → silent skip
        │            │               │           │           (FR-004)
        │            │               │           ▼           
   DefaultGlob  PythonLock-   PythonResolvesMap  RepoRootGlob  LockfilesGlob
                fileSingular
        │            │               │               │            │
        └────────────┴───────┬───────┴───────────────┴────────────┘
                             │
                             ▼
                ┌───────────────────────────────────────┐
                │       dedup_by_canonical_path         │  (m672)
                └───────────────────────────────────────┘
                             │
                             ▼
                     Vec<DiscoveredLockfile>
                             │
                             ▼
                (m223 parse-with-WARN loop — unchanged)
```

---

## Non-goals

- **No new `Serialize` impls** on any struct — everything is
  deserialize-only or in-memory state.
- **No new `Clone` derives** beyond the existing m672 shape.
- **No new public API** — the two new `DiscoverySource` variants are
  `pub(crate)` internal enum extensions; `is_pex_lockfile_content` is
  `pub(crate)` for the tests in `pants/mod.rs`.
- **No recursion under `lockfiles/`** (FR-009).
