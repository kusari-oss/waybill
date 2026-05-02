# Feature Specification: Go source-tree direct dependency edges via synthetic main-module component

**Feature Branch**: `053-go-main-module-edges`
**Created**: 2026-05-02
**Status**: Draft
**Input**: User description: "can we now address this issue?" — referring to issue #102 (Go source-tree scan emits zero dependency edges when GOMODCACHE is empty)

## Clarifications

### Session 2026-05-02

- Q: How aggressive should LICENSE-file detection be for the main-module component (FR-005)? → A: Skip content reading entirely; rely on the C40 role tag to exclude the main-module from sbomqs licensing-coverage denominator. Emit empty `licenses`. Real LICENSE-file detection (SPDX-License-Identifier header scan + content matcher) is deferred to follow-up issue #103.
- Q: How should the main-module component's PURL version be resolved (FR-001)? → A: VCS-tag introspection via `git describe`, with a 3-step fallback: (1) `git describe --tags --exact-match HEAD` for clean tagged releases, (2) `git describe --tags --always` for tag-with-commits-since (e.g., `v3.3.9-2-gabc1234`), (3) the literal placeholder `v0.0.0-unknown` when not in a git repo / no tags reachable / shallow clone without tag fetch. Test fixtures use tarball-style sources (no `.git` dir) so step 3 fires deterministically and goldens stay byte-identical across hosts.
- Q: How should the document root be set in polyglot scans (FR-008, US3 AS#2)? → A: Synthetic super-root that DESCRIBES every per-ecosystem main-module. The Go main-module (the only ecosystem gaining a main-module in milestone 053) is one of N described elements; existing per-ecosystem placeholders are siblings. SPDX 2.3's `documentDescribes` array supports this directly. Adding main-module components for npm / cargo / maven / pip / gem in future milestones (tracked in follow-up issue #104) extends this naturally without re-tie-breaking — each new ecosystem main-module just becomes another DESCRIBES entry.

### Comparative analysis: trivy + syft (research, 2026-05-02)

Cross-validated milestone 053's design against Trivy's `pkg/dependency/parser/golang/mod/parse.go` + `pkg/sbom/io/encode.go` and Syft's `syft/pkg/cataloger/golang/parse_go_mod.go`:

| Spec choice | Trivy | Syft | Verdict |
|-------------|-------|------|---------|
| PURL shape `pkg:golang/<mod>@<ver>` | Same | Same (with namespace split) | ✅ Match |
| Version ladder (`git describe` → placeholder) | Uses raw `m.Mod.Version` (usually empty) | Same as Trivy | ✅ **Better** — neither tool attempts VCS resolution |
| `DependsOn` for direct edges | Same (`RelationshipDependsOn` → CDX `dependsOn` / SPDX `DEPENDS_ON`) | Inverse `DependencyOfRelationship`, flipped on serialize; **only emits if Go toolchain is available** — same gap we're fixing | ✅ Match Trivy |
| LICENSE detection at workspace root | None (only inside cache + vendor for deps) | None | ✅ Consistent (deferred to #103) |
| Polyglot root structure | Single filesystem/repo as `metadata.component`, each ecosystem as `application`-type child with `Contains` edges | Single `Source` as `metadata.component`, packages flat siblings | ⚠️ Trivy's nested pattern cleaner — out-of-scope for 053, deferred |
| **Main-module placement (CDX)** | **`metadata.component` with `Root: true`, `type: application`** — native field, no custom property | Doesn't tag main-module distinctly | ❌ **Spec change**: pivot to native CDX `metadata.component` per Principle V |
| **Main-module placement (SPDX)** | `documentDescribes` + `primaryPackagePurpose: APPLICATION` | Uses `DESCRIBES` + `primaryPackagePurpose: FILE`/`CONTAINER` | ❌ **Spec change**: add `primaryPackagePurpose` to SPDX emission |
| Indirect (`// indirect`) requires | Tagged as `RelationshipIndirect`, not under root's DependsOn — but orphan indirects (no cache-resolved transitive parent) get reparented under root | Emits all requires as packages regardless | ⚠️ Deliberate divergence: mikebom emits all `// indirect` under root regardless of orphan status — see FR-002 rationale |

- Q: Should the main-module component be emitted as a flat `components[]` entry with a `mikebom:component-role` custom property (original 053 plan), or promoted to native CDX `metadata.component` + SPDX `primaryPackagePurpose: APPLICATION` (Trivy's approach)? → A: Native fields per Principle V. The CDX main-module is emitted in `metadata.component` with `type: application` (not in `components[]` as a sibling); the SPDX main-module sets `primaryPackagePurpose: APPLICATION` and is the target of `documentDescribes` / `DESCRIBES`. The `mikebom:component-role: main-module` property remains as a supplementary signal (still useful for SPDX-2.3 consumers reading annotations and for CDX consumers reading nested component lists), but it is no longer the primary mechanism. This matches Trivy's pattern, satisfies Principle V, and is more standards-faithful than the original spec draft.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Fresh-clone Go scan emits direct dependency edges (Priority: P1)

A developer or CI pipeline runs `mikebom sbom scan --path <go-project>` against a Go repository that has just been cloned (no `go mod download`, no `go build`, no populated module cache on the host). The resulting SBOM carries dependency edges from the project's main module to every direct require declared in `go.mod`, so consumers can answer "what does this project directly depend on?" without needing the host to have run a build first.

**Why this priority**: This is the dominant Go-scanning workflow — every CI runner that scans a fresh checkout, every developer running mikebom on a freshly-cloned repo, every container image's source-tree scan. Today these scans produce a packages-only SBOM with zero dependency graph (1 relationship total: just the document `DESCRIBES`), making the SBOM useful only for vulnerability intersection and useless for impact analysis or transitive blast-radius queries. Closes the parity gap with trivy and syft, both of which emit direct edges from a synthetic main-module component in the same scenario.

**Independent Test**: Clone any Go repo with at least one `require` in `go.mod` (e.g., `git clone --depth 1 --branch v3.3.9 https://github.com/argoproj/argo-workflows.git`), run `mikebom sbom scan --path <repo> --format spdx-2.3-json --output sbom.json --no-deep-hash` with an explicitly-empty `GOMODCACHE` env var, and verify the output's `relationships[]` contains at least one `DEPENDS_ON` per `require` entry in the project's `go.mod`. Independently delivers the issue's primary value (a non-empty dep graph in the dominant workflow).

**Acceptance Scenarios**:

1. **Given** a freshly-cloned Go project with `go.mod` declaring N direct require modules and an empty GOMODCACHE on the host, **When** `mikebom sbom scan --path <project> --offline` runs, **Then** the resulting SBOM contains at least N `DEPENDS_ON` edges originating from a single component representing the project's main module, with each edge targeting a distinct direct-require module.
2. **Given** the same project with a populated GOMODCACHE that resolves transitive dep info for some-but-not-all direct requires, **When** mikebom scans, **Then** direct edges still emit for all N requires (cache absence does not suppress direct edges) AND transitive edges emit for the modules whose `.mod` files are cache-resolvable (no regression on existing transitive-edge behavior).
3. **Given** a Go project whose `go.mod` declares zero direct requires (toy "hello world" with stdlib only), **When** mikebom scans, **Then** the SBOM emits the main-module component but no `DEPENDS_ON` edges from it (zero-require case is handled cleanly, no synthetic edges fabricated).
4. **Given** the argo-workflows v3.3.9 reproduction case from issue #102 (cloned without `go mod download`, scanned offline), **When** mikebom scans with the new behavior, **Then** the resulting SPDX 2.3 SBOM contains at least 14 `DEPENDS_ON` edges (matching the count of direct requires in argo's `go.mod`), where pre-053 the same scan emitted 1 relationship total.

---

### User Story 2 - Main-module component is identifiable and excludable (Priority: P2)

A consumer of the resulting SBOM (sbomqs, an internal license-compliance tool, a vuln-intersection tool, etc.) needs to distinguish the synthetic main-module component from real third-party dependencies — for licensing-coverage scoring (the project's own module has no upstream license metadata, so counting it against coverage would skew scores), for vulnerability lookup (the project itself is not in vuln databases), and for visualization (the main module is the root of the dep tree, not a leaf).

**Why this priority**: Without a way to identify the synthetic component, downstream tools either count it as a dep with missing metadata (unfair licensing-coverage penalty) or surface it as an item to vuln-scan (false-positive lookups, no hits, wasted CI time). The existing `mikebom:component-role` catalog row (C40, milestone 048) already declares a `main-module` value reserved for exactly this use; we just need to emit it on the new component. Without this signal, the change ships a regression for sbomqs scoring on existing Go fixtures.

**Independent Test**: Run a Go scan that produces the new main-module component, parse the resulting SBOM, and verify the main-module package carries `mikebom:component-role: main-module` (CycloneDX property) AND the equivalent SPDX 2.3 annotation AND the SPDX 3 native field as defined by the C40 catalog row. Independently testable via parity-extractor C40 in `tests/holistic_parity.rs`.

**Acceptance Scenarios**:

1. **Given** a Go scan producing a main-module component, **When** the SBOM is rendered as CycloneDX 1.6, **Then** the main-module component carries `properties[].name = "mikebom:component-role"` with `value = "main-module"`.
2. **Given** the same scan, **When** rendered as SPDX 2.3, **Then** the main-module package carries the equivalent `mikebom:component-role: main-module` annotation per C40's annotation envelope.
3. **Given** the same scan, **When** rendered as SPDX 3.0.1, **Then** the main-module element carries the C40-mapped native field representing the main-module role.
4. **Given** the new main-module component, **When** sbomqs runs against the SBOM, **Then** the licensing-coverage score is not degraded relative to pre-053 baselines (the main-module component is excluded from "components requiring license" denominator OR carries a license declaration drawn from a `LICENSE`/`LICENSE.md` file at the project root when one exists).

---

### User Story 3 - Document root points at the main-module component (Priority: P3)

Consumers reading the SPDX `documentDescribes` / CycloneDX BOM root expect a single subject component that represents "what was scanned." Today mikebom emits a synthetic `SPDXRef-DocumentRoot-...` placeholder for this slot when no obvious root exists. With a Go workspace having a real main-module component, the document-describes pointer should target that component instead of the placeholder, so SPDX-tree-walking tools surface the project's own module as the SBOM subject.

**Why this priority**: Cosmetic and tool-friendliness improvement — most consumers don't follow the documentDescribes pointer for actionable data, but tools that DO (e.g., sbomqs root-resolution scoring, GitHub dep-tree visualizations) get a more accurate and useful root. Lower priority than US1/US2 because the dependency-graph value is already delivered by US1; this is a polish layer on top.

**Independent Test**: Run a Go scan, inspect the output's `documentDescribes[]` (SPDX 2.3) / `relationships[]` describing the document (SPDX 3 / CDX equivalent), and verify it points at the main-module component's SPDXID (or BOM-ref) — not at a synthetic `DocumentRoot-*` placeholder.

**Acceptance Scenarios**:

1. **Given** a Go-only project scan, **When** the SBOM is rendered as SPDX 2.3, **Then** `documentDescribes` contains exactly the SPDXID of the main-module component, and the document's `DESCRIBES` relationship targets that same SPDXID.
2. **Given** a polyglot project (Go + npm + maven), **When** mikebom scans, **Then** the document root is a synthetic super-root component whose `DESCRIBES` relationship targets the Go main-module AND every existing per-ecosystem placeholder root, in deterministic ecosystem-name-sorted order. No primary ecosystem is picked; every described element is a sibling.

---

### Edge Cases

- **Empty go.mod (no requires, only `module` directive)**: Emit the main-module component with zero outgoing edges. Don't fabricate edges.
- **No go.mod at all (Go binary present but no source)**: Out of scope — this code path runs only for source-tree scans where `go.mod` is found. The Go-binary BuildInfo path (already produces main-module edges from BuildInfo) is unchanged.
- **`replace` directives in go.mod**: A `require X v1.0.0` paired with `replace X => ./internal/X` produces a direct edge to the replaced module — apply the existing `apply_replace_and_exclude` logic to direct requires before emitting edges.
- **`exclude` directives**: A `require X v1.0.0` paired with `exclude X v1.0.0` produces no direct edge for X (it's been explicitly removed from the build). Apply `apply_replace_and_exclude` consistently.
- **Multiple Go workspace roots in one scan**: A monorepo with `go.work` declaring multiple `go.mod` files. The existing `golang::candidate_project_roots()` walker discovers every `go.mod` regardless of `go.work` parsing, so each workspace member directory naturally gets its own main-module component via the new `build_main_module_entry()` (one call per discovered `go.mod`). No separate `go.work` parsing is required — milestone 053 does NOT add a `go.work` parser. The doc root case (US3) for this scenario falls into case 3 of the existing SPDX root-selection algorithm (multiple top-levels → synthetic super-root DESCRIBES each per-module main), already covered by T024 + T025. Verified by the existing walker's behavior; no new FR/task needed for `go.work` specifically.
- **Indirect (`// indirect`) requires**: These are still direct in the sense that `go.mod` declares them at the project root level; they're "indirect" only in the upstream-dep sense. Emit edges from main-module to indirect requires as well — they ARE in the project's dep closure and consumers want them visible. **Deliberate divergence from Trivy**: Trivy tags `// indirect` requires as `RelationshipIndirect` and excludes them from the root's DependsOn unless they're orphan. Mikebom's simpler approach (every go.mod-declared require gets a root edge, indirect or not) gives offline scans more edges to work with — the issue #102 case has zero cache, so under Trivy's rules indirect requires would be orphan-reparented anyway. Net behavior is similar; the implementation is simpler. Consumers that need the direct-vs-indirect distinction can read each component's existing classification metadata.
- **LICENSE file present or absent**: Either way, milestone 053 emits the main-module component with empty `licenses` and relies solely on the C40 role tag to exclude it from sbomqs coverage scoring. LICENSE-file detection is deferred to follow-up issue #103. Acceptance tests must verify sbomqs doesn't regress (US2 AS#4).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: For every Go workspace root discovered during a source-tree scan (every directory containing a `go.mod`), mikebom MUST emit a single component representing the workspace's main module, with PURL `pkg:golang/<module-path>@<version>`. The `<version>` field MUST be resolved by the following 3-step fallback ladder: (1) `git describe --tags --exact-match HEAD` for clean tagged releases (yields e.g. `v3.3.9`); (2) `git describe --tags --always` for tag-with-commits-since (yields e.g. `v3.3.9-2-gabc1234`); (3) the literal placeholder `v0.0.0-unknown` when not in a git repo, when no tags are reachable, or when a shallow clone elided all tags. Step 3 firing is the deterministic golden case used by all integration-test fixtures (which use tarball-style sources with no `.git` directory) so cross-host byte identity is preserved.

- **FR-001a (placement)**: The main-module component MUST be emitted via each format's standards-native "BOM subject" construct, not as a sibling of regular dependency components. Specifically:
  - **CycloneDX 1.6**: emit the main-module as `metadata.component` with `type: "application"` (matching Trivy's `Root: true` pattern). The main-module MUST NOT also appear in the top-level `components[]` array — sibling-emission is the pre-053 pattern this milestone replaces. Edges from `metadata.component` to direct requires use the existing `dependencies[]` block.
  - **SPDX 2.3**: emit the main-module as a regular `packages[]` entry (SPDX 2.3 does not have a native top-level "subject" outside the document level), but set `primaryPackagePurpose: "APPLICATION"` on the package and ensure `documentDescribes` (and the corresponding `SPDXRef-DOCUMENT DESCRIBES <main-module>` relationship) targets it.
  - **SPDX 3.0.1**: set `software_primaryPurpose: "application"` on the main-module element (verified present in `mikebom-cli/tests/fixtures/schemas/spdx-3.0.1.json` at the `prop_software_SoftwareArtifact_software_primaryPurpose` definition), and add a `DESCRIBES` (or v3-equivalent) relationship from the SBOM document to the main-module element.
  This satisfies Constitution Principle V (native fields take precedence). The `mikebom:component-role: main-module` property/annotation per FR-004 remains as a supplementary signal but is NOT the primary placement mechanism.
- **FR-002**: The main-module component's outgoing dependency edges MUST cover every direct require declared in the project's `go.mod`, after applying `replace` and `exclude` directives, including `// indirect` requires.
- **FR-003**: Each direct require's outgoing edge MUST be a `DependsOn` relationship targeting the corresponding `pkg:golang/<module>@<version>` component already emitted via the existing `go.sum` traversal. Dangling targets (requires whose modules don't appear in `go.sum` or as components otherwise) MUST be silently dropped, preserving the existing dangling-target convention.
- **FR-004**: The main-module component MUST also carry `mikebom:component-role: main-module` (catalog row C40) as a **supplementary** signal, emitted via the format-appropriate construct — CycloneDX `properties`, SPDX 2.3 annotation envelope, SPDX 3 native field — exactly as already wired for the C40 row. This is layered on top of FR-001a's native-field emission so consumers reading either signal (the primary native construct OR the supplementary mikebom annotation) recognize the main-module. Per Principle V the native construct is authoritative; the C40 tag exists for backwards-compat with consumers that already read C40, and for the SPDX 2.3 case where `primaryPackagePurpose: APPLICATION` is informational but doesn't carry the "this is THE project being scanned" semantics as cleanly as a custom annotation.
- **FR-005**: The main-module component MUST emit with an empty `licenses` field. Coverage parity with sbomqs is achieved via the C40 role tag (FR-004), which excludes the component from the licensing-coverage denominator. LICENSE-file content detection (`SPDX-License-Identifier` header scan, askalono-style content matching) is **out of scope** for milestone 053 and is tracked for follow-up in issue #103. If T023's manual sbomqs verification (per SC-003) shows >1pp regression vs. the pre-053 baseline, the implementer MUST halt the PR, file a follow-up issue documenting the observed gap, and request maintainer guidance before proceeding — milestone 053 does NOT in-line patch licensing detection in response to a regression.
- **FR-006**: The main-module component MUST carry `mikebom:sbom-tier: source` (the `go.mod` is the authoritative source of direct requires; this matches the existing tier-classification convention for lockfile-sourced entries).
- **FR-007**: Existing transitive-edge emission (via populated GOMODCACHE) MUST continue working unchanged. When BOTH direct requires (new) AND transitive cache lookups (existing) produce an edge to the same target, the edge emits exactly once (deduplicated by the existing edge-dedup pipeline).
- **FR-008**: The SPDX 2.3 `documentDescribes` array (and the SPDX 3 / CycloneDX equivalents) MUST point at the main-module component's identifier in Go-only scans. For polyglot scans (Go + npm + maven + …), mikebom MUST emit a synthetic super-root component whose `DESCRIBES` relationship targets the Go main-module AND every existing per-ecosystem placeholder root, in deterministic order (e.g., sorted by ecosystem name). No per-ecosystem precedence tie-break is required: every described element is a sibling. Future milestones adding main-module components to other ecosystems (issue #104) extend this list without changing the structure.
- **FR-009**: The Go binary BuildInfo path MUST NOT emit a duplicate main-module component when a source-tree scan also runs in the same invocation. The dedup MUST prefer the source-tree main-module (it carries direct-require edges that BuildInfo can't reproduce when the binary is stripped or the BuildInfo block is incomplete). The BuildInfo main module's metadata MUST be merged onto the source-tree entry per the following precedence table — BuildInfo overrides source-tree only when the source-tree value is the deterministic placeholder OR genuinely empty:
  - `version`: BuildInfo overrides ONLY when the source-tree resolved version is the literal `v0.0.0-unknown` placeholder (step 3 of the FR-001 ladder); otherwise source-tree wins.
  - `hashes`: source-tree always wins (typically empty, but if populated it's authoritative for the workspace's content; binary's BuildInfo carries hashes for the binary artifact, not the source workspace).
  - `depends`: source-tree always wins (the source-tree edges are what FR-002 requires; BuildInfo's main_depends is a parallel signal for the binary path's own main-module emission).
  - All other fields: source-tree wins on any non-empty/non-None value; BuildInfo fills in only when source-tree is None/empty.
- **FR-010**: The new main-module component MUST be excluded from `mikebom:not-linked` annotation eligibility (milestone 050) — the project's own module is by definition the linker root, never a non-linked dep.

### Key Entities

- **Main-module component**: A new component representing the Go workspace root. Carries: PURL derived from `module-path` + best-available version; `mikebom:component-role: main-module`; `mikebom:sbom-tier: source`; empty `licenses` field (LICENSE-file detection is deferred to follow-up issue #103, per FR-005 + Q1 clarification); direct-require outgoing edges. Replaces the synthetic `SPDXRef-DocumentRoot-*` placeholder for Go-only scans.
- **Direct-require edge**: A `DependsOn` relationship from the main-module component to each direct require's resolved PURL. Provenance points at the project's `go.mod` path.
- **Workspace-root placeholder (polyglot)**: For multi-ecosystem scans, a synthetic root that DESCRIBES every per-ecosystem main module. Avoids ecosystem-vs-ecosystem tie-breaks for the doc root.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Reproducing the issue #102 case (argo-workflows v3.3.9 cloned without `go mod download`, scanned offline with `--path` and SPDX 2.3 output) MUST produce ≥ 14 `DEPENDS_ON` edges (one per direct require in argo's `go.mod`); pre-053 emitted 1 relationship total.
- **SC-002**: Edge density on a representative Go fixture MUST improve from the pre-053 baseline of "1 relationship per scan when GOMODCACHE is empty" to "≥ N relationships per scan, where N is the count of direct requires in `go.mod`," with zero false edges (every emitted edge corresponds to a real require entry).
- **SC-003 (manual verification, no CI gate)**: At PR time, the author MUST run sbomqs against the new fixture's SBOM both with the milestone-053 binary and with a pre-053 baseline binary, and capture both licensing-coverage scores in the PR description. Acceptable outcome: post-053 score is within ±1 percentage point of pre-053. If the gap exceeds 1pp, halt the PR and file a follow-up issue (do NOT scope-creep this milestone into license detection; that's tracked by issue #103). This SC is intentionally a manual review checkpoint, not an automated CI gate — sbomqs is a third-party tool and adding it to mikebom CI is out of scope. Evidence is captured in the PR description per task T033.
- **SC-004**: When GOMODCACHE IS populated (current happy-path), edge counts MUST not regress relative to pre-053 — every transitive edge that pre-053 emitted continues to emit, plus the new direct edges from main-module that pre-053 did not. Net edge count increases monotonically.
- **SC-005**: SBOM consumers walking from `documentDescribes` (SPDX) or BOM root (CDX) to dependency components in a Go-only scan reach the main module as the first hop in 100% of cases, where pre-053 they reached a synthetic placeholder with no further edges.
- **SC-006**: The change MUST close issue #102: the issue's repro command produces an SBOM with non-zero `DEPENDS_ON` edges in 100% of test runs across CI lanes (linux-x86_64, macOS-latest, eBPF feature lane).
- **SC-007**: Goldens regen with byte-identical output across hosts (per the cross-host byte-identity convention) — the new main-module component's PURL, role tag, license, and edges are deterministic given the same `go.mod` + `LICENSE` inputs.

- **SC-008**: For a representative Go fixture (e.g., the argo-workflows test case), the resulting CycloneDX 1.6 output MUST place the main-module in `metadata.component` (not as a sibling in `components[]`), and the SPDX 2.3 output MUST set `primaryPackagePurpose: "APPLICATION"` on the main-module package and have `documentDescribes` target it. Verifies parity with Trivy's standards-native main-module placement (Principle V).

## Assumptions

- **Existing C40 catalog row covers the role tag**: The `mikebom:component-role: main-module` value is already declared in `docs/reference/sbom-format-mapping.md` (milestone 048, C40). No new catalog row needed; the parity-extractor wiring for C40 already handles the three-format emission.
- **Existing edge-emission loop is reusable**: The scan-pipeline edge-emission loop already converts any component's outgoing-deps list into `DependsOn` relationships. The only change needed in this module is to make sure the main-module component is part of the components vec; no new edge-emission code path.
- **Go binary path's main-module emission is the model**: The Go binary path already populates direct edges from BuildInfo and emits a main-module component. The source-tree path replicates this pattern with `go.mod`'s `requires` block as the input instead of BuildInfo.
- **Workspace-root component-suppression rationale is reversible**: The original suppression rationale (avoid self-dep, avoid sbomqs licensing drag) is preserved by the C40 role tag alone — LICENSE-file detection is deferred to follow-up issue #103 per FR-005. The C40 role tag is the sole mechanism milestone 053 uses to keep the main-module out of sbomqs's licensing-coverage denominator. If T023's manual sbomqs verification reveals C40 is not honored and produces a >1pp regression at SC-003 verification time, halt the PR and file a follow-up issue rather than scope-creep into license detection.
- **Polyglot doc-root uses a synthetic super-root**: Per the Q3 clarification, polyglot scans emit a synthetic super-root that DESCRIBES every per-ecosystem main-module / placeholder root in ecosystem-name-sorted order. Adding main-module components for npm / cargo / maven / pip / gem (follow-up issue #104) extends this list naturally — no doc-root structural change required when those land.
- **Version-resolution shells out to `git`**: FR-001's resolution ladder calls `git describe --tags ...` via subprocess. Implementations MUST handle the absence of `git` on `$PATH`, the absence of a `.git` directory in the workspace root, and `git describe` non-zero exits (e.g., due to no tags) — all three collapse to step 3 of the ladder (`v0.0.0-unknown`). Subprocess timeout: bounded (e.g., 2 s) so a hanging git invocation can't block scans indefinitely.
- **Test fixtures**: The existing Go fixtures (cargo-alongside-go monorepo, polyglot fixture, the holistic_parity ecosystem fixture set) likely need golden regen. A new dedicated argo-workflows-style fixture (a `go.mod` with multiple direct requires, no `go.sum`, no cache) should be added to lock the SC-001 case.
- **Other ecosystems unchanged**: This feature touches Go specifically. Cargo / npm / maven / pip / etc. continue to emit edges from lockfiles directly (no synthetic main-module needed). The asymmetry is documented in `docs/design-notes.md`.
