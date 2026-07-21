---

description: "Task list for m214 — rename mikebom → waybill across all functional identifiers"
---

# Tasks: Rename mikebom → waybill across all functional identifiers

**Input**: Design documents from `/specs/214-rename-to-waybill/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: No new test tasks required — this is a mechanical rename per FR-018. Every existing test must PASS UNCHANGED after identifier substitution. Golden regeneration (T024) uses the existing env-var-driven test pattern (`WAYBILL_UPDATE_*_GOLDENS=1 cargo test`).

**Organization**: 4 user stories from spec.md, one CI grep gate as the merge-blocker (SC-001). Six commit boundaries per plan.md's execution strategy (research R1) — noted in task descriptions where they group.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story (US1, US2, US3, US4)
- File paths absolute-from-repo-root

---

## Phase 1: Setup

**Purpose**: Prerequisites verification + rename harness script bootstrap.

- [ ] T001 Sync `main` locally and create the feature branch: `git checkout main && git pull origin main && git checkout -b 214-rename-to-waybill`. Verify HEAD is at c4c9b25 (post-alpha.65 release) or later via `git log -1 --oneline`.
- [ ] T002 Write `specs/214-rename-to-waybill/scripts/rename_pass.py` per research R2. Substitution catalog covers 6 passes: (1) Cargo package + dirs, (2) Rust modules, (3) strings + env vars, (4) filesystem artifacts + workflows, (5) docs + prose, (6) CI-gate wiring. Allowlist path exclusions match contracts/grep-gate.md. Include `--dry-run` mode, per-file substitution counts, and operator-confirmation prompt before each pass.
- [ ] T003 Verify script's substitution catalog agreement: `python3 specs/214-rename-to-waybill/scripts/rename_pass.py --pass 3 --dry-run` on unchanged tree must report exactly 73 env-var patterns matched + exactly 192 annotation-key patterns matched. Cross-check against contracts/env-var-migration.md (73 entries) + contracts/annotation-migration.md (192 entries surveyed at spec time). Delta between script output and contract enumeration flags an out-of-sync catalog.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Rename the 3 crate directories + Cargo package names + intra-workspace deps. This blocks EVERY user story because all subsequent Rust identifier and string rename passes reference the new package names.

**⚠️ CRITICAL**: `cargo check --workspace` MUST pass at Phase 2 completion. If it doesn't, revert and diagnose before proceeding.

**Commit boundary**: after T009, single commit `chore(214): rename crate directories + Cargo package names`.

- [ ] T004 [P] `git mv mikebom-cli waybill-cli` — preserves blame history via `>50%` content-similarity rename detection (research R3).
- [ ] T005 [P] `git mv mikebom-common waybill-common` — same rationale.
- [ ] T006 [P] `git mv mikebom-ebpf waybill-ebpf` — same rationale.
- [ ] T007 Update root `Cargo.toml` workspace `members = ["waybill-cli", "waybill-common", "xtask"]` and `exclude = ["waybill-ebpf"]` (was `mikebom-{cli,common}` and `mikebom-ebpf`).
- [ ] T008 Update per-crate `[package].name`: `waybill-cli/Cargo.toml` → `name = "waybill"` (was `"mikebom"`), `waybill-common/Cargo.toml` → `name = "waybill-common"`, `waybill-ebpf/Cargo.toml` → `name = "waybill-ebpf"`. (Note: the primary binary crate's package name is `"waybill"` singular; its BINARY name is also `"waybill"` via `[[bin]]` — verify both.)
- [ ] T009 Update intra-workspace path deps: `waybill-cli/Cargo.toml` + `waybill-ebpf/Cargo.toml` each have `mikebom-common = { path = "../mikebom-common" }` → `waybill-common = { path = "../waybill-common" }`.
- [ ] T010 `cargo update -w -p waybill -p waybill-common` — regenerates `Cargo.lock` with the new package names. Then `cargo check --workspace` must succeed cleanly. Commit boundary: single commit `chore(214): rename crate directories + Cargo package names`.

**Checkpoint**: `cargo check --workspace` clean. Any user story work can now begin.

---

## Phase 3: User Story 1 - Downstream SBOM consumer parses annotations under the new prefix (Priority: P1) 🎯 MVP

**Goal**: 192 `mikebom:*` annotation keys renamed to `waybill:*` across all Rust source. 34 golden test files regenerated. Wire-shape prefix swap complete.

**Independent Test**: Grep `"mikebom:"` in any emitted CDX/SPDX-2.3/SPDX-3 SBOM → 0 hits. Grep `"waybill:"` → count matches the pre-rename annotation count. Downstream consumer can do a single `sed 's/mikebom:/waybill:/g'` at their input layer and continue working.

### Implementation for User Story 1

- [ ] T011 [US1] Substitute the 192 `"mikebom:*"` annotation string literals → `"waybill:*"` across `waybill-cli/src/**/*.rs` + `waybill-common/src/**/*.rs`. Use `python3 specs/214-rename-to-waybill/scripts/rename_pass.py --pass 3-annotations` for the substitution. Verify with `grep -rho '"mikebom:[a-z-]*"' waybill-cli/src/ waybill-common/src/ 2>&1 | wc -l` → returns 0.
- [ ] T012 [US1] Substitute the tool-metadata identifier in the SBOM builders (CDX `metadata.tools[].name`, SPDX 2.3 `creationInfo.creators[]` "Tool:" prefix, SPDX 3 `Element.creationInfo.createdBy[]`). File paths: `waybill-cli/src/generate/cyclonedx/`, `waybill-cli/src/generate/spdx/`, `waybill-cli/src/attestation/`. All literal `"mikebom"` (tool name) → `"waybill"`.
- [ ] T013 [US1] Verify `cargo check --workspace` still passes after T011+T012.
- [ ] T014 [US1] Regenerate all 34 golden files:

    ```bash
    WAYBILL_UPDATE_CDX_GOLDENS=1 \
    WAYBILL_UPDATE_SPDX_GOLDENS=1 \
    WAYBILL_UPDATE_SPDX3_GOLDENS=1 \
      cargo test -p waybill \
        --test cdx_regression --test spdx_regression --test spdx3_regression \
        --test pkg_alias_binding_us1 --test oci_pull_backward_compat \
        --test optional_dep_classification
    ```

    Note: at this task's execution point the env-var names must ALREADY be renamed (T016 in US2 runs before T014 in execution order because Phase 4 runs before Phase 3 completion — see execution order in "Dependencies" below).
- [ ] T015 [US1] Verify golden diffs are **pure prefix-swaps** per research R6:

    ```bash
    git diff waybill-cli/tests/fixtures/golden/ | \
      grep -vE 'mikebom|waybill|MIKEBOM|WAYBILL|@@|^diff|^index|^---|^\+\+\+' | \
      head -10
    ```

    Expected output: empty. Any semantic diff (new field, missing field, reordered field, changed non-rename value) blocks the merge as a rename bug — file follow-up issue + revert.
- [ ] T016 [US1] Verify wire-shape acceptance criteria on a live SBOM emission: build the binary (`cargo build --release --bin waybill`), run on any test fixture, grep the emitted JSON for `"mikebom:` → 0 hits AND `"waybill:` → count > 0.

**Checkpoint**: Wire-shape contract flipped. Downstream consumers of the annotation prefix are now dependent on migrating.

---

## Phase 4: User Story 2 - Operator invokes the renamed CLI + env vars (Priority: P1)

**Goal**: `waybill` binary name (was `mikebom`). 73 `MIKEBOM_*` env vars renamed to `WAYBILL_*` across code, scripts, workflows. eBPF binary path + Dockerfile paths + release.yml artifact naming updated.

**Independent Test**: `waybill --version` works. `mikebom --version` returns command-not-found. `WAYBILL_LOG=debug waybill sbom scan --path .` on a fixture behaves identically to pre-rename `MIKEBOM_LOG=debug mikebom sbom scan --path .`.

### Implementation for User Story 2

- [ ] T017 [P] [US2] Substitute 73 `MIKEBOM_*` env-var references → `WAYBILL_*` across all `.rs` files. Use `python3 specs/214-rename-to-waybill/scripts/rename_pass.py --pass 3-envvars` for the substitution. Verify with `grep -rho 'MIKEBOM_[A-Z_]\{3,\}' waybill-cli/ waybill-common/ 2>&1 | wc -l` → returns 0.
- [ ] T018 [P] [US2] Substitute `MIKEBOM_*` env-var references → `WAYBILL_*` across `.github/workflows/*.yml` (release.yml, ci.yml, auto-tag-release.yml, dependabot config).
- [ ] T019 [P] [US2] Substitute `MIKEBOM_*` env-var references → `WAYBILL_*` across `scripts/*.sh` (pre-pr.sh, ebpf-integration-test.sh, verify-recipes.sh if present).
- [ ] T020 [US2] Update eBPF loader's `default_ebpf_path` at `waybill-cli/src/trace/loader.rs`: literal path `"mikebom-ebpf/target/bpfel-unknown-none/release/mikebom-ebpf"` → `"waybill-ebpf/target/bpfel-unknown-none/release/waybill-ebpf"`.
- [ ] T021 [US2] Update `Dockerfile.ebpf-test`: `WORKDIR /mikebom` → `WORKDIR /waybill`, `COPY . .` remains (relative), `ENTRYPOINT` binary path if hard-coded → `waybill`. Update the `RUN cargo run --package xtask -- ebpf` step's output-path expectation (matches T020).
- [ ] T022 [US2] Update `.github/workflows/release.yml` artifact-naming templates: `mikebom-v${version}-${target}.{tar.gz,zip}` → `waybill-v${version}-${target}.{tar.gz,zip}`. Docker metadata: image name `mikebom` → `waybill`. Docker image tag: `ghcr.io/kusari-oss/mikebom:${version}` → `ghcr.io/kusari-oss/waybill:${version}`.
- [ ] T023 [US2] Update `scripts/ebpf-integration-test.sh` internal paths: `MIKEBOM=/mikebom/target/release/mikebom` → `WAYBILL=/waybill/target/release/waybill`, `FIXTURE=/mikebom/...` → `/waybill/...`, all `"$MIKEBOM"` invocations → `"$WAYBILL"`.
- [ ] T024 [US2] `xtask/src/*.rs` — if xtask references any of these paths for the eBPF build step, update accordingly. Verify with `grep -rn 'mikebom' xtask/src/` → returns 0 hits.
- [ ] T025 [US2] Verify `cargo check --workspace` still passes after T017-T024. Commit boundary: single commit `chore(214): rename filesystem artifacts + workflow patterns + MIKEBOM_* env vars`.

**Checkpoint**: CLI + env vars + filesystem artifacts all renamed. Post-Phase-4 the tree can `cargo build --bin waybill` and produce a `target/release/waybill` binary.

---

## Phase 5: User Story 3 - New contributor onboards to the renamed codebase (Priority: P2)

**Goal**: Rust module paths (`mikebom_common::*` → `waybill_common::*`) rewritten across all `.rs` files. README + CLAUDE.md + docs prose rewrites to reference the project as "Waybill". Historical spec docs at `specs/001-*` through `specs/213-*` preserved unchanged per FR-012.

**Independent Test**: Fresh contributor `grep -rE '\bmikebom\b' Cargo.toml **/Cargo.toml src/ waybill-cli/src waybill-common/src waybill-ebpf/src` → 0 hits. `cargo build --workspace` succeeds and produces `target/release/waybill`. README.md opens with "Waybill" (heritage sentence "formerly Mikebom" optionally present, one occurrence max).

### Implementation for User Story 3

- [ ] T026 [US3] Substitute Rust module paths across every `.rs` file: `mikebom_common` → `waybill_common`, `mikebom_cli` → `waybill_cli` (if used), `mikebom_ebpf` → `waybill_ebpf` (if used). Use `python3 specs/214-rename-to-waybill/scripts/rename_pass.py --pass 2` for the substitution. Every `use mikebom_common::foo` → `use waybill_common::foo`; every fully-qualified path likewise. Verify with `grep -rho '\bmikebom_\(common\|cli\|ebpf\)\b' waybill-cli/src/ waybill-common/src/ waybill-ebpf/src/ xtask/src/ 2>&1 | wc -l` → returns 0.
- [ ] T027 [US3] Verify `cargo check --workspace` still passes after T026. This is the second-largest substitution class (after annotations); getting it right is critical.
- [ ] T028 [P] [US3] Rewrite `README.md` prose: `Mikebom` → `Waybill`, `mikebom` → `waybill` (case-preserving). ADD one heritage sentence in the intro: "Waybill (formerly known as Mikebom) is …" — must be exactly ONE occurrence of the phrase; multiple heritage sentences discouraged per FR-009.
- [ ] T029 [P] [US3] Rewrite `CLAUDE.md` prose: `Mikebom` → `Waybill`, `mikebom` → `waybill` throughout. All command examples (`mikebom sbom scan`, `mikebom trace`) update to `waybill sbom scan`, `waybill trace`.
- [ ] T030 [P] [US3] Rewrite `docs/architecture/**/*.md`, `docs/user-guide/**/*.md`, `docs/ecosystems.md`, `docs/design-notes.md`, `docs/index.md`, `docs/DEPENDENCIES.md`, `docs/releases.md`, `docs/reference/**/*.md`, `docs/research/**/*.md`, `docs/examples/**/*.md` — case-preserving `Mikebom` → `Waybill` + `mikebom` → `waybill`. NEW file `docs/migration/mikebom-to-waybill.md` is created in US4 (T033), not here.
- [ ] T031 [US3] EXPLICITLY PRESERVE `docs/audits/*.md` — these are historical audit reports with dates like `2026-07-06-tauri-airflow.md`; the versions + tool names they cite are historical facts preserved as-authored. NO mikebom → waybill substitution here. Verify: `grep -rn '\bmikebom\b' docs/audits/` still returns pre-rename hits.
- [ ] T032 [US3] EXPLICITLY PRESERVE `specs/001-*/` through `specs/213-*/` per FR-012 — these are historical spec directories authored under the pre-rename name. NO substitution. Verify: `grep -rE '\bmikebom\b' specs/213-kernel-noise-filter/spec.md | wc -l` still returns >0 hits (positive check confirming preservation). Commit boundary: after T032, commit `chore(214): rename Rust module paths + rewrite docs/README/CLAUDE.md prose`.

**Checkpoint**: Contributor-facing prose + module paths renamed. Historical artifacts preserved. Grep-based verification passes for both directions (functional identifiers = 0 hits; historical = non-zero hits).

---

## Phase 6: User Story 4 - Existing users find migration guidance (Priority: P3)

**Goal**: Migration guide document created at `docs/migration/mikebom-to-waybill.md` (FR-015). Constitution rename with MAJOR bump + SYNC IMPACT REPORT.

**Independent Test**: A pre-rename user reading the migration guide can complete their migration via pure mechanical text substitution — no code review or judgment required. Guide lists all 73 env-var renames + annotation prefix mapping + binary + Docker rename.

### Implementation for User Story 4

- [ ] T033 [US4] Create `docs/migration/mikebom-to-waybill.md` per FR-015. Contents: (a) BREAKING banner + "who this affects" audience callout, (b) binary rename (`mikebom` → `waybill` + `--version`, `--help` unchanged), (c) env-var prefix rename with LINK to `specs/214-rename-to-waybill/contracts/env-var-migration.md` (or copy the 73-entry table), (d) annotation prefix rename with drop-in sed/jq recipes and link to contracts/annotation-migration.md, (e) Docker image rename (`ghcr.io/kusari-oss/mikebom:v0.1.0-alpha.65` last pre-rename tag → `ghcr.io/kusari-oss/waybill:v0.1.0-alpha.66+`), (f) release-artifact filename rename, (g) CI-script migration example (before/after diff), (h) FAQ / troubleshooting.
- [ ] T034 [US4] Update `.specify/memory/constitution.md` per research R5: prepend SYNC IMPACT REPORT block (v1.5.0 → v2.0.0 MAJOR), update `**Version**: 1.5.0` → `**Version**: 2.0.0`, update `**Last Amended**: 2026-06-20` → `**Last Amended**: 2026-07-21`.
- [ ] T035 [US4] Update constitution TITLE `# mikebom Constitution` → `# Waybill Constitution`. Add a one-line heritage preamble under the title: "> Waybill was previously known as Mikebom. Historical spec docs at `specs/001-*/`..`specs/213-*/` retain the original terminology as authorship artifacts."
- [ ] T036 [US4] Update constitution prose: every Principle body paragraph's `mikebom` → `Waybill` where the project is referred to by name. Every code-fence example command `mikebom trace` → `waybill trace`. The normative content of Principles I-XII is UNCHANGED — only identity references update.
- [ ] T037 [US4] Update the Build & Test Commands table in the constitution: `mikebom` command reference → `waybill`. Commit boundary: after T037, commit `chore(214): create migration guide + constitution MAJOR bump v1.5.0 → v2.0.0 with SYNC IMPACT REPORT`.

