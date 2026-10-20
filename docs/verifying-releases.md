# Verifying waybill releases

Waybill releases **v0.4.0 and later** (stable + nightly) are published with a verifiable **SLSA build provenance attestation** proving the artifact came from the official `kusari-oss/waybill` build pipeline. This page is your copy-paste-ready recipe for verifying any release artifact.

**Audience**: downstream consumer of waybill — distro packager, compliance auditor, air-gapped operator, or first-time verifier.

## Which recipe applies to your release

Provenance attestation started with **v0.4.0**. Pick your column:

| Your release | Binary tarball / zip | OCI image | Source SBOM sidecar |
|---|---|---|---|
| **v0.4.0 and later** | `gh attestation verify` — [Step 2](#step-2--verify-a-downloaded-tarball) | `gh attestation verify oci://…` — [Step 3](#step-3--verify-the-multi-arch-oci-image) | `cosign verify-blob` — [Step 4](#step-4--verify-the-source-sbom-sidecar) |
| **v0.3.0 and earlier** | `SHA256SUMS` only — no provenance exists. [See below](#releases-before-v040) | image exists, but carries no attestation | `cosign verify-blob` — [Step 4](#step-4--verify-the-source-sbom-sidecar) |

`gh attestation verify` against a pre-v0.4.0 artifact returns `HTTP 404: Not
Found` from the attestation API, because no attestation was ever created for
that digest. That is expected, not a verification failure.

## What each release ships

Every release from **v0.4.0** onward carries SLSA provenance attestations for:

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

> **Scripting this? `gh attestation verify` writes that summary only to a
> terminal.** Redirected to a pipe or a file it prints *nothing at all* and
> still exits 0 on success, so a script that greps its output sees an empty
> string whether verification passed or was never attempted. In CI, assert on
> `--format json` content — a non-empty array with the expected
> `predicateType` — and treat an empty document as a failure:
>
> ```bash
> gh attestation verify "$ARTIFACT" --repo kusari-oss/waybill --format json > att.json
> test -s att.json || { echo "no attestation document returned"; exit 1; }
> jq -e '.[0].verificationResult.statement.predicateType
>        == "https://slsa.dev/provenance/v1"' att.json > /dev/null
> ```

## Step 3 — Verify the multi-arch OCI image

```bash
IMAGE_REF=ghcr.io/kusari-oss/waybill:${VERSION}
docker pull "$IMAGE_REF"

gh attestation verify "oci://$IMAGE_REF" --repo kusari-oss/waybill
```

**Expected output**: same shape as Step 2, but the loaded digest is the image's manifest digest (visible via `docker inspect "$IMAGE_REF" | jq '.[0].RepoDigests'`).

## Step 4 — Verify the source SBOM sidecar

> **On v0.3.0 or earlier?** Skip the `gh attestation verify` call below — it
> will return `HTTP 404` because no attestation exists. The `cosign
> verify-blob` recipe further down *does* work on those releases: the signed
> SBOM sidecar reaches back to v0.3.0.

Download the source SBOM (published as a release asset alongside the binary tarballs):

```bash
curl -LO "https://github.com/kusari-oss/waybill/releases/download/${VERSION}/waybill-source.cdx.json"
gh attestation verify waybill-source.cdx.json --repo kusari-oss/waybill
```

**Expected output**: same shape as Step 2.

The release also signs this SBOM with waybill itself (`sbom scan --sign`, Sigstore
keyless as the release workflow). Verify that signature too:

```bash
curl -LO "https://github.com/kusari-oss/waybill/releases/download/${VERSION}/waybill-source.cdx.json.sig.bundle.json"
cosign verify-blob \
  --bundle waybill-source.cdx.json.sig.bundle.json \
  --certificate-identity "https://github.com/kusari-oss/waybill/.github/workflows/release.yml@refs/tags/${VERSION}" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  waybill-source.cdx.json
```

Nightlies are dispatched from `main`, so their identity ends in
`release.yml@refs/heads/main`. The same bundle is also published under its
earlier name, `waybill-source.cdx.json.bundle`.

### Verifying a waybill-produced keyless signature

An SBOM signed with `waybill sbom scan --sign` carries a detached
Sigstore Bundle sidecar at `<output>.sig.bundle.json`. Two things about
the verification command differ from the recipes above, and both have
caused real confusion:

**1. The identity depends on who signed.** waybill records an *identity
shape* alongside the subject:

| Shape | `--certificate-identity` looks like | Produced by |
|---|---|---|
| `email` | `someone@example.com` | a person, via `SIGSTORE_ID_TOKEN` |
| `workload` | `repo:org/repo:ref:refs/heads/main` | a CI job's ambient credential |

A recipe written for one will not match the other.

**2. The issuer is the certificate's, not the token's.** Under
federation these differ — signing through Sigstore's dex with a Google
account yields a token claiming the dex endpoint and a certificate
claiming `https://accounts.google.com`. Passing the token's value fails
with:

```text
Certificate's OIDCIssuer does not match (got ..., expected ...)
```

which reads like a broken signature and is not one.

**So do not compose the command by hand.** waybill prints one on every
successful sign, built entirely from the issued certificate, so it
cannot disagree with what actually verifies:

```text
To verify:
  cosign verify-blob \
      --bundle signed.cdx.json.sig.bundle.json \
      --certificate-identity 'repo:kusari-oss/waybill:ref:refs/heads/main' \
      --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
      signed.cdx.json
```

Signatures made against **Sigstore staging** get a different verifier:
cosign has no built-in staging mode and would need a `--trusted-root`
file, so waybill emits a `sigstore --staging verify identity` command
instead. For a non-standard Sigstore deployment it emits a command
explicitly labelled as a template requiring your own trust root.

## Step 5 — Verify a mirrored artifact (offline / third-party registry)

If the tarball has been re-published into your own artifact registry (Artifactory, Nexus, Harbor, S3 bucket, ...), the `gh attestation verify` API-based path won't reach GitHub's attestation store from the consumer side. Use the transferable Sigstore bundle path instead:

**On the mirror-publish side** (once, at release-publish time):

```bash
gh attestation download "waybill-${VERSION}-${TARGET}.tar.gz" \
    --repo kusari-oss/waybill

# The bundle is written to the current directory as
#   sha256:<artifact-digest>.jsonl
# There is no --output flag; rename it if your mirror needs a stable name.
# Publish the bundle alongside the tarball in your mirror.
```

**On the consumer side** (any time later, no network egress to github.com required):

```bash
gh attestation verify "waybill-${VERSION}-${TARGET}.tar.gz" \
    --bundle "sha256:<artifact-digest>.jsonl" \
    --repo kusari-oss/waybill
```

**Expected output**: same shape as Step 2, read out of the bundle bytes rather
than the attestation API.

> **On air-gapped use**: `--bundle` supplies the attestation itself, so no
> attestation-API lookup is needed. `gh` 2.102.0 has **no `--offline` flag**,
> and this recipe has only been exercised on a host with network access, so
> treat "works with zero egress" as untested rather than guaranteed — validate
> it inside your own isolated environment before depending on it.

## Tamper-detection sanity check

Prove the verification actually detects tampering:

```bash
# Copy the tarball, flip one byte in the copy:
cp "waybill-${VERSION}-${TARGET}.tar.gz" tampered.tar.gz
printf '\x00' | dd of=tampered.tar.gz bs=1 seek=100 count=1 conv=notrunc

# Verification against the tampered copy MUST fail:
gh attestation verify tampered.tar.gz --repo kusari-oss/waybill
```

**Expected output**: a non-zero exit and a 404 from the attestation API —

```text
Error: HTTP 404: Not Found (https://api.github.com/repos/kusari-oss/waybill/attestations/sha256:<hex-of-tampered>?...)
```

Attestations are looked up **by artifact digest**, so a modified file simply
has no attestation record. The failure therefore surfaces as "not found"
rather than as a digest-mismatch message. Either way the exit code is
non-zero, which is what a gate must key on.

## Releases before v0.4.0

Provenance emission shipped in milestone 668, which landed on `main` on
2026-08-28 — after v0.3.0 (2026-08-27) and before v0.4.0 (2026-08-31).
**v0.4.0 is the first release with a SLSA provenance attestation.** For
anything earlier, `gh attestation verify` returns:

```text
Error: HTTP 404: Not Found (https://api.github.com/repos/kusari-oss/waybill/attestations/sha256:<digest>?...)
```

That is the expected answer for those releases, not a tampering signal.

What you can still check on a pre-v0.4.0 release:

**1. Tarball integrity against the published checksums.** Note the limit: on
these releases `SHA256SUMS` is itself unsigned, so this proves the file matches
what the release page lists — it does not prove who built it.

```bash
VERSION=v0.3.0
TARGET=aarch64-apple-darwin
curl -LO "https://github.com/kusari-oss/waybill/releases/download/${VERSION}/SHA256SUMS"
curl -LO "https://github.com/kusari-oss/waybill/releases/download/${VERSION}/waybill-${VERSION}-${TARGET}.tar.gz"

grep "${TARGET}" SHA256SUMS | shasum -a 256 -c -
# waybill-v0.3.0-aarch64-apple-darwin.tar.gz: OK
```

**2. The source SBOM's keyless signature**, which *is* cryptographic and
reaches back to v0.3.0 — use [Step 4](#step-4--verify-the-source-sbom-sidecar).
The identity in a v0.3.0 bundle is
`https://github.com/kusari-oss/waybill/.github/workflows/release.yml@refs/tags/v0.3.0`,
issued by `https://token.actions.githubusercontent.com`.

If you need build provenance for a historical version, rebuild from the tag and
use a current release instead — provenance cannot be issued retroactively.

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

## Other tools

`gh attestation verify` is the recommended path for provenance, and
`cosign verify-blob` is the path for the signed source SBOM (Step 4). One
third-party tool also verifies waybill provenance end to end, and two tools
that look like they should work do not:

**`cosign verify-attestation` does not accept a file.** It resolves its
argument as an OCI reference, so pointing it at a tarball fails before any
verification happens:

```text
Error: could not parse reference: waybill-v0.11.0-aarch64-apple-darwin.tar.gz
```

Use `gh attestation verify` for release files, or `cosign verify-blob` with a
published `.bundle` for the source SBOM.

### `slsa-verifier` — which one matters

Two different projects ship a binary called `slsa-verifier`. One verifies
waybill provenance; the other cannot.

**Works: `slsa-framework/verifier` v0.1.0 or later.** Download a binary from
its [releases](https://github.com/slsa-framework/verifier/releases), then
verify a downloaded tarball against its attestation bundle (obtain the bundle
as in [Step 5](#step-5--verify-a-mirrored-artifact-offline--third-party-registry)):

```bash
slsa-verifier build \
  --param expected_source:github.com/kusari-oss/waybill \
  --param 'trusted_builders:[https://github.com/kusari-oss/waybill/.github/workflows/release.yml@refs/tags/v0.11.0]' \
  --param expected_tag:v0.11.0 \
  "sha256:<digest>.jsonl" waybill-v0.11.0-aarch64-apple-darwin.tar.gz
```

It prints a legible result and exits non-zero on failure — unlike
`gh attestation verify`, it is not TTY-gated, so the same output appears in a
pipe:

```text
PASS
SLSA Level: 3 (spec v1.0)

Core controls:
  [PASS]  L1  source-repo-match                     Source Repository Matches Expected URI
  [PASS]  L2  builder-id-trusted                    Builder Is in the Trusted Builders Allowlist
  [PASS]  L2  builder-identity-bound                Provenance Signed by Its Builder
  [PASS]  L2  provenance-l2-complete                Provenance Has the L2-Required Fields Populated
  [PASS]  L3  provenance-has-invocation-id          Provenance Records a Build Invocation ID
  [PASS]  L3  provenance-has-resolved-dependencies  Provenance Records Resolved Dependencies
```

`builder-identity-bound` passes because the tool's builder registry has an
entry for any GitHub Actions workflow that signs its own provenance: it checks
that the signing certificate names the same workflow `builder.id` claims.
Whether to trust that workflow is your decision, expressed in
`trusted_builders` — which is why that parameter is required, and why naming a
different builder fails the run and caps the reported level at L1.

The reported level describes this tool's assessment of one attestation against
its own controls. Waybill makes no SLSA Build level claim of its own: milestone
668 FR-014 forbids it, on the grounds that a formal level claim requires
ongoing conformance verification. A passing run here is evidence about one
artifact, not a programme-level claim.

**Does not work: `slsa-framework/slsa-verifier` v2.7.1, the older project.**
It trusts exactly one builder repository, a hardcoded constant naming
`slsa-framework/slsa-github-generator` (plus its own end-to-end test
repository). Waybill's provenance is signed by its own release workflow, so the
builder ID is rejected:

```text
FAILED: SLSA verification failed: untrusted reusable workflow:
  https://github.com/kusari-oss/waybill/.github/workflows/release.yml
  with builderID provided: false
```

Supplying `--builder-id` does not help — it is refused as invalid, because the
value is not a recognised builder. That tool additionally requires a builder
ref of the form `refs/tags/vX.Y.Z`, so no branch or commit-SHA pin satisfies
it. Use `slsa-framework/verifier` instead.

## Standard of evidence for this page

The recipes in [Which recipe applies to your release](#which-recipe-applies-to-your-release),
Steps 2-5, the tamper check and [Other tools](#other-tools) were executed
against published `kusari-oss/waybill` releases on 2026-10-19 and 2026-10-20
with `gh` 2.102.0, `cosign` v3.1.3, `slsa-framework/verifier` v0.1.0 and
`slsa-framework/slsa-verifier` v2.7.1 — on v0.11.0 and v0.4.0 for the
provenance paths and on v0.3.0 for the pre-provenance paths. Quoted output is
copied from those runs. The `slsa-framework/verifier` recipe was additionally
checked for teeth: a tampered artifact and a wrong `trusted_builders` value
each exit non-zero, the latter capping the reported level at L1.

Two things on this page are *not* covered by that run, and are called out so
nobody mistakes them for measured:

- the zero-egress property of the `--bundle` path in Step 5 (reasoned from how
  `--bundle` works, not observed on an isolated host);
- [Verifying a waybill-produced keyless signature](#verifying-a-waybill-produced-keyless-signature),
  which documents `waybill sbom scan --sign` output rather than release assets
  and was not re-exercised in this pass.

If you change a recipe here, run it. The two recipes this section replaced were
plausible, documented, and non-functional — one failed on its first argument,
the other could never have worked at all.
