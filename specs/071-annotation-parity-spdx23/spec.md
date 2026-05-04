# Feature Specification: Cross-format SBOM annotation parity

**Feature Branch**: `071-annotation-parity-spdx23`
**Created**: 2026-05-04
**Status**: Draft
**Input**: User description: SBOM-conformance harness reports 12,165 findings against alpha.13. 91.5% (11,130) are CROSS_FORMAT_INEQUIVALENCE — the same `mikebom:*` annotation key appears in some output formats (CDX / SPDX 2.3 / SPDX 3) but not others. The dominant pattern (84% / 9,379 findings) is "SPDX 2.3 missing the key, CDX + SPDX 3 both have it". The user wants every annotation that mikebom emits to appear in all three formats with equivalent values, **except** where the asymmetry is *inherent to the format spec* (one format has a native standards-side construct the other lacks, so the `mikebom:*` annotation is intentionally one-format-only). Those inherent exceptions need to be explicitly catalogued so the conformance test suite can pass them through without flagging.

## Clarifications

### Session 2026-05-04

- Q: When the parity extractor encounters a `mikebom:*` annotation key emitted by any format that has no entry in the parity catalog, should the pre-PR gate hard-fail or soft-warn? → A: Hard fail. Any uncatalogued emitted key aborts the pre-PR gate; the PR cannot merge until a catalog row is added (with directionality + rationale). Soft-warns rot; the whole point of the guardrail is that drift cannot recur silently.
- Q: How is value-canonicalization for cross-format byte-equivalence governed — by a single generic rule or by per-key metadata in the catalog? → A: Generic-with-explicit-override. Default canonicalization = sort arrays lexicographically + sort object keys + normalize whitespace. Catalog rows may carry an `order_sensitive: true` flag to disable array sorting for the rare keys where insertion order is semantic. All currently-named keys (source-files, cpe-candidates, deps-dev-match, npm-role, sbom-tier) are treated as unordered sets under the default rule.
- Q: Does the SC-001 ≤556 CFI measurement target include document-level CFI rows or only component-level? → A: Component-level subset only. Both the 11,130 baseline and the ≤556 post-fix target are computed over component-level CFI rows; document-level rows are reported separately and excluded from the numerator. Document-level parity is a follow-on milestone if a similar gap shows up there.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator gets identical mikebom-side metadata regardless of which SBOM format they consume (Priority: P1) 🎯 MVP

A downstream consumer (vuln scanner, license auditor, attestation tool) picks one of mikebom's three output formats and expects the per-component metadata mikebom adds — source-file provenance, sbom-tier classification, CPE candidates for vuln matching, deps.dev enrichment markers, npm dep-role classification — to be identical in payload regardless of which format they chose.

**Why this priority**: This is the bulk of the conformance findings — 9,641 of the 11,130 CFI rows are SPDX 2.3 missing data the other two formats already carry. Every consumer that picks SPDX 2.3 today is operating on a strict subset of what the same scan emits in CDX or SPDX 3. Closing this is what gets the conformance harness from "headline number dominated by parity gap" to "headline number = real defects."

**Independent Test**: Run a single mikebom scan, request all three formats. For each component identified by canonical PURL, extract the set of `mikebom:*` annotation keys from each format's emission. The set MUST be identical across formats (except documented inherent exceptions per US3). For keys present in all three, the JSON-canonicalized value MUST be byte-equivalent across formats. Achievable by running the existing `mikebom parity-check` subcommand against an alpha.13-shape scan and asserting zero `SymmetricEqual` row violations.

**Acceptance Scenarios**:

1. **Given** a mikebom scan that produces a component with `mikebom:source-files` populated in CDX, **When** the same scan emits SPDX 2.3 and SPDX 3, **Then** both SPDX outputs carry an equivalent `mikebom:source-files` annotation on the matching SPDX package, with the same file-path payload.
2. **Given** the same scan, **When** comparing the value payloads of `mikebom:sbom-tier`, `mikebom:cpe-candidates`, `mikebom:deps-dev-match`, and `mikebom:npm-role` on each component across the three formats, **Then** payloads are byte-equivalent after canonicalization.
3. **Given** any future annotation key added by a new milestone, **When** that milestone lands, **Then** the key is emitted by all three format emitters in the same merge (cannot land in CDX-only and "catch up" SPDX later).
4. **Given** an alpha.13-shape SBOM run through the existing parity-check infrastructure, **When** all three formats are compared, **Then** zero `Directionality::SymmetricEqual` row violations are reported (down from the current 5+ keys driving ~10,200 CFI rows).

---

