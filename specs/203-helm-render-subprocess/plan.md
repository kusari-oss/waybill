# Implementation Plan: Helm `--helm-render` Subprocess Implementation

**Branch**: `203-helm-render-subprocess` | **Date**: 2026-07-17 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/203-helm-render-subprocess/spec.md`

## Summary

Wire the deferred m188 US3 subprocess at `mikebom-cli/src/scan_fs/package_db/helm.rs:300-301` (currently `let _ = render_mode; // future US3 hook`). Add a new `extract_image_refs_rendered(chart_dir, timeout) -> Result<Vec<ImageRef>, HelmRenderError>` function that shells out to `helm template <chart_dir>` via the m055 `run_go_mod_graph` subprocess-with-timeout pattern (thread + `mpsc::channel` + `recv_timeout`). Branch in `helm::read()`: on `HelmRenderMode::OptIn`, call the new function; on any `HelmRenderError`, WARN-log and fall back to the pre-existing `extract_image_refs_unrendered`. Set `ScanDiagnostics.helm_extraction_mode = Some(HelmExtractionMode::Rendered)` on success.

Zero new Cargo dependencies. Bounded change surface: 1 source file edit (helm.rs) + 1 new integration-test file (or extension of existing `tests/helm_reader.rs`) + 1-2 new fixtures. Estimated ~250-300 LOC.

