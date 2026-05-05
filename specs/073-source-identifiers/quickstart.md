# Quickstart — milestone 073 identifiers

> **Rename note (2026-05-03)**: original drafts called this "source identifiers" with a `--with-source <scheme>:<value>` flag; renamed pre-merge to "identifiers" with dedicated flags (`--repo`, `--git-ref`, `--image-id`, `--attestation`, `--id <scheme>=<value>`). See `docs/reference/identifiers.md` for the operator-facing recipes in their post-rename form. The text below was the original-draft set of recipes; it is preserved as historical record but is OUT OF DATE — `--with-source` no longer exists.

Five operator-facing recipes. Each runs end-to-end against a post-073 mikebom build with no special setup beyond a normal git checkout (Recipe 1) or a tempdir (Recipes 2-5).

## Recipe 1 — Auto-detected `repo:` from a git checkout

The zero-config happy path. No flags needed.

```bash
cd ~/projects/my-rust-app  # any project that's a git checkout
mikebom sbom scan --path . --format cyclonedx-json \
    --output cyclonedx-json=/tmp/out.cdx.json
```

Inspect the emitted SBOM:

```bash
jq '.metadata.component.externalReferences[] | select(.type == "vcs")' /tmp/out.cdx.json
```

Expected output:

```json
{
  "type": "vcs",
  "url": "git@github.com:acme/my-rust-app.git",
  "comment": "auto-detected from git remote `origin`"
}
```

The `url` is whatever your `git remote get-url origin` returned. The `comment` records which remote was selected (per the Q1 three-step fallback: `origin` → `upstream` → first-listed).

If your repo has no `origin` but does have `upstream` (common on forks):

```bash
git remote -v
# upstream  git@github.com:acme/my-rust-app.git (fetch)
# upstream  git@github.com:acme/my-rust-app.git (push)

mikebom sbom scan --path . --format cyclonedx-json \
    --output cyclonedx-json=/tmp/out-fork.cdx.json
```

Expected `comment`: `"auto-detected from git remote `upstream` (origin absent)"`.

## Recipe 2 — Manual `--with-source` for non-git projects

Source extracted from a tarball, no git checkout. Manual flag attaches the identifier:

```bash
tar xzf my-app-v1.0.tar.gz -C /tmp/my-app
mikebom sbom scan --path /tmp/my-app \
    --with-source repo:git@github.com:acme/my-app.git \
    --format cyclonedx-json --output cyclonedx-json=/tmp/out.cdx.json
```

Same output as Recipe 1, except the `comment` field reads `"manual --with-source"`.

If you supply BOTH a manual flag AND auto-detection finds something, the manual flag wins (FR-006). Mikebom logs at info level which entry was overridden.

## Recipe 3 — User-defined corporate identifier

Attach an internal asset identifier alongside the auto-detected `repo:`:

```bash
cd ~/projects/my-rust-app
mikebom sbom scan --path . \
    --with-source acme_corp_id:svc-alpha-123 \
    --with-source internal_ticket:PROJ-456 \
    --format cyclonedx-json --output cyclonedx-json=/tmp/out.cdx.json
```

The `repo:` identifier rides `metadata.component.externalReferences[type:vcs]` (auto-detected). The `acme_corp_id:` and `internal_ticket:` user-defined identifiers ride `metadata.properties[]` under the `mikebom:source-identifiers` annotation:

```bash
jq '.metadata.properties[] | select(.name == "mikebom:source-identifiers")' /tmp/out.cdx.json
```

Expected:

```json
{
  "name": "mikebom:source-identifiers",
  "value": "[{\"scheme\":\"acme_corp_id\",\"value\":\"svc-alpha-123\"},{\"scheme\":\"internal_ticket\",\"value\":\"PROJ-456\"}]"
}
```

The array is sorted lex by `(scheme, value)` for determinism.

## Recipe 4 — Auto-detected `image:` from `mikebom sbom scan --image`

