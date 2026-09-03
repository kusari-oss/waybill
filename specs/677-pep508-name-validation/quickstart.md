# Quickstart — PEP 508 name validation for pip reader

## Prerequisites

- Workspace at `677-pep508-name-validation` branch
- No network access needed (no external fixtures)

## Reproduce the bug (baseline)

Before applying the fix, verify the bug reproduces on `main`:

```bash
git checkout main
cargo build -p waybill
mkdir -p /tmp/repro-677
cat > /tmp/repro-677/pyproject.toml <<'EOF'
[project]
name = "{{package-name}}"
version = "0.0.0"
dependencies = [
    "waybill-fixture-real-dep-1",
    "waybill-fixture-real-dep-2",
]
EOF
RUST_LOG=info ./target/debug/waybill --offline sbom scan --path /tmp/repro-677 --format cyclonedx-json --output /tmp/jvm-baseline.cdx.json 2>&1 | grep -iE "pypi|pip reader"
echo "---"
jq '[.components[].purl // "" | select(startswith("pkg:pypi/"))] | length' /tmp/jvm-baseline.cdx.json
jq -r '.components[].purl // empty | select(startswith("pkg:pypi/"))' /tmp/jvm-baseline.cdx.json
```

Expected pre-fix output:

- One or more `pkg:pypi/` components emit — at least `pkg:pypi/{{package-name}}@0.0.0` for the main-module + `pkg:pypi/waybill-fixture-real-dep-*` entries for the declared deps.
- No WARN log about name validation.

## Apply the fix

Switch to the feature branch:

```bash
git checkout 677-pep508-name-validation
```

### Step 1: create the `name_validation` module

Create `waybill-cli/src/scan_fs/package_db/name_validation.rs` per `data-model.md` §Entities 1-3 + `contracts/name-validation-module.md`:

- `NameValidationError` enum (Empty | Malformed { reason })
- `Display` impl
- `is_pep508_name(name: &str) -> bool`
- `validate_pep508_name(name: &str) -> Result<(), NameValidationError>`
- `#[cfg(test)] mod tests` with 14 test cases from the contract's testing table

### Step 2: register the module

Edit `waybill-cli/src/scan_fs/package_db/mod.rs` — add `pub(crate) mod name_validation;` alongside other module declarations.

### Step 3: add the filter pass

In `waybill-cli/src/scan_fs/package_db/pip/mod.rs`:

- Add `use super::name_validation::{validate_pep508_name, NameValidationError};` at the top.
- Add `fn filter_project_roots_by_name(project_roots: &[PathBuf]) -> (Vec<PathBuf>, usize)` per `data-model.md` §Entity 4.
- Insert the filter call at the top of `read()`, immediately before the `pyproject_declared_deps` loop (~line 343). Rebind `project_roots`:

```rust
let (project_roots, names_rejected) = filter_project_roots_by_name(&project_roots);
```

### Step 4: extend the reader-complete log

Find the `tracing::info!(... "pip reader complete")` call in `pip/mod.rs::read()`. Add `names_rejected` as a structured field.

### Step 5: verify unit tests pass

```bash
cargo test -p waybill --bin waybill scan_fs::package_db::name_validation
```

Expected: 14 tests pass (per contracts §Testing).

### Step 6: verify existing pip tests still pass

```bash
cargo test -p waybill --bin waybill scan_fs::package_db::pip
cargo test --test scan_python_m670
```

Expected: all pre-existing tests pass (FR-006 byte-identity for valid-name inputs).

### Step 7: rerun the bug reproduction from Step 0

```bash
cargo build -p waybill
RUST_LOG=info ./target/debug/waybill --offline sbom scan --path /tmp/repro-677 --format cyclonedx-json --output /tmp/jvm-post-fix.cdx.json 2>&1 | grep -iE "pypi|pip reader|name"
echo "---"
jq '[.components[].purl // "" | select(startswith("pkg:pypi/"))] | length' /tmp/jvm-post-fix.cdx.json
```

Expected post-fix output:

- Exactly one WARN log line: `pip: pyproject.toml [project].name failed PEP 508 validation; skipping whole manifest ... name={{package-name}} ...`
- `pip reader complete` log line includes `names_rejected=1`
- `jq` count of `pkg:pypi/*` components: **0** (whole-manifest reject — no main-module, no declared deps).

## Ship the fixture-based integration test

### Step 8: create the fixture

```bash
mkdir -p waybill-cli/tests/fixtures/pip/malformed_name_placeholder
cat > waybill-cli/tests/fixtures/pip/malformed_name_placeholder/pyproject.toml <<'EOF'
[project]
name = "{{package-name}}"
version = "0.0.0"
dependencies = [
    "waybill-fixture-real-dep-1",
    "waybill-fixture-real-dep-2",
]
EOF
```

### Step 9: create the integration test

`waybill-cli/tests/scan_python_m677.rs` per `data-model.md` §Entity 7. Uses the same subprocess-invocation pattern as `scan_python_m670.rs`:

- Scan the fixture with `--offline`
- Assert `.components | map(select(.purl // "" | startswith("pkg:pypi/"))) | length == 0`
- Assert stderr contains exactly one WARN line matching the expected format
- Assert `names_rejected=1` in the reader-complete log

### Step 10: run the integration test

```bash
cargo test --test scan_python_m677
```

Expected: 1 test passes.

## Full pre-PR gate

```bash
./scripts/pre-pr.sh
```

Expected: `>>> all pre-PR checks passed.`

## Common pitfalls

- **Do not add `#[serde(deny_unknown_fields)]` anywhere** — this fix operates on already-parsed `toml::Value` at name-extraction time. No serde struct changes.
- **Do not modify `build_pip_main_module_entry` or `pyproject_declared_deps` internals** — the filter-upstream pattern deliberately keeps those functions untouched. FR-006 byte-identity depends on this.
- **Do not fold validation into `build_pypi_purl_str`** — that function is called from lockfile readers (uv.lock, poetry.lock, requirements.txt, dist-info) too. This feature's scope is `pyproject.toml` only per the reproduction and spec. Follow-ups extending to lockfile-read paths are separate concerns.
- **PEP 508's regex is not the same as PyPI's normalized-name regex** — the regex validates raw declared names (preserving case + underscores); `normalize_pypi_name_for_purl` handles the PURL-normalization pass downstream. Do not merge these two.
- **The `[tool.poetry].name` fallback** — the filter must apply the SAME extraction logic as `build_pip_main_module_entry` at line 644-651 (prefer `[project].name`, fall back to `[tool.poetry].name`, otherwise skip validation — the manifest doesn't emit a main-module anyway).
