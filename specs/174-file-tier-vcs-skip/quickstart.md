# Quickstart: m174 Manual Verification

**Feature**: 174-file-tier-vcs-skip
**Date**: 2026-07-08

Three verification paths — one per user story — plus a bonus path exercising the langflow bug repro end-to-end.

## Path A — US1: git-cloned repo produces zero `.git/` components

**Setup**: use ANY recently git-cloned repository. The mikebom repo itself works.

```bash
mikebom sbom scan --path /Users/mlieberman/Projects/mikebom \
    --format cyclonedx-json \
    --output cyclonedx-json=/tmp/mikebom-self.cdx.json \
    --no-deep-hash

# Assert zero .git/ paths in emitted SBOM
jq '[.components[]?.properties[]?
     | select(.name == "mikebom:source-files")
     | .value | fromjson | .[]
     | select(startswith(".git/"))] | length' /tmp/mikebom-self.cdx.json
```

**Expected**: `0`. Any positive integer means the fix regressed.

Also assert no `pkg:generic/file-tier` component whose `name` is a git hook sample:

```bash
jq -r '.components[]?
       | select((.purl // "") | startswith("pkg:generic/file-tier"))
       | .name' /tmp/mikebom-self.cdx.json | grep -c '\.sample$'
```

**Expected**: `0` matches.

## Path B — US2: first-party scripts still surface

**Setup**: a repository with both a `.git/` directory AND a first-party shell script at repo root. The mikebom repo itself has `scripts/pre-pr.sh` and similar.

```bash
# Verify first-party scripts still surface as file-tier components
jq -r '.components[]?
       | select((.purl // "") | startswith("pkg:generic/file-tier"))
       | .name' /tmp/mikebom-self.cdx.json | grep -cE '\.(sh|ps1|py)$'
```

**Expected**: a positive integer matching the count of first-party scripts in the scanned repo. `0` would mean the fix over-corrected and suppressed legitimate content.

## Path C — US3: `--exclude-path` still composes

**Setup**: pass an `--exclude-path` for something unrelated and verify no interaction with the VCS exclusion.

```bash
mikebom sbom scan --path /Users/mlieberman/Projects/mikebom \
    --exclude-path 'target/**' \
    --format cyclonedx-json \
    --output cyclonedx-json=/tmp/mikebom-excluded.cdx.json \
    --no-deep-hash

# Assert: same zero .git/ paths as Path A
jq '[.components[]?.properties[]?
     | select(.name == "mikebom:source-files")
     | .value | fromjson | .[]
     | select(startswith(".git/"))] | length' /tmp/mikebom-excluded.cdx.json
```

**Expected**: `0`. The `--exclude-path` for `target/**` composes alongside the built-in VCS exclusion; both take effect.

Also assert nothing under `target/` leaked in:

```bash
jq '[.components[]?.properties[]?
     | select(.name == "mikebom:source-files")
     | .value | fromjson | .[]
     | select(startswith("target/"))] | length' /tmp/mikebom-excluded.cdx.json
```

**Expected**: `0`.

## Bonus Path — Reproduce the langflow audit bug

**Setup**: the langflow test repo used during the m174 spec-authoring audit.

```bash
# Clone if not already present
[ -d /tmp/test-langflow ] || git clone --depth 1 https://github.com/kusari-sandbox/test-langflow /tmp/test-langflow

# Scan
mikebom --offline sbom scan --path /tmp/test-langflow \
    --output /tmp/langflow-post-174.cdx.json \
    --no-deep-hash

# Assert the bug is fixed
jq '[.components[]?
     | select(.name | test("\\.sample$"))] | length' /tmp/langflow-post-174.cdx.json
```

**Expected**: `0`. Pre-174 this returned `14`. The 14 excluded were:
```
applypatch-msg.sample     pre-applypatch.sample     pre-push.sample
commit-msg.sample         pre-commit.sample         pre-rebase.sample
fsmonitor-watchman.sample pre-merge-commit.sample   pre-receive.sample
post-update.sample        prepare-commit-msg.sample push-to-checkout.sample
sendemail-validate.sample update.sample
```

Total component count should drop by exactly 14 from the pre-174 scan of the same repo (assuming no other repo content changed).

## Full success criteria table

| SC | Verification | Expected |
|---|---|---|
| SC-001 | Path A first jq | `0` |
| SC-002 | Path B jq | positive integer (unchanged from pre-174) |
| SC-003 | `git diff main --stat -- 'mikebom-cli/tests/fixtures/golden/**'` | empty (no golden delta) |
| SC-004 | `time` comparison pre-174 vs post-174 scan of any repo with heavy `.git/objects/pack/` | ≥25% wall-clock reduction (signal only) |
| SC-005 | Path A with `.hg/` fixture, `.svn/` fixture | `0` for each |
| SC-006 | Path C vs Path A byte-identity comparison (modulo the removed target/ paths) | matched deltas |
