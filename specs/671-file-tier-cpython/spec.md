# Feature Specification: File-tier surfacing for source-heavy trees (SC-003 follow-up)

**Feature Branch**: `671-file-tier-cpython`
**Created**: 2026-09-01
**Status**: Draft
**Input**: User description: "sc-003"

## Context

Milestone 670 (feature `670-pip-under-detection-fix`) closed the pip under-detection gap and met SC-001 (markitdown) + SC-002 (OctoPrint) with 2.7× headroom. **SC-003 (cpython ≥ 50 pypi components) was not met** — cpython emitted 16 pypi components pre-m670 and stays at 16 post-m670.

**Root cause**: cpython **is** the reference Python implementation. It does not consume ≥ 50 PyPI packages at runtime — the stdlib is authored inside cpython, not fetched from PyPI. The declared-file ceiling (3 requirements files with ~11 unique deps + a handful of test-fixture setup.pys + 1-2 nested pyproject.tomls) is ~15-20 pypi components. **The `≥ 50 pypi` framing of SC-003 was mis-scoped.**

**Diagnostic ground-truth (2026-09-01 scan of `kusari-sandbox/test-cpython`)**:

| Signal | Count | Meaning |
|--------|------:|---------|
| Total components emitted | 187 | Combined tiers |
| — library-tier components | 129 | Declared/resolved deps + main-modules (mostly stdlib helpers + tooling) |
| — file-tier components | 58 | Currently emitted by m133 `--file-inventory=orphan` walker |
| **shape_skipped by file-tier walker** | **5890** | Files rejected by the `ContentShape` classifier's extension-based hard-exclusion list |
| `.py` files in tree | 2332 | Source that could be file-tier'd |
| `.c` / `.h` files in tree | 1107 | C source (compiler, extension modules, vendored libs) |

The **5890 shape-skipped files** are the real gap. cpython legitimately does not have ≥ 50 PyPI consumers; it DOES have thousands of unattributed source files that Principle VIII (Completeness) says should surface as file-tier components. The m133 shape allowlist excludes source-code extensions (`.py`, `.c`, `.h`, etc.) by default because most projects don't want their source tree inflated into the SBOM.

This milestone adds an **opt-in mode** that surfaces source-tree content as file-tier components, closing SC-003 for cpython (and for any other source-tree scan an operator explicitly requests).

## Clarifications

### Session 2026-09-01

- Q: Is the operator's shape-restriction list ADDITIVE (surface ANY extension named) or RESTRICTIVE (subset of FR-002 built-in list)? → A: **Restrictive**. Operator's list MUST be a subset of the FR-002 21-extension allowlist; unknown extensions fail loudly at CLI-parse time with a diagnostic naming the FR-002 allowlist. Adding new source-shape extensions requires a follow-up milestone (proper curation review, not operator-time override).
- Q: What is the CLI flag surface for activating the new mode + specifying the restriction subset? → A: Extend `--file-inventory=<value>` with a new enum value (e.g., `source-tree`); use a companion flag `--file-inventory-source-shapes=<comma-list>` for the FR-002-subset restriction. Restriction flag is meaningful only when the file-inventory mode value activates source-tree behavior; otherwise CLI MUST warn or fail (see FR-009 semantics).
- Q: When a package-DB component claims a path but the file's current SHA-256 diverges from what the reader expected (rare drift), does the file-tier walker emit or suppress? → A: **Path-based dedupe wins** (existing m133 FR-011 semantics unchanged). The package-DB reader's claim is authoritative for that path; hash drift is out of scope for this milestone (m038 deep-hash comparison territory). No special-casing; no new annotation.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Operator scans a source tree and wants file-inventory completeness (Priority: P1)

An SBOM operator scanning a source-heavy project (cpython, a Linux kernel checkout, a large Rust workspace source drop) opts into a new file-inventory mode that surfaces `.py`, `.c`, `.h`, `.rs`, `.go`, and other source-code extensions as file-tier components (SHA-256 + observed path, no PURL). Under the new mode, cpython emits ≥ 100 file-tier components covering its source tree.

**Why this priority**: Directly closes SC-003. Aligns with Constitution Principle VIII: "unattributed content — files surviving all package-DB, binary-tier, and fingerprint readers — counts toward Completeness when surfaced as file-tier components." Enables compliance workflows that require full-source-tree inventory for licensing / provenance audits.

**Independent Test**: Clone `kusari-sandbox/test-cpython`; scan with the new opt-in mode (e.g., `waybill sbom scan --path <clone> --file-inventory=<new-mode> --offline --no-deep-hash`); verify emitted CDX contains ≥ 100 file-tier components (up from 58 today). Every file-tier component MUST carry a SHA-256 hash and at least one `evidence.occurrences[].location` path.

