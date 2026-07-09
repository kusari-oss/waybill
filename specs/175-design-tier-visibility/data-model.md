# Phase 1 Data Model: Design-tier visibility (m175)

**Feature**: 175-design-tier-visibility
**Date**: 2026-07-09

Four entities. Zero new types on `mikebom-common` (per Constitution VI — this is a `mikebom-cli`-only milestone). Zero new wire fields (per FR-006 + Principle V KEEP-NATIVE-FIRST). One new advisory log line + one new env-var contract + one new docs-tag polarity.

## Entity 1 — Advisory-log predicate + emission block

**Location**: `mikebom-cli/src/cli/scan_cmd.rs`, at the emission-tail site immediately after the milestone-176 `monorepo shape detected: ` advisory block and before the final `SBOM written` `tracing::info!` line.

**Shape**:

```rust
// Milestone 175 — FR-002 advisory log. Emitted at INFO level exactly
// once when THREE predicates hold:
//   1. At least one component has sbom_tier = "design" (design-tier
//      count > 0).
//   2. The scan produced ≥1 component (empty scans stay quiet).
//   3. The MIKEBOM_NO_DESIGN_TIER_ADVISORY env var is unset (or set
//      to a value other than "1" / "true"). Env-var precedent:
//      milestone 110's MIKEBOM_NO_DEPRECATION_NOTICE=1.
// NOT gated on --offline: the remediation (generate a lockfile /
// install into a venv) works fully offline (FR-002 explicit).
// Stable grep substring: "design-tier components detected: " —
// dashboards grep-detect design-tier scans via this token.
{
    let design_tier_count = components
        .iter()
        .filter(|c| c.sbom_tier.as_deref() == Some("design"))
        .count();
    let suppress = std::env::var("MIKEBOM_NO_DESIGN_TIER_ADVISORY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if design_tier_count > 0 && !components.is_empty() && !suppress {
        tracing::info!(
            "design-tier components detected: {design_tier_count} components lack \
             resolved versions. Remediation: generate a lockfile (uv lock / poetry \
             lock / pip-compile / npm install / bundle lock / cargo generate-lockfile) \
             OR install into a venv and re-scan. See \
             docs/reference/reading-a-mikebom-sbom.md#design-tier-components for jq \
             recipes and per-ecosystem guidance."
        );
    }
}
```

**Contract**:
- **Idempotent**: repeated scans with the same inputs produce the same advisory (or its absence).
- **Deterministic**: no clock/random reads; predicate is pure over the scan's `Vec<ResolvedComponent>` + env-var state.
- **Zero side-effects on SBOM output**: only stderr is affected. Stdout / output-file bytes are unchanged (SC-006 gate).
- **Fires-at-most-once per scan**: the block runs once at emission-tail; N design-tier components produce ONE log line, not N.

## Entity 2 — Suppression env-var contract

**Name**: `MIKEBOM_NO_DESIGN_TIER_ADVISORY`

**Values (case-insensitive)**:

| Value | Effect |
|---|---|
| `1` | Suppress the FR-002 advisory unconditionally |
| `true` | Suppress the FR-002 advisory unconditionally |
| Any other value / unset | Advisory fires when FR-002 predicate holds |

**Precedent**: matches milestone-110's `MIKEBOM_NO_DEPRECATION_NOTICE=1` env-var convention. Empty string (`export MIKEBOM_NO_DESIGN_TIER_ADVISORY=`) is treated as unset (matches std env-var semantics).

**Documentation site**: `docs/reference/reading-a-mikebom-sbom.md` §3.4 new design-tier subsection includes a "Suppressing the advisory in CI" paragraph naming the env var.

## Entity 3 — Reading-guide subsection

**Location**: `docs/reference/reading-a-mikebom-sbom.md`, inserted under §3.4 (Transparency / completeness gaps) — semantically-adjacent to the existing `mikebom:graph-completeness` + `mikebom:go-transitive-coverage` subsections which also carry "here's how mikebom flags a scan-input degradation state" content.

**Subsection outline** (prose-level detail chosen at authoring time):

1. **What is design-tier?** — Definition (`sbom_tier = "design"` on components emitted from constraint-only manifests), the traceability ladder (design → source → analyzed → deployed → build), why empty version is Constitution Principle IX-honest behavior (accuracy over fabrication).
2. **How mikebom flags design-tier components in the SBOM** — native wire signals:
   - CDX 1.6: empty `component.version` + `evidence.identity[].confidence < 1.0` + technique `manifest-analysis` + `metadata.lifecycles[]` contains `{"phase": "design"}` when ≥1 design-tier component exists.
   - SPDX 2.3: empty `Package.versionInfo` on affected Packages + `mikebom:sbom-tier = "design"` per-Package annotation.
   - SPDX 3.0.1: empty `software_Package.packageVersion` + `mikebom:sbom-tier` typed Annotation.
3. **The advisory log** — quotes the exact stable substring (`"design-tier components detected: "`), notes it's INFO-level on stderr, mentions the `MIKEBOM_NO_DESIGN_TIER_ADVISORY=1` env-var suppression for CI opt-out.
4. **Operator remediation** — per-ecosystem table:

    | Ecosystem | Trigger manifest | Remediation |
    |---|---|---|
    | pip | `requirements*.txt` alone | `uv lock` OR `poetry lock` OR `pip-compile requirements.in` — produces a lockfile; components lift to source-tier |
    | pip (deployed) | `requirements*.txt` alone | `python -m venv .venv && .venv/bin/pip install -r requirements.txt`, then `mikebom sbom scan --path <project>` sees the venv → deployed-tier |
    | npm | root `package.json` no lockfile | `npm install` writes `package-lock.json` → source-tier |
    | Cargo | `Cargo.toml` no `Cargo.lock` | `cargo generate-lockfile` → source-tier |
    | Ruby | `Gemfile` no `Gemfile.lock` | `bundle lock` OR `bundle install --deployment` → source-tier |
    | (More ecosystems as authoring bandwidth allows per FR-008) | | |

