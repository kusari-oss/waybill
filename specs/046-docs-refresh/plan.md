# Implementation Plan: Refresh README and user-facing docs

**Branch**: `046-docs-refresh` | **Date**: 2026-04-29 | **Spec**: [spec.md](./spec.md)
**Input**: [spec.md](./spec.md)

## Summary

Docs-only cleanup of 10 audited drift items between user-facing
documentation (`README.md`, `docs/user-guide/cli-reference.md`, plus
a touch on `docs/design-notes.md`) and the post-alpha.6 reality.
No code changes, no test changes, no fixture or golden regen.

The audit (recorded in spec.md §Background and the per-FR drift
items) is the research phase; this plan is the surgical-edit
phase. Three commits map 1:1 to the three priority buckets — the
P1/MVP closes the actively-misleading items; P2/P3 add
discoverability and polish.

## Technical Context

**Language/Version**: N/A — this milestone touches Markdown only.
**Primary Dependencies**: None new.
**Storage**: N/A.
**Testing**: grep-shaped acceptance tests defined per-FR in spec
(no new code paths). Pre-PR gate (`./scripts/pre-pr.sh`) must
remain green; SC-004 enforces zero diff outside `README.md` and
`docs/`.
**Target Platform**: N/A (docs).
**Project Type**: docs refresh / cli reference reconciliation.
**Performance Goals**: N/A.
**Constraints**: zero `mikebom-cli/src/`, `mikebom-common/src/`,
or `mikebom-cli/tests/fixtures/` diff (FR-012 / SC-004).
**Scale/Scope**: ~10 drift items across 3 files. Estimated total
edit volume: ~150–250 LOC of Markdown.

No NEEDS CLARIFICATION markers — the audit was concrete enough.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1
design.*

This milestone touches no source code, so most Constitution
principles are not in scope. Coverage:

| Principle | Relevance | Status |
|---|---|---|
| I. Pure Rust, Zero C | No code change | ✅ vacuously satisfied |
| II. eBPF-Only Observation | No discovery code change | ✅ N/A |
| III–IV. (existing principles) | No code change | ✅ vacuously satisfied |
| V. Specification Compliance | Docs MUST accurately describe SPDX 2.3 / 3.x emitter labeling, including experimental tags | ✅ checked: CLI reference's format table already labels SPDX 3 correctly; this milestone preserves that and doesn't introduce new format claims |
| VI. Three-crate architecture | No code change | ✅ vacuously satisfied |
| Pre-PR Verification | `./scripts/pre-pr.sh` must pass on the docs-only diff | ✅ enforced by SC-005 |

No gate violations; nothing to justify.

## Approach

Three commits, each independently shippable, in priority order.
Each closes one of the user stories and corresponds to one
audit-severity bucket.

### Commit 1 — `docs(046/us1)` — fix the HIGH-severity factual errors

Scope: README.md status line + the two `docs/user-guide/cli-reference.md`
items.

- README.md status line: replace `Status: 0.1.0-alpha.3, pre-1.0.`
  with `Status: 0.1.0-alpha.6, pre-1.0.`. Single literal-string
  pin per the spec's Edge Cases decision (no Cargo.toml-derivation
  tooling in this milestone).
- CLI reference `--image` flag description: rewrite the short
  prose around the existing `--image <ref>` row to describe
  docker-daemon-first behavior, with a forward pointer to the
  `--image-src` row.
- CLI reference `sbom scan` flag table: insert a new row for
  `--image-src` between the existing `--image` and
  `--image-platform` rows. Description includes value grammar,
  default, two examples (default behavior + `--image-src remote`
  override), and one-line "when to override" rationale.

Verification: greps from FR-004, FR-005, plus a positive-match
grep for `--image-src` in the CLI reference. `./scripts/pre-pr.sh`
clean.

### Commit 2 — `docs(046/us2)` — surface shipped capabilities

Scope: README recipes section + `--include-legacy-rpmdb` description
+ OCI-cache cross-links.

- README "Stable recipes" (or equivalent) section: add two new
  recipes (or upgrade existing ones).
  - Recipe: `mikebom sbom scan --image alpine:3.19` with prose
    explaining docker-daemon-first default, `docker save`
    fallback behavior on miss.
  - Recipe: `mikebom sbom scan --image <ecr-ref> --image-src
    remote` for force-fresh-fetch use cases.
- CLI reference `--include-legacy-rpmdb`: rewrite the
  description. Drop the "no-op until that code lands" /
  "milestone 004 US4" framing entirely. New text: name what
  the flag does (enables BDB-format rpmdb reading for
  pre-RHEL-8 / CentOS-7 / Amazon-Linux-2 images) in 1–2
  sentences.
- CLI reference `--no-oci-cache` and `--oci-cache-size` flag-table
  rows: append a markdown link to the "OCI layer caching"
  in-document section (`[OCI layer caching](#oci-layer-caching)`)
  to each row's description.

