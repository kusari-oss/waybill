#!/usr/bin/env bash
# Pre-PR verification gate. Runs the two checks that CI runs in
# .github/workflows/ci.yml — clippy with --all-targets (so the
# `clippy::unwrap_used` deny on the cli crate root applies inside
# `#[cfg(test)]` modules too), the workspace test suite under cargo-nextest,
# and the doctests.
#
# Per CLAUDE.md: ALL must pass locally before any PR. A passing
# `cargo test -p waybill` alone is insufficient — clippy is not run,
# and `--all-targets` enforces lints on tests.
#
# Usage:
#   ./scripts/pre-pr.sh
#       Default lane — matches the `lint-and-test` CI job. Stable
#       toolchain only, no eBPF, no nightly required.
#
#   WAYBILL_PREPR_EBPF=1 ./scripts/pre-pr.sh
#       Opt-in eBPF lane — matches the `lint-and-test-ebpf` CI job.
#       Adds `--features ebpf-tracing` to clippy and both test steps.
#       Linux only (the optional aya/aya-log/libc deps are also
#       target-gated). Requires `cargo run -p xtask -- ebpf` to have
#       been run at least once if any test reaches into the kernel
#       artifact path. See specs/020-ebpf-feature-gate/.
#
# Exits non-zero on the first failing step.

set -euo pipefail

if [[ "${WAYBILL_PREPR_EBPF:-0}" == "1" ]]; then
    printf '>>> running pre-PR checks with --features ebpf-tracing (eBPF lane)\n'
    feature_args=(--features ebpf-tracing)
else
    feature_args=()
fi

# `${feature_args[*]:-}` rather than `${feature_args[*]}`: under `set -u`,
# bash 3.2 treats the expansion of an EMPTY array as an unbound variable and
# aborts. macOS still ships bash 3.2 (Apple froze it in 2007 over the GPLv3
# licence change), so without the `:-` this script dies on line 38 before
# running a single check — on the default, non-eBPF path, on every Mac.
# Bash 4.4+ (every Linux CI runner) is unaffected, which is why CI never
# caught it. See https://github.com/kusari-oss/waybill — reported 2026-09-04.
# The homoglyph and walker-audit gates run FIRST: each takes about a second,
# needs no toolchain, and catches something the compiler and clippy cannot
# see (a Cyrillic `a` in a fixture; a new `fn walk_*` missing from the
# allow-list). The walker audit is the same script CI runs, so local and CI
# cannot drift; before it was shared it lived only in CI, and a local-green
# PR still failed it (#1111).
# The held-temp-path check (#1727, spec 1660) is another fast, toolchain-free
# gate: a temp file dropped in the statement that takes its path leaves the
# path unreserved, which parallel tests can end up sharing.
steps=(
    "python3 scripts/check-homoglyphs.py"
    "bash scripts/check-walker-audit.sh"
    "python3 -I -B scripts/check-held-temp-paths.py"
    "cargo clippy --workspace --all-targets ${feature_args[*]:-} -- -D warnings"
    "cargo nextest run --workspace --no-fail-fast --color never ${feature_args[*]:-}"
    "cargo test --workspace --doc ${feature_args[*]:-}"
)

# Tests run under cargo-nextest (spec 1658, constitution 4.1.0): it runs test
# binaries in parallel, where `cargo test` ran them one at a time. Nextest does
# not run doctests, hence the separate `--doc` step. `--color never` keeps the
# `Summary` line free of escape codes, which CI forces on through
# CARGO_TERM_COLOR=always and which the gate's --expect could not match.
# There is deliberately no fallback to `cargo test`: a gate that silently runs
# a different command is a check that did not run.
if ! cargo nextest --version >/dev/null 2>&1; then
    echo "pre-pr.sh: cargo-nextest is required: brew install cargo-nextest, or cargo install cargo-nextest --locked" >&2
    exit 1
fi

for cmd in "${steps[@]}"; do
    printf '\n>>> %s\n' "$cmd"
    eval "$cmd"
done

printf '\n>>> all pre-PR checks passed.\n'
