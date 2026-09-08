# Feature Specification: Component Source-Provenance References

**Feature Branch**: `776-component-source-refs`
**Created**: 2026-09-05
**Status**: Draft
**Input**: User description: "Populate component `externalReferences` from already-fetched deps.dev links (SOURCE_REPO, ISSUE_TRACKER, DOCUMENTATION) and add deterministic PURL-derived distribution URLs, so emitted components carry source-provenance references across all ecosystems instead of only golang."

## Clarifications

### Session 2026-09-05

- Q: Live sampling shows the enrichment service emits five link labels today — `SOURCE_REPO` (30/30), `ORIGIN` (30/30), `HOMEPAGE` (25/30), `ISSUE_TRACKER` (21/30), `ATTESTATION` (20/30) — not the three the spec anticipated. `ORIGIN` and `ATTESTATION` are present on most components right now, so FR-003's omit-unrecognized rule would silently discard them. Which labels are mapped? → A: **Option B** — map five. Add `HOMEPAGE` → website-kind and `ATTESTATION` → attestation-kind, both of which the target format natively defines and both of which are unambiguous from the label. Defer `ORIGIN` alone: its semantics are not determinable from the label, and guessing would violate FR-003's own principle. Rationale for including `ATTESTATION` specifically: it points at upstream build provenance, which is load-bearing for this project's attestation-first posture — discarding it is a substantive loss, not a neutral default.
- Q: FR-003 requires unrecognized labels be skipped without a per-occurrence warning (correct — that would be per-component noise), but the spec adds no aggregate signal either, so nothing reports how many references were emitted or how many links were skipped. Add aggregate observability, count emissions only, or none? → A: **Option A** — emit one aggregate summary per scan reporting references emitted by kind plus a count of links skipped as unrecognized. Rationale: Q1 demonstrated the failure mode directly — two labels present on most components were being silently discarded by a rule written for hypothetical future labels, and that was only caught by hand-probing the service. A skipped-label count makes vocabulary drift visible automatically, and gives SC-001/SC-002 an in-product check instead of requiring external instrumentation. This mirrors the counter added in the immediately preceding milestone, where a defect survived a full cycle because nothing counted the thing.

## Motivation

An SBOM consumer asking "where did this component come from, and where do I go to inspect or report against it?" currently gets no answer from waybill output for most ecosystems. That question is the point of a source-provenance reference, and every major SBOM format has a native field for it.

### Observed state

A quality-scoring pass over five ecosystems (`go-cobra`, `py-uv`, `rust-ripgrep`, `npm-nodejs`, `maven-jvm`, scanned with enrichment enabled) measured the per-component source-reference coverage waybill emits today:

| Fixture | Components carrying a source reference | Coverage |
|---|---|---:|
| go-cobra | 4 of 7 | 6.2 / 10 |
| py-uv | ~1 of 109 | 0.1 / 10 |
| rust-ripgrep | ~1 of 68 | 0.1 / 10 |
| npm-nodejs | 0 of 369 | 0.0 / 10 |
| maven-jvm | ~1 of 111 | 0.1 / 10 |

A comparison generator scored materially higher on the Python fixture (emitting repository and homepage references where waybill emitted none), and lower or equal elsewhere. The gap is real but narrow and specific: it is about *source-provenance references*, not about component discovery — waybill enumerates equal or more components than the comparison tool on four of the five fixtures.

### Two independent causes

**Cause 1 — already-fetched data is discarded.** On every enrichment-enabled scan, waybill queries a package-metadata service for each component and receives a `links` collection alongside the license data it already consumes. That collection carries labelled entries such as:

```
SOURCE_REPO     https://github.com/pallets/flask
ISSUE_TRACKER   https://github.com/pallets/flask/issues/
DOCUMENTATION   https://flask.palletsprojects.com/
```

waybill parses these into memory and then never reads them. The behavior is acknowledged in a source comment: the enrichment payload "drives license enrichment but `advisory_keys` / `links` aren't yet" consumed. **No additional network requests, and no new data source, are required to fix this** — the information is already retrieved and discarded on every scan.

**Cause 2 — the offline derivation path covers four ecosystems, and mostly with the wrong reference kind.** waybill separately derives references from a component's package identifier alone, with no network access. That derivation covers `cargo`, `golang`, `nuget`, and `maven` — and the `maven` arm is further gated so that it fires only for components discovered inside nested archives. Every other ecosystem (`pypi`, `npm`, `gem`, `composer`, and the rest) has no derivation at all.