5. **jq recipes** — the four canonical queries:
   - **Count**: `jq '[.components[]?.version | select(. == "")] | length' scan.cdx.json` — returns integer count of design-tier components (SC-002 verifiable).
   - **List PURLs**: `jq -r '.components[] | select(.version == "") | .purl' scan.cdx.json` — enumerate design-tier components.
   - **Doc-scope phase check**: `jq '.metadata.lifecycles[]? | select(.phase == "design")' scan.cdx.json` — verify CDX's native aggregate.
   - **Mixed-tier breakdown**: `jq '[.components[]?.properties[]? | select(.name == "mikebom:sbom-tier") | .value] | group_by(.) | map({tier: .[0], count: length})' scan.cdx.json` — tier histogram (leverages existing `mikebom:sbom-tier` annotation).

6. **Threshold-checking in CI**:

    ```bash
    DESIGN=$(jq '[.components[]?.version | select(. == "")] | length' scan.cdx.json)
    if [ "$DESIGN" -gt 0 ]; then
      echo "::warning::mikebom found $DESIGN design-tier components; consider generating a lockfile"
    fi
    ```

    Not a hard gate — informational — matches the m175 "advisory not defect" framing.

7. **Suppressing the advisory in CI** — one-paragraph explanation of `MIKEBOM_NO_DESIGN_TIER_ADVISORY=1` for scans of intentionally-constraint-only projects (linter fixtures, template repos, etc.).

**Word count**: ~800-1000 words end-to-end.

## Entity 4 — sbom-format-mapping.md KEEP-NATIVE-FIRST row

**Location**: `docs/reference/sbom-format-mapping.md`, Section C (existing table). Insert row alphabetically or thematically — the file's existing convention is per-annotation ordering; the design-tier row doesn't fit that convention since it deliberately does NOT introduce an annotation. Recommendation: insert at the end of Section C with a preamble sentence explaining that this row documents a KEEP-NATIVE-FIRST audit rather than a `mikebom:*` construct.

**Row shape**:

```markdown
| — | design-tier component signal | Empty `component.version` (Constitution Principle IX honesty — accuracy over fabrication); `evidence.identity[].confidence < 1.0`; technique `manifest-analysis`; doc-scope `metadata.lifecycles[]` contains `{"phase": "design"}` when ≥1 design-tier component exists (populated by `generate::lifecycle_phases::tier_to_phase`). | Empty `Package.versionInfo` on affected Packages; `mikebom:sbom-tier = "design"` per-Package `MikebomAnnotationCommentV1` envelope. | Empty `software_Package.packageVersion` on affected elements; `mikebom:sbom-tier` typed Annotation. | **KEEP-NATIVE-FIRST** (new tag polarity introduced by milestone 175). Standards-native precedence per Constitution Principle V. Rejected alternative: a new `mikebom:design-tier-count` doc-scope annotation. Rationale: (a) the per-component signal is already native across all 3 formats via empty-version + confidence + evidence-technique; (b) the doc-scope aggregate is already native via CDX `metadata.lifecycles[design]` (SPDX 2.3 has no analogous field — the parity gap is bridged by the existing per-component `mikebom:sbom-tier` annotation, not by a new doc-scope invention); (c) an exact count is derivable in one jq call: `[.components[]?.version | select(. == "")] | length`. Milestone 175 codifies this decision as prior-art for future contributors doing Principle V audits. |
```

**SC-007 gate**: `grep -n KEEP-NATIVE-FIRST docs/reference/sbom-format-mapping.md` returns exactly one match. The row is the first + only prior-art for the KEEP-NATIVE-FIRST polarity.

## Entity 5 — component-tiers.md cross-reference paragraph

**Location**: `docs/reference/component-tiers.md`, appended to or inserted near the existing "Design tier" bullet (this file already discusses the sbom_tier ladder).

**Shape**:

```markdown
For operator-facing guidance on design-tier components — how to recognize
them in an emitted SBOM, per-ecosystem remediation actions, and jq recipes
for count / list / threshold-check — see
[reading-a-mikebom-sbom.md §3.4 → Design-tier components](reading-a-mikebom-sbom.md).
mikebom emits an INFO-level advisory log at scan time when the scan produces
≥1 design-tier component; the advisory can be suppressed in CI via
`MIKEBOM_NO_DESIGN_TIER_ADVISORY=1` (milestone 175).
```

**Word count**: ~60 words (concise cross-reference).

## Cross-entity invariants (post-175)

1. **Zero SBOM byte-changes**: the emitted CDX/SPDX 2.3/SPDX 3 bytes for any scan are byte-identical pre-175 vs post-175 (SC-006 gate). Only stderr may differ (advisory line addition).
2. **KEEP-NATIVE-FIRST is discoverable**: `grep KEEP-NATIVE-FIRST docs/reference/sbom-format-mapping.md` returns the m175 row.
3. **Advisory-log suppression is orthogonal to correctness**: setting `MIKEBOM_NO_DESIGN_TIER_ADVISORY=1` does NOT change the emitted SBOM; it silences the advisory only.
4. **No new `mikebom:*` annotation**: per FR-006, `grep -rc 'mikebom:design-tier' mikebom-cli/src/ docs/reference/` returns zero (the string appears only in the m175 spec artifacts).

## State transitions

None. Pure emission-tail diagnostic + docs additions. No lifecycle events.
