# Feature Specification: Subject identifier scheme + per-component user-defined identifiers

**Feature Branch**: `076-subject-component-ids`
**Created**: 2026-05-06
**Status**: Draft
**Input**: User description: "Add `subject:` as a fifth built-in identifier scheme so build SBOMs can declare their build-output hash as a first-class identifier in the SBOM body (auto-detected from the trace's subject set, or manually supplied via flag). Additionally, allow operators to attach user-defined identifiers (e.g., `kusari-id:asset-foo-prod-v2`) to specific components within an SBOM via a new `--component-id <PURL>=<scheme>:<value>` flag, so external tools can correlate components across SBOMs even when content hashes aren't available."

## Overview

Milestones 072–075 established document-level identifiers (`repo:`, `git:`, `image:`, `attestation:`) and cross-tier binding. The remaining gap in the cross-tier correlation story has two parts:

1. **Build SBOMs don't declare their build-output hash as a first-class identifier in the SBOM body.** The hash exists in the wrapping in-toto attestation envelope's `subjects[]`, but a consumer holding only the `.cdx.json` file (without the `.attestation.dsse.json` wrapper) can't walk from "binary X in this image" → "the SBOM whose subject is X." This breaks the content-addressable correlation pattern external SBOM-stores want to support.

2. **Operators can't attach their own per-component identifiers to specific components.** Today's user-defined `--id` flag attaches at the document level. When an operator wants to say "this specific binary in the image SBOM is also known as `kusari-id:asset-foo-prod-v2` in our internal asset DB," they have no first-class way to do it.

This milestone closes both gaps:

1. Adds `subject:<algo>:<hex>` as a fifth built-in identifier scheme. Auto-detected on build-tier scans from the trace's already-captured subject set (no manual flag needed for the common case); manually settable via `--subject-hash <algo>:<hex>` flag for source-tier and image-tier and overrides.
2. Adds a `--component-id <PURL>=<scheme>:<value>` repeatable flag that attaches user-defined identifiers to specific components within an SBOM. Identifiers ride standards-native per-component fields where they exist (CDX `components[].properties[]` or `externalReferences[]`, SPDX 2.3 `Package.externalRefs[PERSISTENT-ID]`, SPDX 3 `Element.externalIdentifier[]`).

Together, these complete the chain: external SBOM-stores can walk `image-SBOM.components[].hashes[].sha256 == X → SBOM whose subject:sha256:X matches → that build SBOM's git: commit → matching source SBOM` purely by string-match, with no mikebom-side resolver. And operators on internal-asset-tracking pipelines can layer their own per-component IDs on top without a separate annotation overlay.

## Clarifications

### Session 2026-05-06

- Q: When the trace's subject set carries multi-digest subjects (e.g., both sha256 and sha512 in a single subject's `digest` map), what should the auto-detect path emit? → A: sha256-only auto-emit. Drop other algos from auto-detection; operators who need sha512 (or other algos) pass `--subject-hash sha512:<hex>` manually. Subjects lacking a sha256 digest auto-emit nothing for that subject (info-log records the skip).

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Build-tier auto-detects `subject:` from trace output (Priority: P1)

A developer or CI runner invokes `mikebom trace run -- ./build.sh`. The build produces a binary at `target/release/myapp` with SHA-256 `X`. The emitted build SBOM's body carries a `subject:sha256:X` identifier alongside the existing `repo:` and `git:` identifiers (from milestone 074). No manual flag required — mikebom reads the same subject set the in-toto attestation envelope already captures.

**Why this priority**: This is the primary value-delivery for the milestone. Without auto-detect, the build-tier subject hash continues to live only in the attestation envelope and the cross-tier walk requires unwrapping the DSSE envelope to find it. Auto-detect makes the SBOM body self-sufficient for content-addressable correlation. P1 because the build-tier is the only tier where content-addressable subject hashes exist naturally and are auto-detectable; it's the tier where this milestone delivers concrete value.

