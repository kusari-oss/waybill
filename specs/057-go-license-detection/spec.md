# Feature Specification: Go main-module LICENSE detection (Layer 1 — SPDX header)

**Feature Branch**: `057-go-license-detection`
**Created**: 2026-05-02
**Status**: Draft
**Input**: User description: closes #103, follow-up to milestone 053 — populate the synthetic Go main-module component's `licenses` field from a `LICENSE` / `LICENSE.md` / `LICENSE.txt` / `COPYING` (+ British `LICENCE`) file at the workspace root by scanning for the `SPDX-License-Identifier:` header. Layer 2 (content-based matcher) deferred.

## Clarifications

### Session 2026-05-02

- Q: Is Layer 2 (content-based license matcher, e.g. `askalono`) in scope? → A: **No.** Layer 1 (SPDX-License-Identifier header) only. Layer 2 adds a 3MB index + transitive deps for marginal coverage gain on the projects whose LICENSE file is bare canonical text. Catches the same ~30–50 % of projects without the install cost. If real users hit a wall, Layer 2 follows up as a separate milestone.
- Q: Which filenames are scanned? → A: **`LICENSE`, `LICENSE.md`, `LICENSE.txt`, `COPYING`, `LICENCE`** (British), `LICENCE.md`, `LICENCE.txt`. Case-INsensitive match. The first one found wins.
- Q: How much of the file do we scan? → A: **First 4 KB**. Per the issue body. Sufficient for the SPDX header which is conventionally on the first line; cap prevents runaway reads on stray text files.
- Q: How is the SPDX header parsed? → A: Existing `spdx::SpdxExpression::try_canonical` from milestone 010. Same code path that validates SBOM-output license expressions; reuse guarantees consistency.
- Q: What if the file exists but has no SPDX header? → A: **Component emits empty `licenses` (preserves milestone 053 FR-005 default).** Users see "no license detected" rather than a guess. Layer 2 follow-up addresses this case.
- Q: What if the SPDX header expression fails to canonicalize? → A: **Component emits empty `licenses` and a `tracing::warn` line names the path + raw expression.** Don't fail the scan; degrade gracefully.

## Investigation findings

