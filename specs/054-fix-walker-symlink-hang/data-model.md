# Data Model: milestone 054 (walker symlink-loop fix)

## New & changed entities

### `VisitedPathSet` (per-walker, in-memory)

Each walker maintains an instance scoped to a single invocation:

```rust
let mut visited: HashSet<PathBuf> = HashSet::new();
```

Insert keys via:

```rust
let key = std::fs::canonicalize(&candidate)
    .unwrap_or_else(|_| candidate.to_path_buf());
if !visited.insert(key) {
    continue; // already-visited canonical dir — skip recursion
}
```

Pattern matches `golang.rs:1162-1167` and `project_roots.rs:51-69`.

**Field semantics**:

| Field | Type | Behavior |
|-------|------|----------|
| Set storage | `HashSet<PathBuf>` | per-walker-invocation; cleared between scans |
| Key | `PathBuf` (canonicalized) | unique on-disk identity for each directory |
| `canonicalize` failure fallback | `path.to_path_buf()` | broken symlinks / EACCES on parent component → fall back to lexical path so the walker doesn't block on transient lookup failures |

**Validation rules**:
- The set MUST be created fresh per top-level walker entry (no cross-scan persistence — the same project scanned twice shouldn't share state).
- Insert MUST happen BEFORE the recursive call, not after — otherwise a parent `walk_dir(./loop)` returns to its caller after recursing into `./loop/.` first, creating a redundant first-pass.
- The fallback `unwrap_or_else(|_| candidate.to_path_buf())` is mandatory; bare `unwrap()` is forbidden by Constitution Principle IV.

### `MAX_WALK_DEPTH` const (per-walker)

Defense-in-depth backstop:

```rust
const MAX_WALK_DEPTH: usize = 16;
```

**Behavior**:

| Condition | Action |
|-----------|--------|
| Recursion depth `< MAX_WALK_DEPTH` | Recurse normally |
| Recursion depth `>= MAX_WALK_DEPTH` | `tracing::debug!` breadcrumb naming the path; return without recursing |

**Why per-walker (not workspace-wide)**:
- Matches existing per-walker pattern (`cargo.rs:45 const MAX_PROJECT_ROOT_DEPTH: usize = 6;`).
- Issue #108's eventual single-helper migration is the natural place to introduce a shared const.
- Each walker self-contained: drop-in patch with no cross-file coordination required.

### `RealisticProjectFixture` (CI-only, not Rust)

GitHub Actions workflow matrix entry:

```yaml
matrix:
  project:
    - name: knative-func
      url: https://github.com/knative/func.git
      tag: knative-v1.22.0
      expected_min_components: 200      # SC-007 floor for the SBOM
      schemas: [spdx-2.3-json, cyclonedx-json, spdx-3-json]
      max_seconds_linux: 300
      max_seconds_macos: 600
    # Additional fixtures decided in Phase 2 task generation.
```

**Validation gates** (each fixture entry MUST satisfy):

| Gate | Pass criteria |
|------|---------------|
| Clone duration | `git clone --depth 1 --branch <tag>` completes within 30s on linux / 60s on macos |
| Scan duration | `mikebom sbom scan --path <fixture> --offline --no-deep-hash` completes within `max_seconds_<platform>` |
| Exit code | mikebom exits 0 |
| SBOM validity | Each emitted format validates against its schema (existing `tests/spdx3_schema_validation.rs`-style validator for SPDX 3; jq/manual structure check for CDX + SPDX 2.3) |
| Component floor | `pkg:golang` count ≥ `expected_min_components` (smoke gate against silent regression) |

## Relationships

```text
┌─────────────────────────┐    walks      ┌──────────────────────────┐
│ <Walker fn>             │ ────────────▶ │ filesystem subtree       │
│  scan_fs/package_db/    │               │  (potentially with cycles)│
│  scan_fs/binary/        │               └──────────────────────────┘
└─────────────────────────┘                          │
        │                                            │ canonicalize
        ▼                                            ▼
┌─────────────────────────┐               ┌──────────────────────────┐
│ VisitedPathSet          │ ◀──── checks ─│ canonical PathBuf        │
│  HashSet<PathBuf>       │               └──────────────────────────┘
│  per-invocation         │
└─────────────────────────┘
        │
        │ depth tracked separately
        ▼
┌─────────────────────────┐
│ MAX_WALK_DEPTH = 16     │
│  per-walker const       │
│  hard ceiling on stack  │
└─────────────────────────┘
```

## State transitions

None. The walker's state is build-up-then-discard:
1. Create empty `VisitedPathSet` at top-level walker entry.
2. For each candidate dir: canonicalize → check membership → insert if new → recurse if depth < ceiling.
3. Return when subtree exhausted; set falls out of scope, freed.

No persistent state, no cross-scan coupling.

## Validation against constitution

- **Principle IV (Type-Driven Correctness)**: production code uses `Result` for `canonicalize`, `Option` for `path.file_name`. Falls back to `to_path_buf()` on canonicalize failure (transient — broken symlink, EACCES). `.unwrap()` only inside `#[cfg(test)]` modules.
- **Principle VIII (Completeness)**: visited-set ensures every reachable canonical dir is enumerated exactly once. Pre-054 the unprotected walkers either hung (zero output) or terminated by depth limit (incomplete output). Post-054 every reachable directory is visited; the canonical-key dedup means no double-counting either.
- **Principle X (Transparency)**: `tracing::debug!` on cycle detection (FR-008) makes the behavior observable in default-log scans of legitimate trees (zero noise) and in pathological trees (one line per cycle).
