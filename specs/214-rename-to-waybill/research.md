# Phase 0 Research: mikebom → waybill rename

**Feature**: 214-rename-to-waybill
**Date**: 2026-07-21

## R1 — Rename execution strategy (single script vs. incremental commits)

**Decision**: Six sequential commits, one per substitution class. Each commit is verifiable independently, and the final commit adds the CI grep gate that pins SC-001. Reviewers can navigate the PR by commit rather than by 5000-line diff.

**Order of operations** (each is one commit):

1. `chore(214): rename crate directories + Cargo package names` — pure `git mv` for the 3 crate dirs + `[package].name` edits + workspace `members`/`exclude` update. Cargo builds pass after this commit.
2. `chore(214): rename Rust module paths` — `mikebom_common` → `waybill_common` in imports/`use` statements/fully-qualified paths across all `.rs` files. Cargo builds pass after this commit (identifiers all update together).
3. `chore(214): rename annotation prefixes and env-var strings` — 192 `"mikebom:..."` string literals + 73 `MIKEBOM_*` env-var references. Test suite passes but goldens will diff (deferred to commit 6).
4. `chore(214): rename filesystem artifacts + workflow patterns` — eBPF binary path in `loader.rs::default_ebpf_path`; Dockerfile paths; release.yml artifact naming; ci.yml env-var references. Test suite still passes.
5. `chore(214): rewrite docs + README + constitution + migration guide` — prose replacement + heritage sentences + `docs/migration/mikebom-to-waybill.md` creation + constitution MAJOR bump with SYNC IMPACT REPORT.
6. `chore(214): regenerate goldens + add CI grep gate` — `WAYBILL_UPDATE_*_GOLDENS=1 cargo test ...` regenerates all 34 golden files (diff = mechanical prefix swap in each). Add `.github/workflows/ci.yml` step that runs `grep -rE '\bmikebom\b' ...` against the allowlisted exclusions and fails on any hit outside allowlist. This is the SC-001 merge gate.

**Rationale**: incremental commits let reviewers verify each substitution class in isolation without wading through irrelevant golden regeneration diffs. Also lets `cargo build --workspace` pass at every intermediate commit, so `git bisect` remains usable.

**Alternatives considered**:
- (a) **Single big-bang commit** — REJECTED. 5000-line diff across all classes is unreviewable. Loses commit-scoped bisect.
- (b) **Two commits (Cargo + everything else)** — REJECTED. The "everything else" commit is still enormous.
- (c) **Automated script that runs all 6 classes in sequence + emits a single commit** — REJECTED for review reasons. Retain the script as the *tool* used inside each commit (see R2) but keep commits scoped.

## R2 — Script-driven rename harness (idempotent substitution helper)

**Decision**: A single Python script at `specs/214-rename-to-waybill/scripts/rename_pass.py` (feature-local; deleted at PR-merge time) that takes a substitution-class name and applies the matching find-and-replace with allowlist filtering. Invoked once per commit.

**Rationale**: manual `sed -i` invocations across thousands of files are error-prone. Python's os.walk + re.sub with per-file skip lists is easy to audit. Feature-local placement means the script doesn't ship in the release artifacts.

**Substitution passes**:

| Pass | Match pattern                                | Replace pattern                              | Files scoped                       |
|---|---|---|---|
| 1    | `git mv <dir>` (not regex)                   | `git mv` explicit                            | `mikebom-cli/`, `mikebom-common/`, `mikebom-ebpf/` |
| 1    | `name = "mikebom(-cli|-common|-ebpf)?"` | `name = "waybill\1"`                    | Cargo.toml (workspace + each crate)|
| 2    | `\bmikebom_(common|cli|ebpf)\b`              | `waybill_\1`                                 | `**/*.rs`                          |
| 3    | `"mikebom:`                                  | `"waybill:`                                  | `**/*.rs`, `**/*.md`, `**/*.sh`    |
| 3    | `\bMIKEBOM_([A-Z_]+)\b`                      | `WAYBILL_\1`                                 | `**/*.rs`, `**/*.sh`, `**/*.yml`, `**/*.md` |
| 4    | `mikebom-ebpf` (path context)                | `waybill-ebpf`                               | `mikebom-cli/src/trace/loader.rs`, `Dockerfile*`, `.github/**/*.yml`, `scripts/*.sh` |
| 4    | `mikebom-v\d`                                | `waybill-v\d`                                | release.yml (artifact-name templates) |
| 5    | (prose replacement — case-preserving)        | (`Mikebom`→`Waybill`, `mikebom`→`waybill`)   | `README.md`, `CLAUDE.md`, `.specify/memory/constitution.md`, `docs/**/*.md` |
| 6    | (env-var-driven cargo test)                  | (regenerates golden files)                   | `**/tests/fixtures/golden/*.json` |