The existing Go main-module path (`build_main_module_entry` at `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs:657`) hard-codes `licenses: Vec::new()` per milestone 053 FR-005 (LICENSE detection deferred to issue #103). The synthetic main-module component carries a `mikebom:component-role: main-module` annotation (C40) which sbomqs MAY honor for licensing-coverage exclusion — but consumers who want a real license expression on the project's own component get nothing today.

Concrete prevalence check from public Go projects:
- `knative/func@knative-v1.22.0`: LICENSE → starts with `Apache License Version 2.0`, NO SPDX header. Layer 1 misses.
- `kubernetes/kubernetes`: LICENSE → has `SPDX-License-Identifier: Apache-2.0` near top. Layer 1 hits.
- `prometheus/prometheus`: LICENSE → has SPDX header. Layer 1 hits.
- `argoproj/argo-workflows@v3.x`: LICENSE → has SPDX header. Layer 1 hits.

Layer 1 likely covers a meaningful fraction of high-profile Go projects — enough to justify defaulting it on without a feature flag. Projects without SPDX headers degrade to milestone-053 behavior (empty licenses, C40 role tag).

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Real license expression on the project's own component (Priority: P1)

A consumer of mikebom's SBOM output for a Go project wants the synthetic main-module component to carry the project's own license expression — not just an empty array — so legal/audit tooling can answer "what license is this codebase shipped under?" directly from the SBOM.

**Why this priority**: Closes the documented gap from milestone 053 FR-005. Without this, every Go SBOM has a glaring `licenses: []` on the project's own component, and downstream tooling needs to fish around in the underlying repo to figure out what the project ships under. With Layer 1, projects following the SPDX best-practice convention (`SPDX-License-Identifier: Apache-2.0`) get the right answer for free.

**Independent Test**: Construct a workspace fixture with a `LICENSE` file containing `SPDX-License-Identifier: Apache-2.0` on its first line. Scan the workspace; assert the main-module component's `licenses` field contains the canonical `Apache-2.0` SPDX expression. Independently testable: changes are confined to `build_main_module_entry`'s license population.

**Acceptance Scenarios**:

1. **Given** a Go workspace with `LICENSE` containing `SPDX-License-Identifier: Apache-2.0\n\n<full text>`, **When** mikebom scans, **Then** the main-module component's `licenses` field contains `Apache-2.0`.
2. **Given** a Go workspace with `LICENSE.md` containing `SPDX-License-Identifier: MIT OR Apache-2.0`, **When** mikebom scans, **Then** the main-module's `licenses` field reflects the canonicalized compound expression.
3. **Given** a Go workspace with NO `LICENSE` file, **When** mikebom scans, **Then** the main-module's `licenses` is empty (milestone 053 FR-005 behavior preserved).
4. **Given** a Go workspace with `LICENSE` but no SPDX header line, **When** mikebom scans, **Then** the main-module's `licenses` is empty (Layer 1 miss; Layer 2 territory).
5. **Given** a Go workspace with `LICENSE` containing `SPDX-License-Identifier: NotARealLicense`, **When** mikebom scans, **Then** the main-module's `licenses` is empty AND `tracing::warn` records the unparseable expression.

### Edge Cases

- **Multiple candidate filenames** (e.g., `LICENSE` AND `COPYING`): scan in priority order — `LICENSE`, `LICENSE.md`, `LICENSE.txt`, `LICENCE`, `LICENCE.md`, `LICENCE.txt`, `COPYING`. First file found that yields a parseable expression wins.
- **Case-INsensitive filename match** (`license`, `License`, `LICENSE`): match case-INsensitive on the filename.
- **SPDX header buried >4 KB into the file**: Layer 1 misses by design.
- **SPDX header with leading whitespace** or trailing comment markers: match permissively — strip leading whitespace, take everything after `SPDX-License-Identifier:` up to end-of-line, trim surrounding comment delimiters (`<!-- -->`, `// `, `# `).
- **`LICENSE` is a directory**: skip entirely (file-scan logic uses `is_file()`).
- **`LICENSE` is a symlink**: follow it; standard walker symlink handling (milestone 054).
- **License file empty (0 bytes)**: no SPDX header found → empty licenses.
- **Multiple SPDX headers in the same file**: take the first one.
- **Unicode BOM at file start** (`\xEF\xBB\xBF`): strip before searching.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: For every synthetic Go main-module component, mikebom MUST scan the workspace root for a license file (in priority order: `LICENSE`, `LICENSE.md`, `LICENSE.txt`, `LICENCE`, `LICENCE.md`, `LICENCE.txt`, `COPYING`; case-insensitive match) and populate the component's `licenses` field with the canonical SPDX expression from the first `SPDX-License-Identifier:` header found in the first 4 KB of any matching file. Default-on (no feature flag).

- **FR-002**: Header parsing MUST go through `spdx::SpdxExpression::try_canonical`. Unparseable expressions MUST NOT crash the scan; the affected main-module component emits empty `licenses` AND a `tracing::warn` line records the path + raw expression.

- **FR-003**: When no candidate license file exists OR no candidate file contains a parseable SPDX header in its first 4 KB, the main-module component MUST emit empty `licenses` (milestone 053 FR-005 behavior preserved). No `tracing::warn` is emitted — most projects without SPDX headers are fine.

- **FR-004**: Layer 2 (content-based matcher) is OUT OF SCOPE for milestone 057.

- **FR-005**: Goldens MUST stay byte-identical for fixtures whose LICENSE file has no SPDX header (or whose LICENSE file is absent). Concrete fixtures: `tests/fixtures/go/simple-module/`, `tests/fixtures/go/argo-style-no-cache/argo-workflows/`. Both produce unchanged SBOM output post-057.

- **FR-006**: The pre-PR gate (`./scripts/pre-pr.sh`) MUST pass.

### Key Entities

- **License-file scanner**: a small `fn detect_main_module_license(workspace_root: &Path) -> Option<String>` that walks the candidate-file priority list, reads the first 4 KB, scans for the SPDX header, parses via `try_canonical`. Pure I/O + parse; no async.
- **SPDX-License-Identifier header line**: the substring `SPDX-License-Identifier:` followed by a license expression up to end-of-line. The parser strips leading/trailing whitespace and surrounding comment markers (`<!-- ... -->`, `// ...`, `# ...`).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A synthesized fixture with `LICENSE` containing `SPDX-License-Identifier: Apache-2.0` produces a main-module component whose `licenses` field contains `Apache-2.0` (asserted in unit tests).
- **SC-002**: The existing `tests/fixtures/go/simple-module/` and `tests/fixtures/go/argo-style-no-cache/` goldens stay byte-identical post-057.
- **SC-003**: An unparseable SPDX expression produces empty `licenses` AND a `tracing::warn` line.
- **SC-004**: Pre-PR gate passes.

## Assumptions

- **`spdx::SpdxExpression::try_canonical` accepts standard SPDX expressions.** Verified by milestone 010's pipeline.
- **The first SPDX header line in the file is the project's intended declaration.** Multi-LICENSE-file projects are an edge case for follow-up.
- **No new crate.** Standard library `std::fs::File` + `std::io::Read`; existing `spdx` crate for canonicalization.
- **Out of scope**: Layer 2 (content matcher), multi-LICENSE-file aggregation, per-ecosystem main-module LICENSE detection (npm/cargo/maven/pip/gem — tracked in #104 follow-ups).
