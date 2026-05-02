# Feature Specification: Go direct-vs-indirect dependency tagging via `mikebom:dependency-kind`

**Feature Branch**: `058-go-direct-indirect-tagging`
**Created**: 2026-05-02
**Status**: Draft
**Input**: User description: closes #113 (Go ecosystem only); follow-up #104 will extend to npm / cargo / maven / pip / gem when those ecosystems gain main-module components. Surface trivy-equivalent direct-vs-indirect classification on every Go component so downstream consumers can answer "is this project's reliance on `x/y` direct or indirect?" without re-parsing the source manifest.

## Clarifications

### Session 2026-05-02

- Q: Per-component vs per-relationship tagging? → A: **Per-component** via the existing generic `extra_annotations` bag (catalog row C43 — same shape as C40 / C41). Per #113's design block: SPDX 2.3 supports relationship-level annotations but CDX 1.6 doesn't have a clean equivalent, and uniformity-across-formats beats SPDX-specific elegance.
- Q: Native-field audit per Constitution Principle V (v1.4.0)? → A: **No native field exists.** CDX 1.6's `scope` enum (`required`/`optional`/`excluded`) is about runtime inclusion, not direct-vs-indirect. SPDX 2.3 has typed `*_DEPENDENCY_OF` relationships but no `INDIRECT_DEPENDENCY_OF`. SPDX 3.0.1 similarly lacks the distinction. The `mikebom:dependency-kind` annotation is the finer-info carve-out per Principle V, mirroring C42's posture.
- Q: Which classifier wins when a Go module is BOTH explicitly direct AND only reached transitively via another module? → A: **Direct wins.** If the workspace `go.mod`'s `require` block lists the module without `// indirect`, classify as `direct`.
- Q: How does the main-module's own component get tagged? → A: **No tag.** The synthetic main-module entry represents the project itself, not a dependency. Absent annotation is the correct three-state value.
- Q: Other ecosystems? → A: **Out of scope for milestone 058.** Catalog row C43 is established here; npm / cargo / maven / pip / gem readers gain the same `mikebom:dependency-kind` population in follow-up milestones.

## Investigation findings

The Go reader's existing data model already carries the right information:
- `GoModRequire.indirect: bool` (`legacy.rs:151`) records the `// indirect` marker.
- `GoModDocument.requires: Vec<GoModRequire>` is the parsed workspace `go.mod` `require` block.
- `build_entries_from_go_module_with_lookup` (post-milestone 055) iterates `go.sum` entries and builds one `PackageDbEntry` per module. The classifier joins on module path: a `go.sum` entry whose path is in the workspace `go.mod`'s direct-require subset is `Direct`; everything else is `Indirect`.

Emission piggy-backs on the milestone 023 generic per-component annotation bag (`PackageDbEntry::extra_annotations`). The existing CDX builder + SPDX 2.3 emitter + SPDX 3 emitter all surface that bag uniformly without per-format wiring (`generate/cyclonedx/builder.rs:539`, `generate/spdx/annotations.rs:263`, `generate/spdx/v3_annotations.rs:275`). Adding C43 means populating one extra entry in `extra_annotations`; emission is automatic.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Vulnerability triage based on direct vs indirect (Priority: P1)

A security engineer reading a mikebom-generated SBOM for a Go project wants to filter components by whether the project depends on them directly (workspace `go.mod` declares them) or only transitively. A CVE in a direct dep is a code-change-required upgrade; a CVE in an indirect dep can sometimes be force-resolved without code changes via `replace` directives. Today every component looks identical post-emission and consumers re-parse `go.mod` to recover the distinction.

**Why this priority**: Closes the documented trivy-parity gap from milestone 053's research. Without it, mikebom's SBOMs are strictly less useful than trivy's for this widely-used triage workflow.

**Independent Test**: Construct a Go workspace fixture whose `go.mod` declares one direct require + one `// indirect` require + a `go.sum` containing a third module reached only transitively. Assert the resulting CDX/SPDX components carry the right `mikebom:dependency-kind` per FR-001.

