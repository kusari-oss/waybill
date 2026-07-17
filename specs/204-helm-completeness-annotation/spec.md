# Feature Specification: Emit `mikebom:image-extraction-completeness` Document-Scope Annotation

**Feature Branch**: `204-helm-completeness-annotation`
**Created**: 2026-07-17
**Status**: Draft
**Input**: Issue #554 — m188 follow-up: emit `mikebom:image-extraction-completeness` document-scope annotation across CDX 1.6 / SPDX 2.3 / SPDX 3.

## Background

m188 landed the helm chart reader with two extraction modes tracked in `ScanDiagnostics.helm_extraction_mode: Option<HelmExtractionMode>`:

- `Unrendered` — line-based regex extraction over Chart templates. Template placeholders like `{{ .Values.image.repository }}:{{ .Values.image.tag }}` surface as `pkg:generic/` components carrying `mikebom:image-ref-unresolved = "true"`. Reduced fidelity.
- `Rendered` — m203 `--helm-render` shelled out to `helm template` and captured concrete image refs. Full fidelity.

The reader-side plumbing captures the mode. The emitter-side plumbing that surfaces it to SBOM consumers was deferred from m188 to keep the task budget focused on the reader. Result today: an operator running mikebom against a helm chart cannot tell from the emitted SBOM whether they got the reduced-fidelity or the full-fidelity image-ref set — they'd have to inspect per-component `mikebom:image-ref-unresolved` markers and infer.

m204 wires the three emitters (CDX 1.6, SPDX 2.3, SPDX 3) to surface a document-scope `mikebom:image-extraction-completeness` annotation driven by that state.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Helm operator sees fidelity marker on default (unrendered) scan (Priority: P1)

An operator scans a Helm chart WITHOUT `--helm-render`. The emitted SBOM (CDX 1.6, SPDX 2.3, or SPDX 3) carries a document-scope annotation `mikebom:image-extraction-completeness = "partial"`, telling downstream consumers that image-ref extraction used the line-based regex path — some image refs may be unresolved placeholders.

**Why this priority**: This is the default path for every helm scan today. Every m188 user is affected. Without this annotation, operators cannot distinguish "no helm-image-ref fidelity concerns" from "reduced fidelity — check for `mikebom:image-ref-unresolved` markers" without inspecting every component. P1 because it's the majority path and the fidelity information is otherwise opaque.

**Independent Test**: Run `mikebom sbom scan --path <helm-chart-dir> --format cyclonedx-json` on any chart with a `Chart.yaml`. Assert the CDX output contains `metadata.properties[]` with `{name: "mikebom:image-extraction-completeness", value: "partial"}`. Repeat for `--format spdx-json` (SPDX 2.3 doc-scope `Annotation`) and `--format spdx-3-json` (SPDX 3 `Annotation` element). No further plumbing required.

**Acceptance Scenarios**:

1. **Given** a helm chart directory with `Chart.yaml` present, **When** the operator scans without `--helm-render`, **Then** the CDX SBOM's `metadata.properties[]` contains `mikebom:image-extraction-completeness = "partial"`.
2. **Given** the same chart, **When** the operator emits SPDX 2.3, **Then** the document-scope `annotations[]` contains an `Annotation` whose `comment` decodes to a `mikebom:image-extraction-completeness = "partial"` payload following the m071 parity envelope shape.
3. **Given** the same chart, **When** the operator emits SPDX 3, **Then** the JSON-LD graph contains an `Annotation` element with `subject` pointing at the SpdxDocument and `statement` carrying the same payload.

---

### User Story 2 - Helm operator sees fidelity marker on rendered scan (Priority: P1)

An operator scans a Helm chart WITH `--helm-render` and helm template succeeds. The emitted SBOM carries `mikebom:image-extraction-completeness = "full"` — signaling that image refs are the concrete post-render values, not template placeholders.

**Why this priority**: This is the m203 opt-in success path. Distinguishing rendered from unrendered downstream is the whole reason m203 shipped. If both modes emitted no annotation (or the same annotation), the m203 investment is opaque to consumers. P1 because m204 is what makes m203 observable end-to-end.

