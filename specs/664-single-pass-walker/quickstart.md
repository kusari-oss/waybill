# Quickstart: Migrating a Reader to the Shared Walker

**Feature**: 664-single-pass-walker
**Audience**: contributor migrating an ecosystem reader from independent `safe_walk` to the shared reader-registry.

**Status (2026-08-23)**: rewritten post-T067 to reflect the actual T036–T059 migration pattern. The pre-US1 draft cited a simpler `ctx.push()`-based sink; in practice, every US2 reader used state-slot collection + a `finalize()` post-walker helper because the reader's post-walker work (main-module emission, dep-graph wiring, cross-file dedup) doesn't fit inside a per-file callback.

## Before

A reader today walks the tree independently:

```rust
// waybill-cli/src/scan_fs/package_db/<reader>.rs

pub fn read(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PackageDbEntry> {
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: 10,
        should_skip: &|dir: &Path, _| {
            dir.file_name()
                .and_then(|n| n.to_str())
                .map(should_skip_default_descent)
                .unwrap_or(true)
        },
        exclude_set,
    };
    let mut out = Vec::new();
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.file_name().and_then(|n| n.to_str()) == Some("my-manifest.toml") {
            out.extend(process_manifest(path));
        }
    });
    out
}
```

Plus a call site in `read_all` at `package_db/mod.rs`:

```rust
out.extend(<reader>::read(rootfs, exclude_set));
```

## After — the actual T036–T059 pattern

The reader keeps `pub fn read()` under `#[allow(dead_code)]` as a legacy shim (per FR-004 coexistence — test callers still use it), and adds four new helpers: `on_<reader>_file`, `registration()`, `extract_paths()`, `finalize()`. State collection happens in an `Arc<Mutex<...DiscoveredPaths>>` slot; the actual per-project work happens in `finalize()` post-walker.

```rust
// waybill-cli/src/scan_fs/package_db/<reader>.rs

use std::sync::{Arc, Mutex};
use crate::scan_fs::walk_registry::{
    globset_from_patterns, ReaderId, ReaderRegistration, SharedWalkerContext,
};

/// Discovery state — one Vec<PathBuf> per legacy walker site.
#[derive(Default, Debug)]
pub(crate) struct MyReaderDiscoveredPaths {
    pub(crate) manifest_paths: Vec<PathBuf>,
}

fn on_my_reader_file(path: &Path, ctx: &SharedWalkerContext<'_>) {
    if path.file_name().and_then(|s| s.to_str()) != Some("my-manifest.toml") {
        return;
    }
    let Some(state) = ctx.state::<Mutex<MyReaderDiscoveredPaths>>(ReaderId::MY_READER)
    else {
        return;
    };
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.manifest_paths.push(path.to_path_buf());
}

pub(crate) fn registration() -> anyhow::Result<ReaderRegistration> {
    let patterns = globset_from_patterns(&["**/my-manifest.toml"])?;
    Ok(ReaderRegistration {
        reader_id: ReaderId::MY_READER,
        state: Some(Arc::new(Mutex::new(MyReaderDiscoveredPaths::default()))),
        patterns,
        on_file: Some(on_my_reader_file),
        on_dir: None,
    })
}

pub(crate) fn extract_paths(reg: &ReaderRegistration) -> MyReaderDiscoveredPaths {
    let Some(state_arc) = reg.state.as_ref() else {
        return MyReaderDiscoveredPaths::default();
    };
    let Some(mutex) = state_arc.downcast_ref::<Mutex<MyReaderDiscoveredPaths>>() else {
        return MyReaderDiscoveredPaths::default();
    };
    let mut guard = match mutex.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    std::mem::take(&mut *guard)
}

/// Post-walker entry — takes precomputed paths + runs the existing
/// per-project pipeline (main-module emission, sibling reads, etc.).
pub(crate) fn finalize(
    paths: MyReaderDiscoveredPaths,
    rootfs: &Path,
) -> Vec<PackageDbEntry> {
    let mut manifest_paths = paths.manifest_paths;
    manifest_paths.sort();  // FR-006 byte-identity — safe_walk was unsorted; sort defensively.
    let mut out = Vec::new();
    for path in &manifest_paths {
        out.extend(process_manifest(path));
    }
    out
}

/// Legacy public entry — retained during FR-004 coexistence for test callers.
#[allow(dead_code)]
pub fn read(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PackageDbEntry> {
    // Original safe_walk body unchanged. Keep for tests + direct API callers.
    // Body: build cfg, call safe_walk, collect manifest_paths, call finalize.
}
```

