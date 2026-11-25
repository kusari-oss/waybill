# Feature Specification: graph completeness counts a partial closure read

**Feature Branch**: `1656-closure-read-completeness`
**Created**: 2026-11-25
**Status**: Draft
**Input**: #1720. Under `--nix-closure`, C104 `waybill:graph-completeness` can read `complete` while the closure reader left dependency declarations unconsulted or members' contents unread. Refs #1720, #1348, #1696.

## Background (measured)

A closure read can be partial in two ways. Each already has its own disclosure row, and each already makes the formats' native fields withhold their root claim, through `ScanArtifacts::closure_read_partial()` (milestones 1278 and 1378, spec 1654):

- **Unconsulted declarations** (`waybill:nix-closure-read`): store-path fields that derivations declare and classification does not read, and so dependency edges that may exist and were not looked at.
- **Unread contents** (C213 `waybill:nix-closure-contents`): closure members whose outputs were not read for embedded build information (absent, partial or failed). That can mean modules linked into a binary that are never found.

C105 `waybill:graph-completeness-reason` has no code for either, so C104 ignores both.

**On the closure corpus goldens at `main` 1f85936f** (`measurements/read_gaps.py` → `read-gaps-before.txt`):

| target | C104 | unconsulted declarations (derivations) | members with contents unread / members | root `compositions` dependency claim |
|---|---|---|---|---|
| moat | partial | 211 (16) | 1,087 / 1,590 | none |
| openai | partial | 198 (16) | 471 / 1,330 | none |
| shellcheck | **complete** | 211 (16) | 1,284 / 2,045 | none |
| slack-web | partial | 211 (16) | 416 / 1,193 | none |
| slacklinker | partial | 228 (17) | 1,153 / 2,351 | none |

