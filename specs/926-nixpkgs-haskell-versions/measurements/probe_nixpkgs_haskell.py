#!/usr/bin/env python3
"""Probe: what does a pinned nixpkgs revision actually tell us about
Haskell package versions?

Committed per CLAUDE.md ("Measure external behaviour before designing
around it") so every number in spec.md is traceable to an observation and
re-runnable when the nixpkgs layout moves.

Two questions, both answered against a live revision:

  1. Does `pkgs/development/haskell-modules/hackage-packages.nix` at a
     pinned rev resolve package name -> (version, source hash)?
  2. Do the per-compiler configuration files null out boot libraries, so
     that a GHC-specific package set genuinely has no version there?

Usage:
    python3 probe_nixpkgs_haskell.py [--rev REV] [--packages a,b,c]

Network: two raw.githubusercontent.com fetches (~16 MB + ~30 KB).
"""

import argparse
import json
import re
import sys
import urllib.request

RAW = "https://raw.githubusercontent.com/NixOS/nixpkgs/{rev}/{path}"
HACKAGE_PATH = "pkgs/development/haskell-modules/hackage-packages.nix"
GHC_CONFIG_PATH = "pkgs/development/haskell-modules/configuration-ghc-{series}.nix"

# The 19 dependencies declared by the measurement target in #947 (a public
# cabal + hpack + Nix Haskell library). Named here, not the repository, per
# the project's external-name policy.
DEFAULT_PACKAGES = [
    "aeson", "base", "bytestring", "cmark-gfm", "containers", "deepseq",
    "errors", "http-api-data", "http-client", "mtl", "scientific",
    "template-haskell", "text", "th-compat", "time", "transformers",
    "uuid-types", "vector", "vector-algorithms",
]

# `mkDerivation { pname = "x"; version = "y"; ... sha256 = "z"; }`
DERIV_RE = re.compile(
    r'pname\s*=\s*"(?P<pname>[^"]+)"\s*;\s*'
    r'version\s*=\s*"(?P<version>[^"]+)"\s*;'
    r'(?P<rest>.{0,400}?)'
    r'sha256\s*=\s*"(?P<sha256>[^"]+)"\s*;',
    re.DOTALL,
)


def fetch(url: str) -> str:
    with urllib.request.urlopen(url, timeout=180) as r:
        return r.read().decode("utf-8", errors="replace")


def parse_hackage(text: str) -> dict:
    """name -> {version, sha256}. Last definition wins, matching the Nix
    attrset semantics where a later binding shadows an earlier one."""
    out = {}
    for m in DERIV_RE.finditer(text):
        out[m.group("pname")] = {
            "version": m.group("version"),
            "sha256": m.group("sha256"),
        }
    return out


def parse_nulled_boot_libs(text: str) -> set:
    """Attribute names bound to `null` in a per-compiler configuration.

    `null` means 'ships with the compiler, do not build from Hackage', so
    the package has no hackage-packages.nix version for that package set.
    """
    return set(re.findall(r"^\s*([A-Za-z][A-Za-z0-9_'-]*)\s*=\s*null\s*;", text, re.M))


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--rev", default="a799d3e3886da994fa307f817a6bc705ae538eeb",
                    help="nixpkgs revision, as pinned by the target's flake.lock")
    ap.add_argument("--series", default="9.6.x", help="GHC series config to read")
    ap.add_argument("--packages", default=",".join(DEFAULT_PACKAGES))
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    pkgs = [p.strip() for p in args.packages.split(",") if p.strip()]

    hp_text = fetch(RAW.format(rev=args.rev, path=HACKAGE_PATH))
    index = parse_hackage(hp_text)

    cfg_text = fetch(RAW.format(
        rev=args.rev, path=GHC_CONFIG_PATH.format(series=args.series)))
    nulled = parse_nulled_boot_libs(cfg_text)

    resolved, boot, missing = {}, [], []
    for p in pkgs:
        if p in nulled:
            boot.append(p)
        elif p in index:
            resolved[p] = index[p]
        else:
            missing.append(p)

    result = {
        "rev": args.rev,
        "ghc_series": args.series,
        "hackage_packages_nix_bytes": len(hp_text.encode()),
        "derivations_parsed": len(index),
        "declared": len(pkgs),
        "resolved": len(resolved),
        "boot_libraries_nulled": sorted(boot),
        "not_found": sorted(missing),
        "sample": dict(sorted(resolved.items())[:6]),
    }

    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0

    print(f"nixpkgs rev              {result['rev']}")
    print(f"hackage-packages.nix     {result['hackage_packages_nix_bytes']:,} bytes")
    print(f"derivations parsed       {result['derivations_parsed']:,}")
    print(f"GHC series config        {result['ghc_series']} "
          f"({len(nulled)} attrs bound to null)")
    print()
    print(f"declared dependencies    {result['declared']}")
    print(f"  resolved from nixpkgs  {result['resolved']}")
    print(f"  GHC boot (nulled)      {len(result['boot_libraries_nulled'])}  "
          f"{result['boot_libraries_nulled']}")
    print(f"  not found              {len(result['not_found'])}  {result['not_found']}")
    print()
    for name, info in sorted(resolved.items())[:8]:
        print(f"  {name:22} {info['version']:12} sha256={info['sha256'][:18]}...")
    return 0


if __name__ == "__main__":
    sys.exit(main())
