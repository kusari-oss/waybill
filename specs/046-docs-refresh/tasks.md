---
description: "Task list — milestone 046 docs refresh"
---

# Tasks: Refresh README and user-facing docs (post-alpha.6)

**Input**: spec.md ✅, plan.md ✅, checklists/requirements.md ✅. (No
research / data-model / contracts / quickstart — same 4-file
template milestones 021/022/023/042 use; the audit IS the
research, no schema or contract changes.)

**Tests**: included as grep-based acceptance checks per FR (no
new code paths to unit-test).

**Organization**: Three user stories mapping to three audit
severity buckets; one commit per story.

## Format: `[ID] [P?] [Story?] Description`

---

## Phase 1: Setup

- [X] T001 `git checkout 046-docs-refresh` (already on branch). Confirm `git status` is clean and `main` is up to date.
- [X] T002 `./scripts/pre-pr.sh` clean (baseline; should pass since no edits yet).

---

## Phase 2: Foundational

(No foundational tasks — every user story is independent of every
other; no shared scaffolding to set up.)

---

## Phase 3: Commit `docs(046/us1)` — fix HIGH-severity factual errors

**Goal**: Three statements in `README.md` /
`docs/user-guide/cli-reference.md` are factually wrong as of
v0.1.0-alpha.6. Fix them.

**Independent test**: greps from FR-004, FR-005, plus
`grep -- '--image-src' docs/user-guide/cli-reference.md` (positive
match).

