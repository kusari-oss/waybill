# Feature Specification: SBOM consumer-facing reading guide — documenting mikebom annotations and differentiators

**Feature Branch**: `150-sbom-consumer-guide`
**Created**: 2026-06-29
**Status**: Draft
**Input**: User description: "milestone 150: SBOM consumer-facing reading guide documenting mikebom annotations and differentiators (where to find non-standard signals, what they mean, how to interpret them across CDX/SPDX 2.3/SPDX 3)"

## Origin & Context

The mikebom project has accumulated ~98 `mikebom:*` annotations across 50+ milestones (C-rows C1 through C102 as of milestone 149). Every annotation is rigorously documented in `docs/reference/sbom-format-mapping.md` — the **emitter-side correctness contract** that the format serializers honor — but that document is structured for mikebom CODE reviewers (one giant 98-row table; one-line justifications scoped to "why this annotation exists at all" rather than "what should an SBOM consumer do with it"). It is dense, exhaustive, and intentionally written as a spec, not a guide.

The consumer-facing surface is incomplete. Today an external user (a compliance engineer, a vulnerability scanner author, an SBOM-diff tool maintainer, an auditor walking an emitted SBOM) who picks up a mikebom-emitted CycloneDX 1.6 or SPDX 2.3 or SPDX 3 document can find:

- ✅ Per-flag CLI docs in `docs/user-guide/cli-reference.md` (emit side)
- ✅ Narrow topical reference docs (`identifiers.md`, `sbom-types.md`, `component-tiers.md`, `cross-tier-binding.md`) — each covers ONE conceptual area in depth
- ✅ The catalog-style `sbom-format-mapping.md` — comprehensive but built for code-review depth
- ❌ **NO** single doc that asks "what does mikebom emit beyond the SBOM standards' minimums, and how do you USE that data as a consumer?"

The result: consumers either (a) reverse-engineer mikebom's value-adds by reading mikebom source, (b) miss them entirely and treat mikebom output as carrying only standard-baseline signals, or (c) walk the 98-row C-catalog row-by-row trying to assemble a mental model from one-line justifications scoped at "why this annotation exists" rather than "what should I do with it."

This milestone closes the gap by publishing a new consumer-facing reference doc at `docs/reference/reading-a-mikebom-sbom.md` that:

1. **Frames mikebom's positioning** — strict spec conformance first; `mikebom:*` annotations only when no native field exists (Constitution Principle V).
2. **Highlights the data mikebom makes available** — the signals mikebom adds beyond the SBOM standards' minimum content, organized by consumer use case (vulnerability scanning / compliance auditing / build provenance / transparency). Per the 2026-06-29 clarification, the doc does NOT name specific competing SBOM tools — the framing is "here's what mikebom emits and how to use it", not "here's what mikebom emits that other tools don't".
3. **For each non-standard signal**: explains what it means, where to find it in CDX 1.6 vs SPDX 2.3 vs SPDX 3 (with `jq` recipes), and what a consumer should do with it.
4. **Cross-references** `sbom-format-mapping.md` for wire-shape depth when readers want the catalog-level detail.
5. **Documents the `mikebom-annotation/v1` envelope shape** for SPDX 2.3 + SPDX 3 readers (today the envelope schema is recoverable from `mikebom-cli/src/parity/extractors/common.rs` and the per-spec contract files, but not from a single consumer-facing doc).
6. **Provides an appendix index** mapping every `mikebom:*` annotation key to its row in `sbom-format-mapping.md`, so a consumer encountering an unknown key in a real SBOM can look it up in seconds.

The new doc complements (not replaces) the existing `sbom-format-mapping.md` catalog — both files have distinct audiences. The catalog stays the canonical wire-shape contract; this guide is the consumer-onboarding surface.

## Clarifications

### Session 2026-06-29

- Q: How exhaustive should the cross-tool comparison be (syft + trivy + cdxgen + snyk + anchore; subset; or none)? → A: Option D — don't name specific competing SBOM tools at all. Frame the doc around what mikebom MAKES AVAILABLE, not what others lack. Removes the cross-tool verification burden and keeps the focus on mikebom's emitted data as a self-contained reference. FR-010 + SC-006 + Assumption 6 updated to drop tool-naming; references throughout the spec rewritten to remove competitor mentions.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Compliance engineer reads a mikebom SBOM for the first time (Priority: P1)

