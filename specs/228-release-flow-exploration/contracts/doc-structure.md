# Contract: `docs/design/2026-08-05-release-flow-survey.md` structure

**Feature**: 228-release-flow-exploration
**Phase**: 1

Fixes the deliverable's TOC + per-section content contract so writing-phase deviations produce visible violations rather than silent scope drift.

## Document identity

- **Filename**: `docs/design/2026-08-05-release-flow-survey.md`
- **Anchor slug for cross-linking**: `docs/design/2026-08-05-release-flow-survey.md#recommendation` (§4)
- **Front matter**: title (H1), dated, "Status: Draft" line, TOC.

## Section skeleton (7 sections + front matter)

```text
# waybill release-flow survey (2026-08-05)

**Status**: Draft — informs the follow-up implementation spec (229-release-flow-implementation).

<TOC — 1 line per section pointing to anchors>

## 1. waybill context
## 2. Peer-project survey
### 2.1 rust-lang/rust
### 2.2 aquasecurity/trivy
### 2.3 anchore/syft
### 2.4 sharkdp/bat
### 2.5 argoproj/argo-cd
### 2.6 nodejs/node
## 3. Tradeoff matrix
## 4. Recommendation
## 5. Considered and rejected
## 6. Future-distribution compatibility
## 7. Risks and open questions
```

Approximate line budget per section (per Phase 0 §F, total ≤ 800):

| Section | Est. lines | Content type |
|---|---|---|
| Front matter (H1 + TOC) | 30 | Prose + TOC |
| §1 waybill context | 50 | Bulleted state list |
| §2 Peer-project survey | 480 | 6 × ~80-line project entries |
| §3 Tradeoff matrix | 50 | 6×6 table + interpretive prose |
| §4 Recommendation | 80 | Structured decision block |
| §5 Considered and rejected | 40 | ≥2 alternatives × ~15 lines |
| §6 Future-distribution compatibility | 30 | Table + notes |
| §7 Risks and open questions | 30 | Bulleted list |
| Cross-links + closing | 10 | 1-line pointers |
| **Total** | **~800** | at SC-007 ceiling; writing must be disciplined |

## Per-section content contract

### §1 waybill context

Content MUST include (per FR-004):

- Current version — cite `Cargo.toml` line, verify at writing time.
- Current release model — "single sequential alpha channel".
- Current CI shape — 3-lane, < 5 min per memory `project_ci_timing`.
- Known blockers — `RELEASE_TAG_TOKEN` broken (memory `reference_release_process`); release-bump PRs 30+ min (memory `feedback_release_bump_prepr_slow`); goldens regen requires 3 env vars per memory `feedback_release_bump_regen_all_golden_tests`.
- Compliance target — CISA 2026 per constitution Principle V.
- Signing posture — Sigstore keyless via m222 `--sign`; not currently mandatory per channel.

Content MUST NOT:

- Introduce ANY design decision here — this is context, not decision. All decisions live in §4.
- Speculate about what future waybill state might be — the constraints are as-of writing time.

### §2 Peer-project survey (§2.1–§2.6)

Content per project entry MUST include (per FR-001 + FR-006 + Phase 0 §C):

- **Sources**: 3 cited URLs (release page + workflow YAML + docs) at the top of the entry.
- **Project shape**: age, contributor count, primary language, downstream-consumer profile.
- **Channel model**: named channels + one-line audience each.
- **Cadence per channel**: verbatim (per Q3).
- **Tag/version convention per channel**: SemVer, CalVer, or hybrid — state which.
- **Signing/attestation posture**: per-channel.
- **"Why this fits their project" one-sentence note**.

Content MUST NOT:

- Include claims without a corresponding source URL — SC-006 spot-check requires every claim be verifiable.
- Editorialize on whether the project's model is "good" or "bad" — the survey is descriptive, not evaluative. Evaluation happens in §3 (matrix) and §4 (recommendation).

### §3 Tradeoff matrix