**Allowlist** (paths where `mikebom` MUST be preserved):
- `specs/001-*` through `specs/213-*` — historical spec directories (millions of prose references to milestones authored under the mikebom name)
- `docs/migration/mikebom-to-waybill.md` — the migration guide file's title + prose
- `README.md` — one heritage sentence ("Waybill was previously known as mikebom")
- `docs/audits/*.md` — historical audit names + dates
- `.git/` — implicit; not touched
- `CHANGELOG.md` (if it exists) — heritage entries

The script emits a WARN + counts-per-file summary and requires operator confirmation before each pass. Verbose mode dumps every substitution for spot-check.

**Alternatives considered**:
- (a) Pure `find + sed -i` invocations — REJECTED. Cross-platform edge cases (BSD sed vs GNU sed); harder to audit; no dry-run mode.
- (b) `cargo run --package xtask -- rename` — REJECTED. xtask is Rust; rename is one-shot; a Python script is faster to write + delete.
- (c) IDE-driven refactor (e.g., rust-analyzer Rename) — REJECTED. Handles Rust identifiers but not string literals, env vars, docs, or Cargo.toml. Would still need a second pass.

## R3 — git-blame preservation strategy

**Decision**: Use `git mv <src> <dst>` for every file rename. Cargo package rename is a `git mv` of the directory + edits to `Cargo.toml` inside. Rust module renames are content edits (no file moves). Golden file locations move with their parent dir's `git mv`.

**Rationale**: `git log --follow <file>` traces file history across renames when git's rename detection triggers (default 50% content similarity). All of these renames preserve >99% of file content (the change is `mikebom → waybill` in identifiers only), well above the threshold. Reviewers using `git blame` see the pre-rename authorship attributed correctly.

**Verification**: after the rename PR merges, `git log --follow waybill-cli/src/main.rs` should show the entire commit history from the pre-rename `mikebom-cli/src/main.rs`.

**Alternatives considered**:
- (a) `cp + rm` (or filesystem rename outside git) — REJECTED. Git treats this as delete-and-add, losing blame history for every file.
- (b) `git filter-branch` to rewrite history retroactively — REJECTED. Rewrites SHAs, breaks references, huge blast radius. History would then require force-push to main. Not acceptable.

## R4 — CI grep gate for SC-001 (rename bug detection)

**Decision**: New job step in `.github/workflows/ci.yml` after the existing `Walker-audit allow-list check` step. Runs:

```bash
BADHITS=$(grep -rE '\bmikebom\b' \
  waybill-cli/src/ waybill-common/src/ waybill-ebpf/src/ xtask/src/ \
  Cargo.toml waybill-cli/Cargo.toml waybill-common/Cargo.toml waybill-ebpf/Cargo.toml xtask/Cargo.toml \
  .github/workflows/*.yml Dockerfile* scripts/*.sh 2>/dev/null || true)
if [[ -n "$BADHITS" ]]; then
  echo "::error::m214 rename bug — mikebom found in functional-identifier positions:"
  echo "$BADHITS"
  exit 1
fi
```

**Allowlist paths** (excluded from grep, preserved on purpose):
- `specs/**` — historical spec docs
- `docs/**` — narrative docs (one-line heritage sentence acceptable)
- `README.md` — heritage sentence
- `.git/**`
- `MEMORY.md` — user-personal memory index

