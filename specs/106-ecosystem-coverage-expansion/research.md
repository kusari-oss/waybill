# Phase 0 Research: Ecosystem Coverage Expansion (Phase 1)

**Milestone**: 106
**Date**: 2026-05-31
**Status**: Complete; one finding (R1) triggered a spec correction.

## Summary of findings

| ID | Topic | Finding | Spec/Plan impact |
|---|---|---|---|
| R1 | "Existing tsconfig.json JSONC handling" claim | **No such helper exists in the codebase.** The spec's reference was a hallucination during initial drafting. | **Spec corrected**: FR-003 and Clarifications Q4 now reference the actual in-tree comment-stripper patterns (`gem::strip_ruby_comment`, `golang::legacy::strip_line_comment`) instead of a non-existent tsconfig handler. New `npm/jsonc.rs` is built from scratch mirroring those proven patterns. |
| R2 | Milestone-052 `lifecycle_scope` → CDX `scope: "excluded"` mapping | Confirmed at `mikebom-cli/src/generate/cyclonedx/builder.rs:590-605`. Any non-Runtime `LifecycleScope` value emits CDX `scope: "excluded"`. SPDX 2.3 uses native `DEV/BUILD/TEST_DEPENDENCY_OF` relationships (`generate/spdx/relationships.rs:79-91`). | No spec correction needed — the existing infrastructure handles Gradle buildscript and NuGet `PrivateAssets="All"` without changes. Just set `lifecycle_scope: Some(Build)` on the relevant PackageDbEntry. |
| R3 | Parity catalog row for `mikebom:component-role` | **Row C40**, located at `docs/reference/sbom-format-mapping.md:86-87`. Currently documented enum values: `"build-tool"`, `"language-runtime"`, `"main-module"`. The annotation is **open-enum** (not closed) — adding `"workspace-root"` requires only a doc-row update, no parity-extractor change. Directionality: SymmetricEqual (effectively, via the existing extractor). | Plan's Constitution Check + project structure reflect: doc-only update to the C40 row's enum list. Spec's workspace-root references (Clarifications Q1 + FR-015) work as-written. |
| R4 | uv workspace lockfile schema | Confirmed: workspace members emit as `[[package]]` entries with `source = { workspace = true }`. Intra-workspace edges appear under `[[package.dependencies]]` for those members. Root `pyproject.toml` declares `[tool.uv.workspace]` with `members = ["apps/web", "libs/shared"]`. | Spec's US1 scenario 5 + edge case rewrite work as-written. |
| R5 | Bun workspace lockfile schema | Confirmed: root `package.json` has `workspaces: ["packages/*"]`. Workspace members appear in `bun.lock`'s `packages` map keyed by `"@scope/name@workspace:packages/path/to/member"`. `"workspace:*"` source declarations in member `package.json` files mark intra-workspace deps. | Spec's US2 scenario 4 + edge case work as-written. |
| R6 | Test fixture placement (in-repo vs external) | Confirmed convention: **in-repo at `mikebom-cli/tests/fixtures/golden_inputs/<ecosystem>/`** for small + tightly-coupled fixtures (milestone 105's `dedup_collision/` follows this pattern). External `mikebom-test-fixtures` repo (milestone 090) only for large fixtures reused across versions. The 4 milestone-106 fixtures (uv_lock, bun_lock, gradle_lockfile, nuget) all qualify as in-repo per the < 5 KB-each + tightly-coupled criterion. | Plan's project-structure section reflects in-repo placement. No external fixtures repo updates required. |

---

## R1: tsconfig.json JSONC claim — corrected

**Finding**: The spec's original Clarification Q4 and FR-003 both referenced "the existing tsconfig.json handling in the npm reader" as a JSONC-stripping precedent for the new `bun.lock` parser. Empirical check (`grep -r tsconfig mikebom-cli/src/scan_fs/package_db/npm/`) returns no results. The reference was a drafting hallucination — npm reader doesn't parse tsconfig.json at all.

**What IS in the codebase**:

- `scan_fs/package_db/gem.rs:149` — `strip_ruby_comment()`: line-comment scanner for Gemfile (`# foo`).
- `scan_fs/package_db/golang/legacy.rs:82-84` — `strip_line_comment()`: line-comment scanner for `go.mod` (`// foo`).

Both are simple single-line strippers. Neither handles `/* */` block comments. For `bun.lock`, we need both `//` (line) and `/* */` (block) handling because the JSONC spec admits both forms. Plan ships a new `npm/jsonc.rs` helper from scratch (~20 LOC + ~10 unit tests) that handles both. The string-literal boundary awareness (don't strip text inside `"..."` strings) is the only subtlety — the new helper handles it explicitly.

**Spec correction**: Both Clarification Q4's narrative and FR-003 now reference the actual in-tree precedents (`gem::strip_ruby_comment`, `golang::legacy::strip_line_comment`) rather than the non-existent tsconfig helper. Applied before plan.md was written.

---

## R2: milestone-052 lifecycle-scope mapping

**Decision**: Reuse the existing mapping for Gradle buildscript and NuGet `PrivateAssets="All"`. No code changes needed.

**Evidence**:

- **`mikebom_common::resolution::LifecycleScope`** enum has 4 variants: `Runtime`, `Development`, `Build`, `Test` (per `mikebom-common/src/resolution.rs:~212`).
- **`PackageDbEntry.lifecycle_scope: Option<LifecycleScope>`** field at `scan_fs/package_db/mod.rs:~63` carries the value through to emission.
- **CDX emission** at `generate/cyclonedx/builder.rs:590-605`: any non-Runtime value triggers `scope: "excluded"` on the CDX component output. The finer distinction (Development vs Build vs Test) lives in the `mikebom:lifecycle-scope` property (CDX `scope` enum can't express 4-way).
- **SPDX 2.3 emission** at `generate/spdx/relationships.rs:79-91`: uses native relationship types `DEV_DEPENDENCY_OF`, `BUILD_DEPENDENCY_OF`, `TEST_DEPENDENCY_OF` (with reversed direction per SPDX 2.3 spec).
- **SPDX 3.0.1 emission** at `generate/spdx/relationships.rs:37-38`: mentions `lifecycleScope` parameter on `dependsOn` relationships.

**Consequence for milestone 106**: just set `lifecycle_scope: Some(LifecycleScope::Build)` on:
- Gradle buildscript-gradle.lockfile entries
- NuGet `<PackageReference>` entries with `PrivateAssets="All"` (or `IncludeAssets` omitting `runtime`, or `ExcludeAssets="runtime"`)

All three output formats handle the rest automatically.

---

## R3: parity catalog row for `mikebom:component-role`

**Decision**: Open-enum extension — add `"workspace-root"` value to the C40 row doc; no parity-extractor changes.

**Evidence**:

- **Row C40** is at `docs/reference/sbom-format-mapping.md:86-87`.
- Documented enum values today: `"build-tool"`, `"language-runtime"`, `"main-module"`.
- The annotation is open-enum (the existing emission code accepts any string value; the catalog row's enum list is descriptive, not enforcing).
- Existing parity extractor `c40_cdx`/`c40_spdx23`/`c40_spdx3` are simple string-value extractors via the standard `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` macros. No change needed — they pass `"workspace-root"` through identically.

**Implementation step**: milestone 106's task for the catalog update is a single doc-line addition to `docs/reference/sbom-format-mapping.md:86-87` listing `"workspace-root"` alongside the existing 3 enum values, with a brief note that this value is emitted by uv/Bun workspace synthetic roots.

---

## R4: uv workspace lockfile schema

**Decision**: implement per the spec's US1 scenario 5 — workspace members are first-class `[[package]]` entries with `source = { workspace = true }`.

**Schema confirmed** (from uv 0.5+ lockfile format):

```toml
# Root pyproject.toml
[tool.uv.workspace]
members = ["apps/web", "libs/shared"]

# uv.lock
[[package]]
name = "web"
version = "0.1.0"
source = { workspace = true }

[[package.dependencies]]
name = "shared"
# workspace-source intra-workspace dep

[[package]]
name = "shared"
version = "0.1.0"
source = { workspace = true }
```

The implementation walks all `[[package]]` entries:
1. Entries with `source = { workspace = true }` emit as workspace-member components per Clarification Q1 (with `mikebom:component-role: "main-module"`).
2. The synthetic workspace-root component is derived from the root `pyproject.toml`'s `name` field (or `"workspace-root"` placeholder when absent), with `component-role: "workspace-root"`.
3. `[[package.dependencies]]` arrays drive the dependsOn edges, including intra-workspace edges between members.
4. Other `[[package]]` entries (PyPI-sourced) emit normally as `pkg:pypi/...` components.

---

## R5: Bun workspace lockfile schema

**Decision**: implement per the spec's US2 scenario 4 — bun.lock packages map keyed by `"@scope/name@workspace:path/to/member"` for workspace members.

**Schema confirmed** (from Bun 1.2+ lockfile format):

```jsonc
// bun: lockfileVersion: 1
{
  "lockfileVersion": 1,
  "workspaces": {
    "": { "name": "monorepo-root" },
    "packages/web": { "name": "@my/web", "dependencies": { "@my/shared": "workspace:*" } },
    "packages/shared": { "name": "@my/shared" }
  },
  "packages": {
    "@my/web@workspace:packages/web": { "...": "..." },
    "@my/shared@workspace:packages/shared": { "...": "..." },
    "lodash@4.17.21": { "...": "..." }
  }
}
```

The implementation:
1. Parses with the new JSONC stripper (R1).
2. Walks `workspaces` keys (excluding `""` which is the root) to enumerate workspace members.
3. Resolves each `"workspace:*"` source in a member's `dependencies` to an intra-workspace edge.
4. Emits the synthetic workspace-root from `workspaces[""].name`.

---

## R6: test fixture placement convention

**Decision**: in-repo at `mikebom-cli/tests/fixtures/golden_inputs/<ecosystem>/` for all four milestone-106 fixtures.

**Convention** (established by milestone 105 + cited in milestone 090):
- In-repo: small fixtures (< 5 KB each), tightly coupled to a specific milestone's implementation, not reused across mikebom versions.
- External (`mikebom-test-fixtures` git repo): large fixtures (e.g., real-world npm trees, multi-MB JAR archives) reused across versions.

All four milestone-106 fixtures qualify as in-repo:
- `uv_lock/`: ~5 small lockfile fixtures (TOML, each ~1 KB).
- `bun_lock/`: ~4 fixtures with JSONC content (~1-2 KB each).
- `gradle_lockfile/`: ~3 line-format fixtures (sub-1 KB each).
- `nuget/`: ~4 fixtures with XML + JSON content (~1-3 KB each).

Total addition: ~80 KB across ~16 small fixture trees. Comfortably under the in-repo threshold.

---

## Implementation strategy summary

Given the 6 research items, the plan's structure is straightforward:

1. **Phase 2A** (foundational): JSONC stripper in `npm/jsonc.rs` (R1). One small, well-tested helper that US2 depends on.
2. **Phase 2B** (US1 prerequisite): TOML workspace-detection helper (shared with future Cargo workspace work if scope grows). May be folded into US1 itself.
3. **Phase 3-6** (per-US): four user-story phases shipping in priority order (US1 uv → US2 bun → US3 gradle → US4 nuget).
4. **Phase 7** (catalog + docs): C40 row update; `docs/ecosystems.md` coverage matrix update; release cut.

No constitutional blockers, no audit deferrals, no architectural surprises. Clean milestone post-research.
