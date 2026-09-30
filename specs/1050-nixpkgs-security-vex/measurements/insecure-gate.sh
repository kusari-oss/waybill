#!/usr/bin/env bash
# Is `permittedInsecurePackages` observable from a scan?
#
# It turns out not to matter, which is the finding. Nix refuses to
# EVALUATE a package carrying `meta.knownVulnerabilities` unless it has
# been permitted, so a derivation for one cannot reach a closure without
# permission having been granted. Presence IS the acceptance signal;
# waybill never has to locate the config that granted it.
#
# Pick a package that is insecure AND builds on the host platform. An
# earlier run of this used `checkinstall`, which is Linux-only, and got an
# "unsupported for this system" refusal that reads like a confirmation if
# you only check that the command failed. Assert on the MESSAGE.
set -uo pipefail

PKGS=("${@:-cypress alist}")
for p in ${PKGS[@]}; do
  echo "--- nixpkgs#$p"
  out=$(nix derivation show --option allow-import-from-derivation false \
        "nixpkgs#$p" 2>&1 || true)
  if grep -q "because it is marked as insecure" <<<"$out"; then
    echo "    REFUSED AT EVAL: marked insecure (permission required)"
    grep -o "Refusing to evaluate package '[^']*'" <<<"$out" | head -1 | sed 's/^/    /'
  elif grep -q "unsupported for this system" <<<"$out"; then
    echo "    INCONCLUSIVE: unsupported on this platform, not the insecure gate"
  elif [ -n "$out" ] && grep -q '"outputs"' <<<"$out"; then
    echo "    INSTANTIATED: either not insecure, or already permitted here"
  else
    echo "    OTHER FAILURE: $(head -2 <<<"$out" | tr '\n' ' ')"
  fi
done
