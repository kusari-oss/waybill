# Quickstart — milestone 075 credential stripping

Five operator-facing recipes. Each runs end-to-end against a post-075 mikebom build.

## Recipe 1 — Default behavior: credentials stripped

The headline new behavior. No flags needed beyond the normal scan command.

```bash
# Set up a git remote with embedded credentials (common GitHub App pattern)
cd ~/projects/my-app
git remote set-url origin https://x-access-token:ghs_AAA123XYZ@github.com/acme/my-app.git

# Run mikebom — credentials get stripped automatically
mikebom sbom scan --path . --output out.cdx.json
# INFO sanitized userinfo from auto-detected identifier; scheme=`repo`; url_safe=`https://<userinfo redacted>@github.com/acme/my-app.git`
```

Inspect the emitted SBOM:

```bash
jq '.metadata.component.externalReferences[] | select(.type == "vcs")' out.cdx.json
# {
#   "type": "vcs",
#   "url": "https://github.com/acme/my-app.git",
#   "comment": "auto-detected from git remote `origin` (credentials stripped)"
# }

# Verify the token is gone:
grep -c "ghs_AAA123XYZ" out.cdx.json
# 0
```

The `url` has the userinfo stripped. The `comment` field (which carries the `source_label`) reflects the sanitization with `(credentials stripped)`. The original token string appears nowhere in the document.

## Recipe 2 — Manual flag emits verbatim

Operators who explicitly type credentials get them through verbatim. No sanitization, no warning.

```bash
mikebom sbom scan \
  --repo https://USER:TOKEN@github.com/acme/foo.git \
  --path /tmp/non-git-dir \
  --output out.cdx.json
```

Inspect:

```bash
jq '.metadata.component.externalReferences[] | select(.type == "vcs") | .url' out.cdx.json
# "https://USER:TOKEN@github.com/acme/foo.git"
```

Manual = verbatim. The operator typed it; the tool respects that. Use this path if you have credentials you specifically want emitted (typically: airgapped or internal-network setups).

## Recipe 3 — Opt out for non-sensitive credentials

For organizations with internal SBOM pipelines where the credentials are infrastructure-level (e.g., a public read-only deploy token), pass the opt-out flag.

```bash
mikebom sbom scan --path . --keep-credentials-in-identifiers --output out.cdx.json
# INFO --keep-credentials-in-identifiers set; userinfo in auto-detected identifiers will be preserved verbatim
```

Inspect:

```bash
jq '.metadata.component.externalReferences[] | select(.type == "vcs")' out.cdx.json
# {
#   "type": "vcs",
#   "url": "https://x-access-token:ghs_AAA123XYZ@github.com/acme/my-app.git",
#   "comment": "auto-detected from git remote `origin`"
# }
```

The auto-detected URL preserves userinfo. The `(credentials stripped)` suffix is absent because no sanitization happened. An info-level log line at scan start records that the operator opted out.

## Recipe 4 — SSH-form URLs unchanged

SSH-form URLs (`git@github.com:foo/bar.git`) carry no userinfo by construction. Sanitization is a no-op; the SBOM is byte-identical to alpha.16.

```bash
git remote set-url origin git@github.com:acme/my-app.git
mikebom sbom scan --path . --output out.cdx.json
# (no sanitization log line — nothing to strip)
```

```bash
jq '.metadata.component.externalReferences[] | select(.type == "vcs")' out.cdx.json
# {
#   "type": "vcs",
#   "url": "git@github.com:acme/my-app.git",
#   "comment": "auto-detected from git remote `origin`"
# }
```

The `(credentials stripped)` suffix is absent. SSH-form is the safest operator pattern for environments where SBOMs ship publicly.

## Recipe 5 — Build-tier sanitization (mikebom trace run)

The same default-strip behavior applies to `mikebom trace run`. Both auto-detected `repo:` and `git:` identifiers get sanitized.

```bash
git remote set-url origin https://x-access-token:ghs_BBB456@github.com/acme/my-app.git
mikebom trace run -- ./build.sh
# INFO sanitized userinfo from auto-detected identifier; scheme=`repo`; url_safe=`https://<userinfo redacted>@github.com/acme/my-app.git`
# INFO sanitized userinfo from auto-detected identifier; scheme=`git`; url_safe=`https://<userinfo redacted>@github.com/acme/my-app.git#abc1234567890...`
```

Inspect the build-tier SBOM:

```bash
jq '.metadata.component.externalReferences[] | select(.type == "vcs")' build.cdx.json
# [
#   {
#     "type": "vcs",
#     "url": "https://github.com/acme/my-app.git",
#     "comment": "auto-detected from build-tier git remote `origin` (credentials stripped)"
#   },
#   {
#     "type": "vcs",
#     "url": "https://github.com/acme/my-app.git#abc1234567890abcdef1234567890abcdef1234",
#     "comment": "auto-detected from build-tier `git rev-parse HEAD` (credentials stripped)"
#   }
# ]
```

Both identifier slots sanitized; both `comment` fields carry the suffix. The token appears nowhere in the build SBOM, the eBPF trace output, or the signed attestation envelope.

## How to verify on existing alpha.16 outputs (regression check)

Before upgrading: re-scan a repo with a credentialed remote on alpha.16 and confirm the leak.

```bash
mikebom-alpha.16 sbom scan --path . --output before.cdx.json
grep -c "ghs_" before.cdx.json
# (count > 0 means token is in the output — that is the bug 075 fixes)
```

After upgrading to alpha.17 (which ships milestone 075):

```bash
mikebom-alpha.17 sbom scan --path . --output after.cdx.json
grep -c "ghs_" after.cdx.json
# 0 (token successfully stripped)
```
