# Contract: `requirements*.txt` reader

**File**: `waybill-cli/src/scan_fs/package_db/pip/requirements_txt.rs` + `pip/req_scope_heuristic.rs`
**FRs covered**: FR-004 (extended venv-pruning), FR-005, FR-005a (scope heuristic), FR-005b (direct-URL), FR-013, FR-014, FR-015, FR-016
**Called by**: `pip/mod.rs::dispatch` when a `requirements*.txt` is discovered

## Input

- `path: &Path` — absolute path to a `requirements*.txt`
- `walker_context: &SharedWalkerContext`

## Discovery contract

The reader itself does NOT perform the walk — the m664 shared-walker registry finds candidate files via a glob pattern:

```
requirements*.txt
```

Descent into candidate directories is gated by `venv_prune.rs::should_prune(path)` per FR-004 extended:

```rust
pub(crate) const PYTHON_VENDORED_DIRS: &[&str] = &[
    "site-packages", ".venv", "venv", ".tox", "build", ".eggs",
];
pub(crate) fn is_egg_info(name: &str) -> bool { name.ends_with(".egg-info") }
```

The `--include-python-vendored` CLI flag disables this list for the scan.

## Recursive `-r` handling

Requirements files can reference other requirements files via `-r other-file.txt`. The reader recursively parses referenced files with:
- **Bounded depth**: max recursion depth 10
- **Cycle detection**: `HashSet<PathBuf>` of already-visited canonicalized paths
- **Sibling-relative resolution**: `-r other.txt` in `docs/req.txt` resolves to `docs/other.txt` (not scan-root)

Cycles → `tracing::warn!("skipping requirement-file cycle {} -> {}", parent, target)`, do not re-parse.

## Line-by-line parsing

The PEP 508 line grammar is fully specified. Reader dispatches per line-shape:

| Shape | Example | Emission |
|-------|---------|----------|
| Pinned | `requests==2.31.0` | `pkg:pypi/requests@2.31.0` |
| Constrained | `requests>=2.28,<3` | `pkg:pypi/requests@unresolved` + `waybill:version-constraint = ">=2.28,<3"` |
| Unpinned | `requests` | `pkg:pypi/requests@unresolved` + `waybill:unresolved-reason = "python-requirements-txt-unpinned"` |
| With marker | `requests==2.31.0 ; python_version >= '3.10'` | `pkg:pypi/requests@2.31.0` + `waybill:pep508-marker` |
| With extras | `requests[security]==2.31.0` | `pkg:pypi/requests@2.31.0` + `waybill:python-extras = ["security"]` |
| Direct URL | `requests @ git+https://github.com/psf/requests@v2.31` | `pkg:pypi/requests@v2.31` + `waybill:direct-url-source` |
| Editable | `-e .` | Skip (m064 main-module already emits) |
| Editable git | `-e git+https://github.com/psf/requests` | `pkg:pypi/requests@unresolved` + `waybill:unresolved-reason = "python-editable-install"` + `waybill:direct-url-source` |
| Recurse | `-r other.txt` | Recursive parse; no direct emission |
| Constraint | `-c constraints.txt` | Skip (out of scope per spec) — `tracing::debug!` |
| Index URL | `--index-url https://...` | Attach `waybill:index-url` to subsequent entries in this file |
| Comment | `# foo` | Skip |
| Blank | `` | Skip |
| Unknown | `????` | `tracing::warn!("unparseable line at {}:{}", path, lineno)`, skip |

## Scope-tag derivation (FR-005a)

Delegated to `req_scope_heuristic.rs::classify(path)`. Algorithm:

```rust
pub(crate) enum RequirementsScope {
    Main,
    Optional(&'static str),  // "dev", "test", "docs", "ci"
}

pub(crate) fn classify(path: &Path) -> RequirementsScope {
    // 1. Parent-dir signal (highest priority)
    if let Some(parent) = path.parent().and_then(|p| p.file_name()) {
        match parent.to_str().unwrap_or("") {
            "docs" | "doc" | "documentation" => return Optional("docs"),
            "test" | "tests" => return Optional("test"),
            "ci" | ".ci" => return Optional("ci"),
            _ => {}
        }
    }
    // 2. Filename signal
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.contains("dev") { return Optional("dev"); }
    if name.contains("test") { return Optional("test"); }
    if name.contains("docs") || name.contains("doc") { return Optional("docs"); }
    if name.contains("ci") { return Optional("ci"); }
    // 3. Default
    Main
}
```

The result is attached as `waybill:python-req-file-scope = "<derived-name>"` on each emitted component from that file.

## Annotations emitted

Same catalog rows as pyproject reader (C154–C157), plus:
- **C158** `waybill:python-req-file-scope` — new; `SymmetricEqual`; carries the derived scope name

## Test coverage

Unit tests in `pip/requirements_txt.rs #[cfg(test)] mod tests`:
- `parses_pinned_lines`
- `parses_constrained_with_version_constraint_annotation`
- `parses_unpinned_with_reason`
- `parses_pep508_marker`
- `parses_extras`
- `parses_direct_url_git`
- `parses_editable_local` (should skip)
- `parses_editable_git` (should emit with reason + direct-url)
- `recurses_dash_r`
- `detects_dash_r_cycle`
- `respects_bounded_recursion_depth_10`
- `parses_comments_and_blanks`
- `warns_on_unparseable_line`

Unit tests in `pip/req_scope_heuristic.rs`:
- Parent-dir signal: `docs/`, `tests/`, `ci/`
- Filename signal: `dev-requirements.txt`, `requirements-test.txt`
- Precedence: parent-dir wins over filename
- Default → Main
