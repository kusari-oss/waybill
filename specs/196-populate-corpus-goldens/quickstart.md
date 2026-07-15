# Quickstart: Populate Remaining Public-Corpus Goldens

**Date**: 2026-07-14
**Audience**: mikebom maintainer running m196's regen workflow to populate the 5 remaining target goldens.

## Prerequisites

- Working mikebom checkout on branch `196-populate-corpus-goldens`.
- `gh` CLI authenticated against `kusari-oss/mikebom` (read + write for artifact download).
- `docker` on `$PATH` locally — for the one-shot postgres:16 digest resolution.
- No local Rust compilation or corpus scans required — all heavy lifting runs on the CI runner.

## Reproducer 1 — Resolve postgres:16 digest (one-shot, local)

```bash
docker manifest inspect --verbose docker.io/library/postgres:16 \
  | jq -r '.[] | select(.Descriptor.platform.architecture == "amd64"
      and .Descriptor.platform.os == "linux") | .Descriptor.digest'
```

**Expected output**: `sha256:<64-hex>` — the amd64 platform digest of postgres:16 at the time of resolution.

Edit `mikebom-cli/tests/corpus_harness_195/manifest.rs`, find `name: "image-postgres16"`, replace the placeholder `algo_hex` field value with the resolved digest. Commit with the rationale comment from data-model.md M1.

## Reproducer 2 — Add workflow_dispatch input to public-corpus.yml

Apply the three edits from data-model.md M3:

1. Add `regen_goldens: type: boolean, default: false` to `workflow_dispatch.inputs`.
2. Conditionally inject `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS` env into the "Run public corpus" step.
3. Add the `Upload regenerated goldens` step gated on `if: inputs.regen_goldens == true`.

Commit + push to `196-populate-corpus-goldens`.

## Reproducer 3 — Dispatch the regen and fetch the artifact

```bash
gh workflow run public-corpus.yml \
  --ref 196-populate-corpus-goldens \
  -f branch=196-populate-corpus-goldens \
  -f regen_goldens=true

# Get the run ID:
RUN_ID=$(gh run list --workflow=public-corpus.yml --branch=196-populate-corpus-goldens --limit=1 --json databaseId --jq '.[0].databaseId')

# Watch until complete:
gh run watch "$RUN_ID"

# Download the goldens artifact:
gh run download "$RUN_ID" \
  --name corpus-goldens-regen \
  -D mikebom-cli/tests/fixtures/public_corpus/
```

**Note**: the first dispatch may fail Layer 1 for one or more of the 5 non-cobra targets (their assertions were written from spec knowledge in m195, not empirical observation). If so, the artifact is still uploaded (regen mode writes goldens on Layer 1 pass; Layer 1 failures show up in the run log). Iterate:

1. Inspect the emitted-SBOM artifact (`corpus-emitted-sboms`) uploaded on failure to see what mikebom actually emitted.
2. Adjust the failing assertion in `layer1_assertions.rs` per FR-003 (see quickstart Reproducer 4).
3. Push + re-dispatch.
4. Repeat until all 6 targets pass Layer 1 in regen mode.

## Reproducer 4 — Adjust a Layer 1 assertion to match observed output

Given a Layer 1 failure diagnostic like:

```
✗ corpus target FAILED
class:     mikebom-regression
invariant: main-module-purl-present
observed:  no pkg:cargo/ripgrep component
expected:  at least one pkg:cargo/ripgrep@vX.Y.Z component
```

1. Download the failing target's emitted CDX from the `corpus-emitted-sboms` artifact.
2. Query it for the actual main-module PURL:

    ```bash
    jq '.metadata.component | {name, purl}' rust-ripgrep.cdx.json
    ```

3. If the actual is (say) `pkg:generic/rust-ripgrep@0e8390a` (operator-override subject dropping the cargo mainmod per m077 — same shape go-cobra exhibits), update the assertion function to expect the operator-override subject instead:

    ```rust
    // m196: reconciled from cargo-mainmod to operator-override subject.
    // The `--root-name rust-ripgrep` flag in the harness triggers m077,
    // dropping the manifest-derived cargo mainmod.
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:cargo/") || p.starts_with("pkg:generic/rust-ripgrep")) {
        ...
    }
    ```

4. Commit the assertion adjustment.
5. Re-dispatch.

## Reproducer 5 — Verify FR-005 (go-cobra additive-only)

After downloading the artifact into `mikebom-cli/tests/fixtures/public_corpus/`:

```bash
git status mikebom-cli/tests/fixtures/public_corpus/
git diff --stat mikebom-cli/tests/fixtures/public_corpus/go-cobra/
```

**Expected**: `go-cobra/` sub-tree shows zero changes. If it doesn't, DO NOT COMMIT — something drifted between m195's cobra scan and m196's, and needs investigation before proceeding.

## Reproducer 6 — Verify byte-identity (SC-003)

After committing the m196 goldens locally but BEFORE opening the PR, re-dispatch the regen once more (without expectation of new goldens) and verify the artifact byte-identically matches the just-committed files:

```bash
# Re-dispatch, wait, download to a temp:
NEW_RUN_ID=$(...)
gh run download "$NEW_RUN_ID" --name corpus-goldens-regen -D /tmp/regen-verify/

# Compare:
diff -r mikebom-cli/tests/fixtures/public_corpus/ /tmp/regen-verify/
# expected: no output
```

If differences appear, mikebom has a non-determinism issue and m196 shouldn't merge until it's identified and fixed (or the mask helpers extended).

## CI validation recap

Post-merge, the nightly workflow (cron `17 6 * * *`) will run all 6 targets in verify mode. Expected first-run outcome: all 6 report `test ... ok`.