**Acceptance Scenarios**:

1. **Given** a Go workspace with `require ( github.com/foo/bar v1.0.0 )` (no `// indirect`), **When** mikebom scans, **Then** the `pkg:golang/github.com/foo/bar@v1.0.0` component carries `mikebom:dependency-kind: "direct"` in CDX `properties[]`, SPDX 2.3 `annotations[]`, and SPDX 3 `annotations[]`.
2. **Given** a Go workspace with `require ( github.com/foo/bar v1.0.0 // indirect )`, **When** mikebom scans, **Then** the same component carries `mikebom:dependency-kind: "indirect"` across all three formats.
3. **Given** a `go.sum` module not in the workspace `go.mod`'s require block (purely transitive), **When** mikebom scans, **Then** the component carries `mikebom:dependency-kind: "indirect"`.
4. **Given** the synthetic main-module component (milestone 053), **When** mikebom scans, **Then** that component carries NO `mikebom:dependency-kind` annotation.
5. **Given** a workspace where `replace foo v1.0.0 => bar v2.0.0` is in effect and `bar` is in the workspace `require` block as direct, **When** mikebom scans, **Then** the `bar` component is `direct`.

### Edge Cases

- **Go module with no `go.mod` at workspace root** (binary-only scan): no main-module component, no workspace require block. Binary scan codepath uses BuildInfo; C43 is moot.
- **Multi-project rootfs scan** (two `go.mod` files): each project's classifier runs independently. A module that's `direct` in project A and `indirect` in project B gets DIFFERENT classifications in each project's components — correct because components are emitted per-project.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: For every `pkg:golang/...` component emitted from a `go.sum` entry, mikebom MUST set `extra_annotations["mikebom:dependency-kind"]` to either `"direct"` or `"indirect"`. Classification: `"direct"` iff the module's path appears in the workspace `go.mod`'s `require` block AND the corresponding `GoModRequire.indirect == false`; `"indirect"` otherwise.

- **FR-002**: The synthetic main-module component MUST NOT carry the `mikebom:dependency-kind` annotation. Absence is the correct three-state value (matches C40/C41/C42 conventions).

- **FR-003**: Emission MUST be automatic via the existing milestone 023 generic per-component annotation bag.

- **FR-004**: Catalog row C43 MUST be added to `docs/reference/sbom-format-mapping.md` documenting the native-field audit and the open-enum semantics.

- **FR-005**: The parity-extractor framework MUST gain a C43 row exercising symmetric cross-format extraction. C43 is `SymmetricEqual` directionality.

- **FR-006**: Milestone 058 covers the **Go ecosystem only**. Per-ecosystem extension is OUT OF SCOPE.

- **FR-007**: Existing 27 byte-identity goldens MUST be regenerated to absorb the new annotation on Go components. Other ecosystems' goldens stay byte-identical.

- **FR-008**: The pre-PR gate MUST pass.

### Key Entities

- **`mikebom:dependency-kind` annotation**: an open-enum string property on each `pkg:golang/...` component. Values: `"direct"`, `"indirect"`. Absent on main-module and on non-Go components in milestone 058.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A synthesized Go fixture with mixed direct + `// indirect` + purely-transitive modules produces components with the correct `mikebom:dependency-kind` per FR-001, asserted in unit tests for all three output formats.
- **SC-002**: Realistic-project CI gate passes against `knative/func` post-058.
- **SC-003**: Goldens for the existing 9-ecosystem fixtures regenerate cleanly. Non-Go fixtures stay byte-identical.
- **SC-004**: Pre-PR gate passes.

## Assumptions

- **The milestone 053 main-module path is the right place to skip C43**: `build_main_module_entry` doesn't iterate `go.sum`. The classifier lives in `build_entries_from_go_module_with_lookup` which only handles `go.sum` entries — main-module is naturally exempt.
- **The generic `extra_annotations` emitter is feature-complete** per the C40/C41 precedent.
- **Out of scope**: per-ecosystem extension, relationship-level variant.