A compliance engineer at a downstream consumer organization receives a mikebom-emitted SPDX 2.3 SBOM for a vendor product. They want to quickly understand: (a) what's in this SBOM beyond the standard CDX/SPDX baseline they're used to, (b) which fields they can safely ignore vs which they should pay attention to, (c) what `mikebom:*` annotations actually mean.

They open `docs/reference/reading-a-mikebom-sbom.md`, see a "Signals mikebom makes available" section organized by consumer use case (vulnerability scanning / compliance auditing / build provenance / transparency), find the one relevant to their license-audit workflow (e.g., `mikebom:license-concluded-source`), read what it means, copy the `jq` recipe for finding it in SPDX 2.3, and complete their audit task without reading mikebom source or other docs.

**Why this priority**: this is the singular value-add of the milestone. Today's docs require the consumer to be a mikebom code reader OR a 98-row-catalog reader; both are barriers to adoption. Closing this gap is the only purpose of this milestone. The framing per the 2026-06-29 clarification focuses on what mikebom emits (consumer-centric "here's what's available and how to use it"), not on comparing mikebom against named competing tools.

**Independent Test**: Have a person (or via documentation review) walk through the guide for the first time, find at least 3 signals relevant to their use case, and execute the `jq` recipes against a sample mikebom-emitted SBOM successfully without consulting any other doc.

**Acceptance Scenarios**:

1. **Given** an external consumer opens the new doc, **When** they read the opening section, **Then** they understand mikebom's positioning vs other SBOM tools within 3 minutes (mikebom strict-conforms to CDX/SPDX, `mikebom:*` annotations are parity-bridges, the doc tells them what each parity-bridge means).

2. **Given** a consumer needs to find dev/test/build-scoped dependencies in a mikebom SPDX 2.3 SBOM, **When** they search the guide for "lifecycle-scope" or "test deps" or "dev deps", **Then** they find a section explaining the dual-carrier model (native SPDX 2.3 `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` typed relationships AND the `mikebom:lifecycle-scope` annotation on the target Package) with a `jq` recipe that returns the filtered set.

3. **Given** a consumer encounters an unfamiliar `mikebom:*` annotation in an SBOM, **When** they search the guide's appendix index for the key, **Then** they find a one-line description + a link to the corresponding C-row in `sbom-format-mapping.md` for wire-shape depth.

4. **Given** a consumer wants to understand mikebom's build-trace provenance (the eBPF observation model), **When** they read the guide, **Then** they find a "Build provenance" section explaining `mikebom:source-type`, the doc-level `generation_context` signal, and how to filter components by trace-observed vs lockfile-enriched.

5. **Given** a consumer wants to identify "phantom" components (declared in a manifest but not actually built), **When** they read the guide, **Then** they find the `mikebom:source-type` mapping (e.g., `"trace-observed"` vs `"declared-not-cached"`) with a `jq` recipe that surfaces declared-only entries.

---

### User Story 2 - Vulnerability scanner author integrates mikebom-specific signals (Priority: P2)

A vulnerability-scanner maintainer (or any consumer-side tool author building on mikebom output) wants to extend their tool to leverage mikebom-specific signals — e.g., suppress dev-only deps from production-vulnerability alerts using `mikebom:lifecycle-scope`, or correlate divergent-PURL collisions surfaced by `mikebom:duplicate-purl-divergent`. They need machine-readable details on:

- Annotation key names + value shapes (boolean string vs JSON-encoded array vs envelope-wrapped struct)
- Which carrier each annotation rides in per format (CDX `properties[]` vs SPDX 2.3 envelope vs SPDX 3 envelope on a graph-element Annotation)
- The `mikebom-annotation/v1` envelope schema for SPDX 2.3 + SPDX 3 readers
- Stability guarantees (which signals are stable vs experimental, what changes when)

**Why this priority**: enables third-party tool authors to build on mikebom's value-adds without reading mikebom source. Wider ecosystem leverage of mikebom's differentiators.

**Independent Test**: A tool author reads the "for tool authors" section, codifies a filter or correlation rule using ONE mikebom-specific signal, and produces correct results against a real mikebom SBOM without consulting mikebom source.

**Acceptance Scenarios**:

1. **Given** a tool author needs the `mikebom-annotation/v1` envelope JSON schema, **When** they search the guide, **Then** they find the envelope shape (fields `schema`, `field`, `value`) with at least one example per format AND a link to the canonical schema file location.

