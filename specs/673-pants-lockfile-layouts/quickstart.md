# Quickstart — m673 Pants lockfile discovery layout extensions

**Feature**: 673-pants-lockfile-layouts
**Audience**: Implementer picking up this milestone after `/speckit.tasks` runs.

## Goal

Extend the m223 + m672 Pants Python lockfile reader to discover
lockfiles at two additional canonical paths used by Pants 2.31+
default layouts (`<repo-root>/*.lock` and `<repo-root>/lockfiles/*.lock`),
gated by a `pex_version` content-detection check to avoid false-
positive parses of Cargo/Poetry/bun lockfiles that share the `.lock`
extension.

## Files you'll touch

Only 2 source files + 1 new integration test file. No `Cargo.toml`
changes. No workflow YAML changes.

```text
waybill-cli/src/scan_fs/package_db/pants/mod.rs        # ~50 lines added
waybill-cli/src/scan_fs/package_db/pants/lockfile.rs   # ~5 lines added (is_pex_lockfile_content helper)
waybill-cli/tests/scan_pants_m673.rs                   # NEW — integration tests
```

## Verification recipe

### Step 1 — Add the `is_pex_lockfile_content` helper

Per `contracts/content_detection.md` C1–C7, add the pure function
at `pants/lockfile.rs`:

```rust
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

Add inline unit tests covering the 12-row test matrix in
`contracts/content_detection.md`.

Verify:
```bash
cargo +stable test -p waybill --bin waybill scan_fs::package_db::pants::lockfile::tests::is_pex
```

### Step 2 — Extend `DiscoverySource` with two new variants

Per `data-model.md` §"Enum 1", add `RepoRootGlob` + `LockfilesGlob`
variants to the `DiscoverySource` enum in `pants/mod.rs`. Update the
`dedup_by_canonical_path` winner-selection rule to include them as
tied peers with `DefaultGlob` (per research.md §R4).

### Step 3 — Add the two new discovery loops in `discover_lockfiles`

Per `contracts/discovery_paths.md` paths 4 + 5, add:

```rust
// Path 4: <scan_root>/*.lock (non-recursive)
if let Ok(read_dir) = std::fs::read_dir(scan_root) {
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lock") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        if !lockfile::is_pex_lockfile_content(&bytes) {
            continue;  // FR-004 silent-skip
        }
        let resolve_name = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("default")
            .to_string();
        out.push(DiscoveredLockfile {
            path,
            resolve_name,
            origin: DiscoverySource::RepoRootGlob,
        });
    }
}

// Path 5: <scan_root>/lockfiles/*.lock (non-recursive)
let lockfiles_dir = scan_root.join("lockfiles");
if let Ok(read_dir) = std::fs::read_dir(&lockfiles_dir) {
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lock") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        if !lockfile::is_pex_lockfile_content(&bytes) {
            continue;
        }
        let resolve_name = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("default")
            .to_string();
        out.push(DiscoveredLockfile {
            path,
            resolve_name,
            origin: DiscoverySource::LockfilesGlob,
        });
    }
}
```

Place these two loops AFTER the m672 `[python.resolves]` walk but
BEFORE `dedup_by_canonical_path(out)`. The final dedup pass is
unchanged from m672.

### Step 4 — Extend FR-006 signal detection

Per `contracts/discovery_paths.md` C7, update `pants_signal_present`
at `mod.rs::read`:

```rust
let default_dir_exists = scan_root.join("3rdparty").join("python").exists();
let pants_toml_exists = scan_root.join("pants.toml").exists();
let lockfiles_dir_exists = scan_root.join("lockfiles").exists();  // NEW
// Optionally: also count a repo-root PEX lockfile as a signal —
// but this requires content-detecting BEFORE discovery, which
// duplicates work. Skip this bullet: the four signals above are
// sufficient for the diagnostic hint (a Pants-shaped repo will
// almost always have pants.toml or one of the three directories).

let pants_signal_present = default_dir_exists
    || pants_toml_exists
    || lockfiles_dir_exists;
