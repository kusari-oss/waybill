# Phase 0 Research — m776 component source-provenance references

**Feature**: 776-component-source-refs
**Status**: Complete
**Date**: 2026-09-05

Zero `NEEDS CLARIFICATION` remain after `/speckit.clarify` (Q1 → five-label mapping with `ORIGIN` deferred; Q2 → aggregate mapping summary). Research below records the decisions the plan phase locks in for `/speckit.tasks`, plus the empirical checks run during specification.

---

## R1 — The enrichment data is already fetched and discarded

**Decision**: US1 consumes `VersionInfo.links` inside `depsdev_source.rs::apply_version_info` — the function that already applies `info.licenses` to the component from the same response.

**Rationale**: The deps.dev version endpoint returns `links[]` in the same payload as `licenses`, and waybill already deserializes it into `VersionInfo.links: Vec<Link>` where `Link { label: String, url: String }`. A source comment at `deps_dev_client.rs:4` states the situation outright: the payload "drives license enrichment but `advisory_keys` / `links` aren't yet" consumed.

This is why FR-007 can forbid any additional network request: the data is already in memory on every enrichment-enabled scan and is being dropped on the floor. The per-scan response cache in `depsdev_source.rs` is likewise reused unchanged, so link mapping inherits its deduplication of upstream calls.

**Alternatives considered**:
- **A second deps.dev endpoint (project-level) for richer repository metadata**: rejected — adds network cost for information the version endpoint already carries, and would violate FR-007.
- **Fetching repository URLs from each ecosystem's own registry** (PyPI JSON API, npm registry, …): rejected — N new integrations, N new failure modes, and per-ecosystem rate limits, to obtain what one already-made call returns.

---

## R2 — Label vocabulary, measured rather than assumed

**Decision**: Map five labels to natively-defined CycloneDX types; defer `ORIGIN`.

| deps.dev label | CDX `externalReference.type` | Status |
|---|---|---|
| `SOURCE_REPO` | `vcs` | mapped |
| `ISSUE_TRACKER` | `issue-tracker` | mapped |
| `DOCUMENTATION` | `documentation` | mapped |
| `HOMEPAGE` | `website` | mapped |
| `ATTESTATION` | `attestation` | mapped |
| `ORIGIN` | — | **deferred** (Clarifications Q1) |

**Rationale**: The vocabulary was sampled live rather than assumed. Across a 30-component npm sample: `SOURCE_REPO` 30/30, `ORIGIN` 30/30, `HOMEPAGE` 25/30, `ISSUE_TRACKER` 21/30, `ATTESTATION` 20/30. The original spec draft anticipated only three labels; two more were present on the majority of components and would have been silently discarded by FR-003's omit-unrecognized rule — a rule written for *hypothetical future* labels, not for labels shipping today.

All six candidate types were verified present in the CycloneDX 1.6 `externalReference.type` enum (43 types total), so every mapped kind is natively defined and FR-005 holds without a `waybill:*` property.

`ORIGIN` is deferred because its semantics are not determinable from the label alone. Assigning it a kind would be exactly the guess FR-003 exists to prevent. Confirming its meaning upstream and mapping it if warranted is a candidate follow-up.

`ATTESTATION` is mapped deliberately: it points at upstream build provenance, which is load-bearing for this project's attestation-first posture. Discarding it would be a substantive loss rather than a neutral default.

**Alternatives considered**:
- **Map only the three originally anticipated labels**: rejected in Clarifications Q1 — discards two labels present on most components.
- **Map `ORIGIN` to `distribution` now**: rejected — plausible but unconfirmed; a wrong reference is worse than an absent one under Principle IX (Accuracy).

---

## R3 — Coverage targets validated before they were committed to

**Decision**: SC-001 and SC-002 keep their ≥80% targets.

**Rationale**: The targets were checked against the enrichment service before the spec was finalized, not assumed. `SOURCE_REPO` presence measured on the actual component sets: **npm 30/30 (100%)**, **pypi 27/29 (93%)**. Both clear ≥80% with margin; the pypi misses were one package with no links at all and one carrying only `HOMEPAGE` (which US1 also maps, so it still gains a reference).

This check exists because the immediately preceding milestone shipped an aspirational SC-001 that was missed at implement-time and had to be retargeted afterwards. Validating the target during specification rather than discovering it during implementation is the cheaper order.

**Alternatives considered**:
- **State the target as "improved" without a number**: rejected — unfalsifiable, and the whole point of an SC is that it can fail.

---

## R4 — Ordering and deduplication

**Decision**: References are emitted in a deterministic order derived from a stable sort over `(kind, url)`, and deduplicated on that same pair.

