# Feature Specification: preserve manifest-derived main-module as demoted library entry when `--root-name` overrides it

**Feature Branch**: `149-demote-manifest-mainmod`
**Created**: 2026-06-29
**Status**: Draft
**Input**: User description: "issue 151 — preserve manifest-derived main-module as demoted library entry when --root-name overrides it (milestone 077 follow-up)"

## Origin & Context

Issue [#151](https://github.com/kusari-oss/mikebom/issues/151) tracks a follow-up to milestone 077 (`--root-name` / `--root-version` root-component override flags). The milestone-077 spec deliberately implemented a **clean-replacement** semantic: when the operator passes `--root-name widget-svc --root-version 1.2.3` to a manifest-driven scan (Cargo / npm / pip / gem / Maven / Go from milestones 064–070), the manifest-derived main-module identity (e.g., `pkg:cargo/foo-internal@0.5.1`) is *dropped entirely*. The root component's `name` / `version` / `bom-ref` / `purl` / `cpe` all derive exclusively from the operator override; the original manifest identity disappears from the SBOM.

The clean-replacement default optimizes for operator clarity: "I told mikebom to call this widget-svc; the SBOM says widget-svc, nothing else." But it has a cost — compliance auditors lose visibility into what the project's own manifest declared itself to be. For shipped binaries the operator-meaningful identity is correct at the root, but the internal-manifest identity is also useful provenance (it carries the original ecosystem PURL, license declarations, hashes, etc.) and dropping it represents an information loss.

This milestone adds an **opt-in** mechanism to preserve the manifest-derived main-module as a regular library entry in `components[]` when the operator overrides the root. The operator's identity remains the root component; the manifest's identity rides as a sibling library component with its full ecosystem-derived metadata intact. Compliance tooling that grouped by PURL still finds the manifest entry; tooling that read the root component still sees the operator's chosen identity.

### Concrete before / after (Cargo example)

Cargo project with `[package].name = "foo-internal"`, `version = "0.5.1"`. Operator runs:

```bash
mikebom sbom scan --path . --root-name widget-svc --root-version 1.2.3 \
                  --preserve-manifest-main-module
```

**Today (post-077, clean replacement):**

```json
{
  "metadata": {
    "component": {
      "name": "widget-svc", "version": "1.2.3",
      "bom-ref": "widget-svc@1.2.3",
      "purl":    "pkg:generic/widget-svc@1.2.3",
      "type":    "application"
    }
  },
  "components": [ /* deps only — no foo-internal entry */ ]
}
```

**Post-149 with `--preserve-manifest-main-module` opt-in:**

```json
{
  "metadata": {
    "component": {
      "name": "widget-svc", "version": "1.2.3",
      "bom-ref": "widget-svc@1.2.3",
      "purl":    "pkg:generic/widget-svc@1.2.3",
      "type":    "application"
    }
  },
  "components": [
    {
      "name": "foo-internal", "version": "0.5.1",
      "bom-ref": "pkg:cargo/foo-internal@0.5.1",
      "purl":    "pkg:cargo/foo-internal@0.5.1",
      "type":    "library",
      "properties": [
        { "name": "mikebom:demoted-from-main-module", "value": "true" }
      ]
    },
    /* deps */
  ]
}
```

The `mikebom:demoted-from-main-module = "true"` annotation is the parity-bridge transparency signal that tells consumers "this library entry was preserved from the manifest-derived main-module after a `--root-name` override fired". Without it, downstream tooling can't distinguish a normal library dep from a demoted main-module.

## Clarifications

### Session 2026-06-29

- Q: When `--preserve-manifest-main-module` fires, where should the dependsOn edges to direct deps live — only on the operator-override root, only on the demoted manifest entry, or on both? → A: Option A — edges ONLY on the operator-override root (demoted entry has empty `dependsOn`); confirms spec Assumption 5.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Operator preserves manifest identity via opt-in flag (Priority: P1)

A compliance engineer maintains a shipped service `widget-svc` whose internal Cargo crate name is `foo-internal`. They want the SBOM to surface `widget-svc@1.2.3` as the operator-meaningful identity (matches the deployment artifact name) AND to preserve `pkg:cargo/foo-internal@0.5.1` as the manifest provenance (matches the source-tree identity, carries the original Cargo license declarations + dependency edges). They run `mikebom sbom scan --path . --root-name widget-svc --root-version 1.2.3 --preserve-manifest-main-module`. The resulting SBOM has `widget-svc@1.2.3` at the root AND `pkg:cargo/foo-internal@0.5.1` as a `library`-typed component in `components[]`. Both are queryable. The demoted entry carries a `mikebom:demoted-from-main-module = "true"` annotation so the compliance tool can identify its origin.

**Why this priority**: This is the singular value-add of the milestone — closes the manifest-identity-loss gap surfaced by milestone 077's clean-replacement default. Without it, operators who use the override flag forfeit the manifest's PURL + license + hash provenance entirely.

**Independent Test**: Scan a Cargo project (`[package].name = "foo-internal"`, `version = "0.5.1"`) with `--root-name widget-svc --root-version 1.2.3 --preserve-manifest-main-module`. Assert: (a) root component name = "widget-svc", version = "1.2.3"; (b) `components[]` contains an entry with name "foo-internal", version "0.5.1", purl `pkg:cargo/foo-internal@0.5.1`, type "library"; (c) that entry carries `mikebom:demoted-from-main-module = "true"` in its properties / annotations.

**Acceptance Scenarios**:

1. **Given** a Cargo project with `[package].name = "foo-internal"`, version `0.5.1`, **When** the operator runs `mikebom sbom scan --root-name widget-svc --root-version 1.2.3 --preserve-manifest-main-module`, **Then** the CDX output has `metadata.component = {name: "widget-svc", version: "1.2.3", type: "application"}` AND `components[]` contains an entry `{name: "foo-internal", version: "0.5.1", purl: "pkg:cargo/foo-internal@0.5.1", type: "library", properties: [{name: "mikebom:demoted-from-main-module", value: "true"}]}`.

2. **Given** the same project + same flags, **When** the operator emits SPDX 2.3, **Then** the document carries a top-level `Package` for `foo-internal@0.5.1` with `externalRefs[purl] = "pkg:cargo/foo-internal@0.5.1"`, an annotation envelope `{field: "mikebom:demoted-from-main-module", value: "true"}`, AND the original DESCRIBES relationship from the document points at the `widget-svc@1.2.3` operator-override root (not the demoted entry).

3. **Given** the same project + same flags, **When** the operator emits SPDX 3, **Then** the `software_Package` element for `foo-internal@0.5.1` exists in `@graph` with the equivalent annotation shape; the `software_Sbom.rootElement[]` points at the operator-override root.

4. **Given** an operator runs `mikebom sbom scan --root-name widget-svc --root-version 1.2.3` (NO `--preserve-manifest-main-module`), **When** the scan completes, **Then** the output is byte-identical to milestone 077's clean-replacement output — no `foo-internal` entry, no demoted annotation, no behavior change.

5. **Given** an operator runs `mikebom sbom scan --path .` without ANY override flags, **When** the scan completes, **Then** the output is byte-identical to pre-149 — the manifest-derived main-module IS the root (existing milestone 064–070 behavior), and no `mikebom:demoted-from-main-module` annotation is emitted.

---

### User Story 2 - Cross-ecosystem coverage (npm, pip, gem, Maven, Go) (Priority: P2)

The demote behavior MUST work uniformly across all six manifest-driven main-module ecosystems established by milestones 053 / 064 / 066 / 068 / 069 / 070. An npm project with `package.json` `name: foo-internal`, version `0.5.1`, scanned with the same `--preserve-manifest-main-module` flag, produces the equivalent shape: `pkg:npm/foo-internal@0.5.1` as a demoted library, root as the operator override. Same for pip (`pkg:pypi/`), gem (`pkg:gem/`), Maven (`pkg:maven/<group>/<artifact>`), Go (`pkg:golang/<module>`).

**Why this priority**: Operators don't always know which ecosystem reader fires; the behavior MUST be ecosystem-agnostic. Documenting cross-ecosystem coverage explicitly ensures the implementation lands the logic in the SHARED root-selection pipeline (`mikebom-cli/src/generate/root_selector.rs` or sibling), not in each per-ecosystem reader.

**Independent Test**: For each of (npm, pip, gem, Maven, Go), construct a minimal fixture where the manifest declares a main-module identity, scan with `--root-name X --root-version Y --preserve-manifest-main-module`, assert the demoted entry exists with the ecosystem's correct PURL prefix and carries the `mikebom:demoted-from-main-module` annotation.

**Acceptance Scenarios**:

1. **Given** an npm fixture with `package.json` `name: "foo-internal"`, version `0.5.1`, **When** the operator runs `mikebom sbom scan --root-name widget-svc --root-version 1.2.3 --preserve-manifest-main-module --format cyclonedx-json`, **Then** `components[]` contains an entry with `purl: "pkg:npm/foo-internal@0.5.1"`, `type: "library"`, and the `mikebom:demoted-from-main-module = "true"` annotation.

2. **Given** equivalent fixtures for pip / gem / Maven / Go, **When** scanned with the same flag pattern, **Then** each produces the ecosystem-appropriate PURL prefix on the demoted entry.

3. **Given** a no-manifest scan (e.g., a directory with no recognized ecosystem manifests), **When** scanned with `--root-name X --root-version Y --preserve-manifest-main-module`, **Then** no demoted entry is emitted — the manifest-main-module didn't exist to begin with, so there's nothing to demote. (The flag is a no-op in this case, not an error.)

---

### Edge Cases

- **Operator passes `--preserve-manifest-main-module` WITHOUT `--root-name` / `--root-version`**: The flag has no effect. The manifest-derived main-module remains the root component (existing milestones 053/064–070 behavior). Emit an INFO-level log message: `--preserve-manifest-main-module has no effect without --root-name override`.

- **Operator passes `--root-name` WITHOUT `--root-version`** (or vice versa): Inherits milestone 077's existing handling of partial overrides. The demote behavior still fires when `--preserve-manifest-main-module` is set; the demoted entry uses the manifest's full (name, version) regardless of which override field was set.

- **Operator passes `--root-purl` instead of `--root-name`**: Milestone 077 supports `--root-purl` as an alternative override. The `--preserve-manifest-main-module` flag MUST apply uniformly — demoting the manifest-derived main-module regardless of which override flag fired.

- **Multi-main-module scan** (e.g., Cargo workspace with N members per milestone 127): NONE of the workspace members get promoted to `metadata.component` (current placeholder-path behavior). The `--root-name` override would replace the placeholder; `--preserve-manifest-main-module` is a no-op because there's no SINGLE manifest-derived main-module to demote. Emit an INFO log: `--preserve-manifest-main-module skipped: multi-main-module scan (N modules detected)`.

- **Demoted entry has the same PURL as a transitive dependency that already exists in `components[]`** (theoretical): the demoted entry is a duplicate of an existing dep entry. Dedupe by PURL — keep ONE entry, annotate with `mikebom:demoted-from-main-module = "true"` (the demote-source signal wins). The deduplicator's existing `(ecosystem, name, version, parent_purl)` group-key should naturally handle this since both entries have `parent_purl = None`.

- **Operator passes BOTH `--preserve-manifest-main-module` AND `--no-root-purl`** (milestone 077's PURL-suppression flag): `--no-root-purl` suppresses ONLY the root component's PURL; the demoted library entry retains its manifest-derived PURL. Both flags coexist cleanly.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: mikebom MUST accept a new CLI flag `--preserve-manifest-main-module` (boolean, default `false`) that, when set together with `--root-name` and/or `--root-version` and/or `--root-purl`, preserves the manifest-derived main-module identity as a library-typed component in `components[]`.

- **FR-002**: The demoted component's PURL MUST be the manifest-derived PURL that would have appeared at `metadata.component.purl` had no override flag been set — preserving the ecosystem prefix (`pkg:cargo/`, `pkg:npm/`, `pkg:pypi/`, `pkg:gem/`, `pkg:maven/`, `pkg:golang/`).

- **FR-003**: The demoted component MUST be emitted with `type: "library"` (CDX 1.6) / equivalent SPDX 2.3 + SPDX 3 shapes, NOT `type: "application"` — the application role is now exclusively the operator override.

- **FR-004**: The demoted component MUST carry a `mikebom:demoted-from-main-module = "true"` annotation per Constitution Principle V parity-bridging audit (CDX `component.type = "library"` is the closest native signal but does NOT express "this was demoted by an override" semantic; no native field expresses this provenance across all three formats).

- **FR-005**: The demoted component MUST preserve the full manifest-derived metadata of the pre-override main-module: declared licenses, hashes (if any), supplier / maintainer, source-file paths. The ONLY differences vs the pre-override main-module are (a) component type changes from `application` → `library`, (b) the new annotation is added, (c) it appears in `components[]` instead of at `metadata.component`.

- **FR-006**: When `--preserve-manifest-main-module` is set WITHOUT any override flag (`--root-name` / `--root-version` / `--root-purl`), the flag MUST be a silent no-op (or with an INFO-level diagnostic per Edge Case 1). The manifest-derived main-module remains at `metadata.component`; no demoted entry appears.

- **FR-007**: When `--preserve-manifest-main-module` is NOT set, mikebom's behavior MUST be byte-identical to milestone 077 (and pre-149) — no demoted entry, no annotation, no log diagnostic. This guarantees backward compatibility for operators who already use the override flags without the new opt-in.

- **FR-008**: The demoted component MUST appear in `components[]` in a deterministic position — specifically, integrated into the existing deduplicator-driven `Vec<ResolvedComponent>` ordering. No reader-specific ordering tweak; the demote pass runs AFTER the existing `deduplicate` and `canonicalize_source_files_by_purl` (milestone 148) passes so the entry participates in normal post-dedup processing.

- **FR-009**: If the demoted component's PURL matches an existing entry in `components[]` (e.g., a transitive dep that happens to share the manifest identity), the deduplicator's existing `(ecosystem, name, version, parent_purl)` group-key MUST collapse them into ONE entry. The surviving entry MUST carry the `mikebom:demoted-from-main-module = "true"` annotation (the demote signal is informative; the dep-graph role is preserved).

- **FR-010**: The new annotation `mikebom:demoted-from-main-module` MUST be registered as a new row in the parity-catalog (`mikebom-cli/src/parity/extractors/`) with `Directionality::SymmetricEqual` (same boolean-string value across all three formats; cross-format byte-equality holds via the existing annotation envelope plumbing).

- **FR-011**: The new annotation MUST be documented at `docs/reference/sbom-format-mapping.md` with a Principle V audit trail naming the native-field alternatives that were rejected (CDX `component.type`, SPDX 2.3 typed-relationship enum, SPDX 3 `software_softwarePurpose`) and the parity-bridging justification.

- **FR-012**: The behavior MUST work uniformly across all six manifest-driven ecosystems established by milestones 053 (Go), 064 (Cargo), 066 (npm), 068 (pip), 069 (gem), 070 (Maven). No ecosystem-specific code paths.

- **FR-013**: Multi-main-module scans (per milestone 127 — Cargo workspace, polyglot) MUST treat `--preserve-manifest-main-module` as a no-op with an INFO-level diagnostic per Edge Case 4. The flag's semantic requires a SINGLE manifest-derived main-module to demote; multi-module scans don't promote any to root, so there's nothing to demote.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For each of the six manifest-driven ecosystems (Cargo / npm / pip / gem / Maven / Go), a fixture with a manifest-declared main-module identity scanned with `--root-name X --root-version Y --preserve-manifest-main-module` produces an SBOM where the manifest-derived identity appears as a library-typed entry in `components[]` with the `mikebom:demoted-from-main-module = "true"` annotation. All six ecosystem fixtures pass.

- **SC-002**: For each of the six manifest-driven ecosystems, scanning with `--root-name X --root-version Y` (WITHOUT the new flag) produces output that is byte-identical to milestone 077's clean-replacement output. Regression coverage via existing milestone-077 byte-identity tests; the new behavior MUST NOT break the default.

- **SC-003**: For each of the six manifest-driven ecosystems, scanning WITHOUT any override flag produces output that is byte-identical to pre-149 — the manifest-derived main-module IS the root, no demoted entry, no annotation. Regression coverage via existing milestones-053/064-070 byte-identity tests; default-mode behavior MUST NOT change.

- **SC-004**: A new in-tree integration test (`mikebom-cli/tests/demote_manifest_mainmod_md149.rs` or similar) MUST exercise at least three ecosystems (Cargo + npm + Go is a representative trio) and assert the full FR-001–FR-005 contract for each: root has operator identity, components[] has demoted entry with manifest identity + library type + annotation.

- **SC-005**: The parity-catalog row for `mikebom:demoted-from-main-module` MUST be exercised by at least one byte-identity golden update (the chosen ecosystem fixture's CDX + SPDX 2.3 + SPDX 3 goldens refresh to include the new annotation when scanned with the new flag set). Cross-format byte-equivalence holds via the existing `Directionality::SymmetricEqual` invariant.

- **SC-006**: The full pre-PR gate (`./scripts/pre-pr.sh`) MUST pass — both `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` — except the documented pre-existing `sbomqs_parity` env-only failure per memory `feedback_prepr_gate_full_output.md` + milestone-144 T001 note.

- **SC-007**: Documentation updated: `docs/reference/sbom-format-mapping.md` gets the new parity-catalog row entry with the Principle V audit; `docs/reference/identifiers.md` (or the closest doc covering `--root-name` behavior) gets a new section describing `--preserve-manifest-main-module` + how it interacts with the milestone-077 override flags.

## Assumptions

1. **Opt-in default**: The new behavior is gated behind a CLI flag (`--preserve-manifest-main-module`) rather than enabled by default. Per FR-007 this preserves backward compatibility with milestone 077 — operators who already use the override flags without expecting a demoted entry continue to get the clean-replacement output. Switching to opt-in-default vs opt-out can be revisited in a separate milestone once operator usage patterns have stabilized.

2. **Annotation name**: `mikebom:demoted-from-main-module` (boolean string `"true"`). Mirrors the naming convention of existing transparency annotations (`mikebom:is-workspace-root`, `mikebom:component-tier`). Constitution V audit deferred to plan.md: no CDX 1.6 / SPDX 2.3 / SPDX 3 native field expresses "this library was demoted from main-module by an override flag" — `component.type = "library"` describes the role but not the demote provenance.

3. **No new `mikebom:*` annotation on the root override component**. The root's identity is fully described by the operator's chosen name/version. No transparency annotation needed there — milestone 077 already handles the override provenance via the existing `--root-name` flag's documented semantic.

4. **Demoted entry's `bom-ref` / SPDXID**: The demoted entry's `bom-ref` (CDX) / `SPDXID` (SPDX 2.3) / `spdxId` (SPDX 3) MUST be derived from its manifest PURL, NOT from any reference to the demote relationship. Consumers reading by PURL find the entry naturally; the demote provenance is only carried via the new annotation. (This is the same pattern milestone 077 uses for the operator-override root: the bom-ref reflects the new identity, not the old one.)

5. **Demoted entry's relationships**: The demoted entry has NO new outbound or inbound relationships introduced by the demote itself. If the manifest-derived main-module pre-override had `dependsOn` edges to its direct deps, those edges now belong to the OPERATOR-OVERRIDE root (consistent with milestone 077's clean-replacement semantic for the dep-graph topology). The demoted entry is a metadata-only preservation; its relationships are empty.

6. **No new Cargo dependencies**. The implementation uses existing types (`ResolvedComponent`, `Purl`, the milestone-127 root-selector pipeline). No new crates.

7. **The demote logic lives in the root-selector pipeline**, not in any ecosystem reader. Per FR-012 cross-ecosystem uniformity — placing it in the readers would scatter the logic across six modules.

8. **Operator-cadence verification is sufficient post-merge** for the cross-ecosystem coverage claim (SC-001). The in-tree SC-004 integration test (three ecosystems) is the CI-binding signal; full six-ecosystem manual verification can be operator-cadence.

## Out of Scope

1. **Changing milestone 077's clean-replacement default behavior.** The new flag is purely additive opt-in. Operators who don't set it see no behavior change. Switching the default is a separate spec decision.

2. **Demoting other component roles** (e.g., demoting a workspace member, a binary, a service). The scope is specifically the manifest-derived main-module that milestone 077 currently DROPS via override. Other demote semantics can be future milestones.

3. **A flag to suppress the `mikebom:demoted-from-main-module` annotation** (e.g., `--no-demote-annotation`). The annotation is always emitted when the demote fires; consumers who don't want it can filter at parse time. A suppression flag adds wire-shape variability without operator benefit.

4. **Changes to the existing `--root-name` / `--root-version` / `--root-purl` flag semantics from milestone 077**. Those stay verbatim. The new flag layers on top.

5. **Per-format wire-shape differences** for the demoted entry beyond the existing CDX `components[]` library / SPDX 2.3 `Package` / SPDX 3 `software_Package` conventions. No new emission path; the demoted entry rides through the existing per-format emitters' iteration over `components`.

6. **A round-trip "promote-back-to-main-module" inverse flag**. There's no use case for promoting a library entry back to root mid-scan; out of scope.

7. **Interaction with the milestone 134 divergent-PURL detection.** If the manifest-derived main-module's PURL collides with a divergent-PURL detection event, the existing collision-handling code path runs as-is. The demote annotation doesn't change collision semantics.

8. **A `mikebom:override-source = "operator-flag"` annotation on the ROOT override component** documenting that the root identity came from a CLI flag rather than from the manifest. That's a separate Principle V audit (CDX `metadata.component` already names the root; the override provenance is implicit). Future milestone if operator tooling needs it.

9. **Test coverage for ALL six ecosystems** in the CI-binding integration test (SC-004 only mandates a representative trio of Cargo + npm + Go). Operator-cadence coverage for the remaining three (pip / gem / Maven) per Assumption 8.

## Constitution V parity-bridging audit

This milestone introduces ONE new `mikebom:*` annotation: `mikebom:demoted-from-main-module` (boolean string).

| Format | Native field considered | Decision | Reason |
|---|---|---|---|
| CDX 1.6 | `component.type` enum (`application` / `library` / `framework` / `container` / `platform` / `operating-system` / `device` / `firmware` / `file` / `machine-learning-model` / `data` / `cryptographic-asset`) | REJECTED | The `library` value describes the component's ROLE in the dep graph, not the DEMOTE PROVENANCE. A library that was always a library and a library that was demoted from main-module are observationally identical on the `type` field alone. |
| CDX 1.6 | `component.scope` enum (`required` / `optional` / `excluded`) | REJECTED | Lifecycle-scope axis, orthogonal to demote-provenance. |
| SPDX 2.3 | `Package.primaryPackagePurpose` enum (`APPLICATION` / `FRAMEWORK` / `LIBRARY` / `CONTAINER` / `OPERATING-SYSTEM` / `DEVICE` / `FIRMWARE` / `SOURCE` / `ARCHIVE` / `FILE` / `INSTALL` / `OTHER`) | REJECTED | Same shape as CDX `component.type` — describes role, not demote provenance. |
| SPDX 2.3 | `Relationship[relationshipType]` (e.g., `DEPENDS_ON`, `DESCRIBES`, `VARIANT_OF`, `COPY_OF`) | REJECTED | None of the relationship types express "this library was the document's main-module before an override fired". `VARIANT_OF` is the closest but its semantic is about the SAME COMPONENT with variations (architecture, language), not about a role demote. |
| SPDX 3 | `software_softwarePurpose` enum | REJECTED | Same as SPDX 2.3 `primaryPackagePurpose`. |
| SPDX 3 | `LifecycleScopedRelationship.scope` enum | REJECTED | Lifecycle-scope axis, orthogonal. |

**Decision**: Introduce `mikebom:demoted-from-main-module = "true"` as a parity-bridging annotation. CDX → per-component `properties[]` entry; SPDX 2.3 → annotation envelope `{field: "mikebom:demoted-from-main-module", value: "true"}` on the demoted Package; SPDX 3 → same envelope shape on the `software_Package` element. The annotation rides through the existing emitter iteration over `extra_annotations` (no new emission path).

Documented in `docs/reference/sbom-format-mapping.md` per FR-011 with this audit trail.

Constitution Principle V is satisfied: the parity-bridging carve-out is justified by the absence of a native field expressing demote-provenance across all three formats.
