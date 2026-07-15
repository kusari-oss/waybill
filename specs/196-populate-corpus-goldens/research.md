# Research: Populate Remaining Public-Corpus Goldens

**Date**: 2026-07-14
**Purpose**: Resolve 4 mechanical unknowns before task decomposition: (R1) how to trigger regen on a CI runner, (R2) how to extract the freshly-generated goldens off the runner and into a PR commit, (R3) how to resolve the real postgres:16 Docker Hub digest, (R4) how to discover Layer 1 assertion drift empirically.

## R1 — CI regen dispatch mechanism

**Decision**: Extend `public-corpus.yml`'s `workflow_dispatch` block with a boolean input `regen_goldens` (default `false`). When `true`, the workflow injects `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1` into the env for the "Run public corpus" step. Post-run, the workflow uploads the entire `mikebom-cli/tests/fixtures/public_corpus/` tree as an artifact (regardless of pass/fail — regen mode writes files even when Layer 1 assertions were previously firing, per the m195 harness contract at `layer2_golden.rs::compare_golden`).

**Rationale**:
- Uses the existing `workflow_dispatch` mechanism already added in m195 T042 — no new trigger surface.
- Conditional env injection is idiomatic GitHub Actions (`${{ inputs.regen_goldens == 'true' && '1' || '' }}` pattern) — no shell templating risk (values are booleans, not user-supplied strings).
- Post-run artifact upload uses the same `actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a` SHA-pin that m195 established.
- Alternative (a dedicated `regen-goldens.yml` workflow) rejected: extra workflow file to maintain; the existing workflow already knows how to build + run corpus + upload artifacts.
- Alternative (env-var manually set on the runner via `env:` at job level with a hard-coded `'1'`) rejected: would make regen mode the default, defeating the m195 opt-in gate.

**References**:
- `.github/workflows/public-corpus.yml` (m195 T042 — the workflow this milestone extends).
- `mikebom-cli/tests/corpus_harness_195/harness.rs::update_goldens_gate` — the env var the workflow needs to set.
- m195 memory `feedback_sha_pin_before_dependabot` — action SHA-pin convention.

## R2 — Artifact-to-PR extraction workflow

**Decision**: Maintainer downloads the artifact via `gh run download <run-id> --name corpus-goldens-regen -D mikebom-cli/tests/fixtures/public_corpus/` (which unpacks the artifact into the fixtures directory tree in place), then reviews the resulting diff with `git status` / `git diff --stat`, and commits.

