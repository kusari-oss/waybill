# Data Model — Fix m224 coord-table `directDependencies`

## Entity 1 — `Entry` struct (production, modified)

**File**: `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs`

**Note**: The Before/After below reflect the DELIVERED implementation, which is broader than the plan-time draft. Per research.md R1 amendment, the fix also had to change `dependencies` type (real-world lockfiles use coord-table shape on BOTH fields).

### Before

```rust
#[derive(Debug, Deserialize)]
pub(crate) struct Entry {
    #[serde(default, rename = "directDependencies")]
    pub(crate) direct_dependencies: Vec<String>,     // <-- REMOVED
    #[serde(default)]
    pub(crate) dependencies: Vec<String>,             // <-- retyped
    #[serde(default)]
    pub(crate) file_name: Option<String>,
    pub(crate) coord: EntryCoord,
    #[serde(default)]
    pub(crate) file_digest: Option<EntryFileDigest>,
}
```

### After

```rust
#[derive(Debug, Deserialize)]
pub(crate) struct Entry {
    /// Transitive resolved deps. Elements are `DependencyRef` — either
    /// legacy string form or coord-table form.
    #[serde(default)]
    pub(crate) dependencies: Vec<DependencyRef>,
    /// The artifact filename. Retained for future use.
    #[serde(default)]
    pub(crate) file_name: Option<String>,
    /// The Maven coordinate triple + optional classifier + packaging.
    pub(crate) coord: EntryCoord,
    /// Optional artifact hash.
    #[serde(default)]
    pub(crate) file_digest: Option<EntryFileDigest>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum DependencyRef {
    /// Legacy `"group:artifact:version"` string
    String(String),
    /// `[[entries.dependencies]] { group, artifact, version }` coord-table
    CoordTable {
        group: String,
        artifact: String,
        #[serde(default)]
        version: Option<String>,
    },
}
```

### Behavioral consequence

