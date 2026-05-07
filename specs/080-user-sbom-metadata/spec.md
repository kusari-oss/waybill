# Feature Specification: User-provided SBOM metadata

**Feature Branch**: `080-user-sbom-metadata`
**Created**: 2026-05-07
**Status**: Draft
**Input**: GitHub issue #94 — "Add user-provided SBOM metadata: scan-target name, creators, annotators, comments"

## Overview

mikebom currently auto-populates SBOM metadata fields from its own identity only — CDX `metadata.tools[].name = "mikebom"`, SPDX 2.3 `creationInfo.creators[]` with mikebom's tool entry, SPDX 3 `Tool` element. Operators integrating mikebom into automation pipelines (CNCF projects, internal CI workflows, sigstore-style attestation builders) need to inject **their own** identity and context onto the emitted SBOM so downstream consumers can trace which automation produced the file.

Today the only path is to post-process the output with `jq`, which is fragile across CDX/SPDX 2.3/SPDX 3 shape differences, bypasses mikebom's own validation, and forces operators to learn three different format-specific edits for the same conceptual operation. Issue #94 documents a real-world `jq` recipe a CNCF-style automation uses today; this milestone replaces the recipe with native CLI flags that emit equivalent metadata symmetrically across all three formats.

The milestone delivers five flag surfaces — `--scan-target-name`, `--creator <Type: Name>` (repeatable), `--annotator <Type: Name>` + `--annotation-comment`, `--metadata-comment`, and `--metadata-file <path.json>` (sidecar variant) — each landing at the standards-native field position in every emitted format. All flags are additive: they augment mikebom's auto-populated fields rather than replacing them.

The deliberate scope: SBOM-document-level metadata only. Per-component metadata edits are already covered by `mikebom sbom enrich`'s JSON Patch path. Verification that supplied creator strings match a known organization or OIDC identity is a separate signing-side concern.

## Clarifications

### Session 2026-05-07

- Q: When operators want to attach more than one annotation to an SBOM, what's the CLI parsing rule for `--annotator` / `--annotation-comment` pairs? → A: **Positional pairing.** Each `--annotator <Type: Name>` MUST be immediately followed by exactly one `--annotation-comment <text>`; the parser pairs them by position. Repeatable: `--annotator A --annotation-comment X --annotator B --annotation-comment Y` produces two annotations. Out-of-order forms (`--annotator A --annotator B --annotation-comment C`) fail with a clear "expected --annotation-comment after --annotator" error. Mirrors the repeatable-flag idiom of US1's `--creator` while keeping pair semantics explicit.
- Q: If the plan-time CDX 1.6 schema audit finds CDX 1.6's annotation surface insufficient for what this milestone needs, what's the fallback emission strategy? → A: **`mikebom:` namespaced parity bridges in CDX `metadata.properties[]`** per Constitution Principle V's escape clause. Property keys: `mikebom:invocation-comment` (for `--metadata-comment`), `mikebom:annotation` (for each `--annotator`/`--annotation-comment` pair, value JSON-encoded as `{"annotator":"Type: Name","comment":"text"}`). Each bridge MUST be documented in `docs/reference/sbom-format-mapping.md` with a justification clause naming the missing native CDX field. The audit confirms first; if CDX 1.6 turns out to have a native equivalent (likely `bom.annotations[]` per the 1.6 spec changelog), the bridge is unused and full native parity is achieved. Plan-phase Phase 0 §N pins the decision against the actual CDX 1.6 schema.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Record automation provenance via `--creator` (Priority: P1)

A CNCF project's release pipeline runs `mikebom sbom scan` as part of its release-artifact workflow. The pipeline operator needs the emitted SBOM to credit BOTH mikebom (the SBOM-generation tool) AND the CNCF automation (the orchestration layer that ran mikebom). Today the operator's only option is to post-process with `jq`. Post-fix, the operator passes `--creator "Tool: cncf-automation-sbom-generator"` and the resulting SBOM lists both mikebom and the automation in the appropriate native field of each format.

**Why this priority**: This is the dominant operator pain point in the issue body — the CNCF-style `jq` recipe does this exact thing, and replacing it with a native flag is the highest-value single delivery. P1 because operators today have no clean alternative; they ship fragile shell scripts that break across format updates.

