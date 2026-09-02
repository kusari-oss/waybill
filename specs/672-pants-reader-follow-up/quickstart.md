# Quickstart — m672 Pants pex-lockfile reader follow-up

**Feature**: 672-pants-reader-follow-up
**Audience**: Implementer picking up this milestone after `/speckit.tasks` runs.

## Goal

Extend the m223 Pants reader to (1) tolerate the `//`-comment
legacy lockfile shape and (2) discover lockfiles declared via
`pants.toml` `[python.resolves]`. Add a `legacy_shape_lockfiles`
field to the reader-complete INFO log.

## Files you'll touch

Only 4 files (no new crates, no `Cargo.toml` changes, no workflow YAML):

```text
waybill-cli/src/scan_fs/package_db/pants/config.rs      # ~20 lines added
waybill-cli/src/scan_fs/package_db/pants/lockfile.rs    # ~25 lines added (stripper)
waybill-cli/src/scan_fs/package_db/pants/mod.rs         # ~30 lines added (map union + FR-010/013)
waybill-cli/tests/scan_pants_m672.rs                    # NEW — integration tests
```

## Verification recipe

### Step 1 — Add the front-matter stripper

Add `strip_pants_frontmatter` per `contracts/front_matter_stripper.md` to
`lockfile.rs`. Route the existing `parse()` function through it:

```rust
pub(crate) fn parse(bytes: &[u8]) -> Option<PexLockfile> {
    let body = strip_pants_frontmatter(bytes);
    let lock: PexLockfile = serde_json::from_slice(body)
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                "pants-pex reader: failed to parse Pex lockfile as JSON; skipping"
            );
        })
        .ok()?;
    // ... existing pex_version check unchanged ...
}
```

Verify:
```bash
cargo +stable test -p waybill --bin waybill scan_fs::package_db::pants::lockfile::tests
```

Add unit tests directly next to `strip_pants_frontmatter` covering the
9-row test matrix from `contracts/front_matter_stripper.md`.

### Step 2 — Detect stripping to feed FR-013 counter

Modify `parse()` to return `(PexLockfile, bool /* was_legacy_shape */)`
or introduce a sibling `parse_with_metadata()` that reports whether
the stripper consumed any bytes. Thread the `bool` through
`mod.rs::read` into the new `LegacyShapeCounter`.

### Step 3 — Extend `PythonSection` + `discover_lockfiles`

Per `data-model.md` §"Struct 1" + `contracts/python_resolves_map.md`:

1. Add `resolves: BTreeMap<String, toml::Value>` to `PythonSection`.
2. In `discover_lockfiles`, after the existing default-glob loop and
   legacy-`lockfile` singular handling, walk `cfg.python.resolves`:
   - For each `(key, value)` where `value.as_str()` succeeds:
     canonicalize `scan_root.join(value_str)`, append or dedup per FR-009.
   - Else: WARN per FR-007 (name the key + observed TOML type via
     `value.type_str()`).
3. Update `DiscoveredLockfile` with the `origin: DiscoverySource` field
   per data-model.md §"Struct 2".
4. In the dedup loop, when two candidates share a canonicalized path:
   the one with `origin == PythonResolvesMap` wins.

### Step 4 — FR-010/FR-011/FR-012 diagnostic log

Modify `mod.rs::read` to lift the `if candidates.is_empty() { return
Vec::new(); }` early-return at line 111 to a Pants-signal-gated form:

```rust
let default_dir_exists = scan_root.join("3rdparty").join("python").exists();
let pants_toml_exists = scan_root.join("pants.toml").exists();
let pants_signal_present = default_dir_exists || pants_toml_exists;

if candidates.is_empty() {
    if pants_signal_present {
        tracing::info!(
            lockfiles_discovered = 0_usize,
            hint = "supply lockfile paths via `[python.resolves]` or `[python].lockfile` in pants.toml",
            "pants-pex reader complete"
        );
    }
    return Vec::new();
}
```

Extend the happy-path INFO log to include the new counter:

```rust
tracing::info!(
    lockfiles_discovered,
    lockfiles_parsed_ok,
    lockfiles_skipped_corrupt,
    legacy_shape_lockfiles = legacy_counter.as_log_value(),
    components_emitted,
    "pants-pex reader complete"
);
```

### Step 5 — Integration tests

Create `waybill-cli/tests/scan_pants_m672.rs` with 6 tests per
`research.md` §R6:

1. `legacy_shape_lockfile_round_trips_through_stripper`
2. `legacy_shape_malformed_body_fails_open`
3. `python_resolves_map_extends_discovery_set`
4. `python_resolves_map_dedupes_with_default_glob_map_wins`
5. `python_resolves_table_shape_warns_and_skips`
6. `zero_discovered_with_pants_signal_logs_info_line_with_hint`
7. `zero_discovered_without_pants_signal_stays_silent`

Fixture helper (composed at test time — no committed files):
```rust
fn write_pants_repo(root: &Path, layout: &[(&Path, &[u8])]) {
    for (rel_path, contents) in layout {
        let abs = root.join(rel_path);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(abs, contents).unwrap();
    }
}
```

Use synthetic `waybill-fixture-*` package names per memory
`feedback_fixture_synthetic_package_names`.

### Step 6 — Byte-identity guard

Run the existing m223 integration tests to prove SC-003 byte-identity:

```bash
cargo +stable test -p waybill --test scan_pants_pex 2>&1 | tail
```

Expected: same `test result: ok. N passed; 0 failed` as pre-m672.
If any m223 test fails, the always-strip pass introduced a bug on
clean-JSON files.

### Step 7 — Pre-PR gate

```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 \
  PATH="/Users/mlieberman/Projects/mikebom/.venv/spdx3-validate/bin:$PATH" \
  ./scripts/pre-pr.sh
```

Expected: `>>> all pre-PR checks passed.`

## What to skip (v2 extension points)

- **Table-shape `[python.resolves.<name>]` parsing** — WARN+skip only in v1.
- **Per-file DEBUG log naming which files were legacy shape** — v2 nice-to-have.
- **Document-scope annotation for the legacy counter** — v2 (needs new C-row + 3 extractors + parity-catalog registration; deferred per Q1).
- **`.lock.metadata` sidecar parsing** — never; that's a Pants-internal artifact.

## Test the real-world case (Altana adopter sanity check)

If you have access to a Pants 2.33 monorepo with `[python.resolves]`:

```bash
RUST_LOG=info \
  target/release/waybill sbom scan --path <pants-repo> \
  --format cyclonedx-json --output /tmp/pants.cdx.json \
  2>&1 | grep 'pants-pex reader'
```

Look for:
```
INFO pants-pex reader complete lockfiles_discovered=24 lockfiles_parsed_ok=24 lockfiles_skipped_corrupt=0 legacy_shape_lockfiles=0 components_emitted=9838
```

If `legacy_shape_lockfiles > 0`, tell the operator to run
`./pants generate-lockfiles` to regenerate the stale-shape files.
