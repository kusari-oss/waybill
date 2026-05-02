# Implementation Plan: Go source-tree direct dependency edges via synthetic main-module component

**Branch**: `053-go-main-module-edges` | **Date**: 2026-05-02 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/053-go-main-module-edges/spec.md`

## Summary

Add a synthetic main-module component for Go workspace roots so the SBOM emits direct `dependsOn` edges to every `require` in the project's `go.mod`, even when the host's GOMODCACHE is empty. The component is placed in standards-native "BOM subject" slots (CDX `metadata.component`, SPDX `documentDescribes` + `primaryPackagePurpose: APPLICATION`) per Constitution Principle V, with `mikebom:component-role: main-module` (C40) layered as a supplementary signal. Closes issue #102; matches Trivy's pattern; follow-ups #103 (LICENSE detection) and #104 (per-ecosystem main-modules) tracked separately.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–052; no nightly).
**Primary Dependencies**: Existing only — `serde`/`serde_json`, `tracing`, `anyhow`, `tempfile`. **No new crates.** The version-resolution ladder shells out to `git describe`; `git` is already an implicit project assumption (workspace itself is a git repo, CI uses git).
**Storage**: N/A — all state in-process per scan; no persistence.
**Testing**: `cargo +stable test --workspace` (unit + integration); existing golden infrastructure regen for Go-bearing fixtures; new dedicated argo-style fixture for the issue-#102 reproduction case.
**Target Platform**: Linux + macOS (matching existing CI lanes — linux-x86_64, linux-x86_64-ebpf, macos-latest). Windows not in scope; mikebom doesn't support it today.
**Project Type**: CLI (workspace-rooted; reuses `mikebom-cli/src/scan_fs/package_db/golang.rs` and adjacent generate-side modules).
**Performance Goals**: One additional `git describe` subprocess per scanned go.mod (capped via 2 s timeout). Sub-millisecond impact per scan; well under existing perf budgets. Does not affect the dual-format perf gate (`tests/dual_format_perf.rs`) since the synthetic-fixture there uses tarball-style sources (no `.git`) which falls to step 3 of the version ladder.
**Constraints**: Cross-host byte-identity for goldens (test fixtures use tarball-style sources, no `.git` dir → version is always the deterministic `v0.0.0-unknown` placeholder). No new heap allocations on the hot path beyond what's needed to construct one extra `PackageDbEntry` per workspace root. `git describe` subprocess MUST honor a 2 s timeout to prevent hanging scans on broken/partial repos.
**Scale/Scope**: One main-module component per `go.mod` discovered (typical scans: 0–1 in single-project repos, 2–10 in monorepos with `go.work`). Edge count grows by N (N = direct requires count, typically 5–80) per workspace root.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Per `.specify/memory/constitution.md` v1.4.0:

| Principle | Status | Notes |
|-----------|--------|-------|
| **I. Pure Rust, Zero C** | ✅ Pass | All changes in pure Rust; `git` subprocess is already an OS-provided binary, not a C dependency we ship. |
| **II. eBPF-Only Observation** | ✅ Pass | This feature is enrichment-of-already-discovered-components; the main-module component represents the workspace root that's already implicitly the scan target. No new dependency-discovery surface. The Go reader continues to read `go.mod` for dep relationships per Principle XII (enrichment). |
| **III. Fail Closed** | ✅ Pass | When the version-resolution ladder's `git describe` fails (no `.git`, no tags, subprocess timeout, `git` missing from `$PATH`), the implementation falls to step 3 (literal `v0.0.0-unknown`) — a transparent, deterministic fallback rather than a heuristic guess. Edge emission is unaffected by version-resolution outcome. |
| **IV. Type-Driven Correctness** | ✅ Pass | The new main-module entry uses the existing `PackageDbEntry` newtype struct + existing `Purl` newtype. No raw `String` cross-boundary use. Production code uses `anyhow::Result` for the new `git describe` subprocess wrapping; `.unwrap()` only inside `#[cfg(test)]` modules guarded by `#[cfg_attr(test, allow(clippy::unwrap_used))]`. |
| **V. Specification Compliance** | ✅ Pass — **AUDIT PERFORMED** | Per Q4 in `spec.md` (Comparative analysis): the C40 `mikebom:component-role: main-module` property is **NOT** the primary signal. Primary signal is native CDX `metadata.component` with `type: "application"`, and SPDX `primaryPackagePurpose: "APPLICATION"` + `documentDescribes`. C40 is supplementary (preserves existing-consumer compat + carries the finer "this is the project itself, not just an APPLICATION" semantic). Audit recorded in spec FR-001a + Comparative-analysis subsection. PURL emitted conforms to PURL spec (`pkg:golang/<module>@<version>`). |
| **VI. Three-Crate Architecture** | ✅ Pass | All changes within `mikebom-cli/`. No new crates. |
| **VII. Test Isolation** | ✅ Pass | New tests are unit-level (`build_main_module_entry`, version-resolution ladder, license-empty assertion) + integration-level (golden regen for argo-style fixture), all running without elevated privileges. No eBPF involvement. |
| **VIII. Completeness** | ✅ Pass | This feature *increases* completeness — adds direct-edge data that was silently absent pre-053 in the offline scan case. Makes the offline-scan failure mode (zero edges) into a success mode. |
| **IX. Accuracy** | ✅ Pass | Direct edges only emit when their target resolves to a component already in the scan (existing dangling-target dedup logic). No phantom edges. The placeholder `v0.0.0-unknown` is a transparent fallback, not a fabricated identifier — consumers can recognize the literal as "version not knowable from this scan." |
| **X. Transparency** | ✅ Pass | Version-resolution outcome is observable: the user can see whether the main-module's PURL is a real tag (`v3.3.9`), a tag-with-commits-since (`v3.3.9-2-gabc`), or the placeholder (`v0.0.0-unknown`). C40 role tag declares "this is the project itself" so consumers don't mistake it for a real upstream dep. |
| **XI. Enrichment** | ✅ Pass | LICENSE detection is *deferred* to issue #103 explicitly (not silently skipped) per the comparative-analysis result that neither Trivy nor Syft attempts LICENSE detection at the workspace root either. |
| **XII. External Data Source Enrichment** | ✅ Pass | The `go.mod` direct-require list is read for the dep-tree relationships per Principle XII bullet 1 ("dependency relationships from lockfiles"). The main-module component itself represents the workspace root which IS the scan target (not a new component imported from the lockfile); per the Strict Boundaries clause #1, lockfiles MAY be read for enrichment, MUST NOT introduce components not observed. The main-module is "observed" in the trivial sense that the scan literally targets the workspace; its existence is implied by the scan command itself. |
| **Strict Boundary #1 (No lockfile-based dependency discovery)** | ✅ Pass | Same reasoning as Principle XII above — `go.mod` is used to add edges between components that are otherwise either (a) the workspace root itself or (b) already in `go.sum`. No new components are introduced *from* the lockfile. |

