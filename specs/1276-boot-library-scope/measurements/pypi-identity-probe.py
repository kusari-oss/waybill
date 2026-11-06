#!/usr/bin/env python3
"""Which nix-closure components could carry a real ecosystem PURL? (#1365)

waybill emits 781 of 942 components for the pinned `nix-closure-moat`
closure as `pkg:generic`. A feed keys on an ecosystem-qualified PURL, so
none of them is answerable to a vulnerability query. This probe decomposes
the 781 and sizes the subset a direct read of the closure can fix.

Usage:
    nix derivation show --recursive <flakeref>#default > closure.json
    python3 -I pypi-identity-probe.py closure.json [waybill.cdx.json]

Two things this probe learned the hard way, both recorded because each
produced a confident wrong number first:

* `pname` lives in the flat `env` map on most derivations and in the
  **top-level `structuredAttrs`** carrier on others -- 66 of 1,275 on the
  moat closure. Reading only `env` counts those as nameless.
* Output paths in `derivation show` output are bare basenames while `src`
  carries a full `/nix/store/` path, so matching one against the other
  without normalising finds nothing at all.
"""
import json
import re
import sys
import collections

VERSIONED_PY = re.compile(r"^python3\.\d+-")


def attr(drv, key):
    """A derivation attribute from whichever carrier holds it."""
    env = drv.get("env", {}) or {}
    if key in env:
        return env[key]
    structured = drv.get("structuredAttrs")
    if isinstance(structured, dict) and key in structured:
        return structured[key]
    return None


def pep503(name):
    """PyPI's normalised project name."""
    return re.sub(r"[-_.]+", "-", str(name)).lower()


def pypi_candidates(closure):
    """Derivations a direct read says are PyPI packages.

    The discriminator is the **versioned** interpreter prefix on the
    derivation's `name` (`python3.13-urllib3`) while `pname` carries the
    bare project name (`urllib3`). Both halves are load-bearing:

    * A bare `python-` prefix matches nixpkgs setup hooks
      (`python-relax-deps-hook`, `python-namespaces-hook.sh`), which are
      build machinery and not packages. Twenty of them on this closure.
    * nixpkgs' bootstrap variants carry the prefix in `pname` itself
      (`python3.13-bootstrap-tomli`). They are not the PyPI project and
      the second clause drops all six.
    """
    out = []
    for key, drv in closure.items():
        name = drv.get("name") or ""
        pname = attr(drv, "pname")
        if not pname or not VERSIONED_PY.match(name):
            continue
        if str(pname).startswith("python3"):
            continue
        out.append((key, pname, attr(drv, "version")))
    return out


def main():
    raw = json.load(open(sys.argv[1]))
    closure = raw["derivations"] if "derivations" in raw else raw
    print(f"closure derivations: {len(closure)}")

    carriers = collections.Counter()
    for drv in closure.values():
        in_env = "pname" in (drv.get("env", {}) or {})
        structured = drv.get("structuredAttrs")
        in_sa = isinstance(structured, dict) and "pname" in structured
        carriers[(in_env, in_sa)] += 1
    print("pname carrier (env, structuredAttrs):")
    for k, v in sorted(carriers.items()):
        print(f"   {k}  {v}")

    cands = pypi_candidates(closure)
    names = sorted({p for _, p, _ in cands})
    print(f"\nPyPI candidates: {len(cands)} derivations, {len(names)} distinct names")
    unnormalised = [n for n in names if n != pep503(n)]
    print(f"  needing PEP 503 normalisation: {len(unnormalised)} {unnormalised}")
    print("  names:")
    for i in range(0, len(names), 6):
        print("    " + "  ".join(f"{n:22s}" for n in names[i : i + 6]))

    if len(sys.argv) > 2:
        doc = json.load(open(sys.argv[2]))
        comps = doc.get("components", [])
        types = collections.Counter(
            (c.get("purl") or "(none)").split("/")[0] for c in comps
        )
        print(f"\nemitted document: {len(comps)} components")
        print(f"  PURL types: {dict(types)}")
        members = sum(
            1
            for c in comps
            if any(
                p["name"] == "waybill:closure-member-role"
                for p in (c.get("properties") or [])
            )
        )
        print(f"  carrying a closure member role: {members}")
        print(
            "  (a member is a patch file, source tarball or build fragment;\n"
            "   `pkg:generic` is correct for those and changing them is a regression)"
        )


if __name__ == "__main__":
    main()
