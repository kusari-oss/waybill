# What the FR-015 self-verify step enforced, and what it printed

**Issue**: #1240 | **Probe**: `probe-selfverify.sh` | **Measured**: 2026-10-20
**Tool**: `gh` 2.102.0 (2026-09-30)
**Subjects**: the published `v0.11.0` (stable lane) and
`v0.11.1-nightly.20261020` (nightly lane) linux-x86_64 tarballs, plus the
`v0.11.0` multi-arch image — real released artifacts, not fixtures.

All six steps ran:

```
gh attestation verify "$ARTIFACT_PATH" --repo "$REPO_NAME"
```

## Finding 1 — `--repo` is the weakest identity the command offers

`--repo` validates the certificate's SourceRepository and
SourceRepositoryOwner. It does not validate which workflow signed. `gh`'s own
help says so: *"Ideally, the path of the signer workflow is also validated
using the `--signer-workflow` or `--cert-identity` flags."* So the step was
satisfied by a SLSA provenance attestation for that digest produced by **any**
workflow in `kusari-oss/waybill`.

Each added flag was confirmed to have teeth by passing a deliberately wrong
value — a flag that silently ignores its argument would otherwise look exactly
like one that enforces it:

| Case | Exit | What `gh` said |
|---|---|---|
| `--repo` only | 0 | — |
| `+ --signer-workflow …/release.yml` | 0 | — |
| `+ --signer-workflow …/ci.yml` | 1 | `verifying with issuer "sigstore.dev"` |
| `+ --source-ref refs/heads/main` (on a tag build) | 1 | `expected SourceRepositoryRef to be refs/heads/main, got refs/tags/v0.11.0` |
| `+ --source-digest 000…0` | 1 | `expected SourceRepositoryDigest to be 000…0, got 350280487954133b…` |

Two flags were **not** added, because measurement showed they would be
redundant rather than tighter:

- **Predicate type** is already enforced: `--predicate-type` defaults to
  `https://slsa.dev/provenance/v1`. It is now passed explicitly anyway, so the
  gate does not depend on a `gh` default holding.
- **A subject-digest assertion in `jq`** would re-check what the tool already
  checks. Appending one byte to the artifact moved the digest the lookup is
  keyed on and produced `HTTP 404` with exit 1, so the artifact's bytes are
  already bound. This also disposes of the `sha256sum`-vs-`shasum -a 256`
  portability problem #1240 anticipated for the `macos-14` job: nothing needs
  to hash the artifact in the step.

## Finding 2 — the step printed nothing at all on a successful release

`gh attestation verify` writes its human-readable summary **only** to a
terminal. Through a pipe — which is how Actions captures step output — it
printed **0 bytes on stdout and 0 bytes on stderr**, and exited 0.

| Invocation | Exit | stdout |
|---|---|---|
| `gh attestation verify <file> --repo <repo>` | 0 | **0 bytes** |
| `… --format json` | 0 | 13,739 bytes |

The exit code still had teeth, so this was never a vacuous check — a digest
with no attestation returns `HTTP 404` and fails the job. It was an invisible
one. `--format json` with `--jq` fixes both halves: it is the only scriptable
output, `gh` applies the filter with its own built-in jq (so no `jq` is needed
on any runner, including `windows-latest`), and it emitted 439 bytes of
evidence through the same pipe that previously carried nothing.

## Finding 3 — the two release lanes sign at different refs

This is why `--source-ref` cannot be a constant. Read from the certificates
and corroborated by each attestation's own SLSA predicate:

| Lane | `event_name` | `buildSignerURI` ref | `sourceRepositoryDigest` |
|---|---|---|---|
| stable (`v0.11.0`) | `push` | `refs/tags/v0.11.0` | the tag's commit |
| nightly (`…nightly.20261020`) | `workflow_dispatch` | `refs/heads/main` | main's tip |

`nightly.yml` dispatches `gh workflow run release.yml … --ref main`, so the
nightly run's `GITHUB_REF` is `refs/heads/main` even though it builds a tag.
`--source-ref "$GITHUB_REF"` and `--source-digest "$GITHUB_SHA"` are therefore
correct in both lanes with no conditional, and both lanes verify clean under
the full flag set.

The same asymmetry is already handled on the cosign side by the
`refs/(tags/v.+|heads/main)` identity regexp at `release.yml`'s source-SBOM
signature self-check — this finding is the `gh attestation verify` equivalent
of that note. That regexp also works verbatim as
`--cert-identity-regex`, which is what `docs/verifying-releases.md` now offers
consumers who do not want to branch on the channel: measured passing on both
lanes, and exit 1 when pinned to `ci.yml` instead.

## Finding 4 — the OCI subject is a name-plus-digest, not a path

The image attestation's subject is `name = ghcr.io/kusari-oss/waybill` with
`digest.sha256` equal to the manifest digest, so `oci://<ref>@<digest>` is the
only form that resolves, and the summary's `subject:` line reads as a
repository name rather than a filename. The same `--jq` filter renders all
three document shapes (tarball, zip, image) without a variant.

## The step body as shipped, exercised

The `run:` block was extracted from
`.github/actions/verify-attestation/action.yml` and executed locally with the
runner's variables supplied by hand (`GITHUB_REPOSITORY`,
`GITHUB_WORKFLOW_REF`, `GITHUB_REF`, `GITHUB_SHA`), against the real `v0.11.0`
tarball:

| Case | Exit | Behaviour |
|---|---|---|
| all variables correct | 0 | printed the seven-line evidence block |
| `GITHUB_REF=refs/heads/main` on a tag build | 1 | `set -e` propagated `gh`'s failure out of the command substitution |
| `gh` stubbed to exit 0 printing nothing | 1 | `::error::gh attestation verify exited 0 but returned no attestation` |

The third case is the mutation for the emptiness guard: without it, a `gh`
that succeeded while returning nothing would have read as a pass. The second
confirms the step fails closed (contract C-5) through a command substitution,
which `set -e` does propagate.

One portability note resolved here rather than in CI: on `windows-latest`
`$RUNNER_TEMP` is a backslash path (`D:\a\_temp`), which bash will not accept
in a redirect, so the summary is held in a shell variable instead of a file.
That also keeps scratch files out of the workspace the release job packages
from.