**Gate result**: Pass. No constitution violations; no Complexity Tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/053-go-main-module-edges/
├── plan.md              # This file
├── spec.md              # Feature specification (Q1–Q4 clarifications recorded)
├── research.md          # Comparative analysis (trivy + syft) → recorded design decisions
├── data-model.md        # Phase 1 output (this run)
├── quickstart.md        # Phase 1 output (this run)
├── contracts/
│   └── main-module-component.md   # Phase 1 output: native-field placement contract per format
├── checklists/
│   └── requirements.md  # Spec-quality checklist (all items pass)
└── tasks.md             # Phase 2 output (created by /speckit.tasks)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── scan_fs/
│   │   └── package_db/
│   │       ├── golang.rs                    # ⬅️ MAIN CHANGE — new build_main_module_entry()
│   │       │                                #     + extend `read()` to emit it as an additional entry
│   │       │                                #     + new vcs_version_resolver() helper for the
│   │       │                                #       3-step `git describe` ladder (with 2 s timeout)
│   │       └── go_binary.rs                 # ⬅️ DEDUP — guard against double-emission when
│   │                                        #     source-tree main-module also emits;
│   │                                        #     prefer source-tree entry, merge BuildInfo
│   │                                        #     version override only if source-tree got
│   │                                        #     placeholder
│   ├── generate/
│   │   ├── cyclonedx/
│   │   │   ├── metadata.rs                  # ⬅️ ROOT-SUBJECT — when a Go main-module is
│   │   │   │                                #     present, use IT as `metadata.component`
│   │   │   │                                #     (PURL + name + version) instead of the
│   │   │   │                                #     synthetic `pkg:generic/...` placeholder
│   │   │   │                                #     and EXCLUDE the same component from the
│   │   │   │                                #     top-level `components[]` to avoid
│   │   │   │                                #     duplication
│   │   │   ├── builder.rs                   # ⬅️ EXCLUSION — filter the main-module from the
│   │   │   │                                #     normal components-builder pass
│   │   │   └── dependencies.rs              # (no change — existing edge-emission already
│   │   │                                    #  picks up edges whose `from` is the main-module
│   │   │                                    #  PURL; metadata.component dependencies pivot
│   │   │                                    #  through the same `dependencies[].ref`)
│   │   └── spdx/
│   │       ├── packages.rs                  # ⬅️ NEW FIELD — add `primary_package_purpose:
│   │       │                                #     Option<SpdxPrimaryPackagePurpose>` to
│   │       │                                #     `SpdxPackage` (skip_serializing_if =
│   │       │                                #     "Option::is_none"); set to APPLICATION
│   │       │                                #     for the main-module entry only
│   │       ├── document.rs                  # ⬅️ ROOT-SELECTION — existing
│   │       │                                #     `build_document::root_id` algorithm
│   │       │                                #     (case 1 / case 3) ALREADY picks a
│   │       │                                #     top-level component when one exists;
│   │       │                                #     only change is to ensure the main-module
│   │       │                                #     entry has `parent_purl: None` so it
│   │       │                                #     qualifies as top-level — verified by
│   │       │                                #     reading the existing algorithm
│   │       └── v3_relationships.rs          # ⬅️ SPDX 3 DESCRIBES — extend the existing
│   │                                        #     "DESCRIBES from doc to root package"
│   │                                        #     emission to point at the main-module
│   │                                        #     spdxid; add v3-equivalent of
│   │                                        #     primaryPackagePurpose if v3 has one
│   ├── parity/
│   │   └── extractors/
│   │       └── mod.rs                       # (no change — C40 catalog row already wired
│   │                                        #  for `mikebom:component-role`; the supplementary
│   │                                        #  signal continues to emit per existing wiring)
│   └── cli/
│       └── scan_cmd.rs                      # (no change — main-module emission is internal
│                                            #  to the Go reader; CLI flags unchanged)
└── tests/
    ├── fixtures/
    │   ├── go/
    │   │   └── argo-style-no-cache/         # ⬅️ NEW FIXTURE — tarball-style Go project
    │   │                                    #     with go.mod declaring N direct requires
    │   │                                    #     (mirrors argo-workflows v3.3.9's shape but
    │   │                                    #     trimmed; ~14 requires for SC-001 testing)
    │   └── golden/
    │       ├── cyclonedx/
    │       │   └── go-source-fixture.cdx.json   # regen: main-module promoted to
    │       │                                    # metadata.component
    │       ├── spdx-2.3/
    │       │   └── go-source-fixture.spdx.json  # regen: primaryPackagePurpose:
    │       │                                    # APPLICATION on main-module package;
    │       │                                    # documentDescribes targets it
    │       └── spdx-3/
    │           └── go-source-fixture.spdx3.json # regen: v3 equivalent
    ├── scan_go.rs                            # ⬅️ NEW TESTS — main-module edge emission
    │                                         #     (US1 acceptance scenarios 1–4),
    │                                         #     C40 supplementary tag (US2 AS#1–4),
    │                                         #     documentDescribes targeting (US3 AS#1)
    └── holistic_parity.rs                    # (existing C40 parity test continues to work
                                              #  via the supplementary tag)