**Rationale**:
- `gh run download` unpacks preserving relative paths — the artifact's internal layout is `<target>/{cdx,spdx-2.3,spdx-3}.json`, matching the target directory shape exactly.
- Maintainer applies human judgment before committing (per FR-008 in m195): are the diffs consistent with the intentional regen? Any surprising diff (e.g., go-cobra's goldens flipped — FR-005 violation) blocks the commit.
- No script needed — `gh` is a standard tool every mikebom maintainer already uses. A `scripts/corpus/download-goldens.sh` wrapper is a nice-to-have follow-up but not required for MVP.
- Alternative (auto-commit from the workflow with a GH-App bot user) rejected: introduces new secrets, new permissions surface, and violates the "human in the loop" invariant that FR-008 encodes.

**References**:
- `gh run download --help` — standard GH CLI subcommand.
- m195 contracts/corpus-harness.md "Refresh Helper Contract" — the human-in-loop principle.

## R3 — Postgres:16 digest resolution

**Decision**: Maintainer runs `docker manifest inspect docker.io/library/postgres:16 --verbose | jq -r '.[].Descriptor.digest' | head -1` locally (or in any environment with Docker Hub access) and pastes the resulting `sha256:<64-hex>` into the manifest at `TARGETS[5].pinned::PinnedRef::Digest.algo_hex`.

**Rationale**:
- Postgres:16 is a multi-arch manifest list; `docker manifest inspect --verbose` returns per-platform digests. For a Linux amd64 runner (ubuntu-latest), select the `linux/amd64` platform's digest specifically — otherwise the mikebom scan pulls a different arch and the goldens won't reproduce on the amd64 runner.
- The specific jq filter needs refinement for multi-arch: `docker manifest inspect --verbose docker.io/library/postgres:16 | jq -r '.[] | select(.Descriptor.platform.architecture == "amd64" and .Descriptor.platform.os == "linux") | .Descriptor.digest' | head -1`.
- Alternative (pin the multi-arch top-level manifest digest, let Docker's per-platform selection happen at pull time) rejected: the top-level digest points at the manifest list, not a specific image. Docker's per-runner platform selection would happen at pull time, meaning the golden would depend on which platform pulled it. Pinning the amd64-specific descriptor digest guarantees reproducibility.

**Sub-decision — where the resolution happens**: local maintainer machine, one-shot. Not scripted into the harness; not automated. This is a one-time human decision (which arch to pin) applied via a manual manifest.rs edit.

**References**:
- Docker manifest inspect documentation.
- OCI image manifest list spec (multi-arch containers).

## R4 — Layer 1 assertion drift discovery

**Decision**: Once R1's CI regen has run, the maintainer downloads the emitted-SBOM artifact (m195 already publishes this on failure — extend to publish on success too when regen mode is on), inspects each of the 5 non-cobra target's emitted CDX + SPDX 2.3 + SPDX 3, and cross-references against the assertions in `layer1_assertions.rs`. Any mismatch is either (a) fixed in the assertion (per FR-003) or (b) flagged for review if the mismatch suggests a genuine mikebom regression the assertion is correctly catching.

**Rationale**:
- The m195 harness at `harness.rs::scan_target` already persists emitted SBOMs to `~/.cache/mikebom/corpus/*/emitted/` — extending the workflow's artifact-upload to include this directory (already done in m195 T042 for failure case) gives the maintainer full visibility.
- On the CI runner, the maintainer sees:
  - What PURL prefixes each target actually emits.
  - What graph-completeness value each target reports.
  - What edges the mainmod (or operator-override root) has.
  - Whether the assertion function's expectations align.
- Adjustments are per-assertion, one function at a time. Each adjustment carries a doc-comment referencing the observed value (per FR-003).
- Alternative (write assertions purely from the emitted CDX post-facto, ignoring the m195 seed) rejected: loses the class-of-bug tripwire signal. The m195 seed encodes "what the target SHOULD emit to prove m194-class regressions are still caught"; drift adjustments should preserve that intent, not discard it.

**References**:
- m195 research §R8 (invariant-seeding rule).
- `mikebom-cli/tests/corpus_harness_195/harness.rs::scan_target` — where emitted SBOMs persist.

## R5 — Additive-only invariant (FR-005)

**Decision**: Before committing any new goldens, the maintainer verifies that the `go-cobra/{cdx,spdx-2.3,spdx-3}.json` files remain byte-identical to their m195-committed versions. Verification is `git diff --stat mikebom-cli/tests/fixtures/public_corpus/go-cobra/` — MUST show zero lines changed. If it doesn't, the regen batch is discarded and re-investigated (something in the mikebom code or the harness has drifted; that's a separate concern from m196's scope).

**Rationale**:
- FR-005 explicitly guards against accidental go-cobra regen. This gate protects it.
- The regen mechanism from m195 (`MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1`) writes goldens for EVERY passing Layer 1 target — including cobra. If cobra's assertions have drifted for any reason since m195 merged, we'd see a change.
- Alternative (fine-grained regen — only regen the 5 non-cobra targets) rejected: adds cli-filter complexity and creates two code paths ("regen this specific target" vs "regen all"). Simpler to regen all and gate at commit-time via git diff.

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| CI regen trigger | Extend existing `workflow_dispatch` with boolean input | Dedicated regen workflow file | Reuses m195 workflow; no new trigger surface |
| Artifact→PR flow | `gh run download` + local git commit | Auto-commit via GH-App bot | Human-in-loop preserved (FR-008 spirit) |
| Postgres digest | Local `docker manifest inspect --verbose` + amd64 filter | Pin top-level multi-arch manifest | Reproducibility on the amd64 runner |
| Assertion drift discovery | Emitted-SBOM artifact inspection + per-assertion review | Post-facto assertion-from-output regen | Preserves m195 R8 class-of-bug tripwire intent |
| FR-005 protection | Post-regen `git diff` gate on go-cobra fixtures | Fine-grained regen filter | Simpler; catches drift explicitly |
| New Cargo deps | Zero | (n/a) | Nothing to add |
| New workflows | Zero | (n/a) | Modify m195's workflow in-place |
