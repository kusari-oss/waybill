# Implementation Plan: Rename mikebom → waybill across all functional identifiers

**Branch**: `214-rename-to-waybill` | **Date**: 2026-07-21 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/214-rename-to-waybill/spec.md`

## Summary

Mechanical repo-wide rename. Not a code-change milestone — a **substitution PR**. Executed via a script-driven pipeline (grep + sed with allowlist filtering) to guarantee no functional-identifier `mikebom` occurrences remain, verified by a CI-side grep gate (SC-001). Six substitution passes:

1. **Cargo package + directory renames** — `git mv mikebom-cli waybill-cli` (× 3 crates); rewrite `[package].name` in each; update workspace `members` + `exclude`; rewrite intra-workspace path deps.
2. **Rust identifier rename** — `mikebom_common::*` → `waybill_common::*` module imports across every `.rs` file; `mikebom_cli` / `mikebom_ebpf` module-path references.
3. **String-literal renames** — 192 distinct `"mikebom:*"` annotation keys → `"waybill:*"`; 73 `MIKEBOM_*` env vars → `WAYBILL_*`; other user-visible strings (log-line prefixes, tool-metadata identifiers).
4. **Filesystem-artifact rename** — eBPF binary path (`mikebom-ebpf` → `waybill-ebpf` in the loader's `default_ebpf_path`); Dockerfile targets; release-workflow artifact naming pattern.
5. **Docs + prose rewrite** — README.md, constitution.md (title + prose + SYNC IMPACT REPORT), CLAUDE.md, docs/**/*.md, migration guide creation (new file at `docs/migration/mikebom-to-waybill.md`).
6. **Golden regeneration** — 34 golden test files via `WAYBILL_UPDATE_*_GOLDENS=1` (renamed from `MIKEBOM_UPDATE_*_GOLDENS`). Same regeneration pattern as release-version bumps.

Each substitution pass is one commit. Final commit adds the CI grep gate that pins SC-001 as a merge-blocker. **Preserves git-blame history** by using `git mv` for file renames (>50% content threshold triggers rename detection) rather than `cp + rm`.

**Not bundling any functional change** per FR-018. If a rename pass discovers a bug (e.g., a `mikebom:foo` annotation whose key was hand-typed inconsistently as `mikebom-foo` somewhere), it's filed as a follow-up issue — not fixed inline.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–213; no version change). No nightly required beyond the existing `ebpf-tracing` feature's kernel-side compilation.

**Primary Dependencies**: Existing only. **Zero new Cargo dependencies.** This milestone is a rename; it introduces no new libraries, deletes none, bumps none. Every `Cargo.toml`'s `[dependencies]` block stays byte-identical *except* for the intra-workspace `mikebom-common = { path = "..." }` line which becomes `waybill-common = { path = "..." }` (2 sites: `mikebom-cli/Cargo.toml` + `mikebom-ebpf/Cargo.toml`).

**Storage**: N/A — pure identifier rename; no runtime state changes.

**Testing**:
- `cargo test --workspace` — every existing test must pass unchanged. Test bodies get renamed identifiers (e.g., `use waybill_common::events::FileEvent`) but assertions and semantics stay identical.
- Golden regeneration via `WAYBILL_UPDATE_CDX_GOLDENS=1 WAYBILL_UPDATE_SPDX_GOLDENS=1 WAYBILL_UPDATE_SPDX3_GOLDENS=1 cargo test -p waybill --test cdx_regression --test spdx_regression --test spdx3_regression --test pkg_alias_binding_us1 --test oci_pull_backward_compat --test optional_dep_classification`.
- CI-side grep gate: new step in `.github/workflows/ci.yml` that runs `grep -rE '\bmikebom\b' <in-scope paths>` and fails the build if any hits are outside the allowlist (spec docs, README heritage sentence, `docs/migration/mikebom-to-waybill.md`).

**Target Platform**: All existing platforms (linux-x86_64 default + ebpf-tracing, macOS, Windows). No platform coverage changes.

**Project Type**: Same three-crate CLI + eBPF workspace; just renamed. Post-rename layout:

```
waybill-cli/    (was mikebom-cli)
waybill-common/ (was mikebom-common)
waybill-ebpf/   (was mikebom-ebpf)
xtask/          (unchanged)
```

**Performance Goals**: N/A (no runtime code change).

**Constraints**:
- **FR-018 lock**: no functional change permitted. Any bug discovered during rename → separate follow-up issue, not fixed inline. Any refactor → same.
- **git-blame preservation**: use `git mv` (not `cp` + `rm`) for every file rename so `git log --follow` continues to work.
- **Atomic PR**: single PR touches thousands of files; must not land in a half-renamed state on `main`. CI grep gate enforces this.
- **Local pre-PR skip**: per `feedback_release_bump_prepr_slow` memory, the workspace-directory + package-name change invalidates the whole compile cache. Local pre-PR takes 30+ min. CI verifies; skip local.
- **Constitution amendment**: `.specify/memory/constitution.md` gets a MAJOR bump per its own governance rule (project-name change is a redefinition). SYNC IMPACT REPORT block prepended to the constitution file matches the existing convention (see `constitution.md` lines 1-76).

**Scale/Scope**:
- 3 crate directories renamed.
- 3 `Cargo.toml` (`[package].name` + intra-workspace deps).
- 1 workspace `Cargo.toml` (`members`).
- 1 `Cargo.lock` (auto-updated by cargo).
- ~4000 `mikebom` string occurrences in `src/` (identifiers, imports, log messages, annotation keys, comments referring to the tool's own name).
- 192 distinct annotation keys (mechanical prefix swap).
- 73 environment variable names (`MIKEBOM_*` → `WAYBILL_*`).
- 34 golden test JSON files (regenerated via env-var-driven test).
- ~440 lines of `mikebom` references in README + constitution + CLAUDE.md + docs/ (prose rewrite + heritage-preservation).
- `.github/workflows/*.yml` — release.yml, ci.yml, auto-tag-release.yml, dependabot config, release-workflow patterns.
- `Dockerfile.ebpf-test`, `Dockerfile` (if present).
- **Total estimated diff**: ~5000 LOC changed, ~50 files renamed. Mostly mechanical.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C**: ✅ No change. Renaming touches no build-pipeline languages.
- **II. eBPF-Only Observation**: ✅ No change. The eBPF-only discovery contract is unaffected by the crate rename.
- **III. Fail Closed**: ✅ No change. Trace-failure semantics untouched.
- **IV. Type-Driven Correctness**: ✅ No change. All existing newtypes + no-unwrap discipline preserved. Renaming `mikebom_common::types::purl::Purl` to `waybill_common::types::purl::Purl` is a fully-qualified path substitution, not a type-system change.
- **V. Specification Compliance**: ⚠️ **Requires justification**. The `mikebom:*` → `waybill:*` annotation prefix rename IS a wire-shape change per FR-005 and per Clarification Q1 (hard break confirmed). Principle V explicitly says "Sub-element validity is as critical as envelope validity." Downstream consumers parsing the current `mikebom:*` prefix will find no matches post-rename. **Justification**: this is a mechanical prefix swap accompanied by a migration guide (FR-015); the JSON schema and every field value (except the prefix string) remain byte-identical. Every emitted SBOM post-rename remains fully CycloneDX 1.6 / SPDX 2.3 / SPDX 3.0.1 spec-compliant. The rename does not violate the standard; it renames a project-namespaced property key. Standards allow project-namespaced properties (CDX `metadata.properties`, SPDX 2.3 `Annotation`, SPDX 3 `Annotation`); which project owns the namespace is a project decision. ✅ Justification recorded.
- **VI. Three-Crate Architecture**: ✅ Preserved. Still exactly three crates (`waybill-cli`, `waybill-common`, `waybill-ebpf`). Only names change.
- **VII. Test Isolation**: ✅ No change. Unit / integration test isolation semantics unchanged.
- **VIII. Completeness**: ✅ No change. Trace completeness contract untouched.
- **IX. Accuracy**: ✅ No change.
- **X. Transparency**: ✅ No change. Transparency annotations use the new prefix but semantics preserved.
- **XI. Enrichment**: ✅ No change.
- **XII. External Data Source Enrichment**: ✅ No change.

**Strict Boundaries**:
- 1. No lockfile-based discovery — N/A.
- 2. No MITM proxy — N/A.
- 3. No C code — enforced (rename adds none).
- 4. No `.unwrap()` in production — enforced (rename touches identifiers, not error-handling code).
- 5. No file-tier duplicates — N/A.

**Governance / Amendment procedure**: The constitution's own governance rules (line 498-522) require semantic versioning of the constitution itself. This rename bumps the constitution's project name (`mikebom Constitution` → `Waybill Constitution`) and its Principle titles in prose. Per the constitution's own definition — "MAJOR: Principle removed, redefined, or made incompatible with prior interpretation" — a project-name change qualifies as a redefinition, so this is a **MAJOR bump: 1.5.0 → 2.0.0**. The bump PR (this milestone) MUST include a SYNC IMPACT REPORT block matching the existing convention (constitution.md lines 1-76).

**Verdict**: ✅ Constitution check passes. Principle V requires the wire-shape justification above (recorded); no unjustified violations. Proceed to Phase 0.

## Project Structure

### Documentation (this feature)

```text
specs/214-rename-to-waybill/
├── plan.md                          # This file (/speckit.plan command output)
├── research.md                      # Phase 0 — rename strategy, order of operations, git-blame preservation, script-driven rename harness
├── data-model.md                    # Phase 1 — rename-category taxonomy: functional identifier vs historical artifact, per data-model in spec
├── quickstart.md                    # Phase 1 — end-to-end rename recipe (script invocation + verification commands)
├── contracts/
│   ├── grep-gate.md                 # CI-side grep gate contract (allowlisted hits vs. rename bugs)
│   ├── env-var-migration.md         # 73-entry MIKEBOM_* → WAYBILL_* mapping table
│   └── annotation-migration.md      # 192-entry mikebom:* → waybill:* mapping table
├── checklists/
│   └── requirements.md              # (already exists from /speckit.specify)
└── tasks.md                         # Phase 2 output (/speckit.tasks — NOT created by /speckit.plan)
```

### Source Code (repository root) — POST-RENAME LAYOUT

```text
waybill-cli/                         # was mikebom-cli/
├── Cargo.toml                       # [package].name = "waybill", dep: waybill-common (path)
└── src/                             # all mikebom_common imports → waybill_common
    └── (unchanged structure)

waybill-common/                      # was mikebom-common/
├── Cargo.toml                       # [package].name = "waybill-common"
└── src/                             # all mikebom_common:: references → waybill_common
    └── (unchanged structure)

waybill-ebpf/                        # was mikebom-ebpf/
├── Cargo.toml                       # [package].name = "waybill-ebpf", dep: waybill-common (path)
└── src/                             # all mikebom_common imports → waybill_common
    └── (unchanged structure — no #[map] name changes needed; FILTER_CATEGORY_HITS etc. keep their kernel-side names)

xtask/                               # unchanged

Cargo.toml                           # [workspace].members = ["waybill-cli", "waybill-common", "xtask"]
                                     # [workspace].exclude = ["waybill-ebpf"]
Cargo.lock                           # auto-regenerated by `cargo update -w`

.github/workflows/
├── ci.yml                           # new "walker-audit-style" grep-gate step for m214 SC-001
├── release.yml                      # artifact naming: waybill-v0.1.0-alpha.N-<target>.tar.gz
├── auto-tag-release.yml             # unchanged (already matches v*-alpha.* pattern)
└── (rest unchanged)

Dockerfile.ebpf-test                 # ENTRYPOINT + WORKDIR paths use /waybill/ (was /mikebom/)
scripts/
├── pre-pr.sh                        # unchanged
└── ebpf-integration-test.sh         # /waybill/ paths, waybill trace capture, m213 signals renamed

docs/
├── architecture/attestations.md     # waybill:* annotation names
├── user-guide/cli-reference.md      # `waybill <noun> <verb>` invocations
├── migration/mikebom-to-waybill.md  # NEW — pre-rename user migration guide (FR-015)
├── ecosystems.md
├── audits/*.md                      # rewrite waybill in prose; keep historical audit names + dates
└── (rest — prose replacement)

.specify/memory/constitution.md      # # Waybill Constitution + v1.5.0 → v2.0.0 (MAJOR) + SYNC IMPACT REPORT

README.md                            # waybill throughout + one heritage sentence
CLAUDE.md                            # waybill throughout
MEMORY.md                            # untouched (user memory index; personal)

waybill-cli/tests/fixtures/golden/   # renamed with parent dir via git mv
├── cyclonedx/*.cdx.json             # regenerated via WAYBILL_UPDATE_CDX_GOLDENS=1
├── spdx-2.3/*.spdx.json             # regenerated
└── spdx-3/*.spdx3.json              # regenerated
```

**Structure Decision**: preserved three-crate architecture (Constitution VI). Every directory rename uses `git mv` for git-blame continuity. The workspace-level `Cargo.toml` `members`/`exclude` list gets renamed alongside the directories. No new modules, no new files, no restructuring — just renames.

### Historical artifacts (preserved unchanged per FR-011 + FR-012)

```text
specs/001-*/ … specs/213-*/          # 213 historical spec directories — untouched
                                       preserved as milestone artifacts of authorship
                                       under the mikebom name

.git/                                # git history preserves original commit messages
                                       + prior tag names (v0.1.0-alpha.7..65)

CHANGELOG-like docs                  # heritage-attribution allowed; multiple explicit
                                       "mikebom" mentions permitted only in change-log
                                       entries that themselves predate the rename
```

## Complexity Tracking

> Constitution Check violations: none unjustified. Principle V justification recorded above.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Wire-shape break (annotation prefix `mikebom:*` → `waybill:*`) | User-directed hard-break rename; annotation namespace ownership matches project identity | Dual-emit bridge release considered + rejected via Clarification Q1 — user chose hard break for alpha software |
