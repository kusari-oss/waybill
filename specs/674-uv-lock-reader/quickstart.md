# Quickstart — m674 uv.lock reader

**Feature**: 674-uv-lock-reader
**Audience**: Implementer picking up this milestone after `/speckit.tasks` runs.

## Goal

Add a new `uv` package_db reader that parses Astral's `uv.lock`
TOML-format lockfiles and emits components with the same PURL +
hash + annotation shape as the m223 Pants PEX reader. Two ingest
paths: `<scan_root>/uv.lock` (standalone) + m673 Pants pipeline
FR-002 fallback (for Pants monorepos using uv as resolver backend).

## Files you'll touch

New module + 4 plumbing edits + 1 new integration test file + new
parity C-row. Zero `Cargo.toml` changes.

```text
waybill-cli/src/scan_fs/package_db/uv/           # NEW
├── mod.rs                                       # ~80 lines
├── lockfile.rs                                  # ~200 lines
└── source_variant.rs                            # ~50 lines
waybill-cli/src/scan_fs/package_db/
├── mod.rs                                       # +5 lines (register uv in read_all)
└── pants/mod.rs                                 # +10 lines (FR-002 fallback)
waybill-cli/src/parity/extractors/
├── mod.rs                                       # +3 lines (C157 registration)
├── cdx.rs                                       # +2 lines (c157_cdx)
├── spdx2.rs                                     # +2 lines (c157_spdx23)
└── spdx3.rs                                     # +2 lines (c157_spdx3)
waybill-cli/tests/
└── scan_uv_lock_m674.rs                         # NEW
waybill-cli/tests/fixtures/uv_lock/              # NEW
├── minimal_uv/{pyproject.toml, uv.lock}
├── multi_source/{pyproject.toml, uv.lock}
└── pants_uv_backend/{pants.toml, 3rdparty/python/*.lock}
docs/reference/sbom-format-mapping.md            # +1 row (C157)
```

## Verification recipe

### Step 1 — Scaffold the new `uv/` module

Create `waybill-cli/src/scan_fs/package_db/uv/mod.rs` + `lockfile.rs` +
`source_variant.rs`. Wire the module into `package_db/mod.rs`:

```rust
pub(crate) mod uv;
```

Register the reader in the `read_all` dispatcher (search for how
`pants::read` is invoked — mirror that shape). At this task, the
module compiles but does no real work.

Verify:
```bash
cargo +stable check -p waybill
```

### Step 2 — Implement `UvSource` enum + PURL helpers in `source_variant.rs`

Per `contracts/source_variants.md` + `data-model.md` §"Enum 1":

```rust
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(untagged)]
pub(crate) enum UvSource {
    Registry { registry: String },
    Git { git: String, rev: String },
    Path { path: String },
    Url { url: String },
    Editable { editable: String },
    Virtual { virtual: String },
}

impl UvSource {
    /// Returns None for Editable + Virtual (FR-006 SKIP).
    pub(crate) fn build_purl(&self, name: &str, version: &str) -> Option<Purl> {
        match self {
            UvSource::Registry { .. } => {
                let normalized = pip::normalize_pypi_name_for_purl(name);
                Some(Purl::new("pypi", &normalized, version, ...)?)
            }
            UvSource::Git { .. } | UvSource::Path { .. } | UvSource::Url { .. } => {
                Some(Purl::new("generic", name, version, ...)?)
            }
            UvSource::Editable { .. } | UvSource::Virtual { .. } => None,
        }
    }

    pub(crate) fn build_source_annotations(&self, registry_is_pypi: bool) -> Vec<(String, String)> {
        // per contracts/source_variants.md §C1-C4
    }
}
```

Add inline unit tests covering the 8-row test matrix from
`contracts/source_variants.md` (registry-default, registry-custom,
git, path-abs, path-rel, url, editable-skip, virtual-skip).

### Step 3 — Implement `uv/lockfile.rs`

Per `data-model.md` §"Struct 2" + `contracts/uv_lockfile_schema.md`.
Shape:

```rust
#[derive(Debug, serde::Deserialize)]
pub(crate) struct UvLockfile { /* per data-model.md */ }

#[derive(Debug, serde::Deserialize)]
pub(crate) struct UvPackage { /* per data-model.md */ }

#[derive(Debug, serde::Deserialize)]
pub(crate) struct UvHashArtifact { /* per data-model.md */ }

pub(crate) fn parse(bytes: &[u8]) -> Option<UvLockfile> {
    let text = std::str::from_utf8(bytes).ok()?;
    let lockfile: UvLockfile = toml::from_str(text)
        .map_err(|e| tracing::warn!(error = %e, "uv-lock reader: parse failed; skipping"))
        .ok()?;
    if lockfile.version != 1 {
        tracing::warn!(
            version = lockfile.version,
            "uv-lock reader: unsupported uv.lock schema version (expected 1); skipping"
        );
        return None;
    }
    Some(lockfile)
}

pub(crate) fn to_entry(
    package: &UvPackage,
    source_file: &Path,
    pants_resolve_name: Option<&str>,
) -> Option<PackageDbEntry> {
    let purl = package.source.build_purl(&package.name, &package.version)?;
    let mut hashes = Vec::new();
    if let Some(sdist) = &package.sdist {
        if let Some(h) = parse_sha256_hash(&sdist.hash) { hashes.push(h); }
    }
    for wheel in &package.wheels {
        if let Some(h) = parse_sha256_hash(&wheel.hash) { hashes.push(h); }
    }
    hashes.sort_by(|a, b| a.value.cmp(&b.value));
    hashes.dedup_by(|a, b| a.value == b.value);
    // ... construct PackageDbEntry with annotations per contracts/source_variants.md §C7
}
```

Add inline unit tests for the 7-row test matrix in
`contracts/uv_lockfile_schema.md`.

### Step 4 — Implement `uv/mod.rs::read` orchestrator

Standalone uv discovery path per FR-001:

```rust
pub fn read(scan_root: &Path) -> Vec<PackageDbEntry> {
    let uv_lock_path = scan_root.join("uv.lock");
    if !uv_lock_path.exists() {
        return Vec::new();  // FR-013 byte-identity for non-uv repos
    }
    let bytes = match std::fs::read(&uv_lock_path) { ... };
    let Some(lockfile) = lockfile::parse(&bytes) else { ... };
    let mut components = Vec::new();
    for package in &lockfile.packages {
        if let Some(entry) = lockfile::to_entry(package, &uv_lock_path, None) {
            components.push(entry);
        }
    }
    tracing::info!(
        lockfiles_discovered = 1_usize,
        lockfiles_parsed_ok = 1_usize,
        components_emitted = components.len(),
        "uv reader complete"
    );
    components
}
```

### Step 5 — Wire the Pants FR-002 fallback in `pants/mod.rs`

Per `contracts/pants_integration.md` C1–C6. Modify the parse loop:

```rust
// Existing m223 loop; extended per FR-002.
for candidate in &candidates {
    let bytes = std::fs::read(&candidate.path)?;
    match lockfile::parse(&bytes) {
        Some((pex, was_legacy_shape)) => { /* existing m223+m672+m673 path */ }
        None => {
            // m674 FR-002 fallback: try uv.
            if let Some(uv_lockfile) = crate::scan_fs::package_db::uv::lockfile::parse(&bytes) {
                tracing::info!(
                    lockfile = %candidate.path.display(),
                    packages = uv_lockfile.packages.len(),
                    "uv-lock reader: recognized as uv.lock format after Pex parse rejection"
                );
                for package in &uv_lockfile.packages {
                    if let Some(entry) = crate::scan_fs::package_db::uv::lockfile::to_entry(
                        package,
                        &candidate.path,
                        Some(&candidate.resolve_name),  // FR-002 propagate Pants tag
                    ) {
                        components.push(entry);
                    }
                }
            }
        }
    }
}
```

### Step 6 — Register C157 in the parity catalog

Per `data-model.md` §"New parity catalog row" + m670 C154 / m671
C156 precedent:

- `docs/reference/sbom-format-mapping.md` — add a C157 row after
  C156.
- `waybill-cli/src/parity/extractors/cdx.rs` — add
  `cdx_anno!(c157_cdx, "waybill:python-lockfile-format", component);`
- `waybill-cli/src/parity/extractors/spdx2.rs` — add
  `spdx23_anno!(c157_spdx23, "waybill:python-lockfile-format", component);`
- `waybill-cli/src/parity/extractors/spdx3.rs` — add
  `spdx3_anno!(c157_spdx3, "waybill:python-lockfile-format", component);`