**Rationale**: FR-013 requires determinism and SC-005 verifies it by double-run byte-identity. deps.dev's `links[]` array order is not contractually stable, and the existing `external_refs_from_purl` output would be concatenated with it, so relying on insertion order would risk cross-run diffs.

Deduplication is on `(kind, url)` rather than URL alone, per the spec edge case: the same URL legitimately appears under two kinds (a repository that is also the documentation site), and those are two different claims a consumer filters on independently.

**Alternatives considered**:
- **Preserve upstream order**: rejected — not contractually stable; risks SC-005 failures that would look like nondeterminism bugs.
- **Deduplicate on URL alone**: rejected — collapses genuinely distinct claims.

---

## R5 — No emitter changes are required (FR-016 is free)

**Decision**: This milestone touches only the population side. All three emitters already consume `ResolvedComponent.external_references`.

**Rationale**: Verified by inspection:
- **CycloneDX** — `generate/cyclonedx/builder.rs` (~:1212) maps each entry to `{type, url}` on the component.
- **SPDX 2.3** — `generate/spdx/packages.rs` (~:520) emits `externalRefs[]` with `category: OTHER`, passing `ref_type` through verbatim, so *any* mapped kind flows through.
- **SPDX 3** — `generate/spdx/v3_packages.rs` (~:165) maps `vcs → software_sourceInfo`, `homepage`/`website` → `software_homePage`, `distribution` → `software_downloadLocation`.

Populating the field is therefore sufficient for all three formats, which is what makes FR-016 satisfiable without emitter work.

**Known asymmetry, accepted**: SPDX 3's per-package mapping has scalar slots only for vcs/website/distribution; `issue-tracker`, `documentation`, and `attestation` fall through its `_ => {}` arm. Those kinds will appear in CycloneDX and SPDX 2.3 but not SPDX 3. FR-016's "wherever each format natively supports them" is written to cover exactly this. SPDX 3 does have per-element `ExternalRef` machinery (used today for the document-level OpenVEX sidecar), so extending it is possible — but that is emitter work with its own parity and catalog obligations, and is out of scope here. Recorded as a follow-up.

---

## R6 — Parity extractors A9/A10/A11 stop being vacuous

**Decision**: No new catalog rows are added. Rows A9 (homepage), A10 (vcs), and A11 (distribution) already exist with native homes documented in all three formats, and all three already have parity extractors (`cdx_homepage`/`spdx23_homepage`/`spdx3_homepage`, and the `_vcs` / `_distribution` equivalents).

**Rationale, and a risk worth naming**: those extractors are currently passing on essentially empty data — almost no component carries a reference today, so the cross-format comparison compares nothing to nothing. **This milestone makes them meaningful for the first time.** Any asymmetry between how CycloneDX, SPDX 2.3, and SPDX 3 represent the same reference will surface as a parity failure the moment references are populated.

That is a feature, not a hazard: it is a built-in correctness guard that arrives free. But it means the parity suite is a *likely* place for this milestone to fail first, and task ordering should treat a parity failure as expected-to-be-investigated rather than surprising.

Per memory `feedback_sbom_format_mapping_extractor_gate`, every catalog row must have a matching extractor or the gate fails. Since no row is added, that gate is unaffected. Adding rows for issue-tracker/documentation/attestation would require SPDX 3 emission first (R5), so rows must not be added ahead of that work.

**Alternatives considered**:
- **Add three new catalog rows now**: rejected — would require extractors, which would require SPDX 3 per-package `ExternalRef` emission to make parity pass. Rows ahead of emission code is precisely what the extractor gate forbids.

---

## R7 — Offline derivation: which ecosystems qualify

**Decision**: US2 adds `distribution` references only for ecosystems whose registry download URL is fully determined by the PURL — name, version, and (where applicable) namespace — with no registry metadata lookup.

**Rationale**: The existing `external_refs_from_purl` is a pure function of the PURL plus annotations, with no I/O. US2 preserves that property, which is what lets it work under `--offline` where US1 cannot.

Qualification is a property of each registry's URL scheme, not of waybill. An ecosystem whose download URL embeds a content hash or an upload-time path segment does not qualify, and FR-010 requires emitting nothing rather than guessing. The precise per-ecosystem qualification list is a task-phase determination: each candidate must be verified against its registry's documented URL scheme before an arm is added, and an arm must not be added on the strength of a pattern that merely appears to work.

**Correction to the existing behavior, in scope**: three of the four ecosystems currently covered (`cargo`, `nuget`, `maven`) emit a registry *landing page* as `website`. That is a different claim from a distribution URL and does not answer the source-provenance question — which is why `rust-ripgrep` measured 0.1/10 for source coverage while carrying 61 references. FR-011 requires those existing references be preserved; US2 adds the distribution reference alongside rather than replacing it.

