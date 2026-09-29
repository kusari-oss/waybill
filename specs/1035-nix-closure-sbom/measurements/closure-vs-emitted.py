#!/usr/bin/env python3
"""How much of a Nix closure is content waybill does not already emit?

The question SC-001 rests on. Without it "the closure has more components" is
a direction, not a claim — the closure could be four times larger and still be
the same components counted differently.

    nix derivation show -r .#default > closure.json
    waybill sbom scan --path <project> --nix-eval --output sbom.cdx.json
    python3 closure-vs-emitted.py closure.json sbom.cdx.json

Both directions matter. Components in the closure and not the SBOM are what
this feature would add. Components in the SBOM and not the closure are the
harder finding: they suggest the closure of one attribute does not cover
everything a manifest declares, which decides whether closure emission replaces
the manifest-derived set or sits beside it.
"""
import json
import sys

TOOLING_FIELDS = ("nativeBuildInputs", "depsBuildBuild", "depsBuildHost", "nativeCheckInputs")
ARTIFACT_FIELDS = ("buildInputs", "propagatedBuildInputs", "depsHostHost")


def artifact_names(path):
    drvs = json.load(open(path))["derivations"]
    # Output paths are stored without the /nix/store/ prefix; env fields carry
    # it. Compare basenames or nothing matches.
    out2drv = {}
    for key, drv in drvs.items():
        for out in drv.get("outputs", {}).values():
            if out.get("path"):
                out2drv[out["path"].split("/")[-1]] = key

    tooling, artifact = set(), set()
    for drv in drvs.values():
        env = drv.get("env", {})
        for fields, sink in ((TOOLING_FIELDS, tooling), (ARTIFACT_FIELDS, artifact)):
            for field in fields:
                for p in str(env.get(field, "")).split():
                    key = out2drv.get(p.split("/")[-1])
                    if key:
                        sink.add(key)

    return {
        str(drvs[k].get("env", {}).get("pname") or drvs[k].get("name", "?"))
        for k in artifact
    }


def emitted_names(path, prefix="pkg:hackage/"):
    doc = json.load(open(path))
    return {
        c["name"]
        for c in doc.get("components", [])
        if str(c.get("purl", "")).startswith(prefix)
    }


def main(closure, sbom):
    a, e = artifact_names(closure), emitted_names(sbom)
    print(f"closure artifact inputs : {len(a)}")
    print(f"emitted today           : {len(e)}")
    print(f"overlap                 : {len(a & e)}")
    print(f"in closure, not emitted : {len(a - e)}   <- what this feature adds")
    print(f"emitted, not in closure : {len(e - a)}   <- why? (test/bench deps?)")
    if a - e:
        print(f"\n  sample added : {', '.join(sorted(a - e)[:8])}")
    if e - a:
        print(f"  sample absent: {', '.join(sorted(e - a)[:8])}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
