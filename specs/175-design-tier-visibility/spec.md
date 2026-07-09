# Feature Specification: Design-tier component visibility for operators

**Feature Branch**: `175-design-tier-visibility`
**Created**: 2026-07-08
**Status**: Draft
**Input**: User description: "m175 (spec'd concurrently with paused m174) — surface design-tier components (unresolved-constraint components emitted from requirements.txt-only Python projects and similar sources) to operators via (a) reading-guide docs explaining the traceability ladder + how to get resolved versions, and (b) an at-scan-time advisory log naming the fix. Motivation: langflow + test-tensorflow-models audits showed 48-of-77 pypi components with empty version strings — technically the honest Constitution IX behavior (empty version = 'we don't know'), but operators don't discover the concept without reading source code. The wire signal is ALREADY native (empty `component.version` + `metadata.lifecycles[design]`); no new `mikebom:*` annotation is invented — Principle V standards-native precedence."

## Clarifications

### Session 2026-07-08

- Q: Should this milestone introduce a new `mikebom:*` doc-scope annotation counting design-tier components? → A: **No**. Per Constitution Principle V audit: (a) the per-component signal is already native across all 3 formats (empty `component.version`/`Package.versionInfo`/`software_Package.packageVersion` + CDX `evidence.confidence < 1.0` + technique `manifest-analysis`); (b) the doc-scope aggregate is already partially native via CDX `metadata.lifecycles[]` containing `"design"` when ≥1 design-tier component exists. A count of "how many design-tier components exist" is derivable in one jq line from the empty-version field. Inventing `mikebom:design-tier-count` would violate the standards-native precedence rule.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator discovers what "design-tier" means from the reading guide (Priority: P1)

An operator scans a Python project that has only `requirements.txt` files (no `venv/`, no `poetry.lock`, no `uv.lock`, no `Pipfile.lock`). The emitted SBOM contains dozens of components with empty `version` strings. The operator opens `docs/reference/reading-a-mikebom-sbom.md`, finds a clear explanation of the traceability ladder (deployed → source → design), understands why mikebom emits these components with empty versions (Constitution Principle IX — accuracy over fabrication), and knows exactly what to do to get resolved versions on their next scan.

**Why this priority**: this is the direct discoverability gap. Two audits (langflow, test-tensorflow-models) surfaced the same pattern — operators see empty versions and don't know if it's a bug or intentional. The reading guide fix is the primary deliverable of the milestone.

**Independent Test**: an operator new to mikebom reads only the design-tier subsection of `reading-a-mikebom-sbom.md`. Within 5 minutes they can (a) recognize a design-tier component by its native signals, (b) count them via a jq recipe from the guide, (c) name the operator-side action that would lift them to source-tier (generate a lockfile) or deployed-tier (install into a venv).

**Acceptance Scenarios**:

1. **Given** an operator reading the new reading-guide section, **When** they inspect their SBOM with the provided jq recipes, **Then** they can enumerate the design-tier components and understand each field: `version = ""`, `evidence.identity[].confidence = 0.85`, technique `manifest-analysis`, and the CDX `metadata.lifecycles[]` entry `{"phase": "design"}`.
2. **Given** an operator whose SBOM contains design-tier components, **When** they follow the reading-guide's operator-remediation subsection, **Then** they can produce a follow-up SBOM whose formerly-design-tier components are now source-tier (via `uv lock` / `poetry lock` / `pip-compile`) or deployed-tier (via install-into-venv-then-scan).
3. **Given** an operator scanning a project with NO design-tier components (fully lockfile-resolved), **When** they consult the reading guide, **Then** the subsection reads coherently as background context rather than as an active-issue diagnostic (no "your SBOM has a problem" framing).

---

### User Story 2 — Advisory log surfaces the design-tier situation inline at scan time (Priority: P1)

An operator runs `mikebom sbom scan --path <requirements.txt-only-project>` in a CI pipeline. The scan completes, produces an SBOM containing design-tier components, and emits exactly one INFO-level log line pointing at the operator-side remediation. The message names both the design-tier count and the specific fix (venv + lockfile). Grep-friendly for CI dashboards.

**Why this priority**: docs alone don't help operators who never look at docs. The advisory log matches the m173 FR-004 pattern (proven at-scan-time hint) and complements the reading guide by surfacing the concept inline.

**Independent Test**: run mikebom against a `requirements.txt`-only fixture (bare + versioned entries, no venv). Assert stderr contains exactly one line matching a stable substring naming both the design-tier count AND the venv+lockfile remediation.