docs/
├── reference/
│   └── sbom-format-mapping.md               # ⬅️ DOC UPDATE — annotate C40 row with
│                                            #     "primary signal is native fields per
│                                            #     milestone 053; C40 is supplementary"
└── design-notes.md                          # ⬅️ DOC UPDATE — new section documenting the
                                             #     Go-vs-other-ecosystems asymmetry (Go gets
                                             #     a synthetic main-module; npm/cargo/maven/
                                             #     etc. don't yet, tracked in #104)

CHANGELOG.md                                 # ⬅️ DOC UPDATE — `[Unreleased]` →
                                             #     `### Changed (BREAKING — SBOM output
                                             #     shape, milestone 053)` entry: Go scans
                                             #     now emit a main-module component;
                                             #     `metadata.component` shifts from
                                             #     pkg:generic placeholder to
                                             #     pkg:golang/<mod>@<ver>; goldens regen
```

**Structure Decision**: Single-crate (`mikebom-cli`) feature. The Go reader (`scan_fs/package_db/golang.rs`) emits the new entry; the CDX + SPDX generators are taught to recognize it as the BOM subject (CDX `metadata.component`, SPDX `documentDescribes` + `primaryPackagePurpose`). No new crate, no new top-level module, no API surface change at the CLI layer. Mirror-fixture (`tests/fixtures/go/argo-style-no-cache/`) locks the SC-001 reproduction case.

## Phase 0: Outline & Research — COMPLETE

`research.md` was authored during `/speckit.clarify` (after Q1–Q3) when the user requested a comparative-analysis pass against trivy + syft. It captures decisions for all seven spec choices (PURL shape, version ladder, component-role tagging, `DependsOn`, indirect requires, polyglot doc-root, LICENSE detection) with primary-source citations from both tools' source code.

Key decisions recorded there:

- **Decision**: Native CDX `metadata.component` + SPDX `primaryPackagePurpose: APPLICATION` for placement (not flat `components[]` sibling). **Rationale**: Trivy's pattern, Principle V. **Alternatives considered**: flat sibling with `mikebom:` property only (rejected per Principle V audit); both flat AND `metadata.component` (rejected as redundant duplication).
- **Decision**: 3-step git-describe version ladder (exact-tag → tag-with-commits-since → `v0.0.0-unknown`). **Rationale**: Better than trivy's empty-version-from-go.mod-line. **Alternatives**: always placeholder (rejected per Q2 user feedback that it misses the point); commit-SHA-only (rejected — breaks across-host byte identity for the same workspace at different checkouts).
- **Decision**: All `// indirect` requires get root edges. **Rationale**: simpler implementation; offline scans benefit; matches existing dangling-edge-dedup convention. **Alternatives**: trivy's "only direct under root, with orphan-reparenting" (rejected as over-complex for the marginal accuracy gain — net behavior is similar in the offline case anyway).
- **Decision**: LICENSE detection deferred to #103, emit empty `licenses` on main-module. **Rationale**: matches trivy + syft baseline; C40 role tag preserves sbomqs coverage parity. **Alternatives**: full askalono content matcher (rejected — heavy dep, scope creep); `SPDX-License-Identifier` header scan only (rejected — fine but punted to follow-up to keep this milestone tight).
- **Decision**: Synthetic super-root for polyglot doc-root with multi-DESCRIBES, ecosystem-name-sorted. **Rationale**: extends to per-ecosystem main-modules (issue #104) without re-tie-breaking. **Alternatives**: trivy's nested-application pattern (rejected as out-of-scope for 053 — tracked separately).

No further Phase 0 work needed. **Output**: `research.md` (already exists).

## Phase 1: Design & Contracts

### 1. Data model

`data-model.md` (new, this run) — captures:

- **MainModuleEntry**: a `PackageDbEntry` with the following constrained shape:
  - `purl: Purl` — `pkg:golang/<module-path>@<resolved-version>` per FR-001
  - `name: String` — the `module` directive's bare path (e.g., `github.com/argoproj/argo-workflows`)
  - `version: String` — output of the version-resolution ladder (FR-001)
  - `lifecycle_scope: None` — main-module is Runtime-by-default; this milestone doesn't touch lifecycle
  - `sbom_tier: Some("source")` — per FR-006
  - `extra_annotations: vec![mikebom:component-role: "main-module"]` — supplementary C40 signal per FR-004
  - `parent_purl: None` — top-level (so SPDX root-selection picks it via case 1 / case 3 of `build_document::root_id`)
  - `depends: Vec<String>` — direct-require module names, post-`apply_replace_and_exclude` per FR-002
  - `licenses: vec![]` — empty per FR-005 (LICENSE detection is #103)
- **Direct-require edge**: a `Relationship { from: <main-module-purl>, to: <require-target-purl>, relationship_type: DependsOn, provenance: { source: <go.mod-path>, data_type: "go-mod-direct-require" } }` — emitted via the existing edge-emission loop at `scan_fs/mod.rs:526-547`.
- **`primaryPackagePurpose` enum (SPDX 2.3)**: new `pub enum SpdxPrimaryPackagePurpose { Application, Library, Framework, ... }` matching SPDX 2.3 spec §7.24's enum. For milestone 053, only `Application` is emitted (heuristic: every Go workspace root is an `APPLICATION` per SPDX semantics — even libraries are "applications" in the sense of "the thing this BOM is about").
- **VCS-resolved version**: an internal helper struct or just a `Result<String, anyhow::Error>` — no new public type. The 3-step ladder is encapsulated in a private `resolve_workspace_version(project_root: &Path) -> String` function.

### 2. Contracts

`contracts/main-module-component.md` (new, this run) — captures the per-format placement contract:

- **CycloneDX 1.6**: main-module appears in `metadata.component` with `type: "application"`. NOT in top-level `components[]`. Edges from main-module to direct requires appear in `dependencies[]` keyed by `metadata.component.bom-ref`. Property `mikebom:component-role: main-module` attached to the metadata.component (via its `properties[]`).
- **SPDX 2.3**: main-module appears in `packages[]` (SPDX has no separate metadata-component slot). Sets `primaryPackagePurpose: "APPLICATION"`. `documentDescribes[]` targets the main-module's SPDXID. Document-level relationship `SPDXRef-DOCUMENT DESCRIBES <main-module-spdxid>` emitted by the existing relationships builder. Annotation `mikebom:component-role` attached on the package via the existing C40 wiring.
- **SPDX 3.0.1**: main-module appears as a regular Element/Package. Document-level `DESCRIBES` (or v3 equivalent) targets it. Native role/purpose field set if v3 exposes one comparable to 2.3's `primaryPackagePurpose` — research.md flagged this as needing one cross-check during implementation.

### 3. Quickstart

`quickstart.md` (new, this run) — gives implementers + reviewers a 5-step verification recipe:

1. `git clone --depth 1 --branch v3.3.9 https://github.com/argoproj/argo-workflows.git /tmp/argo-053`
2. `HOME=$(mktemp -d) GOMODCACHE=$(mktemp -d)/empty target/debug/mikebom --offline sbom scan --path /tmp/argo-053 --format spdx-2.3-json --output /tmp/argo-053.spdx.json --no-deep-hash`
3. `jq '{rel: (.relationships | length), pkg: (.packages | length), main: [.packages[] | select(.primaryPackagePurpose == "APPLICATION") | .name][0], described: .documentDescribes}' /tmp/argo-053.spdx.json`
4. **Expect**: `rel ≥ 14`, `pkg ≥ 280`, `main = "github.com/argoproj/argo-workflows"`, `described = ["SPDXRef-Package-..."]` resolving to the same package.
5. Re-run with `--format cyclonedx-json` and `jq '.metadata.component | {name, type, purl, properties}'` — expect `{name: "github.com/argoproj/argo-workflows", type: "application", purl: "pkg:golang/github.com/argoproj/argo-workflows@v3.3.9", properties: [..., {name: "mikebom:component-role", value: "main-module"}]}`.

### 4. Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` after this plan is committed — adds milestone 053 entry to `CLAUDE.md`'s Active Technologies list. No new technologies to register (no new crates, no new languages); the script will record "053-go-main-module-edges: Existing only".

### 5. Re-evaluate Constitution Check

Re-checked above table after Phase 1 design — no new violations introduced. The native-field placement (CDX `metadata.component`, SPDX `primaryPackagePurpose`) actively *strengthens* Principle V compliance. The Go-vs-other-ecosystems asymmetry (Principle V perspective: every ecosystem ought to have a main-module) is documented in `docs/design-notes.md` and tracked in #104; no spec violation, just a deliberate scope boundary.

**Phase 1 outputs**: `data-model.md`, `contracts/main-module-component.md`, `quickstart.md`, agent-context update. All feature into `/speckit.tasks` next.

## Complexity Tracking

*No constitution violations to justify. Section intentionally empty.*
