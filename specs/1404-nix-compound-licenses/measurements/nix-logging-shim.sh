#!/usr/bin/env bash
# A `nix` that records every argv waybill hands it, then runs the real one.
#
# Placed first on PATH as `nix`, it lets probe.sh read the exact attribute
# names the m1281 license evaluation asks for, instead of re-deriving them
# from the closure (a re-derivation would model the reader and drift).
#
# NIX_SHIM_LOG: file to append to; one record per invocation, args
#               NUL-separated, record terminated by a lone "\x1e".
# NIX_REAL:     the real nix binary.
set -u
{
  for a in "$@"; do printf '%s\0' "$a"; done
  printf '\036'
} >>"${NIX_SHIM_LOG:?}"
exec "${NIX_REAL:?}" "$@"