**Independent Test**: Run `mikebom sbom scan --path . --creator "Tool: my-pipeline" --format cyclonedx-json,spdx-2.3-json,spdx-3-json`; assert the emitted CDX has a `metadata.tools[]` entry for `my-pipeline`, the SPDX 2.3 has `creationInfo.creators` containing `Tool: my-pipeline`, and the SPDX 3 has a `Tool` element with that name.

**Acceptance Scenarios**:

1. **Given** a single `--creator "Tool: my-pipeline"` invocation, **When** the SBOM is emitted in any of the three formats, **Then** the resulting SBOM contains BOTH the mikebom auto-populated entry AND a new entry for `my-pipeline` in the format's native creators/tools field.
2. **Given** multiple `--creator` flags (e.g., `--creator "Tool: ci-runner" --creator "Organization: ACME Corp"`), **When** the SBOM is emitted, **Then** all flag values appear in the format's native field, additive on top of the mikebom entry.
3. **Given** `--creator "Person: Alice <alice@example.com>"`, **When** the SBOM is emitted in SPDX 2.3, **Then** the SPDX 2.3 `creationInfo.creators[]` contains `Person: Alice <alice@example.com>` verbatim. (SPDX 2.3's creator format is `Type: Name`; the flag value is passed through.)

---

### User Story 2 — Record document-level context via `--metadata-comment` and `--annotator` + `--annotation-comment` (Priority: P1)

The same CNCF-style automation needs to record contextual information about the SBOM itself — what release tag it covers, what time window the scan represents, who reviewed it. Two flag families serve this need:

- `--metadata-comment "<text>"` — single free-text comment about the SBOM-as-a-document (e.g., `--metadata-comment "Release v2.5.0 of foo/bar"`).
- `--annotator "<Type: Name>" --annotation-comment "<text>"` — structured annotation pair attributing a comment to a specific annotator (e.g., `--annotator "Tool: security-scanner" --annotation-comment "Reviewed for CVE-2024-1234 exposure"`).

**Why this priority**: The issue's `jq` recipe writes both `creationInfo.comment` and `annotations[]` fields. Both are common operator needs; co-shipping them keeps the milestone scope tight. P1 because without them, operators still need `jq` post-processing for half their use cases — defeats the milestone's purpose.

**Independent Test**: Run `mikebom sbom scan --path . --metadata-comment "Release v1.0.0" --annotator "Tool: reviewer" --annotation-comment "Approved 2026-05-07"`; in each emitted format, assert the metadata-comment lands at the format's document-level free-text slot AND the annotator/annotation-comment lands as a document-level annotation entry.

**Acceptance Scenarios**:

1. **Given** `--metadata-comment "Release v1.0.0"`, **When** the SBOM is emitted in SPDX 2.3, **Then** `creationInfo.comment` equals `"Release v1.0.0"`.
2. **Given** the same flag, **When** the SBOM is emitted in CDX 1.6, **Then** the comment lands at the CDX-native equivalent (per the Principle V audit at plan time — likely `bom.annotations[]` if CDX 1.6 has native annotations support, or `metadata.properties[]` with a documented `mikebom:invocation-comment` key as the parity bridge).
3. **Given** the same flag, **When** the SBOM is emitted in SPDX 3, **Then** an `Annotation` element of type `OTHER` is present in `@graph` with the comment text.
4. **Given** `--annotator "Tool: T" --annotation-comment "C"`, **When** the SBOM is emitted in any format, **Then** a document-level annotation entry exists at the format's annotation slot, attributing `C` to `T` with a deterministic timestamp (the SBOM emission time, same as mikebom's existing creationInfo timestamp).
5. **Given** `--annotation-comment "C"` without a corresponding `--annotator`, **When** the CLI parses the flags, **Then** the invocation fails with a clear error message — the two flags MUST appear together.

---

### User Story 3 — Override the auto-derived scan-target name via `--scan-target-name` (Priority: P2)

When mikebom scans a directory, it auto-derives a scan-target name from the path basename (or the manifest-derived main module if present, post-milestone 077). Operators sometimes want to override it — for instance, scanning a temp dir checked out by CI but wanting the SBOM to identify the conceptual project name, not `mikebom-build-12345-tmp`. The `--root-name` flag from milestone 077 already handles the **root component** name; this milestone adds `--scan-target-name` for the **document/Sbom-level** name in formats that distinguish the two.