**Independent Test**: Requires a real `helm` binary. Run `mikebom sbom scan --helm-render --path <helm-chart-dir> --format cyclonedx-json`. Assert `metadata.properties[]` contains `mikebom:image-extraction-completeness = "full"`. Gated behind `MIKEBOM_HELM_INTEGRATION=1` per m188/m203 precedent.

**Acceptance Scenarios**:

1. **Given** a helm chart + local `helm` binary, **When** the operator passes `--helm-render` and helm succeeds, **Then** all three emitted formats carry `mikebom:image-extraction-completeness = "full"`.
2. **Given** `--helm-render` set but helm fails (any fallback class from m203: `BinaryNotFound`, `NonZeroExit`, `Timeout`, `IoError`), **When** mikebom falls back to unrendered extraction, **Then** the emitted annotation is `"partial"` (not `"full"`) — the value reflects the actual extraction path, not the requested mode.

---

### User Story 3 - Non-Helm scan sees no annotation (byte-identity guard) (Priority: P1)

An operator scans a non-Helm project (any directory without `Chart.yaml` at any depth the helm reader touches). The emitted SBOM contains NO `mikebom:image-extraction-completeness` annotation at all.

**Why this priority**: FR-016-analog to m203 — this annotation MUST be gated on `helm_extraction_mode.is_some()` so that non-Helm scans see zero drift. Without this guard, every non-Helm SBOM regresses by one property/annotation, breaking existing golden tests + downstream consumer contracts. P1 because it's a hard byte-identity requirement.

**Independent Test**: Scan any non-Helm project (e.g., an existing test fixture without any `Chart.yaml`). Assert the emitted SBOM contains no property/annotation whose name equals `mikebom:image-extraction-completeness` in any of the three formats. `git diff --stat mikebom-cli/tests/fixtures/` post-implementation should show zero pre-existing golden drift.

**Acceptance Scenarios**:

1. **Given** a directory with no `Chart.yaml`, **When** the operator scans in any of the three formats, **Then** the emitted document contains zero `mikebom:image-extraction-completeness` annotations.
2. **Given** the pre-m204 goldens for every non-Helm fixture in the mikebom test suite, **When** m204's implementation is applied, **Then** none of those goldens change (byte-identical output).

---

### Edge Cases

- **Helm scan where extraction produced zero image refs at all** (e.g., a chart with no templates directory) — `helm_extraction_mode` is still `Some(Unrendered)` because the reader ran. Annotation is emitted with value `"partial"`. This is correct: absence of image refs isn't the same as absence of extraction; the operator should see that mikebom ran the reduced-fidelity path.
- **Helm scan that mixes with non-Helm content** (e.g., a repo where `Chart.yaml` is at `charts/foo/Chart.yaml`, alongside a Cargo workspace at repo root) — the diagnostic is set exactly when the helm reader was invoked. Non-Helm portions of the same scan don't turn the flag off. Annotation reflects the single-scan-diagnostic value.
- **Multiple helm charts under the scan root** (m188 recursion into `charts/*/`) — one `ScanDiagnostics.helm_extraction_mode` flag per scan, so one annotation per emitted document. If any chart was scanned in rendered mode, the emitted value is `"full"`; if any was unrendered and none rendered, `"partial"`. Implementation determined by the reader's existing state-setting semantics (last-wins in m188/m203's current code); this spec does not change that.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The CDX 1.6 emitter MUST include `mikebom:image-extraction-completeness = "partial"|"full"` as a `metadata.properties[]` entry when `ScanDiagnostics.helm_extraction_mode` is `Some(_)`. The property name is exactly `mikebom:image-extraction-completeness` (kebab-case, `mikebom:` prefix). The value is exactly `"partial"` for `Unrendered` and exactly `"full"` for `Rendered`.
- **FR-002**: The SPDX 2.3 emitter MUST include the same annotation as a document-scope `Annotation` element following the m071 parity envelope shape (`MikebomAnnotationCommentV1` JSON with `k: "mikebom:image-extraction-completeness"` and `v: "partial"|"full"` in the annotation `comment` field). `annotator = "Tool: mikebom-<version>"`, `annotationType = "OTHER"`, `annotationDate` = the emission timestamp already used for other document-scope annotations.
- **FR-003**: The SPDX 3 emitter MUST include the same annotation as an SPDX 3 `Annotation` element in the JSON-LD graph. `subject` points at the SpdxDocument's IRI. `statement` carries the `mikebom:image-extraction-completeness = "partial"|"full"` payload following the m145/m166 SPDX 3 annotation-emission conventions.
- **FR-004**: When `ScanDiagnostics.helm_extraction_mode` is `None` (no helm reader invoked during the scan), NO `mikebom:image-extraction-completeness` annotation MUST be emitted in ANY format. Non-Helm scans see zero drift versus pre-m204 output.
- **FR-005**: When m203's `--helm-render` succeeds and `helm_extraction_mode` is set to `Rendered`, the emitted value MUST be `"full"`. When m203's `--helm-render` fails and falls back to unrendered (any of the 4 fallback classes), `helm_extraction_mode` is `Unrendered` and the emitted value MUST be `"partial"` — reflecting the actual extraction path, not the requested mode.
- **FR-006**: The m071 parity catalog MUST include a new symmetric-equal entry for `mikebom:image-extraction-completeness` so the existing CDX/SPDX parity tests exercise the three-way format equivalence automatically.
- **FR-007**: The change MUST NOT alter any pre-m204 golden JSON under `mikebom-cli/tests/fixtures/` for non-Helm scans. Golden regeneration is expected only for helm-scan fixtures that were previously emitted without the annotation.
- **FR-008**: Emission MUST be deterministic across runs (annotation timestamp uses the existing scan-emission timestamp source; annotation value is a pure function of `helm_extraction_mode`).