- [X] T003 [US1] Edit `README.md`: locate the project status header (around the top of the file) and replace `Status: 0.1.0-alpha.3, pre-1.0.` with `Status: 0.1.0-alpha.6, pre-1.0.`. Per spec Edge Cases, this is a literal-string pin; no auto-derivation tooling.
- [X] T004 [US1] Edit `docs/user-guide/cli-reference.md`: rewrite the prose around the `--image <ref>` flag's behavior section to describe **docker-daemon-first** as the default (local cache first, registry fallback). Drop or rewrite any sentence containing the phrase `Refs are pulled from the registry` or `pulled from the registry, layers decompressed` so the description matches the actual default. Add a one-line forward pointer to the `--image-src` row (added in T005).
- [X] T005 [US1] Edit `docs/user-guide/cli-reference.md`: insert a new flag-table row for `--image-src <docker|remote>[,<...>]` between the existing `--image` and `--image-platform` rows (preserving the table's column alignment). Description MUST cover (a) the value grammar (`docker`, `remote`, comma list), (b) the default `docker,remote`, (c) at least two example invocations — one showing default behavior, one showing `--image-src remote` to force a registry pull, and (d) a one-line "when to override" rationale (CI without docker, force-fresh-fetch, etc.).
- [X] T006 [US1] Verify FR-004: `grep -nE 'Status: 0\.1\.0-alpha\.[0-5]([^0-9]|$)' README.md` returns zero matches.
- [X] T007 [US1] Verify FR-005: `grep -nE 'Refs are pulled from the registry|pulled from the registry, layers decompressed' docs/user-guide/cli-reference.md` returns zero matches.
- [X] T008 [US1] Verify positive: `grep -n -- '--image-src' docs/user-guide/cli-reference.md` returns at least one match in the `sbom scan` flag-table region.
- [X] T009 [US1] `./scripts/pre-pr.sh` clean.
- [X] T010 [US1] Commit: `docs(046/us1): correct alpha-6 version pin + document --image-src + describe docker-daemon-first default`.

---

## Phase 4: Commit `docs(046/us2)` — surface shipped capabilities

**Goal**: Four MEDIUM-severity discoverability gaps. Capabilities
exist; docs don't surface them.

**Independent test**: greps from FR-006, FR-007, FR-008.

- [X] T011 [US2] Edit `README.md` "Stable recipes" (or equivalent recipes) section: add a recipe `mikebom sbom scan --image alpine:3.19` with one paragraph of prose explaining mikebom checks the local docker daemon first and falls back to a registry pull on miss. Position near the existing `docker save` recipe so users see both options together.
- [X] T012 [US2] Edit `README.md` recipes section: add a follow-up recipe `mikebom sbom scan --image <ref> --image-src remote` for force-registry-fetch use cases (CI without docker, or after rebuilding a tag and wanting the latest). One sentence of when-to-use prose.
- [X] T013 [US2] Edit `docs/user-guide/cli-reference.md`: rewrite the description of `--include-legacy-rpmdb`. Drop ALL language matching `(no-op|threads through|until that code lands|deferred|milestone 004)`. New description (1–2 sentences) names what the flag does: enables BDB-format rpmdb reading for pre-RHEL-8 / CentOS-7 / Amazon-Linux-2 base images.
- [X] T014 [US2] Edit `docs/user-guide/cli-reference.md` flag-table rows for `--no-oci-cache` and `--oci-cache-size`: append a markdown link `[OCI layer caching](#oci-layer-caching)` (or whatever the section's anchor is — verify it matches the in-doc heading slug) to each row's description. Goal: a user skimming the flag table can jump straight to the section that explains semantics.
- [X] T015 [US2] Verify FR-006: `grep -nE 'mikebom sbom scan --image alpine|mikebom sbom scan --image [^[:space:]]+ --image-src remote' README.md` returns at least 2 matches.
- [X] T016 [US2] Verify FR-007: `grep -nE 'no-op|threads through|until that code lands|deferred|milestone 004' docs/user-guide/cli-reference.md` returns zero matches in the `--include-legacy-rpmdb` row context.
- [X] T017 [US2] Verify FR-008: `grep -n 'oci-layer-caching\|OCI layer caching' docs/user-guide/cli-reference.md` shows links from BOTH the `--no-oci-cache` and `--oci-cache-size` flag rows.
- [X] T018 [US2] `./scripts/pre-pr.sh` clean.
- [X] T019 [US2] Commit: `docs(046/us2): surface --image OCI ref + --image-src recipes; refresh --include-legacy-rpmdb; cross-link OCI cache flags`.

---

## Phase 5: Commit `docs(046/us3)` — cosmetic / framing cleanup

**Goal**: Three LOW-severity items. Pure polish — no behavior
implications.

**Independent test**: SC-003's repo-wide grep.

- [X] T020 [US3] Edit `README.md` intro paragraph: drop the `(new in milestone 013)` parenthetical from the SBOM-analysis description. Either delete entirely or replace with `since v0.1.0-alpha.X` if the version origin is known and worth surfacing.
- [X] T021 [US3] Edit `docs/user-guide/cli-reference.md`: locate and remove the `# 'oci-registry' is on by default as of milestone 033.` comment (and any sibling `# milestone NNN` jargon comments in user-facing example blocks). The behavior is documented in the surrounding prose; the milestone-numbered comments are internal-jargon leakage.
- [X] T022 [US3] Edit `docs/design-notes.md`: scan the deferred-items lists (around lines 226–236, 372–378 per the audit). For items the audit confirmed are still genuinely backlogged (glibc/musl/V8 version-string detection, PE Authenticode), keep the entry but reframe milestone-numbered framing where it adds no signal. FR-011 is loose here — the goal is editorial cleanup, not exhaustive removal.
- [X] T023 [US3] Verify SC-003: `grep -rnE '(new in milestone|milestone 0[0-3][0-9])' README.md docs/user-guide/ docs/reference/` returns zero matches. (CHANGELOG and `docs/contributing/` are exempt by design.)
- [X] T024 [US3] `./scripts/pre-pr.sh` clean.
- [X] T025 [US3] Commit: `docs(046/us3): drop stale milestone-numbered framing from user-facing docs`.

---

## Phase 6: Polish & PR

- [X] T026 Verify SC-004: `git diff main..HEAD --stat -- mikebom-cli/src/ mikebom-common/src/ mikebom-cli/tests/ mikebom-cli/tests/fixtures/` shows zero lines. Docs-only milestone — any non-doc diff is a violation.
- [X] T027 Verify SC-005: `./scripts/pre-pr.sh` clean from a fresh shell. Should be tautology since each commit ran it, but final pass catches any across-commit interaction.
- [X] T028 Push branch: `git push -u origin 046-docs-refresh`.
- [X] T029 Open PR titled `docs(046): refresh README + cli-reference for post-alpha.6 reality`. Body includes: 3-bucket audit summary, per-commit scope, the 6 SC verification commands.
- [ ] T030 Verify SC-006: all 3 CI lanes (linux x86_64, linux ebpf, macos-latest) green on the PR.

---

## Dependency graph

```text
T001-T002 (setup)
   │
   ├──►  T003-T010   [Commit 1: US1 — HIGH-severity fixes]
   │
   ├──►  T011-T019   [Commit 2: US2 — MEDIUM discoverability]
   │
   └──►  T020-T025   [Commit 3: US3 — LOW cosmetic]
                        │
                        ▼
                T026-T030 (verify + PR)
```

US1, US2, US3 are independent — each closes a distinct audit-severity
bucket and could ship as a standalone PR. Bundled into one PR for
review economy (single editorial pass on shared files).

## Parallel opportunities

Within each commit, edits land in different files (README.md vs
cli-reference.md vs design-notes.md), so individual edit tasks are
parallelizable. The commits themselves are sequenced (one PR, three
commits in priority order) to keep the diff readable.

| Bucket | Parallel-eligible tasks |
|---|---|
| US1 | T003 (README) || T004+T005 (cli-reference, same file → sequential within commit) |
| US2 | T011+T012 (README, same file → sequential) || T013+T014 (cli-reference, same file → sequential) |
| US3 | T020 (README) || T021 (cli-reference) || T022 (design-notes) — three different files, all parallel |

## Estimated effort

| Phase | Effort | Notes |
|---|---|---|
| Phase 1 (setup) | 5 min | Just confirms baseline |
| Phase 3 (US1) | 30 min | New `--image-src` row is the meatiest edit |
| Phase 4 (US2) | 45 min | Two recipe additions + flag rewrite + 2 cross-links |
| Phase 5 (US3) | 20 min | Three small surgical edits |
| Phase 6 (verify + PR) | 15 min | Greps + push + PR description |
| **Total** | **~2 hr** | One focused session |

## MVP scope

US1 alone (commits 1) is a viable MVP: it closes the three
actively-misleading items. Shipping just US1 leaves the
discoverability and cosmetic gaps but stops the bleeding on the
factual errors. US2 and US3 are pure improvements — defer if
review bandwidth is tight, ship together if not.

The current plan ships all three in one PR for review economy.