**Checkpoint**: Migration guide exists + constitution renamed + all doc-side heritage preservation intact.

---

## Phase 7: Polish & CI grep gate (SC-001 merge blocker)

**Purpose**: The CI grep gate is the merge-blocking enforcement of SC-001. Once added, any future PR that reintroduces `mikebom` in a functional-identifier position fails CI. Also polish tasks + final PR-open.

- [ ] T038 Add CI grep-gate step per contracts/grep-gate.md to `.github/workflows/ci.yml`. Placement: after the existing `Walker-audit allow-list check` step; before `Clippy`. Uses POSIX bash + grep + diff — zero new tool installations. Emits GitHub Actions error annotation on any hit outside allowlist.
- [ ] T039 Locally verify the CI gate does NOT false-positive on the current tree:

    ```bash
    BADHITS=$(grep -rE '\bmikebom\b' \
      waybill-cli/src waybill-common/src waybill-ebpf/src xtask/src \
      Cargo.toml waybill-cli/Cargo.toml waybill-common/Cargo.toml waybill-ebpf/Cargo.toml xtask/Cargo.toml \
      .github/workflows/*.yml Dockerfile.ebpf-test scripts 2>/dev/null || true)
    [[ -z "$BADHITS" ]] && echo "OK" || { echo "FAIL"; echo "$BADHITS"; exit 1; }
    ```

    Must print "OK". Any hit is a rename bug that must be fixed before proceeding to T040.