Worse, of the four covered ecosystems only `golang` emits a *repository* reference. The other three emit a *registry landing page*, which is a different kind of reference and does not answer the source-provenance question. This is why `rust-ripgrep` scores 0.1 despite carrying 61 references: they are all the wrong kind.

### Why this is worth doing

The reference kinds involved are natively defined by every target format — no vendor-specific extension is introduced or needed. Consumers use them for source auditing, vulnerability triage, and license verification. The fix for Cause 1 is a mapping over data already in memory; the fix for Cause 2 is deterministic string derivation requiring no network.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Source-provenance references from enrichment metadata (Priority: P1)

An operator scans a project in any ecosystem the enrichment service supports, with enrichment enabled (the default). Each component that the service has repository, issue-tracker, or documentation information for carries corresponding references in the emitted SBOM, so a consumer can navigate from a component to its upstream source.

**Why this priority**: This is the milestone's value. It resolves the gap uniformly across every enrichment-supported ecosystem in one place, rather than ecosystem by ecosystem, and it consumes data waybill already pays to fetch. It requires no new network traffic, no new data source, and no new configuration.

**Independent Test**: Scan the Python fixture with enrichment enabled and confirm components carry repository references where the enrichment service supplies them. Measurable via SC-001. Verifiable without US2.

**Acceptance Scenarios**:

1. **Given** a component for which the enrichment service supplies a repository link, **When** the operator runs a scan with enrichment enabled, **Then** the emitted component carries a reference of the repository kind pointing at that URL.
2. **Given** a component for which the service supplies issue-tracker, documentation, homepage, and attestation links, **When** the scan runs, **Then** the component carries references of the corresponding kinds, each with the correct URL and each using a natively-defined kind.
3. **Given** a component for which the service supplies an origin-labelled link, **When** the scan runs, **Then** no reference is emitted for it, and the component's other references are unaffected (Clarifications Q1).
4. **Given** a component for which the service supplies a link whose label waybill does not recognize, **When** the scan runs, **Then** that link is omitted rather than guessed at, and the component's other references are unaffected.
5. **Given** a component for which the service supplies no links at all, **When** the scan runs, **Then** the component carries no enrichment-derived references and the scan completes normally.
6. **Given** a component that already carries a reference of a given kind and URL from another source, **When** enrichment supplies the same kind and URL, **Then** the emitted component carries that reference exactly once.
7. **Given** an operator scanning with enrichment disabled, **When** the scan runs, **Then** no enrichment-derived references appear, and output is otherwise unchanged from pre-milestone.
8. **Given** a scan over components whose enrichment metadata includes both mapped and unmapped link labels, **When** the scan completes, **Then** a single aggregate summary reports references emitted per kind and links skipped as unmapped, and the reported per-kind counts equal the references present in the emitted document.

---

### User Story 2 - Offline-derivable distribution references (Priority: P2)

An operator scans without network access. Components in ecosystems whose distribution URL is fully determined by the component's package identifier carry a distribution reference, so an air-gapped or offline scan still yields a retrievable artifact location.

**Why this priority**: Independently valuable and independently testable, but narrower than US1: it applies only to ecosystems whose distribution URL is derivable from the identifier alone, and it does not supply repository information. It also protects the offline path, which US1 cannot reach by construction. P2 because US1 carries the measured value; US2 can ship alongside or separately.

**Independent Test**: Scan a fixture with network access disabled and confirm components carry distribution references. Verifiable without US1.

**Acceptance Scenarios**:

1. **Given** a component in an ecosystem whose distribution URL is fully determined by its package identifier, **When** the operator scans with network access disabled, **Then** the component carries a distribution reference at the correct URL.
2. **Given** a component in an ecosystem whose distribution URL is NOT determined by the identifier alone, **When** the operator scans offline, **Then** no distribution reference is fabricated for it.
3. **Given** a component whose identifier lacks a version, **When** the scan runs, **Then** no distribution reference is emitted, because the URL cannot be formed correctly.
4. **Given** a component that today receives a registry-landing-page reference, **When** this milestone lands, **Then** that existing reference is preserved and the distribution reference is added alongside it rather than replacing it.
5. **Given** an ecosystem that currently receives no derived references at all, **When** the scan runs, **Then** it receives a distribution reference if and only if its URL is identifier-derivable.