## Migration recipe (7 steps)

Every reader-migration PR performs these steps for exactly one reader.

### Step 1 — Declare the `ReaderId` constant

In `waybill-cli/src/scan_fs/walk_registry/mod.rs`, add BOTH:

```rust
impl ReaderId {
    // ... existing consts ...
    /// My-reader — one-sentence description covering shape (single-walker /
    /// two-phase / marker-detect / enrichment-only) + any skip-parity notes.
    /// Migrated in milestone-664 US2 T0XX.
    pub const MY_READER: ReaderId = ReaderId::new("my-reader");
}

pub(crate) const ALL_READER_IDS: &[ReaderId] = &[
    // ... existing entries ...
    ReaderId::MY_READER,  // contract C9 uniqueness enforced at test time
];
```

The string value should match the reader's module name (kebab-case). This is the value that appears in FR-009 diagnostic log's `per_reader_dispatch_counts` field.

### Step 2 — Add the state struct + `on_file` (or `on_dir`) callback

State usually contains one `Vec<PathBuf>` per legacy walker site. Multi-site readers use multiple vectors (see `nuget/mod.rs::NugetDiscoveredPaths` — csproj + deps_json + dll_paths; `cocoapods.rs::CocoapodsDiscoveredPaths` — podfile_locks + manifest_locks + podfiles).

Callback shape depends on the reader:

- **Single-file readers** (e.g., haskell, erlang, dart, swift): `on_file` matches basename → push path.
- **Two-phase readers** (needing sibling lookup): `on_file` matches marker → consult `ctx.dir_index().contains(dir, "sibling.toml")` before pushing.
- **Directory-driven readers** (e.g., swift): `on_dir` inspects `ctx.dir_index().contains(dir, "Package.resolved")` for a project marker.
- **Marker-detect gates** (e.g., vcpkg, conan, bazel, cmake, pants_jvm): state = `Mutex<bool>` seen flag; `finalize` gates the O(1) fixed-root read on `seen || fallback_fs_check`.
- **Precomputed-paths threading** (e.g., cargo, pants_go, golang, yocto): state = `Vec<PathBuf>`; `finalize` accepts optional precomputed paths via new `Option<Vec<PathBuf>>` parameter on the reader's `read()` signature. See T047 pants_go, T058 golang, T059 yocto for the pattern.

### Step 3 — Preserve skip-set parity

The shared walker's default skip set covers: leading-`.` prefix (`.git`, `.svn`, `.hg`, `.build`, `.dart_tool`, etc.), `node_modules`, `bower_components`, `vendor`, `target`, `dist`, `build`, `out`, `coverage`, `__pycache__`, `venv`.

- If the reader's legacy skip set is a **strict subset** (e.g., dart's `.dart_tool` / `.pub-cache` / `build` / `.git` / `.hg` / `.svn` / `node_modules`): NO ancestor-path filter needed in the callback. See dart.rs T050.
- If the reader's legacy skip set has **additions** not in the shared walker default (`_`-prefix, `testdata`, `Pods/`, `DerivedData/`, `deps/`, `priv/`, `cover/`, `go/pkg/mod`): apply `any_ancestor_matches` filtering in the callback. See elixir.rs T051 (`_build`/`deps`/`priv`/`cover`), cocoapods.rs T048 (`Pods`/`DerivedData`), golang/legacy.rs T058 (`testdata`/`_`-prefix/`go/pkg/mod`).

### Step 4 — Write `extract_paths()` + `finalize()`

Boilerplate from any migrated reader:

```rust
pub(crate) fn extract_paths(reg: &ReaderRegistration) -> MyReaderDiscoveredPaths {
    reg.state.as_ref()
        .and_then(|arc| arc.downcast_ref::<Mutex<MyReaderDiscoveredPaths>>())
        .map(|mutex| {
            let mut guard = mutex.lock().unwrap_or_else(|p| p.into_inner());
            std::mem::take(&mut *guard)
        })
        .unwrap_or_default()
}
```

`finalize` runs the reader's post-walker pipeline over precomputed paths. Two flavors:

- **Simple readers**: `finalize(paths, rootfs)` iterates paths + emits entries.
- **Readers with cross-file state** (main-module dedup, workspace resolution, cross-project depedges): `finalize` mirrors the original `read()` body but with `for path in paths` instead of `safe_walk(...)`.

