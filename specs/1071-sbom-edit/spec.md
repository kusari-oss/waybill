# Feature Specification: Edit an emitted SBOM — filter, redact, re-identify, with a derivation record and signature chain

**Feature Branch**: `1071-sbom-edit`
**Created**: 2026-10-06
**Status**: Draft
**Input**: Issue #1129, "SBOM editing: filter, redact and re-identify components in an emitted SBOM", with the maintainer direction recorded on it (2026-10-05), and a review of OpenSSF bomctl (2026-10-06).

## Context

waybill generates SBOMs with as much data as it can, by design. An organisation that **distributes** an SBOM often needs a different document from the one waybill generated. It wants less irrelevant detail, no internal names or paths, and an honest statement that a patched internal package is really a known open-source one. Today the only post-generation tool is `waybill sbom enrich --patch`: a raw JSON Patch, for CycloneDX only, with no idea of "drop the dev dependencies" or "redact internal hostnames". Generation-time flags (`--exclude-path`, `--tier`, `--split`) shape a document only by scanning again, which defeats the model the issue sets out:

> generate everything once, derive what you distribute.

**What bomctl contributes, and what it doesn't.** bomctl (OpenSSF) is format-agnostic SBOM tooling.
- **Taken:** its small vocabulary of document operations (`merge`, `link`, `alias`, and the proposed `redact` and `trim`) is the right shape.
- **Not taken:** it parses every document into a format-neutral model that is deliberately limited to the NTIA minimum fields, and operates on a cache database. waybill's documents carry much more: 213 catalogued fields kept equivalent across three formats, `waybill:` annotations, and signatures. Round-tripping them through a lossy model would discard exactly what makes them useful. A file-in, file-out command suits waybill, which keeps no state between runs.

