# Feature Specification: Helm Chart `--helm-render` Subprocess Implementation

**Feature Branch**: `203-helm-render-subprocess`
**Created**: 2026-07-17
**Status**: Draft
**Input**: User description: "553" (GitHub issue #553 — m188 follow-up: `--helm-render` subprocess implementation)

## Overview

Milestone 188 (#549) landed the Helm chart cataloger with two operating modes documented in the `HelmRenderMode` enum: `Off` (default, always-on unrendered extraction) and `OptIn` (opt-in via `--helm-render` CLI flag or `MIKEBOM_HELM_RENDER=1` env var). The CLI surface + env-var propagation + `ScanDiagnostics.helm_extraction_mode` state capture ALL landed in m188. The `helm template` subprocess implementation was deferred to keep m188's task budget focused on the always-on unrendered path.

Post-m188, when an operator passes `--helm-render`, the helm reader currently notes the mode but doesn't actually shell out — falls back to the unrendered path with the same output as `--helm-render` NOT passed. m203 wires the deferred Phase C subprocess per the m188 `contracts/extraction-pipeline.md §Phase C` design.

**User-observable outcome**: with `--helm-render` set + `helm` binary on `$PATH`, image refs extracted from a chart get the higher-fidelity rendered treatment — no `{{ .Values.image.tag }}`-style placeholder markers in the output because helm's template engine has already substituted them. Operators who accept the opt-in dependency on the `helm` CLI get better SBOM completeness for their K8s deployments.

**m188 relationship**: m203 completes the m188 Phase C implementation. m188 US3 (opt-in rendered extraction) is unblocked by this landing. m554 (image-extraction-completeness annotation `"full"` value) also depends on m203 landing to have a `Rendered` mode to signal.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Rendered extraction produces higher-fidelity image refs (Priority: P1)

An operator has `helm` on `$PATH` and scans a helm chart with `mikebom sbom scan --helm-render --path <chart>`. Instead of the unrendered line-based regex extraction (which surfaces literal `image: {{ .Values.registry }}/{{ .Chart.Name }}:{{ .Values.tag }}` template placeholders in the SBOM), mikebom shells out to `helm template <chart>`, runs the template engine, and extracts image refs from the FULLY-RENDERED stdout — no placeholders remain. The emitted SBOM lists concrete image refs (e.g., `docker.io/library/nginx:1.27.0`).

**Why this priority**: This IS the m188 US3 deliverable. Unrendered fallback (m188 US2) is a functional-but-lower-fidelity path; the rendered path is what makes the helm reader competitive with tools that specifically target rendered manifests. Closes #553.

**Independent Test**: Fixture chart with a `templates/deployment.yaml` containing `image: {{ .Values.image.repository }}:{{ .Values.image.tag }}` + a `values.yaml` setting `image.repository = "nginx"` and `image.tag = "1.27.0"`. Scan with `--helm-render`. Assert the SBOM lists `docker.io/library/nginx:1.27.0` (or `nginx:1.27.0` — implementation-decided normalization) and NO literal `{{` character sequence appears in the emitted image refs.

**Acceptance Scenarios**:

1. **Given** a helm chart with template-parameterized image refs + a `values.yaml` supplying the concrete values,
   **When** mikebom scans with `--helm-render` and `helm` is on `$PATH`,
   **Then** the emitted SBOM component list contains the concrete image refs (no `{{` placeholders), AND `ScanDiagnostics.helm_extraction_mode == Some(HelmExtractionMode::Rendered)` is captured internally.

2. **Given** the same chart scanned WITHOUT `--helm-render`,
   **When** mikebom emits the SBOM,
   **Then** the extracted image refs contain literal template placeholder text OR are omitted from the SBOM (unrendered fallback behavior — unchanged from m188 US2). `ScanDiagnostics.helm_extraction_mode == Some(HelmExtractionMode::Unrendered)`.

3. **Given** a helm chart that renders cleanly via `helm template` and produces N distinct image refs post-render,
   **When** mikebom scans with `--helm-render`,
   **Then** the SBOM contains N `pkg:oci/*` (or equivalent format-specific) image-ref components, one per unique post-render image ref.

---

### User Story 2 - Missing `helm` binary falls back gracefully (Priority: P1)

An operator passes `--helm-render` on a scan environment where `helm` is NOT installed. mikebom does NOT abort the scan; instead, logs a WARN-level message with the fallback reason and proceeds via the unrendered extraction path (same as if `--helm-render` was omitted). The SBOM is still emitted; the operator learns from the log that the higher-fidelity path was unavailable.

**Why this priority**: Fail-closed is not appropriate for a supplementary rendering mode. Aborting the scan when helm is absent would break every unrelated Helm-chart-in-image scan where the operator doesn't care about rendered fidelity but happens to have a Helm chart in their scan tree. Fall-back-and-log is the correct posture (Constitution Principle III + XI — DX).

**Independent Test**: Scan a fixture chart with `PATH=""` (empty PATH scrubs helm from lookup) + `--helm-render`. Assert (a) scan exits 0 (success), (b) a WARN-level log line mentions the fallback + the `HelmRenderError::BinaryNotFound` reason, (c) `ScanDiagnostics.helm_extraction_mode == Some(HelmExtractionMode::Unrendered)` (fell back).

**Acceptance Scenarios**:

1. **Given** a helm chart fixture + `PATH=""` (or `helm` not installed),
   **When** mikebom scans with `--helm-render`,
   **Then** scan exits with status 0, WARN log mentions `HelmRenderError::BinaryNotFound`, and the fallback (unrendered) extraction runs.

2. **Given** a helm chart fixture + `helm` installed but the subprocess exits non-zero (invalid chart, missing dep, template error),
   **When** mikebom scans with `--helm-render`,
   **Then** scan exits 0, WARN log includes the first 20 lines of the `helm template` stderr (secrets guard per m188 FR-018), and the fallback extraction runs. `helm_extraction_mode == Unrendered`.

3. **Given** a helm chart that hangs on `helm template` (e.g., dependency-fetch hang),
   **When** mikebom scans with `--helm-render` (default 60s timeout),
   **Then** the subprocess is killed after 60s, WARN log mentions `HelmRenderError::Timeout`, and the fallback extraction runs. `helm_extraction_mode == Unrendered`.

4. **Given** a helm chart that would hang > 60s under `helm template` + operator sets `MIKEBOM_HELM_RENDER_TIMEOUT_SECS=5`,
   **When** mikebom scans with `--helm-render`,
   **Then** the subprocess is killed after 5s. Operator-set timeout MUST be honored.

---

### Edge Cases

- **`--helm-render` NOT set (default `Off` mode)**: no subprocess invoked at all. `Command::new("helm").spawn()` MUST NOT execute even on a system with `helm` installed. Verified via a test that stubs `PATH` and asserts no subprocess trace.
- **`helm` binary that exits 0 but writes garbage to stdout**: the image-ref regex applied to the rendered stdout finds nothing → 0 image refs → no image-ref components in the SBOM. NOT a fallback trigger; the run succeeded, just produced no findings.
- **Chart with no `templates/` directory** (Helm library chart / values-only): `helm template` typically errors → non-zero exit → fallback triggered. Documented degradation.
- **Chart with a `helmignore` pattern that hides templates**: helm respects ignore patterns; rendered output may be empty. Same as garbage-stdout case above.
- **Very large rendered output** (>100 MB YAML from 500-template chart): mikebom MUST NOT buffer unbounded. Cap stdout at a reasonable limit (e.g., 50 MB per m188 constraint documentation OR document the effective cap discovered at implement time) and truncate + WARN if exceeded. Extract image refs from what was buffered.
- **Chart requiring `--dependency-update` first**: if `helm template` fails because deps aren't resolved, we get non-zero exit → fallback. mikebom does NOT attempt `helm dependency update` automatically (operator responsibility).
- **Chart with subcharts referencing external repositories**: `helm template` requires the subcharts to be present under `charts/`. If missing, subprocess exits non-zero → fallback. mikebom does NOT fetch subcharts.
- **PATH points to a NON-`helm` executable named `helm`** (a shim, wrapper script): mikebom trusts the resolved binary. If it produces valid `helm template`-shaped output, extraction succeeds. If not, non-zero exit → fallback.
- **Stderr contains a kubeconfig path or secret value**: the first 20 lines cap (per m188 FR-018) limits leakage. Operators concerned about full-secret-in-log should not use `--helm-render` on charts that touch cluster-connected `.Files.Get()` calls (Helm best practice for rendering is offline).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When `--helm-render` is set OR `MIKEBOM_HELM_RENDER=1` env var is present, the helm reader MUST shell out to `helm template <chart-dir>` and use the subprocess's stdout as the image-ref extraction source. The pattern MUST match the m188 `contracts/extraction-pipeline.md §Phase C` design: `Command::new("helm").args(["template", <chart>]).stdout(Stdio::piped()).stderr(Stdio::piped())`.
- **FR-002**: Subprocess execution MUST enforce a timeout. Default 60 seconds. Operator override via `MIKEBOM_HELM_RENDER_TIMEOUT_SECS=<n>` (positive integer, seconds). On timeout expire, the child process MUST be killed (`Child::kill()`), pipe buffers drained, and the reader MUST return `HelmRenderError::Timeout`.
- **FR-003**: `helm` binary absence (`Command::spawn()` returns `ENOENT`) MUST NOT abort the scan. Reader logs a WARN with the reason (`HelmRenderError::BinaryNotFound`) and falls back to the m188-established unrendered extraction path. Scan exits 0.
- **FR-004**: `helm template` non-zero exit MUST NOT abort the scan. Reader logs WARN with the first 20 lines of stderr (secrets guard per m188 FR-018), sets `HelmRenderError::NonZeroExit { code, stderr_head }`, and falls back to unrendered extraction. Scan exits 0.
- **FR-005**: On successful `helm template` (exit 0), the extracted image refs MUST come from the rendered stdout (using the m188 `IMAGE_REGEX` applied to fully-substituted YAML). No `{{`-style template placeholder text may appear in the emitted image-ref components. Post-fix, `ScanDiagnostics.helm_extraction_mode = Some(HelmExtractionMode::Rendered)` for the successful case.
- **FR-006**: When `--helm-render` is NOT set (default `HelmRenderMode::Off`), the reader MUST NOT invoke `Command::spawn("helm")` at all — even if `helm` is on `$PATH`. This preserves the m188 FR-013 "zero-external-binary in default flow" guarantee.
- **FR-007**: The reader MUST support four failure classes documented in `HelmRenderError`: `BinaryNotFound`, `NonZeroExit { code, stderr_head }`, `Timeout`, `IoError`. All four fall back to unrendered extraction with a WARN log. Scan never aborts.
- **FR-008**: `./scripts/pre-pr.sh` MUST continue to pass green post-implementation.
- **FR-009**: Non-Helm scans (scan trees with no `Chart.yaml`) MUST be unaffected — no subprocess invocation, no diagnostic-mode capture. Byte-identity guarantee for the 99% of scans that don't touch Helm.

### Key Entities

- **`HelmRenderMode` enum (existing at `mikebom-cli/src/scan_fs/package_db/helm.rs:57`)**: `Off` (default) or `OptIn` (opt-in via CLI flag or env var). No shape change from m188.
- **`HelmExtractionMode` enum (existing)**: `Unrendered` (m188 US2 always-on path) or `Rendered` (m203-activated when `--helm-render` succeeds).
- **`HelmRenderError` enum (NEW)**: `BinaryNotFound`, `NonZeroExit { code: i32, stderr_head: String }`, `Timeout`, `IoError(std::io::Error)`. All four map to "fall back to unrendered + WARN log."
- **`MIKEBOM_HELM_RENDER_TIMEOUT_SECS` env var (NEW)**: positive-integer seconds override for the subprocess timeout. Default 60 when unset. Bounded [1, 3600] per implementation-choice (documented at plan time).
- **`ScanDiagnostics.helm_extraction_mode` (existing at m188 emission-time)**: `Option<HelmExtractionMode>`. Consumed by follow-up #554 to emit the document-scope `mikebom:image-extraction-completeness` annotation. m203 sets this to `Some(Rendered)` in the successful subprocess path.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For a fixture helm chart with `image: {{ .Values.image.repository }}:{{ .Values.image.tag }}` + values-supplied concrete `nginx:1.27.0`, scanned with `--helm-render` and `helm` on `$PATH`: the emitted SBOM component list contains an image ref with the concrete value (`nginx:1.27.0` or format-normalized equivalent), AND zero components contain the literal `{{` character sequence in any image-ref field.
- **SC-002**: For the same fixture scanned WITHOUT `--helm-render` (m188 US2 baseline): the emitted SBOM either lists the raw template placeholder OR omits the image ref entirely. Unchanged from m188 US2.
- **SC-003**: For any fixture scanned with `--helm-render` + `PATH=""`: scan exits 0, log contains a WARN-level line mentioning `BinaryNotFound`, output is byte-identical to the unrendered path.
- **SC-004**: For a fixture crafted to make `helm template` hang past the timeout: scan exits 0 within `MIKEBOM_HELM_RENDER_TIMEOUT_SECS + 5s` (child-kill + cleanup budget). WARN log mentions `Timeout`.
- **SC-005**: Non-Helm scans (test tree with 0 Chart.yaml files) produce byte-identical SBOMs pre/post-m203. Zero drift on existing goldens.
- **SC-006**: `./scripts/pre-pr.sh` wall-clock delta ≤ 5 seconds vs pre-m203 baseline.
- **SC-007**: Post-merge, `#553` closes automatically via `Closes #553` in the PR body.

## Assumptions

- **`helm` binary compatibility**: mikebom assumes `helm template` semantics match Helm v3.x (the current stable). Helm v2 is legacy (EOL) — no compatibility required. Documented in the reader's doc comment.
- **Subprocess pattern reuse**: the timeout + kill + pipe-drain plumbing MUST reuse the m055 `run_go_mod_graph` at `mikebom-cli/src/scan_fs/package_db/golang/go_mod_graph.rs:81-158` pattern verbatim (thread + `std::sync::mpsc::channel`), per m188 contract. Same convention as m053 (`git describe`) + m173 (warm-go-cache). No new subprocess-runtime abstraction added.
- **Zero new Cargo dependencies**: the fix uses `std::process::Command` + `std::thread` + `std::sync::mpsc` — all stdlib. No new crates.
- **Constitution Principle I compliance**: reuses the same pattern established by m053/m055/m173 for opt-in external-tool subprocess invocation. External-tool integration is explicitly gated behind an opt-in CLI flag; zero external dependencies in the default flow (FR-006).
- **Constitution Principle III compliance**: EVERY failure mode falls back to the m188 unrendered path with a WARN log. Scan never aborts due to helm-render issues.
- **Constitution Principle X compliance**: stderr head-cap (20 lines per m188 FR-018) prevents kubeconfig / secret leakage in log output.
- **Constitution Principle V compliance**: no new `mikebom:*` annotations introduced by m203. The `Rendered` mode value is captured in `ScanDiagnostics.helm_extraction_mode` — an INTERNAL state field consumed by follow-up #554 (m204 candidate) to emit the document-scope completeness annotation. m203 alone doesn't touch emitted-format wire shapes for non-Helm scans.
- **Regression goldens scope**: expected 0 pre-existing goldens require regen. Non-Helm scans byte-identical per FR-009. Existing m188 fixture goldens may regen if the `helm_extraction_mode` values embedded there change — but m188 already captures `Unrendered` in tests without `--helm-render`, and m203 doesn't change that default-flow value. Re-verified at implement time per m199-m202 lesson.
- **Real `helm` binary in CI is out of scope**: US3-success integration tests requiring a real `helm` binary land behind a `MIKEBOM_HELM_INTEGRATION=1` env-var gate (m188 pattern). CI's default lane doesn't install helm; the gated tests run in a dedicated nightly job.