**Acceptance Scenarios**:

1. **Given** a scan target with ≥1 design-tier component AND non-offline mode AND no scan-suppression override, **When** the scan completes, **Then** exactly ONE INFO-level advisory log line is emitted to stderr containing (a) the specific count of design-tier components, (b) an operator-facing remediation string naming lockfile generation and venv installation, and (c) a stable grep-substring for CI dashboards.
2. **Given** a scan target with ZERO design-tier components (fully resolved via lockfile / venv), **When** the scan completes, **Then** NO advisory log line is emitted (avoids noise when the operator's env is already correct).
3. **Given** a scan invoked with `--offline`, **When** the scan completes with ≥1 design-tier component, **Then** the advisory log IS still emitted — this milestone's advisory is orthogonal to `--offline` (unlike m173's advisory which is offline-suppressed because warming is offline-suppressed; here the operator remediation works fully offline via lockfile generation).
4. **Given** a scan invoked with a suppression flag (e.g., `--suppress-advisory-hints` or an environment variable if introduced), **When** the scan completes, **Then** NO advisory log line is emitted regardless of design-tier count.

---

### User Story 3 — Format-mapping doc codifies the KEEP-NATIVE-FIRST decision (Priority: P2)

A future contributor considers adding a `mikebom:design-tier-count` doc-scope annotation. They open `docs/reference/sbom-format-mapping.md`, find an existing row explicitly documenting the design-tier semantic + its native CDX/SPDX carriers (empty `version` + `metadata.lifecycles[design]`), and see the row is tagged **KEEP-NATIVE-FIRST** (the opposite polarity of the KEEP-NO-NATIVE audits from milestones 172/173). The contributor sees the decision was consciously made and does not proceed with the `mikebom:*` invention.

**Why this priority**: prevents architectural drift. Every past milestone that introduced a `mikebom:*` annotation went through a KEEP-NO-NATIVE audit documenting why standards had no equivalent. The MIRROR case — where standards DO have a native carrier and mikebom consciously uses it — has never been explicitly documented. Ranked P2 because it's a preventive control, not directly operator-visible; but load-bearing for future spec quality.

**Independent Test**: the sbom-format-mapping.md file contains a new row explicitly tagged **KEEP-NATIVE-FIRST**, documents the empty-version + `lifecycles[design]` native carriers across all 3 formats, and names the exact rejected `mikebom:*` invention that this row is preventing (`mikebom:design-tier-count`).

**Acceptance Scenarios**:

1. **Given** the updated `sbom-format-mapping.md` file, **When** a reviewer opens it, **Then** a new Section-C-adjacent row explicitly documents design-tier as a "native carrier already exists" case with the tag **KEEP-NATIVE-FIRST** and lists the rejected alternative `mikebom:design-tier-count` with the rationale.
2. **Given** a future contributor writing a spec that proposes adding `mikebom:design-tier-count`, **When** they consult the mapping doc during Principle V audit, **Then** they find prior-art rejection and are pointed at the existing native fields.

---

### Edge Cases