2. **Given** a tool author needs to know which signals are stable vs experimental, **When** they read the guide, **Then** they find a stability statement (e.g., "all `C*` rows in the catalog are stable wire shapes; the catalog row number is the durable identifier") and a callout for any opt-in or experimental flags (`--file-inventory=full`, `--preserve-manifest-main-module`, etc.).

3. **Given** a tool author wants to write a CDX-to-SPDX-3 normalizer that preserves mikebom value-adds, **When** they read the "Cross-format reading patterns" section, **Then** they find a per-signal carrier-mapping table (or pointer to `sbom-format-mapping.md`'s catalog) showing where the same signal lives in each format.

---

### Edge Cases

- **A consumer reads the guide but their use case is NOT one of the documented thematic clusters**: the guide includes a "find by annotation key" appendix so the consumer can search for the specific `mikebom:*` key they encountered in an SBOM and get a one-line description + link to wire-shape details.

- **An annotation is added to mikebom in a future milestone after the guide ships**: the guide MUST link to `sbom-format-mapping.md` as the source-of-truth catalog so consumers reading a future-mikebom SBOM with unknown annotations have a single canonical lookup destination. The guide's appendix index need not be exhaustively maintained at every annotation addition — only the catalog is mandatory to update.

- **A consumer reads the guide and disagrees with mikebom's emission choice** (e.g., wants to suppress a `mikebom:*` annotation, or wants the demoted-from-main-module entry's outbound edges preserved): the guide MUST point at the relevant FILED ISSUE or accept-future-issue path (Constitution Principle V audit + `mikebom:*` annotation governance) rather than imply emission choices are non-negotiable.

- **A consumer encounters a `mikebom:*` annotation NOT in the guide's appendix index** (because the guide is published at milestone-N but the annotation was added at milestone-N+M): they should still find the annotation in `sbom-format-mapping.md`'s catalog. The guide's appendix is best-effort current; the catalog is the canonical mandatory reference.

- **A consumer wants to use the guide as a one-stop reference and does NOT want to drill into `sbom-format-mapping.md`**: the guide MUST be self-contained for the top ~10-15 differentiator signals — each gets a full explanation in the guide. Drilling into the catalog is for wire-shape edge cases and for the long tail of annotations.

- **A consumer wants to verify the doc is current with the shipped binary**: the guide MUST cite specific milestone numbers (e.g., "added in milestone 134", "stabilized in milestone 145") so consumers comparing against a specific mikebom binary version can confirm coverage. mikebom versions correspond to the `v*-alpha.*` tag sequence; the guide's signal-by-signal milestone citations let consumers map binary-version → signal availability.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: mikebom MUST publish a new consumer-facing reference doc at `docs/reference/reading-a-mikebom-sbom.md` covering mikebom's value-add signals (annotations + native-field usage patterns that differ from other SBOM tools).

- **FR-002**: The doc MUST open with a positioning section explaining: (a) mikebom strictly conforms to CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 — most data lives in spec-native fields; (b) `mikebom:*` annotations are parity-bridges introduced per Constitution Principle V when no native field carries a particular signal; (c) the doc's job is to tell consumers what the parity-bridges (and a few other notable native-field-usage patterns) mean.

- **FR-003**: The doc MUST organize differentiator signals into thematic clusters aligned with common consumer use cases — minimum: vulnerability scanning, compliance auditing, build provenance, transparency / completeness gaps. Each cluster MUST include at least 2 specific mikebom signals with worked examples.

- **FR-004**: For each documented signal, the doc MUST provide: (a) plain-language description of what it means; (b) where to find it in CDX 1.6, SPDX 2.3, and SPDX 3 (with the per-format carrier shape); (c) a working `jq` recipe that returns the signal value or filters by it; (d) what a consumer should do with the signal (action-oriented guidance).

- **FR-005**: The doc MUST document the `mikebom-annotation/v1` envelope shape (fields `schema`, `field`, `value`) with at least one example per format (CDX `properties[]` value, SPDX 2.3 annotation `comment`, SPDX 3 annotation `statement`). The doc MAY link to the canonical envelope-schema file at `mikebom-cli/src/parity/extractors/common.rs` (or wherever the envelope is defined) rather than re-state the schema in full.

- **FR-006**: The doc MUST include an appendix index that maps every `mikebom:*` annotation key to its corresponding C-row in `sbom-format-mapping.md`. The index MUST be a flat alphabetical lookup table; entries MUST link directly to the C-row anchor in the catalog. The index covers all annotation keys present at the time of milestone-150 ship (a snapshot — future annotations land in the catalog, not in this guide's appendix; the guide's catalog link covers them).