Always sort `paths` at the start of `finalize` — `safe_walk` returned OS-natural order; the shared walker sorts per-dir but not cross-dir. Sorting normalizes for FR-006 byte-identity.

### Step 5 — Retain `pub fn read()` as a shim

Do NOT delete the legacy `pub fn read()` — retain it under `#[allow(dead_code)]` per FR-004 coexistence. Test paths + direct API callers still use it. The T063 audit sweep confirmed this pattern; deletion is a follow-up task after the FR-004 coexistence period ends.

### Step 6 — Wire into `SharedPilotOutput` + `run_shared_walker_pilot`

In `waybill-cli/src/scan_fs/package_db/mod.rs`:

1. **Extend `SharedPilotOutput`** with a new field (`my_reader: Vec<PackageDbEntry>` for standard readers; `my_reader_paths: Vec<PathBuf>` for precomputed-paths threading like pants_go / golang / yocto).
2. **Register in `run_shared_walker_pilot`**:
   ```rust
   if let Some(r) = register("my_reader", my_reader::registration()) {
       builder = builder.register(r);
   }
   ```
3. **Extract + finalize** in the pilot:
   ```rust
   let my_reader_entries = registry
       .registrations()
       .iter()
       .find(|r| r.reader_id == ReaderId::MY_READER)
       .map(|reg| {
           let paths = my_reader::extract_paths(reg);
           my_reader::finalize(paths, rootfs)
       })
       .unwrap_or_default();
   ```
4. **Populate the struct return**:
   ```rust
   SharedPilotOutput {
       // ...
       my_reader: my_reader_entries,
   }
   ```

### Step 7 — Swap the `read_all` call site

Find `<reader>::read(...)` in `read_all` and replace with:

```rust
// Milestone 664 US2 T0XX: my_reader splice from shared-walker pilot.
out.extend(std::mem::take(&mut shared_pilot.my_reader));
```

For readers using precomputed-paths threading (pants_go / golang / yocto pattern), take the paths out of `shared_pilot` and pass them as an `Option<Vec<PathBuf>>` argument to the reader's extended `read()` signature.

## Verify the migration

Per-reader migration PRs MUST pass, in this order:

1. **Build + clippy clean**:
   ```bash
   cargo +stable build -p waybill --all-targets
   cargo +stable clippy -p waybill --all-targets -- -D warnings
   ```
2. **Byte-identity (FR-006 / C7)** — every touched golden SBOM stays byte-identical. Do NOT run with `WAYBILL_UPDATE_*=1`:
   ```bash
   cargo +stable test -p waybill --no-fail-fast
   ```
   Target: 5017/0 (or the current count if the test corpus has grown).
3. **Walker-audit CI-shape check** — reproduce the m117 audit locally:
   ```bash
   bash -c '
   LIVE=$(LC_ALL=C grep -rEn --include="*.rs" "fn walk[_(]" waybill-cli/src/scan_fs/ | \
     while IFS=: read -r path line content; do
       prev=$((line - 1))
       if [ "$prev" -ge 1 ]; then
         prev_line=$(LC_ALL=C sed -n "${prev}p" "$path" 2>/dev/null)
         case "$prev_line" in *"// walker-audit:"*) continue;; esac
       fi
       printf "%s:%s:%s\n" "$path" "$line" "$content"
     done | sed -e "s/^\([^:]*\):[0-9]*:/\1:/" | LC_ALL=C sort -u)
   COMMITTED=$(LC_ALL=C sed -e "s/^\([^:]*\):[0-9]*:/\1:/" \
     waybill-cli/src/scan_fs/walk.audit-allowlist.txt | LC_ALL=C sort -u)
   diff <(echo "$LIVE") <(echo "$COMMITTED")
   '
   ```
   Expected: empty diff.
4. **Perf regression guard (SC-005)** — synthetic tree microbenchmark:
   ```bash
   WAYBILL_PERF_TEST_ENABLED=1 cargo test --release --test perf_walk_dispatch -- \
     sc005_synthetic_10k_file_tree_p95_dispatch_overhead --nocapture
   ```
   Assertion: per-file p95 ≤ 100 µs (total ≤ 1s across 10k files).

## Verify the overall milestone perf claim

Once at least the US1 pilot has landed, verify SC-001 empirically:

```bash
git clone --depth=1 https://github.com/ansible/ansible.git /tmp/ansible-perf-check
cargo build --release -p waybill
/usr/bin/time -h ./target/release/waybill sbom scan \
    --offline \
    --file-inventory=off \
    --path /tmp/ansible-perf-check/ \
    --format cyclonedx-json \
    --output /tmp/waybill.cdx.json
# Expected post-US1: wall time ≤ 3.5s (down from 4.10s baseline)
# Expected post-US2: wall time ≤ 1.2s (SC-001 target hit)
```

Same recipe with `github.com/pytorch/pytorch` for SC-002 (≤ 1.5s post-US2) and `github.com/mongodb/mongo` for SC-003 (≤ 3.0s post-US2).

## FR-005 escape-hatch conditions

A walker CANNOT migrate to the shared registry when it is:

- **Per-project-anchored** (bounded to a subtree already discovered by the outer walker; e.g., npm's `walk_node_modules`, pants_shell's per-target glob resolver, golang's per-project `package main` enumeration).
- **Non-scan-tree** (walks a cache like `~/.m2/`, `~/.cache/go-build/`, or an archive-internal structure via the `zip` crate).
- **Descend-into-required** (needs to enter dirs the shared walker skips by default: `vendor/`, `target/`, `build/`, `dist/`, `venv/`). Three readers deferred pending a per-registration `descend_into` API extension: **T029 yocto/recipe** (recipe body-parse pipeline coupling), **T039 maven** (target/ dual-walker skip conflict), **T057 go_binary** (build-output dirs).

Every escape-hatch site carries an inline `// FR-005 permanent escape hatch — <reason>` annotation. New escape hatches also need an entry in `waybill-cli/src/scan_fs/walk.audit-allowlist.rationale.md` under the appropriate category.

## When things go wrong

**"The golden SBOM diff is non-empty after migrating my reader."**
This is a blocker per C7 / FR-006. Do NOT run with `WAYBILL_UPDATE_*=1`. Root-cause the diff:
- **Filename ordering**: is `finalize()` sorting `paths` at the start? Safe_walk was unsorted; the shared walker sorts per-dir but not cross-dir.
- **Skip-set parity**: did you handle legacy-only skip additions via ancestor-path filtering? See elixir T051 (`_build`/`deps`/`priv`/`cover`) or cocoapods T048 (`Pods`/`DerivedData`) as a model.
- **Sibling lookup**: is `ctx.dir_index().contains(dir, name)` returning the same set the old walker observed? An excluded-then-descended-into subtree could show up differently.

**"CI walker-audit fails with a new `safe_walk` caller."**
Either (a) your migration didn't touch every `safe_walk` call site the reader had — check `git grep 'safe_walk(' waybill-cli/src/scan_fs/package_db/<reader>` — or (b) you added a new `fn walk_*` that needs classification. Per T065's diagnostic, new entries MUST fall into one of the 4 categories documented in `walk.audit-allowlist.rationale.md`.

**"My reader's `on_file` callback panics in production."**
Per C4, the shared walker catches the panic and continues. Look for `tracing::warn!` output with your reader's ID + the offending path. The panic itself is a reader bug (probably an `unwrap` on a malformed manifest); the panic-isolation is defense-in-depth.

**"The shared-walker pilot doesn't run for my scan and my splice returns empty."**
Check `run_shared_walker_pilot`'s bailout paths: if the registry builder fails (contract C8 duplicate reader IDs), the pilot returns `SharedPilotOutput::default()`. Verify your `ALL_READER_IDS` entry didn't accidentally duplicate an existing const.

## Reference migrations

Read the tasks.md close-out entries for concrete examples matching each pattern:

- **Single-walker simple**: dart T050, swift T052 (both have no ancestor filter needed).
- **Multi-walker with ancestor filter**: elixir T051, cocoapods T048.
- **Marker-detect gate**: vcpkg T053, conan T054, bazel T055, cmake T056, pants_jvm T045.
- **Precomputed-paths threading (`Option<Vec<PathBuf>>`)**: cargo T037, pants_go T047, golang T058, yocto T059.
- **Sibling-lookup `on_dir`**: gradle T041, swift T052.
- **Case-insensitive extension match**: composer T049, cmake T056.
- **FR-005 escape-hatch retention**: pants_shell T046 (target_resolver kept), cmake T056 (per-subdir walks kept), composer T049 (Pass B kept).
- **Deferred readers**: T029 yocto/recipe, T039 maven, T057 go_binary — all citing the `descend_into` API blocker.
