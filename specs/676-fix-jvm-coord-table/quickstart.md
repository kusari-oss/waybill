# Quickstart — Fix m224 coord-table `directDependencies`

## Prerequisites

- Workspace at `676-fix-jvm-coord-table` branch
- `git` on PATH
- For corpus-target verification: `WAYBILL_RUN_PUBLIC_CORPUS=1` env var and network access to `github.com/kusari-sandbox/example-jvm`

## Reproduce the bug (baseline)

Before touching any code, verify the bug reproduces on `main`:

```bash
git checkout main
cargo build -p waybill --release
mkdir /tmp/repro && cd /tmp/repro
git clone --depth 1 https://github.com/pantsbuild/example-jvm .
git checkout 675ee75d36f2c1b096b0def51efcfffd02bd1251
RUST_LOG=info /path/to/waybill --offline sbom scan --path . --format cyclonedx-json --output /tmp/jvm.cdx.json 2>&1 | grep -iE "pants|jvm|coursier"
```

Expected pre-fix output:

- `WARN pants-coursier-jvm reader: coursier TOML body parse error; skipping lockfile=... error=TOML parse error at line 47, column 1`
- `INFO pants-coursier-jvm reader complete lockfiles_discovered=1 lockfiles_parsed_ok=0 lockfiles_skipped_corrupt=1 components_emitted=0`
- Zero `pkg:maven/*` components in `/tmp/jvm.cdx.json`

## Apply the fix

Switch to the feature branch:

```bash
git checkout 676-fix-jvm-coord-table
```

### Step 1: production code change

Edit `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs`:

- **Remove** lines 58-60 (the `direct_dependencies` field declaration inside `struct Entry`).
- **Remove** line 358 (`let _ = &entry.direct_dependencies;`).
- **Update** the doc comment above `struct Entry` if it lists `direct_dependencies` as a documented field.

### Step 2: fix the in-file test constructor

At line ~497 in `parse_valid_pants_coursier_lockfile`, remove the `direct_dependencies: Vec::new(),` line from the `Entry { ... }` initializer.

### Step 3: verify unit tests still pass

```bash
cargo test --test pants_coursier_jvm_reader
cargo test -p waybill --lib pants_jvm
```

Both should complete `ok. N passed; 0 failed`.

### Step 4: add new unit tests

Extend the `#[cfg(test)] mod tests` block in `lockfile.rs` with the 5 tests from data-model.md §Entity 4:

- `parse_coord_table_single_dep`
- `parse_coord_table_multi_dep`
- `parse_mixed_empty_and_coord_table`
- `parse_legacy_string_form_deps`
- `malformed_coord_entry_skipped_at_emission` (locks in existing FR-004 behavior)

### Step 5: verify the fix reproduces green

Rebuild + rerun the reproduction from the top of this file:

```bash
cargo build -p waybill --release
cd /tmp/repro
RUST_LOG=info /path/to/waybill --offline sbom scan --path . --format cyclonedx-json --output /tmp/jvm.cdx.json 2>&1 | grep -iE "pants|jvm|coursier"
```

Expected post-fix output:

- No `WARN pants-coursier-jvm reader: coursier TOML body parse error` line.
- `INFO pants-coursier-jvm reader complete lockfiles_discovered=1 lockfiles_parsed_ok=1 lockfiles_skipped_corrupt=0 components_emitted=N` where N ≥ 20.
- `jq '.components[].purl' /tmp/jvm.cdx.json | grep -c pkg:maven/` ≥ 20.
- Includes `pkg:maven/com.google.guava/guava@31.0.1-jre` and `pkg:maven/org.scala-lang/scala-library@2.13.8`.

## Restore the corpus target

### Step 6: manifest entry

Edit `waybill-cli/tests/corpus_harness_195/manifest.rs`:

- Replace the multi-line `NOTE: pants-example-jvm intentionally omitted...` comment block with the `CorpusTarget` entry from data-model.md §Entity 5.

### Step 7: layer 1 assertion function

Add `pants_example_jvm_layer1` to `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs` per data-model.md §Entity 6 (4 invariants).

### Step 8: test entry point

Add `#[test] fn corpus_pants_example_jvm() { run_target("pants-example-jvm"); }` to `waybill-cli/tests/public_corpus.rs` per data-model.md §Entity 7.

### Step 9: generate goldens

```bash
WAYBILL_RUN_PUBLIC_CORPUS=1 \
WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1 \
WAYBILL_CORPUS_SKIP_OCI=1 \
cargo test --test public_corpus corpus_pants_example_jvm
```

Expected: `1 passed`. Verify `waybill-cli/tests/fixtures/public_corpus/pants-example-jvm/{cdx,spdx-2.3,spdx-3}.json` now exist.

### Step 10: byte-identity across two runs

Rerun without the update env var:

```bash
WAYBILL_RUN_PUBLIC_CORPUS=1 \
WAYBILL_CORPUS_SKIP_OCI=1 \
cargo test --test public_corpus corpus_pants_example_jvm
```

Expected: `1 passed` (golden comparison succeeds).

### Step 11: verify no cross-target impact

```bash
WAYBILL_RUN_PUBLIC_CORPUS=1 \
WAYBILL_CORPUS_SKIP_OCI=1 \
cargo test --test public_corpus
```

Expected: all corpus targets pass. Any pre-existing golden drift on the 5 stale targets (per issue #763) is unrelated to this fix.

### Step 12: full pre-PR gate

```bash
./scripts/pre-pr.sh
```

Expected: `>>> all pre-PR checks passed.`

## Common pitfalls

- **Do not add `#[serde(deny_unknown_fields)]` to `Entry`** — the fix relies on serde's default field-tolerance to silently ignore `directDependencies` in whatever shape upstream emits. Adding the strict attribute would revert the fix's core mechanism.
- **Do not touch `dependencies` (the sibling field)** — it carries the transitive dep graph and is load-bearing. Only `direct_dependencies` is unused.
- **Do not delete the whole dead-code sink block at lines 355-362** — it also silences warnings for `file_name` and `serialized_bytes_length`. Only remove the specific `let _ = &entry.direct_dependencies;` line.
- **Golden regen if lockfile changes**: if upstream `pantsbuild/example-jvm` rewrites its lockfile, the corpus target's layer 2 goldens must be regenerated. The refresh flows through `scripts/corpus/refresh-pins.sh` (bump the pinned SHA) followed by golden regen.
