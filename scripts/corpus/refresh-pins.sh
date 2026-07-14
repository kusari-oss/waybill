#!/usr/bin/env bash
# scripts/corpus/refresh-pins.sh — m195 T039 (US4).
#
# Resolves the current upstream SHA / digest for each pinned corpus
# target and prints a unified diff of proposed manifest changes.
# Does NOT auto-commit — per FR-008, invariant updates must land
# alongside any mikebom behavior change that motivated them.
#
# Usage:
#   ./scripts/corpus/refresh-pins.sh
#
# Requires: git, docker (for OCI targets), grep, awk.

set -euo pipefail

MANIFEST="mikebom-cli/tests/corpus_harness_195/manifest.rs"

if [[ ! -f "$MANIFEST" ]]; then
  echo "error: manifest not found at $MANIFEST" >&2
  echo "hint: run from the mikebom repo root" >&2
  exit 2
fi

echo "==> scanning $MANIFEST for pinned targets..."

# Git targets — line-based extraction. Each entry spans multiple lines
# so we grep-and-neighbor. Format expected:
#   source: SourceKind::Git { clone_url: "https://github.com/<org>/<repo>" },
#   pinned: PinnedRef::Sha { ... hex: "<40-hex>" }
#   ... and a comment line above the hex naming the tag.
grep -nE '(clone_url|hex): "' "$MANIFEST" | \
  awk 'BEGIN {url=""; tag=""}
    /clone_url:/ {
      match($0, /"[^"]*"/); url=substr($0, RSTART+1, RLENGTH-2)
    }
    /hex:/ {
      match($0, /"[^"]*"/); pinned=substr($0, RSTART+1, RLENGTH-2)
      if (url != "" && pinned != "") {
        printf "GIT|%s|%s\n", url, pinned
        url=""
      }
    }
  ' | while IFS='|' read -r kind url pinned; do
    echo ""
    echo "  target: $url"
    echo "    current pin: $pinned"
    latest=$(git ls-remote --heads --tags "$url" 2>/dev/null | awk '$2 ~ /refs\/tags\/(v?[0-9]+\.[0-9]+\.[0-9]+)$/ {print $1, $2}' | sort -k2 -V | tail -1)
    if [[ -n "$latest" ]]; then
      lsha=$(echo "$latest" | awk '{print $1}')
      ltag=$(echo "$latest" | awk '{print $2}' | sed 's|refs/tags/||')
      if [[ "$pinned" != "$lsha" ]]; then
        echo "    ↑ proposed:  $lsha  ($ltag)"
      else
        echo "    ✓ up to date ($ltag)"
      fi
    else
      echo "    ⚠ could not resolve latest tag"
    fi
  done

echo ""
echo "==> OCI-image targets (docker manifest inspect):"

grep -nE '(image_ref|algo_hex): "' "$MANIFEST" | \
  awk 'BEGIN {img=""; pin=""}
    /image_ref:/ {
      match($0, /"[^"]*"/); img=substr($0, RSTART+1, RLENGTH-2)
    }
    /algo_hex:/ {
      match($0, /"[^"]*"/); pin=substr($0, RSTART+1, RLENGTH-2)
      if (img != "" && pin != "") {
        printf "OCI|%s|%s\n", img, pin
        img=""
      }
    }
  ' | while IFS='|' read -r kind img pin; do
    echo ""
    echo "  target: $img"
    echo "    current pin: $pin"
    if ! command -v docker >/dev/null 2>&1; then
      echo "    ⚠ docker not on PATH — skipping"
      continue
    fi
    ldigest=$(docker manifest inspect "$img" 2>/dev/null | grep -m1 '"digest"' | awk -F'"' '{print $4}' || true)
    if [[ -n "$ldigest" ]]; then
      if [[ "$pin" != "$ldigest" ]]; then
        echo "    ↑ proposed:  $ldigest"
      else
        echo "    ✓ up to date"
      fi
    else
      echo "    ⚠ could not resolve digest — is the image still published?"
    fi
  done

echo ""
echo "==> Done. To apply: edit $MANIFEST by hand and commit alongside any"
echo "    mikebom behavior change that motivates the pin bump (FR-008)."
echo "    Then regen goldens: ./scripts/corpus/regen-goldens.sh"
