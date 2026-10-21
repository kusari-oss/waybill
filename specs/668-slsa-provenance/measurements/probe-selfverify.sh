#!/usr/bin/env bash
# Probe for #1240: what the FR-015 self-verify step actually enforces, and
# what it prints.
#
# Every claim in `selfverify-identity.md` comes from this script. Re-run it
# when `gh` changes: an undocumented behaviour (the TTY-only summary) and a
# set of flag semantics are both things that can move without notice.
#
#   ./probe-selfverify.sh [<stable-tag> <nightly-tag>]
#
# Defaults to the two tags the finding was measured against. Needs `gh`
# authenticated, `jq`, and — for the OCI case — a ghcr.io login.
set -euo pipefail

STABLE_TAG="${1:-v0.11.0}"
NIGHTLY_TAG="${2:-v0.11.1-nightly.20261020}"
REPO=kusari-oss/waybill
SIGNER="$REPO/.github/workflows/release.yml"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
echo "work dir: $WORK"

fetch() { # <tag> <dir>
  mkdir -p "$WORK/$2"
  gh release download "$1" -R "$REPO" -D "$WORK/$2" \
    -p "waybill-$1-x86_64-unknown-linux-gnu.tar.gz" --clobber
  echo "$WORK/$2/waybill-$1-x86_64-unknown-linux-gnu.tar.gz"
}

# Report a command's exit status and the first line of what it said, without
# letting a non-zero status abort the probe: a failing case IS a measurement.
probe() { # <label> <expected-exit> <cmd...>
  local label="$1" want="$2"; shift 2
  local out="$WORK/out.txt" err="$WORK/err.txt" got=0
  "$@" >"$out" 2>"$err" || got=$?
  printf '%-44s exit=%s (want %s) stdout=%sB %s\n' \
    "$label" "$got" "$want" "$(wc -c <"$out" | tr -d ' ')" \
    "$(head -1 "$err" | cut -c1-90)"
  [ "$got" = "$want" ] || echo "  !! UNEXPECTED EXIT — the finding may be stale"
}

STABLE="$(fetch "$STABLE_TAG" stable)"
NIGHTLY="$(fetch "$NIGHTLY_TAG" nightly)"

echo
echo "## 1. The identity each flag enforces, and the identity it does not"
echo
# The shape that shipped: passes, and tells you only that SOME workflow in
# the repository attested this digest.
probe "repo only (the shipped shape)" 0 \
  gh attestation verify "$STABLE" --repo "$REPO" --format json
probe "+ signer-workflow" 0 \
  gh attestation verify "$STABLE" --repo "$REPO" --signer-workflow "$SIGNER" --format json
probe "+ signer-workflow WRONG (ci.yml)" 1 \
  gh attestation verify "$STABLE" --repo "$REPO" \
  --signer-workflow "$REPO/.github/workflows/ci.yml" --format json
probe "+ source-ref WRONG (heads/main)" 1 \
  gh attestation verify "$STABLE" --repo "$REPO" --signer-workflow "$SIGNER" \
  --source-ref refs/heads/main --format json
probe "+ source-digest WRONG (zeroes)" 1 \
  gh attestation verify "$STABLE" --repo "$REPO" --signer-workflow "$SIGNER" \
  --source-digest 0000000000000000000000000000000000000000 --format json

# One appended byte changes the digest the lookup is keyed on, so the tool
# itself binds the attestation to the artifact's bytes. Re-asserting the
# subject digest in the step would be redundant with this.
cp "$STABLE" "$WORK/tampered.tar.gz"
printf 'x' >> "$WORK/tampered.tar.gz"
probe "tampered artifact (one byte appended)" 1 \
  gh attestation verify "$WORK/tampered.tar.gz" --repo "$REPO" \
  --signer-workflow "$SIGNER" --format json

echo
echo "## 2. What the step prints when stdout is a pipe, not a terminal"
echo
# This is the whole of problem 2 in #1240: zero bytes on BOTH streams, exit 0.
probe "no --format (the shipped shape)" 0 \
  gh attestation verify "$STABLE" --repo "$REPO"
probe "--format json" 0 \
  gh attestation verify "$STABLE" --repo "$REPO" --format json

echo
echo "## 3. Both release lanes, with the identity the composite action enforces"
echo
JQ='.[].verificationResult | "  subject:   \(.statement.subject[0].name)\n  digest:    sha256:\(.statement.subject[0].digest.sha256)\n  predicate: \(.statement.predicateType)\n  signer:    \(.signature.certificate.buildSignerURI)\n  source:    \(.signature.certificate.sourceRepositoryURI)@\(.signature.certificate.sourceRepositoryRef)\n  commit:    \(.signature.certificate.sourceRepositoryDigest)\n  runner:    \(.signature.certificate.runnerEnvironment)"'

for lane in stable nightly; do
  case "$lane" in
    stable)  art="$STABLE";  ref=refs/tags/"$STABLE_TAG" ;;
    nightly) art="$NIGHTLY"; ref=refs/heads/main ;;
  esac
  # The commit is read back out of the attestation rather than asserted: the
  # point of this section is the ref asymmetry between the lanes, which is
  # why --source-ref cannot be a constant.
  digest="$(gh attestation verify "$art" --repo "$REPO" --signer-workflow "$SIGNER" \
    --format json --jq '.[0].verificationResult.signature.certificate.sourceRepositoryDigest')"
  echo "### $lane — ref=$ref commit=$digest"
  gh attestation verify "$art" \
    --repo "$REPO" \
    --signer-workflow "$SIGNER" \
    --source-ref "$ref" \
    --source-digest "$digest" \
    --predicate-type https://slsa.dev/provenance/v1 \
    --deny-self-hosted-runners \
    --format json --jq "$JQ"
  echo "    exit=$?"
done

echo
echo "## 3b. One invocation for both channels, via --cert-identity-regex"
echo
# docs/verifying-releases.md offers this to consumers who do not want to know
# which channel they are verifying. It is the same identity the release's own
# cosign self-check enforces.
RE='^https://github\.com/kusari-oss/waybill/\.github/workflows/release\.yml@refs/(tags/v.+|heads/main)$'
probe "cert-identity-regex, stable" 0 \
  gh attestation verify "$STABLE" --repo "$REPO" --cert-identity-regex "$RE" \
  --deny-self-hosted-runners --format json
probe "cert-identity-regex, nightly" 0 \
  gh attestation verify "$NIGHTLY" --repo "$REPO" --cert-identity-regex "$RE" \
  --deny-self-hosted-runners --format json
probe "cert-identity-regex pinned to ci.yml" 1 \
  gh attestation verify "$STABLE" --repo "$REPO" \
  --cert-identity-regex '^https://github\.com/kusari-oss/waybill/\.github/workflows/ci\.yml@' \
  --format json

echo
echo "## 4. The OCI subject's shape (needs a ghcr.io login)"
echo
if gh attestation verify "oci://ghcr.io/kusari-oss/waybill:$STABLE_TAG" \
     --repo "$REPO" --signer-workflow "$SIGNER" --deny-self-hosted-runners \
     --format json --jq "$JQ" 2>"$WORK/oci.err"; then
  echo "    exit=0"
else
  echo "    SKIPPED or FAILED: $(head -1 "$WORK/oci.err" | cut -c1-120)"
fi
