# Phase 0 Research: `--tier=<mode>` output-filter flag

**Feature**: 232-tier-filter-flag
**Date**: 2026-08-10

Both open ambiguities from spec drafting were resolved in the `/speckit.clarify` session (see `spec.md § Clarifications`). No open NEEDS CLARIFICATION items.

## R1 — Where does the filter insertion point live?

**Decision**: Insert the tier filter at `waybill-cli/src/cli/scan_cmd.rs` right after the existing `--exclude-scope` filter block (line 3175–3199), before the format-builder dispatch at line 3200+. Same shape (`components.retain(...)` + `relationships.retain(...)` + count-log INFO), same location in the pipeline.

**Rationale**: The pipeline order (line 3150+) is:
1. `deduplicator::deduplicate(components)` — cross-reader dedup
2. `reconciler::reconcile_design_source_tiers(...)` — m191 design/source reconciliation
3. `--exclude-scope` filter (existing)
4. **[NEW: --tier filter]**
5. Format-builder dispatch (each format runs `compute_graph_completeness` internally over the incoming components + relationships slice)

Because graph-completeness runs INSIDE each format builder (verified: `cyclonedx/builder.rs:644`, `spdx/document.rs:715`, `spdx/v3_document.rs:777`), filtering BEFORE dispatch means all three formats' annotations naturally reflect the filtered set — no emitter-side changes needed. This is the SC-004 requirement satisfied for free.

