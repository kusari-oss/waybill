# Contract: workflow-step shape for provenance emission + self-verify

**Feature**: `668-slsa-provenance` | **Applies to**: `.github/workflows/release.yml` + `.github/workflows/nightly.yml`

Every provenance-emission site in the release + nightly workflows MUST conform to these contracts. Each contract is verifiable via YAML inspection at review time and via CI enforcement at runtime.

## Interface

### Input (per emission site)

- **Artifact path**: absolute or workspace-relative path to a file that exists at the moment the emission step runs.
- **Job-scope permissions**: `id-token: write`, `attestations: write`, `contents: read` at minimum.
- **Repository context**: emission MUST run inside a GitHub Actions job attached to the `kusari-oss/waybill` repo (not a fork's PR context).

### Output

- **Attestation bundle** uploaded to `https://api.github.com/repos/kusari-oss/waybill/attestations`.
- **Bundle mirror** in Sigstore Rekor at `rekor.sigstore.dev`.
- **Self-verify result** printed in the workflow log — exit code 0 iff verification succeeded.

## Behavioral contracts

### C-1: SHA-pinned action reference

Every `attest-build-provenance` step MUST reference the action by its full commit SHA, NOT a tag. Waybill's dependabot config keeps SHAs current.

**Wrong**:
```yaml
- uses: actions/attest-build-provenance@v3
```

**Right**:
```yaml
- uses: actions/attest-build-provenance@<40-char-SHA>  # v3.0.0
```

The version comment after the SHA is documentation only; enforcement is on the SHA itself. Verified by Kusari Inspector's mutable-tag scanner + the pre-existing SHA-pin audit script.

### C-2: One emission step per subject; 6 subjects total per release

The release workflow MUST have EXACTLY 6 emission sites — one per subject enumerated in FR-001/FR-002/FR-003:

| Subject | Emission site (job) | Artifact path |
|---|---|---|
| linux-x86_64 tarball | `build-linux-x86_64` | `${{ env.TARBALL_PATH }}` (per-job) |
| linux-aarch64 tarball | `build-linux-aarch64` | `${{ env.TARBALL_PATH }}` |
| macos-aarch64 tarball | `build-macos-aarch64` | `${{ env.TARBALL_PATH }}` |
| windows-x86_64 tarball | `build-windows-x86_64` | `${{ env.TARBALL_PATH }}` |
| Multi-arch OCI image | `publish-container-image` | via `subject-digest` from `docker/build-push-action` output |
| Source SBOM sidecar | `release` (final job) | `${{ env.SOURCE_SBOM_PATH }}` |

Nightly workflow MUST have EXACTLY 5 emission sites (same shape minus source SBOM if the nightly doesn't emit one; verify at task time from `nightly.yml`).

### C-3: Fail-closed emission

Every emission step MUST NOT set `continue-on-error: true`. Emission failure fails the job; job failure fails the release (FR-008).

**Wrong**:
```yaml
- uses: actions/attest-build-provenance@<SHA>
  continue-on-error: true  # ❌ violates FR-008
```

**Right**:
```yaml
- uses: actions/attest-build-provenance@<SHA>
  # No continue-on-error; default is `false`.
```

### C-4: Self-verify immediately follows emission (FR-015)

Every emission step MUST be followed (in the same job, in order) by a `gh attestation verify` step against the same artifact. No other steps may run between them.

```yaml
- name: Emit SLSA provenance for tarball
  uses: actions/attest-build-provenance@<SHA>
  with:
    subject-path: ${{ env.TARBALL_PATH }}

- name: Self-verify tarball provenance (FR-015)
  env:
    GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
    ARTIFACT_PATH: ${{ env.TARBALL_PATH }}
  run: |
    gh attestation verify "$ARTIFACT_PATH" --repo ${{ github.repository }}
```

Rationale: interposing steps (esp. artifact-upload steps) risks a race where the emission is queued but Rekor hasn't yet accepted it. Ordering pairs the two steps as a unit.

### C-5: Fail-closed self-verify

Every self-verify step MUST NOT set `continue-on-error: true`. Verify failure fails the job; job failure fails the release (FR-015 hard gate).

### C-6: No shell-injection into `env:` context expressions

Per waybill's existing Kusari Inspector requirement (established at `release.yml:571-574`): NEVER interpolate `${{ ... }}` context expressions directly into `run:` shell blocks. Bind to `env:` variables first. Applied to both the emission and verify steps' `run:` blocks.

### C-7: Non-release workflows emit no provenance

Only `release.yml` and `nightly.yml` emit provenance. Every other workflow file (`ci.yml`, `ebpf-canary.yml`, `public-corpus.yml`, `test-signing.yml`) MUST NOT reference `actions/attest-build-provenance`. This is verified at review time by a simple grep in the pre-PR gate.

### C-8: Coexistence with cosign-keyless — both signature paths preserved

The pre-existing cosign-keyless signature steps at `release.yml:701` (source SBOM) and `release.yml:585-616` (OCI image) MUST NOT be removed or modified by this feature. Both signature paths run in the same job as their SLSA provenance emission; both must succeed for the job to succeed.

## Job-graph diff summary

**Release workflow — required changes per job**:

| Job | Add permissions? | Add emit step | Add verify step |
|---|---|---|---|
| `build-ebpf` | No (not shipping a subject) | No | No |
| `build-linux-x86_64` | Yes (id-token: write, attestations: write) | Yes (post-tarball-build) | Yes (post-emit) |
| `build-linux-aarch64` | Yes | Yes | Yes |
| `build-macos-aarch64` | Yes | Yes | Yes |
| `build-windows-x86_64` | Yes | Yes | Yes |
| `publish-container-image` | Yes (already has id-token: write for cosign) | Yes (post-docker-push) | Yes |
| `release` | Yes (post-source-SBOM-generation) | Yes | Yes |

**Nightly workflow**: same shape as release minus any jobs that don't ship a subject in the nightly cadence (verify at task time).

## Non-contracts

- The GitHub action's INTERNAL implementation is not a waybill contract — we consume the output only.
- The Sigstore Fulcio CA and Rekor mirror URLs are GitHub's operational concern.
- Bundle format version (v0.3 today) will bump as Sigstore evolves; waybill inherits transparently.

## Test-authoring rules

- Every new emit step gets a corresponding review comment in the PR: "Where's the self-verify step for this subject?" — enforced by contract-review.
- YAML syntax validation via GitHub's workflow parser is part of the pre-merge CI (already runs).
- Post-merge acceptance test: fire a manual dispatch of the release workflow against a test tag; inspect the 6 uploaded attestations for shape.