**Maintainer direction (issue #1129):**
1. Start simple, with policy files later, but design the operations so a policy file can drive them.
2. Re-identification uses the formats' native fields.
3. A derivative states what categories were removed, never the removed values.
4. A signature chain: the derived document *y* states it came from *x* by hash, carries or references *x*'s signature, and is itself signed.
5. Redaction is not anonymisation, and the documentation must say so.

## Clarifications

### Session 2026-10-06

- Q: When a dropped component sat between others in the dependency graph? → A: **Bridge.** Its dependents gain edges to its dependencies, so the graph stays connected, and their dependency lists are marked incomplete (FR-006).
- Q: Redaction mode? → A: **Either, chosen per field class**: remove the value, or replace it with a keyed pseudonym that is stable across documents and not reversible without the key.
- Q: Re-identification in this milestone? → A: **Next milestone**, alongside policy files. This milestone covers filter, redact, the derivation record and the signature chain.

## Out of Scope

- **Policy files.** The declarative file that applies a saved set of operations comes in the next milestone. This one makes every operation expressible as such an entry (FR-014).
- **Merge and split of multiple documents** (#627).
- **SBOM diff.**
- **Formats other than CycloneDX 1.6, SPDX 2.3 and SPDX 3.0.1 JSON**, including XML and tag-value.
- **Re-running enrichment** (deps.dev, advisories) after an edit.
- **Re-identification** (User Story 4), which is the next milestone, alongside policy files (Clarifications). The story is kept below so the operation vocabulary is designed with it in mind (FR-014).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Filter out what the recipient doesn't need (Priority: P1)

A release engineer has the full SBOM waybill generated for a product. Before sending it to a customer they remove the dev and test dependencies, the file-level inventory, and waybill's diagnostic annotations. They get a smaller document in the same format that is still valid, still self-consistent, and still says what it is.

**Why this priority**: it is the most common distribution need, and the base the other operations build on: selecting components, removing them cleanly, and recording that it happened.

**Independent Test**: generate a CycloneDX, an SPDX 2.3 and an SPDX 3 document from one scan. Drop the dev- and test-scoped components from each. Each output validates, refers to nothing that was removed, and the three still agree with each other.

**Acceptance Scenarios**:

1. **Given** a waybill SBOM, **When** components are dropped by lifecycle scope, **Then** the output contains none of them, and nothing in it refers to them: dependencies, relationships, vulnerability entries, completeness claims or licence references.
2. **Given** the same SBOM, **When** waybill's diagnostic annotations are dropped by namespace, **Then** none remain, and annotations from other producers are untouched.
3. **Given** the same edit applied to the CycloneDX, SPDX 2.3 and SPDX 3 outputs of one scan, **When** the three results are compared field by field, **Then** they agree, as the originals did.
4. **Given** any edit, **When** the output is compared with the input, **Then** everything the edit did not target is unchanged.

---

### User Story 2 - Redact what must not leave the organisation (Priority: P1)

A security team must not publish internal file paths, internal repository URLs and hostnames, or the names of internal-only packages. They redact those classes of data. The output contains none of the redacted values anywhere, and states that redaction was applied and to which categories.

**Why this priority**: it is the case that blocks distribution outright today. An SBOM with internal paths and hosts can't be published at all.

**Independent Test**: redact paths, an internal host pattern and an internal name pattern from a scan of a fixture that contains all three. A search of the output for each original value finds nothing, in any field.

**Acceptance Scenarios**:

1. **Given** an SBOM containing internal file paths, **When** paths are redacted, **Then** no original path appears anywhere in the output.
2. **Given** an internal hostname pattern, **When** URLs are redacted by that pattern, **Then** no matching URL or host appears anywhere, including in package identifiers and references.
3. **Given** an internal component-name pattern, **When** names are redacted, **Then** each matching name is redacted wherever it occurs, including inside package URLs. Each component stays internally consistent: its references, dependencies and identifiers still match one another.
4. **Given** any redaction, **When** the output's derivation record is read, **Then** it names the redacted categories and contains none of the redacted values.

---

### User Story 3 - The edited document is verifiably derived from the original (Priority: P1)

A customer receives an edited SBOM. They want to know it was derived from the vendor's signed original, and that it has not changed since the vendor edited it. They verify the chain with waybill.

**Why this priority**: without it, editing destroys integrity. A signed SBOM becomes an unsigned edited copy, and a consumer cannot tell a vendor's redaction from a third party's tampering.

**Independent Test**: sign a generated SBOM, edit it, and sign the result. Verification succeeds for the pair. It fails when either document is altered, and when the derived document is paired with a different original.

**Acceptance Scenarios**:

1. **Given** a signed original and an edit, **When** the output is produced with signing on, **Then** the output states it is derived from the original (by the original's hash), carries or references the original's signature, and is itself signed.
2. **Given** that output and the original, **When** the consumer verifies the chain, **Then** waybill reports both signatures valid and the derivation link intact.
3. **Given** the output alone, without the original, **When** verified, **Then** waybill reports the output's own signature and the recorded identity of the original, and says the original wasn't available to check.
4. **Given** an edit of an already-edited document, **When** verified, **Then** the chain covers every step back to the first original.
5. **Given** an unsigned original, **When** edited and signed, **Then** the derivation record says the original was unsigned. Nothing claims a signature that didn't exist.

---

### User Story 4 - Re-identify a patched internal package as its upstream (Priority: P2, next milestone)

An organisation ships `acme-foo`, which is open-source `foo` 2.3.1 with three internal patches. They declare that relationship. The SBOM then says, in each format's own vocabulary, that `acme-foo` is a variant of `pkg:…/foo@2.3.1`, so consumers that look up vulnerabilities and licences by upstream identity find the right ones.

**Why this priority**: it is valuable, and named in the issue, but it adds a third kind of change (adding information rather than removing it). Filtering, redaction and the chain stand without it.

**Independent Test**: declare a component a variant of an upstream PURL. Each format carries the relationship in its native field, and the parity check agrees across formats.

**Acceptance Scenarios**:

1. **Given** a component and an upstream PURL, **When** re-identified, **Then** CycloneDX records the upstream as its pedigree ancestor, SPDX 2.3 as a `VARIANT_OF` relationship, and SPDX 3 with its native equivalent.
2. **Given** declared patches, **When** re-identified, **Then** they are recorded where the format supports patches natively.

---

### Edge Cases

- **Dropping the document's root or subject component**: refused, with an error naming it, since the document would describe nothing.
- **A component in the middle of the dependency graph is dropped**: its dependents are bridged to its dependencies, and their dependency lists are marked incomplete (FR-005, FR-006). An edge that already exists is not duplicated. A bridge that would make a component depend on itself is not added.
- **A selector matches nothing**: the edit succeeds, and the report says the operation matched nothing.
- **A malformed selector or an unknown field class**: an error before anything is written.
- **Vulnerability and VEX entries about dropped components**: removed, or their affected list trimmed, so nothing points at a component that isn't there.
- **Format bookkeeping that only existed for dropped components**: removed rather than left dangling. Examples are SPDX 2.3 extracted-licence entries and SPDX 3 elements.
- **Completeness claims** (CycloneDX compositions, SPDX 3 relationship completeness): a dependency list that lost members no longer claims to be complete.
- **Cross-tier binding records** (m072) that hash the original: they stay as they were, true statements about the original. The derivation record is what links the two documents.
- **A document not produced by waybill**: accepted if it's one of the three supported formats. Operations act on standard fields. Operations on `waybill:` annotations find nothing. The cross-format agreement guarantee covers waybill-produced documents only.
- **What redaction cannot hide**: content hashes, version strings and the shape of the dependency graph can still identify a redacted component. This is documented (direction #5), and none of these are claimed as anonymised.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: waybill MUST provide a command that reads one SBOM (CycloneDX 1.6, SPDX 2.3 or SPDX 3.0.1, JSON), applies one or more edit operations, and writes an SBOM in the same format.
- **FR-002**: Content not targeted by an operation MUST be preserved unchanged. The only other changes allowed are:
  - the consequential ones required for consistency (FR-004, FR-006);
  - the derivation record (FR-009);
  - signatures (FR-011).
- **FR-003**: Operations MUST select components by any of:
  - package URL pattern;
  - ecosystem;
  - lifecycle scope;
  - SBOM tier;
  - component role;
  - name pattern.

  Several selectors in one operation combine with AND.
- **FR-004**: Dropping components MUST remove every reference to them:
  - dependencies and relationships;
  - vulnerability and VEX entries;
  - completeness claims;
  - format bookkeeping that existed only for them.

  The output MUST pass the same conformance checks waybill applies to the format on emission.
- **FR-005**: When a dropped component sat between others in the dependency graph, each of its dependents MUST gain a dependency on each of its dependencies (bridging). The bridged edge keeps the most restrictive lifecycle scope of the two edges it replaces. The graph stays connected, and no component becomes unreachable merely because something it was reached through was dropped. Bridging repeats across consecutive dropped components.
- **FR-006**: A dependency list that lost members through an edit MUST NOT keep a `complete` claim. It is downgraded to incomplete.
- **FR-007**: Annotations MUST be removable by namespace (for example every `waybill:` annotation, or a named class of them), without touching annotations of other namespaces.
- **FR-008**: Redaction MUST cover these field classes:
  - file paths;
  - URLs and hostnames matching a pattern;
  - component names matching a pattern.

  A redacted value MUST NOT remain anywhere in the output, including inside package URLs and references. The mode is chosen per field class:
  - **remove**: the value is deleted, or replaced with a fixed marker where the format requires a value;
  - **pseudonymise**: the value is replaced with a keyed pseudonym. The same input and key always give the same pseudonym, across documents. Without the key the original cannot be recovered or tested for. The key is supplied by the operator and never written to the output.

  Two distinct redacted values never collapse into one pseudonym.
- **FR-009**: Every edited document MUST carry a derivation record. It states that the document is a derivative, the hash of the original, which categories of operation were applied, and how many items each removed or changed. It MUST NOT contain any removed or redacted value.
- **FR-010**: The derivation link MUST be expressed in each format's native vocabulary where one exists, so a reader that knows the standard but not waybill can see it (Constitution Principle V).
- **FR-011**: When signing is requested, the output MUST be signed with the same signing options waybill offers on generation. It MUST also carry or reference the original's signature: an embedded signature, or a reference to its sidecar or transparency-log entry. An unsigned original MUST be recorded as unsigned.
- **FR-012**: waybill MUST provide a verification that checks an edited document's own signature and its derivation link. Given the original, it also checks the original's hash and signature. It follows a chain of several edits back to the first original, and reports which links it could check and which it could not.
- **FR-013**: Applying the same operations to the CycloneDX, SPDX 2.3 and SPDX 3 outputs of one scan MUST produce documents that still agree under waybill's cross-format parity check, apart from the removals themselves.
- **FR-014**: Every operation MUST be expressible as a self-contained declarative entry (selector + action + parameters), so a later policy file can drive the same operations without new semantics. The first release exposes them as command-line options.
- **FR-015**: Re-identification (US4) is out of scope for this milestone (Clarifications). The operation vocabulary (FR-014) MUST leave room for it as an action that adds information.
- **FR-016**: The command MUST report what each operation matched and changed, and MUST exit non-zero on an invalid operation or an unsupported input. A refused operation (dropping the root) writes nothing.
- **FR-017**: Documentation MUST state what redaction does not hide (content hashes, versions, graph shape) and that redaction is not anonymisation.

### Key Entities

- **Edit operation**: a selector, an action (drop component, drop annotations, redact field class; re-identify next milestone) and its parameters. The unit a policy file will list.
- **Selector**: criteria that choose components: package URL pattern, ecosystem, scope, tier, role, name pattern.
- **Derivation record**: document-level statement of derivation. It holds the original's hash, the categories of operation and the counts per category, and the original's signing status and signature reference. Never values.
- **Signature chain**: the sequence of derivation links from an edited document back to its first original, each link verifiable by hash and signature.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Removing dev- and test-scoped components from each of the three formats of a public-corpus scan gives outputs that all pass conformance validation, contain zero references to removed components, and still agree with each other under the parity check.
- **SC-002**: For every edit in the test matrix, the difference between input and output is limited to:
  - the targeted content;
  - the consequential consistency changes;
  - the derivation record;
  - signatures.

  Nothing else differs.
- **SC-003**: For a fixture containing known internal paths, hosts and names, a search of the redacted output for each original value returns zero matches, in all three formats.
- **SC-004**: Chain verification succeeds for every untampered pair in the test matrix. It fails for 100% of the tamper cases: the original altered, the derivative altered, a mismatched original, or a signature removed.
- **SC-005**: The derivation record names every operation category applied, and contains none of the removed or redacted values (checked by search).
- **SC-007**: After a bridged drop on a fixture whose graph has dropped intermediates, every component reachable from the root before the edit and not dropped is still reachable after it.
- **SC-008**: Pseudonymising the same values with the same key in two separate documents gives identical pseudonyms. A different key gives none of the same ones. No pseudonym equals any original value.
- **SC-006**: Editing a document the size of the largest public-corpus SBOM finishes in the same order of time as generating it. The baseline is measured during planning; the target is set as a ratio to it.

## Assumptions

- **Input is a waybill-produced document**, or a third-party document in a supported format. For third-party documents, the "unchanged" guarantee (FR-002) and the cross-format agreement (FR-013) apply to the fields waybill reads.
- **Signing reuses the existing machinery** (`--sign-key`, keyless `--sign`, CycloneDX embedded signature, sidecars for SPDX). How each format references the original's signature is settled in planning, by measurement against the validators.
- **SPDX 3's native equivalent of `VARIANT_OF`, and each format's native field for "derived from"**, are confirmed in planning against the bundled schemas and validators.
- **A file-in, file-out command, with no state between runs**: no import cache (unlike bomctl).