- [ ] T040 Locally verify historical preservation intact:

    ```bash
    [[ -n "$(grep -rE '\bmikebom\b' specs/213-kernel-noise-filter/ 2>/dev/null)" ]] && echo "OK" || { echo "FAIL: historical spec preservation broken"; exit 1; }
    ```

    Positive check that spec/001-213 still contain mikebom references.
- [ ] T041 Final golden regeneration verification: re-run T014's regeneration commands + T015's diff-purity check. Any residual mikebom in golden files at this point = rename bug.
- [ ] T042 Commit boundary: after T038-T041, commit `chore(214): regenerate goldens + add CI grep gate (SC-001)`.
- [ ] T043 Push branch: `git push origin 214-rename-to-waybill`.
- [ ] T044 Open PR against `main` with title `chore(214): rename mikebom → waybill across all functional identifiers`. PR body MUST include: (a) prominent BREAKING banner, (b) link to migration guide, (c) 6-commit summary, (d) Test Plan enumerating (i) CI matrix + grep gate, (ii) local Colima verification (if applicable), (iii) SC-001 through SC-008 coverage. Do NOT run local pre-PR — per feedback_release_bump_prepr_slow memory + FR-018 the workspace-dir rename invalidates the whole compile cache and local pre-PR takes 30+ min; CI verifies.