- **Non-Python design-tier components**: mikebom's `sbom_tier = "design"` isn't exclusive to pip. Ruby `Gemfile` (no `Gemfile.lock`), npm root `package.json` with no lockfile, Cargo without lockfile — any ecosystem where the reader emits from a constraint-only manifest tags as design-tier. The reading guide + advisory MUST cover the general concept, not just pip. Ecosystem-specific remediation is named per ecosystem (bundler `bundle lock`, npm `npm install` producing package-lock, `cargo generate-lockfile`, etc.).
- **Mixed-tier SBOMs**: an operator's SBOM has some design-tier + some source-tier + some deployed-tier components. The advisory log names the SPECIFIC count of design-tier ones; the reading guide clarifies that a mixed SBOM is normal (e.g., a Python project with `uv.lock` for its own deps but `requirements.txt` for a scripts subdirectory).
- **Design-tier components with a resolvable version by coincidence**: possible if a `requirements.txt` line pins exactly like `kaggle==1.7.5` (equality pin, not a range). mikebom's pip reader currently tags equality-pinned entries how? — the spec-time answer is: still tagged `sbom_tier = "design"` because the file itself is a constraint file, not a resolved lock. This may surprise operators. The reading guide explicitly notes: pin syntax does not upgrade tier; source-tier requires an actual lockfile (`.lock` extension or equivalent registry-of-resolved-versions).
- **Empty scan target**: no components emitted. Advisory log NOT emitted (matches m173's non-Go behavior — nothing to advise about).
- **Offline mode**: unlike m173's warming advisory (which is offline-suppressed because warming would be a no-op), the m175 advisory is NOT offline-suppressed — the remediation (generate a lockfile, install into venv) works fully offline. FR-002 codifies this.
- **Docs-only environments**: if a scan target contains ONLY `.md` / `.rst` / `.txt` documentation files and no manifests, mikebom emits zero components. Neither the reading guide nor advisory apply.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The `docs/reference/reading-a-mikebom-sbom.md` reference doc MUST include a new dedicated subsection titled around the design-tier concept (exact heading is prose-level detail, chosen at authoring time to fit the existing section layout). The subsection MUST explain: (a) what design-tier means, (b) the native wire signals across CDX 1.6 / SPDX 2.3 / SPDX 3.0.1, (c) the traceability ladder (deployed → source → design), (d) operator-side remediation naming at least three concrete actions (generate a lockfile per ecosystem, install into a venv, use `pip-compile`), and (e) jq recipes for counting + listing + threshold-checking design-tier components.
- **FR-002**: The `mikebom sbom scan` command MUST emit exactly one INFO-level advisory log line to stderr when ALL THREE predicates hold: (a) the scan produced at least one component tagged `sbom_tier = "design"`, (b) the operator has not passed an advisory-suppression signal (see FR-005), (c) the scan target was not empty. The advisory MUST NOT be gated on `--offline` — the remediation works offline.
- **FR-003**: The advisory log line MUST name (a) the specific count of design-tier components in the scan, (b) at least one operator-facing remediation string (venv install OR lockfile generation), and (c) a stable substring suitable for `grep -F` matching in CI dashboards. The exact wording is prose-level detail chosen at authoring time; the stability constraint is the load-bearing requirement.
- **FR-004**: The `docs/reference/sbom-format-mapping.md` mapping doc MUST include a new row (position between existing rows chosen at authoring time to fit the alphabetical or thematic sort convention) explicitly documenting the design-tier → `metadata.lifecycles[design]` (CDX) + empty `Package.versionInfo` (SPDX 2.3) + empty `software_Package.packageVersion` (SPDX 3) mapping. The row MUST be tagged **KEEP-NATIVE-FIRST** (a new tag polarity introduced by this milestone) and MUST name the rejected `mikebom:*` invention (`mikebom:design-tier-count`) with rationale.
- **FR-005**: The operator MUST have a mechanism to suppress the FR-002 advisory log without disabling other tool output. The mechanism is one of: (a) a new boolean CLI flag `--no-design-tier-advisory`, (b) reuse of an existing broader flag like `--quiet` or a hypothetical `--no-advisories` if one exists, (c) an environment variable like `MIKEBOM_SUPPRESS_ADVISORY_HINTS=1`. The exact choice is deferred to plan-phase but the operator-facing suppression capability MUST exist.
- **FR-006**: The tool MUST NOT emit any new `mikebom:*` doc-scope OR per-component annotation as part of this milestone. Constitution Principle V audit outcome is KEEP-NATIVE-FIRST for both scopes; introducing any `mikebom:*` annotation for the design-tier count / list / marker would violate the standards-native precedence rule.
- **FR-007**: The tool MUST NOT change any existing per-component field emission (`version`, `evidence.identity[].confidence`, `evidence.identity[].techniques[]`, `mikebom:sbom-tier`, `mikebom:source-files`, `mikebom:requirement-range`, etc.). The wire contract is already correct; this milestone adds only operator-UX and docs.
- **FR-008**: The docs subsection MUST include ecosystem-specific remediation guidance for at least: pip (`uv lock`, `poetry lock`, `pip-compile`, venv install), npm (`npm install` to generate `package-lock.json`), Cargo (`cargo generate-lockfile`), Ruby (`bundle lock`). Additional ecosystems MAY be covered as authoring bandwidth allows.
- **FR-009**: Non-Python scans producing design-tier components (e.g., an npm project with only a root `package.json` and no lockfile) MUST trigger the advisory log with the same wording pattern, with the remediation string ecosystem-appropriate (or ecosystem-agnostic — the exact wording is prose-level).
- **FR-010**: Golden regression fixtures for non-design-tier scans MUST remain byte-identical pre-175 vs post-175. Golden fixtures for design-tier scans (if any exist today) MAY show a new advisory log line in captured stderr but no change to the emitted SBOM bytes.

### Key Entities

- **Design-tier component**: a component whose `sbom_tier` (m002 traceability ladder R13) equals `"design"`. Semantically: the operator's manifest DECLARED this dependency but no lockfile or install evidence pins its resolved version. Wire signals: empty `version` field, `evidence.identity[].confidence` below 1.0, technique `manifest-analysis`. Contributes `"design"` to CDX `metadata.lifecycles[]`.
- **Traceability ladder**: closed ordered enum of `sbom_tier` values ascending in evidence strength: `"design"` (unlocked manifest declaration) → `"source"` (lockfile entry) → `"analyzed"` (artifact file on disk with hash) → `"deployed"` (installed package DB / installed venv) → `"build"` (eBPF trace evidence). Higher tiers imply stronger provenance; operators MAY threshold their CI on minimum tier.
- **Advisory suppression signal**: any operator-provided input (CLI flag or environment variable, exact choice at plan phase) that disables the FR-002 advisory log without silencing other tool output. Necessary for CI pipelines that intentionally scan constraint-only projects and don't want log noise.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An operator new to mikebom, given only the new reading-guide subsection, can identify design-tier components in a supplied SBOM (correctly counts them, correctly extracts one specific one) within 5 minutes.
- **SC-002**: The FR-002 advisory log fires exactly once per scan when the scan produces ≥1 design-tier component and no suppression is set. Verified by integration test asserting `grep -c` on captured stderr equals 1.
- **SC-003**: The advisory log fires zero times when the scan produces zero design-tier components. Verified by integration test on a fully-lockfile-resolved fixture.
- **SC-004**: The advisory log fires zero times when the suppression signal is set, regardless of design-tier count. Verified by integration test.
- **SC-005**: The advisory log fires exactly once when the scan is invoked with `--offline` AND produces ≥1 design-tier component. Verified by integration test; establishes that the m175 advisory is orthogonal to `--offline` (unlike m173's).
- **SC-006**: Byte-identical SBOMs pre-175 vs post-175 for all ~30 existing golden regression fixtures. Verified by the existing byte-identity golden suite showing zero delta. No new fields, no new annotations, no new SBOM bytes.
- **SC-007**: The new `sbom-format-mapping.md` row can be located by `grep -n KEEP-NATIVE-FIRST docs/reference/sbom-format-mapping.md` returning exactly one match (the m175 row), demonstrating the new tag polarity is present in the docs canon.
- **SC-008**: Operator remediation efficacy: an operator who follows the reading guide's remediation subsection on a test-tensorflow-models-shape fixture (48 design-tier pypi components) produces a follow-up scan where the design-tier count drops to zero (all 48 are now source-tier via lockfile OR deployed-tier via venv). Verified by manual audit; SC does not require automated CI enforcement.

## Assumptions

- The operator's mental model when they see empty `version` fields today is either (a) "this looks like a bug" or (b) "I have no idea what this means." The reading guide + advisory log convert this to "mikebom is telling me my scan target is constraint-only; I need a lockfile or venv." Empirical evidence: two audits (langflow, test-tensorflow-models) produced this exact confusion in the user + downstream consumers.
- Constitution Principle V's standards-native precedence applies to BOTH polarities: consciously using an existing native field is as valuable to document as consciously introducing a new `mikebom:*` annotation. The KEEP-NATIVE-FIRST tag makes the "we deliberately did NOT invent" decision reviewable.
- The advisory log wording is a UX-quality concern with prose-level detail, not a spec-level correctness constraint. The load-bearing requirements are (a) the log is emitted under the FR-002 predicate, (b) it names both the count and the remediation, (c) it carries a stable substring for CI grep. The exact prose is chosen at authoring time and MAY be iterated post-ship without spec churn.
- The advisory suppression signal SHOULD align with any existing mikebom-wide advisory suppression convention if one emerges (m173's advisory does NOT have an explicit suppression flag today — it's implicitly suppressed when the operator explicitly sets `--warm-go-cache=off`, which isn't quite the same as suppression). The plan phase may consolidate into a shared mechanism.
- Design-tier is not a defect: mikebom's emission of empty-version components IS the accuracy-honest behavior. The advisory log's tone MUST be informational, not error-shaped — the SBOM is CORRECT, and the operator's scan-input state is what could improve.
- The reading guide's operator-remediation subsection MUST NOT recommend `pip install <manifest>` without a virtualenv — global installs pollute the operator's system Python. Every remediation MUST assume venv isolation or explicit alternative.