- `waybill-cli/src/parity/extractors/mod.rs` — register
  `ParityExtractor { row_id: "C157", label: "waybill:python-lockfile-format", ..., directional: SymmetricEqual, order_sensitive: false }` +
  add 3 name imports.

### Step 7 — Create committed fixtures under `waybill-cli/tests/fixtures/uv_lock/`

Per `research.md` §R8. Three fixtures:

1. `minimal_uv/`:
   - `pyproject.toml` — trivial `[project]` with 3 deps.
   - `uv.lock` — 3 registry-sourced packages + 1 transitive.

2. `multi_source/`:
   - `uv.lock` — one of each variant (registry, git, path, url,
     editable, virtual) — 6 packages total.

3. `pants_uv_backend/`:
   - `pants.toml` with `[python.resolves]` naming 2 files under
     `3rdparty/python/`.
   - `3rdparty/python/python-default.lock` — uv-shape (~5 packages).
   - `3rdparty/python/tools.lock` — uv-shape (~3 packages).

All package names use `waybill-fixture-*` prefix.

### Step 8 — Write integration tests

Create `waybill-cli/tests/scan_uv_lock_m674.rs` with tests for:

1. `standalone_uv_project_emits_pypi_components` — SC-001 (US1)
2. `multi_source_variants_emit_correctly` — every FR-004 through FR-007 (US1)
3. `editable_and_virtual_are_skipped` — FR-006 (US1)
4. `pants_uv_backend_recovers_components` — SC-002 (US2)
5. `pants_resolve_annotation_preserved_via_fr002_fallback` — C4 (US2)
6. `mixed_pex_and_uv_lockfiles_both_emit` — US2 AS2
7. `pyproject_declared_deps_deduped_against_uv_lock` — SC-005 (US3)
8. `pre_m674_byte_identity_on_non_uv_repos` — SC-004 regression guard
9. `version_2_uv_lock_rejected_with_warn` — C1 (version-gate)

Reuse the m672 / m673 `strip_ansi` + `run_scan` + `component_purls`
helpers.

### Step 9 — Byte-identity guards

```bash
cargo +stable test -p waybill --test pants_pex_reader \
    --test scan_pants_m672 --test scan_pants_m673 \
    --test scan_uv_lock_m674 2>&1 | grep 'test result:'
```

Expected:
- `pants_pex_reader`: 10/10 (m223 goldens — unchanged).
- `scan_pants_m672`: 10/10 (m672 tests — unchanged).
- `scan_pants_m673`: 6/6 (m673 tests — unchanged).
- `scan_uv_lock_m674`: 9/9 (new tests).

### Step 10 — Real-world smoke test

Clone `meilisearch/meilisearch-python` + `lablup/backend.ai` +
scan both. Expected:

- `meilisearch-python`: ≥ 50 pypi components from `uv.lock` (SC-003).
- `backend.ai`: ≥ 400 pypi components across 9 uv-shape lockfiles
  (SC-002; was 133 pre-m674 from pyproject.toml fallback alone).

Save artifacts under `specs/674-uv-lock-reader/artifacts/`.

### Step 11 — Pre-PR gate

```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 \
  PATH="/Users/mlieberman/Projects/mikebom/.venv/spdx3-validate/bin:$PATH" \
  ./scripts/pre-pr.sh
```

Expected: `>>> all pre-PR checks passed.`

## What to skip (v2 extension points)

- **`uv.lock` schema v2+**: WARN + skip in v1 per FR-003.
- **Recursive `**/uv.lock` discovery**: only `<root>/uv.lock` in v1
  (per Assumptions in spec.md).
- **`resolution-markers` filtering**: every locked package emits
  regardless of marker constraints (v2 extension point).
- **Wheel-per-platform expansion**: one component per (name,
  version) with multi-hash attachment.
- **Content-detect gate at m673 discovery**: try-PEX-then-uv sequence
  is cheap enough; adding a `is_uv_lockfile_content` helper is a
  v2 optimization if empirically needed.

## Pre-implementation grep check (per feedback_no_customer_names_in_code_or_docs)

Before opening the PR, grep everything staged for customer names +
competitor names (Tier 2 audit per the memory-recorded tier-based
policy):

```bash
grep -rEi '<blocklist-pattern>' $(git diff --cached --name-only) && exit 1
```

Zero hits required. The blocklist pattern is documented in the
memory note; run the grep with the actual customer/competitor names
per that policy.