### Key Entities *(include if feature involves data)*

- **`mikebom:image-extraction-completeness` annotation**: document-scope metadata carrying a two-valued string (`"partial"` or `"full"`) that signals the fidelity of image-ref extraction. Present exactly when the helm reader ran during the scan. Absent otherwise.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a helm chart scan without `--helm-render`, all three emitted SBOM formats contain exactly one `mikebom:image-extraction-completeness = "partial"` document-scope annotation each.
- **SC-002**: On a helm chart scan with `--helm-render` and a real `helm` binary present, all three emitted SBOM formats contain exactly one `mikebom:image-extraction-completeness = "full"` document-scope annotation each. Verifiable via `MIKEBOM_HELM_INTEGRATION=1`-gated test.
- **SC-003**: On a non-Helm scan, zero `mikebom:image-extraction-completeness` annotations appear in any of the three emitted formats.
- **SC-004**: `git diff --stat mikebom-cli/tests/fixtures/` post-implementation shows drift ONLY on fixtures that scan a helm chart. Non-Helm golden fixtures are byte-identical.
- **SC-005**: The m071 CDX ↔ SPDX 2.3 ↔ SPDX 3 parity test suite passes without new parity failures — the new annotation flows through all three formats symmetrically.
- **SC-006**: `./scripts/pre-pr.sh` wall-clock delta versus pre-m204 baseline is ≤ 5 seconds.
- **SC-007**: PR description references `Closes #554`.

## Assumptions

- `ScanDiagnostics.helm_extraction_mode` field exists and is populated by the helm reader at the exact code path where `helm::read` returns. Verified pre-m204 at `mikebom-cli/src/scan_fs/package_db/helm.rs` (m188 landed T023; m203 wired the `Rendered` branch).
- The three document-scope annotation emission conventions are already established:
  - CDX 1.6: `metadata.properties[]` entries with `name` + `value` (m071, m127, m134 precedent).
  - SPDX 2.3: document-scope `Annotation` elements with `MikebomAnnotationCommentV1`-shaped `comment` (m071 parity envelope).
  - SPDX 3: `Annotation` elements in the JSON-LD graph (m145/m166 precedent).
- `build_metadata` in `cyclonedx/metadata.rs` and its SPDX analogs are already parameterized on `ScanDiagnostics` and can accept one more field without cross-cutting refactor.
- Zero new Cargo dependencies (matches m203 posture). All plumbing is `serde_json::Value` construction against existing infrastructure.
- No changes to the helm reader's diagnostic-setting logic — m204 is emitter-only.
- Windows / cross-host portability: unchanged from m188/m203. Emitter-only work; no subprocess calls.
