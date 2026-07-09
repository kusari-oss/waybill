# Phase 0 Research: Monorepo workspace-member visibility (m176)

**Feature**: 176-workspace-visibility
**Date**: 2026-07-08

Five research questions resolved. Every question was answerable by prior code inspection during the langflow + test-tensorflow-models audits + spec authoring; this file consolidates the answers so plan.md and tasks.md have single-point references.

---

## R1 — Where does workspace-membership information live in the codebase today?

**Decision**: **`ResolutionEvidence.source_file_paths` at `mikebom-common/src/resolution.rs:283`**, populated by each reader's `source_path` field on `PackageDbEntry` at `mikebom-cli/src/scan_fs/package_db/mod.rs:64-69`.

**Verified via**:
- Direct grep: `grep -rn "source_file_paths\|source_path:" mikebom-cli/src/scan_fs/package_db/` returns hits in every reader (`maven.rs`, `pip/`, `npm/`, `cargo.rs`, `go/`, etc.).
- Empirical: emitted SBOM contains `mikebom:source-files` per-component annotation carrying exactly these paths. On the tf-models scan, sampled values:
  ```
  official/projects/waste_identification_ml/circularnet-docs/themes/hugo-theme-techdoc/package-lock.json
  official/projects/movinet/requirements.txt
  official/projects/unified_detector/requirements.txt
  ```
  These are already root-relative, forward-slash-normalized.

**Implication**: m176 requires ZERO new reader logic. The workspace root is a pure derivation from `source_file_paths`: for each path, take `Path::parent()` (or `"."` if the path is a root-level manifest). The complete workspace-membership set is derivable at emission time.

**Alternatives considered**:
- **Add a new field to `ResolutionComponent`**: rejected. Adds plumbing without new information — every derivable-fact should be derived, not stored.
- **Add a `workspace_root: Option<String>` to `PackageDbEntry`**: rejected. Every reader would need to set it identically to `dirname(source_path)` — same-information duplication.

---

## R2 — How does `source_path` vary across readers?

**Decision**: two shapes that must both be handled by `derive_workspace_root()`:

1. **Relative root-relative filesystem path** — most common shape. Examples: `official/requirements.txt`, `src/frontend/package.json`, `Cargo.toml`. Standard for all lockfile-derived + manifest-derived entries.
2. **`path+file://<absolute-path>` URI** — used by pip main-modules per `pip/mod.rs:521` (`let source_path = format!("path+file://{}", project_root.display())`). Contains an absolute filesystem path.

The derivation logic must:
- For shape 1: `Path::parent().to_string_lossy().to_string()`. If parent is empty (root-level manifest), return `"."`.
- For shape 2: strip the `path+file://` prefix, then attempt to strip the scan-root prefix to get root-relative form. If stripping the scan-root fails (path is outside scan root — malformed evidence), return `None` and omit the annotation.

**Alternatives considered**:
- **Regex on the source_path string**: rejected — brittle. Just check for `path+file://` prefix with `str::strip_prefix`.
- **Refactor readers to always emit relative paths**: rejected — would ripple through every existing golden fixture and 10+ readers. Handle both shapes at derivation time; refactor later if desired.

---

## R3 — Where in the emitter pipeline should the two annotations be injected?

**Decision**: at the standard document-scope + per-component emission sites — same pattern as m172 C117, m173 C118/C119.

- **CDX 1.6**: in `mikebom-cli/src/generate/cyclonedx/metadata.rs` (doc-scope annotation next to C118/C119 emission block) + per-component within the CDX component emitter (where `mikebom:source-files` is already emitted).
- **SPDX 2.3**: in `mikebom-cli/src/generate/spdx/annotations.rs` (doc-scope envelope emit) + per-Package annotation emitter.
- **SPDX 3.0.1**: in `mikebom-cli/src/generate/spdx/v3_annotations.rs` (doc-scope typed `Annotation` graph element) + per-`software_Package` annotation emitter.

**Verified via**:
- `grep -n "mikebom:source-files" mikebom-cli/src/generate/` — 3 emission sites, one per format. m176 co-locates the C120 emission with existing `mikebom:source-files` code so consumers can visually compare the two (they encode adjacent concepts).