Verification: greps from FR-006, FR-007, FR-008.

### Commit 3 — `docs(046/us3)` — cosmetic cleanup

Scope: README intro + CLI reference example comment + design-notes
language.

- README intro paragraph: drop or restate the "(new in milestone
  013)" framing. Either delete the parenthetical or replace with
  a user-facing version reference (e.g. "since v0.1.0-alpha.X").
- CLI reference example comment(s): remove the
  `# 'oci-registry' is on by default as of milestone 033.`
  internal-jargon comment. Either delete (the behavior is
  documented in the surrounding prose) or restate without the
  milestone number.
- design-notes.md: scan the deferred-items lists; reframe
  milestone-numbered framing where appropriate. FR-011 keeps
  this loose (contributor-facing doc); the goal is editorial
  cleanup, not exhaustive removal.

Verification: SC-003's repo-wide grep returns zero matches in
user-facing files.

### Final verification (before PR)

- `./scripts/pre-pr.sh` clean — no source/test/fixture diff.
- `git diff main..HEAD --stat -- mikebom-cli/src/ mikebom-common/src/ mikebom-cli/tests/`
  empty.
- All 6 SC-* greps pass when run from the repo root.
- All 3 CI lanes green on the PR.

## Touched files

| File | Commit | Edit volume |
|---|---|---|
| `README.md` | 1, 2, 3 | ~50 LOC of Markdown across 3 sections (status line, recipes, intro) |
| `docs/user-guide/cli-reference.md` | 1, 2, 3 | ~100 LOC: new `--image-src` table row, rewritten `--image` description, rewritten `--include-legacy-rpmdb` description, OCI-cache cross-links, internal-jargon comment cleanup |
| `docs/design-notes.md` | 3 | <30 LOC: editorial pass on deferred-items lists |

Total Markdown: ~150–200 LOC across 3 files, spread over 3
commits.

## Risks

- **R1: Dropping internal-milestone references too aggressively.**
  CHANGELOG and contributor-facing docs legitimately use milestone
  numbers; the grep in SC-003 is scoped to `README.md` and the
  user-facing doc subdirectories specifically (`docs/user-guide/`,
  `docs/reference/`) to avoid catching legitimate usage in
  `CHANGELOG.md` or `docs/contributing/`. Mitigation: encoded into
  the SC-003 grep pattern.
- **R2: Hard-coded version pin drifts again on the next release
  bump.** Per Edge Cases, this milestone accepts the simpler
  literal-string approach (matches what the rest of the repo
  does). If the next release bump doesn't update the README, we
  see the same drift. Mitigation: a one-line note in the release
  contributor-doc (or the alpha bump PR template) is a follow-on
  worth filing if drift recurs. Out of scope for this milestone.
- **R3: Reviewer spots additional drift the audit missed.**
  Possible — the audit covered what a typical user touches but
  isn't exhaustive. Mitigation: file-as-follow-on policy
  documented in spec's Out-of-scope. Don't expand this PR's scope
  in flight; close the audited drift cleanly, then take a second
  pass if needed.

## Phasing

| Phase | Commits | Effort |
|---|---|---|
| Setup + recon | (audit already done; no setup) | 0 |
| Commit 1 (US1, MVP) | 1 | 30 min |
| Commit 2 (US2) | 1 | 45 min |
| Commit 3 (US3) | 1 | 20 min |
| Verify + PR | 0 (verify gates) | 15 min |
| **Total** | **3 commits** | **~2 hr** |

## What this milestone does NOT do

- Does not auto-generate the CLI reference from `clap`. Worth
  considering, but is itself a multi-hour milestone (would need
  a `cargo run -- --help` post-processor + golden file). Tracked
  as a follow-on if the docs drift recurs.
- Does not introduce a docs build / lint pipeline (markdownlint,
  link checker, prose linter). All would be useful but bigger
  than this cleanup needs.
- Does not change any goldens, fixtures, or SBOM tool-version
  strings. Those are byte-identity-pinned to the workspace
  version and were regenerated in the alpha.6 release PR.
- Does not retitle, move, or split any docs files. Content-
  correctness pass only; no IA refactor.

## Why no `research.md` / `data-model.md` / `contracts/` / `quickstart.md`

Same rationale as milestones 021, 022, 023, 042 (the project's
4-file tighter spec set):

- `research.md`: the audit IS the research; no NEEDS CLARIFICATION
  markers remain in the spec.
- `data-model.md`: no code change → no schema change → no entities
  to model.
- `contracts/`: no public API surface changes; the CLI flag
  surface is reconciled to its existing implementation, not
  changed.
- `quickstart.md`: the spec's User Stories already include
  per-story acceptance scenarios that read like quickstart steps;
  duplicating them here would be noise.

This is the fifth use of the tighter 4-file template (after 021,
022, 023, 042). Pattern stable for genuinely contained content
milestones.
