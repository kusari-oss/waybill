# Verifying waybill releases

Every waybill release (stable + nightly) is published with a verifiable **SLSA build provenance attestation** proving the artifact came from the official `kusari-oss/waybill` build pipeline. This page is your copy-paste-ready recipe for verifying any release artifact.

**Audience**: downstream consumer of waybill — distro packager, compliance auditor, air-gapped operator, or first-time verifier.

## What each release ships

Every release carries SLSA provenance attestations for:

| Artifact | Example filename | How to verify |
|---|---|---|
| linux-x86_64 tarball | `waybill-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` | Step 2 |
| linux-aarch64 tarball | `waybill-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz` | Step 2 |
| macos-aarch64 tarball | `waybill-vX.Y.Z-aarch64-apple-darwin.tar.gz` | Step 2 |
| windows-x86_64 zip | `waybill-vX.Y.Z-x86_64-pc-windows-msvc.zip` | Step 2 |
| multi-arch OCI image | `ghcr.io/kusari-oss/waybill:vX.Y.Z` | Step 3 |
| Source SBOM sidecar | `waybill-source.cdx.json` | Step 4 |

Each attestation binds the artifact's SHA-256 digest to a SLSA Provenance v1.0 predicate that names the source commit, the workflow-run URL, and the builder identity.

## Prerequisites

- **`gh` CLI ≥ 2.49** (verify with `gh --version`). Install from [cli.github.com](https://cli.github.com) if absent.
- **Network access** to `github.com` and `rekor.sigstore.dev`. (Air-gapped? See Step 5 for the bundle-file path.)
- **The artifact to verify** — a downloaded tarball, a pullable OCI image ref, or a source SBOM sidecar.

## Step 2 — Verify a downloaded tarball

Pick a release from https://github.com/kusari-oss/waybill/releases. Download one of the platform tarballs:

```bash
VERSION=v0.4.0   # or whatever the latest is
TARGET=x86_64-unknown-linux-gnu   # your platform
curl -LO "https://github.com/kusari-oss/waybill/releases/download/${VERSION}/waybill-${VERSION}-${TARGET}.tar.gz"

gh attestation verify "waybill-${VERSION}-${TARGET}.tar.gz" --repo kusari-oss/waybill
```

**Expected output**:

```text
Loaded digest sha256:<hex> for file://waybill-v0.4.0-x86_64-unknown-linux-gnu.tar.gz
Loaded 1 attestation from GitHub API

✓ Verification succeeded!

The following policy criteria were satisfied:
- SANRegex: (?i)^https://github\.com/kusari-oss/waybill/
- Predicate type matches: https://slsa.dev/provenance/v1
- Source repository matches: kusari-oss/waybill
```

The trailing message lines out the SLSA Provenance predicate URI (`https://slsa.dev/provenance/v1`), the source repo (`kusari-oss/waybill`), and — with `--format json` — the source commit SHA plus the workflow run URL that produced the tarball.

## Step 3 — Verify the multi-arch OCI image

```bash
IMAGE_REF=ghcr.io/kusari-oss/waybill:${VERSION}
docker pull "$IMAGE_REF"

gh attestation verify "oci://$IMAGE_REF" --repo kusari-oss/waybill
```

**Expected output**: same shape as Step 2, but the loaded digest is the image's manifest digest (visible via `docker inspect "$IMAGE_REF" | jq '.[0].RepoDigests'`).

## Step 4 — Verify the source SBOM sidecar

Download the source SBOM (published as a release asset alongside the binary tarballs):

```bash
curl -LO "https://github.com/kusari-oss/waybill/releases/download/${VERSION}/waybill-source.cdx.json"
gh attestation verify waybill-source.cdx.json --repo kusari-oss/waybill
```

**Expected output**: same shape as Step 2.

## Step 5 — Verify a mirrored artifact (offline / third-party registry)

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

## Tamper-detection sanity check

Prove the verification actually detects tampering:

```bash
# Copy the tarball, flip one byte in the copy:
cp "waybill-${VERSION}-${TARGET}.tar.gz" tampered.tar.gz
printf '\x00' | dd of=tampered.tar.gz bs=1 seek=100 count=1 conv=notrunc

# Verification against the tampered copy MUST fail:
gh attestation verify tampered.tar.gz --repo kusari-oss/waybill
```

**Expected output**:

```text
✘ Verification failed: subject digest sha256:<hex-of-tampered> does not match
  any subject in the emitted attestations for kusari-oss/waybill
```

Exit code is non-zero.

## Historical releases (before m668 landed)

Releases produced BEFORE the SLSA provenance feature (milestone 668) shipped do NOT have SLSA provenance. Running `gh attestation verify` against them will report:

```text
✘ Loaded 0 attestations from GitHub API. The artifact must have been built with SLSA attestation emission enabled.
```

This is expected. Older releases still carry the pre-existing cosign-keyless signature (m222) — verify those via `cosign verify-blob` instead. See [RELEASING.md](../RELEASING.md) for the cosign recipe.

## Troubleshooting

### `gh: command not found`

Install the GitHub CLI from https://cli.github.com. Version 2.49+ is required for `gh attestation verify`.

### `gh: unknown command "attestation"`

Your `gh` is older than 2.49. Upgrade: `brew upgrade gh` (macOS), `apt-get upgrade gh` (Ubuntu with the GH apt repo), or download the latest release from https://github.com/cli/cli/releases.

### `Verification failed: no attestation found` on a fresh release

Wait 30-60 seconds. GitHub's attestation-store indexing is asynchronous; the release publishes before the attestation is fully indexed. If the failure persists past 5 minutes, waybill's release-workflow self-verify step (`Self-verify tarball provenance (m668 FR-015)`) should also have failed — check https://github.com/kusari-oss/waybill/actions for the workflow run.

### Verification succeeds but the source commit isn't what I expected

Add `--format json` to get the full predicate JSON. Look at `.buildDefinition.resolvedDependencies[]` for the source commit SHA and `.runDetails.metadata.invocationId` for the exact workflow-run URL that produced this artifact. Cross-check against `git log --oneline <SHA>` in your local waybill checkout.

## Provenance predicate reference

The full data model — SLSA Provenance predicate shape, subject shape, Sigstore bundle envelope — is documented at [`specs/668-slsa-provenance/data-model.md`](../specs/668-slsa-provenance/data-model.md).

## Related references

- SLSA v1.0 specification: https://slsa.dev/spec/v1.0
- SLSA Provenance predicate schema: https://slsa.dev/spec/v1.0/provenance
- `gh attestation` reference: https://cli.github.com/manual/gh_attestation
- Waybill milestone 668 spec: [`specs/668-slsa-provenance/spec.md`](../specs/668-slsa-provenance/spec.md)
- Waybill release-workflow source: [`.github/workflows/release.yml`](../.github/workflows/release.yml)

## Also-supported verification tools

`gh attestation verify` is the recommended path. The same Sigstore bundle also verifies via:

- **cosign**: `cosign verify-attestation --certificate-oidc-issuer https://token.actions.githubusercontent.com --certificate-identity-regexp '^https://github\.com/kusari-oss/waybill/' <artifact>`
- **slsa-verifier** (reference SLSA verifier): `slsa-verifier verify-artifact --source-uri github.com/kusari-oss/waybill <artifact>`

These are longer recipes with more configuration; use `gh attestation verify` unless you have a specific reason to prefer the alternatives.
