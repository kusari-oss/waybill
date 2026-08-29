# Quickstart: verifying a waybill release via SLSA provenance

**Audience**: downstream consumer of waybill (distro packager, compliance auditor, air-gapped operator, or a curious first-time verifier). Post-merge of m668, every waybill release carries verifiable SLSA build provenance.

## 5-step recipe

### Step 1 — Prerequisites

- **`gh` CLI ≥ 2.49** (verify with `gh --version`). Install from [cli.github.com](https://cli.github.com) if absent.
- **Network access** to `github.com` and `rekor.sigstore.dev`. (Air-gapped? See Step 5 for the bundle-file path.)
- **The artifact to verify** — a downloaded tarball, a pullable OCI image ref, or a source SBOM sidecar.

### Step 2 — Verify a downloaded tarball

Pick a release from https://github.com/kusari-oss/waybill/releases. Download one of the platform tarballs:

```bash
VERSION=v0.4.0   # or whatever the latest is
TARGET=x86_64-unknown-linux-gnu   # your platform
curl -LO "https://github.com/kusari-oss/waybill/releases/download/${VERSION}/waybill-${VERSION}-${TARGET}.tar.gz"

gh attestation verify "waybill-${VERSION}-${TARGET}.tar.gz" --repo kusari-oss/waybill
```

**Expected output**:
```
Loaded digest sha256:<hex> for file://waybill-v0.4.0-x86_64-unknown-linux-gnu.tar.gz
Loaded 1 attestation from GitHub API

✓ Verification succeeded!

The following policy criteria were satisfied:
- SANRegex: (?i)^https://github\.com/kusari-oss/waybill/
- Predicate type matches: https://slsa.dev/provenance/v1
- Source repository matches: kusari-oss/waybill
```

The trailing message lines out the SLSA Provenance predicate URI (`https://slsa.dev/provenance/v1`), the source repo (`kusari-oss/waybill`), and — with `--format json` — the source commit SHA plus the workflow run URL that produced the tarball.

### Step 3 — Verify the multi-arch OCI image

```bash
IMAGE_REF=ghcr.io/kusari-oss/waybill:${VERSION}
docker pull "$IMAGE_REF"

gh attestation verify "oci://$IMAGE_REF" --repo kusari-oss/waybill
```

**Expected output**: same shape as Step 2, but the loaded digest is the image's manifest digest (visible via `docker inspect "$IMAGE_REF" | jq '.[0].RepoDigests'`).

### Step 4 — Verify the source SBOM sidecar

Download the source SBOM (published as a release asset alongside the binary tarballs):

```bash
curl -LO "https://github.com/kusari-oss/waybill/releases/download/${VERSION}/waybill-source.cdx.json"
gh attestation verify waybill-source.cdx.json --repo kusari-oss/waybill
```

**Expected output**: same shape as Step 2.

### Step 5 — Verify a mirrored artifact (offline / third-party registry)

If the tarball has been re-published into your own artifact registry (Artifactory, Nexus, Harbor, S3 bucket, ...), the `gh attestation verify` API-based path won't reach GitHub's attestation store from the consumer side. Use the transferable Sigstore bundle path instead:

**On the mirror-publish side** (once, at release-publish time):

```bash
gh attestation download waybill-${VERSION}-${TARGET}.tar.gz \
    --repo kusari-oss/waybill \
    --output waybill-${VERSION}-${TARGET}.bundle.jsonl

# Publish the bundle alongside the tarball in your mirror.
```

**On the consumer side** (any time later, no network egress to github.com required):

```bash
gh attestation verify \
    --bundle waybill-${VERSION}-${TARGET}.bundle.jsonl \
    waybill-${VERSION}-${TARGET}.tar.gz
```

**Expected output**: same shape as Step 2, but the bundle bytes carry the verification material end-to-end — no GitHub-side API call, no live Rekor query.

## Tamper detection sanity check

```bash
# Copy the tarball, flip one byte in the copy:
cp "waybill-${VERSION}-${TARGET}.tar.gz" tampered.tar.gz
printf '\x00' | dd of=tampered.tar.gz bs=1 seek=100 count=1 conv=notrunc

# Verification against the tampered copy MUST fail:
gh attestation verify tampered.tar.gz --repo kusari-oss/waybill
```

**Expected output**:
```
✘ Verification failed: subject digest sha256:<hex-of-tampered> does not match
  any subject in the emitted attestations for kusari-oss/waybill
```

Exit code is non-zero. This is SC-003 in action.

## Historical releases (before m668 landed)

Releases produced BEFORE m668 shipped do NOT have SLSA provenance. Running `gh attestation verify` against them will report:

```
✘ Loaded 0 attestations from GitHub API. The artifact must have been built with SLSA attestation emission enabled.
```

This is expected. Use the cosign-keyless signature path (documented separately in `docs/verifying-releases.md`) for older releases.

## Troubleshooting

### `gh: command not found`

Install the GitHub CLI from https://cli.github.com. Version 2.49+ is required for `gh attestation verify`.

### `gh: unknown command "attestation"`

Your `gh` is older than 2.49. Upgrade: `brew upgrade gh` (macOS), `apt-get upgrade gh` (Ubuntu with the GH apt repo), or download the latest release from https://github.com/cli/cli/releases.

### `Verification failed: no attestation found` on a fresh release

Wait 30-60 seconds. GitHub's attestation-store indexing is asynchronous; the release publishes before the attestation is fully indexed. If the failure persists past 5 minutes, the release workflow's FR-015 self-verify should also have failed — check https://github.com/kusari-oss/waybill/actions for the workflow run.

### Verification succeeds but the source commit isn't what I expected

Add `--format json` to get the full predicate JSON. Look at `.buildDefinition.resolvedDependencies[]` for the source commit SHA and `.runDetails.metadata.invocationId` for the exact workflow-run URL that produced this artifact. Cross-check against `git log --oneline <SHA>` in your local waybill checkout.

## Reference

- Full data model: [`data-model.md`](./data-model.md)
- Emission contract: [`contracts/workflow-step.md`](./contracts/workflow-step.md)
- Recipe contract: [`contracts/verification-recipe.md`](./contracts/verification-recipe.md)
- SLSA v1.0 spec: https://slsa.dev/spec/v1.0
- `gh attestation` reference: https://cli.github.com/manual/gh_attestation
- Deferred CLI verification (`waybill verify slsa`): [issue #725](https://github.com/kusari-oss/waybill/issues/725)
