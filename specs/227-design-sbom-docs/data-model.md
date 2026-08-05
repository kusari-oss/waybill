# Data Model: doc-structure entities

**Feature**: 227-design-sbom-docs
**Phase**: 1

Documentation-only feature. "Entities" here are the structural building blocks of the new section — not runtime Rust data structures. Each entity has a rendering invariant that the writing phase must satisfy.

## Entity 1: Tier concept box (§1 of the new section)

A short conceptual explanation of the three tiers (source / design / binary) with a per-tier "how to detect this in an emitted SBOM" row.

### Fields

- **Tier name** (source / design / binary) — enum.
- **Trigger condition** — one-sentence "waybill emits this tier when …" summary.
- **PURL shape** — canonical example (`pkg:cargo/serde@1.0.197` for source, `pkg:cargo/serde` for design when versionless, etc.).
- **`sbom_tier` field value** — the literal string in the emitted SBOM (`"source"`, `"design"`, `"binary"`).
- **Detection recipe pointer** — link to the corresponding `jq` recipe (Entity 4).

### Rendering invariant

- Rendered as a Markdown table with 5 columns (Tier | Trigger | PURL shape | `sbom_tier` value | Recipe link).
- Fits on one screen without horizontal scroll on the GitHub default viewport.

## Entity 2: Per-ecosystem design-tier fallback matrix (§2 of the new section)

The core per-ecosystem inventory table.

### Fields per row