**Why this priority**: Lower priority because milestone 077 already covers the dominant rename case (root component name). The remaining gap is the document-level `name` field (CDX `metadata.component.name` is partially overridable via 077; SPDX 2.3 `name` and SPDX 3 `software_Sbom.name` may emit a different default that's not yet operator-controllable). The issue body notes "already partially there via the milestone 005 `scan_target_coord` plumbing — extend to all three formats." P2 because it's an extension, not a new gap.

**Independent Test**: Run `mikebom sbom scan --path /tmp/build-12345 --scan-target-name "myproject"`; assert all three emitted formats use `myproject` as the document/Sbom-level name (NOT the path basename).

**Acceptance Scenarios**:

1. **Given** `--scan-target-name "foo"`, **When** the SBOM is emitted in SPDX 2.3, **Then** the document `name` field equals `"foo"`.
2. **Given** the same flag, **When** the SBOM is emitted in SPDX 3, **Then** the `software_Sbom.name` field equals `"foo"`.
3. **Given** the same flag, **When** the SBOM is emitted in CDX 1.6, **Then** the `metadata.component.name` field equals `"foo"` (interaction with milestone 077's `--root-name` documented at plan time — likely either flag overrides the auto-derived default, with `--root-name` taking precedence when both are passed).

---

### User Story 4 — Sidecar JSON file `--metadata-file <path.json>` (Priority: P2)

Pipelines that already manage structured metadata (e.g., a release manifest stored alongside CI configuration) prefer to point mikebom at a JSON file rather than passing six flags. The `--metadata-file <path.json>` flag accepts a JSON file with the same fields as the individual flags; the file's values are equivalent to passing each flag explicitly.

**Why this priority**: Convenience surface for pipelines, not a new capability. The flag-only invocations from US1 + US2 + US3 cover every functional scenario; the file is a UX improvement. P2 because operators can ship US1+US2+US3 with shell-script-managed flag invocations until the file is added.

**Independent Test**: Create a JSON file with the metadata fields; run `mikebom sbom scan --path . --metadata-file /tmp/meta.json`; assert the emitted SBOM contains every field from the file at the same locations as if the flags had been passed individually.

**Acceptance Scenarios**:

1. **Given** a `meta.json` containing `{"creators": ["Tool: my-pipeline"], "metadata_comment": "Release v1.0.0"}`, **When** the operator invokes `mikebom sbom scan --metadata-file meta.json`, **Then** the emitted SBOM contains BOTH the creator entry AND the metadata comment as if `--creator "Tool: my-pipeline" --metadata-comment "Release v1.0.0"` had been passed.
2. **Given** `--metadata-file meta.json --creator "Tool: extra"`, **When** the SBOM is emitted, **Then** both the file-supplied creators AND the flag-supplied creators are present (additive behavior; flag and file don't conflict — they merge).
3. **Given** `meta.json` with an unknown top-level field, **When** the file is loaded, **Then** the invocation fails with a clear "unknown field" error message naming the offending field.
4. **Given** `meta.json` malformed JSON, **When** the file is loaded, **Then** the invocation fails with a clear parse-error message including line+column.

---

### Edge Cases

- **`--creator "Tool: mikebom"` (operator passes mikebom's own auto-populated identity)**: post-fix output has TWO mikebom-tool entries (mikebom's auto-populated + the operator's duplicate). Operator-error; mikebom does NOT dedup. Document this; operators inspecting their flags can correct.
- **Format-specific creator-prefix conventions**: SPDX 2.3 `creationInfo.creators[]` uses `Type: Name` prefixes (`Tool:`, `Organization:`, `Person:`). CDX 1.6 `metadata.tools[]` doesn't have a Type prefix — instead, `tools[]` is for tools and `metadata.authors[]` is for humans, with shape `{name, email}` for authors. SPDX 3 has separate `Tool`, `Organization`, `Person` element classes. The flag value `--creator "Type: Name"` MUST be parsed once at the CLI layer and routed format-appropriately: `Tool:` → CDX `tools[]` / SPDX 2.3 `creators[]` / SPDX 3 `Tool` element; `Organization:` → CDX `metadata.manufacturer` (or equivalent) / SPDX 2.3 `creators[]` / SPDX 3 `Organization` element; `Person:` → CDX `metadata.authors[]` / SPDX 2.3 `creators[]` / SPDX 3 `Person` element.
- **Invalid `Type:` prefix in `--creator`**: e.g., `--creator "Bot: foo"` (Bot is not in the SPDX 2.3 spec). The CLI MUST reject at parse time with a clear "valid prefixes are Tool, Organization, Person" message.
- **`--annotator` without `--annotation-comment`** (or vice versa): MUST fail at parse time. Document-level annotations require both.
- **Multiple `--annotator` invocations**: each one MUST be immediately followed by exactly one `--annotation-comment` per the 2026-05-07 clarification (positional pairing). `--annotator A --annotator B --annotation-comment C` fails parsing because B has no following `--annotation-comment`. `--annotator A --annotation-comment X --annotator B --annotation-comment Y` succeeds and emits two annotations.
- **Free-text values containing JSON-LD-significant characters** (quotes, backslashes, newlines): MUST be JSON-encoded correctly in the emitted SBOM. Standard `serde_json::to_string` handles this.
- **`--metadata-comment` length**: no hard limit imposed by mikebom. The SPDX 2.3 spec doesn't bound `creationInfo.comment` length; long comments emit verbatim.
- **`--metadata-file` containing flag values that conflict with command-line flags** (e.g., file has `metadata_comment: "X"` and the operator also passes `--metadata-comment Y`): the LATER source wins, and a warning is emitted. Or: invocation fails with a clear conflict message. Plan-time decision.
- **Multiple SBOM emissions per invocation** (e.g., `--format cyclonedx-json,spdx-2.3-json,spdx-3-json`): the SAME flag values land at the equivalent native fields in all three emitted documents. Determinism preserved.
- **Existing milestone-073/074/075/076/077/078/079 byte-identity goldens**: WILL regenerate when the new metadata fields land — the auto-populated mikebom entry stays the same, but format-emission code paths gain new metadata serialization. Per-fixture diff sizes will vary by format; CDX likely smallest, SPDX 3 likely largest (new graph elements for Tool / Organization / Annotation).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: mikebom MUST accept the `--creator <Type: Name>` flag (repeatable) on `mikebom sbom scan` and `mikebom trace run`. Each invocation appends one creator entry to the emitted SBOM at the format's standards-native location: CDX `metadata.tools[]` (for `Tool:`), `metadata.manufacturer` or equivalent (for `Organization:`), `metadata.authors[]` (for `Person:`); SPDX 2.3 `creationInfo.creators[]` with the `Type: Name` string verbatim; SPDX 3 `Tool` / `Organization` / `Person` element added to `@graph` with `creationInfo: _:creation-info`. Mikebom's auto-populated mikebom entry is preserved alongside.
- **FR-002**: mikebom MUST accept the `--metadata-comment <text>` flag on `mikebom sbom scan` and `mikebom trace run`. The text lands at the format's standards-native document-level free-text slot: SPDX 2.3 `creationInfo.comment`; SPDX 3 `Annotation` element of type `OTHER` attached to the `SpdxDocument` with the text as `statement`; CDX 1.6 `bom.annotations[]` if the plan-time Principle V audit confirms native support, OR per the 2026-05-07 Q2 fallback: `metadata.properties[]` with key `mikebom:invocation-comment` as a documented parity bridge in `docs/reference/sbom-format-mapping.md`.
- **FR-003**: mikebom MUST accept the `--annotator <Type: Name>` flag paired with `--annotation-comment <text>` on `mikebom sbom scan` and `mikebom trace run`. The pair emits a document-level annotation entry: SPDX 2.3 `annotations[]` with `annotator`, `annotationDate` (the SBOM emission timestamp), `annotationType: "OTHER"`, `comment`; SPDX 3 `Annotation` element with the same fields; CDX 1.6 per the plan-time audit. Per the 2026-05-07 clarification, the parser pairs the flags **positionally**: each `--annotator` MUST be immediately followed by exactly one `--annotation-comment` before any subsequent `--annotator` or non-paired flag. Repeatable: `--annotator A --annotation-comment X --annotator B --annotation-comment Y` produces two annotations. Out-of-order forms (`--annotator A --annotator B --annotation-comment C`, or `--annotator A` with no following `--annotation-comment`) MUST fail parsing with a clear error message.
- **FR-004**: mikebom MUST accept the `--scan-target-name <name>` flag on `mikebom sbom scan` and `mikebom trace run`. The name overrides the auto-derived document/Sbom-level `name` field: SPDX 2.3 `name`; SPDX 3 `software_Sbom.name`; CDX 1.6 `metadata.component.name` (interacting with milestone 077's `--root-name` per the plan-time decision — likely `--root-name` takes precedence when both are passed because root-component naming is more specific). Validation rules MUST mirror milestone 077's `--root-name` (non-empty UTF-8, no control characters, no `?`/`#`).
- **FR-005**: mikebom MUST accept the `--metadata-file <path.json>` flag on `mikebom sbom scan` and `mikebom trace run`. The file's content MUST be valid JSON with a defined top-level shape (per the plan/research-time schema): an object with optional fields `creators` (array of `Type: Name` strings), `annotators` (array of `{type_name, comment}` objects so multiple annotations can be specified), `metadata_comment` (string), `scan_target_name` (string). Each field's effect equals passing the corresponding flag once per array element. Unknown top-level fields fail with a clear error.
- **FR-006**: When BOTH `--metadata-file` AND individual flags are passed, the values MUST merge additively: file-supplied creators + flag-supplied creators all appear in the output. Single-valued fields (`metadata_comment`, `scan_target_name`) MUST NOT both appear in file AND flag — if so, fail with a clear conflict message naming both sources.
- **FR-007**: All user-supplied metadata flags MUST be additive — they augment mikebom's auto-populated fields rather than replacing them. The mikebom entry on every format's creators/tools field is preserved.
- **FR-008**: All five flag families MUST emit symmetric metadata across all three formats. Where the plan-time CDX 1.6 audit reveals CDX lacks a native equivalent (e.g., no `bom.annotations[]` or no document-level free-text slot), mikebom MUST use the 2026-05-07 Q2 fallback: a `mikebom:` namespaced property in `metadata.properties[]` (keys `mikebom:invocation-comment` for `--metadata-comment`; `mikebom:annotation` with JSON-encoded value `{"annotator":"Type: Name","comment":"text"}` for each annotation pair). Each parity bridge actually emitted MUST be recorded in `docs/reference/sbom-format-mapping.md` with a justification clause naming the missing native CDX field, per Constitution Principle V's standards-native-precedence escape clause.
- **FR-009**: Emission MUST be deterministic — same flag inputs + same scan inputs produce byte-identical SBOMs across re-runs. The new metadata fields use stable serialization order (alphabetical by key in JSON; sorted by `Type: Name` string within array fields).
- **FR-010**: All emitted SBOMs MUST continue to pass schema validation: CDX 1.6 JSON schema (the existing dev-dep validator), SPDX 2.3 JSON schema, SPDX 3 JSON schema, AND the JPEWdev `spdx3-validate` SHACL gate from milestone 078. Adding metadata MUST NOT introduce SHACL or schema violations.
- **FR-011**: Cross-format parity for the new metadata fields MUST be verified by integration tests (in `mikebom-cli/tests/sbom_user_metadata.rs`) asserting that the same operator input lands at each format's standards-native location across CDX 1.6 / SPDX 2.3 / SPDX 3 emission. Where a format lacks a native field for a given input (the Q2 fallback path; not triggered per the Phase 0 §1 audit), the parity bridge MUST be documented in `docs/reference/sbom-format-mapping.md` with a justification clause naming the missing native field. The existing `holistic_parity` test target is NOT extended — its scope (CDX/SPDX 2.3/SPDX 3 component-level emission parity) is orthogonal to document-level metadata; per-flag cross-format equivalence is asserted directly in the new integration test file.
- **FR-012**: All flag values containing user-supplied free text (`Name` portion of `Type: Name`, `--metadata-comment`, `--annotation-comment`, `--scan-target-name`) MUST be JSON-encoded correctly in the emitted SBOM — standard escaping for quotes, backslashes, newlines, control characters. No injection vector into JSON-LD output.

### Key Entities

- **Creator**: An entry in the SBOM's "who/what produced this document" list. Composed of: `Type` (∈ {Tool, Organization, Person}); `Name` (free-text). User-supplied via `--creator` (repeatable) or `--metadata-file`'s `creators[]` field. mikebom always auto-populates a `Tool: mikebom-<version>` entry alongside.
- **Annotator**: An entity that has commented on or reviewed the SBOM. Composed of: `Type` (∈ {Tool, Organization, Person}); `Name` (free-text); `comment` (free-text). User-supplied via `--annotator` + `--annotation-comment` pair, or via `--metadata-file`'s `annotators[]` array of `{type_name, comment}` objects.
- **Metadata comment**: A free-text comment about the SBOM document as a whole. User-supplied via `--metadata-comment` or `--metadata-file`'s `metadata_comment` field. At most ONE per emission.
- **Scan-target name**: Operator-supplied override for the document/Sbom-level `name` field. User-supplied via `--scan-target-name` or `--metadata-file`'s `scan_target_name` field. At most ONE per emission. Interacts with milestone 077's `--root-name` per the plan-time decision.
- **Metadata file**: A sidecar JSON file containing structured metadata. Fields are equivalent to the individual flags. File schema is defined at plan/research time; arbitrary extension fields are rejected.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An operator running `mikebom sbom scan --path . --creator "Tool: my-pipeline"` sees the new tool entry in the emitted CDX 1.6 SBOM's `metadata.tools[]` array AND in the SPDX 2.3 SBOM's `creationInfo.creators[]` AND in the SPDX 3 SBOM's `@graph` (as a `Tool` element). Verified by integration test asserting on each format.
- **SC-002**: An operator running `mikebom sbom scan --path . --metadata-comment "Release v1.0.0"` sees the comment in the emitted SPDX 2.3 SBOM's `creationInfo.comment` AND in the SPDX 3 SBOM's `Annotation` element AND in the CDX 1.6 SBOM at the format-native location identified by the plan-time audit. Verified by integration test.
- **SC-003**: An operator running `mikebom sbom scan --path . --annotator "Tool: T" --annotation-comment "C"` sees a document-level annotation entry with annotator=`T`, comment=`C`, and a deterministic timestamp in all three emitted formats. Verified by integration test.
- **SC-004**: An operator running `mikebom sbom scan --path . --scan-target-name "foo"` sees the document/Sbom name = `foo` in all three emitted formats. Verified by integration test.
- **SC-005**: An operator running `mikebom sbom scan --path . --metadata-file meta.json` (where the file contains the equivalent of `--creator X --metadata-comment Y`) gets a byte-identical SBOM to the same invocation with `--creator X --metadata-comment Y` passed as flags. Verified by determinism integration test that emits both forms and byte-compares.
- **SC-006**: Pre-existing operator workflows that rely on `jq` post-processing for the same metadata fields (per the issue body's CNCF-style recipe) can be replaced with native flags in a single shell-script edit. Verified by manual smoke against the issue's reproduction recipe (or documented in quickstart with a before/after diff).
- **SC-007**: Schema validation passes for every emitted SBOM with the new metadata fields populated. Verified by the existing CDX 1.6 + SPDX 2.3 + SPDX 3 schema-validation test suites.
- **SC-008**: `spdx3-validate` (the JPEWdev SHACL validator from milestone 078, pinned at 0.0.5) reports zero violations for SPDX 3 SBOMs containing the full metadata-flag set. Verified by extending milestone 078's `spdx3_conformance.rs` integration test.
- **SC-009**: Determinism — the same flag values + same scan inputs produce byte-identical output across re-runs. Verified by determinism integration test.
- **SC-010**: Pre-flag invocations produce byte-identical SBOMs to alpha.20 — the existing 27 CDX 1.6 + SPDX 2.3 + SPDX 3 byte-identity goldens DO NOT regenerate as part of this milestone. The new metadata flags are off-by-default; goldens (emitted without flags) stay byte-identical. Verified by `cdx_regression`, `spdx_regression`, and `spdx3_regression` test targets passing without their `MIKEBOM_UPDATE_*_GOLDENS` env vars. (This is the inverse contract from milestones 077/078/079, where version-string + emission-shape changes propagated through the goldens. Milestone 080's flag-gated emission preserves the byte-identity contract for the no-flag baseline.)
- **SC-011**: Cross-format parity for the new metadata fields is verified by integration tests in `mikebom-cli/tests/sbom_user_metadata.rs` (per the new test matrix; see contracts/user-sbom-metadata.md). Per the Phase 0 §1 audit, full native CDX 1.6 parity is achieved (`bom.annotations[]` confirmed); no `mikebom:` parity bridges are introduced. If a future audit reveals an insufficient native field, a B-row or M-row in `docs/reference/sbom-format-mapping.md` MUST document the bridge per Constitution Principle V's escape clause (this milestone's audit-record entry in `sbom-format-mapping.md` documents the positive audit outcome — native parity confirmed, no bridge needed).

## Assumptions

- The five flag surfaces (`--scan-target-name`, `--creator`, `--annotator` + `--annotation-comment`, `--metadata-comment`, `--metadata-file`) attach to BOTH `mikebom sbom scan` AND `mikebom trace run`. Both subcommands emit SBOMs and equivalently benefit. The `mikebom sbom enrich` JSON-Patch path is out of scope (it's post-hoc patching with its own input model).
- `--creator <Type: Name>` parses the colon-separated form once at the CLI layer. Valid `Type` values are exactly `{Tool, Organization, Person}` matching the SPDX 2.3 spec's `Creator:` field types. Any other prefix fails parsing.
- The `Name` portion of `Type: Name` (and all other free-text fields) accepts any non-empty UTF-8 string excluding control characters (matching milestone 077's `--root-name` validation).
- `--annotator` and `--annotation-comment` MUST appear as a positionally-paired set per the 2026-05-07 clarification: each `--annotator <Type: Name>` is immediately followed by exactly one `--annotation-comment <text>`. Multiple annotations: `--annotator A --annotation-comment X --annotator B --annotation-comment Y` emits two annotations.
- For multiple-annotation expression in `--metadata-file`, the file uses an `annotators: [{type_name: "Tool: T1", comment: "C1"}, {type_name: "Tool: T2", comment: "C2"}]` array shape so the pairing is explicit and avoids the CLI flag-pair ambiguity.
- `--metadata-file` schema: object with optional fields `creators` (array of strings), `annotators` (array of `{type_name, comment}` objects), `metadata_comment` (string), `scan_target_name` (string). Unknown top-level fields are rejected. The exact field names + naming convention (snake_case vs kebab-case) is finalized at research time, with the convention matching mikebom's existing JSON-input conventions for `mikebom sbom enrich`.
- When BOTH `--metadata-file` AND individual flags are passed, ARRAY fields (`creators`, `annotators`) merge additively; SINGLE-VALUED fields (`metadata_comment`, `scan_target_name`) fail with a conflict error if specified in both — the operator's intent is ambiguous and a silent override would be hostile.
- The new metadata fields use `serde_json::Value::Object` round-tripping via mikebom's existing emission code paths. No new serialization machinery needed beyond extending the per-format builders.
- CDX 1.6 is presumed to have native annotations support per the 1.6 schema changelog (`bom.annotations[]` was added in 1.6, replacing the older 1.5 `metadata.properties[]` workaround). The plan-phase Principle V audit confirms against the actual schema; if confirmed, full native parity is achievable for both `--metadata-comment` and `--annotator`/`--annotation-comment`. If the audit reveals CDX 1.6 lacks the needed surface, per the 2026-05-07 Q2 clarification mikebom falls back to documented `mikebom:invocation-comment` and `mikebom:annotation` properties in `metadata.properties[]`, with each emitted bridge recorded in `docs/reference/sbom-format-mapping.md` per Principle V's escape clause.
- This milestone deliberately ships as a single PR. The five flag surfaces are tightly coupled (they share the metadata-file shape, the parsing logic, the format-symmetric emission structure). Splitting would create transient states where some operator workflows are migrated to native flags and others still need `jq` — the worst of both worlds.
- All 27 existing byte-identity goldens (9 CDX + 9 SPDX 2.3 + 9 SPDX 3) regenerate as the expected operator-visible change of this milestone. Diff is bounded to the new metadata fields; existing component identity, version, and PURL emission stays unchanged.
- This milestone deliberately does NOT verify that `--creator` values match a known organization or OIDC identity. That's a signing-side concern (sigstore-style verification) tracked separately.
