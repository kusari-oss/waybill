# Data Model: survey-document entities

**Feature**: 228-release-flow-exploration
**Phase**: 1

Documentation-only feature. Entities here are the structural building blocks of the survey document, not runtime Rust structs.

## Entity 1: waybill-context block (§1 of the deliverable)

A concise section listing the constraints any recommendation must fit. Sourced from spec FR-004.

### Fields

- Current version — `v0.1.0-alpha.70` (verified via `grep "^version" Cargo.toml` at writing time).
- Current release model — single sequential alpha channel.
- Current CI shape — 3-lane CI < 5 min (per memory `project_ci_timing`).
- Known blockers — enumerated: `RELEASE_TAG_TOKEN` broken (memory `reference_release_process`); release-bump PRs 30+ min (memory `feedback_release_bump_prepr_slow`).
- Compliance target — CISA 2026 per constitution Principle V.
- Signing posture — Sigstore keyless via m222 `--sign` (available but not currently mandatory per channel).

### Rendering invariant

- Bullet list or short table; no prose exposition beyond a lead sentence.
- Every claim about waybill's current state cites the verifying source (memory ID, Cargo.toml field, or commit SHA).

## Entity 2: Peer-project entry (§2 — 6 rows)

Each of the 6 shortlisted projects gets one entry with the same fields.

### Fields per entry

- **Project name + link** — `sharkdp/bat` etc., linked to GitHub.
- **Project shape** — age (rough year of first release), approximate contributor count, primary language, downstream-consumer profile ("developers using it as a CLI tool", "CI-integrated SBOM tool", etc.).
- **Channel model** — named channels + one-line-each intended audience.
- **Cadence per channel** — verbatim, no normalization (per Q3 + FR-003).
- **Tag/version convention per channel** — e.g., "nightly: `nightly-YYYY-MM-DD`"; "stable: `vMAJOR.MINOR.PATCH`".
- **Signing/attestation posture** — does the project ship signed artifacts? Which mechanism? Per-channel or blanket?
- **"Why this fits their project" note** — one-sentence explanation why the peer chose this model given their project shape.
- **Source citations** — 3 mandatory URLs per Phase 0 research §C: release page, workflow YAML, docs.

### Rendering invariant

- One `###` heading per project.
- 3 mandatory-source links appear in a `**Sources**:` line at the top of each entry so a spot-check reader can jump directly to verification.
- Line budget per entry: 60–80 lines (per Phase 0 §F).

## Entity 3: Tradeoff-matrix cell (§3)

A 6-row × 6-column table (6 projects × 6 axes per Phase 0 §B).

### Fields per cell

- Value — categorical (LOW/MEDIUM/HIGH, WEAK/MODERATE/STRONG, etc.) or verbatim string (nightly cadence).
- Optional footnote pointer — for cells that need a per-project caveat (e.g., "trivy nightly = per-commit but skipped for docs-only merges — see footnote 3").

### Rendering invariant

- Standard pipe-delimited markdown table (GitHub-renderer compatible per FR-009).
- Cell values fit on one line; longer explanations go into per-project entry or footnotes.
- No cell left blank — use "N/A" or "unknown-source" explicitly.

## Entity 4: Recommendation block (§4 — the payload)

The single decisive recommendation per Q1 + FR-005. Contains 5 required components per Phase 0 §D.

### Fields

1. **Channel manifest** — named list of channels + one-line audience each.
2. **Per-channel cadence** — verbatim cadence per channel.
3. **Per-channel tag/version convention** — SemVer pre-release syntax preferred per Q2 compatibility invariant.
4. **Per-channel signing decision** — YES/NO with rationale per channel.
5. **Migration path** — either explicit "next release is `<x>`" or "no migration".

### Rendering invariant

- One `###` heading; all 5 fields as sub-bullets or a sub-table.
- Every decision links back to the tradeoff matrix's axis-column it optimizes for (e.g., "cadence = 1×/day scheduled — optimizes for axis-4 artifact-availability latency without blowing axis-1 maintainer time cost").

## Entity 5: Considered-and-rejected block (§5)

At least 2 alternatives with brief rationale per Q1 + FR-005.

### Fields per rejected alternative

- Alternative name (e.g., "rust-lang/rust nightly/beta/stable model").
- One-sentence "why this looked good".
- One-sentence "why not for waybill" — tied to a waybill-specific constraint (contributor pool size, CI cadence, compliance target, etc.).

### Rendering invariant

- Prose bullets; not a table (rejections need brief narrative context).
- Minimum 2 alternatives per FR-005.

## Entity 6: Future-distribution-compatibility block (§6)

Per FR-012 — which downstream surfaces the recommendation has been checked against + which conventions from those surfaces are honored.

### Fields

- Table with columns: Surface | Common convention | Recommendation-compatibility note.
- Rows for at minimum: crates.io, homebrew, cargo-binstall, apt/rpm/dnf.

### Rendering invariant

- If a surface has known issues with the recommended tag/version convention, note the issue explicitly (e.g., "homebrew formulas can't easily consume `v0.2.0-nightly.20260806` tags — recommendation compatible via a separate `formula-<version>` tag if formula publishing is added later").

## Entity 7: Risks + open questions block (§7)

Per FR-008 — things the recommendation deliberately doesn't answer.

### Fields per risk

- One-line risk statement.
- Deferred-to designation (usually "229-release-flow-implementation").

### Rendering invariant

- Bullet list, brief.
- ≥3 items (Phase 0 §G surfaced 4 already).

## Entity relationships

```text
Entity 1 (waybill context)
    ↓ constrains
Entity 2 (peer-project entries)  ←→  Entity 3 (tradeoff matrix cells)
    ↓ feeds
Entity 4 (recommendation)
    ↓ requires
Entity 5 (rejected alternatives)  +  Entity 6 (future-distribution compatibility)  +  Entity 7 (risks + open questions)
```

Every entity is either evidence, evaluation, or decision. No entity introduces new capabilities or CLI surface.
