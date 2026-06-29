# Feature Specification: Expand consumer-guide depth coverage — trust trio + linkage + unresolved-deps + assertion-conflict

**Feature Branch**: `151-expand-consumer-guide`
**Created**: 2026-06-29
**Status**: Draft
**Input**: User description: "151"

## Origin & context

Milestone 150 shipped `docs/reference/reading-a-mikebom-sbom.md`, a consumer-facing reading guide that depth-covers 12 `mikebom:*` signals across 4 thematic clusters (vulnerability scanning, compliance auditing, build provenance, transparency / completeness gaps) and indexes ~85 additional signals in Appendix A with a one-line summary + a link to the catalog C-row.

A maintainer-cadence review surfaced that the 12-signal selection was not fully principled:

- The selection was driven partly by which signals had **recent milestone hooks** (most recently shipped milestones came to mind first when authoring), rather than by a **consumer-utility ranking**.
- Several older signals — `mikebom:evidence-kind` (milestone 002-era, catalog C4), `mikebom:confidence` (C16), `mikebom:linkage-kind` (C12), `mikebom:not-linked` (C41), `mikebom:depends-unresolved` / `mikebom:rdepends-unresolved` (C77 / C78), and `mikebom:assertion-conflict` (C67) — are **as consumer-actionable** as the 12 already depth-covered, yet sit in the appendix-only tier.
- The maintainer flagged `mikebom:evidence-kind` specifically as a signal that "seems like something a consumer would want to know" and noted that the depth-vs-appendix split "feels somewhat random" for at least some of the deferred signals.

The maintainer also explicitly agreed that **ecosystem-niche signals** (Yocto `mikebom:yocto-*`, Mach-O `mikebom:macho-*`, Maven `mikebom:shade-relocation`, etc.) are correctly deferred to the appendix — the gap is specifically in the **tier-1 cross-ecosystem consumer-actionable** layer.

This milestone closes that gap on two axes:

1. **Add depth coverage** for the 6 missed tier-1 signals listed above (paired signals C77 + C78 count as one entry).
2. **Document the curation criterion** explicitly in the doc itself, so future maintainers (and external reviewers) can apply the same standard when new `mikebom:*` keys are added to the catalog.

This is a focused docs-only milestone, same shape as milestone 150: single-file edit to `docs/reference/reading-a-mikebom-sbom.md` plus a re-run of the `verify-recipes.sh` authoring harness against the new jq recipes.

## Clarifications

### Session 2026-06-29

- Q: What shape should the documented curation criterion take in the doc? → A: Decision rubric — 3–5 yes/no criteria; depth-cover if N (a documented threshold) or more apply. Mechanical, falsifiable, removes author discretion.
- Q: How should the depth-coverage section frame the emission scope of `mikebom:depends-unresolved` / `mikebom:rdepends-unresolved`? → A: Reserved-key framing — describe the wire shape generically; note inline "currently emitted only by the Yocto recipe reader (milestone 128)"; signal the key namespace is reserved for future cross-ecosystem use.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Vulnerability-scanner author needs to threshold on identification trust (Priority: P1)

A consumer building a vulnerability-scanning policy engine needs to know **how confident mikebom is** about each component's identity, so they can apply differential alerting (e.g., "alert on confirmed-runtime + direct-observation + confidence ≥ 0.85, downgrade to advisory on heuristic-confidence ones"). Today the consumer guide depth-covers `mikebom:source-type` (where the evidence came from) but **not** `mikebom:evidence-kind` (how it was derived) or `mikebom:confidence` (how strongly mikebom backs the claim). The trust-trio cluster is incomplete; the consumer must read the catalog row + Rust source to figure out the value space and pairing semantics.

**Why this priority**: This is the gap the maintainer explicitly flagged. The trust trio (`mikebom:source-type` + `mikebom:evidence-kind` + `mikebom:confidence`) is the single most-asked-about cross-ecosystem signal for risk-weighting SBOM consumption. Without depth coverage of all three, consumers can't build the threshold-based policies that vulnerability-scanner authors actually want.

