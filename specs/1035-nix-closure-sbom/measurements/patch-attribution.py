#!/usr/bin/env python3
"""Which component applies which patch, and which patches name a CVE.

Resolves research task T-R1. The closure records patches and components
separately; this is the join, and it is mechanical rather than heuristic —
each derivation's own `patches` field lists the store paths it applies.

    nix derivation show -r .#default > closure.json
    python3 patch-attribution.py closure.json

Why this exists rather than a scan of derivation names: scanning names finds 3
and 4 CVEs on the two measured projects, this finds 18 and 14. Many patches are
files referenced by store path, not separate derivations with CVE-shaped names.
An implementation that scans names undercounts fivefold and does so silently.
"""
import collections
import json
import re
import sys

CVE = re.compile(r"CVE-\d{4}-\d+")
TOOLING = ("nativeBuildInputs", "depsBuildBuild", "depsBuildHost", "nativeCheckInputs")
ARTIFACT = ("buildInputs", "propagatedBuildInputs", "depsHostHost")


def roles(drvs):
    out2drv = {}
    for key, drv in drvs.items():
        for out in drv.get("outputs", {}).values():
            if out.get("path"):
                out2drv[out["path"].split("/")[-1]] = key
    tooling, artifact = set(), set()
    for drv in drvs.values():
        env = drv.get("env", {})
        for fields, sink in ((TOOLING, tooling), (ARTIFACT, artifact)):
            for field in fields:
                for p in str(env.get(field, "")).split():
                    key = out2drv.get(p.split("/")[-1])
                    if key:
                        sink.add(key)

    def role_of(key):
        t, a = key in tooling, key in artifact
        return "tooling" if t and not a else "artifact" if a and not t else "both" if a else "neither"

    return role_of


def main(path):
    drvs = json.load(open(path))["derivations"]
    role_of = roles(drvs)

    appliers, all_cves = {}, set()
    patches_total = patches_without_cve = 0
    for key, drv in drvs.items():
        env = drv.get("env", {})
        paths = str(env.get("patches", "")).split()
        if not paths:
            continue
        name = str(env.get("pname") or drv.get("name", "?"))
        ids = set()
        for p in paths:
            patches_total += 1
            found = CVE.search(p.split("/")[-1])
            if found:
                ids.add(found.group(0))
            else:
                patches_without_cve += 1
        if ids:
            appliers[name] = (role_of(key), env.get("version", ""), sorted(ids))
            all_cves |= ids

    print(f"{path}")
    print(f"  derivations applying patches : {sum(1 for d in drvs.values() if d.get('env',{}).get('patches'))}")
    print(f"  patches total                : {patches_total}")
    print(f"  patches naming no CVE        : {patches_without_cve}")
    print(f"  distinct CVEs                : {len(all_cves)}")
    print("\n  by component:")
    for name, (role, version, ids) in sorted(appliers.items()):
        print(f"    {name:12} {version:10} role={role:9} cves={len(ids)}")
        print(f"      {', '.join(ids)}")
    print(
        "\n  Note the roles. The evidence does not cluster in the language's own\n"
        "  dependency graph — it is in the C utilities nixpkgs builds with, which\n"
        "  no manifest mentions. Scoping any role out of the document would drop\n"
        "  part of it."
    )


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
