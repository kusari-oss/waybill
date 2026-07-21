#!/usr/bin/env bash
# scripts/corpus/regen-goldens.sh — m195 T045 (US5).
#
# Wrapper for running the public-corpus test suite in golden-regen
# mode. Matches the quickstart.md Reproducer 4 UX and keeps the
# `WAYBILL_UPDATE_*_GOLDENS=1` naming convention consistent across
# all waybill golden-regression suites.
#
# Usage:
#   ./scripts/corpus/regen-goldens.sh              # regen all targets
#   ./scripts/corpus/regen-goldens.sh corpus_go_cobra    # single target
#
# Requires: cargo, git, docker (for OCI targets).

set -euo pipefail

WAYBILL_RUN_PUBLIC_CORPUS=1 \
WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1 \
  cargo test --test public_corpus --release -- --nocapture "$@"

echo ""
echo "==> Done. Review the diff under waybill-cli/tests/fixtures/public_corpus/"
echo "    before committing; every diff should be consistent with the"
echo "    intentional waybill behavior change that motivated the regen"
echo "    (per spec FR-008)."