**Alternatives considered**:
- **Replace the `website` references with `distribution`**: rejected — violates FR-011 and would be a regression for consumers relying on the landing page.
- **Add PURL-derived `vcs` guesses for pypi/npm** (e.g. assuming a GitHub org from the package name): rejected outright — unsound, and would fabricate references in violation of Principle IX.

---

## R8 — Observability shape

**Decision**: One aggregate summary per scan (FR-014a) reporting references emitted per kind, links skipped as unmapped, and links skipped as malformed — the last two counted separately (FR-014b).

**Rationale**: Q1 is the argument for this. Two labels present on most components were being silently discarded by a rule intended for unknown future labels, and that was discovered only by hand-probing the service during specification. A skipped-label counter turns that class of drift into a visible number on every scan.

Separating unmapped-label skips from malformed-URL skips (FR-014b) matters because they call for different responses: a rising unmapped count means the upstream vocabulary moved and waybill should consider mapping a new label; a rising malformed count means upstream data quality degraded and waybill should not. Conflating them would obscure both.

Aggregate-once rather than per-component is required by FR-014a — on a 369-component fixture, per-component output would be unusable noise, and the per-occurrence warning is explicitly forbidden by FR-003.

**Alternatives considered**:
- **Count emissions only, not skips**: rejected in Clarifications Q2 — confirms coverage but leaves vocabulary drift invisible, which is the failure mode actually observed.
- **A `waybill:*` document property carrying the counts**: rejected — this is operator diagnostics, not SBOM content, and would add a vendor property (FR-005 disfavors) for information with no consumer in the document.

---

## R9 — Pipeline ordering: where the summary may be emitted, and what operator refs actually are

**Decision**: emit the FR-014a summary after the last component-set mutation (~`scan_cmd.rs:3883`) and before serialization. Do not anchor it to the enrichment call.

**Rationale**: several passes run after `enrich_components` (~3520) and can still remove components: `deduplicate()` (~3587), `reconcile_design_source_tiers()` (~3600), a `components.retain` drop pass (~3624), the supplement install rebinding `components` (~3849), and the layer-digest and workspace-member tag passes (~3865, ~3883). Because the plan derives per-kind counts by counting the final component set — precisely so the counts cannot drift from the document — counting before those passes would reintroduce the drift it was designed to prevent, and would **overcount** whenever a component carrying references is dropped. That is a direct SC-009a violation.

The pre-existing `"scan complete"` log at ~3406 is not a valid anchor either: it fires *before* enrichment.

**Operator-supplied references are structurally out of reach.** The supplement merge stores them as a `waybill:supplement-externalReferences` annotation (`supplement/merge.rs:193`), not in the `external_references` field, and runs at ~3841. FR-012 ("unmodified and un-reordered") is therefore satisfied by construction rather than by effort — and the one way to actually violate it would be to "helpfully" wire supplement references into `external_references` so that normalization covers them. Do not.

**Related risk, unverified**: neither `deduplicator.rs` nor `reconciler.rs` touches `external_references` in production code — both occurrences are `#[cfg(test)]` fixtures — which implies whole-value retention when components are folded. If a folded-away duplicate carried references its survivor lacks, they would be lost silently. Enrichment runs before the fold, so both duplicates are enriched and the survivor should carry its own. Noted so that coverage landing below target is not misdiagnosed as a mapping defect; the T024 baseline diff would surface it.

**Alternatives considered**:
- **Emit inside `enrich_components`**: rejected — it cannot see US2's derived references (added earlier, in `scan_fs`) nor any later component drop, so its counts could not satisfy SC-009a.
- **Thread counters through both paths instead of counting the final set**: rejected — reintroduces exactly the drift between reported and actual counts that counting the final set eliminates.

---

## Non-decisions / explicit deferrals

- **`ORIGIN` label mapping**: deferred pending upstream confirmation of its semantics (R2, Clarifications Q1).
- **SPDX 3 per-package `ExternalRef` emission** for kinds without a `software_*` scalar slot: deferred (R5). Would enable catalog rows for issue-tracker/documentation/attestation.
- **New catalog rows** for the three kinds above: blocked on the SPDX 3 work; rows must not precede emission code (R6).
- **Ecosystems whose distribution URL requires registry metadata**: out of scope by construction (R7, FR-010).
- **The scoring tool that surfaced this gap** is an instrument, not a specification. Requirements are stated against emitted native fields; score movement is a consequence. Where the tool's conventions and the format specifications disagree, the specifications govern — a principle adopted after the immediately preceding investigation rejected a proposed change that would have encoded a scorer's private convention over a documented format-semantics decision.
