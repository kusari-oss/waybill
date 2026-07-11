# Data Model: pip / poetry / uv optional-dependency classification (m183)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md)

## 1. No New Types

m183 introduces ZERO new types. All classification flows through existing infrastructure:

- **`LifecycleScope::Optional`** — introduced by m179, already the target enum variant
- **`RelationshipType::OptionalDependsOn`** — introduced by m179, already the edge scope
- **`SpdxRelationshipType::OptionalDependencyOf`** — introduced by m179, already the SPDX 2.3 enum value
- **`extra_annotations: BTreeMap<String, Value>`** on `PackageDbEntry` — already carries `mikebom:optional-derivation`
- **C122 parity extractor** at `mikebom-cli/src/parity/extractors/mod.rs:545` — already registered, `Directionality::SymmetricEqual`
- **`apply_lifecycle_scope_to_edges`** at `mikebom-cli/src/scan_fs/mod.rs:1261` — already dispatches `LifecycleScope::Optional` to `RelationshipType::OptionalDependsOn`

## 2. Classifier Decision Matrix

### US1 — `poetry.lock`

Extends `poetry.rs:67` from a 2-arm match on `poetry_is_dev` to a 3-arm match consulting BOTH `poetry_is_dev` AND `tbl.get("optional")`:

| `poetry_is_dev(tbl)` | `tbl.get("optional")` | Emitted `lifecycle_scope` | Emitted `mikebom:optional-derivation` |
|---|---|---|---|
| `Some(true)` | `Some(true)` | `Development` | (not emitted — dev wins per Decision 2) |
| `Some(true)` | `Some(false)` or absent | `Development` | (not emitted) |
| `Some(false)` | `Some(true)` | **`Optional`** ✱ | **`"pip-optional-dependencies"`** ✱ |
| `Some(false)` | `Some(false)` or absent | `Runtime` | (not emitted) |
| `None` | `Some(true)` | **`Optional`** ✱ | **`"pip-optional-dependencies"`** ✱ |
| `None` | `Some(false)` or absent | `None` | (not emitted) |

✱ = new m183 behavior. All other rows preserve pre-m183 behavior byte-identically.

**Read helper**:
```rust
/// True iff the poetry.lock `[[package]]` entry declares `optional = true`.
/// Missing or non-boolean value returns false (default behavior).
fn poetry_is_optional(tbl: &toml::value::Table) -> bool {
    tbl.get("optional").and_then(|v| v.as_bool()).unwrap_or(false)
}
```

### US2 — `pyproject.toml [project.optional-dependencies]`

A new helper `optional_deps_from_pyproject` extracts the set of package names declared under any `[project.optional-dependencies].<extra>` array:

```rust
/// Collect direct-dep package names classified as optional by the
/// project's pyproject.toml `[project.optional-dependencies]` tables.
/// Excludes names that also appear in `[project.dependencies]`
/// (diamond-shape: Runtime wins).
///
/// Returns HashSet<String> keyed by the PEP-508 first-token name
/// (matches `build_pip_main_module_entry`'s `take_first_token` shape).
fn optional_deps_from_pyproject(project_table: &toml::Value) -> HashSet<String>;
```

The set is returned to the `read` dispatcher, which then post-processes the `Vec<PackageDbEntry>` to mark matching child components:

```rust
// After all readers return their PackageDbEntry vectors:
for entry in &mut entries {
    if optional_names.contains(&entry.name)
        && entry.lifecycle_scope.is_none()  // don't override lockfile classification (Decision 3)
    {
        entry.lifecycle_scope = Some(LifecycleScope::Optional);
        entry.extra_annotations.insert(
            "mikebom:optional-derivation".to_string(),
            serde_json::Value::String("pip-optional-dependencies".to_string()),
        );
    }
}
```

### US3 — `uv.lock [[package]].optional-dependencies.<extra>`

`uv_lock.rs` gains a per-package optional-dependency walk. Same classifier semantics as US2:

```rust
// Inside the existing [[package]] iteration:
if let Some(opt_table) = pkg.get("optional-dependencies").and_then(|v| v.as_table()) {
    for (_extra_name, arr) in opt_table {
        if let Some(deps) = arr.as_array() {
            for dep in deps {
                if let Some(name) = dep.as_table()
                    .and_then(|t| t.get("name"))
                    .and_then(|v| v.as_str())
                {
                    // Skip if same name already in the primary dependencies array
                    // (diamond-shape: Runtime wins)
                    if !primary_dep_names.contains(name) {
                        optional_names.insert(name.to_string());
                    }
                }
            }
        }
    }
}
```

The `optional_names` HashSet is threaded into the same post-pass as US2's.

## 3. FR-006 Lockfile-Precedence Rule (implementation)

The `read` dispatcher at `pip/mod.rs:101` invokes readers in this order:

1. `read_poetry_lock(rootfs, include_dev)` → `Vec<PackageDbEntry>`
2. `read_uv_lock(rootfs, include_dev)` → `Vec<PackageDbEntry>`
3. `read_pipfile_lock(rootfs, include_dev)` → `Vec<PackageDbEntry>`
4. `read_requirements_txt(rootfs)` → `Vec<PackageDbEntry>`
5. `read_dist_info(rootfs, include_dev)` → `Vec<PackageDbEntry>`
6. Main-module extraction (`build_pip_main_module_entry`) → optional-name-set via US2 helper

The US2 post-pass check `entry.lifecycle_scope.is_none()` enforces Decision 3's precedence: if a lockfile reader already classified the entry (Runtime, Dev, or Optional), the manifest-based classification is a no-op.

**Special case for US1's dev-wins-over-optional (Decision 2)**: enforced inside `poetry.rs`'s classifier, not in the post-pass. The post-pass never overrides Dev because the dev-classified entries already have `Some(LifecycleScope::Development)`.

## 4. C122 Parity Catalog Value-Set Update

The C122 catalog row at `mikebom-cli/src/parity/extractors/mod.rs:545` is UNCHANGED — it's a `Directionality::SymmetricEqual` extractor that emits the annotation value from all three formats. Only the docstring at `cdx.rs:866` needs a text update:

**Before**:
```rust
// C122 — `mikebom:optional-derivation` (milestone 179). Records
// which ecosystem construct produced the component's
// `LifecycleScope::Optional` classification (`cargo-optional-true`,
// `npm-optional-dependencies`, `pip-extras-require`,
// ...
```

**After** (m183 fix):
```rust
// C122 — `mikebom:optional-derivation` (milestone 179). Records
// which ecosystem construct produced the component's
// `LifecycleScope::Optional` classification (`cargo-optional-true`,
// `npm-optional-dependencies`, `pip-optional-dependencies`,
// ...
```

Zero behavior change — the docstring lists expected values as documentation, not runtime configuration.

## 5. Test Contract

**Unit tests** (colocated with the code they cover):

- `poetry.rs::tests::optional_true_non_dev_classifies_as_optional` (new — US1 acceptance 1)
- `poetry.rs::tests::optional_true_annotation_carries_pip_optional_dependencies` (new — US1 acceptance 2)
- `poetry.rs::tests::dev_classified_package_still_dev_ignoring_optional_flag` (new — US1 acceptance 3 + 4, Decision 2)
- `poetry.rs::tests::optional_false_stays_runtime` (new — regression pin)
- `poetry.rs::tests::optional_field_absent_stays_runtime` (new — regression pin)
- `pip/mod.rs::tests::optional_deps_from_pyproject_extracts_names` (new — US2 helper unit)
- `pip/mod.rs::tests::optional_deps_diamond_shape_runtime_wins` (new — US2 acceptance 3 + FR-005)
- `pip/mod.rs::tests::main_module_dep_split_still_records_optional_names` (new — US2 wiring)
- `uv_lock.rs::tests::optional_dependencies_sub_table_classifies` (new — US3 acceptance 1 + 2)
- `uv_lock.rs::tests::uv_lock_diamond_shape_runtime_wins` (new — US3 acceptance 3)
- `uv_lock.rs::tests::uv_lock_optional_absent_stays_none` (new — regression pin)

**Integration tests** (via existing pip fixtures + one new fixture per US):

- New pip regression fixture `tests/fixtures/scan_fs/pip/poetry-optional/` — poetry.lock with `optional = true` non-dev package (US1 end-to-end)
- New pip regression fixture `tests/fixtures/scan_fs/pip/pyproject-optional/` — pyproject.toml with `[project.optional-dependencies]` and no lockfile (US2 end-to-end)
- New pip regression fixture `tests/fixtures/scan_fs/pip/uv-optional/` — uv.lock with `[[package]].optional-dependencies.<extra>` (US3 end-to-end)
- Existing pip regression golden regen — should show additive changes ONLY on the specific `optional = true` fixture rows (per SC-004)

**Filter-parity assertion** (per SC-001/002/003): the set of pip-emitted PURLs marked `scope: "excluded"` in the CDX 1.6 golden MUST equal the set of PURLs appearing as source-side of any `*_DEPENDENCY_OF` typed relationship in the SPDX 2.3 golden.

## 6. Backward Compatibility

- **Fixtures with NO `optional = true` poetry entries + NO `[project.optional-dependencies]` + NO uv `optional-dependencies`**: byte-identical output (SC-005 gate).
- **Fixtures with dev-classified packages (regardless of `optional` flag)**: byte-identical output (Decision 2 preserves the dev-classification path).
- **Existing pip regression fixture at `poetry.rs:178+`** (currently asserts `LifecycleScope::Runtime` for `optional = false`): unchanged. A NEW fixture entry for `optional = true, category = "main"` is what asserts the m183 fix.
