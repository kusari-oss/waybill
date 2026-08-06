# Contract: `.github/workflows/nightly.yml`

**Feature**: 229-release-flow-impl
**Phase**: 1

Pins the nightly.yml behavior contract so implementation-phase deviations produce visible violations.

## Triggers

- **`schedule: cron: '0 6 * * *'`** — 06:00 UTC daily.
- **`workflow_dispatch:`** — manual escape hatch, no required inputs.

## Workflow-level configuration

```yaml
name: Nightly release
permissions:
  contents: write   # tag push + release create
  actions: write    # gh workflow run dispatch
concurrency:
  group: nightly-release
  cancel-in-progress: false
env:
  BASELINE_VERSION_FILE: Cargo.toml   # source of truth for the current stable baseline
```

## Job structure

Single job `nightly-tag-and-dispatch` on `ubuntu-latest` with the following steps in strict order:

### Step 1 — checkout

```yaml
- uses: actions/checkout@<pinned-sha>
  with:
    fetch-depth: 0   # full history for tag-lookup
    ref: main
```

### Step 2 — skip-if-unchanged

```yaml
- name: Determine skip condition
  id: skip
  run: |
    LAST_NIGHTLY=$(git tag -l 'v*-nightly.*' | sort -V | tail -1)
    if [ -z "$LAST_NIGHTLY" ]; then
      echo "no prior nightly; proceeding"
      echo "skip=false" >> "$GITHUB_OUTPUT"
    else
      LAST_SHA=$(git rev-list -n 1 "$LAST_NIGHTLY")
      HEAD_SHA=$(git rev-parse HEAD)
      if [ "$LAST_SHA" = "$HEAD_SHA" ]; then
        echo "no new commits since last nightly at $LAST_NIGHTLY"
        echo "skip=true" >> "$GITHUB_OUTPUT"
      else
        echo "changes present since $LAST_NIGHTLY; proceeding"
        echo "skip=false" >> "$GITHUB_OUTPUT"
      fi
    fi
```

**Contract**: on skip-true, the workflow exits at Step 2 (all subsequent steps have `if: steps.skip.outputs.skip != 'true'`). Log line contains the FR-001(d) stable substring `"no new commits since last nightly at "`.

### Step 3 — compute tag

```yaml
- name: Compute nightly tag
  id: tag
  if: steps.skip.outputs.skip != 'true'
  run: |
    BASELINE=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    DATE=$(date -u +%Y%m%d)
    TAG="v${BASELINE}-nightly.${DATE}"
    echo "tag=$TAG" >> "$GITHUB_OUTPUT"
```

**Contract**: BASELINE reads `Cargo.toml`'s `[workspace.package].version`. If the workspace is still on `0.1.0-alpha.70` (pre-release-bump PR), the nightly.yml MUST no-op with a warning — the `alpha.70` baseline is not a valid nightly-tag prefix. Implementation adds an early check for this precondition (see Step 3.5).

### Step 3.5 — precondition check

```yaml
- name: Verify stable-model baseline exists
  if: steps.skip.outputs.skip != 'true'
  run: |
    BASELINE=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    if echo "$BASELINE" | grep -q -- '-alpha\.'; then
      echo "::warning::Baseline version is still on the retiring alpha.N model ($BASELINE). Nightly cannot be produced until the v0.2.0 release-bump PR merges."
      echo "exit 0" # graceful no-op
      exit 0
    fi
```

**Contract**: prevents accidental `v0.1.0-alpha.70-nightly.20260806` tags during the transition window.

### Step 4 — create + push tag

```yaml
- name: Create and push nightly tag
  if: steps.skip.outputs.skip != 'true'
  run: |
    TAG="${{ steps.tag.outputs.tag }}"
    git tag -a "$TAG" -m "Nightly release ${TAG}"
    git push origin "$TAG"
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### Step 5 — dispatch release.yml

```yaml
- name: Dispatch release workflow
  if: steps.skip.outputs.skip != 'true'
  run: |
    gh workflow run release.yml \
      -f tag="${{ steps.tag.outputs.tag }}" \
      --ref main
  env:
    GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

**Contract**: matches the existing `auto-tag-release.yml` handoff pattern (research §A). `--ref main` ensures release.yml's checkout uses `main`-current definition of the workflow (defensive against workflow-file changes on a feature branch).

### Step 6 — retention cleanup

```yaml
- name: Cleanup nightlies older than 30 days
  if: steps.skip.outputs.skip != 'true' && always()
  continue-on-error: true
  run: |
    CUTOFF=$(date -u -d '30 days ago' +%Y-%m-%dT%H:%M:%SZ)
    gh release list --limit 200 --exclude-drafts \
      --json name,tagName,createdAt,isPrerelease \
    | jq -r --arg cutoff "$CUTOFF" \
        '.[] | select(
          .isPrerelease
          and (.tagName | test("^v[0-9]+\\.[0-9]+\\.[0-9]+-nightly\\.[0-9]{8}$"))
          and (.createdAt < $cutoff)
        ) | .tagName' \
    | while read -r TAG; do
        [ -z "$TAG" ] && continue
        echo "Deleting nightly $TAG (created before $CUTOFF)"
        gh release delete "$TAG" --yes --cleanup-tag || echo "::warning::Failed to delete $TAG"
      done
  env:
    GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

**Contract**: per research §B. Anchored regex prevents accidental deletion of bridge pre-releases. `continue-on-error: true` matches FR-011a's "cleanup failures don't fail the workflow-run".

## Invariants

- **Signing scope**: nightly.yml does NOT sign artifacts directly. Signing happens inside release.yml (which nightly.yml dispatches). Unified signing pipeline per Q2.
- **Idempotency**: if nightly.yml is manually re-run on a day where the nightly tag already exists (e.g., accidental workflow_dispatch), the tag-push step fails (`git push` rejects existing tag), which is fine — the workflow-run fails visibly rather than duplicating a release.
- **Concurrency**: `concurrency.group: nightly-release` + `cancel-in-progress: false` — if a cron and a workflow_dispatch fire simultaneously, they queue; only one runs at a time.

## Test plan (from FR-011)

- **Static**: `actionlint .github/workflows/nightly.yml` returns clean.
- **Dry-run**: `workflow_dispatch` on a feature branch (`229-release-flow-impl`) — the tag push targets the feature-branch commit; must verify the tag gets deleted post-test.
- **First cron on main**: post-merge, first cron fire produces a `v0.2.0-nightly.<today>` tag on the release page.
- **Skip-if-unchanged**: seed a "no changes" period (rare in practice), verify the skip log substring appears.
- **Retention**: seed 31-day-old nightly tag manually, verify cleanup step deletes it on next cron.
