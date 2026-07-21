# Quickstart: mikebom → waybill rename execution

**Feature**: 214-rename-to-waybill
**Date**: 2026-07-21

End-to-end recipe for executing the rename branch. Follow this in order; each step corresponds to one commit per research R1.

## Prerequisites

- Fresh checkout of `main` at post-alpha.65 state (SHA c4c9b25 or later)
- Local workspace directory at `/Users/mlieberman/Projects/mikebom` (or equivalent)
- `git`, `cargo`, `python3` (for the rename harness script)
- Optional (for local golden regeneration): `cargo test --workspace` toolchain warmed up

## Setup

```bash
cd /Users/mlieberman/Projects/mikebom
git checkout main && git pull
git checkout -b 214-rename-to-waybill
```

## Commit 1: Cargo package + directory renames

```bash
# Rename crate directories via git mv (preserves blame history)
git mv mikebom-cli waybill-cli
git mv mikebom-common waybill-common
git mv mikebom-ebpf waybill-ebpf

# Update workspace + per-crate Cargo.toml `[package].name` and path deps
python3 specs/214-rename-to-waybill/scripts/rename_pass.py --pass 1

# Verify cargo can still resolve the workspace
cargo check --workspace

# Verify eBPF crate config
ls waybill-ebpf/Cargo.toml

git status
git add -A
git commit -m "chore(214): rename crate directories + Cargo package names

- git mv mikebom-{cli,common,ebpf} → waybill-{cli,common,ebpf}
- [package].name in each Cargo.toml
- workspace members + exclude + intra-workspace path deps
- Cargo.lock regenerated via cargo update -w"
```

## Commit 2: Rust identifier rename

```bash
# Substitute mikebom_common / mikebom_cli / mikebom_ebpf module paths
python3 specs/214-rename-to-waybill/scripts/rename_pass.py --pass 2

# Verify the workspace still compiles
cargo check --workspace

git add -A
git commit -m "chore(214): rename Rust module paths mikebom_common → waybill_common"
```

## Commit 3: String-literal + env-var rename

```bash
# Substitute:
#  - "mikebom:*" annotation keys → "waybill:*"
#  - MIKEBOM_* env-var refs → WAYBILL_*
#  - other user-visible strings
python3 specs/214-rename-to-waybill/scripts/rename_pass.py --pass 3

# Cargo builds; tests will pass but golden diffs are deferred to commit 6
cargo check --workspace
cargo test -p waybill-common --lib   # small unit tests should still pass

git add -A
git commit -m "chore(214): rename annotation prefixes + MIKEBOM_* env vars"
```

## Commit 4: Filesystem-artifact + workflow patterns

```bash
# Substitute:
#  - loader.rs::default_ebpf_path
#  - Dockerfile.ebpf-test paths (WORKDIR, ENTRYPOINT)
#  - .github/workflows/release.yml artifact naming + Docker image name
#  - scripts/ebpf-integration-test.sh /mikebom/ paths → /waybill/
python3 specs/214-rename-to-waybill/scripts/rename_pass.py --pass 4

cargo check --workspace

git add -A
git commit -m "chore(214): rename filesystem artifacts + workflow patterns"
```

## Commit 5: Docs + prose rewrite

```bash
# Case-preserving substitution in prose files (README, CLAUDE.md, docs/, constitution.md).
# Creates docs/migration/mikebom-to-waybill.md as a new file.
# Applies constitution MAJOR bump (1.5.0 → 2.0.0) with SYNC IMPACT REPORT block.
python3 specs/214-rename-to-waybill/scripts/rename_pass.py --pass 5

# Manually verify heritage sentences are preserved:
grep -n "previously known as mikebom\|formerly.*[Mm]ikebom" README.md
grep -n "^# Waybill Constitution" .specify/memory/constitution.md
head -30 .specify/memory/constitution.md   # verify SYNC IMPACT REPORT prepended

# Verify migration guide exists
cat docs/migration/mikebom-to-waybill.md | head -20

git add -A
git commit -m "chore(214): rewrite docs + README + constitution + add migration guide

- README + CLAUDE.md prose replacement (Mikebom → Waybill)
- Constitution v1.5.0 → v2.0.0 MAJOR bump with SYNC IMPACT REPORT
- docs/architecture/*, docs/user-guide/*, docs/ecosystems.md prose
- NEW: docs/migration/mikebom-to-waybill.md (per FR-015)
- Heritage sentences preserved in README + audit reports"
```

## Commit 6: Golden regeneration + CI grep gate

```bash
# Regenerate all 34 golden files (mechanical prefix-swap diffs only)
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

# Verify diffs are pure prefix swaps (should return empty)
git diff waybill-cli/tests/fixtures/golden/ | \
  grep -vE 'mikebom|waybill|MIKEBOM|WAYBILL|@@|^diff|^index|^---|^\+\+\+' | \
  head -10

# Add the CI grep gate step per contracts/grep-gate.md
python3 specs/214-rename-to-waybill/scripts/rename_pass.py --pass 6-add-ci-gate

# Verify the gate itself doesn't false-positive against the current tree
BADHITS=$(grep -rE '\bmikebom\b' waybill-cli/src waybill-common/src waybill-ebpf/src xtask/src Cargo.toml waybill-cli/Cargo.toml waybill-common/Cargo.toml waybill-ebpf/Cargo.toml xtask/Cargo.toml .github/workflows/*.yml Dockerfile.ebpf-test scripts 2>/dev/null || true)
if [[ -n "$BADHITS" ]]; then
  echo "FAIL — CI gate would fire on current tree. Fix these before committing:"
  echo "$BADHITS" | head
  exit 1
fi

git add -A
git commit -m "chore(214): regenerate goldens + add CI grep gate for SC-001

- 34 golden files regenerated via WAYBILL_UPDATE_*_GOLDENS=1
- Diffs verified pure prefix-swap (no semantic changes)
- CI ci.yml: new step 'm214 rename-completeness grep gate (SC-001)'
- Post-merge: gate blocks any future PR that reintroduces mikebom in
  functional-identifier positions"
```