### Final gates

- [ ] T045 CI-side verification: all 4 Lint+test lanes (linux-x86_64 default + ebpf-tracing, macOS, Windows) + Kusari Inspector + 15 rootfs/language scanners MUST pass. Any failure → diagnose per feedback_ebpf_container_test_gap memory (macOS default-features can miss Linux+ebpf-tracing bugs). Merge blocked until all 20 checks green.

---

## Post-merge (not part of the rename PR; separate follow-up PRs)

- [ ] T046 **Post-merge cleanup PR** per research R9. Branch `214-cleanup-rename-scripts` — one commit: `git rm -r specs/214-rename-to-waybill/scripts/`. Removes the feature-local Python harness; leaves spec-kit artifacts (spec/plan/research/data-model/contracts/quickstart/tasks) as historical reference. Small PR, single reviewer.
- [ ] T047 **Release PR for v0.1.0-alpha.66** per quickstart.md's "After PR merges" section. Follows the m212/m213 alpha.65 release pattern: version bump + golden regen + PR title matching `release: bump workspace to v` prefix (so auto-tag-release.yml fires, subject to #623 fix). First waybill-named release: binary `waybill`, artifacts `waybill-v0.1.0-alpha.66-*`, Docker image `ghcr.io/kusari-oss/waybill:v0.1.0-alpha.66`.

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (Phase 1)**: no dependencies. T001-T003 sequential (branch → script → catalog verification).
- **Foundational (Phase 2)**: depends on Setup. T004-T006 parallelizable (three independent `git mv` commands). T007-T010 sequential (Cargo.toml edits + verification).
- **US1 (Phase 3)**: depends on Foundational. T011-T013 sequential (rename → verify compile). T014 golden regeneration DEPENDS ON Phase 4's T017 completing first (env-var rename must precede golden regen because the env-var name itself changed from `MIKEBOM_UPDATE_*` to `WAYBILL_UPDATE_*`).
- **US2 (Phase 4)**: depends on Foundational only. T017-T024 mostly parallelizable (different files). T025 verification sequential.
- **US3 (Phase 5)**: depends on Foundational. T026-T027 sequential (Rust modules + verify). T028-T030 parallelizable (different doc files). T031-T032 preservation checks are read-only.
- **US4 (Phase 6)**: depends on nothing beyond Phase 1 (writes new file + edits constitution). Can run in parallel with US1/US2/US3.
- **Polish (Phase 7)**: depends on all preceding phases. T038-T042 sequential (CI gate → verify → commit). T043-T045 sequential (push → PR → CI).