**Independent Test**: After this milestone ships, a consumer reads the updated guide end-to-end, formulates the question "how do I filter to only high-confidence directly-observed components in a mikebom CDX SBOM?", and constructs a working `jq` query using only the depth-covered sections (sections 3.1 and 3.3 — vulnerability-scanning and build-provenance clusters). They never need to leave the guide.

**Acceptance Scenarios**:

1. **Given** the updated guide, **When** a consumer opens section 3.3 (build provenance), **Then** they find `mikebom:evidence-kind` and `mikebom:confidence` documented with the same depth structure used for `mikebom:source-type` (per-format placement, value space, action guidance, jq recipe with expected output).
2. **Given** the depth coverage for the trust trio, **When** the consumer reads it, **Then** they understand how the three signals compose — specifically, that `mikebom:source-type` answers "where", `mikebom:evidence-kind` answers "how", and `mikebom:confidence` answers "how strongly", and that they're meant to be filtered on together.
3. **Given** the trust-trio depth coverage, **When** the consumer runs the documented jq recipe, **Then** the output matches the example in the doc (verified at authoring time via `verify-recipes.sh`).

---

### User Story 2 — Binary-tier consumer needs to filter CVEs by linkage mode (Priority: P1)

A consumer running CVE matching against a mikebom-emitted SBOM for an OCI image needs to know **which components were actually linked into the final binary**, so they can suppress false-positive vulnerability alerts for source-tier modules that the linker dead-code-eliminated. Today `mikebom:linkage-kind` (binary-tier dynamic / static / mixed marker) and `mikebom:not-linked` (Go-specific "declared in source but not in the binary's BuildInfo" marker) are appendix-only — the consumer can't build a CVE-suppression policy without reading the catalog rows.

**Why this priority**: Same severity as US1 — these signals materially affect which CVEs apply to a deployed artifact. Without depth coverage, downstream CVE-matching tooling either over-reports (alerts on not-linked modules that aren't in the binary) or silently drops the signal.

**Independent Test**: A consumer reads the updated guide's vulnerability-scanning cluster (section 3.1), formulates the question "I'm getting CVE alerts for `bytedance/sonic` but I'm using gin's slow-path build tag — how do I suppress them?", and constructs a jq query against the `mikebom:not-linked` annotation using only the guide content. They never need to leave the guide.

**Acceptance Scenarios**:

1. **Given** the updated guide's section 3.1, **When** the consumer searches for "linkage" or "linked", **Then** `mikebom:linkage-kind` and `mikebom:not-linked` are both depth-covered with per-format placement and a worked jq example.
2. **Given** the depth coverage, **When** the consumer reads the `mikebom:not-linked` section, **Then** they understand the two-state semantics: present = `true` (proven not-linked); absent = either "confirmed linked via BuildInfo" OR "no binary present to compare against" — and they're told explicitly to check whether a binary was scanned to disambiguate the absent case (per catalog C41).
3. **Given** the `mikebom:linkage-kind` coverage, **When** the consumer reads the cross-format placement table, **Then** the closed enum (`dynamic` / `static` / `mixed`) is documented along with its native-field-equivalence note (no native field; parity bridge per Constitution Principle V).

---

### User Story 3 — Compliance auditor needs unresolved-deps + supplement-conflict visibility (Priority: P1)

A consumer using a mikebom SBOM as the source of truth for compliance attestation needs to know **what mikebom couldn't fully resolve** (declared deps that didn't pin to a concrete component) and **where the operator's supplement file overrode scanner-discovered facts**, because both are auditability-critical: declared-but-unresolved deps represent closure gaps in the SBOM, and assertion conflicts represent operator-asserted overrides that auditors must validate against external evidence. Today `mikebom:depends-unresolved` + `mikebom:rdepends-unresolved` (paired closure-gap markers from the Yocto reader, milestone 128) and `mikebom:assertion-conflict` (the supplement-merge conflict marker from milestone 119) are appendix-only.

**Why this priority**: Same severity as US1 + US2 — these signals appear when the SBOM is being read as a compliance artifact, which is one of the doc's three named consumer personas (vulnerability scanning, compliance auditing, build provenance). The transparency / completeness cluster (section 3.4) and the compliance cluster (section 3.2) both need these signals to be self-sufficient for the auditor persona.

