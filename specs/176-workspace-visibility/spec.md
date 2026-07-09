# Feature Specification: Monorepo workspace-member visibility for scoped SBOM consumption

**Feature Branch**: `176-workspace-visibility`
**Created**: 2026-07-08
**Status**: Draft
**Input**: User description: "m176 — surface which workspace member each component belongs to inside monorepo scans. Motivation: langflow (9 pypi + 2 npm workspace members) and test-tensorflow-models (3 requirements.txt files across `official/`, `research/deep_speech/`) scans produce a single merged SBOM where vulnerability scanners + CVE-triage teams cannot scope 'which subproject is affected'. Root-selection heuristic warns about 10-way ambiguity (langflow picked `langflow-base@0.10.2`; losers: langflow, langflow-docs, langflow-sdk, langflow-stepflow, lfx-arxiv, lfx-docling, lfx-duckduckgo, lfx-ibm, lfx). Fix via per-component `mikebom:workspace-member` annotation + doc-scope `mikebom:workspaces-detected` aggregate + at-scan-time advisory log when N > 1 workspaces detected. Zero SBOM shape restructuring — just adds annotations that enable operator-side and consumer-side per-workspace slicing via jq. Sets up structural composition (m177+) and per-workspace multi-SBOM emission (m178+) as follow-ups."

## Clarifications

### Session 2026-07-08