**Independent Test**: Run `mikebom trace run -- /usr/bin/true` (a wrapped command that produces no real artifacts but the trace still records what was observed). Inspect the emitted SBOM's `metadata.component.externalReferences[type:provenance]` (or whichever native carrier the implementation uses) and verify that, when the trace captured a subject, a corresponding `subject:sha256:<hex>` identifier appears.

**Acceptance Scenarios**:

1. **Given** a build that produces one output binary at hash `X`, **When** `mikebom trace run -- ./build.sh` is invoked, **Then** the emitted build SBOM body contains a `subject:sha256:X` identifier; the value matches the hash in the attestation envelope's `subjects[]` byte-for-byte.
2. **Given** a multi-output build (e.g., `make all` produces 5 binaries with hashes X, Y, Z, W, V), **When** `mikebom trace run -- make all` is invoked, **Then** the emitted SBOM contains 5 `subject:` identifiers (one per output), in deterministic order.
3. **Given** a build that produces no output (e.g., `mikebom trace run -- echo hello` — wrapped command produced no observable artifact), **When** the SBOM is emitted, **Then** no `subject:` identifier is emitted (the slot is omitted; the SBOM body is otherwise unchanged).
4. **Given** the operator passes `--subject-hash sha256:Y` manually alongside an auto-detected `subject:sha256:X`, **When** the SBOM is emitted, **Then** both identifiers appear in the output (manual augments rather than overrides; `subject:` is multi-valued by design). If the manual value duplicates the auto-detected one, deduplication collapses them per milestone 073's `(scheme, value)` rule.

---

### User Story 2 — Source-tier and image-tier accept manual `subject:` (Priority: P2)