---

### Edge Cases

- **Malformed URL from the enrichment service**: a link whose URL is empty or not a well-formed absolute URL must be omitted rather than emitted, so the SBOM never carries an unusable reference.
- **Duplicate links from the service**: the same kind and URL supplied twice must produce one reference, not two.
- **Same URL under different labels**: if the service reports the same URL as both repository and documentation, both references are emitted — they are different claims about the same location, and consumers filter by kind.
- **Enrichment service reachable but returns no metadata for a component**: no references, no warning noise, scan proceeds.
- **Components with no package identifier** (file-tier entries, diagnostic entries): no derived references; these are not package coordinates.
- **Operator-supplied references**: an operator supplying references through the existing supplement mechanism must not have them dropped or reordered by this milestone.
- **Identifier containing characters requiring encoding** (scoped or namespaced package names): the derived URL must be correctly formed rather than naively concatenated.
- **An ecosystem whose registry URL scheme changes upstream**: derived URLs are a point-in-time convention; the milestone does not attempt to detect or adapt to upstream registry changes.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When enrichment is enabled and the enrichment service supplies a repository link for a component, the system MUST emit a repository-kind reference for that component carrying the supplied URL.
- **FR-002**: The system MUST likewise emit references for every other link label in the mapped set, each to its corresponding natively-defined kind. Per Clarifications Q1 the mapped set is five labels: repository, issue-tracker, documentation, homepage (to the website kind), and attestation (to the attestation kind). Each mapped kind MUST be one the target format natively defines.
- **FR-002a**: The system MUST NOT map the service's origin-labelled link. Its meaning is not determinable from the label, and assigning it a kind would be the guess FR-003 exists to prevent. It is treated as an unrecognized label until its semantics are confirmed upstream.
- **FR-003**: The system MUST omit links whose label it does not recognize, rather than guessing a reference kind. Unrecognized labels MUST NOT cause the scan to fail or emit a warning per occurrence.
- **FR-004**: The system MUST omit links whose URL is empty or not a well-formed absolute URL.
- **FR-005**: Every reference kind the system emits MUST be one the target formats natively define. This milestone MUST NOT introduce a vendor-prefixed property to carry source-provenance information.
- **FR-006**: The system MUST NOT emit duplicate references: two references with the same kind and the same URL on one component MUST be collapsed to one.
- **FR-007**: The system MUST NOT issue any additional network request to satisfy FR-001 through FR-004. The links consumed MUST come from metadata the scan already retrieves.
- **FR-008**: When enrichment is disabled or the enrichment service is unreachable, the system MUST emit no enrichment-derived references and MUST complete the scan normally.
- **FR-009**: The system MUST emit a distribution-kind reference for a component whose distribution URL is fully determined by its package identifier, without network access.
- **FR-010**: The system MUST NOT emit a distribution reference when the URL cannot be determined from the identifier alone, including when the identifier lacks a version.
- **FR-011**: The system MUST preserve every reference it emits today. Existing registry-landing-page references MUST remain; new references are additive.
- **FR-012**: The system MUST preserve operator-supplied references from the existing supplement mechanism, unmodified and un-reordered.
- **FR-013**: References MUST be emitted in a deterministic order, so that two scans of the same input produce identical output.
- **FR-014**: The system MUST NOT introduce new operator-facing configuration — no new flags, no new environment variables.
- **FR-014a**: Per Clarifications Q2 the system MUST emit exactly one aggregate summary per scan reporting the number of source-provenance references emitted, broken down by reference kind, and the number of enrichment links skipped because their label is not in the mapped set. The summary MUST be emitted once per scan regardless of component count, and MUST NOT produce per-component or per-link output.
- **FR-014b**: The skipped-link count MUST distinguish links skipped for an unmapped label from links skipped for a malformed URL (FR-004), so that vocabulary drift is not conflated with upstream data quality.
- **FR-015**: The system MUST NOT introduce new third-party dependencies.
- **FR-016**: References MUST appear in all three emitted formats wherever each format natively supports them, consistent with existing cross-format parity treatment for this field.

### Non-Functional Requirements

