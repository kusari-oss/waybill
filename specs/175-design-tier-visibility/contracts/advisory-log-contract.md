# Contract: Advisory-log wording + suppression + KEEP-NATIVE-FIRST tag

**Feature**: 175-design-tier-visibility
**Date**: 2026-07-09

Authoritative reference for the four external-facing surfaces this milestone introduces. Deviations in emitted output are grounds for review comment.

## Surface 1 — Advisory log line

**Location**: stderr (INFO-level `tracing::info!` at `mikebom-cli/src/cli/scan_cmd.rs` emission-tail).

**Predicate**: `design_tier_count > 0 && !components.is_empty() && !suppress`, where:
- `design_tier_count = components.iter().filter(|c| c.sbom_tier.as_deref() == Some("design")).count()`
- `suppress = std::env::var("MIKEBOM_NO_DESIGN_TIER_ADVISORY").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false)`

**Stable grep substring**: `"design-tier components detected: "` (INCLUDES the trailing colon-space for token-boundary clarity). CI dashboards `grep -F` this token.

**Full form** (subject to authoring-time refinement — the substring above is the load-bearing contract):

```
design-tier components detected: 48 components lack resolved versions. Remediation: generate a lockfile (uv lock / poetry lock / pip-compile / npm install / bundle lock / cargo generate-lockfile) OR install into a venv and re-scan. See docs/reference/reading-a-mikebom-sbom.md#design-tier-components for jq recipes and per-ecosystem guidance.
```

**Level**: `INFO` — NOT `WARN`. The advisory is informational, not error-shaped. The SBOM is CORRECT; the operator's scan-input state is what could improve.

**Frequency**: at most one line per scan invocation. The predicate is checked once at emission-tail after the m176 monorepo-advisory block.

**Offline behavior**: fires under `--offline` (unlike the m173 warming advisory which is offline-suppressed). Remediation is fully offline-capable (generate a lockfile, install into venv — no network required for the *concept*; the underlying commands may or may not need network depending on cache state, but that's an operator concern, not mikebom's).

## Surface 2 — Env-var suppression

**Name**: `MIKEBOM_NO_DESIGN_TIER_ADVISORY`

**Truthy values (case-insensitive)**: `1`, `true`

**Falsy / unset / any other value**: advisory fires per Surface 1 predicate.

**Precedent**: matches milestone 110's `MIKEBOM_NO_DEPRECATION_NOTICE=1`.

**Non-goals**:
- Does NOT suppress other advisories (m173 Go cache-warming, m176 monorepo shape).
- Does NOT modify the emitted SBOM. Purely diagnostic-suppression.

## Surface 3 — KEEP-NATIVE-FIRST tag polarity

**Introduction site**: `docs/reference/sbom-format-mapping.md`, one new row in Section C. See data-model.md §Entity 4 for the exact row shape.

**Semantic contract**:
- `KEEP-NO-NATIVE` (existing) — a `mikebom:*` construct was introduced because native constructs across CDX/SPDX 2.3/SPDX 3 lack the semantic. Rejected alternatives are enumerated in the row.
- `KEEP-NATIVE-FIRST` (new — m175) — a proposed `mikebom:*` construct was rejected because native constructs already exist. Rejected `mikebom:*` invention is enumerated in the row.

Both tag polarities are additive; existing rows are untouched.

**SC-007 gate**: `grep -n KEEP-NATIVE-FIRST docs/reference/sbom-format-mapping.md` returns exactly one match (the m175 row).

## Surface 4 — Reading-guide subsection anchor

**Location**: `docs/reference/reading-a-mikebom-sbom.md` §3.4 (Transparency / completeness gaps), new subsection.

**Anchor slug**: `design-tier-components` (Markdown-standard slug of the section heading, matches the advisory-log's `#design-tier-components` fragment reference).

**Contents**: see data-model.md §Entity 3 outline (7 subsections: definition + native wire signals + advisory description + per-ecosystem remediation table + jq recipes + threshold-check CI recipe + advisory-suppression paragraph).

**SC-001 gate**: an operator new to mikebom can identify a design-tier component + count them + name one remediation action within 5 minutes of reading this subsection alone.

## Consumer jq recipes

### Recipe 1 — Count design-tier components (SC-002 verifiable)

```jq
[.components[]?.version | select(. == "")] | length
```

Returns integer count. Equivalent to the `design_tier_count` variable in the advisory-log predicate.

### Recipe 2 — List design-tier PURLs

```jq
.components[] | select(.version == "") | .purl
```

Returns PURL strings — one per design-tier component.

### Recipe 3 — Verify CDX doc-scope native aggregate

```jq
[.metadata.lifecycles[]? | select(.phase == "design") | .phase] | length
```

Returns `1` when ≥1 design-tier component exists (CDX aggregates the phase presence, not a count); `0` when none.

### Recipe 4 — Cross-check FR-002 predicate holds pre-advisory

```bash
COUNT=$(jq '[.components[]?.version | select(. == "")] | length' scan.cdx.json)
COMPONENTS=$(jq '.components | length' scan.cdx.json)
if [ "$COUNT" -gt 0 ] && [ "$COMPONENTS" -gt 0 ]; then
  echo "advisory should have fired"
else
  echo "advisory correctly suppressed"
fi
```

## Non-goals

- No new CLI flag (per FR-005 R2 decision — env var only).
- No structural SBOM changes (`components[]`, `dependencies[]`, `metadata.component`, `metadata.lifecycles[]` all unchanged — FR-007 + FR-010).
- No new `mikebom:*` annotation (per FR-006 + Principle V KEEP-NATIVE-FIRST audit).
- No golden regeneration (per SC-006 + R5).
- No touch to the existing `mikebom:sbom-tier` per-component annotation (per FR-007 — the wire contract is already correct).