**Alternatives considered**:
- **Emit at the `ResolvedComponent → EmittedComponent` translation site**: rejected — too abstract. Each format's emitter already handles its own annotation shape; centralizing would require inventing a new abstraction.
- **Emit lazily via a jq post-processor**: rejected — post-hoc jq filtering can't emit into the same document-scope location natively; would require a wrapper tool. Native emission is simpler.

---

## R4 — How should the advisory log be worded and where should it fire?

**Decision**:
- **Location**: `mikebom-cli/src/cli/scan_cmd.rs`, at the same emission-tail site the m173 FR-004 + m175 FR-002 advisory logs would use. Just before the "SBOM written" `tracing::info!` line.
- **Predicate**: `workspaces_detected.len() > 1 && !components.is_empty()`. No offline gating.
- **Stable substring** (load-bearing per FR-004): the log line must contain the literal substring `"monorepo shape detected"` at the beginning + the workspace count + the workspace paths. Rest of the message wording is prose-level per spec Assumptions.

**Proposed exact text** (subject to authoring-time refinement):

```
monorepo shape detected: N workspaces (path1, path2, ...). Downstream consumers can filter per-workspace via `mikebom:workspace-member`; see docs/reference/monorepos.md for jq recipes.
```

**Alternatives considered**:
- **Fire the advisory in structured JSON via `tracing::info!` with fields**: partially adopted — the substring guarantee lets structured-log-mode emitters (JSON) include the same substring in the `message` field so `grep -F` on the rendered form matches. Same pattern as m173 FR-004.
- **Emit as a WARN**: rejected — matches m173/m175 convention that advisory logs are INFO level. A monorepo is not a defect; the log is informational.

---

## R5 — Are any existing golden fixtures affected by this change?

**Decision**: **YES — every existing Go / npm / pip / etc. golden fixture will gain**:
- Per-component: a new `mikebom:workspace-member = ["<workspace-path>"]` annotation on every workspace-attributable component.
- Doc-scope: a new `mikebom:workspaces-detected = ["<workspace-path>"]` annotation.

**Verified via**: inspection of `mikebom-cli/tests/fixtures/golden/cyclonedx/*.cdx.json` — every golden component today has a `mikebom:source-files` annotation, meaning every component IS workspace-attributable. m176 will double-emit alongside `source-files`.

**Regeneration is required.** SC-004 gate: verify post-regeneration that ONLY the two new annotations are added; no other bytes change. Verified by post-regen jq assertion:

```bash
jq -S 'del(.components[]?.properties[]? | select(.name == "mikebom:workspace-member")) | del(.metadata.properties[]? | select(.name == "mikebom:workspaces-detected"))' pre-176.cdx.json > /tmp/pre.stripped
jq -S 'del(.components[]?.properties[]? | select(.name == "mikebom:workspace-member")) | del(.metadata.properties[]? | select(.name == "mikebom:workspaces-detected"))' post-176.cdx.json > /tmp/post.stripped
diff /tmp/pre.stripped /tmp/post.stripped  # MUST be empty
```

**Alternatives considered**:
- **Skip goldens; use synthesized fixtures only**: rejected — the point of goldens is to detect emission drift, and m176 is emission-shape drift. Regenerating and gating on SC-004 is the correct action.

---

## Summary table

| ID | Question | Decision |
|---|---|---|
| R1 | Where does workspace-membership live? | `PackageDbEntry.source_path` → `ResolutionEvidence.source_file_paths` — already emitted as `mikebom:source-files` |
| R2 | How does `source_path` vary? | Two shapes: root-relative filesystem path AND `path+file://<abs>` URI (pip main-modules) — both handled in derive helper |
| R3 | Where in emitters? | Same sites as m172 C117 / m173 C118/C119 — one per format (CDX metadata, SPDX 2.3 annotations, SPDX 3 v3_annotations) |
| R4 | Advisory log wording / location? | Fire at emission-tail in scan_cmd.rs; stable substring `monorepo shape detected: N workspaces`; INFO level; no offline gating |
| R5 | Golden fixtures affected? | YES — every golden regenerates with +2 annotations; SC-004 gate verifies no other byte changes |
