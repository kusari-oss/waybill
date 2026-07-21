#!/usr/bin/env bash
# Install JPEWdev's `spdx3-validate` Python tool into a project-local
# virtualenv at .venv/spdx3-validate/. Pinned per
# specs/078-spdx3-conformance/research.md §2.
#
# Usage:
#   bash scripts/install-spdx3-validate.sh
#
# Idempotent: re-running is a no-op when the venv already has the
# pinned version installed. Prints the binary path to stdout on
# success so CI / shell substitution callers can use it directly:
#
#   "$(bash scripts/install-spdx3-validate.sh)" -j out.spdx3.json
#
# This is the install step the .github/workflows/ci.yml `Lint + test
# (linux-x86_64)` job invokes before `cargo test --workspace` runs the
# `spdx3_conformance` integration test (with
# WAYBILL_REQUIRE_SPDX3_VALIDATOR=1 set so absent-binary is a hard
# failure on CI). Local-dev callers can skip this step; the
# integration test gracefully skips when the binary is absent and
# WAYBILL_REQUIRE_SPDX3_VALIDATOR is unset (research §5).

set -euo pipefail

# Resolve to repo root so the script works from any cwd.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

VENV_DIR="$REPO_ROOT/.venv/spdx3-validate"
BIN_PATH="$VENV_DIR/bin/spdx3-validate"

# Pinned version per research §2. Bumping requires a deliberate PR
# with proof the new version doesn't surface false positives against
# post-fix waybill output (FR-008).
PINNED_VERSION="0.0.5"

# Pick a Python interpreter. Prefer python3; fall back to python.
PYTHON_BIN="${PYTHON:-}"
if [[ -z "$PYTHON_BIN" ]]; then
    if command -v python3 >/dev/null 2>&1; then
        PYTHON_BIN="python3"
    elif command -v python >/dev/null 2>&1; then
        PYTHON_BIN="python"
    else
        printf >&2 'install-spdx3-validate: no python3/python on PATH; install Python 3.10+ first.\n'
        exit 1
    fi
fi

# Idempotent fast-path: if the venv exists AND the pinned version is
# already installed, just print the binary path and exit. The
# `--version` substring check is intentional — see research §3 on
# validator output formatting (we do not pin to an exact equality
# format).
if [[ -x "$BIN_PATH" ]]; then
    if "$BIN_PATH" --version 2>&1 | grep -qF "$PINNED_VERSION"; then
        printf '%s\n' "$BIN_PATH"
        exit 0
    fi
    printf >&2 'install-spdx3-validate: existing venv has the wrong version; recreating.\n'
    rm -rf "$VENV_DIR"
fi

printf >&2 'install-spdx3-validate: creating venv at %s\n' "$VENV_DIR"
"$PYTHON_BIN" -m venv "$VENV_DIR"

printf >&2 'install-spdx3-validate: installing spdx3-validate==%s\n' "$PINNED_VERSION"
"$VENV_DIR/bin/pip" install --quiet --upgrade pip
"$VENV_DIR/bin/pip" install --quiet "spdx3-validate==$PINNED_VERSION"

if [[ ! -x "$BIN_PATH" ]]; then
    printf >&2 'install-spdx3-validate: install completed but binary not found at %s\n' "$BIN_PATH"
    exit 1
fi

printf >&2 'install-spdx3-validate: ready at %s\n' "$BIN_PATH"
printf '%s\n' "$BIN_PATH"