Reconnaissance findings (per m199-m202 lesson):
- m055 `run_go_mod_graph` pattern verified at `golang/go_mod_graph.rs:81-158` — probe-binary-first (`Command::new("go").arg("version").output()`) then spawn-in-worker-thread + `mpsc::channel` + `rx.recv_timeout(timeout)`. Same pattern m053 (`git describe`) + m173 (warm-go-cache) use.
- Branch site pinned at `helm.rs:300-301` (`let _ = render_mode; // future US3 hook` comment).
- `HelmRenderMode` enum + `HelmExtractionMode` enum + `ScanDiagnostics.helm_extraction_mode` field + `--helm-render` CLI flag + env-var propagation ALL landed in m188. m203 is a small delta on top of finished scaffolding.
- Golden regen expected 0 files (non-Helm scans byte-identical per FR-009; existing m188 helm fixture goldens don't exercise `--helm-render` per m188's default-flow-only test posture).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–202; no nightly).
**Primary Dependencies**: Existing only — `std::process::Command`, `std::thread`, `std::sync::mpsc`, `std::time::Duration` (all stdlib), `tracing` (existing WARN log dep), `anyhow`/`thiserror` (existing error propagation). **No new crates.** External-tool runtime dep: `helm` binary on `$PATH`, OPT-IN via `--helm-render` flag or `MIKEBOM_HELM_RENDER=1` env var. Absent by default; scan works without it.
**Storage**: N/A — all state in-process per scan; matches every reader milestone since 002.
**Testing**: New integration tests appended to the existing `mikebom-cli/tests/helm_reader.rs`. US2 fallback tests (missing binary, non-zero exit, timeout) use synthetic `PATH` scrubbing or stub shell scripts — no real `helm` binary required in CI's default lane. US1 success test gated behind `MIKEBOM_HELM_INTEGRATION=1` env var (matches m188 pattern) — runs in the dedicated nightly job with real `helm` installed.
**Target Platform**: Same as mikebom itself. Note: `helm` binary availability is a POSIX-ish assumption; Windows CI's smoke tests skip the US1 gated test but exercise US2 fallback classes.
**Project Type**: Reader-side subprocess integration + fallback. ~50 LOC in `helm.rs` (new `extract_image_refs_rendered` function + `HelmRenderError` enum + `resolve_render_timeout` helper) + ~30 LOC branch wiring at helm.rs:300 + ~150 LOC integration tests + ~50 LOC fixtures + optional shell-script stubs. **Roughly 300 LOC total.**
**Performance Goals**: No perf regression beyond FR-008 (`./scripts/pre-pr.sh` wall-clock delta ≤ 5s per SC-006). Note: non-Helm scans MUST show zero delta (no subprocess invoked).
**Constraints**: (a) zero new Cargo deps; (b) opt-in-only external subprocess per Constitution Principle I / FR-006; (c) EVERY failure class falls back to unrendered per Constitution Principle III / FR-007; (d) stderr head-cap 20 lines per m188 FR-018 (secrets guard).
**Scale/Scope**: 1 source file edit (helm.rs) + 1 test file extension + 1-2 fixtures. Small, focused change.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C** — ✅ PASS. All new Rust code; subprocess invocation via stdlib `Command`. `helm` external binary integration is opt-in only per FR-006 (matches m053 `git describe` + m055 `go mod graph` + m173 warm-go-cache precedents).
- **II. eBPF-Only Observation** — ✅ N/A. Reader-side extension, not a dep-discovery mechanism.
- **III. Fail Closed** — ✅ PASS. Every failure class (`BinaryNotFound`, `NonZeroExit`, `Timeout`, `IoError`) falls back to unrendered extraction with a WARN log per FR-007. Scan never aborts due to helm-render issues. This DOES follow the fail-closed spirit even though it's a graceful-degradation path: the operator gets a scan result AND a transparency signal about the reduced fidelity (Principle X + XI dovetail).
- **IV. Type-Driven Correctness** — ✅ PASS. `HelmRenderError` newtype enum with named variants; `Duration` newtype for timeout; existing `HelmRenderMode` + `HelmExtractionMode` enums unchanged. No stringly-typed boundaries introduced.
- **V. Specification Compliance** — ✅ PASS. Zero new `mikebom:*` annotations introduced by m203. The `Rendered` mode value is set on `ScanDiagnostics.helm_extraction_mode` — an INTERNAL state field consumed by follow-up #554 (m204) to emit the document-scope completeness annotation. m203 alone doesn't touch emitted-format wire shapes for non-Helm scans (FR-009 byte-identity). For Helm scans WITH `--helm-render` succeeding, the emitted image-ref components are the same PURL shape as the unrendered path — just with fully-substituted values (no template placeholders in the emitted PURLs). No wire-format contract change; no Principle V audit needed.
- **VI. Three-Crate Architecture** — ✅ PASS. All changes stay in `mikebom-cli`.
- **VII. Test Isolation** — ✅ PASS. US1 success test gated behind `MIKEBOM_HELM_INTEGRATION=1` (nightly-only). US2 fallback tests run in default CI via synthetic PATH manipulation + stub shell scripts (no real `helm` needed).
- **VIII. Completeness** — ✅ PASS. Improves image-ref extraction fidelity for Helm charts (fewer template-placeholder artifacts in emitted PURLs). Directly serves the Completeness rationale.
- **IX. Accuracy** — ✅ PASS. Rendered extraction eliminates false-positive image-ref components where the template placeholder text was interpreted as an image ref (e.g., `image: {{ .Values.image.repository }}:{{ .Values.image.tag }}` in unrendered mode surfaces the literal Go-template string as a "component"; rendered mode extracts the concrete value).
- **X. Transparency** — ✅ PASS. Every fallback class writes a WARN log naming the reason. Downstream follow-up #554 surfaces the mode via document-scope annotation (m204 candidate). Operator visibility into the reduced-fidelity paths is high.
- **XI. Enrichment (DX)** — ✅ PASS. Explicit opt-in flag; graceful degradation on all failure classes; no scan-abort surprises. Matches the fail-graceful posture that Principle XI encourages.
- **XII. External Data Source Enrichment** — ✅ N/A. `helm` is a local binary, not an external data source.
- **Strict Boundary §5 (file-tier)** — ✅ N/A.

**Result**: All principles PASS. No violations.

**Post-Phase-1 re-check**: N/A — Phase 1 introduces no new entities beyond what's already documented above (`HelmRenderError` enum + `resolve_render_timeout` helper + `extract_image_refs_rendered` function). Constitution gate trivially remains PASS post-design.