**Acceptance Scenarios**:

1. **Given** a Python source tree with 2000+ `.py` files, **When** waybill scans it with the new opt-in mode, **Then** every `.py` file surfaces as a file-tier component with SHA-256 + path evidence (unless claimed by a package-DB reader or excluded by a user's `--exclude` flag).
2. **Given** a C source tree (cpython's `Modules/`, or a Linux kernel snapshot), **When** waybill scans it with the new mode, **Then** every `.c` and `.h` file surfaces as a file-tier component.
3. **Given** a source tree that also contains lockfiles and installed venv content, **When** waybill scans it with the new mode, **Then** file-tier components DO NOT overlap with package-DB-claimed paths (existing m133 dedupe preserved).
4. **Given** cpython (with the new opt-in mode active), **When** waybill scans it, **Then** the emitted SBOM contains ≥ 100 file-tier components (SC-003 met at total-component-count level).

---

### User Story 2 - Existing users get byte-identical output on the default path (Priority: P1)

An operator running any existing waybill workflow (default `--file-inventory=orphan`, or `--file-inventory=off`, or `--file-inventory=full`) sees byte-identical emission before vs. after this milestone. The new source-tree mode is exclusively opt-in.

**Why this priority**: Preventing SBOM inflation for the 99%+ of existing users who scan pyproject-based Python projects (markitdown, OctoPrint) and don't need source-tree coverage. Constitution Strict Boundary #5 says file-tier default-mode MUST NOT introduce duplicate components; this extends that to "MUST NOT introduce inflation for existing users."

**Independent Test**: Re-run the milestone-670 T019 sweep against all 21 kusari-sandbox `test-*` fixtures without the new flag; component counts MUST stay within ± 5% of the post-m670 baseline for every fixture. Byte-identity gate on the 6 golden test suites (cdx_regression, spdx_regression, spdx3_regression, pkg_alias_binding_us1, oci_pull_backward_compat, optional_dep_classification) MUST pass without regeneration.

**Acceptance Scenarios**:

1. **Given** the default `--file-inventory=orphan` mode, **When** waybill scans any of the 21 sweep fixtures, **Then** emitted component counts stay within ± 5% of the post-m670 baseline for every non-cpython fixture.
2. **Given** the 6 golden test suites, **When** run without regeneration, **Then** all pass byte-identical against the v0.5.0 baselines (no default-mode drift).
3. **Given** the new opt-in mode is NOT active, **When** cpython is scanned, **Then** shape-skipped count stays at 5890 (same as v0.5.0).

---

### User Story 3 - Operator can filter which source shapes surface (Priority: P2)

An operator running the new mode on a polyglot repo can restrict which source-shape extensions get surfaced (e.g., "just `.py`, not `.c`") to keep their SBOM scoped.

**Why this priority**: Not every operator wants EVERY source extension. Someone doing a Python-only audit doesn't want 1000s of `.c` files from `Modules/_ctypes/`. Enables the mode to be useful across a spectrum of source-tree audits (Python-only, C-only, all-source).

**Independent Test**: Scan cpython with a shape-filter restricting to `.py` only; verify emitted file-tier components count ~2300 and no `.c`/`.h` file-tier components appear.

**Acceptance Scenarios**:

1. **Given** cpython, **When** the operator restricts the mode to `.py` shapes only, **Then** the file-tier walker emits file-tier components ONLY for `.py` files.
2. **Given** cpython, **When** the operator restricts to `.c` and `.h`, **Then** the walker emits ONLY C-source file-tier components.
3. **Given** any restricted shape list, **When** the walker encounters a file whose extension is not in the list, **Then** the file is `shape_skipped` (same code path as when the mode is off).

---

### Edge Cases

- **Symlink loops in a source tree**: the m054 safe-walk mechanism handles this; new mode must not introduce a code path that bypasses safe-walk.
- **Very large files** (multi-GB build artifacts accidentally checked into a source tree): existing `oversize_skipped` mechanism must apply to the new mode.
- **Binary files with source-shape extensions**: a `.py` that's actually a compiled `.pyc` renamed (rare) — file-tier's SHA-256 emission is content-agnostic, so this is fine; the operator still gets a hash + path.
- **Overlap with existing package-DB coverage**: a `.py` file that's ALSO covered by a `dist-info/RECORD` entry — existing m133 dedupe on `evidence.occurrences[].location` MUST prevent double-emission.
- **`__pycache__/*.pyc` files**: opt-in mode MUST still skip these (they're derivative content, not source).
- **Test fixtures with source extensions inside vendored-lib directories**: e.g., `Modules/_ctypes/libffi_msvc/*.c`. These are vendored 3rd-party sources; surfacing them via the mode is correct (Principle VIII: unattributed content).
- **Empty files (`__init__.py` with 0 bytes)**: SHA-256 is well-defined for empty content; emit with the empty-file hash. Downstream tools can filter zero-byte components if desired.
- **Files under a user-declared `--exclude` glob**: MUST be skipped (existing m113 ExclusionSet applies uniformly).

## Requirements *(mandatory)*

### Functional Requirements

**Discovery & Emission**:

- **FR-001**: Waybill MUST provide an opt-in file-inventory mode that surfaces source-code file extensions as file-tier components. The mode is activated by extending the existing `--file-inventory=<value>` enum with a new value (e.g., `source-tree`); a companion flag `--file-inventory-source-shapes=<comma-list>` selects the FR-002 subset restriction (Q1 semantics). The new mode MUST NOT be active unless the operator explicitly requests it via the new `--file-inventory` value. The companion shape-restriction flag is meaningful only under the new mode; using it under other file-inventory modes MUST fail with a clear diagnostic.
- **FR-002**: Under the new opt-in mode, waybill MUST classify files with these extensions as file-tier candidates (case-insensitive): `.py`, `.pyi`, `.c`, `.cc`, `.cpp`, `.cxx`, `.h`, `.hh`, `.hpp`, `.rs`, `.go`, `.java`, `.kt`, `.js`, `.ts`, `.rb`, `.php`, `.cs`, `.swift`, `.m`, `.mm`. Additional extensions MAY be added in follow-up milestones.
- **FR-003**: File-tier components emitted under the new mode MUST carry: SHA-256 hash (unless the operator passes `--no-deep-hash`, in which case the hash is omitted per existing m033 semantics), `evidence.occurrences[].location` scan-root-relative path, and CDX `type: "file"` / SPDX equivalent.

**Dedupe & non-inflation**:

- **FR-004**: Files claimed by a package-DB reader (via `evidence.occurrences[].location` on any non-file-tier component) MUST NOT emit as duplicate file-tier components — the existing m133 FR-011 hybrid dedupe (path coverage + hash coverage) MUST apply uniformly to the new mode. **Path-match wins**: if a package-DB component claims the path, no file-tier emission for that path, regardless of whether the file's current SHA-256 matches what the reader recorded. Hash-drift detection is out of scope for this milestone (defer to m038 deep-hash comparison follow-ups).
- **FR-005**: Files under a user-declared `--exclude` glob MUST NOT emit as file-tier components (existing m113 ExclusionSet).
- **FR-006**: `__pycache__/**/*.pyc`, `**/*.o`, `**/*.obj`, `**/*.pyd` and similar derivative-artifact patterns MUST NOT emit under the new mode (they're not source).

**Backward compatibility**:

- **FR-007**: On the DEFAULT path (mode not active), waybill emission MUST be byte-identical to v0.5.0 output for every scan input. The new mode MUST NOT change default-path behavior in any way.
- **FR-008**: The 6 v0.5.0 golden test suites MUST continue passing without regeneration.

**Filter granularity**:

- **FR-009**: Waybill MUST allow the operator to restrict which shape extensions surface under the new mode. The operator's restriction list MUST be a **subset** of the FR-002 21-extension allowlist. If the operator names an extension NOT in FR-002 (e.g., `.md`, `.toml`), the CLI MUST fail loudly at parse time with a diagnostic naming the FR-002 allowlist — no silent acceptance, no silent skip. When the operator restricts the shape list to a valid subset, extensions outside that subset MUST be `shape_skipped` (same code path as when the mode is off). Adding new source-shape extensions beyond FR-002 requires a follow-up milestone (curation review, not operator-time override).

**Transparency**:

- **FR-010**: When the new mode is active, waybill MUST emit a document-scope annotation naming the mode + any restrictions (via a new parity-catalog row that follows the m665 `waybill:binary-scan-suppressed` shape). Downstream consumers MUST be able to detect the mode without inspecting waybill invocation state.
- **FR-011**: The existing `file_tier walker complete` INFO log line MUST continue to report `shape_skipped` count — under the new mode, the shape_skipped count reflects files ALSO not in the operator's restriction (if any); under the default mode, it stays the same as v0.5.0.

**Performance**:

- **FR-012**: Scan-time cost of the new mode MUST scale linearly with file count. SHA-256 computation is the dominant cost per file; the walker itself MUST NOT re-walk the tree or re-open files.
- **FR-013**: The new mode's byte-identity dedupe (against package-DB-claimed paths) MUST NOT quadratic-scale — the m133 FR-011 hybrid dedupe is already implemented via hash-set + path-set; the new mode reuses that mechanism.

### Key Entities

- **Source file**: a file in the scan tree whose extension matches the shape allowlist under FR-002 and which is not claimed by a package-DB reader, not under an `--exclude` glob, and not a derivative artifact under FR-006.
- **File-tier component**: existing m133 concept — a component with CDX `type: "file"`, SHA-256 hash, and `evidence.occurrences[].location` path. Under this milestone, an additional variant emits from the new source-shape mode.
- **Shape-mode restriction**: an optional operator-specified subset of the FR-002 shape list. Files whose extension isn't in the restriction get `shape_skipped`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Scanning `kusari-sandbox/test-cpython` (189 MB) with the new opt-in mode emits ≥ 100 file-tier components (up from 58 in v0.5.0). Verified by an automated fixture-integration test that gates on component-count monotonicity.
- **SC-002**: Every emitted file-tier component under the new mode carries: a SHA-256 hash (when `--deep-hash` is on, the default) OR the m033 no-hash annotation (when `--no-deep-hash` is used); AND at least one `evidence.occurrences[].location` scan-root-relative path.
- **SC-003**: On the default path (mode inactive), the 21-fixture kusari-sandbox sweep component-counts stay within ± 1% of v0.5.0 baseline (tighter than the m670 ± 5% envelope — no code path changes on default should mean no delta at all, but ± 1% allows for genuine SHA-256 nondeterminism if any).
- **SC-004**: The 6 v0.5.0 golden test suites (cdx_regression, spdx_regression, spdx3_regression, pkg_alias_binding_us1, oci_pull_backward_compat, optional_dep_classification) pass byte-identical without regeneration.
- **SC-005**: Scan wall-clock time for cpython with the new mode MUST NOT exceed 2× the current 580ms baseline — the mode adds SHA-256 hashing over ~3400 additional files, but the walker itself does not re-traverse the tree.
- **SC-006**: Restricting the new mode to `.py`-only produces ~2000-2400 file-tier components on cpython (bounded by the number of `.py` files under the source root minus package-DB-claimed paths).
- **SC-007**: The new mode's document-scope annotation surfaces the mode name + any active restrictions, and is verifiable via a jq path on the emitted CDX / SPDX 2.3 / SPDX 3 documents.

## Assumptions

- **Source-shape list is a build-time constant**, not a runtime config file. Adding new shape extensions requires a follow-up milestone. This keeps the mode's semantics stable and testable.
- **Opt-in via an extension of `--file-inventory=<mode>`**: the natural CLI surface is to add a new value (e.g., `--file-inventory=source-tree` or `--file-inventory=full-with-source`). Existing values (`off`, `orphan`, `full`) retain byte-identity. Specific value naming is a plan-phase decision.
- **Zero new Cargo dependencies**. The mechanism reuses m133's existing walker + hasher + dedupe. Only the `ContentShape::classify` extension-hard-exclusion list needs a mode-gated bypass.
- **No new subprocess calls, no network access**. Same in-process posture as every recent milestone.
- **Constitution Strict Boundary #5 continues to apply**: file-tier emission MUST NOT introduce duplicate components in the DEFAULT mode. This milestone extends the "no default-mode inflation" invariant to include "the new opt-in mode is fully separate from default."
- **cpython test-fixture pin (`kusari-sandbox/test-cpython`) stays stable** at its current HEAD until this milestone ships. If the pin moves, SC-001 threshold may need adjustment based on the new `.py`/`.c`/`.h` file count.
- **SHA-256 hash computation is bounded by existing 256 MB per-file cap** from m133; larger files trigger `oversize_skipped` with existing behavior.
- **New parity-catalog row for the document-scope annotation** follows the m665 C153 pattern (closed-enum value naming the mode + restriction, `SymmetricEqual` across CDX / SPDX 2.3 / SPDX 3).

## Out of Scope (v1)

- **Language-specific enrichment**: this milestone treats `.py` files identically to `.c` files — SHA-256 + path + `type: file`, no PURL, no license extraction, no import-graph analysis. Deeper Python-specific enrichment (import-graph, license detection, docstring extraction) is deferred.
- **Symbol-level fingerprint matching** for source files: m108's corpus-fingerprint enrichment matches installed-binary hashes against a corpus; extending it to `.py`-source-hash matching is deferred.
- **Auto-detection of source-tree vs artifact-tree scans**: the mode stays opt-in. No heuristic that auto-activates it based on tree shape.
- **Header-file de-duplication across include chains**: emit every `.h` file as its own component. Downstream deep-hash tooling can merge duplicates if desired.
- **Non-source extensions in the allowlist**: docs (`.md`, `.rst`), config files (`.toml`, `.yaml`), assets (`.png`) — stay `shape_skipped`. Adding these is a follow-up milestone.
- **New CLI verb**: this milestone extends an existing flag; no new `waybill <verb>` surface.
- **Reworking the m133 dedupe**: existing FR-011 hybrid dedupe is preserved verbatim. No changes to `evidence.occurrences[]` shape or dedupe semantics.