- **FR-007**: The doc MUST cross-reference `docs/reference/sbom-format-mapping.md` as the canonical wire-shape catalog whenever it discusses a specific signal — readers who want full per-format wire-shape detail follow the link. The guide is the consumer-onboarding surface; the catalog is the contract.

- **FR-008**: The doc MUST cross-reference the narrow topical refs (`identifiers.md`, `sbom-types.md`, `component-tiers.md`, `cross-tier-binding.md`) when discussing signals covered in depth elsewhere. The guide names the signal + provides a one-paragraph summary + links out for depth.

- **FR-009**: The doc MUST be linked from `docs/index.md` in the **Reference material** section, listed alongside the existing reference docs (`identifiers.md`, `sbom-types.md`, etc.), with a one-line description identifying it as the consumer-onboarding surface.

- **FR-010**: The doc MUST frame its content around what mikebom EMITS rather than what other SBOM tools omit. Per the 2026-06-29 clarification (Q1 Option D), the doc MUST NOT name specific competing SBOM tools (no "syft does X but mikebom does Y" passages). The framing instead is "here's what mikebom makes available and how to use it" — consumer-centric, comparison-tool-agnostic. The doc MAY refer in passing to "standard SBOM tool output" or "the CDX/SPDX spec baseline" as shorthand for the reference behavior; it MUST NOT cite specific tool names or behaviors.

- **FR-011**: Every `jq` recipe in the doc MUST be runnable against a real mikebom-emitted SBOM (verified at doc-authoring time) and MUST produce the documented output. The recipes are illustrative — they need not cover every variant — but each MUST be correct as written.

- **FR-012**: The doc MUST include a "Stability" section covering: (a) every `C*` row in `sbom-format-mapping.md` is a stable wire shape — its row number is the durable identifier; (b) the `mikebom-annotation/v1` envelope shape is stable; (c) any opt-in or experimental flags that affect emission are called out (e.g., `--file-inventory=full`, `--preserve-manifest-main-module`).

- **FR-013**: The doc MUST cite specific milestone numbers for each documented signal (e.g., "added in milestone 134", "stabilized in milestone 145") so consumers can map a mikebom binary version to signal availability.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A reviewer (operator-cadence — not a CI gate) reads the new doc end-to-end and confirms they can answer the following questions WITHOUT consulting `sbom-format-mapping.md` or any other doc: "What does `mikebom:lifecycle-scope` mean?", "How do I find dev-only dependencies in a mikebom SPDX 2.3 SBOM?", "Where do I find which OCI layer a binary came from?", "What's the difference between `mikebom:source-type = trace-observed` and `mikebom:source-type = declared-not-cached`?", "What's the `mikebom-annotation/v1` envelope shape?".

- **SC-002**: The doc's appendix index lists every `mikebom:*` annotation key present in `sbom-format-mapping.md` at milestone-150 ship time. A test or audit can verify by comparing the index keys against the catalog's row labels.

- **SC-003**: The doc is reachable from `docs/index.md`'s "Reference material" section with a one-line description.

- **SC-004**: At least 5 `jq` recipes in the doc are verified runnable against a real mikebom-emitted SBOM at doc-authoring time. The recipes' expected outputs match the doc's claimed outputs.

- **SC-005**: At least 4 distinct thematic clusters are present in the doc (vulnerability scanning / compliance auditing / build provenance / transparency / completeness gaps), each with at least 2 documented signals.

- **SC-006**: The doc covers at least 8 distinct mikebom-emitted signals in depth (full per-format wire-shape + plain-language meaning + `jq` recipe + action-oriented consumer guidance per FR-004). Per the 2026-06-29 clarification (Q1 Option D), this metric replaces the original cross-tool comparison count — the doc focuses on signal coverage breadth rather than competitor-tool naming. An audit/reviewer can count documented signals to verify.

- **SC-007**: The pre-PR gate (`./scripts/pre-pr.sh`) passes — clippy clean + `cargo test --workspace` clean (excepting the pre-existing `sbomqs_parity` env-only failure). Since this milestone is docs-only, the gate is essentially a no-op (no Rust source code change) but the convention is followed.

- **SC-008**: A reverse-link test or audit confirms `docs/reference/sbom-format-mapping.md` is linked from the new doc at least once.

## Assumptions

