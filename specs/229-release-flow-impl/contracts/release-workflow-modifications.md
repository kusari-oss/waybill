# Contract: `.github/workflows/release.yml` modifications

**Feature**: 229-release-flow-impl
**Phase**: 1

Pins the changes required to the existing `release.yml`. This is a MODIFICATION contract, not a wholesale replacement — most of release.yml stays intact.

## Modification 1 — expand tag-trigger regex

**Current** (verified at `.github/workflows/release.yml:3-7`):

```yaml
push:
  tags:
    - 'v*-alpha.*'
    - 'v*-beta.*'
    - 'v*-rc.*'
```

**Post-229**:

```yaml
push:
  tags:
    - 'v*-alpha.*'       # bridge alphas (Q3 always-acceptable)
    - 'v*-beta.*'        # existing; reserved for future beta channel
    - 'v*-rc.*'          # existing; bridge RCs (Q3 always-acceptable)
    - 'v*-nightly.*'     # NEW — nightly channel (US2)
    - 'v*-preview.*'     # NEW — Q3 bridge with -preview suffix
    - 'v[0-9]+.[0-9]+.[0-9]+'  # NEW — bare-SemVer stables
```

**Contract**: any tag matching one of these patterns triggers the release-build. Non-matching tags (e.g., `v-audit-baseline`, feature branch tags) do NOT trigger.

**GHA-trigger compatibility note**: GitHub Actions tag-filter patterns use simple globs (`*` = any chars) plus `[0-9]` character classes. The pattern `v[0-9]+.[0-9]+.[0-9]+` is confirmed to work per GitHub docs — the `+` is the trigger-filter's "one or more" quantifier.

## Modification 2 — integrate `--sign` unconditionally

**Verified m222 CLI surface** (grep of `waybill-cli/src/cli/scan_cmd.rs`): `--sign` is a **flag on `sbom scan`**, NOT a standalone `sbom sign` sub-command. Correct invocation is `waybill sbom scan --path <target> --sign --output <sbom>` — generates + signs in one shot.

**Current release.yml state** (verified via inspection): today's release.yml does NOT invoke `waybill sbom scan`. Image SBOM is docker/buildx-generated (`sbom: true` on `docker/build-push-action`); image signing is `cosign sign` on the image digest. Waybill's own SBOM of release binaries is absent from the release pipeline today.

**229 integration**: add a NEW step that invokes `waybill sbom scan --sign` on the produced release binaries + OCI image, attach the signed SBOMs as release artifacts. This is what CISA 2026 Author Signature actually wants — waybill's own SBOM attestation of what waybill produced.

### New step in the release-creation job

Positioned after the multi-arch OCI image publish + cosign image-sign steps, before the GitHub release-creation step. Runs on Linux x86_64 (has waybill binary readily available from the build-linux-x86_64 job's artifact):

```yaml
- name: Download waybill release binary (Linux x86_64)
  uses: actions/download-artifact@<pinned-sha>
  with:
    name: waybill-linux-x86_64
    path: ./bin

- name: Chmod waybill binary
  run: chmod +x ./bin/waybill

- name: Generate + sign SBOM of the OCI image (waybill's own; unconditional per Q2)
  # NO if:-branch on tag format. Per FR-003 + FR-004, ALL releases sign.
  env:
    OWNER: ${{ github.repository_owner }}
    DIGEST: ${{ steps.build.outputs.digest }}
  run: |
    set -euo pipefail
    IMAGE="ghcr.io/${OWNER}/waybill@${DIGEST}"
    # waybill sbom scan --image <ref> --sign --output <sbom.json>
    # (Sigstore keyless via ambient GHA OIDC — no --sign-key)
    ./bin/waybill sbom scan --image "$IMAGE" --sign --output waybill-image.cdx.json.sig

- name: Generate + sign SBOM of the source tree (waybill's own; unconditional per Q2)
  run: |
    set -euo pipefail
    ./bin/waybill sbom scan --path . --sign --output waybill-source.cdx.json.sig
```

**Contract per FR-003 + FR-004**:
- Applies unconditionally to every tag matching the modification-1 trigger regex.
- Fail-closed: if `waybill sbom scan --sign` exits non-zero, the workflow-step (and the workflow-run) fails naturally via `set -euo pipefail`. No unsigned fallback.
- Uses the m222 CLI surface verbatim (`sbom scan --sign` — verified against `waybill-cli/src/cli/scan_cmd.rs` at PR-authoring time).
- OIDC identity comes from the ambient GHA token (`id-token: write` permission required at workflow level — see Modification 3).

### Modification of `create-github-release` step

The existing release-creation step must ALSO upload the new `waybill-image.cdx.json.sig` + `waybill-source.cdx.json.sig` artifacts to the release page. Adjust the `files:` list on the release-creation action to include those two files.

**Note on file extensions**: `waybill sbom scan --output <path>` writes the signed SBOM to `<path>` — the `--sign` flag causes waybill to write the signature envelope INSIDE the JSON payload (Sigstore bundle format), NOT as a separate `.sig` sidecar. Verify at implementation time whether the m222 output shape is (a) a single file with embedded signature, or (b) file + sidecar `<path>.sig`. Adjust the release-artifact upload accordingly.

## Modification 3 — permissions verification

The existing release.yml already has `id-token: write` per m222. Verify at implementation time that this permission grant is:

- (a) present at workflow level (not per-job), so cron-triggered nightly dispatches inherit it
- (b) documented in a workflow-level comment referencing m222

If either is missing, add the missing piece.

## NOT modified

- Existing `build-ebpf` job — unchanged.
- Existing per-platform build jobs (Linux x86_64/aarch64, macOS aarch64, Windows x86_64) — unchanged.
- Existing `publish-container` multi-arch OCI publishing — unchanged.
- The `workflow_dispatch` entry-point + `tag` input — unchanged (nightly.yml relies on this).
- The `env.TAG_NAME: ${{ inputs.tag || github.ref_name }}` fallback — unchanged.

## Regression risk

- **The unconditional `--sign` step MAY reject signing on non-`main`-branch tags** if the OIDC token audience differs. Test on the feature branch via workflow_dispatch before merging.
- **Sigstore/Fulcio downtime** — a Fulcio outage would fail EVERY release under the new model. Accepted risk; matches CISA 2026 compliance stance (Principle V + III together).

## Test plan

- **Static**: `actionlint .github/workflows/release.yml` returns clean post-modification.
- **Dry-run**: workflow_dispatch with a test tag on the feature branch — verify: (a) build succeeds; (b) sign step runs; (c) `.sig` files uploaded to the test release.
- **First stable**: post-merge, cutting `v0.2.0` produces signed SBOMs verifiable via `cosign verify-blob --certificate-identity-regexp "https://github.com/kusari-oss/waybill/.*" --certificate-oidc-issuer https://token.actions.githubusercontent.com`.
- **First nightly**: same verify command against a `v0.2.0-nightly.YYYYMMDD` SBOM succeeds.