- TOML input lockfiles containing `directDependencies = []` continue to parse (serde silently ignores unknown TOML fields when the target struct doesn't have `#[serde(deny_unknown_fields)]`).
- TOML input lockfiles containing `[[entries.directDependencies]] { group, artifact, version }` (the coord-table shape that today rejects) also parse — the field is invisible to the struct, and TOML's nested-array-of-tables shape doesn't produce a parse error for an ignored field.
- TOML input lockfiles containing the legacy `directDependencies = ["group:artifact:version"]` string form also parse — same reason.
- Any future Pants-invented shape for `directDependencies` also parses.

### Removed sink

`lockfile.rs:358` — `let _ = &entry.direct_dependencies;` — this line becomes an unused import / unused pattern and MUST be removed. The surrounding dead-code-sink block (lines 355-362) survives; only the specific `direct_dependencies` reference is removed.

## Entity 2 — `CoursierLockfile` (unchanged)

No structural change. The `entries: Vec<Entry>` field is unchanged.

## Entity 3 — Test fixture updates

### `lockfile.rs::tests::parse_valid_pants_coursier_lockfile` (line 497)

Remove the `direct_dependencies: Vec::new(),` initializer line. All other fields unchanged.

### TOML fixture strings (`lockfile.rs` lines 411, 441)

Existing test strings include `directDependencies = []` — these compile untouched under the post-fix struct because TOML fields without a struct counterpart are silently ignored.

## Entity 4 — New unit tests

**File**: `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs` (in `#[cfg(test)] mod tests`)

### Test T-A: `parse_coord_table_single_dep`

TOML fixture: one `[[entries]]` block with a single `[[entries.directDependencies]]` coord-table entry. Assert `parse()` returns `Ok(_)` and `entries.len() == 1`.

### Test T-B: `parse_coord_table_multi_dep`

TOML fixture: one `[[entries]]` block with three `[[entries.directDependencies]]` coord-table entries. Assert `parse()` returns `Ok(_)` and `entries.len() == 1`.

### Test T-C: `parse_mixed_empty_and_coord_table`

TOML fixture: one entry with `directDependencies = []`, one entry with `[[entries.directDependencies]]` coord-table. Assert both parse and `entries.len() == 2`.

### Test T-D: `parse_legacy_string_form_deps`

TOML fixture: one `[[entries]]` block with `directDependencies = ["com.google.guava:guava:31.0.1-jre"]` (legacy string-array form, synthetic). Assert `parse()` returns `Ok(_)` and `entries.len() == 1`.

### Test T-E: `malformed_coord_entry_skipped_at_emission`

Verify FR-004's existing behavior. TOML fixture: one entry with `coord.group = ""` (empty group triggers the existing per-entry validation at line 240). Call `entry_to_package_db_entry` and assert it returns `None` with the expected WARN log fired. (Existing behavior; test locks it in.)

## Entity 5 — Corpus target restoration

**File**: `waybill-cli/tests/corpus_harness_195/manifest.rs`

Restore `pants-example-jvm` `CorpusTarget` entry (deleted comment block; replace with entry):

```rust
CorpusTarget {
    name: "pants-example-jvm",
    source: SourceKind::Git {
        clone_url: "https://github.com/kusari-sandbox/example-jvm",
    },
    pinned: PinnedRef::Sha {
        // Fork of pantsbuild/example-jvm HEAD as of 2026-09-02
        hex: "675ee75d36f2c1b096b0def51efcfffd02bd1251",
    },
    ecosystem: Ecosystem::JavaMaven,
    exercises: "m224 Pants coursier-JVM reader (3rdparty/jvm/default.lock) — \
                unblocked by #676 (coord-table directDependencies fix)",
    layer1: super::layer1_assertions::pants_example_jvm_layer1,
},
```

Placement: immediately after the existing `pants-example-django` entry (per convention of listing pants examples together).

## Entity 6 — Layer 1 assertion function

**File**: `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs`

New function `pants_example_jvm_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure>`.

### Invariants

| # | Invariant | Check |
|---|---|---|
| 1 | `maven-transitives-present-at-scale` | count `pkg:maven/*` components ≥ 20 (of 27 declared entries in the fixture; leaves headroom) |
| 2 | `top-level-guava-present` | any `pkg:maven/com.google.guava/guava@*` component present |
| 3 | `top-level-scala-library-present` | any `pkg:maven/org.scala-lang/scala-library@*` component present |
| 4 | `pants-resolve-annotation-present` | at least one `pkg:maven/*` component carries `waybill:pants-resolve` in `.properties[]` |

### Failure diagnostics

Follow the AssertionFailure shape from PR #757. Each invariant's `suggested_action` names the specific reader function or milestone the maintainer should investigate.

## Entity 7 — Test entry point

**File**: `waybill-cli/tests/public_corpus.rs`

Add:

```rust
#[test]
fn corpus_pants_example_jvm() {
    run_target("pants-example-jvm");
}
```

Placement: immediately after `corpus_pants_example_django` (matches the manifest ordering).

## Entity 8 — Golden fixture directory

**Path**: `waybill-cli/tests/fixtures/public_corpus/pants-example-jvm/`

**Files**: `cdx.json`, `spdx-2.3.json`, `spdx-3.json` (all three formats, generated via `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1`).

**Filtering**: unlike `pants-example-javascript` (which used JS-only filter per feature 675 FR-008), this target does NOT need per-target filtering. The `pantsbuild/example-jvm` fixture is JVM-only (no mixed ecosystems), so full-SBOM goldens are compact and stable. No new `layer2_golden.rs::compare_golden` dispatch needed.

**Expected size**: Comparable to `maven-guice` corpus target (which uses similar ecosystem). Empirical measurement at implement-time.

## No state transitions

The reader remains stateless. `Entry` is a POD parse target. No new state introduced by this fix.