## Project Structure

### Documentation (this feature)

```text
specs/203-helm-render-subprocess/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0 output — 4 mechanical decisions
├── data-model.md        # Phase 1 output — HelmRenderError enum + subprocess flow
├── quickstart.md        # Phase 1 output — 5 reproducers (fallback classes + success)
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (created by /speckit-tasks)
```

No `contracts/` sub-directory — the entire subprocess contract is documented at m188's `specs/188-helm-chart-scanning/contracts/extraction-pipeline.md §Phase C`. m203 inherits by reference.

### Source Code (repository root)

```text
mikebom-cli/src/scan_fs/package_db/
└── helm.rs                                          # MODIFIED — the entirety of m203:
                                                     #
                                                     # New items (~50 LOC total):
                                                     #   - HelmRenderError enum (4 variants:
                                                     #       BinaryNotFound, NonZeroExit,
                                                     #       Timeout, IoError)
                                                     #   - resolve_render_timeout() helper
                                                     #       (reads MIKEBOM_HELM_RENDER_TIMEOUT_SECS,
                                                     #        clamps to [1, 3600], default 60)
                                                     #   - extract_image_refs_rendered(chart_dir,
                                                     #       timeout) — mirrors m055's
                                                     #       run_go_mod_graph pattern verbatim
                                                     #       (probe-then-spawn-then-recv_timeout)
                                                     #
                                                     # Modified items (~30 LOC):
                                                     #   - read() at line 282+ — replace the
                                                     #     `let _ = render_mode;` at line 300
                                                     #     with the match-on-render_mode branch
                                                     #     per m188 contracts §Phase C pseudocode.
                                                     #     Set diagnostics.helm_extraction_mode
                                                     #     to Rendered on success, Unrendered on
                                                     #     fallback.

mikebom-cli/tests/
└── helm_reader.rs                                   # MODIFIED — extend with m203 tests:
                                                     #   - US2.1 missing-helm binary fallback
                                                     #       (PATH="" fixture)
                                                     #   - US2.2 non-zero-exit fallback
                                                     #       (stub shell script that exits 1)
                                                     #   - US2.3 timeout fallback (stub script
                                                     #       that sleeps > timeout)
                                                     #   - US2.4 timeout env-var override
                                                     #       (MIKEBOM_HELM_RENDER_TIMEOUT_SECS=1
                                                     #        + stub script sleeping 3s)
                                                     #   - US1 success test (gated
                                                     #       MIKEBOM_HELM_INTEGRATION=1 — real
                                                     #       helm binary required)
                                                     #   - FR-006 default-flow guarantee: assert
                                                     #       zero subprocess invocation when
                                                     #       --helm-render NOT set (regression
                                                     #       guard for the m188 zero-external-
                                                     #       binary default).

mikebom-cli/tests/fixtures/helm/
├── (existing m188 fixtures unchanged)
├── render_success_m203/                             # NEW — US1 fixture:
│   ├── Chart.yaml                                   #   Minimal valid chart
│   ├── values.yaml                                  #   image.repository = "nginx"
│                                                    #   image.tag = "1.27.0"
│   └── templates/deployment.yaml                    #   image: {{ .Values.image.repository }}
│                                                    #          :{{ .Values.image.tag }}
└── render_stub_scripts_m203/                        # NEW — US2 stub scripts (shell):
    ├── helm-exit1.sh                                #   #!/bin/sh; exit 1
    ├── helm-sleep-forever.sh                        #   #!/bin/sh; sleep 3600
    └── helm-sleep-3s.sh                             #   #!/bin/sh; sleep 3
```

**Structure Decision**: 1 source file edit + test-file extension + 2 fixture directories (1 real chart + 1 stub-script bundle). Zero existing goldens expected to require regen per plan reconnaissance (re-verified at implement time).

## Complexity Tracking

No constitution violations. All principles pass on first check. The subprocess pattern is precedented (m053/m055/m173); no new architectural choices needed.
