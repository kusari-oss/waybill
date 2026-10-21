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
- **Self-verify result** printed in the workflow log — exit code 0 iff verification succeeded. Printing is part of the contract, and the shape that shipped did not satisfy it: `gh attestation verify` writes its human-readable summary only to a terminal, so through the pipe Actions captures step output with, it printed nothing on either stream and still exited 0 (#1240).

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

Nightly workflow MUST have EXACTLY 0 emission sites. Checked after the fact (#1240): `nightly.yml` tags the commit and then dispatches `release.yml` with `gh workflow run release.yml -f tag=<tag> --ref main`, so a nightly's six subjects are emitted by the six sites above, in a run whose `GITHUB_REF` is `refs/heads/main`. Nothing in `nightly.yml` references `attest-build-provenance`.

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

### C-4: Self-verify immediately follows emission, and enforces this run's identity (FR-015)

Every emission step MUST be followed (in the same job, in order) by a verify step against the same subject. No other steps may run between them.

```yaml
- name: Emit SLSA provenance for tarball
  uses: actions/attest-build-provenance@<SHA>
  with:
    subject-path: ${{ env.TARBALL_PATH }}

- name: Self-verify tarball provenance (FR-015)
  uses: ./.github/actions/verify-attestation
  with:
    subject: ${{ env.TARBALL_PATH }}
    token: ${{ secrets.GITHUB_TOKEN }}
```

Rationale for the ordering: interposing steps (esp. artifact-upload steps) risks a race where the emission is queued but Rekor hasn't yet accepted it. Ordering pairs the two steps as a unit.

The verify step MUST go through `./.github/actions/verify-attestation` rather than calling `gh attestation verify` inline, so that the identity it enforces is defined in one place. That identity is, at minimum:

| Enforced | How | Why not weaker |
|---|---|---|
| source repository | `--repo "$GITHUB_REPOSITORY"` | — |
| signer workflow | `--signer-workflow "${GITHUB_WORKFLOW_REF%@*}"` | `--repo` alone validates only SourceRepository and SourceRepositoryOwner, so it is satisfied by provenance for that digest from *any* workflow in the repository (#1240) |
| source ref | `--source-ref "$GITHUB_REF"` | pins the attestation to the tag being released — `refs/tags/v<X>.<Y>.<Z>` on a stable release, `refs/heads/main` for a nightly, which `nightly.yml` dispatches with `--ref main` |
| source commit | `--source-digest "$GITHUB_SHA"` | pins it to this run's commit |
| runner class | `--deny-self-hosted-runners` | waybill releases only from GitHub-hosted runners |
| predicate type | `--predicate-type https://slsa.dev/provenance/v1` | already the command's default; passed explicitly so the gate does not depend on a `gh` default holding |

The four values that name an identity are read from the run's own default environment rather than written out, so they cannot drift from the workflow that signs. The remaining two are constants.

Evidence is part of the contract (see Output, above): the step MUST emit the verified subject, digest, predicate type, signer URI, source ref, commit and runner class into the log, and MUST fail if verification returned no attestation at all — asserting on a positive signal rather than on the absence of an error.

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

**Nightly workflow**: no changes — it dispatches `release.yml`, so it inherits every row above (see C-2).

## Non-contracts

- The GitHub action's INTERNAL implementation is not a waybill contract — we consume the output only.
- The Sigstore Fulcio CA and Rekor mirror URLs are GitHub's operational concern.
- Bundle format version (v0.3 today) will bump as Sigstore evolves; waybill inherits transparently.

## Test-authoring rules

- Every new emit step gets a corresponding review comment in the PR: "Where's the self-verify step for this subject?" — enforced by contract-review.
- YAML syntax validation via GitHub's workflow parser is part of the pre-merge CI (already runs).
- Post-merge acceptance test: fire a manual dispatch of the release workflow against a test tag; inspect the 6 uploaded attestations for shape.