For source-tier scans (where the source tree isn't natively content-addressable) and image-tier scans (where the `image:` identifier already encodes the digest, but operators may want a redundant `subject:` for cross-tier consistency), the operator can pass `--subject-hash sha256:<hex>` on the command line. The flag is repeatable for multi-subject SBOMs.

**Why this priority**: Lower-priority because the use cases are narrower than US1: source-tier scans don't have an obvious "subject hash" (source trees are mutable), and image-tier scans already get `image:` (which encodes the digest natively). The manual flag exists for completeness and for operators with custom workflows that need `subject:` on these tiers.

**Independent Test**: Run `mikebom sbom scan --path . --subject-hash sha256:abc123... --output out.cdx.json` and verify `subject:sha256:abc123...` appears in the emitted identifier set.

**Acceptance Scenarios**:

1. **Given** a source-tier scan with `--subject-hash sha256:X`, **When** the SBOM is emitted, **Then** the `subject:sha256:X` identifier is present alongside the auto-detected `repo:` from milestone 073.
2. **Given** the operator passes `--subject-hash` repeatedly (e.g., `--subject-hash sha256:X --subject-hash sha256:Y`), **Then** both identifiers are emitted, in supply order.
3. **Given** the operator passes a malformed hash like `--subject-hash banana`, **Then** the value soft-fails through milestone 073's `UserDefined` path: emitted under the user-defined namespace with a `tracing::warn!`; the scan does not fail.

---

### User Story 3 — Cross-tier digest handshake by string match (Priority: P1)

An external SBOM-store consumer holds:
- `image.cdx.json` with `components[]` listing binaries, each carrying `hashes[].sha256`.
- `build-myapp.cdx.json` with a `subject:sha256:X` identifier (from US1).

Without invoking mikebom, the consumer correlates: for each image-SBOM component with `hashes[].sha256 == X`, find any SBOM in the store whose document-level identifier set contains `subject:sha256:X`. The match resolves to `build-myapp.cdx.json`. From there the consumer reads the build SBOM's `git:` identifier to find the matching source SBOM. Pure content-addressable string-match correlation across tiers.

**Why this priority**: This is the headline value the milestone delivers. Without it, the chain established by milestones 072–075 has a gap at the build-tier-to-image-tier hop. P1 because the entire previous arc (072+073+074+075) was about getting *to* this point.

**Independent Test**: Build a tempdir fixture: a "source" git repo, a "build" SBOM produced manually with a known `subject:sha256:X`, an "image" SBOM listing one component whose hash is `X`. Write a small Python or jq harness that performs the walk: extract image components' hashes, search the store for SBOMs whose `subject:` identifier matches, surface the chain. Assert the harness recovers the source/build/image SBOMs in the right order.

**Acceptance Scenarios**:

1. **Given** an image SBOM listing a component with hash `X`, and a build SBOM in the same store with `subject:sha256:X`, **When** an external consumer walks the chain, **Then** the consumer correlates the image component to the build SBOM with no mikebom intervention.
2. **Given** the same setup plus a source SBOM with `repo:R` matching the build SBOM's `repo:R`, **When** the consumer continues the walk, **Then** the source SBOM is correlated to the build SBOM.
3. **Given** a multi-output build (build SBOM has 3 `subject:` identifiers), **When** the image SBOM has components matching 2 of those 3 hashes, **Then** the consumer correlates 2 of 3; the third subject (unused by this image) is benign.

---

### User Story 4 — Per-component user-defined identifier attachment (Priority: P1)

An operator runs:

```
mikebom sbom scan --path . \
    --component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-shared-lib-v2" \
    --component-id "pkg:cargo/myapp@0.5.1=acme-asset:myapp-prod-001" \
    --output out.cdx.json
```

The emitted SBOM has those two specific components carrying the user-defined identifiers in standards-native fields. An external consumer reads the per-component identifiers and correlates against the operator's internal asset DB without needing mikebom-specific decoders.

**Why this priority**: Closes the operator-facing gap in cross-SBOM correlation. Many enterprise SBOM-store users have internal asset-tracking systems where components are referenced by org-specific IDs, not by content hashes. Letting them attach those IDs as first-class per-component fields means their SBOMs become useful inside their org's tooling immediately. P1 because it's the second deliverable the user explicitly asked for in the same milestone.

**Independent Test**: Scan a project with two components and a `--component-id` flag matching one of them. Verify the matching component carries the identifier in its native per-component carrier (CDX `properties[]` / SPDX 2.3 `externalRefs[]` / SPDX 3 `externalIdentifier[]`); the non-matching component is unchanged.

**Acceptance Scenarios**:

1. **Given** a project with components `pkg:cargo/serde@1.0.0` and `pkg:cargo/foo@0.1.0`, **When** the operator passes `--component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-shared-lib-v2"`, **Then** the emitted SBOM has `serde@1.0.0` carrying a `kusari-id` identifier with value `asset-shared-lib-v2`; `foo@0.1.0` is unchanged.
2. **Given** the same project, **When** the operator passes `--component-id "pkg:cargo/nonexistent@0.0.0=asset-id:foo"` (no matching component), **Then** the scan emits a `tracing::warn!` listing the unmatched selector; the scan does not fail.
3. **Given** a project where multiple components match a single PURL (rare but possible across different `bom-ref` values), **When** the operator passes `--component-id` with that PURL, **Then** ALL matching components receive the identifier (deliberate broadcast; matches the spec's "by PURL" semantics).
4. **Given** the operator passes `--component-id "pkg:cargo/foo@1.0=subject:sha256:X"` (built-in scheme name in a per-component context), **Then** the CLI rejects the flag at parse time with a clear error: per-component built-in schemes are reserved for future native-field usage; user-defined schemes only.
5. **Given** the operator passes `--component-id` with a malformed selector or value (empty PURL, missing `=`, empty scheme, empty value), **Then** the CLI rejects at parse time with a clear, actionable error message.

---

### Edge Cases

- **Build with no identifiable output** (e.g., `mikebom trace run -- echo hello`): no `subject:` identifier emitted; SBOM body is otherwise identical to alpha.16 build behavior.
- **Subject with no sha256 digest** (e.g., a build that only produced a sha1 or sha512 entry — rare but possible for legacy ecosystems): auto-detect emits nothing for that subject and logs a `tracing::info!` with the subject name + available algos so operators can decide whether to fall back to manual `--subject-hash` for the non-sha256 case (per the 2026-05-06 clarification).
- **Multi-output build with one output also matching the image digest** (e.g., the wrapped command was `docker build -t foo:v1 .` and the emitted image is `foo:v1@sha256:X`): the build SBOM emits `subject:sha256:X`; an image scan of `foo:v1` emits `image:foo:v1@sha256:X`. The X portion matches across the two tiers without any operator coordination — this is the cleanest possible cross-tier handshake.
- **`subject:` value validation failure**: malformed `<algo>:<hex>` (wrong length, non-hex chars, unknown algo) soft-fails to `IdentifierKind::UserDefined` per milestone 073's FR-010 rule. The scan does not fail.
- **Auto-detected and manual `--subject-hash` with same value**: deduplicated to one entry attributed to the manual flag (per milestone 073 FR-006 dedup rule).
- **`--component-id` with a built-in scheme** (`subject:`, `repo:`, `git:`, `image:`, `attestation:`): rejected at CLI parse time. Per-component identifiers are user-defined-only in this milestone; the built-in schemes are reserved for the document level (where their semantics are well-defined). A future milestone may reconsider.
- **`--component-id` selector that matches zero components**: warn + continue. Operators may pass selectors speculatively (e.g., across multiple scan runs of different projects); failing the scan would be too strict.
- **`--component-id` selector that's not a valid PURL**: reject at parse time. The selector is contract-shape — non-PURL inputs aren't ambiguity, they're errors.
- **Component selector matches a component that already has a same-`(scheme, value)` identifier**: deduplicated. The same identifier value attached twice produces one entry, not two.
- **CDX 1.6 vs SPDX 2.3 vs SPDX 3 emission semantics**: all three formats have native per-component fields suitable for user-defined identifiers (per Constitution Principle V's native-precedence rule). No `mikebom:component-identifiers` annotation is introduced; the implementation rides existing native carriers per format.
- **`subject:` on source-tier scans**: source trees are mutable and not content-addressable; `subject:` is omitted by default. The manual `--subject-hash` flag works if operators have an external content-addressing scheme they want to attach.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Add `subject:<algo>:<hex>` as a fifth built-in identifier scheme. Algo MUST be one of `sha256`, `sha512` (lowercase). Hex MUST be lowercase, of length matching the algo (64 chars for sha256, 128 chars for sha512).
- **FR-002**: When `mikebom trace run` produces a non-empty subject set (the same set the in-toto attestation envelope captures in `statement.subject[]`), the build-tier SBOM body MUST emit one `subject:sha256:<hex>` identifier per subject that has a sha256 digest in its `digest` map. Identifiers are emitted in the same deterministic order the attestation uses (lexical sort by `(name, digest)` per witness-v0.1 conventions). Subjects with multi-algo digest maps (e.g., both sha256 and sha512) auto-emit only the sha256 form; other algos are dropped from auto-detection per the 2026-05-06 clarification. Subjects lacking a sha256 digest auto-emit nothing for that subject and the system MUST log a `tracing::info!` recording the skip with the subject's name and the available algos. Operators who need a non-sha256 form pass `--subject-hash sha512:<hex>` (or other) manually.
- **FR-003**: Add `--subject-hash <algo>:<hex>` flag, repeatable, to both `mikebom sbom scan` and `mikebom trace run`. Manual `--subject-hash` values augment auto-detected ones (per US1 scenario 4); deduplication by `(scheme, value)` collapses exact matches per milestone 073 FR-006.
- **FR-004**: `subject:` identifiers MUST ride standards-native per-document carriers consistent with milestone 073: CDX `metadata.component.externalReferences[]` (with an appropriate `type` value, e.g., `provenance` or another existing CDX 1.6 enum value that fits the "binary subject" semantic), SPDX 2.3 `Package.externalRefs[].referenceCategory = PERSISTENT-ID`, SPDX 3 `Element.externalIdentifier[]` with `type` matching the scheme name (`subject`).
- **FR-005**: When the `subject:` value fails validation (FR-001's regex rule), the identifier MUST soft-fail to `IdentifierKind::UserDefined` per milestone 073 FR-010, with a `tracing::warn!` log. The scan MUST NOT fail.
- **FR-006**: Source-tier scans (`mikebom sbom scan --path`) MUST NOT auto-detect a `subject:` identifier. The manual `--subject-hash` flag works on source-tier; the auto-detect path is reserved for tiers where content-addressing is natively available (build-tier; image-tier already covers it via `image:`).
- **FR-007**: Add `--component-id <PURL>=<scheme>:<value>` flag, repeatable, to both `mikebom sbom scan` and `mikebom trace run`. The `<PURL>` is exact-match (no glob) and selects components by their emitted `purl` field. The `<scheme>:<value>` follows milestone 073's identifier-pair format.
- **FR-008**: Per-component user-defined identifiers MUST ride standards-native per-component carriers per Constitution Principle V: CDX `components[].properties[]` with `name = "<scheme>"` and `value = "<value>"` (or `components[].externalReferences[]` if the component-level shape is more idiomatic — Phase 0 research pins the choice), SPDX 2.3 `Package.externalRefs[].referenceCategory = PERSISTENT-ID` with `referenceType = <scheme>` and `referenceLocator = <value>`, SPDX 3 `Element.externalIdentifier[].type = <scheme>` with `identifier = <value>`. No new `mikebom:*` annotations introduced (audit per Principle V's 5th bullet).
- **FR-009**: `--component-id` MUST reject built-in scheme names (`subject`, `repo`, `git`, `image`, `attestation`) at CLI parse time with a clear error. Per-component built-in schemes are reserved for future native-field usage; user-defined schemes only at this layer.
- **FR-010**: When a `--component-id` selector matches zero components in the emitted SBOM, the system MUST emit a `tracing::warn!` listing the unmatched selector and continue. The scan MUST NOT fail.
- **FR-011**: When a `--component-id` selector matches multiple components (e.g., the same PURL appears under different `bom-ref` values), the identifier MUST be attached to ALL matching components.
- **FR-012**: All emission MUST be deterministic per milestone 073's contract: same input → byte-identical output. Per-component identifiers are emitted in lexical order by `(scheme, value)` for stable serialization.
- **FR-013**: Cross-format byte-identity goldens for fixtures with no `--component-id` flags and no auto-detected `subject:` (i.e., all existing 073/074/075 fixtures since none had build outputs flowing through the auto-detect path) MUST stay byte-identical to alpha.17. Goldens for fixtures that do exercise the new paths gain additive entries — that is the expected golden regen for this milestone.
- **FR-014**: The image-tier handshake described in US3 (build SBOM's `subject:sha256:X` matches image SBOM's `image:` digest) MUST work without any mikebom-side coordination — purely string-match correlation by external tools. The `subject:` value's hex portion equals the digest portion of the corresponding `image:` value when they refer to the same artifact.

### Key Entities

- **SubjectIdentifier**: A document-level identifier of form `subject:<algo>:<hex>` declaring "this SBOM describes the artifact with the given content hash." Composed of: scheme name (always `subject`), algo (sha256 or sha512), hex digest. Multiple subject identifiers may attach to one SBOM (multi-output builds). Carries a `source_label` per milestone 073's pattern indicating origin (auto-detected from trace subject set / manual `--subject-hash` flag).
- **ComponentIdentifier**: A per-component user-defined identifier attached via `--component-id`. Composed of: a PURL selector that picks components, a scheme name (any string passing the FR-004 regex from milestone 073, but rejected if it equals a built-in), a value. Materialized in the emitted SBOM as a per-component native field per format.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For a `mikebom trace run -- ./build.sh` invocation that produces a known binary at hash `X`, the emitted build SBOM body contains a `subject:sha256:X` identifier without any manual flag. Verified by integration test against a tempdir fixture that builds a small Rust crate and asserts on the emitted SBOM's identifier set.
- **SC-002**: An external SBOM-store consumer (a small Python or jq harness) holding a build SBOM with `subject:sha256:X` and an image SBOM listing a component with `hashes[].sha256 == X` can correlate the two by string match alone, without invoking mikebom. Verified by an end-to-end integration test that runs the harness and asserts the correlation succeeds.
- **SC-003**: A `mikebom sbom scan` invocation with `--component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-foo"` produces an SBOM where exactly one component (the matching one) carries the per-component identifier in standards-native carriers; non-matching components are unchanged. Verified by integration test asserting on per-component fields across CDX, SPDX 2.3, and SPDX 3 outputs.
- **SC-004**: When the same SBOM is emitted twice with identical inputs (same scan target, same flags, same fixture state), both emissions are byte-identical. Verified by determinism integration test re-running scan against a fixed fixture.
- **SC-005**: Source-tier scans of fixtures without `--component-id` and without `--subject-hash` produce SBOMs byte-identical to alpha.17 (no regression). Verified by the existing parity-check golden suite continuing to pass unchanged.
- **SC-006**: A multi-output build (5 binaries) produces a build SBOM with 5 distinct `subject:` identifiers in deterministic lexical order. Verified by integration test on a synthetic multi-output fixture.
- **SC-007**: `--component-id` with a built-in scheme name (e.g., `subject:`, `repo:`) fails at CLI parse time with a clear error message. Verified by negative integration tests asserting on exit code and error text.
- **SC-008**: `--component-id` with a selector matching zero components emits a warning and the scan exits 0. Verified by integration test running a scan with a non-matching selector.
- **SC-009**: All three SBOM formats (CDX 1.6, SPDX 2.3, SPDX 3) carry the per-component user-defined identifier in their respective standards-native per-component fields. Constitution Principle V audit passes — no new `mikebom:*` annotations introduced.

## Assumptions

- The build-tier subject set comes from the same in-toto witness-v0.1 attestation collection that already runs as part of `mikebom trace run`. Reading these subjects at SBOM-emit time is a small additional read of in-process state, not a new subprocess or filesystem touch.
- The `subject:<algo>:<hex>` value validator restricts the algo to `sha256` or `sha512` initially. Other hash algos (sha1, blake2, etc.) are out of scope; they can be added incrementally if downstream demand emerges.
- The `--component-id` selector is exact-PURL-match in the MVP. Glob/wildcard support, version-range matching, and `bom-ref`-based selection are explicit future-milestone work.
- Per-component identifiers ride existing standards-native per-component fields. The implementation MUST NOT introduce a new `mikebom:component-identifiers` annotation. Constitution Principle V's native-first rule applies; per-format native carriers exist for all three formats and are sufficient.
- Per-component built-in schemes (e.g., a per-component `subject:` identifier attached to a specific component) are out of scope. Only document-level `subject:` and per-component user-defined are in this milestone. The CLI rejects per-component built-in schemes to keep the boundary clear.
- Image-tier scans don't introduce new automatic `subject:` emission since the image digest is already encoded in the `image:` identifier (per milestone 073 FR-008). Operators who want a redundant `subject:` on image-tier scans can pass `--subject-hash` manually.
- Operators on alpha.16 / alpha.17 who want the per-component identifier behavior can mitigate today by post-processing emitted SBOMs with `mikebom sbom enrich` or another JSON-Patch-based tool. This milestone makes the behavior first-class.
- Backward compatibility: existing flag set is unchanged; only two new flags (`--subject-hash`, `--component-id`) are added. Operators not using either get byte-identical output to alpha.17 (per FR-013 + SC-005).
- The cross-tier digest handshake (US3, FR-014) does not require any new mikebom-side infrastructure. It works because the build-tier subject hash and the image-tier image-manifest digest happen to be the same string when the build produced an image that was later scanned. The handshake is a property of how the inputs flow through; mikebom merely ensures both ends emit the digest in stable, parseable identifier slots.
- "Kusari-id" as a scheme name in the user description is a placeholder; mikebom does not bake in any vendor-specific scheme name. Operators choose their own scheme names (e.g., `kusari-id`, `acme-asset-id`, `internal_ticket`, etc.) — anything passing milestone 073's FR-004 regex is accepted as a user-defined scheme on `--component-id`.