- **Ecosystem** — string; links to the existing per-ecosystem section anchor further down in `ecosystems.md`.
- **Design-tier fallback?** — yes / no / opt-in-flag / conditional (with flag name).
- **Trigger condition** — what causes design-tier vs source-tier for this ecosystem.
- **PURL shape** — `pkg:<type>/<name>` (versionless) OR `pkg:<type>/<name>@<version>` (version present but design-classified — e.g., pants Go's synthetic `pkg:generic/go@<version>`).
- **`waybill:unresolved-reason` emitted?** — yes / no (research §B says only nuget today).

### Rendering invariant

- Rendered as a Markdown table with 5 columns.
- Rows sorted alphabetically by ecosystem name to match the existing per-ecosystem section order.
- Every row's ecosystem cell contains a working Markdown link to the corresponding per-ecosystem section anchor (verified with a link-check pass before commit).

### Row source

Each row's factual content comes from research.md §A. Any row that lacks a direct source-code citation in §A gets flagged during writing and either (a) grepped-and-verified on the spot OR (b) marked as "unverified" and moved to the follow-up-issue seed list.

## Entity 3: Design-tier detection recipes (§3 of the new section)

`jq` recipes for CycloneDX and SPDX output formats.

### Fields per recipe

- **Recipe title** — human-readable, e.g., "Filter to source-tier components only (CycloneDX)".
- **Command block** — the actual `jq` invocation, wrapped in a fenced code block with `bash` highlighter.
- **Expected-output shape** — one-line description of what the recipe returns (e.g., "one JSON object per source-tier component").
- **Format** — CycloneDX or SPDX 2.3 or SPDX 3.

### Recipe inventory (6 recipes minimum per FR-003)

1. Filter to source-tier only (CycloneDX)
2. Filter to design-tier only (CycloneDX)
3. Filter to source-tier only (SPDX 2.3)
4. Filter to design-tier only (SPDX 2.3)
5. Extract `waybill:unresolved-reason` per design-tier component (CycloneDX; NuGet-only today per research §B)
6. Count components per tier (CycloneDX + SPDX)

### Rendering invariant

- Every recipe wrapped in a fenced code block with `bash` highlighter.
- Every recipe verified against a real waybill SBOM per research §C before commit.
- Recipe verification evidence recorded as a comment inside the doc or in a companion note (not required to be a running script per research §C's rationale).

## Entity 4: `waybill:unresolved-reason` annotation subsection

A focused subsection documenting the one existing annotation (NuGet-only today).

### Fields

- **Where it appears** — property/annotation location in each output format.
- **Value shape** — the specific string(s) NuGet emits today.
- **Consumer interpretation** — how a downstream tool should surface this to a human reviewer.
- **Adoption gap note** — explicit statement that other readers do NOT emit this annotation today; consumers should treat its ABSENCE as "no reason provided" rather than "component was resolved".

### Rendering invariant

- Includes a small code sample showing the annotation shape in each output format.
- Explicit "cross-reader consistency gap" callout — linked to the follow-up issue that gets filed after this docs milestone.

## Entity 5: "When design-tier is enough vs when it isn't" guidance (§4)

Two side-by-side lists of use cases.

### Fields

- **Design-tier appropriate for**: bulleted list of use cases where declared-inventory is sufficient (compliance attribution, contract audits, declared-inventory manifests, ISM/CMMC evidence, procurement checklists).
- **Design-tier insufficient for**: bulleted list where transitive graph is needed (exact-version CVE scanning, transitive license-conflict analysis, dependency-confusion detection, SLSA level-3+ provenance claims).
- **Bridging techniques**: one paragraph per technique listing how to upgrade design → source for a given ecosystem (generate lockfile / use `--supplement-cdx` / opt-in resolver flags like `--warm-go-cache`).

### Rendering invariant

- The "insufficient for" list explicitly names the silent-miss failure mode called out in spec edge-cases: "running exact-version CVE matches on a versionless PURL produces false-negative silent misses, not false positives — the scan appears to find nothing where the real answer is 'we don't know'".
- The bridging-techniques paragraph cross-links to the per-ecosystem sections' remediation subsections.

## Entity 6: Graph-completeness interaction subsection (§5)

Connects the design-tier concept to the milestone-158 graph-completeness annotation at document scope.

### Fields

- **How design-tier affects graph-completeness classification** — one paragraph explaining that design-tier components inherently mean "partial coverage" from m158's perspective.
- **How consumers distinguish "partial due to design-tier" from "partial due to unreachable orphans"** — the two orphan classes both exist; the doc points consumers to the specific reason-code strings emitted by m158.
- **Recipe** — a jq snippet showing how to extract the m158 completeness annotation and its reason-code list.

### Rendering invariant

- Cross-references milestone-158's existing docs at `docs/reference/graph-completeness.md` (if such a doc exists — verified during writing; else linked to the code path).
- Doesn't duplicate m158's docs; only shows the design-tier interaction.

## Entity 7: Contributor guidance subsection (§6)

For contributors implementing new ecosystem readers per US3.

### Fields

- **The 4-field convention** — versionless PURL + `sbom_tier: "design"` + `waybill:unresolved-reason` annotation + trigger condition.
- **Precedent readers to copy from** — 2–3 concrete file paths (e.g., `gem.rs::build_gem_purl`, `nuget/mod.rs::read_one_project` post-#653).
- **Anti-patterns** — explicit "don't invent your own `@unresolved` sentinel; use the design-tier + versionless convention" callout referencing the #653 pre-fix behavior.

### Rendering invariant

- Every file-path citation is a repo-relative absolute path (e.g., `waybill-cli/src/scan_fs/package_db/gem.rs`) with a line-number range where useful.
- Every precedent-file citation is verified against source at writing time.

## Entity 8: Per-ecosystem cross-reference blocks (~30 edits scattered through existing per-ecosystem sections)

Each existing per-ecosystem section (~30 of them) gets a small addition — either an inline "Design-tier fallback" note (if the section doesn't already discuss it) or a link back to the new tier section (if it does).

### Fields per cross-reference

- **Style** — inline paragraph OR blockquote OR sub-heading, chosen per-ecosystem for visual consistency with the section's existing structure.
- **Link target** — anchor to the new tier section (`#design-tier-sbom-fallback` or similar; concrete anchor decided during writing).
- **Content** — 1–3 sentences: what this ecosystem's design-tier trigger is, what PURL shape it produces, link to the general tier section for full context.

### Rendering invariant

- Every one of the 30 per-ecosystem sections receives either the addition OR gets its existing tier discussion cross-linked. Nothing is silently skipped.
- SC-003 verification: manual count post-writing confirms 100% coverage. A grep for the new-section anchor across `ecosystems.md` returns a hit-count equal to the per-ecosystem section count (± readers that emit no design-tier at all, e.g., OS package readers, which get an explicit "no design-tier fallback for this ecosystem" note instead).

## Entity relationships

```text
Entity 1 (Tier concept)
    ↓ frames
Entity 2 (Per-ecosystem matrix)  ←→  Entity 8 (Per-section cross-refs)
    ↓ populates
Entity 3 (Recipes)  ←→  Entity 4 (unresolved-reason)
    ↓ enables
Entity 5 (Guidance)  ←→  Entity 6 (Graph completeness)
    ↓ referenced by
Entity 7 (Contributor guidance)
```

Every entity is either primary-content or cross-linkage. No entity introduces new signals; everything documents existing waybill emission behavior verifiable per research §A / §B.