**Cross-story execution order** (recommended solo-dev sequence):

1. T001 → T002 → T003 (Setup)
2. T004 || T005 || T006 → T007 → T008 → T009 → T010 (Foundational, one commit)
3. T017 → T018 || T019 → T020 → T021 → T022 → T023 → T024 → T025 (US2, one commit) — do US2 BEFORE US1's T014 because env-var rename must precede golden regen
4. T026 → T027 → T028 || T029 || T030 → T031 (verify) → T032 (verify) (US3 Rust + docs, one commit)
5. T011 → T012 → T013 → T014 → T015 → T016 (US1 annotations + goldens, one commit; T014 works now because env vars renamed in step 3)
6. T033 → T034 → T035 → T036 → T037 (US4 migration guide + constitution, one commit)
7. T038 → T039 → T040 → T041 → T042 (Polish CI gate + verify, one commit)
8. T043 → T044 → T045 (Push + PR + CI)
9. T046 → T047 (post-merge follow-ups)

### Parallel opportunities

- T004 || T005 || T006 — three independent `git mv`s
- T017 || T018 || T019 — env-var rename in three different file categories (.rs / .yml / .sh)
- T028 || T029 || T030 — three doc-prose rewrites in different files

---

## Implementation Strategy

### Single-PR delivery