Image-tier scan auto-detects the `image:` identifier from the resolved registry reference + digest:

```bash
mikebom sbom scan --image docker.io/acme/foo:v1 \
    --format cyclonedx-json --output cyclonedx-json=/tmp/out-img.cdx.json
```

Inspect:

```bash
jq '.metadata.component.externalReferences[] | select(.type == "distribution")' /tmp/out-img.cdx.json
```

Expected (per the Q3 canonical shape):

```json
{
  "type": "distribution",
  "url": "docker.io/acme/foo:v1@sha256:abc1234...",
  "comment": "auto-detected from resolved image reference"
}
```

When the image is loaded from a tarball without a registry context, the URL omits the registry: `acme/foo@sha256:abc1234...`.

## Recipe 5 — Cross-tier handshake (forward-looking)

This recipe is the milestone-074 preview. Today it works at the EMISSION level (this milestone) but not at the resolution level (milestone 074).

After Recipe 1, you have `~/projects/my-rust-app/source.cdx.json` carrying a `repo:git@github.com:acme/my-rust-app.git` identifier. After Recipe 4, you have `/tmp/out-img.cdx.json` for the deployed image.

Today (post-073): an external tool can extract the identifier from the source SBOM and use it as a stable handle:

```bash
jq -r '.metadata.component.externalReferences[] | select(.type == "vcs") | .url' \
    /tmp/source.cdx.json
# git@github.com:acme/my-rust-app.git
```

Tomorrow (post-074): `mikebom sbom scan --image foo:v1 --bind-to-source repo:git@github.com:acme/my-rust-app.git` will resolve the identifier to the source SBOM file path via a small lookup directory. Operators won't need to track file paths.

This milestone (073) lays the foundation. The forward-looking handshake passes the SC-005 acceptance test once milestone 074 ships.

## Recipe 6 — `mikebom trace` build-tier identifiers (manual)

Build-tier scans accept `--with-source` but don't auto-detect (the build context is opaque to mikebom-trace):

```bash
mikebom trace --with-source repo:git@github.com:acme/foo.git \
    --with-source git:https://github.com/acme/foo.git#abc1234567890 \
    -- ./build.sh
```

The build-tier SBOM carries both identifiers in the same standards-native carriers as source-tier scans. The `git:` identifier with the commit-anchored fragment is useful here because the build-tier SBOM is typically associated with a specific commit-of-record, not just the repo identity.

## Recipe 7 — External tool extracting identifiers (any language)

Any SBOM consumer can extract identifiers without mikebom source-code access. CDX-side example using `jq`:

```bash
jq '
{
  builtin: [.metadata.component.externalReferences[]
              | select(.type == "vcs" or .type == "distribution" or .type == "attestation")
              | {scheme: (if .type == "vcs" then "repo" elif .type == "distribution" then "image" else "attestation" end),
                 value: .url, comment}],
  user_defined: (.metadata.properties[]?
                  | select(.name == "mikebom:source-identifiers")
                  | .value | fromjson)
}
' /tmp/out.cdx.json
```

Expected output (per the published `docs/reference/source-identifiers.md` decode recipe):

```json
{
  "builtin": [
    { "scheme": "repo", "value": "git@github.com:acme/foo.git",
      "comment": "auto-detected from git remote `origin`" },
    { "scheme": "image", "value": "docker.io/acme/foo:v1@sha256:abc...",
      "comment": "auto-detected from resolved image reference" }
  ],
  "user_defined": [
    { "scheme": "acme_corp_id", "value": "svc-alpha-123" }
  ]
}
```

Same data is extractable from SPDX 2.3 (`Package.externalRefs[PERSISTENT-ID]` + `creationInfo.creators` + envelope-decode for user-defined) and from SPDX 3 (`Element.externalIdentifier[]` for both built-in and user-defined). The published reference at `docs/reference/source-identifiers.md` documents per-format decode recipes.
