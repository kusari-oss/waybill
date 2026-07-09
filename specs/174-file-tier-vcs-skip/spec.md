# Feature Specification: Exclude VCS metadata directories from the file-tier walker

**Feature Branch**: `174-file-tier-vcs-skip`
**Created**: 2026-07-08
**Status**: Draft
**Input**: User description: "m174 — fix file-tier walker emitting `.git/hooks/*.sample` (and other VCS metadata files) as SBOM components. Repro: `mikebom sbom scan --path <any-cloned-git-repo>` on a repo without pre-existing exclusions surfaces 14+ `.sample` files under `pkg:generic/file-tier?content-sha256=...` bom-refs. Root cause: `mikebom-cli/src/scan_fs/file_tier/walker.rs` doesn't skip `.git/` (or `.hg/`, `.svn/`) directories. Per-ecosystem walkers already exclude these — the file-tier walker (added m133) missed the treatment."

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Clean SBOM from a git-cloned repository (Priority: P1)

An operator runs `mikebom sbom scan --path <repo>` against any repository that was cloned via `git clone` (which populates `.git/hooks/` with git's default `.sample` templates on every clone). Post-scan, the emitted SBOM must not contain components representing those hook templates — they are noise emitted by git itself, present in every repo everywhere, and have zero attribution value.

**Why this priority**: this is the direct bug motivating the milestone. Every git-cloned repo scan today has this contamination — the langflow audit surfaced 14 spurious `pkg:generic/file-tier?content-sha256=...` bom-refs pointing at `.git/hooks/*.sample` files. Downstream SBOM consumers (vulnerability scanners, license auditors) waste review cycles on these phantoms. Fixing the walker is the smallest possible change that eliminates the contamination for every scan.

**Independent Test**: run `mikebom sbom scan --path <path-to-any-recently-cloned-git-repo>` (e.g. the mikebom repo itself). Assert the emitted SBOM contains ZERO components whose `mikebom:source-files` annotation value contains a path starting with `.git/`. Assert the same for `.hg/` and `.svn/`.

**Acceptance Scenarios**:

1. **Given** a repository that has a `.git/` directory containing git's default hook samples, **When** the operator runs `mikebom sbom scan --path <repo>`, **Then** the emitted SBOM contains NO components representing any file inside `.git/` at any depth.
2. **Given** a repository that has a `.hg/` directory (Mercurial), **When** the operator runs `mikebom sbom scan --path <repo>`, **Then** the emitted SBOM contains NO components representing any file inside `.hg/` at any depth.
3. **Given** a repository that has a `.svn/` directory (Subversion), **When** the operator runs `mikebom sbom scan --path <repo>`, **Then** the emitted SBOM contains NO components representing any file inside `.svn/` at any depth.
4. **Given** a repository that has NO VCS metadata directory at all (e.g., a scanned rootfs from an image extraction), **When** the operator runs `mikebom sbom scan --path <path>`, **Then** the scan behavior is byte-identical to pre-174 output (no other file-tier components disappear).

---

### User Story 2 — First-party scripts stored at repo root still surface correctly (Priority: P1)

An operator has a repository with legitimate first-party scripts at the repo root (e.g., `dev.start.sh`, `ci-push.sh`, `run-tests.sh`, `build.ps1`). These files have no package manifest attributing them — they're first-party operator content. The file-tier walker should continue to surface them as `pkg:generic/file-tier?content-sha256=...` components. The VCS exclusion must NOT accidentally suppress these.

**Why this priority**: preserves the m133 file-tier walker's core value proposition. Any regression here means the fix trades one class of noise for a different class of blind spot. Ranked P1 alongside US1 because the two are load-bearing together — the fix is only valuable if it correctly distinguishes VCS metadata (out) from repo-root shell scripts (in).

**Independent Test**: use a repository fixture with both a `.git/` directory AND first-party scripts at repo root. Assert the emitted SBOM contains ZERO components from `.git/` AND still contains the first-party scripts (matched by name).

**Acceptance Scenarios**:

1. **Given** a repository with `.git/hooks/pre-commit.sample` AND a first-party `dev.start.sh` at the repo root, **When** the operator runs `mikebom sbom scan --path <repo>`, **Then** the emitted SBOM contains a component representing `dev.start.sh` AND contains NO component representing `pre-commit.sample`.
2. **Given** a repository where a directory OTHER than the VCS-metadata names is called `.gitignore-alike` or `.githooks` (note: without the exact `.git` name), **When** the scan runs, **Then** the walker treats these directories normally (they are not VCS metadata — only exact-name matches on `.git`, `.hg`, `.svn` are excluded).

---

### User Story 3 — Existing operator-level `--exclude-path` overrides still work (Priority: P2)

An operator has been using `--exclude-path .git/**` (or similar patterns) as a manual workaround for the pre-174 bug. Post-174, the flag continues to work for its intended purpose (excluding OTHER paths the operator doesn't want scanned). The VCS exclusion is layered underneath the operator's flag machinery — an operator setting `--exclude-path` for something unrelated shouldn't interact with the VCS-skip logic.

**Why this priority**: preserves existing operator workarounds without regression. Operators who were using `--exclude-path .git/**` as a workaround simply don't need it anymore, but their existing flag setups still parse and behave the same for other patterns. Ranked P2 because the direct fix (US1 + US2) fully solves the reported bug; this story just codifies the non-regression guarantee.

**Independent Test**: run a scan with an existing `--exclude-path` pattern that targets some non-VCS path (e.g., `--exclude-path 'tests/fixtures/**'`). Assert the flag continues to work exactly as it did pre-174 (that subpath is excluded), AND the VCS metadata is also excluded (by the new built-in behavior).

**Acceptance Scenarios**:

1. **Given** a scan invocation with `--exclude-path 'tests/fixtures/**'`, **When** the walker runs, **Then** the operator's pattern is honored AND VCS directories are ALSO excluded (both behaviors compose without conflict).
2. **Given** a scan invocation that redundantly passes `--exclude-path '.git/**'` (an operator's pre-174 workaround they haven't yet cleaned up), **When** the walker runs, **Then** no error, no warning, and the outcome is identical to the fix's built-in behavior — the extra `--exclude-path` is a harmless no-op.

---

### Edge Cases

- **Nested VCS metadata inside a scanned rootfs**: an OS image rootfs sometimes has a `.git/` directory embedded inside `/opt/some-app/.git/` (e.g., an application shipped as a git checkout). This case is treated identically to top-level `.git/` — the entire subtree is excluded from the file-tier walker.
- **Bare git repository** (no working tree, everything IS the git metadata): `mikebom sbom scan --path <bare-repo>` completes successfully but MAY emit file-tier components for readable text files at the bare repo's root (`HEAD`, `refs/heads/main`, `packed-refs`, `config`). The m174 exclusion is scoped to descendants named `.git`/`.hg`/`.svn` at any depth; a bare repo's OWN root is not named that, so the exclusion does not fire. Bare-repo detection (recognizing the internal-layout shape) is deliberately out of scope for m174 — the reported bug is about regular git-cloned working trees leaking their `.git/hooks/*.sample` templates, which the m174 fix does solve.
- **`.git` file (not directory)**: git submodules use a `.git` FILE (containing a `gitdir:` pointer) rather than a directory. The exclusion applies to `.git` at any file-system entry type — file OR directory — because a `.git` FILE doesn't contain source content either.
- **`.git/` inside a scanned path that was intended as a scan target**: rare but valid. Example: an operator wants to scan a repo's `.git/objects/` directory for some pack-file-inspection reason. In this case the operator explicitly passes `--path <repo>/.git/objects` — the exclusion is scoped to descendants of the scanned root, not to the root itself. This behavior matches the operator's explicit choice.
- **Case sensitivity on macOS/Windows filesystems**: filesystems that fold case (HFS+ default, NTFS) may present a `.GIT/` directory. The exclusion is exact-name-match on the canonical VCS-metadata names (`.git`, `.hg`, `.svn`); the walker does not attempt fold-safe comparison. Rationale: no known VCS actually creates upper-case metadata directories; false positives from operator-created folders with matching names but different case are treated as legitimate scan content.
- **Operator-provided `.gitignore` file at repo root**: NOT excluded. A `.gitignore` file is repo content, not VCS metadata — the `.git/` directory contains git metadata; the `.gitignore` file is operator-authored configuration. The exclusion applies ONLY to the metadata directory tree.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The file-tier walker MUST NOT descend into any directory whose base name exactly matches `.git`, `.hg`, or `.svn`, regardless of the depth at which that directory appears within the scanned path.
- **FR-002**: The file-tier walker MUST NOT emit a file-tier component for any file whose base name is `.git` (a git-submodule pointer file — non-directory case).
- **FR-003**: The exclusion set MUST be an internal built-in list, not operator-configurable via a new flag in this milestone. Operators who need to override or extend the list can continue using the existing `--exclude-path` mechanism as a workaround.
- **FR-004**: Per-ecosystem walkers (dart, cocoapods, composer, erlang, haskell, scala, rpm_file, ipk_file, and any other walker that already excludes `.git`) MUST remain unchanged in behavior. The fix is scoped to the m133 file-tier walker; existing exclusions in sibling walkers are unaffected.
- **FR-005**: Existing per-scan telemetry (`mikebom:file-inventory-skipped-oversize`, `mikebom:file-inventory-mode`, etc.) MUST remain unchanged in behavior on scans of repos that DO NOT have any VCS metadata. Scans of repos WITH VCS metadata may report different `shape_skipped` / total-visited counters, but no new counter category is added in this milestone.
- **FR-006**: Non-VCS-metadata directories whose names LOOK similar (e.g., `.github/`, `.githooks/`, `.gitignore-fixtures/`) MUST NOT be excluded — the match is exact against the closed 3-name list.
- **FR-007**: When the operator scans a bare repository (no working tree, i.e., a directory whose top-level contents ARE what a `.git/` interior normally contains: `HEAD`, `objects/`, `refs/`, `packed-refs`, `config`, etc.), the tool MUST complete the scan successfully, not error out. The tool MAY still emit file-tier components for readable text files at the bare repo's top level — the m174 exclusion is scoped to descendants named `.git` / `.hg` / `.svn`, and a bare repo's own root is not named that. This behavior is a deliberate scope limit of m174; adding bare-repo detection is out of scope and could be a follow-up milestone if operator demand surfaces.
- **FR-008**: The exclusion MUST apply during the m133 file-tier walker's directory descent, so the walker never opens `.git/`, `.hg/`, or `.svn/` subtrees at all. This prevents wasted I/O + spurious `shape_skipped` counter inflation.
- **FR-009**: The tool MUST NOT emit any warning or log line about the built-in VCS exclusion during a normal scan (the exclusion is unremarkable operator-invisible plumbing, not a diagnostic signal). Debug-level (`tracing::debug!`) logging of "skipping VCS metadata directory" MAY be emitted for troubleshooting but MUST NOT appear at INFO or higher.

### Key Entities

- **VCS metadata directory name**: closed set of three exact base-name strings: `.git`, `.hg`, `.svn`. This is the ENTIRE scope of the exclusion — no glob patterns, no case variants, no extensions. Adding a fourth name requires a follow-up milestone.
- **File-tier walker directory-skip decision**: pre-descend check happening at every directory encountered by the walker. Returns "skip subtree entirely" when the directory's base name matches the closed set; returns "descend normally" otherwise. The walker's other skip decisions (size cap, content-shape filters, operator `--exclude-path` patterns) compose alongside this check without ordering constraints.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An operator scanning any git-cloned repository at repo root produces an SBOM containing ZERO components whose `mikebom:source-files` annotation contains a path starting with `.git/`. Verified by a golden fixture test on a repository with a populated `.git/hooks/` subtree.
- **SC-002**: The same operator's SBOM continues to contain their first-party scripts (`ci-push.sh`, `dev.start.sh`, `run-tests.sh`, `*.ps1`, etc.) as file-tier components. Verified by the same fixture asserting the presence of at least one named first-party script.
- **SC-003**: Scans of repositories that have NO VCS metadata directories produce byte-identical SBOMs pre-174 vs post-174. Verified by the existing byte-identity golden regression suite showing zero delta on the ~30 non-VCS-containing golden fixtures.
- **SC-004**: The scan wall-clock time for a repository with a heavy `.git/objects/` subtree (e.g., a large monorepo's git history) reduces by ≥25% post-174 vs pre-174 for the file-tier walker phase specifically. Verified by a signal-only measurement on the mikebom repo itself (its `.git/objects/pack/` alone is >100 MB). This is a nice-to-have side effect; the exclusion primarily targets correctness, not performance.
- **SC-005**: Scans of a Mercurial-managed repository (with `.hg/`) and a Subversion-managed repository (with `.svn/`) produce SBOMs with zero components under those paths. Verified via synthesized fixtures.
- **SC-006**: An operator who was using `--exclude-path '.git/**'` as a pre-174 workaround can remove that flag from their scan invocation without any behavioral change in the emitted SBOM (the built-in exclusion covers the same ground). Verified by a scan-with-vs-scan-without comparison producing byte-identical SBOMs.

## Assumptions

- The closed set of three VCS-metadata names (`.git`, `.hg`, `.svn`) covers the observed 99%+ of real-world scan targets. Rare VCS systems (Bazaar `.bzr/`, Fossil `.fslckout`, Perforce workspaces) can be added in a follow-up milestone if operator demand surfaces; adding a fourth name here would expand test surface without clear benefit.
- Operators do not intentionally scan `.git/` subtrees as their primary target. If they do (e.g., forensic tools inspecting pack files), they explicitly pass `--path <repo>/.git/subpath` which points inside a metadata directory and correctly bypasses the exclusion (the exclusion is scoped to descendants of the scanned root).
- Fold-case filesystems (macOS HFS+, Windows NTFS with case-insensitivity enabled) do not create upper-case metadata directory names on their own. Every git tool creates `.git/` in lowercase; every Mercurial tool creates `.hg/`; every Subversion tool creates `.svn/`. Fold-case comparison would add complexity for zero observed benefit and would risk false-positive exclusion of unrelated operator content.
- The fix belongs in the m133 file-tier walker specifically, not layered as a generic `scan_fs/walk::safe_walk`-level exclusion. Rationale: per-ecosystem walkers already have their own VCS exclusion logic tuned to each ecosystem's traversal semantics (e.g., cocoapods excludes `.git`+`.svn`+`.hg`+`Pods`+`node_modules`+`build`+`DerivedData`; the file-tier walker's scope is different). Centralizing all VCS exclusion in one place would be a larger refactor with cross-ecosystem risk; the surgical fix in the m133 walker delivers the reported user value without that risk.
- The operator's existing `--exclude-path` flag mechanism (milestone 113) continues to work exactly as it did before. This milestone adds a built-in floor of always-excluded VCS names; the operator's per-scan patterns compose on top.
