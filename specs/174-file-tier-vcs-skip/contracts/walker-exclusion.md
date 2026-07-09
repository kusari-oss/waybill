# Contract: File-tier walker VCS metadata exclusion (m174)

**Feature**: 174-file-tier-vcs-skip
**Date**: 2026-07-08

Authoritative behavior contract for the file-tier walker's VCS metadata exclusion. Deviations are grounds for review comment.

## Directory-descend gate

**Contract**: given a candidate directory path `<abs>`, the file-tier walker's `should_skip` closure returns `true` iff `<abs>.file_name()` decodes to a UTF-8 string exactly equal to one of `.git`, `.hg`, `.svn`. Returning `true` suppresses descent — the walker does not `readdir()` the excluded directory, does not visit its child entries, does not descend to any depth beneath it.

**Positive matches** (excluded, walker returns without descending):
- `<root>/.git`
- `<root>/subdir/.git`
- `<root>/a/b/c/d/.git`
- `<root>/.hg`
- `<root>/.svn`

**Negative matches** (walker descends normally):
- `<root>/.github` (five chars, not three)
- `<root>/.githooks` (nine chars)
- `<root>/.gitignore-fixtures` (not exact `.git`)
- `<root>/git` (no leading dot)
- `<root>/.GIT` (case difference — see Assumptions #3)
- `<root>/subdir_with_dot_git_in_middle`

**Boundary**: the exclusion is scoped to descendants of the scanned root. If the operator invokes `mikebom sbom scan --path <repo>/.git/objects`, the scan root IS `.git/objects`; the exclusion applies to descendants of that root, not to the root itself. This matches the spec's Edge Case for operators explicitly scanning inside `.git/`.

## File-form gate

**Contract**: given a candidate file path `<abs>` reaching the visit callback, the callback returns early (before any I/O) iff `<abs>.file_name()` decodes to a UTF-8 string exactly equal to `.git`.

**Rationale**: git submodules create a `.git` FILE (not directory) at each submodule's root containing a `gitdir:` pointer. The file-form check catches this case; the directory-descend gate doesn't (that only fires on directories being descended into).

**Positive matches**:
- `<root>/.git` when it's a file (submodule pointer)
- `<root>/submodule/.git` when submodule is a git-submodule with a pointer file

**Negative matches** (visit callback proceeds normally):
- `<root>/.gitignore` (five chars — not `.git`)
- `<root>/.gitattributes`
- `<root>/.gitmodules`
- `<root>/random_file.git.bak` (basename is `random_file.git.bak`, not `.git`)

**Ordering constraint**: the file-form check MUST run before `std::fs::symlink_metadata(abs_path)`. This ensures a symlink named `.git` (e.g., pointing to `/etc/passwd`) is also skipped — the check inspects only the filename, never touches the filesystem.

**Skip semantics**: excluded files do NOT increment any counter category (`unreadable_skipped`, `shape_skipped`, `special_skipped`, `oversize_skipped`, `dedupe_skipped`, `emitted`). Their existence is invisible to the walker's stats output. Per FR-005, no new counter category is introduced.

## Log level

**Contract**: when either gate returns `true`, exactly one `tracing::debug!` line is emitted naming the excluded candidate path. Debug level per FR-009. Default log level (INFO) suppresses; `RUST_LOG=debug` surfaces the skip decisions.

**Log line shape** (structured tracing field):

```
DEBUG mikebom_cli::scan_fs::file_tier::walker: file-tier walker: skipping VCS metadata candidate=<absolute-path>
```

The exact prose after "skipping" is prose-level detail (the message field); the load-bearing contract is (a) log level == DEBUG and (b) the `candidate` field carries the path. FR-009 explicitly forbids INFO+.

## Pure-function guarantee

`is_vcs_metadata_name(candidate: &Path) -> bool` is a pure function of the filename bytes. It:
- Does NOT perform I/O.
- Does NOT allocate (returns a `bool`; borrows the `&str` from the OsStr).
- Does NOT depend on the current working directory.
- Does NOT depend on environment variables.
- Is thread-safe (uses only `'static` const data).
- Is idempotent (repeated calls with the same input return the same value).
- Is deterministic across process instances.

This lets the closure be `Send + Sync` for future multi-threaded walker refactors (out of scope for m174 but preserved as a property).

## Consumer test snippets

Consumers verifying the fix in a downstream SBOM:

### jq — verify no `.git/` paths in `mikebom:source-files`

```jq
[
  .components[]?.properties[]?
  | select(.name == "mikebom:source-files")
  | .value
  | fromjson
  | .[]
]
| map(select(startswith(".git/")))
| length
```

Post-174 on any scanned git-cloned repo: output MUST equal 0.

### jq — verify first-party scripts still surface

```jq
[
  .components[]?.properties[]?
  | select(.name == "mikebom:source-files")
  | .value
  | fromjson
  | .[]
]
| map(select(endswith(".sh") or endswith(".ps1")))
| length
```

Post-174: output MUST equal the count of first-party shell scripts in the repo (unchanged from pre-174 for the same fixture).
