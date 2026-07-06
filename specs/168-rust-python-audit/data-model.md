# Data Model: milestone 168 — Rust + Python monorepos audit

**Date**: 2026-07-06
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

Phase 1 data model. Milestone 168 has no Rust types — this is a doc-only audit. The "data model" is the schema of the audit report itself + the intermediate measurement artifacts.

## E1 — Report file

**Path**: `docs/audits/2026-07-06-tauri-airflow.md`

**Structure** (mirrors m165 verbatim + FR-011 + FR-012 additions per research §R8):

```text
# Empirical audit of mikebom against Tauri + Apache Airflow (Round 4)

## Header
- Audit date
- mikebom commit SHA (post-m167 build)
- Trivy version, Syft version, spdx3-validate version
- Tauri repo URL + pinned commit SHA
- Airflow repo URL + pinned commit SHA
- Host environment (OS + arch)

## Per-Target Section: Tauri
### Setup + scan invocation
### Per-tool metrics table (mikebom / Trivy / Syft × component count, edge count, BFS%, wall-clock, ecosystem breakdown)
### mikebom failure-mode classification (buckets with count + example PURL + disposition)
### Tool Comparison Delta (mikebom-advantage, trivy-advantage, syft-advantage lists)
### SPDX validation results (CDX schema, SPDX 2.3 jsonschema, SPDX 3 spdx3-validate)
### Cross-ecosystem observations (Cargo ↔ npm edges; m111 alias-binding hits; m116 produces-binaries hits)

## Per-Target Section: Airflow
### Setup + scan invocation
### Per-tool metrics table
### mikebom failure-mode classification
### Tool Comparison Delta
### SPDX validation results (with special attention to LicenseRef-* per FR-006)
### Python-source-attribution table (pip requirements vs pyproject/Poetry vs uv.lock breakdown)

## Recommended Follow-On Milestones
### Top-3 ranked candidates
### m167 Vocabulary Applicability sub-section (FR-012 + SC-012)  ← NEW vs m165

## Cross-Round Trend Analysis (FR-011)  ← NEW vs m165
### m158 (Round 1) baseline reference (with freshness caveats per Q3)
### m165 (Round 3) baseline reference (with freshness caveats per Q3)
### m168 (this round) new observations
### Recurring class table (m158 seen ✓/✗, m165 seen ✓/✗, m168 seen ✓/✗, priority multiplier)

## Backlog Observations
### Smaller findings not making top-3

## Executive Summary  ← at end per m165 pattern
### Headline numbers
### Actionable-bug-class outcome (SC-011)
### m167 vocab outcome (SC-012)
### Cross-round pattern outcome (FR-011)

## Reproduction Appendix
### Exact commands to reproduce every number
### jq recipes for tool comparison delta
### Known install-friction notes (m165 §Trivy install carried forward)
```

**Validation rules**:
- File must exist at exactly this path (SC-001).
- All section headers present (SC-002, SC-003, SC-004, SC-005, SC-006, SC-011).
- Every metric anchored to a specific commit SHA in the header (SC-010).
- Reproduction Appendix commands must be self-contained (SC-009).

## E2 — Per-tool measurement record (in-report)

Each per-target section has a table:

| Tool | Total components | Total edges | BFS reachable % | Wall-clock (s) | pkg:cargo/ | pkg:npm/ | pkg:pypi/ | other |
|---|---|---|---|---|---|---|---|---|
| mikebom | ... | ... | ... | ... | ... | ... | ... | ... |
| Trivy | ... | ... | ... | ... | ... | ... | ... | ... |
| Syft | ... | ... | ... | ... | ... | ... | ... | ... |

**Validation rules**:
- Every cell is a numeric value or "N/A" with explicit reason (e.g., "Trivy does not emit BFS reachability").
- Ecosystem cells sum to Total components (± non-package-tier file-tier entries — flagged in a footnote).

## E3 — Root-cause bucket record (in-report)

Each mikebom orphan / empty-version / phantom edge gets grouped into a named bucket:

```text
Bucket: <name>
    Count: <N>
    Example PURL: pkg:<ecosystem>/<...>
    Source path (if applicable): <relative path in target repo>
    Disposition: fix-in-follow-on-milestone|accept-as-is-with-rationale
    m167 vocab match: <one of the 5 codes, or "unmapped">
```

**Validation rules**:
- Every bucket has all 5 fields populated (SC-003).
- Dispositions are exactly one of the two literals.
- m167 vocab match is one of {stale-go-sum-entry, dead-lockfile-entry, hoisted-unused, unresolved-indirect-require, flat-attached-fallback, unmapped}. If ≥1 orphan carries "unmapped", the report proposes a new code in FR-012's Vocabulary Applicability sub-section.

## E4 — Tool Comparison Delta record (in-report)

For each (target, ecosystem) pair:

```text
Target: <tauri|airflow>
Ecosystem: <pkg:cargo/|pkg:npm/|pkg:pypi/>
mikebom_advantage: [<PURL>, ...]  # components mikebom finds, others miss
trivy_advantage:   [<PURL>, ...]  # components Trivy finds, mikebom + Syft miss
syft_advantage:    [<PURL>, ...]  # components Syft finds, mikebom + Trivy miss
```

