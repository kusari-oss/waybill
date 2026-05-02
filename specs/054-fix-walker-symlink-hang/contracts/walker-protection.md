# Contract: per-walker symlink-loop protection

This contract defines the exact protection invariants every filesystem walker in `mikebom-cli/src/scan_fs/` MUST satisfy post-054. Reviewers verify these via grep audit + unit tests; tasks reference this document for "what does a protected walker look like."

## Mandatory invariants

Every `fn walk_*` (or `fn walk_dir`) function in `mikebom-cli/src/scan_fs/` MUST:

### Invariant 1: Canonicalize-keyed visited-path set

The walker MUST maintain a `HashSet<PathBuf>` keyed by `std::fs::canonicalize` output, scoped to a single walker invocation. Membership check MUST happen BEFORE recursion, not after.

```rust
// Reference shape (from golang.rs:1162-1167 + project_roots.rs):
let key = std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.to_path_buf());
if !visited.insert(key) {
    tracing::debug!(path = %candidate.display(), "walker: cycle/visited skip");
    continue;
}
walk_inner(&candidate, depth + 1, /*…*/, visited);
```

### Invariant 2: Max-depth backstop

The walker MUST honor `const MAX_WALK_DEPTH: usize = 16;` (or equivalent). Crossing the depth ceiling MUST emit `tracing::debug!` and return without recursing.

```rust
const MAX_WALK_DEPTH: usize = 16;

fn walk_inner(dir: &Path, depth: usize, visited: &mut HashSet<PathBuf>, /*…*/) {
    if depth >= MAX_WALK_DEPTH {
        tracing::debug!(depth, path = %dir.display(), "walker: max-depth reached");
        return;
    }
    // ...
}
```

### Invariant 3: Cycle-detection observability

Cycle detection MUST emit `tracing::debug!` (NOT `info` or `warn`) naming the canonical path. Default-log scans (`tracing::Level::INFO`) MUST emit zero loop-detection chatter on legitimate trees; debug-level scans surface the cycles when investigating bug reports.

### Invariant 4: Robustness on transient failures

The walker MUST tolerate without crashing:

| Condition | Expected behavior |
|-----------|-------------------|
| Broken symlink (target doesn't exist) | `path.is_dir()` returns false → walker skips silently |
| EACCES on parent component during canonicalize | fallback to `path.to_path_buf()` as visited-set key |
| `read_dir` returns Err on subdir | `let Ok(entries) = … else { return; }` — preserve existing behavior |
| Permission-denied subdirectory | walker skips silently (existing behavior preserved) |

### Invariant 5: No false positives on legitimate symlinked trees

A test fixture with symlinks used for actual content (e.g., a `vendor/` mirroring an upstream tree via symlinks) MUST be processed exactly once per canonical path. The walker MUST NOT double-count a file reachable via two symlink paths to the same on-disk inode.

Verified by: a synthesized fixture with `dirA/file.txt` + `dirB/link -> ../dirA` MUST produce a single emission for `file.txt`.

## Audit rubric (PR-review-time check)

`grep -rn "fn walk" mikebom-cli/src/scan_fs/` at PR-review time MUST find every match either:

(a) Carrying a visible `HashSet<PathBuf>` parameter or local creation, OR

(b) Carrying an inline `// SAFETY:` comment explicitly justifying the deviation. Acceptable justifications include "bounded-by-construction: only iterates a finite hardcoded list" or "delegates to safe-walk helper from issue #108."

Any walker matching neither is a blocking review finding.

## Per-walker patch shape

For walkers that already have a `(depth: usize)` signature, the patch is uniform:

```rust
// Before
fn walk_for_<thing>(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth >= MAX_<...>_DEPTH { return; }
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // ... skip-list checks ...
            walk_for_<thing>(&path, depth + 1, out);
        }
        // ...
    }
}

// After
fn walk_for_<thing>(
    dir: &Path,
    depth: usize,
    out: &mut Vec<PathBuf>,
    visited: &mut HashSet<PathBuf>,    // ⬅️ NEW PARAM
) {
    if depth >= MAX_<...>_DEPTH { return; }
    let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(key) {
        tracing::debug!(path = %dir.display(), "walker: cycle/visited skip");
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // ... skip-list checks ...
            walk_for_<thing>(&path, depth + 1, out, visited);  // ⬅️ thread visited
        }
        // ...
    }
}

// Top-level entry creates the set:
pub(crate) fn discover_<thing>(rootfs: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut visited = HashSet::new();
    walk_for_<thing>(rootfs, 0, &mut out, &mut visited);
    out
}
```

For walkers that lack a depth parameter (`rpm_file::walk_dir`, `binary::discover::walk_dir`):

```rust
// Before — no depth, no visited
fn walk_dir(dir: &Path, acc: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // ...
            walk_dir(&path, acc);
        }
        // ...
    }
}

// After — add depth + visited; introduce MAX_WALK_DEPTH const
const MAX_WALK_DEPTH: usize = 16;

fn walk_dir(dir: &Path, depth: usize, visited: &mut HashSet<PathBuf>, acc: &mut Vec<PathBuf>) {
    if depth >= MAX_WALK_DEPTH { return; }
    let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(key) {
        tracing::debug!(path = %dir.display(), "walker: cycle/visited skip");
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // ...
            walk_dir(&path, depth + 1, visited, acc);
        }
        // ...
    }
}
```

## Realistic-project CI contract

The new `.github/workflows/realistic-projects.yml` MUST satisfy:

### Trigger

- `pull_request` (every PR)
- `push: branches: [main]` (post-merge regression check)
- `workflow_dispatch` (manual rerun)

### Job structure

- Matrix-driven over `RealisticProjectFixture` entries (initial: knative/func; expand in task generation).
- One job per project per platform (`{linux-x86_64, macos-latest}`).
- Each job: clone (with `actions/cache@v4`) → build mikebom → scan → schema-validate → assert component floor.

### Cache key

```yaml
- uses: actions/cache@v4
  with:
    path: /tmp/realistic-fixtures/${{ matrix.project.name }}-${{ matrix.project.tag }}
    key: realistic-fixture-${{ matrix.project.name }}-${{ matrix.project.tag }}
```

### Failure mode

- Clone fails → CI rerun fixes (don't silently skip).
- Scan exceeds `max_seconds_<platform>` → fail with project name + scan duration.
- Scan exits non-zero → fail with project name + stderr tail.
- Schema validation fails → fail with project name + validator output.
- Component floor unmet → fail with project name + count + expected floor.

### Independent re-runnability

The new workflow MUST NOT block the main pre-PR `ci.yml` lanes — it runs in parallel. A flake in `realistic-projects.yml` is independently re-runnable via `gh run rerun --failed`.

## Cross-walker invariants

These hold across all 9 walkers:

1. **Identical visited-set semantics**: every walker keys by `canonicalize(path).unwrap_or_else(|_| path.to_path_buf())`. No walker invents its own keying scheme.
2. **Identical max-depth value**: every walker uses 16 (or accepts a justification comment if its existing const is intentionally tighter, e.g., `cargo.rs:45 MAX_PROJECT_ROOT_DEPTH = 6` because cargo workspaces are shallow by convention).
3. **Identical debug-log shape**: cycle-detection emits `tracing::debug!(path = %dir.display(), "walker: cycle/visited skip");` — exact format string so downstream log filters can grep for it.
4. **Test parity**: every walker's `#[cfg(test)] mod tests` includes a `walks_symlink_loop_without_hanging` test using a synthesized minimal `tmpdir/loop/link -> .` fixture.

## Out of contract

- The migration to a single shared `safe_walk` helper (issue #108): future work; milestone 054 stays per-walker.
- Performance optimization for pathological inputs (millions of files): not in scope.
- Windows symlink semantics: mikebom doesn't support Windows.
- LICENSE-file detection on the main-module (issue #103): unrelated; separate scope.
