# Feature Specification: Say why deps.dev did not enrich a component

**Feature Branch**: `1067-depsdev-enrichment-outcome`
**Created**: 2026-10-04
**Status**: Draft
**Input**: Issue #1058, "deps.dev: a component with no enrichment cannot say why — not queried, absent, or declined", plus measurements in `measurements/` (2026-10-04).

## Background (measured, not assumed)

waybill asks deps.dev for licence and metadata about each component in the six ecosystems deps.dev indexes (cargo, npm, pypi, go, maven, nuget). When a component gets nothing, the document looks the same whatever the reason:

1. **Not queried**: the ecosystem is not indexed by deps.dev, the coordinate is incomplete (no real version), or the scan was offline with no cached answer.
2. **Queried, absent**: deps.dev has no record (HTTP 404).
3. **Queried, declined**: deps.dev returned a record but waybill rejected its contents, e.g. `licenses: ["non-standard"]`, which is not a valid SPDX expression.
4. **Transport failure**: counted in the document-level degradation record (C158), but not attributed to the component.

Case 2 is where silent zero-coverage bugs hide. Three past defects each zeroed out a whole ecosystem while looking like normal operation, because a malformed query returns 404 and 404 reads as "no data": maven queried by artifact only, Go paths doubled, npm scopes doubled.

Measured on six public-corpus repositories, online (`measurements/README.md`):

| repo | components | queried | record found | queried, nothing usable | never queried |
|---|---:|---:|---:|---:|---:|
| express (npm) | 369 | 369 | 369 | 0 | 0 |
| flask (pypi) | 108 | 108 | 105 | 3 | 0 |
| opentelemetry-go | 334 | 333 | 303 | **30** | 1 |
| guice (maven) | 109 | 46 | 43 | 3 | **63** |

In opentelemetry-go, **28 of the 30 misses are the project's own modules at the placeholder version `v0.0.0-unknown`**. waybill asks deps.dev about a version that cannot exist, and the 404 looks exactly like "deps.dev has no data". The issue's failure shape occurs today in a real repository.

## Clarifications

### Session 2026-10-04

- Q: Where does the per-component reason live? → A: Both. A per-component annotation goes on each component deps.dev did not enrich, and a document-level count per reason covers the scan. Enriched and matched components carry nothing, so a fully enriched scan is unchanged.
- Q: For "declined", does the document name what was rejected? → A: The reason code only (e.g. `declined-invalid-license`). No deps.dev content is copied into the document; the rejected value goes to the log.
- Q: Do offline scans, or scans with deps.dev turned off, carry per-component reasons? → A: No. When the deps.dev pass did not query the network, components carry nothing, and existing document-level signals say enrichment was offline or off. Offline goldens stay byte-identical.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A consumer can tell why a component has no deps.dev data (Priority: P1)

A security engineer reviewing an SBOM finds a component with no licence. Today they cannot tell whether waybill never asked, asked and deps.dev had nothing, deps.dev had something waybill rejected, or the request failed. With this feature, the document says which, so they know whether to chase deps.dev, waybill, or their network.

**Why this priority**: the issue's core gap. It also turns waybill's own silent-miss bugs, like the placeholder-version queries measured above, into visible signals.

**Independent Test**: scan a project whose dependencies include one deps.dev knows, one it does not (404), one with an unsupported ecosystem, and one whose record carries an invalid licence. Each un-enriched component's outcome is recorded with the right reason, in all three formats.

**Acceptance Scenarios**:

1. **Given** a component deps.dev returns 404 for, **When** scanned online, **Then** the document records `absent` for it.
2. **Given** a component whose deps.dev record's only licence is `non-standard`, **When** scanned, **Then** the document records `declined-invalid-license`, and the value `non-standard` appears nowhere in the document.
3. **Given** a component in an ecosystem deps.dev does not index, **When** scanned, **Then** it is recorded as not queried, with that reason.
4. **Given** a transport failure for a component, **When** scanned, **Then** it is recorded as `transport-failure`, and the existing document-level degradation record (C158) is unchanged.

---

### User Story 2 - waybill stops asking deps.dev about versions that cannot exist (Priority: P1)

The scanned project's own modules often carry placeholder versions (`v0.0.0-unknown` for Go workspace modules). waybill queries deps.dev for them today, and the 404s look like data gaps. With this feature they are not queried, and are recorded as not queried for an incomplete coordinate.

**Why this priority**: it is the measured case. On opentelemetry-go, 28 of 30 misses are this, and they would otherwise dominate the new signal with noise.

**Independent Test**: a Go workspace with a `v0.0.0-unknown` main module. No deps.dev request is made for it, and it is recorded as not queried (`incomplete-coordinate`).

