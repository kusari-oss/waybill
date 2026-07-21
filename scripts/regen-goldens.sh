#!/usr/bin/env bash
# Regenerate every byte-identity golden the workspace test suite
# can produce.
#
# Runs the WHOLE workspace test suite under the three documented
# `WAYBILL_UPDATE_*` env vars at once. Each affected test honors the
# env var by writing its own pinned golden in place; everything else
# just runs normally.
#
# Why workspace-wide instead of `--test cdx_regression --test
# spdx_regression --test spdx3_regression`:
#
#   The three regression suites cover the main `tests/fixtures/golden/`
#   tree, but per-test pinned goldens elsewhere (e.g.
#   `tests/fixtures/pkg_alias_binding/image-baz.cdx.json` from the
#   milestone-111 byte-identity regression) honor the SAME env vars
#   and live outside that tree. Narrowing cargo to the three suites
#   silently skips those. See https://github.com/kusari-oss/waybill/issues/361.
#
# Usage:
#   ./scripts/regen-goldens.sh
#       Regenerate every CDX / SPDX 2.3 / SPDX 3 golden the
#       workspace can produce. Run this from a clean working tree
#       so the resulting `git diff` only contains the intended
#       golden churn.
#
# Exits non-zero if cargo fails.

set -euo pipefail

printf '>>> regenerating all byte-identity goldens via workspace test sweep\n'

WAYBILL_UPDATE_CDX_GOLDENS=1 \
WAYBILL_UPDATE_SPDX_GOLDENS=1 \
WAYBILL_UPDATE_SPDX3_GOLDENS=1 \
    cargo +stable test --workspace --no-fail-fast >/dev/null

printf '\n>>> regen sweep done. Review `git status` + `git diff --stat`\n'
printf '    to confirm only intended goldens churned.\n'