- **Unconsulted fields:** always `defaultNativeBuildInputs` and `extraNativeBuildInputs`. Carrier disagreements, the other half of the read gap, are 0 on all five.
- **The masking:** no C105 mentions either gap. shellcheck reads `complete` because spec 1655 (#1710) removed its only other reason. The other four are `partial` for other reasons, which hide the gap.
- **The stub closure fixture** (`tests/fixtures/nix_closure_boot_versions`) has no `nix-closure-read` row, but 4 of its 4 members have unread contents.

**This contradicts spec 1278 FR-015.** A waybill property asserts `complete` beside native fields that withhold it. That is worse than either alone, because a consumer that reads only one of them is misled.

## Clarifications

### Session 2026-11-25

- Q: One reason code or two? → A: **Two**, `closure-declarations-unconsulted` and `closure-contents-unread`.
  - Each mirrors a disclosure row that already exists.
  - They have different remedies: consulting more fields, or reading a store the scan could not reach.
  - They occur independently: the stub has the second without the first.

  One code would force a reader back to the disclosure rows to learn which gap fired, which is the indirection C105 exists to avoid.
- Q: (plan) Do all four completeness callers need the input? → A: No, the three format emitters. The orphan-reason pass reads only the reachable set, and passes no scope-drop input for the same reason (FR-005 corrected; research R2).
- Q: Detail grammar? → A: Counts only, never paths or field names, as `manifest-unread` does, because the value is pinned by goldens compared across hosts. The field names stay in `waybill:nix-closure-read`. The two clauses are:
  - `closure-declarations-unconsulted: <N> declaration(s) in <M> derivation(s)`, plus `, <K> carrier disagreement(s)` only when K > 0;
  - `closure-contents-unread: <U> of <T> member(s)`, where U = absent + partial + failed and T = members.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - C104 never claims more than the native fields (Priority: P1)

A consumer reads C104 to decide whether a closure document's dependency graph can be trusted as complete. Today, on shellcheck, it says `complete` while the document's own disclosures say 211 declarations went unread.

**Why this priority**: this is the defect. A false `complete` is worse than a `partial`.

**Independent Test**: scan a closure fixture with an unconsulted-declaration gap. C104 reads `partial` and C105 names `closure-declarations-unconsulted` with its counts, in all three formats.

**Acceptance Scenarios**:

1. **Given** a closure whose read left 211 declarations in 16 derivations unconsulted, **When** it is scanned, **Then** C105 contains `closure-declarations-unconsulted: 211 declaration(s) in 16 derivation(s)` and C104 is `partial`.
2. **Given** a closure whose members' contents were not all read, **When** it is scanned, **Then** C105 contains `closure-contents-unread: <unread> of <members> member(s)` and C104 is `partial`.
3. **Given** a closure read with neither gap, **When** it is scanned, **Then** neither code appears, and C104 is decided by the other reasons alone. It can be `complete`.

### User Story 2 - one predicate, no new degradation (Priority: P2)

The native fields and C104 decide from one input, and the new codes change nothing else.

**Why this priority**: two copies of one rule are how C104 and `compositions[]` came to disagree.

**Independent Test**: on the closure goldens after regeneration, `compositions[]` and every SPDX relationship `completeness` are byte-identical to before, and only C104/C105 change.

**Acceptance Scenarios**:

1. **Given** the new codes, **When** ecosystems are classified as degraded, **Then** neither code degrades any ecosystem. Milestone 1278 named none, because the unread declarations' ecosystem is unknown.
2. **Given** any document, **When** C105 carries either code, **Then** `closure_read_partial()` is true for it, and the reverse also holds.

### Edge Cases

- **No closure.** Neither code appears, and every non-closure document is byte-identical.
- **A gap made only of carrier disagreements** (none measured): the declarations code fires with `0 declaration(s) in 0 derivation(s), K carrier disagreement(s)`.
- **Contents never attempted versus attempted and absent:** C213 already distinguishes these, and this feature only counts what C213 reports as unread.
- **A closure that fails to evaluate:** that is C190's case, not this one. No read happened, so neither code appears.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: C105 MUST gain `closure-declarations-unconsulted`. It fires exactly when the closure read outcome has a gap (unconsulted fields or carrier disagreements), with the detail in Clarifications.
- **FR-002**: C105 MUST gain `closure-contents-unread`. It fires exactly when the closure contents record has members absent, partial or failed, with the detail in Clarifications.
- **FR-003**: The two codes together MUST fire if and only if `closure_read_partial()` is true. A test MUST assert the equivalence on fixtures covering each gap alone, both, and neither.
- **FR-004**: Neither code MAY degrade an ecosystem. `degraded_ecosystems`, the per-ecosystem `compositions[]` records and SPDX 3 relationship `completeness` MUST be unchanged.
- **FR-005**: The three format emitters (CycloneDX, SPDX 2.3, SPDX 3) MUST pass the input to the completeness computation, so C104 and C105 are identical in all three formats, and parity-check MUST report no new failing row. The orphan-reason pass does not need it: it reads only the reachable set, which a reason code cannot change. It already passes no input for the same reason (spec 1652's scope drop), and it MUST be shown unchanged.
- **FR-006**: The C105 catalog row (`docs/reference/sbom-format-mapping.md`) and the graph-completeness vocabulary contract MUST list both codes with their detail templates.
- **FR-007**: Corpus goldens MUST be regenerated only by `public-corpus.yml` dispatch. The diff MUST be traced: only C104 and C105 change, and only on the five closure targets.
- **FR-008**: Zero new Cargo dependencies, and no new network, store or file read. The inputs are already computed.

### Key Entities

- **Closure read outcome**: the existing `ReadOutcome`, holding unconsulted fields, declarations, derivations affected and carrier disagreements.
- **Contents record**: the existing C213 record, holding members read, partial, absent, failed and with no outputs.
- **Closure-read reason codes**: the two new C105 codes, each a pure function of one of the two records.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: shellcheck's golden reads C104 `partial`. Its C105 is exactly `closure-declarations-unconsulted: 211 declaration(s) in 16 derivation(s); closure-contents-unread: 1284 of 2045 member(s)`.
- **SC-002**: moat, openai, slack-web and slacklinker each gain exactly those two clauses, with their measured counts (211/16 and 1087/1590; 198/16 and 471/1330; 211/16 and 416/1193; 228/17 and 1153/2351), and nothing else in the document changes.
- **SC-003**: no non-closure golden changes.
- **SC-004**: on stub fixtures, a scan with neither gap names neither new code, and its C104 is decided by its other reasons alone (`complete` when it has none); a scan with each gap names that gap's code. A mutation feeding `false` to the new input makes the shellcheck-shaped test fail.
- **SC-005**: parity-check's failing rows on every golden are unchanged.

## Assumptions

- The clause order appends after the existing codes, so every existing clause keeps its position and only new text is added to each golden's C105.
- `closure_read_partial()` is the authority. This feature adds no new notion of "partial"; it only reports the existing one in C105.

## Out of Scope

- Consulting `defaultNativeBuildInputs` and `extraNativeBuildInputs`, which would close the declaration gap itself.
- Reading members' contents from a remote store.
- CycloneDX `compositions[]` gaining a record when C104 is `complete`. That is unchanged and was observed under #1720.