### User Story 2 — Conformance harness false-positive count drops to true-defect count (Priority: P1)

The user runs an external conformance harness over mikebom's output and wants the headline finding count to reflect actual emission defects, not the artifact of mikebom being the only multi-format emitter in the comparison set. Today, 91.5% of the harness's 12,165 findings are CFI rows that all stem from one root cause (SPDX 2.3 annotation lag), masking the ~250 real false-positives, 14 real missing-component cases, and 5–8 annotation coverage gaps that deserve attention.

**Why this priority**: Same priority as US1 because they're two views of the same fix. US1 is the *user-facing* outcome (consumer parity); US2 is the *operator-facing* outcome (the conformance signal becomes legible). Achieving US1 mechanically achieves US2.

**Independent Test**: Run the published conformance harness against a post-fix mikebom build over the existing 36-fixture suite. CFI count drops by ≥95% from the alpha.13 baseline (11,130 → ≤556). The remaining CFI rows MUST all be either (a) inherent format-asymmetry rows documented per US3, or (b) genuine bugs slated for follow-on work.

**Acceptance Scenarios**:

1. **Given** the alpha.13 conformance baseline of 11,130 CFI findings, **When** the same harness runs against the post-fix build, **Then** CFI count is reduced by ≥95% (≤556 remaining).
2. **Given** the residual CFI findings after the fix, **When** each is inspected, **Then** every one corresponds to a row marked with a non-`SymmetricEqual` directionality in the parity extractor catalog, and that row's directionality choice is documented with rationale.
3. **Given** the headline finding total (currently 12,165 across all kinds), **When** the harness re-runs post-fix, **Then** the total drops to ≤1,500 — i.e., the headline number is now dominated by the FALSE_POSITIVE / MISSING_COMPONENT / ANNOTATION_MISSING / PURL_MISMATCH buckets, not by CFI.

---

### User Story 3 — Inherent format asymmetries are explicitly catalogued and documented (Priority: P2)

Some asymmetries are not bugs — they exist because one format has a native standards-side construct the other format's spec doesn't have an equivalent for. Example: CDX 1.6's `scope: "excluded"` field has no SPDX 2.3 / SPDX 3 native equivalent; mikebom emits `mikebom:lifecycle-scope` only in CDX (not as a fallback annotation in SPDX, because the scope information is already standards-native there). The user wants every such intentional asymmetry to be (a) documented in the parity catalog with the reason, (b) marked in code with a non-`SymmetricEqual` directionality so the parity extractor doesn't flag it, and (c) explained in `docs/reference/sbom-format-mapping.md` so operators can read the rationale.