**Acceptance Scenarios**:

1. **Given** a component whose version is a known placeholder, **When** scanned online, **Then** no deps.dev request is sent for it, and its outcome is `not-queried` with reason `incomplete-coordinate`.

---

### User Story 3 - Scans where every component was enriched are unchanged (Priority: P1)

A scan where deps.dev matched every queried component and no other component is un-enriched gains nothing, and nor does any offline or deps.dev-disabled scan.

**Independent Test**: express (369 queried, 369 found) carries no per-component reason. Every offline corpus golden and in-repo golden is byte-identical.

### Edge Cases

- **Already licensed from the lockfile, so not queried** (the likely guice case, to be confirmed in the plan): recorded as `not-needed`, not as a gap. Not having asked is fine when the licence is already known.
- **Matched, but nothing new added** (express: 369 matched, 4 enriched): not a gap. The record was used and confirmed nothing, so there is no per-component reason.
- **Cache hits:** a cached positive is `matched`. A cached negative from an earlier 404 is `absent`, the same as live.
- **Both passes:** if the initial and post-graph passes reach different outcomes for one component, the later pass's outcome wins. A component is never recorded twice.
- **Nested or shaded components** follow the same rules as top-level ones.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: For each component the deps.dev pass considered and did not enrich, waybill MUST classify the outcome into a closed set:
  - `not-queried`, with a reason: `unsupported-ecosystem`, `incomplete-coordinate` or `not-needed`;
  - `absent`;
  - `declined`;
  - `transport-failure`.
- **FR-002**: The classification MUST appear in the document in two forms:
  - a per-component annotation on every component with a non-matched outcome, whose value is the outcome, e.g. `absent` or `not-queried:unsupported-ecosystem`;
  - a document-level count per outcome, emitted iff at least one component has one.

  Both MUST be identical in CycloneDX, SPDX 2.3 and SPDX 3, and parity-catalogued as new rows. Components that are enriched or matched carry nothing (FR-003).
- **FR-003**: A component deps.dev enriched, or matched without adding anything, MUST carry no per-component outcome.
- **FR-004**: waybill MUST NOT send a deps.dev request for a coordinate whose version is a known placeholder (at least `v0.0.0-unknown`; the plan enumerates the set). Such a component's outcome is `not-queried` / `incomplete-coordinate`.
- **FR-005**: A deps.dev 404 MUST be distinguished from a transport failure, as the ClearlyDefined path already does. The two MUST NOT share an outcome.
- **FR-006**: `declined` MUST be recorded as a reason code from a closed set (at least `declined-invalid-license`). No deps.dev content may be copied into the document. The rejected value MAY appear in the log.
- **FR-007**: When the deps.dev pass made no network request for the scan (`--offline`, or deps.dev disabled), no component MAY carry a per-component outcome and no document-level count is emitted. Existing document-level signals already describe that mode.
- **FR-008**: The existing document-level degradation record (C158) and the per-pass log line MUST keep their current meaning and values.
- **FR-009**: Scans in which no component is left un-enriched (FR-003), and all offline or deps.dev-disabled scans (FR-007), MUST produce byte-identical output to the output before this feature. FR-004's skipped request changes no output by itself.

### Key Entities

- **Enrichment outcome**: per component and per scan. Its states are `matched`, `not-queried{reason}`, `absent`, `declined` and `transport-failure`; only the non-matched states are emitted.
- **Placeholder version**: a version string that denotes "unknown" rather than a release, such as `v0.0.0-unknown`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a fixture covering the four outcome kinds, 100% of un-enriched components carry the correct outcome in all three formats.
- **SC-002**: On opentelemetry-go (the cached corpus checkout, online), the 28 placeholder-version modules send 0 deps.dev requests and are recorded `incomplete-coordinate`. The measured baseline is 28 requests that all 404.
- **SC-003**: A deps.dev 404 and a transport failure for the same coordinate produce different outcomes (FR-005).
- **SC-004**: Every public-corpus golden and in-repo golden is byte-identical (they are produced offline; FR-007).
- **SC-005**: On the cached opentelemetry-go checkout online, the document-level counts equal the per-component annotations tallied by outcome.

## Assumptions

- **Scope:** deps.dev only. ClearlyDefined is opt-in since #930 and already separates 404 from transport failure; extending this signal to it is a follow-up.
- **Cache record format:** the disk cache may need to store a negative (404) result to answer "absent" from cache. That is a plan concern; a format change must stay readable for existing caches, which are treated as misses.
- **`not-needed` reason:** the exact skip rule behind guice's never-queried maven components is to be confirmed by plan research. The spec only requires that a skip by design is not reported as a gap.