## Verification

Before pushing / opening PR:

```bash
# 1. Commit count sanity
git log --oneline main..HEAD   # should show 6 commits

# 2. No mikebom in functional-identifier paths (the CI gate mirror)
BADHITS=$(grep -rE '\bmikebom\b' \
  waybill-cli/src waybill-common/src waybill-ebpf/src xtask/src \
  Cargo.toml waybill-cli/Cargo.toml waybill-common/Cargo.toml waybill-ebpf/Cargo.toml xtask/Cargo.toml \
  .github/workflows/*.yml Dockerfile.ebpf-test scripts 2>/dev/null || true)
[[ -z "$BADHITS" ]] && echo "OK: zero mikebom in functional-identifier paths" || { echo "FAIL:"; echo "$BADHITS"; exit 1; }

# 3. Historical paths still have mikebom (positive check — preservation working)
[[ -n "$(grep -rE '\bmikebom\b' specs/213-kernel-noise-filter/ 2>/dev/null)" ]] && \
  echo "OK: historical spec preservation intact" || \
  echo "FAIL: historical specs got mangled — revert prose pass!"

# 4. Migration guide exists
[[ -f docs/migration/mikebom-to-waybill.md ]] && echo "OK" || { echo "FAIL"; exit 1; }

# 5. Constitution title
head -1 .specify/memory/constitution.md | grep -q '^<!--' && \
  head -100 .specify/memory/constitution.md | grep -q '^# Waybill Constitution' && \
  echo "OK: constitution title + SYNC IMPACT REPORT present" || \
  { echo "FAIL"; exit 1; }

# 6. Cargo builds cleanly (skip full pre-PR — 30+ min from cache invalidation)
cargo check --workspace   # < 2 min, sanity-only

# 7. Push branch
git push origin 214-rename-to-waybill
```

## Open PR

```bash
gh pr create \
  --repo kusari-oss/waybill \
  --base main \
  --head 214-rename-to-waybill \
  --title "chore(214): rename mikebom → waybill across all functional identifiers" \
  --body "@ specs/214-rename-to-waybill/spec.md + PR body per plan.md"
```

**PR body MUST include**:
1. Prominent BREAKING callout: "This release renames the `mikebom:*` annotation prefix to `waybill:*` — see docs/migration/mikebom-to-waybill.md before consuming post-alpha.66 SBOMs."
2. Link to spec + plan + research + data-model + contracts.
3. 6-commit summary (one per commit).
4. Test Plan: (a) CI matrix (all 4 lint+test lanes + inspector + rootfs scanners), (b) grep gate step results, (c) golden diffs verified pure prefix-swap.

## After PR merges

**Post-merge cleanup PR** (separate small PR per research R9):

```bash
git checkout main && git pull
git checkout -b 214-cleanup-rename-scripts
git rm -r specs/214-rename-to-waybill/scripts/
git commit -m "chore(214): remove feature-local rename scripts post-merge

The rename_pass.py script served its purpose in the m214 rename PR
(commits 1-6). Removing now keeps main clean; the historical spec
artifacts (spec/plan/research/data-model/contracts/quickstart/tasks)
remain as reference."
git push origin 214-cleanup-rename-scripts
gh pr create --title "chore(214): cleanup rename scripts"
```

**Release PR** (immediately after m214 merge — this is the alpha.66 release with waybill name):

Follow the standard release-bump procedure per m212/213 precedent:
1. `git checkout -b release-v0.1.0-alpha.66`
2. Bump `Cargo.toml` `[workspace.package].version = "0.1.0-alpha.66"`
3. `cargo update -w -p waybill -p waybill-common`
4. Regenerate goldens with `WAYBILL_UPDATE_*_GOLDENS=1` env vars (same as m214 commit 6)
5. PR title: `release: bump workspace to v0.1.0-alpha.66`
6. On merge, `auto-tag-release.yml` fires (auth pending #623 fix — may need manual tag push)
7. `release.yml` publishes multi-arch binaries `waybill-v0.1.0-alpha.66-*` and Docker image `ghcr.io/kusari-oss/waybill:v0.1.0-alpha.66`

**Developer local checkout** (non-normative per research R8):

```bash
cd /Users/mlieberman/Projects
mv mikebom waybill    # rename local checkout
cd waybill
git status            # should be clean
```

## Rollback recipe

If the rename PR needs to be reverted after merge:

```bash
git revert -m 1 <merge-commit-sha>
```

Reverts all 6 commits + the merge, restoring `main` to alpha.65-post-merge state. Consumers who already migrated to `waybill:*` annotations would need to migrate back to `mikebom:*` — the PR description should make it clear this is a one-way door.