This is a mechanical rename — the ENTIRE m214 work is one PR. There's no MVP-vs-full split. The 6 commits within the PR give reviewers commit-scoped diffs to audit each substitution class independently.

- **Merge floor**: US1 + US2 + US3 + US4 + CI gate all complete. Cannot merge with any user story deferred.
- **Rollback**: `git revert -m 1 <merge-commit-sha>` reverts all 6 commits atomically. Consumers who migrated to `waybill:*` would need to migrate back — one-way door; the PR body must make this explicit.

### Solo-dev sequencing (recommended)

Given single-file rename passes coordinate across all crates + tightly-coupled commit boundaries, solo sequential execution beats parallel-team overhead. Estimated time: 4-6 hours end-to-end (including CI wait time).

### Verification-first

Every substitution pass MUST end with a verification step (T009, T013, T025, T027, T032, T039, T040). If any verification fails, revert the pass + diagnose. Do NOT proceed to the next substitution class on a broken tree.

---

## Notes

- [P] tasks = different files, no dependencies.
- [Story] label maps task to user story for traceability.
- **No new test files** — rename is verified by (a) existing tests passing unchanged, (b) golden regeneration diff-purity check, (c) CI grep gate. Adding new tests for the rename itself would be scope creep per FR-018.
- Commit after each logical group per the 6-commit schedule in quickstart.md. Reviewers can navigate the PR by commit; blame history preserved via `git mv`.
- **Skip local `./scripts/pre-pr.sh`** per feedback_release_bump_prepr_slow memory. The crate-directory + package-name change invalidates the whole compile cache; local pre-PR takes 30+ min. CI verifies via all 4 Lint+test lanes.
- **Verifier / macOS-blindspot concern (from m213 experience)**: if any m213-era `#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]`-gated code has string identifiers that need rename, macOS-local `cargo check` won't catch them. The CI grep gate (T038) is the safety net — it runs against ALL source files regardless of cfg-gating. Trust the CI grep gate, not local cargo check, for full coverage.
- **Post-rename memory update** (out of scope but worth noting): `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/` — this Claude Code project-memory directory path itself has "mikebom" in it. The memory index (`MEMORY.md`) and per-memory files reference the project by both names during the transition. Update at operator discretion post-merge; not part of the rename PR scope.
