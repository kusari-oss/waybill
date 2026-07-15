# Contract: `public-corpus.yml` Regen-Dispatch Surface

**Date**: 2026-07-14
**Purpose**: Documents the small addition to `.github/workflows/public-corpus.yml`'s dispatch input surface introduced by m196. Establishes the shape the workflow presents to maintainers who want to regen goldens on-runner.

## Trigger Surface Added

### `workflow_dispatch.inputs.regen_goldens` (boolean, default `false`)

When set to `true` via manual dispatch (either `gh workflow run` CLI or the GitHub Actions UI), the workflow:

1. Runs the corpus with `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1` injected — the m195 harness writes goldens instead of comparing against them.
2. Uploads the entire `mikebom-cli/tests/fixtures/public_corpus/` directory as an artifact named `corpus-goldens-regen`, retention 14 days.
3. Workflow status (pass/fail) reflects Layer 1 assertion results — Layer 2 golden diff is bypassed by the regen mode (per m195 `layer2_golden::compare_golden` contract).

## Invocation Examples

Manual dispatch from CLI:

```bash
gh workflow run public-corpus.yml \
  --ref 196-populate-corpus-goldens \
  -f branch=196-populate-corpus-goldens \
  -f regen_goldens=true
```

Manual dispatch from GitHub UI: Actions → Public corpus regression → Run workflow → check "Regenerate public-corpus goldens" → Run workflow.

## Post-Dispatch Retrieval Flow

1. Wait for the workflow to complete (`gh run watch <run-id>`).
2. Download the artifact into the fixtures directory in place:

    ```bash
    gh run download <run-id> \
      --name corpus-goldens-regen \
      -D mikebom-cli/tests/fixtures/public_corpus/
    ```

3. Review the diff:

    ```bash
    git status mikebom-cli/tests/fixtures/public_corpus/
    git diff --stat mikebom-cli/tests/fixtures/public_corpus/
    # Verify: no changes to go-cobra/ (FR-005 gate)
    git diff --stat mikebom-cli/tests/fixtures/public_corpus/go-cobra/
    ```

4. Commit alongside any other m196 changes (postgres digest, assertion adjustments).

## Contract Invariants

- **Regen mode is opt-in per dispatch**: the default `regen_goldens=false` MUST preserve the nightly workflow's read-only-verify behavior. Nightly cron runs MUST NEVER regen goldens.
- **Regen mode writes ALL passing targets**: the harness does not accept a per-target filter for regen (m195 T014 contract). The FR-005 additive-only guarantee is enforced at commit-time via `git diff` review, not at regen-time.
- **Artifact name is stable**: `corpus-goldens-regen`. Downstream scripts / documentation reference this name.
- **Artifact path shape**: the artifact preserves the `<target>/{cdx,spdx-2.3,spdx-3}.json` layout so `gh run download -D <fixtures-dir>` places files where they belong.
- **Retention**: 14 days. Enough for a maintainer to fetch and commit; not so long that artifacts accumulate.

## Non-Goals

- Automating the commit itself (per FR-008 human-in-loop invariant — inherited from m195).
- Filtering regen to specific targets (would add code paths for negligible benefit; git-diff at commit is a sufficient gate).
- Cross-platform regen (macOS runners deliberately excluded per Q1 clarification).
