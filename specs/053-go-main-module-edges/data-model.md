# Data Model: milestone 053 (Go main-module edges)

## New & changed entities

### `MainModuleEntry` (constrained shape of the existing `PackageDbEntry`)

The new main-module is **not** a new struct — it's a `PackageDbEntry` (already defined at `mikebom-cli/src/scan_fs/package_db/mod.rs:53`) with a constrained shape produced by the new factory function `golang::build_main_module_entry()`. Constraints:

| Field | Value | Source / rationale |
|-------|-------|--------------------|
| `purl` | `Purl::new("pkg:golang/<module-path>@<version>")` | FR-001. `<module-path>` from `GoModDocument.module_path`; `<version>` from `resolve_workspace_version()` ladder. |
| `name` | `<module-path>` (e.g., `github.com/argoproj/argo-workflows`) | FR-001. Bare module path, no version suffix. |
| `version` | Output of 3-step ladder | FR-001. Possible values: `v3.3.9`, `v3.3.9-2-gabc1234`, `v0.0.0-unknown`. |
| `arch` | `None` | Go modules are not arch-specific. |
| `source_path` | `<workspace-root>/go.mod` | Used by the existing edge-emission loop's `Relationship.provenance.source`. |
| `depends` | `Vec<String>` of post-`apply_replace_and_exclude` direct require paths | FR-002. Includes `// indirect` requires per the deliberate Trivy-divergence note in spec Edge Cases. |
| `maintainer` | `None` | No upstream registry to query for the project itself. |
| `licenses` | `vec![]` | FR-005. Empty per the LICENSE-detection deferral (issue #103). |
| `concluded_licenses` | `vec![]` | Same. |
| `hashes` | `vec![]` | The project itself isn't content-hashable in the same sense as a downloaded `.zip`; deep-hash applies per-file but not at the workspace level. |
| `cpes` | `vec![]` | Generator may synthesize a CPE from name+version per existing `metadata.rs::cpe_sanitize` if the main-module is promoted to `metadata.component`. |
| `advisories` | `vec![]` | n/a |
| `occurrences` | `vec![]` | n/a |
| `lifecycle_scope` | `None` | Main-module is implicit Runtime; explicitly leaving as `None` matches "Runtime-by-default" semantics elsewhere. |
| `requirement_range` | `None` | n/a |
| `source_type` | `None` | The main-module is the workspace itself, not a registry/git/path-sourced dep. |
| `sbom_tier` | `Some("source")` | FR-006. The `go.mod` is the authoritative source. |
| `buildinfo_status` | `None` | Source-tree path; BuildInfo path may merge later per FR-009 dedup. |
| `evidence_kind` | `None` | Optional metadata not load-bearing. |
| `binary_class` / `binary_stripped` / `binary_packed` | `None` / `None` / `None` | Source-tree, not binary. |
| `linkage_kind` | `None` | n/a |
| `detected_go` | `Some(true)` | Trivially true; helps the parity-extractor framework recognize the entry as Go-ecosystem. |
| `confidence` | `None` | The main-module is observed-with-certainty (it's the scan target itself). |
| `npm_role` | `None` | n/a — Go-only. |
| `raw_version` | `None` | The version IS the resolved version per the ladder; no upstream "raw" form. |
| `parent_purl` | `None` | **Critical** — must be `None` so the SPDX root-selection algorithm at `generate/spdx/document.rs:248-281` picks it as a top-level component (case 1 single-top-level OR case 3 name-match). |
| `co_owned_by` | `vec![]` | n/a |
| `shade_relocation` | `None` | Maven-specific; n/a here. |
| `external_references` | per existing `external_refs_from_purl()` | Same as any Go component. |
| `extra_annotations` | `vec![{ name: "mikebom:component-role", value: "main-module" }]` | FR-004 — supplementary C40 signal layered on top of the native-field placement. |

### `SpdxPrimaryPackagePurpose` (NEW enum)

Added to `mikebom-cli/src/generate/spdx/packages.rs`:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub enum SpdxPrimaryPackagePurpose {
    #[serde(rename = "APPLICATION")] Application,
    #[serde(rename = "FRAMEWORK")] Framework,
    #[serde(rename = "LIBRARY")] Library,
    #[serde(rename = "CONTAINER")] Container,
    #[serde(rename = "OPERATING-SYSTEM")] OperatingSystem,
    #[serde(rename = "DEVICE")] Device,
    #[serde(rename = "FIRMWARE")] Firmware,
    #[serde(rename = "SOURCE")] Source,
    #[serde(rename = "ARCHIVE")] Archive,
    #[serde(rename = "FILE")] File,
    #[serde(rename = "INSTALL")] Install,
    #[serde(rename = "OTHER")] Other,
}
```

Per SPDX 2.3 spec §7.24. Enum exhaustiveness matches the spec (12 values). Milestone 053 only constructs `Application`; the others are reserved for future ecosystems' main-modules (#104) and for any other use case where a `primaryPackagePurpose` distinction adds signal. Added as `Option<SpdxPrimaryPackagePurpose>` on `SpdxPackage` with `#[serde(skip_serializing_if = "Option::is_none")]` so existing packages without purpose stay byte-identical.

### `WorkspaceVersionResolution` (internal, not a public type)

The version ladder is encapsulated in a private function:

```rust
/// Resolve the synthetic main-module version per FR-001's 3-step ladder.
/// Honors a 2-second timeout on every git subprocess.
fn resolve_workspace_version(project_root: &Path) -> String {
    // 1. git describe --tags --exact-match HEAD
    // 2. git describe --tags --always
    // 3. literal "v0.0.0-unknown"
}
```

No public type emerges; the `String` return is consumed directly by `build_main_module_entry()`. Errors from git invocation (binary missing from `$PATH`, no `.git` dir, subprocess timeout, non-zero exit) ALL collapse to step 3 — there's no observable error path beyond the resulting string.

## Field-relationship matrix

```text
┌──────────────────────────┐      depends[]     ┌─────────────────────────────┐
│ MainModuleEntry          │ ─────────────────▶ │ Existing go.sum-derived     │
│  purl: pkg:golang/X@v3.x │                    │  PackageDbEntry components  │
│  parent_purl: None       │                    │  (transitives, leaf nodes)  │
│  sbom_tier: source       │                    └─────────────────────────────┘
│  extra: C40=main-module  │                              ▲
└──────────────────────────┘                              │
        │                                                 │
        │                                                 │ existing edge-
        ▼                                                 │ emission loop
┌──────────────────────────┐                              │ (scan_fs/mod.rs)
│ CDX metadata.component   │ ── dependencies[].ref ──────┘
│  type: application       │
│  primary signal per      │
│  Principle V             │
└──────────────────────────┘

┌──────────────────────────┐
│ SPDX 2.3 Package entry   │ ─── primaryPackagePurpose: "APPLICATION"
│  spdxid: SPDXRef-Pkg-... │ ─── documentDescribes[]: [SPDXRef-Pkg-...]
│  rel: SPDXRef-DOCUMENT   │ ─── DESCRIBES → SPDXRef-Pkg-...
└──────────────────────────┘
```

## Validation rules

- **Uniqueness**: at most one `MainModuleEntry` per discovered `go.mod`. The Go reader's `seen_purls: HashSet<String>` dedup logic continues to apply; a synthetic main-module that happens to collide with a `go.sum` entry's PURL (impossible in practice — the workspace's own module is intentionally excluded from go.sum) would be deduplicated by the existing rule.
- **Determinism**: given identical `go.mod` + `LICENSE` + (lack of `.git`) inputs, the produced entry's PURL, name, version, depends list, and extra_annotations MUST be byte-identical across hosts. Verified by golden regen + cross-host CI.
- **Ordering**: when emitting the entry into the components vec, position matters for goldens. Pre-053 the SPDX `build_document` algorithm prepends the synthetic root; post-053 the root selection picks the main-module via case 1 / case 3 of the existing logic, which doesn't move the entry — it stays wherever the Go reader inserted it (position-irrelevant for SPDX since it's referenced by spdxid). For CDX, the main-module is **excluded** from the top-level `components[]` (it lives in `metadata.component`), so position is moot.

## State transitions

None. The main-module entry is constructed once per scan and frozen. The resolution-pipeline rewrites at `scan_fs/mod.rs:571+` (`apply_lifecycle_scope_to_edges` from milestone 052) operate on the entry's outgoing edges but leave the entry itself unchanged.