**Alternatives considered**:
- *Insert filter INSIDE each format builder.* Rejected: triples the touch surface (three builders), requires each to duplicate the filter logic, and risks drift between formats.
- *Insert filter BEFORE `reconciler::reconcile_design_source_tiers`.* Rejected: the reconciler correctly merges design-tier and source-tier siblings pre-filter. Filtering first would prevent that merge and produce wrong output (e.g., a source-tier component would be missing its design-tier sibling's evidence contributions).
- *Insert filter AFTER graph-completeness computation.* Rejected: violates SC-004 — annotations would reflect the pre-filter graph.

## R2 — Existing `--exclude-scope` pattern as the template

**Decision**: Model the `--tier` filter EXACTLY on the `--exclude-scope` pattern at `scan_cmd.rs:3175-3199`. That block:

1. Skips its work when the exclude set is empty (default value).
2. Computes a `HashSet<String>` of dropped-component PURLs.
3. Retains components not in the dropped set.
4. Retains edges whose `from` AND `to` are not in the dropped set.
5. Emits an `INFO` log with the drop count.

The `--tier` filter reuses steps 1–5 verbatim, changing only the predicate (`c.lifecycle_scope.is_some_and(...)` → `!tier_matches(c.sbom_tier.as_deref(), mode)`). Same helper shape; same pipeline placement; same drop-log convention.

**Rationale**: Reduces the review surface — reviewers know this pattern already. Matches the m230/m231 convention of following the closest existing sibling exactly.

**Alternatives considered**:
- *Introduce a `Filter` trait and refactor `--exclude-scope` to use it.* Rejected: adds abstraction for two callers. Constitution Principle IV favors direct code over premature generalization.

## R3 — `sbom_tier` value inventory

**Decision**: The existing `sbom_tier: Option<String>` field on `waybill_common::resolution::ResolvedComponent` (line 103) has the following values in production emission today:

| Value | Meaning | Emitted by (representative) |
|---|---|---|
| `Some("source")` | Manifest-declared or lockfile-declared with resolved version | cargo, npm, gem, nuget, maven, pip, gomod, ... |
| `Some("design")` | Manifest-declared with UNRESOLVED version (design-tier fallback) | m655 nuget, m064 cargo main-modules on workspaceless roots, etc. |
| `Some("binary")` | Binary-artifact-derived (ELF/PE/Mach-O readers, PE-CLR/DLL reader, etc.) | m130 pe_clr, m129 elf/macho readers |
| `Some("analyzed")` | Enrichment-tier (deps.json, .buildinfo, etc.) | m129 deps.json (.NET runtime), m033 buildinfo |
| `Some("file")` | File-tier orphan fallback (m133 content-addressed) | m133 file-tier reader |
| `None` | Rarely — legacy path, no primary tier assigned | ~0.1% of emissions historically |

Per spec Clarifications §1 (strict-literal match):
- `--tier=source-only` retains only `Some("source")`
- `--tier=design-only` retains only `Some("design")`
- `--tier=source-and-binary` retains `Some("source")` OR `Some("binary")`

Components with `Some("analyzed")`, `Some("file")`, or `None` are dropped under all three non-default modes.

**Rationale**: Strict literal match matches the operator's mental model (flag name = filter). Future modes are trivial one-line enum extensions if operators ask.

**Alternatives considered**: The three non-A options from the Clarifications §1 question — all rejected by the user in favor of strict literal match.

## R4 — CLI-flag ergonomics

**Decision**: Use `clap::ValueEnum` with `#[clap(rename_all = "kebab-case")]` — same pattern as the existing `EnrichSource` at `scan_cmd.rs:41-42` and `SbomSourceMode` at line 77-79. Concrete enum:

```rust
#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[clap(rename_all = "kebab-case")]
pub enum TierMode {
    /// Default: emit all resolved components regardless of tier.
    /// Preserves pre-232 behavior byte-identically per FR-002 / SC-003.
    #[default]
    All,
    /// Emit only components tagged `sbom_tier: "source"`.
    SourceOnly,
    /// Emit only components tagged `sbom_tier: "design"`.
    DesignOnly,
    /// Emit only components tagged `sbom_tier: "source"` or "binary".
    SourceAndBinary,
}
```

Field on `ScanArgs`:

```rust
#[arg(long, value_enum, default_value_t = TierMode::All)]
pub tier: TierMode,
```

Usage: `waybill sbom scan --tier=source-only ...`

**Rationale**: Matches every existing multi-mode flag on `waybill sbom scan`. Kebab-case for CLI-user ergonomics; snake-case enum variants for Rust idiom; `#[default]` variant satisfies FR-002's byte-parity guarantee.

**Alternatives considered**:
- *Boolean flag pair `--source-only` / `--design-only`.* Rejected: flag-explosion; can't express `source-and-binary` cleanly.
- *Comma-list `--tier=source,binary`.* Rejected: harder to document valid combinations; the three named modes cover 100% of the use cases in the spec Background.

## R5 — Integration-test scaffold reuse

**Decision**: Reuse the `common::bin` + `apply_fake_home_env` subprocess scaffold verbatim from `waybill-cli/tests/nuget_main_module_parity.rs` (m230). Same pattern:

```rust
mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn run_scan(path: &Path, tier: &str) -> serde_json::Value { ... }
```

New integration-test file `waybill-cli/tests/tier_filter_flag.rs` runs four subprocess-based tests (one per mode) against a fixture that produces at least one component per relevant tier.

**Rationale**: Zero new test infrastructure. Matches m230/m231/m216 convention verbatim.

**Alternatives considered**:
- *Test the filter helper as a pure unit test with a mocked component slice.* Included as a colocated unit test in `scan_cmd.rs`; but the integration test is still needed to prove the CLI flag is wired through end-to-end and the emitted SBOM's `components[]` reflects the filter.

## R6 — Test fixture — reuse or create?

**Decision**: Reuse the existing `waybill-cli/tests/fixtures/golden_inputs/nuget/packages_lock_present/` fixture from m230. It produces both source-tier NuGet components (the resolved packages) and a design-tier main-module (`pkg:generic/App@0.0.0`, per m230's version-ladder fallback). That mix satisfies SC-001 and SC-002 without needing a new fixture.

For SC-005 (source + binary), reuse `waybill-cli/tests/fixtures/golden_inputs/nuget/private_assets_all/` if it produces binary-tier components, OR add one small synthetic file to `packages_lock_present` that produces a binary-tier component. Determined at implementation time.

**Rationale**: Fixture reuse is fastest and lowest-risk. m230's fixture is already the go-to "mixed-tier NuGet fixture" and is stable.

**Alternatives considered**:
- *Create a new fixture explicitly for m232.* Rejected: reuse is preferred when the existing fixture covers the assertion space.