**Why this priority**: Lower than US1/US2 because the volume here is small (~262 CFI rows are lifecycle-scope today, and that's already correctly modelled `Directionality::CdxOnly` in the extractor). The work is mostly auditing for *new* legitimate asymmetries the alpha.7 → alpha.13 work introduced and confirming they have the right directionality marker. Without US3, US1 risks over-fitting — forcing a fake annotation into a format where it doesn't belong.

**Independent Test**: Read `mikebom-cli/src/parity/extractors/mod.rs` and `docs/reference/sbom-format-mapping.md`. Every parity row whose directionality is `CdxOnly`, `CdxSubsetOfSpdx`, or `PresenceOnly` (rather than `SymmetricEqual`) MUST have a one-line rationale in the catalog comment AND a corresponding entry in the format-mapping doc explaining why the asymmetry is intentional and what standards-native field replaces the missing annotation in the other format(s).

**Acceptance Scenarios**:

1. **Given** the parity extractor catalog post-fix, **When** auditing each non-`SymmetricEqual` row, **Then** the row carries an inline comment stating which format(s) intentionally lack the key and what standards-native construct supersedes it (e.g. "CDX-only — SPDX uses native `primaryPackagePurpose` for the same signal").
2. **Given** `docs/reference/sbom-format-mapping.md`, **When** searching for any parity row marked non-symmetric, **Then** that row appears in the doc with rationale and pointer to the standards-native replacement.
3. **Given** a future annotation that is intentionally CDX-only (e.g. a future CDX-1.7-only feature with no SPDX equivalent), **When** the engineer adds it to the parity catalog, **Then** they MUST set the directionality to a non-`SymmetricEqual` variant AND add a doc entry — the parity-check subcommand fails CI if the catalog and doc disagree.

---

### User Story 4 — Future annotations land in all three emitters together, not CDX-first (Priority: P2)

The root cause of the alpha.13 conformance gap is process: every annotation channel added in milestones 007–070 landed in CDX + SPDX 3 first, with SPDX 2.3 catching up later (sometimes much later). The user wants a guardrail so this drift cannot recur — adding a new annotation key to one emitter without the other two should fail CI before merge.

**Why this priority**: Preventive, not corrective. P2 because the CFI fix from US1 closes the *current* gap, but without US4 the next milestone reopens a fresh gap. Worth doing in the same window so the prevention lands together with the cure.

**Independent Test**: Add a synthetic test: introduce an annotation key in *one* emitter (CDX only) and run the parity-check subcommand. The subcommand MUST fail with a clear error naming the key, the format(s) missing it, and the parity-catalog row that needs updating. Implementing US4 means adding this guardrail check to the existing CI lanes.

**Acceptance Scenarios**:

1. **Given** a PR that adds `mikebom:foo-annotation` to the CDX emitter only (no SPDX 2.3 / SPDX 3 emitter changes, no parity-catalog row), **When** the PR runs the standard pre-PR gate, **Then** the gate fails with a message identifying the asymmetric emission and listing the formats missing the key.
2. **Given** the same PR but with a parity-catalog row added that explicitly marks `mikebom:foo-annotation` as `CdxOnly` plus a docs entry justifying the asymmetry, **When** the gate re-runs, **Then** it passes — the asymmetry is registered as intentional.
3. **Given** the alpha.13-era 6-key gap (`source-files`, `sbom-tier`, `cpe-candidates`, `deps-dev-match`, `npm-role`, `lifecycle-scope`), **When** US1 closes them, **Then** US4's guardrail prevents any of them from regressing in future PRs.

---

### Edge Cases

- **An annotation's *value* is structurally different across formats but semantically equivalent.** Example: CDX may carry `mikebom:source-files` as a JSON array property value, while SPDX 2.3 may need to carry it as a comma-joined string under an annotation comment because SPDX 2.3 annotation `comment` fields are scalar. The parity test MUST canonicalize before comparison — equivalent JSON values MUST match regardless of how the underlying format encoded them. Where canonicalization isn't possible (the formats genuinely cannot represent the same shape), the parity catalog row is downgraded to `PresenceOnly` directionality with explicit rationale.

- **A component appears in only some formats.** Already 46 of the alpha.13 CFI rows have this shape. Out of scope for parity-of-existing-components; tracked under a separate FALSE_POSITIVE / MISSING_COMPONENT investigation. The SymmetricEqual check only fires when the component IS present in all three formats but the annotation isn't.

- **Empty / null / absent: which one is the canonical "absent" representation?** The parity check MUST treat "key not emitted" as identical to "key emitted with empty value `[]` or `""`". Otherwise legitimate cases like "no source files known for this component" produce CFI flags depending on emitter implementation choice.

- **A single annotation key is intentionally redundant in one format.** E.g., if SPDX 3 carries the data in a native field AND mikebom adds the same data as a `mikebom:*` annotation in SPDX 2.3 (because 2.3 lacks the native field), should the parity catalog flag that as inherently asymmetric? Decision: yes — that's `CdxSubsetOfSpdx` / `SpdxOnly` territory, and the doc explains "SPDX 2.3 uses the annotation because it lacks the native construct present in SPDX 3 + CDX."

- **A key emitted before alpha.13 baseline that the catalog doesn't know about yet.** Discovery-pass: the parity-check subcommand MUST surface annotations it sees in any format that aren't in the catalog at all, so audits don't silently miss new keys. Per the 2026-05-04 clarification, any uncatalogued key is a **hard fail** — the pre-PR gate aborts and the engineer must add a catalog row (with directionality + rationale) before the PR can merge. There is no default-to-`SymmetricEqual` fallback and no soft-warn mode.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: For every `mikebom:*` annotation key emitted on a component-level construct in any of the three output formats (CycloneDX 1.6, SPDX 2.3, SPDX 3.0.1), mikebom MUST emit the same key on the same component in the other two formats — UNLESS that key is registered in the parity catalog with a non-`SymmetricEqual` directionality.
- **FR-002**: When a key is emitted across all three formats, the value payload MUST be byte-equivalent after applying the default JSON canonicalization rule: object keys sorted lexicographically, whitespace normalized, arrays sorted lexicographically. A catalog row MAY set `order_sensitive: true` to disable array sorting for that specific key when array order is semantic; this opt-out is rare and the catalog rationale must justify it. All currently-named keys (`mikebom:source-files`, `mikebom:cpe-candidates`, `mikebom:deps-dev-match`, `mikebom:npm-role`, `mikebom:sbom-tier`) are unordered and use the default rule.
- **FR-003**: The parity catalog (existing `parity/extractors/mod.rs` infrastructure) MUST list every `mikebom:*` annotation key currently emitted by any format. Discovery of an emitted key not in the catalog is itself a defect.
- **FR-004**: Every parity-catalog row whose directionality is non-`SymmetricEqual` MUST carry an inline comment in code stating (a) which format(s) intentionally lack the key and (b) what standards-native construct in the other format(s) makes the asymmetry semantically correct.
- **FR-005**: `docs/reference/sbom-format-mapping.md` MUST contain a section enumerating every non-symmetric parity row with the same rationale, so operators can read the published mapping without grepping source.
- **FR-006**: The pre-PR gate (`./scripts/pre-pr.sh`) MUST run a parity-equivalence check that fails when any `mikebom:*` annotation key is emitted in fewer than the three expected formats without a registered non-symmetric catalog row.
- **FR-007**: A canonicalization layer MUST exist for parity comparison so that legitimate format-specific encoding differences (e.g., CDX property arrays vs. SPDX scalar comment strings) do not produce false CFI flags when payloads are semantically equivalent.
- **FR-008**: The component-level CFI count produced by the published external conformance harness over the existing 36-fixture suite MUST drop by ≥95% from the alpha.13 component-level baseline (11,130 → ≤556) once this milestone ships. Document-level CFI rows are excluded from this measurement and tracked as a separate follow-on milestone.
- **FR-009**: The 6 specific annotation keys driving the bulk of the alpha.13 CFI gap (`mikebom:source-files`, `mikebom:sbom-tier`, `mikebom:cpe-candidates`, `mikebom:deps-dev-match`, `mikebom:npm-role`, `mikebom:lifecycle-scope`) MUST each end up either (a) emitted symmetrically in all three formats, or (b) explicitly marked as inherently asymmetric in the parity catalog with documentation. No silent middle ground.
- **FR-010**: Every milestone after this one MUST add new annotation keys to all three format emitters in the same merge. The pre-PR gate (FR-006) is the enforcement mechanism.
- **FR-011**: Inherent format asymmetries that already exist (`mikebom:lifecycle-scope` is the known case — CDX-only because SPDX has no `scope: "excluded"` equivalent) MUST be re-audited and confirmed correctly classified. The audit deliverables are: (a) an inline `CatalogRowRationale` comment on the catalog row per FR-004, AND (b) a corresponding entry in `docs/reference/sbom-format-mapping.md` per FR-005. Re-audit completion is binary — every non-`SymmetricEqual` row either has both deliverables or the audit is incomplete.
- **FR-012**: When SPDX 2.3 cannot represent a value with the same shape as CDX / SPDX 3 (because its annotation comment field is a scalar string only), the parity layer MUST canonicalize at extraction time so the comparison is value-equivalent rather than shape-equivalent — OR the catalog row is explicitly downgraded to `PresenceOnly` with rationale.

### Key Entities

- **Annotation key**: A `mikebom:<name>` identifier for a piece of metadata mikebom adds to a component or document-level construct (e.g., `mikebom:source-files`, `mikebom:sbom-tier`). Has a value that may be a string, array, or object.
- **Parity catalog row**: A registry entry pairing one annotation key with its three per-format extractor functions and a directionality marker (`SymmetricEqual`, `PresenceOnly`, `CdxOnly`, `CdxSubsetOfSpdx`, etc.). The catalog is the source of truth for what cross-format equivalence is expected.
- **Directionality**: The constraint shape between the three formats for a given key. `SymmetricEqual` (default) means all three must carry the key with the same value. `PresenceOnly` means all three carry the key but values may legitimately differ in shape. `CdxOnly` / `SpdxOnly` means the key is intentionally one-format-only because the other format(s) have a native standards construct that supersedes it.
- **Inherent asymmetry**: A directionality choice driven by format-spec differences (one spec has a native field the other lacks). MUST be documented with the standards-native superseding construct named explicitly.
- **CFI finding**: A CROSS_FORMAT_INEQUIVALENCE row produced by the external conformance harness when it detects mikebom's three formats disagree on an annotation. The headline metric this milestone reduces.
- **Format**: One of CycloneDX 1.6, SPDX 2.3, SPDX 3.0.1 — mikebom's three supported emitter targets.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: External conformance harness component-level CFI finding count drops by ≥95% (11,130 → ≤556) when run against the post-fix build over the existing 36-fixture suite. Document-level CFI rows are reported separately and not counted toward this number.
- **SC-002**: External conformance harness *total* finding count (all kinds, component-level + document-level + non-CFI buckets) drops by ≥85% (12,165 → ≤1,800) — the headline number now reflects real defects, not parity artifacts.
- **SC-003**: Of the 6 specific annotation keys catalogued as the bulk drivers, 100% are resolved either by symmetric emission across all three formats or by an explicit non-symmetric catalog row with documented rationale.
- **SC-004**: A synthetic regression test (introduce a one-format-only annotation in a draft commit, run pre-PR gate) fails with a clear error message identifying the asymmetric key, the missing format(s), and the parity-catalog row that needs updating.
- **SC-005**: 100% of the parity-catalog rows whose directionality is non-`SymmetricEqual` carry both an inline code-comment rationale and a corresponding entry in `docs/reference/sbom-format-mapping.md`.
- **SC-006**: A consumer reading any one of mikebom's three SBOM formats receives the same set of `mikebom:*` annotation keys per component as a consumer reading either of the other two, except for keys explicitly registered as inherently format-specific.
- **SC-007**: Every milestone after this one passes the pre-PR gate's parity-equivalence check on first run for any new annotation key it introduces.

## Assumptions

- The existing parity-extractor infrastructure under `mikebom-cli/src/parity/extractors/` is the right substrate for both the equivalence check (FR-006) and the catalog-of-rationale layer (FR-004 / FR-005). It already declares 68 parity rows with directionality markers; this milestone extends rather than replaces it.
- The 6 alpha.13-era CFI-driving keys are correctly enumerated in the user input. New keys discovered during implementation (FR-003 discovery pass) will be folded into the same fix scope rather than deferred — discovery is part of US1.
- The 84% / 13% / 2% breakdown (SPDX 2.3 missing / value-not-equal / CDX-only-when-not-intended) accurately characterizes the gap shape. Implementation may discover the value-not-equal bucket needs format-specific canonicalization rules beyond simple sorted-keys JSON canonicalization; that's in scope.
- The 36-fixture suite is the correct measurement substrate for SC-001 and SC-002. If new fixtures are added during this milestone window, the baseline-vs-post-fix delta is measured on the *intersection* — fixtures present in both the alpha.13 run and the post-fix run.
- Inherent asymmetries are rare. The lifecycle-scope row is the one known case; the audit (FR-011) may surface 1–3 more, not dozens. If the audit surfaces more than ~5, that's a flag the directionality model itself needs review and would be a follow-up milestone.
- The pre-PR gate enhancement (FR-006, FR-010) runs in <30s even on full-workspace test runs — it's a static analysis over emitted-vs-catalogued keys, not a fixture-running check.
- "Equivalent value after canonicalization" is achievable with deterministic rules per FR-007. If a key surfaces where this isn't true, that key gets downgraded to `PresenceOnly` directionality and noted as a known limitation rather than blocking the milestone.

## Out of Scope

- The 951 FALSE_POSITIVE findings — separate root cause (ground-truth staleness in the conformance fixtures, or default-flip from alpha.10's Dev/Build/Test scope emission). Tracked separately; some belong on the conformance-fixture maintainer's side, not mikebom's.
- The 17 PURL_MISMATCH + 14 VERSION_MISMATCH findings — all in the `base-image-layers` fixture, root-caused to nested `node_modules/` matcher pairing. Matcher-side semantic, not a mikebom emission issue. Tracked separately.
- The 14 MISSING_COMPONENT findings — mostly fixture-side TODOs (placeholder versions in ground truth, alpha.13 main-module emission shape changes the GT predates). Tracked separately as ground-truth refresh.
- The 38 ANNOTATION_MISSING findings — these are the *opposite* direction (GT expected an annotation mikebom didn't emit). Most are advisory-tier per Constitution Principle V; the sbom-tier (3) and binary-class (3) blocking-tier ones are real coverage gaps tracked separately.
- Adding *new* mikebom annotation channels. This milestone closes the parity gap for keys mikebom *already* emits. New keys per future milestones will use the FR-006 / FR-010 guardrail to land in all three formats together from day one.
- Format-version migrations (CDX 1.7 prep, SPDX 3.1 prep). Same three target versions as alpha.13: CDX 1.6, SPDX 2.3, SPDX 3.0.1.
- Document-level annotations (the `metadata.properties` array on CDX, the document-level annotations on SPDX). Initial scope is component-level only — that's where the 11,130 CFI rows in the SC-001 measurement live. Per the 2026-05-04 clarification, both the baseline and post-fix targets are computed over component-level CFI only; document-level rows are reported separately by the harness and excluded from SC-001's numerator. Document-level parity is a follow-on milestone if a similar gap emerges there.
- Modifying the external conformance harness or its fixture set to reduce the headline number — that would be measurement gaming. The fix is on the mikebom side, measured by an *unchanged* harness over the *same* fixtures.