**Validation rules**:
- Per SC-004, jq recipes producing these lists are included in the Reproduction Appendix.
- Every list can be empty (e.g., mikebom + Syft may agree completely on Cargo side).

## E5 — Cross-Round Trend record (in-report, FR-011)

For each surfaced bug class:

```text
Class: <name>
    m158 seen: <yes|no>
    m165 seen: <yes|no>
    m168 seen: yes  (definitionally — otherwise the class wouldn't be in the table)
    Priority multiplier: <1|2|3>  # 1× for one-off, 2× for two-round pattern, 3× for cross-round persistence
    Recommendation: <fix-in-milestone-N|accept-with-rationale>
```

**Validation rules**:
- Multiplier drives ordering into the top-3 (a 3× multiplier outranks a 1× multiplier of similar per-round impact).
- Every recurring class has explicit m158 + m165 back-references (report line numbers or section anchors).

## E6 — m167 Vocabulary Applicability record (in-report, FR-012 + SC-012)

For each ecosystem measured in m168:

```text
Ecosystem: <pkg:cargo/|pkg:pypi/>
Orphan-reason classes surfaced: [<class>, ...]
m167 vocab coverage:
    Classes mapped: [<class → code>, ...]
    Classes UNmapped: [<class>, ...]  # ← if non-empty, drives an FR-012 vocab-extension candidate
Proposed new codes (if any):
    - name: <slug>
      rationale: <one-line>
      example PURL: pkg:<ecosystem>/...
      candidate milestone: milestone 169 (or later)
```

**Validation rules**:
- Cargo + Python each get their own record (SC-012 says "explicitly documents whether the m167 C45 orphan-reason vocabulary is sufficient for the Rust + Python classes surfaced, OR proposes vocabulary extensions").
- npm-side observations remain fully covered by m167's existing codes (m167 FR-001 scope was Go + npm) — the record notes this and moves on.

## E7 — Intermediate artifact schema

Under `specs/168-rust-python-audit/artifacts/`:

```text
tauri/
├── mikebom.cdx.json     # raw mikebom CycloneDX output
├── trivy.cdx.json       # raw Trivy CycloneDX output
├── syft.cdx.json        # raw Syft CycloneDX output
├── mikebom.spdx23.json  # raw mikebom SPDX 2.3 output
├── mikebom.spdx3.json   # raw mikebom SPDX 3 output
└── analysis.json        # analyze.py parsed metrics — see next paragraph
airflow/
└── (same structure)
```

`analysis.json` schema (produced by m165's `analyze.py`, target-agnostic):

```json
{
  "target": "tauri",
  "target_sha": "<commit SHA>",
  "tools": {
    "mikebom": {
      "component_count": 1234,
      "edge_count": 5678,
      "bfs_reachable_pct": 98.5,
      "wall_clock_seconds": 12.3,
      "per_ecosystem": {"pkg:cargo/": 234, "pkg:npm/": 567, ...},
      "orphans_by_class": {"path-dep-no-version": 3, "unresolved-indirect-require": 1, ...},
      "spdx3_validate": "pass|fail",
      "spdx2_jsonschema": "pass|fail"
    },
    "trivy": { ... },
    "syft": { ... }
  },
  "comparison_delta": {"mikebom_advantage_purls": [...], "trivy_advantage_purls": [...], "syft_advantage_purls": [...]}
}
```

**Validation rules**:
- `analysis.json` is regenerable — not versioned. Report body references its content but does NOT depend on it existing for readers.
- Report body includes formatted tables + narrative; `analysis.json` is the machine-readable source-of-truth for reproducibility (SC-009).

## Wire types

**None.** This is a documentation milestone. mikebom emits its normal CDX/SPDX outputs; the audit consumes them via `jq` + `analyze.py`. No new API contracts.

## Relationships

```text
run-audit.sh
    ↓ clones
Tauri repo (pinned SHA) → mikebom scan → mikebom.cdx.json + mikebom.spdx23.json + mikebom.spdx3.json
                        → Trivy scan   → trivy.cdx.json
                        → Syft scan    → syft.cdx.json
Airflow repo (pinned SHA) → (same)
    ↓
analyze.py (from m165) reads all raw SBOMs → analysis.json (E7)
    ↓
Report author (human + AI-assisted) synthesizes analysis.json + qualitative observations →
    docs/audits/2026-07-06-tauri-airflow.md (E1 through E6)
```

## State transitions

**None.** One-shot audit; no evolving state. Report is a snapshot; intermediate artifacts are regenerable.

## Data volume assumptions

- Tauri source: ~50 MB clone → ~1000-1500 components emitted by mikebom (Rust + npm combined) → ~10 MB CDX output.
- Airflow source: ~200 MB clone → ~1200-1500 components emitted by mikebom (Python) → ~15 MB CDX output.
- Report itself: 1000-1500 lines Markdown, ~50-80 KB text.
- Total intermediate artifacts: ~40-60 MB (gitignored per plan.md).