1. **Docs-only milestone**: this milestone touches only Markdown files under `docs/`. No Rust source code changes, no CLI flag changes, no parity-catalog row additions, no test changes. The new doc is a published reference, not an emitted artifact.

2. **No emitter-behavior change**: the wire format mikebom emits is unchanged. Consumers reading SBOMs against the existing mikebom binary (alpha.52 + this milestone's docs) see the same wire bytes as alpha.52 alone.

3. **Catalog stays canonical**: `sbom-format-mapping.md` remains the source-of-truth wire-shape catalog. The new doc cross-references it; the new doc does NOT duplicate per-row wire-shape detail except for the top ~10-15 differentiator signals that get full inline coverage.

4. **Appendix index is a snapshot**: the appendix maps every annotation key present at milestone-150 ship time. Future annotations (milestones N+M for N > 150) land in the catalog only; the guide's appendix is best-effort current but the catalog is the canonical mandatory reference for new signals.

5. **`jq` recipes are illustrative**: the recipes cover the common case but not every variant. They MUST be correct as written; they need not be exhaustive.

6. **No cross-tool comparison** (2026-06-29 clarification Q1 Option D): the doc does NOT name specific competing SBOM tools. Framing is consumer-centric ("here's what mikebom emits and how to use it"), not competitive ("here's what mikebom does that others don't"). Removes the verification burden of pinning specific tool versions / dates / license tiers, and keeps the doc evergreen against external-tool behavioral drift.

7. **No new schema files**: the `mikebom-annotation/v1` envelope schema is already defined in mikebom code (`mikebom-cli/src/parity/extractors/common.rs` or similar canonical location). The new doc links to the existing canonical schema rather than duplicating it.

8. **One file, one doc**: the deliverable is a SINGLE Markdown file at `docs/reference/reading-a-mikebom-sbom.md`. It MAY be long (the existing `sbom-format-mapping.md` is ~200 lines but ships as one file). A single file keeps the doc easy to grep + bookmark; multi-file splits add navigation friction.

9. **Operator-cadence quality review**: the doc's quality is assessed via an operator-cadence read-through (per SC-001), not via automated tests. The doc cannot fail in CI; it can only fail an operator's "I read it and got my answer" check.

## Out of Scope

1. **Changes to `sbom-format-mapping.md`'s structure or content**. The existing catalog stays as-is. The new doc complements it.

2. **Changes to the wire format mikebom emits**. Purely documentation.

3. **New `mikebom:*` annotations**. No new annotations introduced — the doc covers what's already there as of milestone-150 ship.

4. **Auto-generation of the appendix index from the catalog**. The index is hand-maintained at ship time (SC-002 audit confirms coverage). A future milestone could automate via a build-time script reading both files; out of scope here.

5. **Translations or multi-language versions**. English-only.

6. **Per-annotation deep-dive subdocs** (e.g., a separate file per `mikebom:*` key). Single-file deliverable per Assumption 8.

7. **Interactive consumer tooling** (e.g., a web tool that parses an uploaded SBOM and explains mikebom signals). Static Markdown only.

8. **Comparison-tool implementations**. The doc names other tools' behaviors descriptively; it does NOT include a benchmark suite or comparison-CI harness.

9. **Coverage of every `C*` row in the catalog**. The guide covers the top ~10-15 differentiator signals in depth + the appendix lists every key. The long-tail catalog rows stay in the catalog.

10. **Per-format SBOM viewers / parsers / linters**. Reference material only; no tooling.

## Key Entities

- **Consumer-guide document** (`docs/reference/reading-a-mikebom-sbom.md`): the new single-file reference. Sections: positioning, differentiator clusters by use case, envelope schema, stability statement, cross-tool comparison, milestone-citation pattern, appendix index. Cross-references the existing catalog + narrow topical refs.

- **Appendix index entries**: each is a row `{annotation-key, one-line-description, link-to-C-row-in-catalog}`. Snapshot at milestone-150 ship; future entries land in catalog only.

- **Cited milestones**: each documented signal cites the milestone that introduced or stabilized it. Format: `(milestone N — verb)`, e.g., `(milestone 134 — added)`, `(milestone 145 — stabilized envelope shape)`.

- **Documented `mikebom:*` annotation set**: the union of all annotations covered in depth (the top ~10-15) PLUS the appendix index (all keys at ship time). The depth-covered subset is a curated choice; the appendix-listed subset is automatically determined from `sbom-format-mapping.md` snapshot at ship time.