- Q: Does this milestone restructure the SBOM shape (nested `components[].components[]`, CDX compositions, multi-SBOM emission)? → A: **No**. m176 is purely additive: two new `mikebom:*` annotations. The SBOM's top-level `components[]` array + `dependencies[]` graph is unchanged. Structural restructuring (nested CDX composition per workspace) is a candidate follow-up milestone (m177) that would build on m176's per-component workspace tag as the substrate. Multi-SBOM emission (one SBOM per workspace) is a separate candidate follow-up (m178) blocked on operator-workflow audit against 3+ real monorepos.
- Q: What defines a "workspace" for m176's purposes? → A: **Any directory containing a package manifest that mikebom's readers currently identify as a "main module" root**. Concretely: `pyproject.toml`, `package.json`, `Cargo.toml`, `go.mod`, `pom.xml`, `Gemfile`, and equivalents already recognized by per-ecosystem readers as project boundaries (m066 npm, m068 pip, m064 cargo, m053 go, m070 maven, m069 gem, etc.). Additional entries: `requirements*.txt`, `Pipfile.lock`, `uv.lock`, `poetry.lock` at directory roots without a matching `pyproject.toml`. No new detection logic in m176 — reuses each reader's existing workspace-boundary discovery.
- Q: For a component that legitimately belongs to MULTIPLE workspaces (npm hoisted dep shared by root + subproject; Python transitive dep required by 2 workspace members' lockfiles), what does the annotation say? → A: **The annotation value is a JSON-encoded array of workspace paths, alphabetically sorted, deduplicated**. Matches the m147 peer-edge-targets shape precedent. Single-workspace components emit a 1-element array (not a bare string) for consumer-parsing uniformity.
- Q: For file-tier / unattributed components (m133 orphan-content components — shell scripts, opaque binaries, etc.), what workspace annotation do they get? → A: **NONE — the annotation is omitted entirely for these components**. Absence encodes "no workspace." Cleaner semantic than sentinel values: (1) file-tier components exist BECAUSE no manifest attributed them; making that fact structural (annotation absent) is more honest than inventing a `<scan-root>` fallback token; (2) consumers wanting to enumerate file-tier components already have the existing `mikebom:sbom-tier` / `mikebom:component-tier=file` discriminators; (3) consumer filters on `.name == "mikebom:workspace-member"` naturally exclude file-tier components — no special-case jq clause needed.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — CVE triage: filter emitted components by workspace membership (Priority: P1)

An operator's security team receives a fresh CVE affecting `pyyaml < 6.0.2`. Their SBOM (from a monorepo scan) contains 48 pypi components including one `pkg:pypi/pyyaml` with empty version. They want to answer: "which of our subprojects actually declare / lock this dep?" — the answer determines which deployment artifact needs remediation. Currently they must cross-reference `mikebom:source-files` annotations manually. Post-m176, one jq query returns the answer.

**Why this priority**: this is the highest-impact use case for the discoverability primitive. Vulnerability triage is the operator's most frequent SBOM-consuming activity; every unrelated subproject the triage team has to scope-out slows response time. Ranked P1 because the langflow + tf-models audits both surfaced this exact pain point.

**Independent Test**: scan a monorepo fixture with 2 workspaces (root + `subproject/`), each declaring a distinct dependency (e.g., root wants `numpy>=1.20`, subproject wants `pyyaml>=6.0`). Post-scan, run `jq '.components[] | select(.properties[]?.name == "mikebom:workspace-member" and (.properties[]?.value | fromjson | contains(["subproject"]))) | .purl'` — MUST return the subproject-scoped components only.

**Acceptance Scenarios**:

1. **Given** an emitted SBOM from a scan of a repo with 2 workspaces `root` + `subproject`, **When** a consumer runs a jq query filtering `mikebom:workspace-member` for value `["root"]`, **Then** they get only root-workspace components (with no subproject-only components in the result).
2. **Given** the same SBOM, **When** they filter for `["subproject"]`, **Then** they get only subproject-workspace components.
3. **Given** a transitive dep that legitimately belongs to BOTH workspaces (both lockfiles pin the same package version), **When** they filter for `["root"]` OR `["subproject"]`, **Then** the shared dep appears in BOTH filters (its annotation is `["root", "subproject"]`, matching both single-workspace filters via array-containment semantics).

---

### User Story 2 — Advisory log surfaces monorepo shape at scan time (Priority: P1)

An operator invokes `mikebom sbom scan --path <monorepo>` in CI. The scan detects N > 1 workspace roots. Instead of quietly emitting a single merged SBOM, mikebom emits one INFO-level advisory log line naming the detected workspaces + pointing at the operator options for per-workspace consumption. Grep-stable substring for CI dashboards.

**Why this priority**: matches the m173 + m175 advisory-log pattern — at-scan-time hints are load-bearing for operator discoverability. Without the log, only operators who read the reading guide (or the SBOM they emitted) will realize their scan is monorepo-shaped. Ranked P1 alongside US1 because the two compose: US1 gives per-workspace slicing power; US2 tells operators the power exists.

**Independent Test**: scan a fixture with 3 workspace roots. Assert stderr contains exactly one INFO-level log line naming all 3 workspace paths AND containing a stable substring pointing operators at the per-workspace-jq-slicing recipe in the reading guide.

**Acceptance Scenarios**:

1. **Given** a scan target with N > 1 detected workspace roots, **When** the scan completes, **Then** exactly ONE INFO-level advisory log line is emitted to stderr containing: (a) the count of workspace roots N, (b) each workspace's root-relative path, (c) a stable substring pointing at the reading-guide's monorepo section for scoped consumption recipes.
2. **Given** a scan target with N = 1 workspace root (single-project layout), **When** the scan completes, **Then** NO advisory log line is emitted (nothing multi-workspace to advise about).
3. **Given** a scan target with N = 0 detected workspaces (a scanned rootfs with no manifests — e.g., a raw filesystem image from a container extraction), **When** the scan completes, **Then** NO advisory log line is emitted.
4. **Given** a scan target with N > 1 workspaces AND the scan is invoked with `--offline`, **When** the scan completes, **Then** the advisory log IS still emitted — this milestone's advisory is orthogonal to `--offline` (matches the m175 pattern; the remediation is operator-side consumption logic, not network activity).

---

### User Story 3 — Doc-scope annotation enables SBOM-level workspace enumeration (Priority: P2)

A downstream SBOM-processing tool (dashboard, compliance reporter, dependency-freshness scanner) wants to enumerate every workspace present in an SBOM without walking every `components[]` entry's per-component annotation. A doc-scope annotation `mikebom:workspaces-detected` at `metadata.properties[]` (CDX 1.6) / SPDX 2.3 document annotations / SPDX 3 typed Annotation on the document root exposes the enumerated list directly.

**Why this priority**: enables "compose workspaces from the emitted SBOM" without full-component-list traversal. Ranked P2 because the primitive is derivable from US1's per-component annotation via jq — but at scale (SBOMs with thousands of components) the doc-scope aggregate is a materialization that lets consumers filter/enumerate quickly.

**Independent Test**: emit an SBOM from a 3-workspace monorepo scan; assert `metadata.properties[]` contains a `mikebom:workspaces-detected` entry with value being a JSON-encoded array of 3 workspace root-relative paths, alphabetically sorted.

**Acceptance Scenarios**:

1. **Given** an SBOM from a scan of an N-workspace monorepo, **When** a consumer runs `jq '.metadata.properties[]? | select(.name == "mikebom:workspaces-detected") | .value | fromjson' scanned.cdx.json`, **Then** they get a JSON array of exactly N workspace paths, alphabetically sorted, root-relative.
2. **Given** an SBOM from a scan target with N = 1 workspace, **When** they run the same jq, **Then** they get a 1-element array `["<workspace-root>"]`. This is deliberate — the annotation surface is unconditional for scan targets that discover any workspace at all.
3. **Given** an SBOM from a scan target with N = 0 workspaces detected, **When** they run the same jq, **Then** the annotation is ABSENT (matches other absent-vs-empty patterns like m173 C119).

---

### Edge Cases

- **Component detected at scan-root but no matching manifest**: possible for file-tier components (m133) that survived content-shape filtering — a shell script at the repo root has no enclosing package manifest. **Decision**: no `mikebom:workspace-member` annotation is emitted for these components (per FR-002). Absence encodes "no workspace attribution." Consumers wanting to enumerate file-tier components have the existing `mikebom:sbom-tier` / `mikebom:component-tier=file` discriminators. Workspace-scoped filters (`.name == "mikebom:workspace-member"`) naturally return only workspace-attributable components via absence-selection.
- **Nested workspaces**: a `pyproject.toml` at `<root>/` AND at `<root>/subproject/` — the outer `pyproject.toml` might be a workspace parent (`[tool.uv.workspace]` with members) OR a sibling. **Decision**: reuse whatever each reader's existing main-module logic determines. If the reader already emits BOTH as separate main modules (langflow case per pip reader), both appear in `mikebom:workspaces-detected`. If the reader treats one as a child of the other, only the parent appears. m176 does not add new nesting-resolution logic.
- **Same-purl components across workspaces**: identical PURL emitted by two workspaces (e.g., both declare `pyyaml==6.0` in their lockfiles). Post-milestone-134, mikebom already collapses same-PURL duplicates into one component; the surviving component's `mikebom:workspace-member` value must be the sorted-deduplicated array of ALL originating workspaces. Consistent with the Q3 clarification.
- **Workspace path with special characters**: `<root>/services/@scope/pkg/` or `<root>/src/lib-vX.Y.Z/`. **Decision**: paths are emitted verbatim as UTF-8 strings in the JSON array. No URL encoding, no PURL normalization. Consumers can jq-string-match against the exact path they see in their filesystem layout.
- **Renamed workspace during scan lifetime**: mikebom doesn't detect this (scan is snapshot-based). The annotations reflect the paths as seen at scan time. No compensating logic.
- **`--path <root>/subproject`** (operator scans INTO a nested workspace rather than the repo root): m176 operates on whatever mikebom's readers actually detect during the given scan. If the reader identifies `subproject` as a workspace root, the annotation says `["subproject"]` (root-relative to whatever `--path` was). If no workspace is detected (bare directory), no annotations fire.
- **Windows path separators**: on Windows, workspace paths MAY contain backslashes as native path separators. **Decision**: emit as forward-slash-separated strings for cross-platform SBOM portability, matching mikebom's existing wire convention (verified by inspection of `mikebom:source-files` emission which is already forward-slash-normalized).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The tool MUST emit a per-component `mikebom:workspace-member` annotation on every component derived from a workspace-scoped source (any package-manifest-detected component). Value is a JSON-encoded array of workspace root-relative paths (forward-slash separator) that the component belongs to, alphabetically sorted, deduplicated. Single-workspace components emit a 1-element array (not a bare string).
- **FR-002**: File-tier components (m133 orphan-content components — shell scripts, opaque binaries, etc.) and any other component that is not attributable to a specific workspace MUST NOT have a `mikebom:workspace-member` annotation. Absence of the annotation is the wire-visible signal for "no workspace attribution." Consumers wanting to enumerate file-tier components have the existing `mikebom:sbom-tier` and `mikebom:component-tier=file` discriminators; workspace-scoped filters (`.name == "mikebom:workspace-member"`) naturally exclude them via absence-selection.
- **FR-003**: The tool MUST emit a document-scope `mikebom:workspaces-detected` annotation whenever the scan discovers at least ONE workspace (N >= 1). Value is a JSON-encoded array of all detected workspace root-relative paths, alphabetically sorted. The annotation is ABSENT (not present as an empty array) when zero workspaces are detected.
- **FR-004**: The tool MUST emit exactly ONE INFO-level advisory log line to stderr when the scan detects N > 1 workspaces. The log line MUST include: (a) the specific count N, (b) each workspace's root-relative path, (c) a stable grep-substring pointing at the reading-guide's monorepo section. The exact wording is prose-level detail chosen at authoring time; the stability constraint is the load-bearing requirement.
- **FR-005**: The advisory log MUST NOT fire when N <= 1 (single-project layout OR bare filesystem with no workspaces). No noise on non-monorepo scans.
- **FR-006**: The advisory log MUST NOT be gated on `--offline`. The remediation (per-workspace jq slicing) is entirely consumer-side and requires no network. Matches the m175 pattern.
- **FR-007**: The tool MUST NOT introduce any new CLI flags in this milestone. The workspace annotations are unconditional emission; the advisory log fires per the FR-004 predicate. Follow-up milestones (m177 structural composition, m178 multi-SBOM emission) MAY introduce flags.
- **FR-008**: The tool MUST NOT change any existing SBOM shape aspect — `components[]` array structure, `dependencies[]` edge graph, `metadata.component` root selection, per-component field emission (purl, version, evidence, hashes, licenses). Additive-only.
- **FR-009**: The tool MUST reuse each reader's existing workspace-boundary detection (m053 go, m064 cargo, m066 npm, m068 pip, m069 gem, m070 maven, m106 kotlin/swift, m107 yocto, etc.). No new detection logic is added in m176. If a reader currently emits a main-module component for a workspace member, that workspace path is a source of truth for FR-001 and FR-003.
- **FR-010**: The tool MUST emit workspace paths as forward-slash-separated UTF-8 strings for cross-platform SBOM portability. On Windows hosts where paths natively use backslashes, the tool MUST normalize to forward slashes before emission. This matches the existing `mikebom:source-files` wire convention.
- **FR-011**: The `mikebom:workspace-member` and `mikebom:workspaces-detected` annotations MUST be emitted across all three format outputs (CDX 1.6, SPDX 2.3, SPDX 3.0.1) using the standing envelope shape (m080 `MikebomAnnotationCommentV1` for SPDX 2.3; property on `components[]` / `metadata.properties[]` for CDX; typed Annotation graph element for SPDX 3).
- **FR-012**: The doc-scope `mikebom:workspaces-detected` annotation's value MUST equal the alphabetically-sorted, deduplicated UNION of all per-component `mikebom:workspace-member` values across the emitted SBOM. Consumers can verify by cross-checking; the tool guarantees this by construction (both derive from the same in-process workspace enumeration).
- **FR-013**: Non-monorepo scans (single-project layout, N = 1) MUST produce SBOMs whose ONLY change from pre-176 output is: (a) the addition of the per-component `mikebom:workspace-member` annotation on every workspace-attributable component (value `["<the-single-workspace>"]`; file-tier / unattributed components do NOT gain the annotation per FR-002); (b) the addition of the doc-scope `mikebom:workspaces-detected` annotation (value `["<the-single-workspace>"]`). The `components[]` list, dependencies, root selection, and every other existing annotation MUST remain byte-identical modulo these additions.

### Key Entities

- **Workspace**: a directory (relative to scan root, or the scan root itself) that mikebom's package-database readers identify as a project boundary — anchored by a manifest file (`pyproject.toml`, `package.json`, `Cargo.toml`, `go.mod`, `pom.xml`, `Gemfile`, etc.) OR a lockfile without a matching manifest (`uv.lock`, `poetry.lock`, `Pipfile.lock`, `requirements*.txt`, etc.). Represented as a forward-slash-separated root-relative path string.
- **Workspace membership**: the relation "this component was discovered via reading workspace X's manifest / lockfile / installed artifacts." A component MAY belong to multiple workspaces when the same PURL is emitted by more than one reader run (e.g., cross-workspace shared dep in a monorepo). The membership set is stored as an alphabetically-sorted, deduplicated array of workspace paths. Components with NO workspace attribution (file-tier, orphan-content) do not have the annotation at all — absence is the signal per FR-002.
- **Advisory context**: the two-input predicate driving the FR-004 advisory log — `workspaces_detected_count > 1` AND `scan_produced_at_least_one_component`. The scan-target-was-empty case is absorbed into the second predicate (no components → no advisory). No offline gating.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A consumer running `jq '.components[] | select((.properties[]? | select(.name == "mikebom:workspace-member") | .value | fromjson) | contains(["<workspace>"])) | .purl'` on an SBOM from a 2-workspace monorepo returns ONLY the components attributable to that workspace. Verified by integration test on a synthesized 2-workspace fixture.
- **SC-002**: The advisory log fires exactly once per scan when N > 1 workspaces detected AND the scan produced at least one component. Verified by integration test asserting `grep -c` on captured stderr equals 1.
- **SC-003**: The advisory log fires zero times when N = 1 OR N = 0. Verified by integration test on a single-project fixture AND a bare-directory fixture.
- **SC-004**: Every existing golden regression fixture (33 across CDX / SPDX 2.3 / SPDX 3) MUST retain byte-identical `.components[]` array structure + `.dependencies[]` edges + `metadata.component` selection + every existing annotation. The ONLY permitted delta is: (a) the addition of `mikebom:workspace-member` on every workspace-attributable component (file-tier components do NOT gain this annotation per FR-002); (b) the addition of `mikebom:workspaces-detected` at document scope. Golden regeneration IS expected but bounded to these additions. Verified by post-regeneration jq diff assertion.
- **SC-005**: An operator scanning the langflow test fixture (10 workspace members) sees exactly one advisory log line naming all 10 workspace paths + the stable grep-substring `"monorepo shape detected: "` (aligned with FR-004 stability requirement), verifiable via `grep -cF 'monorepo shape detected: ' stderr` returning 1. Verified by an integration test using an equivalent synthesized 10-workspace fixture (the langflow vendored fixture is out of test-corpus scope; the synthesized fixture is the machine-checked verification vector).
- **SC-006**: An operator scanning the test-tensorflow-models fixture (3 requirements.txt files across `official/`, `research/deep_speech/`, root) sees per-component workspace-member annotations distinguishing the three, verifiable via `jq` returning 3 distinct workspace paths across all component annotations.
- **SC-007**: A monorepo consumer who receives an SBOM can enumerate every workspace present in a single jq call: `jq '.metadata.properties[]? | select(.name == "mikebom:workspaces-detected") | .value | fromjson'` returns the full workspace list without walking `components[]`.
- **SC-008**: Non-monorepo scans (single-project layout) show byte-identical SBOM output pre-176 vs post-176 EXCEPT for the addition of the two new annotations. Verified via the golden regression suite post-regeneration: every non-added byte position is unchanged.

## Assumptions

- **Every existing package-DB reader already emits a "main module" component for each detected workspace member** — this is load-bearing per FR-009. Verified by inspection during langflow audit: 9 pypi main modules + 2 npm main modules emitted for langflow's 11 workspace members. mikebom's readers therefore already know the workspace-membership of every component they emit — m176 just surfaces that knowledge as an annotation.
- **File-tier components (m133) don't belong to any workspace by definition** — they are files that survived content-shape classification WITHOUT matching a package manifest. FR-002 handles this with the `<scan-root>` fallback.
- **Root component selection is orthogonal to workspace membership** — the m127 root-selection heuristic picks ONE component as the SBOM's `metadata.component`; the other N-1 workspace roots are still discoverable via `mikebom:workspaces-detected`. m176 does NOT change root selection.
- **Standards-native audit outcome is KEEP-NO-NATIVE** — CDX `component.group` is the closest native field ("The grouping name or identifier. This will often be a shortened, single name of the company or project that produced the component") but its semantic is "component's authoring organization / project," not "workspace this component was discovered from within the scan target." Different concept: `group` scopes to the component's identity; workspace-member scopes to the scan target's boundary. SPDX 2.3 has no analogous field; SPDX 3 `Element.namespace` scopes to identity URI generation, not workspace boundary. Milestone Constitution Principle V audit at plan phase will formally rule KEEP-NO-NATIVE for both annotations.
- **Follow-up milestones (m177, m178) can build on m176 without rework** — the per-component workspace tag is the primitive both m177 (nested CDX composition per workspace) and m178 (per-workspace multi-SBOM emission) need. Emit the tag once in m176; downstream restructurings reuse it.
- **Wire shape for the JSON-encoded array in a string-valued CDX property matches m134 / m147 / m173 precedent** — CDX property values are string-typed; the array is serialized via `serde_json::to_string(&paths)` producing the same envelope operator-facing tools already handle for `mikebom:purl-collisions-detected` / `mikebom:peer-edge-targets` / `mikebom:go-cache-warming-failed`.
- **This milestone is docs-and-annotations, not detection logic** — every workspace boundary already known to mikebom becomes visible. Workspaces mikebom fails to detect today (e.g., a bespoke build system that mikebom's readers don't recognize) remain undetected post-m176.