```

**Note**: the "at least one repo-root PEX lockfile" signal from
FR-006 could be added by moving `discover_lockfiles` before the
signal check — but that inverts the current control flow. Simpler:
just gate on the three directory-existence signals. If a user has
ONLY a repo-root PEX lockfile and no `pants.toml`, they don't get
the hint on the zero-discovered path — but that's an edge case
because such a repo would have discovered the lockfile already.
Prefer the simpler control flow.

### Step 5 — Add integration tests

Create `waybill-cli/tests/scan_pants_m673.rs` with 7 tests per
`research.md` §R6:

1. `repo_root_lockfile_discovered` (US1)
2. `multiple_repo_root_lockfiles_discovered_with_stem_names` (US1)
3. `repo_root_non_pex_lockfile_silent_skipped` (US1 + US3)
4. `lockfiles_directory_layout_discovered` (US2)
5. `lockfiles_dir_ignores_non_lock_files` (US2)
6. `content_detection_silent_skips_cargo_and_poetry` (US3)
7. `pre_m672_layout_byte_identity` (SC-005 regression guard — reuse
   an m672 committed fixture as a control)

Fixture helper (composed at test time — no committed files); reuse
the m672 `strip_ansi` + `run_scan` + `component_purls` helpers.

### Step 6 — Byte-identity guards

Run:

```bash
cargo +stable test -p waybill --test pants_pex_reader --test scan_pants_m672 --test scan_pants_m673 2>&1 | grep 'test result:'
```

Expected:
- `pants_pex_reader`: 10/10 (m223 goldens — must pass unchanged).
- `scan_pants_m672`: 10/10 (m672 tests — must pass unchanged).
- `scan_pants_m673`: 7/7 (new tests).

If m223 OR m672 tests fail, the m673 discovery loops accidentally
changed behavior on the pre-m672 layouts. Do NOT regenerate goldens
— fix the code.

### Step 7 — Real-world smoke tests

Clone `pantsbuild/example-python` + `pantsbuild/example-django` +
scan both:

```bash
for repo in example-python example-django; do
    tmp=$(mktemp -d)
    git clone --depth 1 --single-branch --no-tags \
        "https://github.com/pantsbuild/$repo" "$tmp" >/dev/null 2>&1
    RUST_LOG=info target/release/waybill \
        --offline sbom scan --path "$tmp" \
        --no-deep-hash --format cyclonedx-json --output /tmp/$repo.cdx.json \
        2>&1 | grep 'pants-pex reader complete'
    n=$(jq '[.components[] | select(.purl | startswith("pkg:pypi/"))] | length' /tmp/$repo.cdx.json)
    echo "$repo: $n pypi components"
    rm -rf "$tmp"
done
```

Expected:
- `example-python`: ≥ 8 pypi components (was 0 pre-m673 from Pants reader).
- `example-django`: > 20 pypi components (Django's transitive closure).

### Step 8 — Pre-PR gate

```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 \
  PATH="/Users/mlieberman/Projects/mikebom/.venv/spdx3-validate/bin:$PATH" \
  ./scripts/pre-pr.sh
```

Expected: `>>> all pre-PR checks passed.`

## What to skip (v2 extension points)

- **Recursive `lockfiles/<team>/<resolve>.lock`** — non-recursive
  only in v1 (FR-009). Multi-team monorepos with deeper nesting can
  file a v2 follow-up issue.
- **Non-lowercase directory variants** (`Lockfiles/`, `LOCKFILES/`) —
  case-sensitive match on `lockfiles/` (spec Assumptions).
- **Detecting FR-006 "at least one repo-root PEX lockfile" signal** —
  the three directory-existence signals are sufficient for the hint
  (see Step 4 note).
- **Per-directory content-detect concurrency** — sub-100ms overhead
  even on pathological cases; no need for a thread pool.

## Real-world sanity check post-PR

Re-run Step 7 after the PR merges to main. The `example-python`
and `example-django` component counts should match the numbers
above — if they don't, something drifted in `main` between
implementation and merge.
