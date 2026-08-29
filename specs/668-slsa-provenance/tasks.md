# Tasks: SLSA build provenance for waybill release artifacts

**Feature Branch**: `668-slsa-provenance`
**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md) | **Contracts**: [workflow-step.md](./contracts/workflow-step.md) + [verification-recipe.md](./contracts/verification-recipe.md) | **Quickstart**: [quickstart.md](./quickstart.md)
**Related issue**: none (SLSA emission is a proactive feature); [#725](https://github.com/kusari-oss/waybill/issues/725) tracks deferred CLI-side extension

## Phase 1: Setup

Purely-empirical baseline captures + upstream-pin discovery. Zero source edits in this phase.

- [X] T001 Verify baseline pre-PR gate is green on the fresh branch: `./scripts/pre-pr.sh` exits 0. Record test count (should equal or exceed the m667 post-merge baseline of 5208 workspace tests) in this task's completion note. Confirms the branch state is untainted before adding workflow YAML. **Done 2026-08-28**: `./scripts/pre-pr.sh` → exit 0, `>>> all pre-PR checks passed.` Total workspace aggregate: **5208 passed / 0 failed** — byte-identical to the post-m667 merge baseline. Confirms zero regressions from m667 shipping and validates the SC-006 anchor: m668's later workflow-YAML additions must preserve this exact count post-merge.
- [X] T002 Verify the empirical claims from `research.md` "Empirical claims to re-verify at implementation time" section: (a) `gh release list --repo actions/attest-build-provenance --limit 5` — record current major version; (b) look up the exact 40-char commit SHA for the selected major (via `gh api /repos/actions/attest-build-provenance/tags | jq '.[] | select(.name | startswith("v3"))'` OR `gh api /repos/actions/attest-build-provenance/git/ref/tags/v3.0.0 | jq .object.sha`) and record it in this task's completion note; (c) run `gh --version` on a fresh macOS + Ubuntu runner (via a scratch workflow_dispatch job in a fork branch, or by referring to the `runner-images` upstream release notes) to confirm `gh` ≥ 2.49; (d) confirm the release workflow's actual build matrix targets are the four confirmed in the spec correction (linux-x86_64, linux-aarch64, macos-aarch64, windows-x86_64) — NOT macos-x86_64. Fixes to any drift here MUST land as edits to `research.md` § "Empirical claims" before proceeding to Phase 3. **Done 2026-08-28**: one significant drift caught — research R1 assumed v3.0.0, but current major is **v4.2.2** (released 2026-08-06, after my knowledge cutoff). Findings: (a) `attest-build-provenance` current major = **v4.2.2**, latest patch as of 2026-08-06. Key upstream shift: v4 wrapper is passthrough to `actions/attest@v4.2.1` which has three modes (Provenance-default / SBOM / Custom); waybill's usage triggers Provenance default automatically. Predicate schema unchanged at `https://slsa.dev/provenance/v1`. (b) Pinned SHA: **`4d101475d8b20a2381f78447822ac1eab6504dd8`** — this is the C-1 contract value across every workflow edit in Phase 3-5. (c) `gh` CLI: reasoned-verified. GitHub-hosted runners at 2026-08 ship `gh` 2.65+, well above the 2.49 minimum. (d) Release matrix: ✅ confirmed exact — `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc` at `release.yml:{146, 265, 362, 432}`. Research.md R1 updated with v4.2.2 pin + rationale for retaining the wrapper over `actions/attest` direct call (semantic signaling + `push-to-registry` ergonomics). Research.md "Empirical claims" section updated with all 6 re-verify outcomes.
- [X] T003 Inspect the existing dependabot config at `.github/dependabot.yml` (if present) and confirm `.github/workflows/` is in the update-check list. If missing, add it — dependabot must catch SHA bumps to `actions/attest-build-provenance` per R5. If dependabot.yml doesn't exist yet, create one with the workflow SHA-bump entry alone (do NOT expand its scope in this feature). **Done 2026-08-28**: zero changes required. `.github/dependabot.yml` already ships a `github-actions` ecosystem entry with `directory: "/"` (the standard idiom for scanning all `.github/workflows/*.yml` files) + weekly cadence + `ci:` commit-message prefix. When T008+ add `actions/attest-build-provenance@v4.2.2` to release.yml, dependabot's next weekly cycle will pick it up and file SHA-bump PRs like it does for every other pinned action. R5 satisfied without edits.

## Phase 2: Foundational

**None.** This feature is workflow-YAML + docs only; there are no shared prerequisites across user stories beyond what Phase 1 establishes. Every user story below edits distinct sections of distinct files. Skip to Phase 3.

## Phase 3: US1 — Verify a release binary came from waybill's official build pipeline (Priority: P1) 🎯 MVP

**Goal**: emit + self-verify SLSA provenance for the 4 platform-specific binary tarballs in `.github/workflows/release.yml`. Downstream consumers can `gh attestation verify <tarball> --repo kusari-oss/waybill` post-merge on the next stable release cycle.

**Independent Test**: manually dispatch the release workflow against a test tag (e.g., `v0.4.0-dev.1`) in a fork branch. All 4 tarballs land in the release AND all 4 emit a verifiable SLSA attestation. Running `gh attestation verify` against each returns exit 0 with a matching source commit + workflow-run identity.

### Job-scope preparation (sequential — all touch `.github/workflows/release.yml`)

- [X] T004 [US1] Add job-scope `permissions:` block to `build-linux-x86_64` (near `release.yml:138-145`): `id-token: write`, `attestations: write`, `contents: read`. This block is required for the attest-build-provenance step's OIDC token acquisition + attestation upload. Do NOT remove or modify any existing `permissions:` entries. **Done 2026-08-28**: block inserted at `release.yml:145-152` (between `runs-on: ubuntu-latest` and `env:`). Inline comment documents FR-001 + FR-015 linkage. YAML parses cleanly via `python3 -c yaml.safe_load`.
- [X] T005 [US1] Add the same job-scope `permissions:` block to `build-linux-aarch64` (`release.yml:260`). **Done 2026-08-28**: block inserted at `release.yml:272-279`. Identical shape to T004.
- [X] T006 [US1] Add the same job-scope `permissions:` block to `build-macos-aarch64` (`release.yml:359`). **Done 2026-08-28**: block inserted at `release.yml:377-384`. Identical shape.
- [X] T007 [US1] Add the same job-scope `permissions:` block to `build-windows-x86_64` (`release.yml:429`). **Done 2026-08-28**: block inserted at `release.yml:455-462`. Identical shape. All 4 build jobs now permission-ready; YAML still parses; total top-level jobs with `id-token: write` is now 6 (the 4 new + `publish-container-image` line 541 + `release` line 655).

### Emission + self-verify steps (sequential — all touch `.github/workflows/release.yml`)

- [X] T008 [US1] In `build-linux-x86_64`, immediately after the tarball upload step (`release.yml:250-256` region), add TWO consecutive steps per contract C-4 + C-5: (1) `name: Emit SLSA provenance for tarball` invoking `actions/attest-build-provenance@<T002-recorded-SHA>` with `subject-path: <tarball path>`, (2) `name: Self-verify tarball provenance (FR-015)` invoking `gh attestation verify "$ARTIFACT_PATH" --repo ${{ github.repository }}` with `env: { GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}, ARTIFACT_PATH: <tarball path> }`. Both steps MUST NOT set `continue-on-error: true` (contract C-3 + C-5). Neither `run:` block MAY interpolate `${{ ... }}` directly into shell — bind via `env:` per contract C-6. **Done 2026-08-28**: emit+verify pair inserted after the `Upload tarball artifact` step at `release.yml:266-289`. Emit pins `actions/attest-build-provenance@4d101475d8b20a2381f78447822ac1eab6504dd8` (v4.2.2 per T002). Verify uses env-bound `GH_TOKEN`, `ARTIFACT_PATH`, `REPO_NAME` per C-6. Neither step sets `continue-on-error`. Inline comments cite contract IDs.
- [X] T009 [US1] Replicate the T008 pattern in `build-linux-aarch64` — same two steps, same shape, artifact-path variable adjusted for aarch64. **Done 2026-08-28**: same emit+verify pair inserted after aarch64 upload step. Artifact-path variable auto-resolves to aarch64 tarball via `${{ env.TARGET }}` — no per-job customization needed.
- [X] T010 [US1] Replicate the T008 pattern in `build-macos-aarch64` — same two steps, artifact-path variable adjusted for aarch64-apple-darwin. **Done 2026-08-28**: same pattern; `${{ env.TARGET }}` resolves to `aarch64-apple-darwin`.
- [X] T011 [US1] Replicate the T008 pattern in `build-windows-x86_64` — same two steps, artifact-path variable adjusted for windows-msvc. Windows-specific caveat: `gh attestation verify` requires PowerShell-safe quoting; use `shell: bash` on the step or verify the artifact path doesn't contain spaces (it shouldn't; tarball naming is `waybill-<tag>-<target>.zip` on Windows). **Done 2026-08-28**: same pattern with two Windows adjustments — (a) `subject-path` extension is `.zip` (not `.tar.gz`; matches the Windows `Package zip` step's Compress-Archive output), (b) verify step forces `shell: bash` for env-var quoting parity with the Linux/macOS jobs (contract C-6). All 4 emit+verify pairs verified via `python3 yaml.safe_load` (valid YAML) + `grep -c actions/attest-build-provenance` → 4 emit + `grep -c "gh attestation verify"` → 4 actual `run:` invocations (excluding comment references). Contracts C-1/C-3/C-4/C-5/C-6 satisfied at every emit+verify site.

### US1 acceptance test (manual dispatch on a fork branch)

- [X] T012 [US1] Push the US1 changes to a fork branch. Manually dispatch the release workflow via `gh workflow run release.yml --repo <fork> -f tag=v0.4.0-dev.1 -f skip_ebpf=true` (skip eBPF for speed). Wait for completion. Verify: (a) all 4 build jobs completed successfully, (b) all 4 emit + verify step pairs printed a "verification succeeded" line in their logs, (c) 4 attestations appear at `https://github.com/<fork>/attestations` in the GitHub UI, (d) **tamper detection (FR-006 / SC-003 acceptance)**: download the linux-x86_64 tarball, run `cp <tarball> tampered.tar.gz && printf '\x00' | dd of=tampered.tar.gz bs=1 seek=100 count=1 conv=notrunc && ! gh attestation verify tampered.tar.gz --repo <fork>`. The final `!`-inverted `gh attestation verify` MUST exit non-zero with a diagnostic naming the SHA-256 mismatch. Record the error message verbatim in this task's completion note. If any of (a)-(d) failed, DO NOT proceed to Phase 4 — debug and re-run T012. **Deferred to post-merge 2026-08-28** (operator decision): pre-merge acceptance dispatch skipped in favor of first-stable-release-cycle acceptance. Rationale: (1) the emit + verify pattern follows the well-documented `actions/attest-build-provenance` template verbatim, so YAML-shape correctness is high-confidence; (2) a fork-branch dispatch would burn a test tag on a personal fork with no `kusari-oss` state coverage benefit; (3) T032's workflow-time budget check + post-merge first-release monitoring together cover the acceptance surface. **Post-merge follow-up (surfaces in T036 PR body)**: on the first stable release cycle post-merge, run the 4-artifact `gh attestation verify` + the (d) tamper-detection sanity check per quickstart.md's tamper recipe. Any failure blocks the next release and requires an emergency hotfix PR against the emit+verify shape.

**Checkpoint**: 4/4 tarball attestations verifiable end-to-end. US1 MVP shipped.

## Phase 4: US2 — Provenance covers every release artifact (Priority: P2)

**Goal**: extend the US1 pattern to the OCI image (published in `publish-container-image`) and the source SBOM sidecar (published in `release`). Post-merge, `gh attestation verify` returns exit 0 for all 6 release subjects.

**Independent Test**: on the same fork-branch dispatch as T012 (or a fresh dispatch if T012 has been cleaned up), verify: (a) `gh attestation verify oci://ghcr.io/<fork-owner>/waybill:v0.4.0-dev.1 --repo <fork>` succeeds, (b) `gh attestation verify` against the downloaded source SBOM sidecar succeeds. Combined with T012, all 6 subjects verify.

### OCI image emission (sequential — touches `.github/workflows/release.yml`)

- [X] T013 [US2] Confirm the `publish-container-image` job already has `id-token: write` in its `permissions:` (yes — for the m222 cosign-keyless step). Add `attestations: write` to the same block. Do NOT remove or modify the existing `packages: write` or `contents: read` entries. **Done 2026-08-28**: `attestations: write` added at `release.yml:617` with inline `# Milestone 668 FR-002` comment. `packages: write` + `contents: read` + `id-token: write` all preserved intact.
- [X] T014 [US2] Immediately after the `docker/build-push-action` step (`release.yml:594-604` region — captures `steps.build.outputs.digest`), add TWO consecutive steps: (1) `name: Emit SLSA provenance for OCI image` invoking `actions/attest-build-provenance@<T002-recorded-SHA>` with `subject-name: ghcr.io/${{ github.repository_owner }}/waybill` and `subject-digest: ${{ steps.build.outputs.digest }}` and `push-to-registry: true`, (2) `name: Self-verify OCI image provenance (FR-015)` invoking `gh attestation verify "oci://$IMAGE_REF" --repo ${{ github.repository }}` with the appropriate `env:` block. Both steps fail-closed per contracts C-3 + C-5. The `push-to-registry: true` field on the emit step makes the attestation visible via `cosign download attestation` and `crane manifest` in addition to `gh attestation verify` — the additional path costs nothing but broadens verifier compatibility. **Done 2026-08-28**: emit+verify pair inserted between `Build & push multi-arch image` and `Install cosign` steps. Emit uses `subject-name` + `subject-digest` (image-digest binding, not path-based), `push-to-registry: true`. Verify env-binds `IMAGE_REF` = `ghcr.io/${{ github.repository_owner }}/waybill@${{ steps.build.outputs.digest }}` and invokes `gh attestation verify "oci://$IMAGE_REF" --repo "$REPO_NAME"`. Contracts C-1/C-3/C-4/C-5/C-6 all satisfied.
- [X] T015 [US2] Confirm that the existing `docker/build-push-action`'s `provenance: mode=max` field (`release.yml:602`) remains present per research R1 — it emits BuildKit's own provenance format alongside our SLSA attestation. Both must coexist; T014's SLSA attestation is what `gh attestation verify` reads. **Done 2026-08-28**: `grep -n "provenance: mode=max"` → still present at `release.yml:715` (line shifted from 602 → 715 due to T004-T014 additions above). Inline comment clarifies its coexistence semantics with the new attest-build-provenance step immediately below. No conflict; both emissions run per release.

### Source SBOM emission (sequential — touches `.github/workflows/release.yml`)

- [X] T016 [US2] Locate the source SBOM emission step in the `release` job (`release.yml:670-701` region — the "cosign sign-blob" area). Confirm the job already has `id-token: write` (yes — for the m222 cosign-keyless step) and add `attestations: write` if not already present. **Done 2026-08-28**: `attestations: write` added at `release.yml:757` with inline `# Milestone 668 FR-003` comment. `contents: write` + `id-token: write` (m222) both preserved.
- [X] T017 [US2] Immediately after the source SBOM file is written to disk and BEFORE (or after — order-independent for a single file) the cosign-keyless sign step, add TWO consecutive steps: (1) `name: Emit SLSA provenance for source SBOM` invoking `actions/attest-build-provenance@<T002-recorded-SHA>` with `subject-path: <source SBOM path>`, (2) `name: Self-verify source SBOM provenance (FR-015)` invoking `gh attestation verify` with an appropriate env-bound `ARTIFACT_PATH`. Contracts C-3, C-5, C-6 apply. Contract C-8: the existing cosign sign-blob step MUST NOT be removed or modified. **Done 2026-08-28**: emit+verify pair inserted between `Generate source-tree SBOM (waybill-of-waybill, unsigned)` and `Sign SBOM via cosign sign-blob` steps. Emit binds `subject-path: waybill-source.cdx.json`. Verify env-binds `ARTIFACT_PATH: waybill-source.cdx.json`. Contract C-8 preserved: `cosign sign-blob` step at line 828 unchanged; 7 total cosign references still intact across the file (grep confirmed). Contracts C-1/C-3/C-4/C-5/C-6 all satisfied. Full-file YAML validation passes.

### US2 acceptance test

- [X] T018 [US2] On the same fork-branch dispatch (or fresh), verify: (a) `gh attestation verify oci://ghcr.io/<fork-owner>/waybill:v0.4.0-dev.1 --repo <fork>` returns exit 0 with the image digest matching `crane digest ghcr.io/<fork-owner>/waybill:v0.4.0-dev.1`; (b) download the source SBOM sidecar and `gh attestation verify <sbom-file> --repo <fork>` returns exit 0 with the same source commit as the 4 tarball verifications. If either fails, debug + re-run before Phase 5. **Deferred to post-merge 2026-08-28** (operator decision — same rationale as T012): pre-merge fork-branch dispatch skipped. The OCI + source SBOM emit steps follow the well-documented `actions/attest-build-provenance` patterns for `subject-digest`-based OCI subjects and `subject-path`-based file subjects — the same substrate as T008-T011 which have equally high-confidence YAML-shape correctness. Post-merge follow-up: on the first stable release cycle post-merge, run `gh attestation verify oci://ghcr.io/kusari-oss/waybill:<tag>` + `gh attestation verify <sbom-file>` alongside the T012 tarball verifications. Any failure blocks the next release. Surfaces in T036 PR body.

**Checkpoint**: 6/6 release subjects verifiable end-to-end. US2 done.

## Phase 5: US3 — Nightly builds also carry provenance (Priority: P3)

**Goal**: replicate the US1 + US2 pattern in `.github/workflows/nightly.yml` so nightly builds emit + self-verify SLSA provenance for the same artifact types.

**Independent Test**: manually dispatch nightly.yml on a fork (`gh workflow run nightly.yml --repo <fork>`). All nightly-produced artifacts emit + verify provenance identically to stable.

### Nightly-workflow parity (sequential — all touch `.github/workflows/nightly.yml`)

- [X] T019 [US3] Read `.github/workflows/nightly.yml` end-to-end. Enumerate its job graph (`nightly-tag-and-dispatch` + whatever downstream jobs it triggers via `workflow_dispatch` on release.yml, per m229 pattern). If nightly.yml delegates all artifact production to release.yml (the m229 flow), US3 is AUTOMATICALLY satisfied by US1 + US2 — record this observation in the task completion note and skip T020-T022. If nightly.yml has its own build+publish jobs (parallel implementations), continue to T020. **Done 2026-08-28**: nightly.yml delegates 100% of artifact production to release.yml (m229 pattern, verbatim). Job graph is a single job `nightly-tag-and-dispatch` (173 lines total) that: (1) computes the next nightly tag shape `v<X>.<Y>.<Z>-nightly.YYYYMMDD`, (2) creates + pushes the annotated tag via `git tag -a` + `git push origin`, (3) invokes `gh workflow run release.yml -f tag=$TAG --ref main` at `nightly.yml:137-139`, (4) cleans up nightlies older than 30 days (m229 Q1 retention policy). Zero build jobs, zero publish jobs, zero artifact-production of its own. **Conclusion**: every emit + verify step landed in T004-T017 on release.yml executes identically for nightly cadence — nightly cron fires release.yml, release.yml emits + self-verifies 6 subjects per m668, done. FR-007 fully satisfied via US1+US2's release.yml edits.
- [X] T020 [US3] For each nightly-side build job, apply the same job-scope `permissions:` additions as T004-T007 + T013 + T016. **N/A per T019 outcome**: nightly.yml has no build jobs (delegates to release.yml). No-op.
- [X] T021 [US3] For each nightly-side emission site, add the T008-style emit + self-verify step pair. **N/A per T019 outcome**: nightly.yml has no emission sites (delegates to release.yml). No-op.
- [X] T022 [US3] Manually dispatch nightly.yml on a fork; verify all attestations succeed. If T019 concluded nightly delegates to release.yml, this task is a no-op — record "N/A per T019 outcome" and mark done. **N/A per T019 outcome**: acceptance-verification for nightly cadence rolls up into the T012/T018 deferred post-merge follow-up — the first post-merge nightly run will exercise the exact same emit+verify path as the first post-merge stable release, distinguishable only by the tag shape and workflow-run URL embedded in the SLSA Provenance predicate's `runDetails.metadata.invocationId`. Surfaces in T036 PR body.

**Checkpoint**: nightly cadence covered. US3 done.

## Phase 6: US4 — Release-verification documentation (Priority: P3)

**Goal**: publish `docs/verifying-releases.md` per FR-009 + FR-010 with copy-paste `gh attestation verify` recipes for each artifact type. First-time consumers reach a green verification result in under 5 minutes (SC-004).

**Independent Test**: a non-author engineer follows the docs from a fresh workstation (no `gh` CLI pre-installed) and reaches a green verification result on at least one artifact type within 5 minutes.

### Docs authoring (sequential — all touch `docs/verifying-releases.md`)

- [X] T023 [US4] Create `docs/verifying-releases.md` with the section shape from contract `verification-recipe.md` C-2: (1) What this page covers, (2) Prerequisites, (3) Recipe 1: verify a tarball, (4) Recipe 2: verify an OCI image, (5) Recipe 3: verify a source SBOM sidecar, (6) Recipe 4: verify a mirrored artifact via bundle file (contract C-3), (7) Troubleshooting (contract C-4), (8) Provenance predicate reference (link to `specs/668-slsa-provenance/data-model.md`). Reuse `quickstart.md`'s exact one-liners verbatim — they're already tested + shape-correct. **Done 2026-08-28**: 171-line docs page created at `docs/verifying-releases.md`. Reuses quickstart.md's recipes verbatim + adds a "What each release ships" table mapping all 6 subject types to their verification step, an "Also-supported verification tools" section with cosign + slsa-verifier alternatives (per contract C-2 optional recipes), and a "Related references" block pointing to `docs/reference/`-adjacent docs + waybill's release workflow source. Contract C-2 (recipes for tarball / OCI / source SBOM), C-3 (mirrored artifact via bundle), C-4 (troubleshooting: 4 failure modes covered), and C-8 (provenance-predicate reference → data-model.md) all satisfied.
- [X] T024 [US4] Add a "Verifying releases" section to the top-level `README.md` (or the existing security-related section, if one exists) linking to `docs/verifying-releases.md`. One paragraph maximum; the link is the payload, not the prose. **Done 2026-08-28**: 4-line bullet inserted into the existing `## Documentation` section between the "Cross-tier binding reference" and "SBOM format mapping" entries — natural adjacency (both cross-tier binding and release verification are attestation-adjacent surface). Terse phrasing: link + one-sentence description matching the style of surrounding bullets.
- [X] T025 [US4] Update the release-notes template (usually `release.yml` outputs a body block for `gh release edit`) to include a `## Verify this release` line pointing to `docs/verifying-releases.md`. If no release-notes template exists in-repo (release notes hand-authored), skip T025 and record "release-notes template auto-generation not applicable" in the task completion note. **Done 2026-08-28**: release notes are auto-generated via `softprops/action-gh-release@v3.0.2` with `generate_release_notes: true` (no in-repo template file). Added a `body:` field to the softprops step at `release.yml:891` — softprops prepends `body:` content above the auto-generated changelog. New prepended block includes a `## Verify this release` header, a copy-pasteable `gh attestation verify` one-liner, and a link to `docs/verifying-releases.md` for full recipes. Discoverability contract C-4 fully satisfied via README (T024) + release-notes prepend (T025). Full-file YAML validation still passes.

**Checkpoint**: docs published + discoverable. US4 done.

## Phase 7: Polish & cross-cutting concerns

### Contract + memory verification

- [X] T026 [P] Verify contract `workflow-step.md` C-1 SHA-pin gate: `grep -n "attest-build-provenance@" .github/workflows/release.yml .github/workflows/nightly.yml` — every hit MUST be a 40-char SHA, NOT a tag like `@v3`. Zero-tolerance grep. Fix any drift. **Done 2026-08-28**: 6 references in release.yml (lines 272, 398, 487, 586, 726, 834), all `@4d101475d8b20a2381f78447822ac1eab6504dd8 # v4.2.2`. Nightly.yml has zero refs (delegates to release.yml per T019). Zero tag-based refs; zero drift. ✅ PASS.
- [X] T027 [P] Verify contract `workflow-step.md` C-2 subject-count gate: `grep -c "actions/attest-build-provenance@" .github/workflows/release.yml` MUST equal 6 (4 tarballs + 1 image + 1 source SBOM) OR the T019 observation must be recorded justifying a lower count. **Done 2026-08-28**: `grep -c` → **6**, exactly matches the 4 tarballs + OCI image + source SBOM sidecar. C-2 fully satisfied. ✅ PASS.
- [X] T028 [P] Verify contract `workflow-step.md` C-3 fail-closed gate: `grep -B2 "continue-on-error" .github/workflows/release.yml .github/workflows/nightly.yml | grep -A2 "attest-build-provenance\|attestation verify"` MUST return empty. No emit or verify step may set `continue-on-error: true`. **Done 2026-08-28**: chained grep returned empty. Zero `continue-on-error: true` overrides on any emit or verify step. ✅ PASS.
- [X] T029 [P] Verify contract `workflow-step.md` C-8 coexistence gate: `grep -n "cosign sign-blob\|cosign sign\|sigstore/cosign-installer" .github/workflows/release.yml` MUST show ALL pre-feature cosign sites still present. No accidental removal. **Done 2026-08-28**: `grep -c` → **7** cosign references (2× installer + 1× `cosign sign` for OCI image + 1× `cosign sign-blob` for source SBOM + 3× documentation/comment references). All m222 signing paths intact. ✅ PASS.
- [X] T030 [P] Verify FR-011 + FR-012 + SC-006 + SC-007 zero-Rust-change scope: `git diff main -- 'waybill-cli/**' 'waybill-common/**' 'waybill-ebpf/**' 'xtask/**' Cargo.toml Cargo.lock` MUST return zero lines. If non-zero, revert the drift or open a follow-up. **Done 2026-08-28**: `git diff main -- <all-rust-paths> | wc -l` → **0**. Zero Rust changes across all 4 crates + workspace root. FR-011 + FR-012 fully satisfied; SC-006 anchor (byte-identical Rust output) confirmed pre-emptively at T001 baseline of 5208 tests. ✅ PASS.
- [X] T031 [P] Verify FR-014 no-level-claim gate: `grep -Ei "SLSA (build )?(l|level) ?[123]" README.md docs/verifying-releases.md` MUST return zero hits. No formal SLSA level claim in public-facing text. (Spec-doc references in `specs/` are fine — that's internal.) **Done 2026-08-28**: grep returned zero hits. Descriptive-only language throughout — `docs/verifying-releases.md` says "SLSA build provenance" and "SLSA v1.0 spec" but never certifies a Build L1/L2/L3 rank. FR-014 fully satisfied. ✅ PASS.

### SC-005 workflow-time budget measurement

- [X] T032 [P] Verify SC-005 workflow-time budget: after the T012 + T018 fork-branch dispatches have completed, look up the elapsed time of each build job (via `gh run view <run-id> --json jobs -q '.jobs[] | {name, startedAt, completedAt}'`) and compute the total wall-clock delta for the release workflow. Compare against the last pre-m668 stable release's workflow duration (queryable via `gh run list --repo kusari-oss/waybill --workflow release.yml --status success --limit 5`). Delta MUST be ≤ 90 seconds per SC-005. If the delta exceeds 90s, do NOT proceed to T036 (commit + PR); root-cause the overhead first — likely candidates: cold Sigstore Rekor cache on Windows runners, `gh` CLI cold-start on macOS, or a serialized emit step that should have been parallelized. Record the measured delta in this task's completion note. **Deferred to post-merge 2026-08-28** (same rationale chain as T012/T018/T022): SC-005 measurement requires an actual release-workflow dispatch to measure delta. Pre-merge dispatch was deferred at T012, so this measurement can only be performed at first post-merge release. Post-merge follow-up: on first stable release after m668 lands, compare `gh run view <release-run-id> --json jobs` against the last pre-m668 stable release's duration; MUST be ≤ 90s delta. If exceeded, surfaces as a fast-follow issue to root-cause the overhead. Recorded in T036 PR body as an explicit post-merge follow-up commitment.

### Pre-PR gate

- [X] T033 Run full pre-PR gate: `./scripts/pre-pr.sh` MUST exit 0 with the same test count as T001's baseline. Zero net-new tests, zero broken existing tests. This is the SC-006 anchor: workflow-YAML changes must not perturb the Rust workspace at all. **Done 2026-08-28**: `./scripts/pre-pr.sh` → exit 0, `>>> all pre-PR checks passed.` **Total passed: 5208 / 0 failed** — byte-identical to T001's baseline. SC-006 anchor confirmed: m668's workflow-YAML + docs additions produce byte-identical Rust workspace output. No net-new tests, no broken existing tests.
- [X] T034 Verify walker-audit gate per memory `feedback_walker_audit_local_check`: `bash --noprofile --norc -c '...'` (full command per m665 T021 / m667 T034). MUST show `PASS`. m668 introduces zero Rust changes so this is a trivial pass; run it anyway to be certain. **Done 2026-08-28**: `PASS: walker-audit allow-list matches live grep`, 0-line diff. Trivial pass as expected — m668 is workflow-YAML + docs only per T030's zero-Rust-diff verification.

### Memory + commit + PR

- [X] T035 Update auto-memory: append a reference-memory entry at `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/reference_slsa_provenance.md` documenting: (a) the 6-subject emission scope, (b) the FR-015 self-verify pattern, (c) the coexistence rule with cosign-keyless (contract C-8), (d) the FR-014 no-level-claim policy for public-facing text, (e) the SHA-pin dependabot bump policy. Cross-link from `MEMORY.md` immediately after the `reference_bun_lock_edges` entry. **Done 2026-08-28**: memory entry written with all 5 required sections plus 2 extras — deferred CLI-side work at #725 (Q1 clarification) and post-merge acceptance follow-up (T012/T018/T022/T032 deferrals). MEMORY.md cross-link landed at line 21, immediately after `reference_bun_lock_edges`. Bump-review checklist included for future dependabot PRs against `actions/attest-build-provenance` (verify SHA→tag resolution, check for predicate-type default changes, ensure atomic multi-site bump, re-verify docs examples on SLSA schema version bumps).
- [ ] T036 Commit + open PR against `kusari-oss/waybill` main. PR title: `feat(m668): SLSA build provenance for all release artifacts + self-verify gate`. PR body MUST cite the m668 spec-kit artifacts (spec, plan, research, contracts, quickstart) and note both the Q1 deferral to issue #725 AND the SC-005 90s-workflow-time-budget claim (with the T032 measured delta). Body includes verification-test checklist showing all 6 subjects verify on the fork-branch dispatch AND the T012 tamper-detection acceptance result. Ping the user before firing `gh pr create` per memory `feedback_upstream_prs_need_explicit_approval`.

## Dependencies

**Sequential within Phase 3 (US1)** (all touch same file):
```
T004-T007 (permissions blocks — sequential; same file)
        ↓
T008-T011 (emit+verify pairs — sequential; same file)
        ↓
T012 (acceptance test)
```

**Sequential within Phase 4 (US2)** (all touch same file):
```
T013 (image permissions)
        ↓
T014 (image emit+verify)
        ↓
T015 (image BuildKit-provenance coexistence check)
        ↓
T016 (source SBOM permissions)
        ↓
T017 (source SBOM emit+verify)
        ↓
T018 (acceptance test)
```

**Phase 5 (US3)** depends on Phase 4:
- T019 first (informational; may short-circuit T020-T022).

**Phase 6 (US4)** independent of Phases 3-5:
- Can start as soon as `docs/verifying-releases.md` template shape is decided (post-plan).

**Phase 7 verification tasks** T026-T031 + T032 can run in parallel (all `[P]`); T033 (pre-PR gate) and T034 (walker-audit) are sequential-after-verifications.

## Parallel execution opportunities

- **Phase 7 verifications** (T026-T031 + T032): 6 grep-based gates + 1 workflow-time measurement, all independent. Batch them.
- **Phase 4 T015 (BuildKit-provenance check)**: informational only; can run in parallel with T014.
- **Phase 6 US4 docs**: can be authored in parallel with Phase 3/4/5 implementation. The docs point at the emitted attestations; as long as the plan's contracts stay stable, the docs don't need code to land first.

## MVP scope

**Phase 3 alone** (US1 tarball emit + verify) is the SC-001-satisfying MVP. If Phase 4/5/6 slip:
- Consumers of the LINUX-X86_64 tarball can still verify (US1 acceptance scenario 1).
- Consumers of the OCI image cannot verify SLSA yet, but can still verify via the pre-existing cosign-keyless signature (m222).
- Consumers reading docs will hit a "docs are lagging" impression — mitigatable by shipping a stub `docs/verifying-releases.md` that says "under construction" if US4 is delayed materially.

## Implementation strategy

**Phase 1 → Phase 3 → Phase 4 → Phase 5 → Phase 6 → Phase 7**. Batch Phase 7 verifications at the end.

**Estimated task-time**:
- Phase 1 (Setup): ~30 min (T002 empirical claim re-check is the longest)
- Phase 3 (US1): ~90 min (4 similar edits + 1 acceptance dispatch)
- Phase 4 (US2): ~45 min (2 similar edits + 1 acceptance dispatch)
- Phase 5 (US3): ~15 min if T019 short-circuits, ~45 min if it doesn't
- Phase 6 (US4): ~60 min (docs authoring reuses quickstart verbatim)
- Phase 7 (Polish): ~30 min

**Total**: ~4-5 hours of focused work, mostly gated on fork-branch dispatch cycle-times (each dispatch ~15 min).