Content MUST include (per FR-003):

- 6-row × 6-column table: rows = 6 shortlisted projects, columns = 6 axes from Phase 0 §B.
- One paragraph of interpretive prose beneath the table naming which axes matter most for waybill's context (referencing §1) — sets up §4 recommendation.
- Footnote references for any per-project caveats (e.g., "trivy nightly = per-commit but skipped for docs-only merges").

Content MUST NOT:

- Leave any cell blank — use "N/A" or "unknown-source" explicitly.
- Present the matrix without interpretive prose — a raw matrix is data, not analysis.

### §4 Recommendation

Content MUST include (per FR-005 + Q1 + Phase 0 §D):

- **Channel manifest**: named channels + intended audience.
- **Per-channel cadence**: verbatim.
- **Per-channel tag/version convention**: SemVer pre-release syntax preferred per Q2 compatibility invariant.
- **Per-channel signing decision**: per FR-007a, whether each channel gets Sigstore keyless signature. Include rationale per decision.
- **Migration path from `v0.1.0-alpha.70`**: either explicit next-tag OR "no migration".
- **Explicit ties back to §3 matrix**: every decision references the axis-column it optimizes for.

Content MUST NOT:

- Present 2+ alternative models as "the recommendation" — Q1 explicitly says SINGLE decisive recommendation. Alternatives go in §5.
- Leave any of the 5 required fields ambiguous — SC-004 requires the follow-up implementation spec to be write-able from this section alone.

### §5 Considered and rejected

Content MUST include (per FR-005 + Q1):

- ≥ 2 alternatives from the surveyed set.
- Per alternative: one-line "why it looked good" + one-line "why not for waybill" tied to a specific waybill constraint from §1.

Content MUST NOT:

- Include alternatives that weren't in the §2 survey — every rejected model must be one that was actually surveyed.

### §6 Future-distribution compatibility

Content MUST include (per FR-012):

- Table with columns: Surface | Common convention | Recommendation-compatibility note.
- Rows for at minimum: crates.io, homebrew, cargo-binstall, apt/rpm/dnf.
- If a known convention conflicts with the recommended tag/version syntax, an explicit "how the recommendation stays compatible" note.

Content MUST NOT:

- Introduce plans for expanding to those surfaces — those are OUT OF SCOPE per FR-011. This section is compatibility-check-only.

### §7 Risks and open questions

Content MUST include (per FR-008):

- ≥ 3 items (Phase 0 §G surfaced 4).
- Per item: one-line risk statement + deferred-to designation (usually "229-release-flow-implementation").

Content MUST NOT:

- Resolve any of the risks in-line — resolution belongs to the follow-up implementation spec.

## Success predicates (from spec, mapped to concrete verifiable checks)

- **SC-001** (peer-pattern recall): reading test — a first-time reader can name ≥4 distinct patterns from §2 after 5-minute skim.
- **SC-002** (5 projects across 3 categories): count §2's project entries + verify each entry names its FR-002 category.
- **SC-003** (matrix scores every project on 5+ axes): count §3's table dimensions.
- **SC-004** (recommendation actionable): pseudocode-implementation test — write ≥5 executable tasks for the follow-up implementation spec from §4 alone.
- **SC-005** (recommendation addresses 4 waybill concerns): grep §4 for the 4 FR-007 concerns.
- **SC-006** (5/5 random claims match sources): spot-check 5 randomly-chosen claims in §2 against cited source URLs.
- **SC-007** (≤ 800 lines total): `wc -l docs/design/2026-08-05-release-flow-survey.md`.
- **SC-008** (consumer channel choice): reading test against a written persona; can they pick a channel from §4?

## Placement + cross-linking

- File lives at `docs/design/2026-08-05-release-flow-survey.md` (create `docs/design/` if missing).
- If `docs/index.md` has a "Design docs" section, add 1-line pointer. If not, no cross-link edit.
- The follow-up implementation spec (229) will link back to this survey as motivating context.