- **NFR-001**: The added work MUST NOT measurably increase scan wall time, since it consumes data already in memory and performs string derivation only. Verified by SC-006.
- **NFR-002**: A component whose enrichment metadata is malformed MUST NOT abort the scan or drop that component from output; the component is emitted without enrichment-derived references.

### Key Entities

- **Source-provenance reference**: a typed pointer from a component to an external location — repository, issue tracker, documentation, or distribution artifact. Each carries a kind and a URL. Natively supported by all three target formats.
- **Enrichment link**: a labelled URL supplied by the package-metadata service alongside license data. Already retrieved and parsed on every enrichment-enabled scan; currently discarded. Labels are a service-defined vocabulary; four are mapped onto natively-defined reference kinds and the rest — including the origin label — are skipped (Clarifications Q1).
- **Mapping summary**: a per-scan aggregate of how the enrichment link vocabulary was handled — references emitted per kind, links skipped as unmapped, links skipped as malformed. Its purpose is to make vocabulary drift and coverage observable in-product rather than by external probing.
- **Identifier-derived distribution URL**: a distribution location computed from a component's package identifier alone, with no network access, for ecosystems where the registry URL scheme makes this deterministic.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the Python fixture, the proportion of components carrying a source-provenance reference rises from approximately 1 in 109 to at least 80%, when scanned with enrichment enabled.
- **SC-002**: On the JavaScript fixture, the proportion rises from 0 of 369 to at least 80% under the same conditions.
- **SC-003**: Across the five-ecosystem measurement set, no fixture's source-reference coverage decreases relative to its pre-milestone value.
- **SC-004**: With network access disabled, at least one ecosystem that today emits no references emits distribution references for the majority of its components.
- **SC-005**: Two scans of the same input produce byte-identical output, modulo document-identity fields.
- **SC-006**: Scan wall time on the largest measurement fixture is within 3% of its pre-milestone value.
- **SC-007**: Zero new third-party dependencies.
- **SC-008**: The project's mandatory pre-PR verification passes: zero lint errors, and every test suite reporting all tests passed with none failed.
- **SC-009**: Every reference kind emitted is natively defined by the target format; no vendor-prefixed property is introduced for source-provenance information.
- **SC-009a**: Scanning a multi-ecosystem fixture reports the per-kind reference counts and the skipped-link counts in a single aggregate summary, and those counts match the references actually present in the emitted document. Verifiable in the project's automated checks without external instrumentation.
- **SC-010**: Output differences relative to the pre-milestone baseline are confined to added references. No component, relationship, license, or other annotation changes.

## Assumptions

- Enrichment is enabled by default, and operators who disable it accept reduced metadata. US1 therefore does not apply to explicitly-offline scans; US2 exists to cover that path.
- The enrichment service's link-label vocabulary is stable enough to map from. Five labels were observed across a live 30-component sample (repository 30/30, origin 30/30, homepage 25/30, issue-tracker 21/30, attestation 20/30); four are mapped and origin is deferred per Clarifications Q1. Unrecognized labels are skipped rather than guessed (FR-003), so vocabulary additions upstream degrade to no-ops rather than to wrong references. Confirming origin's semantics upstream, and mapping it if warranted, is a candidate follow-up.
- The enrichment service is a best-effort metadata source, not an authority. A component without links is normal, not an error.
- Reference *kind* correctness matters more than reference *count*. The observed rust-ripgrep case — 61 references, near-zero source-provenance coverage — is the motivating example: registry landing pages are not source references. This milestone adds correct kinds rather than more references.
- Distribution-URL derivability varies by ecosystem and is a property of each registry's URL scheme, not of waybill. Ecosystems where the URL requires registry metadata (a content hash, for example) are out of scope for US2 by construction.
- The measurement set is five fixtures spanning Go, Python, Rust, JavaScript, and JVM ecosystems, scanned on one host. It is a coverage signal, not a statistical sample.
- The scoring tool used to quantify the gap is a measurement instrument, not a specification. Requirements here are stated against the emitted SBOM's native fields; any score movement is a consequence, not a goal. Where the tool's conventions and the format specifications disagree, the specifications govern.
- Adding references is additive to existing output. Fixtures whose stored expected output does not yet include them will require review and regeneration, and that diff is expected to consist solely of added references (SC-010).