**Independent Test**: An auditor reads the updated guide, formulates the question "did mikebom resolve every dep this SBOM declared? and were any of the values it emitted overridden by the operator?", and constructs jq queries against the depth-covered annotations to answer both questions, using only the guide content.

**Acceptance Scenarios**:

1. **Given** the updated guide's section 3.4 (transparency / completeness), **When** the auditor reads it, **Then** `mikebom:depends-unresolved` + `mikebom:rdepends-unresolved` are depth-covered as a paired entry (same paired-coverage pattern already used for `mikebom:graph-completeness` + `mikebom:graph-completeness-reason`).
2. **Given** the updated guide, **When** the auditor reads the unresolved-deps entry, **Then** they understand the value space (JSON-encoded array of names that did NOT resolve) generically as a wire shape, see an inline note clarifying that the only current emitter is the Yocto recipe reader (milestone 128), recognize that the key namespace is reserved for future cross-ecosystem use, and grasp the audit interpretation (each entry is a known dep that mikebom couldn't pin to a concrete component — closure gap, NOT a missing dep).
3. **Given** the depth coverage for `mikebom:assertion-conflict`, **When** the auditor reads it, **Then** they understand the structured record shape (`{field, scanner_value, supplement_value, winner, justification}`), the closed `justification` enum (`bytes-evident-detection-preserved` / `developer-metadata-override`), and the audit-significant question this answers ("the operator told mikebom X; mikebom observed Y; here's who won and why").
4. **Given** all 3 signals' depth coverage, **When** the auditor runs the documented jq recipes against a real mikebom-emitted SBOM, **Then** the output matches the example in the doc (verified at authoring time via the extended `verify-recipes.sh`).

---

### User Story 4 — Maintainer needs an explicit curation criterion to avoid drift (Priority: P2)

A future maintainer adding a new `mikebom:*` annotation to the catalog needs a written rule for **when the new key warrants depth coverage versus appendix-only inclusion**, so the depth-vs-appendix split stays principled across milestones rather than drifting back into "whatever the author remembered."

**Why this priority**: This is the meta-fix for the maintainer's "feels random" critique. Without a documented criterion, a future milestone (say 200) would face the same problem this milestone is solving: which of the newest `mikebom:*` keys cross the depth-coverage threshold? Documenting the criterion is cheaper than re-litigating it every time.

**Independent Test**: A future maintainer adds a new `mikebom:foo-bar` key to the catalog. They open the consumer guide, find the curation-criterion section, and can determine in under 5 minutes whether `mikebom:foo-bar` warrants depth coverage or appendix-only treatment — without needing to consult the maintainer who wrote this milestone.

**Acceptance Scenarios**:

1. **Given** the updated guide, **When** the maintainer reads section 2 (How to read this doc) or a new dedicated subsection, **Then** they find a documented criterion that distinguishes depth-covered signals from appendix-only signals.
2. **Given** the documented criterion, **When** the maintainer applies it to the existing 18 depth-covered signals (12 from milestone 150 + 6 from this milestone), **Then** each one satisfies the criterion (no false positives).
3. **Given** the documented criterion, **When** the maintainer applies it to representative appendix-only signals (e.g., `mikebom:macho-load-cmd-version`, `mikebom:shade-relocation`, `mikebom:yocto-layer-version-missing`), **Then** none of them satisfy it (no false negatives among the deferred set the maintainer flagged as correctly deferred).

---

### User Story 5 — Appendix hygiene cleanup (Priority: P3)

The appendix index currently lists some keys that are either **internal-only** (stripped before emission, never reach the consumer) or **cross-reference targets that no longer point at depth coverage** post-milestone-150. A future-proofing pass tightens the appendix to only list keys that actually appear in emitted SBOMs.

**Why this priority**: Low-risk hygiene. Wrong appendix entries waste consumer time but don't block any specific consumer workflow; deferring to the lowest priority slot.

**Independent Test**: A consumer searching the appendix for an annotation key always finds either (a) the depth-coverage section they need, or (b) the catalog row for the wire shape — never an entry that points at something that doesn't exist or describes an internal-only field.

**Acceptance Scenarios**:

1. **Given** the appendix, **When** a maintainer audits each entry against the actually-emitted set (grep the codebase for `properties.push` / `annotations.push` / equivalent emission points), **Then** every appendix entry corresponds to an annotation that an SBOM consumer can actually observe in the wire output.
2. **Given** the audit, **When** any internal-only key is found (e.g., a key stripped before emission per a milestone-specific cleanup), **Then** it's removed from the appendix with a brief note in the milestone's PR description.
3. **Given** the audit, **When** any appendix entry's cross-reference points at a section that doesn't exist in the doc, **Then** the cross-reference is corrected to point at a section that does (depth coverage if available, catalog row otherwise).

---

### Edge Cases

- **Catalog drift during this milestone**: A new `mikebom:*` key may be added to the catalog while this milestone is in flight (e.g., a parallel milestone N+1 adds a key). The doc's appendix MUST cover every key in the catalog at THIS milestone's ship time (same invariant as milestone 150 SC-002). Resolution: re-run the SC-002 audit immediately before merge.
- **A depth-covered signal's wire format changes**: If a milestone after 151 changes one of the 18 depth-covered signals' value space or per-format placement, the depth coverage becomes stale. Resolution: each depth-covered section already links to its C-row (the wire-shape source-of-truth), and the documented curation criterion (US4) lets future authors keep the depth coverage in sync with the catalog. No special handling required at THIS milestone.
- **A signal that's marginally tier-1 today becomes clearly tier-1 later**: If consumer feedback after merge surfaces another appendix-only signal as "actually consumer-actionable," that's a follow-up milestone (152 or later). The documented criterion (US4) makes the decision principled rather than reactive.
- **Format-specific divergence in jq recipe shape**: CDX uses flat `properties[].value`; SPDX 2.3 + SPDX 3 use the `mikebom-annotation/v1` envelope `{schema, field, value}` carrier. Each new depth-covered signal's jq recipe block MUST show all three formats (matching the milestone-150 per-signal-rendering invariant from data-model §2).
- **`mikebom:not-linked` Go-only emission**: this signal is emitted by the Go-specific binary-tier comparison logic (milestone 050). Documenting it as a general "vulnerability-scanning" signal risks consumers expecting it on non-Go components. The depth coverage MUST be explicit about the Go-only scope.

## Requirements *(mandatory)*

### Functional Requirements

#### Depth coverage additions (US1 + US2 + US3)

- **FR-001**: The updated guide MUST add a depth-covered subsection for `mikebom:evidence-kind` (catalog C4) under section 3.3 (build provenance), following the milestone-150 per-signal rendering invariant: What it is, Where it lives (per-format), Value space, What to do with it, Milestone, Catalog link, jq recipe + Expected output.

- **FR-002**: The updated guide MUST add a depth-covered subsection for `mikebom:confidence` (catalog C16) under section 3.3 (build provenance), following the same per-signal rendering invariant as FR-001. The section MUST explicitly call out the pairing semantics with `mikebom:source-type` and `mikebom:evidence-kind` (the "trust trio" framing).

- **FR-003**: The updated guide MUST add a depth-covered subsection for `mikebom:linkage-kind` (catalog C12) under section 3.1 (vulnerability scanning), following the per-signal rendering invariant. The value space MUST be documented as a closed enum (`dynamic` / `static` / `mixed`) per catalog C57.

- **FR-004**: The updated guide MUST add a depth-covered subsection for `mikebom:not-linked` (catalog C41) under section 3.1 (vulnerability scanning), following the per-signal rendering invariant. The section MUST explicitly state the Go-only emission scope AND the two-state interpretation rule (present = proven not-linked; absent = either confirmed-linked OR no-binary-present).

- **FR-005**: The updated guide MUST add a depth-covered subsection for `mikebom:depends-unresolved` + `mikebom:rdepends-unresolved` (catalog C77 + C78) under section 3.4 (transparency / completeness gaps), as a paired entry (same paired-coverage pattern milestone 150 uses for `mikebom:graph-completeness` + `mikebom:graph-completeness-reason`). The section MUST use **reserved-key framing** per Clarifications Q2: describe the wire shape generically (JSON-encoded array of unresolved dep names), include an inline note that the only current emitter is the Yocto recipe reader (milestone 128), and signal that the key namespace is reserved for any future cross-ecosystem use. This framing accurately reflects current emission reality (Yocto-only today) without forcing a doc update if other readers adopt the key later.

- **FR-006**: The updated guide MUST add a depth-covered subsection for `mikebom:assertion-conflict` (catalog C67) under section 3.4 (transparency / completeness gaps), following the per-signal rendering invariant. The section MUST document the structured record shape (`{field, scanner_value, supplement_value, winner, justification}`) and the closed `justification` enum.

#### Curation criterion (US4)

- **FR-007**: The updated guide MUST include a written curation criterion that distinguishes depth-covered signals from appendix-only signals. The criterion MUST be shaped as a **decision rubric**: a small set of 3–5 yes/no criteria, with a documented threshold N such that a signal warrants depth coverage if N or more criteria evaluate to "yes." The criterion MUST be unambiguous enough that a future maintainer can apply it mechanically to a new `mikebom:*` key without re-litigating the depth-vs-appendix decision case-by-case. (Drafted as a rubric per Clarifications Q1 — removes author discretion that produced milestone-150's "feels random" selection.)

- **FR-008**: The curation rubric MUST be consistent with the 18 depth-covered signals after this milestone (the original 12 + the 6 added here) — i.e., each of the 18 must score ≥ N on the rubric.

- **FR-009**: The curation rubric MUST exclude the representative appendix-only signals the maintainer flagged as correctly deferred (e.g., Mach-O load-command details, Maven shade relocations, Yocto layer-version metadata) — i.e., none of these may score ≥ N on the rubric.

#### Appendix hygiene (US5)

- **FR-010**: The updated guide's Appendix A MUST list ONLY annotation keys that are present in emitted SBOMs (i.e., keys that survive to the wire output). Internal-only keys that are stripped before emission MUST be removed from the appendix.

- **FR-011**: Every appendix entry's cross-reference (the "see §X" pointer) MUST resolve to a section that exists in the doc — either a depth-coverage section (preferred) or the catalog row in `sbom-format-mapping.md` (fallback).

#### Recipe verification + cross-format coverage

- **FR-012**: The `verify-recipes.sh` authoring harness at `specs/151-expand-consumer-guide/verify-recipes.sh` (a new file mirroring milestone 150's harness) MUST verify every new jq recipe added by this milestone. The harness MUST exit 0 when every recipe produces the documented output shape against a real mikebom-emitted SBOM.

- **FR-013**: Each new depth-covered signal MUST have a per-format placement entry covering all three SBOM formats (CDX 1.6, SPDX 2.3, SPDX 3.0.1), even when the wire shape is identical across formats (the consistency benefits consumers more than the redundancy costs).

#### Discoverability + linking

- **FR-014**: Each new depth-covered signal's Appendix A entry MUST be updated to cross-reference the new depth-coverage section (replacing the previous catalog-only pointer with a "see §3.X for depth coverage" pointer).

- **FR-015**: Appendix B (the milestone-citation map) MUST list the 6 newly-depth-covered signals with their originating milestones (002 / 050 / 119 / 128 / etc.) — same shape as the existing Appendix B entries.

#### Out-of-scope guards

- **FR-016**: This milestone MUST NOT introduce any new `mikebom:*` annotation keys (it documents existing ones).
- **FR-017**: This milestone MUST NOT change the wire format of any existing annotation (no value-space changes, no envelope-shape changes, no per-format placement changes).
- **FR-018**: This milestone MUST NOT change `sbom-format-mapping.md` (the catalog is the source of truth; this milestone consumes it, doesn't extend it). Exception: an inline catalog clarification IS allowed if the depth-coverage authoring surfaces a genuine catalog-row ambiguity that needs to be resolved at the source — but that should be flagged in the PR description as a side effect, not the main work.
- **FR-019**: This milestone MUST NOT promote any *additional* appendix-only signals to depth coverage beyond the 6 explicitly listed in FR-001 through FR-006. If consumer feedback after merge surfaces additional signals as warranting depth coverage, that's a follow-up milestone (152 or later) — keeping this milestone's scope tight prevents the "creeping ambition" failure mode that produced milestone 150's curation drift in the first place.

### Key Entities

- **The 6 newly-depth-covered signals**: `mikebom:evidence-kind`, `mikebom:confidence`, `mikebom:linkage-kind`, `mikebom:not-linked`, `mikebom:depends-unresolved` + `mikebom:rdepends-unresolved` (paired), `mikebom:assertion-conflict`.
- **The curation rubric**: a decision rubric (3–5 yes/no criteria with a documented threshold N) embedded in the doc, that future maintainers can apply mechanically to decide depth-vs-appendix for new `mikebom:*` keys. Must be falsifiable against the existing 18-signal depth-covered set (each scores ≥ N) and the appendix-only set the maintainer flagged as correctly deferred (each scores < N).
- **Per-signal rendering invariant**: the milestone-150 data-model §2 shape — What it is, Where it lives (per-format), Value space, What to do with it, Milestone, Catalog link, jq recipe + Expected output — reused unchanged.
- **`verify-recipes.sh`**: a new authoring harness at `specs/151-expand-consumer-guide/verify-recipes.sh`, mirroring milestone 150's pattern. Authoring artifact, not shipped publicly.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (consumer-utility validation)**: After merge, a maintainer or external reviewer with no prior context can read the updated guide and answer all 8 of these questions WITHOUT consulting `sbom-format-mapping.md` or any other doc:
  1. What does `mikebom:evidence-kind` mean and what values can it take?
  2. How do `mikebom:source-type`, `mikebom:evidence-kind`, and `mikebom:confidence` compose to support threshold-based vulnerability-scanner policies?
  3. What's the difference between `mikebom:linkage-kind = "dynamic"` and `mikebom:linkage-kind = "static"`?
  4. If a Go component has `mikebom:not-linked = true`, what does that mean for runtime CVE matching?
  5. If a Go component is missing the `mikebom:not-linked` annotation entirely, what are the two possible interpretations and how do I disambiguate them?
  6. What's the difference between `mikebom:depends-unresolved` and a component being absent from the SBOM entirely?
  7. If a component has a `mikebom:assertion-conflict` annotation with `winner = "supplement"` and `justification = "developer-metadata-override"`, what should an auditor do with that signal?
  8. (Curation-criterion check:) If I'm a future maintainer adding a new `mikebom:foo-bar` annotation, where in the doc do I find the rule for whether `mikebom:foo-bar` warrants depth coverage versus appendix-only?
  - If all 8 answers come from the guide alone: ✅ SC-001 passes. If any answer requires reading the catalog or source: that signal's depth coverage needs strengthening before merge.

- **SC-002 (catalog-↔-appendix coverage)**: Every `mikebom:*` key in `docs/reference/sbom-format-mapping.md` at THIS milestone's ship time MUST be in Appendix A. (Same invariant as milestone 150 SC-002; re-verified at this milestone's ship.) Verified mechanically via the milestone-150 grep-and-diff recipe in `specs/150-sbom-consumer-guide/quickstart.md` Scenario 2.

- **SC-003 (jq recipe runnable verification)**: ≥6 new jq recipes (≥1 per new depth-covered signal × 3 formats minus paired-coverage collapse = ~14 to 16 recipes total) MUST execute successfully against a real mikebom-emitted SBOM and produce the documented output shape, verified by the extended `verify-recipes.sh` harness at authoring time.

- **SC-004 (depth-covered signal count)**: The doc MUST have ≥18 depth-covered signals after this milestone (the original 12 from milestone 150 + the 6 added here). Verified mechanically by the milestone-150 SC-006 quickstart scenario (`awk '/^### 3\./,/^### 4 /' | grep -cE "^#### "`).

- **SC-005 (cluster balance)**: After this milestone, each of the 4 thematic-cluster sections (3.1 / 3.2 / 3.3 / 3.4) MUST contain ≥3 depth-covered signals (was ≥3 before; this milestone keeps that floor and pushes 3 of the 4 clusters above it). The trust-trio additions go to §3.3 (was 3 sections, becomes 5), the linkage additions go to §3.1 (was 3 sections counting paired collapse, becomes 5), the unresolved-deps + assertion-conflict additions go to §3.4 (was 3 sections counting paired collapse, becomes 5). §3.2 (compliance) stays at 3. Final cluster sizes: 5 / 3 / 5 / 5 = 18 sections covering 20 unique catalog keys (3 paired-entry collapses produce the 18-section vs 20-key delta).

- **SC-006 (curation-rubric application)**: A maintainer running the documented decision rubric against the 18 depth-covered signals gets 18/18 scoring ≥ N (depth-cover). Running the rubric against the 7 representative appendix-only signals the maintainer flagged (any of: `mikebom:macho-*`, `mikebom:pe-*`, `mikebom:elf-*`, `mikebom:yocto-*`, `mikebom:co-owned-by`, `mikebom:also-detected-via`, `mikebom:shade-relocation`) gets 0/7 scoring ≥ N. Verified at authoring time by the spec author or a second reviewer.

- **SC-007 (single-file deliverable)**: The shipped change is a single-file edit to `docs/reference/reading-a-mikebom-sbom.md` (plus the new `specs/151-expand-consumer-guide/verify-recipes.sh` authoring artifact, plus the standard speckit branch artifacts). No other doc paths, no Rust source paths, no CI workflow paths touched.

- **SC-008 (pre-PR gate)**: `./scripts/pre-pr.sh` MUST pass with the same status as pre-151 main (i.e., docs-only milestone — clippy + test results unchanged). The documented pre-existing `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` env-only failure remains the only acceptable test failure.

- **SC-009 (appendix-hygiene audit, US5)**: After this milestone, every entry in Appendix A corresponds to an annotation that an SBOM consumer can actually observe in the wire output of at least one mikebom-emitted SBOM. Verified at authoring time by a manual audit; the audit's outcome is documented in the milestone's PR description.

- **SC-010 (cross-reference correctness, US5)**: Every appendix entry's "see §X" pointer resolves to a section that exists in the doc. Verified mechanically via a grep-and-check at authoring time (extract every `see §[0-9.]+` token, confirm each one matches a real section heading).

## Assumptions

1. The milestone-150 per-signal-rendering invariant (What it is / Where it lives / Value space / What to do with it / Milestone / Catalog link / jq recipe + Expected output) is reused unchanged. Re-designing the invariant would be a larger refactor and is out of scope.
2. The milestone-150 4-cluster organization (vulnerability scanning / compliance auditing / build provenance / transparency-completeness) is reused unchanged. The 6 new signals slot into existing clusters; no new clusters are created.
3. The maintainer (mike@kusari.dev) operating-cadence read-through audit is the canonical SC-001 validator. No automated test can verify "is this doc useful to a consumer?" — same operator-cadence pattern milestone 150 established. **Definition** (per analysis remediation A5): "Maintainer-cadence review" / "operator-cadence read-through audit" refers to the SC-001 question-by-question read-through pattern milestone 150 established — a maintainer (or external reviewer simulating a first-time consumer) reads the relevant doc end-to-end and attempts to answer a fixed list of consumer-relevant questions using ONLY the doc content. Any question that requires consulting another doc or the source code is a documentation gap to fix before merge. The 8 SC-001 questions in this milestone are the canonical question list.
4. The `verify-recipes.sh` authoring harness pattern (a Bash script that runs each documented jq recipe against a real mikebom-emitted SBOM and counts pass/fail) is reused from milestone 150. The harness is an authoring artifact, NOT a CI-gated test (mikebom doesn't ship public CI gates for jq-recipe correctness; the harness exists so the milestone author can validate the doc claims at authoring time and so future maintainers can re-run it post-edit).
5. The catalog (`docs/reference/sbom-format-mapping.md`) is the source of truth for wire shape — when this milestone documents a signal's per-format placement, it cites the C-row rather than describing the placement independently. The C-row links in each depth-coverage section's "Catalog link" line are non-optional.
6. The Go-specific scope of `mikebom:not-linked` (catalog C41) is correctly attributed to milestone 050. The guide will not generalize the signal beyond Go even if a future milestone extends emission to other binary tiers — that future milestone will update the guide.
7. The closed enum for `mikebom:linkage-kind` (`dynamic` / `static` / `mixed` per catalog C57's reference to C12) is the canonical value space at this milestone's ship time. The guide cites the closed enum verbatim; if a future milestone extends it (e.g., adds `cgo-import`), that milestone updates the guide.
8. The maintainer's "feels random" critique is interpreted as a real curation gap, not a stylistic preference. The fix is BOTH (a) add the 6 missing tier-1 signals AND (b) document the curation criterion so future drift is prevented. (a) alone would close the immediate gap but leave the next maintainer in the same position; (b) alone would document the rule without applying it.
9. The 6-signal selection is the **final tier-1 set** at this milestone's ship time. The post-merge feedback loop — operator-cadence read-throughs, follow-up issues — drives any further depth-coverage additions in a later milestone. Per FR-019, this milestone does not promote additional signals beyond the 6 listed.
10. SPDX 3 subject-routing quirks (e.g., the milestone-149 demote-annotation case where an annotation lands on the synth-root IRI instead of the demoted entry's IRI) are NOT a concern for this milestone — the 6 new signals are all per-component annotations on real (non-synth-root) Package elements, and the SPDX 3 jq recipes can walk Annotation elements by `field` key (same pattern milestone 149 used).
11. Per Constitution Principle V, the depth-covered sections MUST not invent new "use the native field instead" guidance — when a native field exists (e.g., CDX 1.6's `evidence.identity[].confidence` for milestone-110 fingerprint-confidence), the catalog row already documents the dual-emission posture. The guide reflects what the catalog documents, doesn't extend it.

## Dependencies

- **Milestone 150** (PR #482, merged 2026-06-28): the consumer guide this milestone extends. Provides the per-signal rendering invariant, the 4-cluster organization, the `verify-recipes.sh` harness pattern, the Appendix A/B shape, the SC-001/SC-002/SC-003 audit patterns. This milestone is a strict superset of milestone 150's deliverable.
- **Catalog at `docs/reference/sbom-format-mapping.md`**: source of truth for the 6 newly-depth-covered signals' wire shapes (C4 / C12 / C16 / C41 / C67 / C77 / C78). Each depth-coverage section links back to its C-row.
- **The 6 signals' originating milestones** (for Appendix B citations + the "Milestone" rendering field): C4 evidence-kind → milestone 002-era (foundational), C12 linkage-kind → milestone 005-era (binary readers), C16 confidence → milestone 002-era, C41 not-linked → milestone 050, C67 assertion-conflict → milestone 119, C77/C78 unresolved-deps → milestone 128.

## Out of Scope

- No new `mikebom:*` annotation keys (FR-016).
- No wire-format changes to existing annotations (FR-017).
- No catalog changes beyond inline clarifications surfaced by depth-coverage authoring (FR-018).
- No promotion of additional appendix-only signals beyond the 6 listed (FR-019). Any other signal the post-merge feedback loop surfaces as warranting depth coverage is a future milestone.
- No competitor-tool comparisons (per milestone 150 Q1 Option D — the framing stays consumer-centric).
- No auto-generated appendix (still manually maintained at this milestone's ship; could be a future automation milestone).
- No JSON Schema artifact for `mikebom-annotation/v1` (still cites existing Rust source as canonical).
- No translations (English-only).
- No interactive consumer tooling (static Markdown only).
- No CI gating for `verify-recipes.sh` (still an authoring artifact).
- No Yocto-`mikebom:*` depth coverage (the maintainer explicitly agreed these are correctly appendix-only at this milestone).
- No Mach-O / PE / ELF binary-forensics annotation depth coverage (same as Yocto — correctly appendix-only).