**Rationale**: matches the m115/m117 walker-audit-gate pattern (source-tree-committed allowlist file, POSIX-shell grep + diff, zero new tool installations). This step becomes the runtime enforcement of SC-001.

**Alternatives considered**:
- (a) `just build --deny-warnings` style — REJECTED. Rust identifier-name policing is compile-time (already handled); string-literal policing is not (that's what this gate exists for).
- (b) Pre-commit hook — REJECTED. Local hooks aren't reliable; CI-side is authoritative.
- (c) Custom Rust `#[deny(clippy::disallowed_names)]` config with `mikebom` in disallowed-names list — REJECTED. Only catches identifiers, misses string literals + non-Rust files (yml, sh, md).

## R5 — Constitution amendment: SYNC IMPACT REPORT block

**Decision**: MAJOR bump v1.5.0 → v2.0.0. The rename PR MUST prepend a SYNC IMPACT REPORT block matching the existing convention at `.specify/memory/constitution.md` lines 1-76. Report contents:

```html
<!--
  ============================================================
  SYNC IMPACT REPORT
  ============================================================
  Version change: 1.5.0 → 2.0.0
  Bump rationale: MAJOR — project rename mikebom → Waybill.
  Constitution's project-name identity + every Principle heading
  refer to the project by name. Per the constitution's own
  Amendment procedure (line ~508 pre-rename): "MAJOR: Principle
  removed, redefined, or made incompatible with prior
  interpretation." Renaming the project the constitution governs
  qualifies as redefinition — every prior reference to the
  "mikebom Constitution" is now, retroactively, an artifact of
  the pre-rename name. All Principles' NORMATIVE CONTENT is
  unchanged — this is purely an identity update.

  Modified sections:
    - Constitution title: `# mikebom Constitution` → `# Waybill
      Constitution`
    - Preamble (added): one-line heritage note ("Waybill was
      previously known as mikebom; historical spec docs at
      specs/001-*/..specs/213-*/ retain the original terminology
      as authorship artifacts.")
    - Every Principle body paragraph: "mikebom" → "Waybill"
      substitution in prose where the project is referred to by
      name.
    - Build & Test Commands table: `mikebom` binary name →
      `waybill`.
    - Async Runtime section: unchanged (references only tokio).
    - Governance section: unchanged in intent; version numbers
      updated per the bump.

  Added sections: heritage preamble sentence.
  Removed sections: none.

  Previous SYNC IMPACT history: 1.4.0 → 1.5.0 (per constitution
  header comment). Full history preserved in prior blocks.

  Templates requiring updates:
    - .specify/templates/plan-template.md    ✅ no update needed
                                              (references project
                                              only in generic terms)
    - .specify/templates/spec-template.md    ✅ no update needed
    - .specify/templates/tasks-template.md   ✅ no update needed
    - .specify/templates/checklist-template.md ✅ no update needed
    - .specify/templates/agent-file-template.md ✅ verify: agent
                                              context filename may
                                              embed project name

  Follow-up TODOs: none
  ============================================================
-->
```

**Rationale**: matches the existing SYNC IMPACT REPORT format from prior bumps (1.3.0 → 1.4.0 → 1.5.0). Preserves institutional memory of the change while making the rename fully auditable.

**Alternatives considered**:
- (a) MINOR bump — REJECTED. The constitution's own governance rule (line ~508 pre-rename) defines MAJOR as "Principle removed, redefined, or made incompatible with prior interpretation" — a project-name change redefines the identity the entire constitution governs. This meets the MAJOR bar.
- (b) PATCH bump — REJECTED (would violate the constitution's own governance rules).
- (c) Skip the constitution entirely, treat it as a doc — REJECTED. The constitution is a first-class governance artifact; it renames alongside everything else per FR-010.

## R6 — Golden regeneration + verification

**Decision**: Follow the release-bump pattern from `feedback_release_bump_regen_all_golden_tests`. Six golden-owning test files:

```bash
WAYBILL_UPDATE_CDX_GOLDENS=1 \
WAYBILL_UPDATE_SPDX_GOLDENS=1 \
WAYBILL_UPDATE_SPDX3_GOLDENS=1 \
  cargo test -p waybill \
    --test cdx_regression \
    --test spdx_regression \
    --test spdx3_regression \
    --test pkg_alias_binding_us1 \
    --test oci_pull_backward_compat \
    --test optional_dep_classification
```

**Verification** (post-regeneration):
- Diff of each regenerated golden vs. pre-rename must be a **pure prefix swap** — every line contains either `mikebom → waybill` in exactly one position, or `mikebom-common → waybill-common`, or `MIKEBOM → WAYBILL`. Any semantic diff (new field, missing field, reordered field, changed value not related to the rename) is a rename bug and blocks merge.
- Verification recipe: `git diff --stat waybill-cli/tests/fixtures/golden/ | wc -l` should return exactly 34 (one line per file); `git diff waybill-cli/tests/fixtures/golden/ | grep -vE 'mikebom|waybill|MIKEBOM|WAYBILL|@@|^diff|^index|^---|^\+\+\+'` should return empty (any non-rename-related change surfaces).

**Rationale**: matches the m212 + m213 + release-bump pattern; no new infrastructure required.

**Alternatives considered**:
- (a) Manual golden editing — REJECTED. 2492 lines of `mikebom` in golden files across 34 files; error-prone.
- (b) Regenerate only some goldens — REJECTED. The 6 test suites all embed `mikebom` in their tool-metadata; partial regen produces inconsistent goldens.

## R7 — Docker image + release workflow rename

**Decision**: `.github/workflows/release.yml` renames go in commit 4 alongside eBPF binary path:

- Artifact filename template: `mikebom-v${version}-${target}.tar.gz` → `waybill-v${version}-${target}.tar.gz`
- Docker image tag: `ghcr.io/kusari-oss/mikebom:${version}` → `ghcr.io/kusari-oss/waybill:${version}`
- Docker image name in `docker/metadata-action`: `mikebom` → `waybill`

Pre-rename image tags at `ghcr.io/kusari-oss/mikebom:v0.1.0-alpha.7..65` remain on GHCR as historical artifacts (no cleanup). Post-rename releases publish under `ghcr.io/kusari-oss/waybill:*`.

**Rationale**: matches the repo rename that already happened on GitHub. Old image tags stay accessible for pre-rename installations; new tags land in the new namespace.

**Alternatives considered**:
- (a) Publish both `mikebom` AND `waybill` image tags for a bridge period — REJECTED per Clarification Q1 (hard break). Doubles GHCR storage + build time for no operator value.
- (b) Delete pre-rename image tags — REJECTED. Breaks existing deployments; hostile to users mid-migration.

## R8 — Local workspace directory rename (developer machine)

**Decision**: Out of scope for the rename PR. The user's local `/Users/mlieberman/Projects/mikebom` checkout is developer-personal. Post-merge, the user can `git mv` at the shell level or `mv mikebom waybill` outside git; all repo-side tooling uses relative paths that work either way.

**Rationale**: FR-018 keeps scope tight; local dir naming isn't a functional-identifier concern.

**Recommendation to developer** (post-merge, non-normative): update your local checkout with `cd .. && mv mikebom waybill && cd waybill && git status` (should be clean).

## R9 — Post-merge cleanup (removing this milestone's local scripts)

**Decision**: `specs/214-rename-to-waybill/scripts/rename_pass.py` is a feature-local implementation aid. It is committed into the branch so reviewers can audit its logic; after PR merge to `main`, a **follow-up cleanup PR removes the script directory** (leaves the spec + plan + research + data-model + contracts as historical artifacts).

**Rationale**: the script is a one-shot substitution helper; there's no ongoing use. Removing it after merge keeps `main` clean.

**Alternatives considered**:
- (a) Keep the script indefinitely as "renamer" reusable tool — REJECTED. mikebom → waybill is a one-shot rename; another such rename would need a different substitution catalog anyway.
- (b) Delete the script in the same commit as the rename — REJECTED. Reviewers reading the PR should see the script that produced the diffs, not have to reconstruct it from the commits.
