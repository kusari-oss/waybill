#!/usr/bin/env python3
"""Classify a `nix derivation show -r` closure by SBOM relevance.

Answers the question "what would a full-closure SBOM actually contain?",
which is the substance of issues #1034 (closure depth) and #1040 (VEX for
backported patches). Run it before deciding what to filter:

    nix derivation show -r .#default > closure.json
    python3 classify-derivation-closure.py closure.json

Two things the output is meant to settle:

  * the split between packages that go into the artifact and tooling that
    only builds it -- nix records this, so it need not be guessed; and
  * whether the remainder is safely discardable. It is not: patch
    derivations live there, and some carry CVE identifiers in their names.
"""
import collections
import json
import re
import sys

ARTIFACT_FIELDS = ("buildInputs", "propagatedBuildInputs", "depsHostHost")
TOOLING_FIELDS = (
    "nativeBuildInputs",
    "depsBuildBuild",
    "depsBuildHost",
    "nativeCheckInputs",
)


def load(path):
    with open(path) as fh:
        return json.load(fh)["derivations"]


def classify(drvs):
    # Output paths are stored WITHOUT the /nix/store/ prefix while the env
    # fields carry it; compare basenames or nothing matches.
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
                for path in str(env.get(field, "")).split():
                    key = out2drv.get(path.split("/")[-1])
                    if key:
                        sink.add(key)
    return tooling, artifact


def name_of(drv):
    return str(drv.get("env", {}).get("pname") or drv.get("name", "?"))


def main(path):
    drvs = load(path)
    tooling, artifact = classify(drvs)

    counts = collections.Counter()
    for key in drvs:
        in_t, in_a = key in tooling, key in artifact
        counts[
            "artifact input"
            if in_a and not in_t
            else "build tooling only"
            if in_t and not in_a
            else "both"
            if in_a and in_t
            else "neither"
        ] += 1

    print(f"{path}: {len(drvs)} derivations")
    for label, n in counts.most_common():
        print(f"  {label:22} {n:5}")

    rest = [k for k in drvs if k not in tooling and k not in artifact]
    shapes = collections.Counter()
    for key in rest:
        name = name_of(drvs[key])
        if name.endswith(".patch"):
            shapes["patches"] += 1
        elif name.endswith((".tar.gz", ".tar.xz", ".zip")) or name.endswith("-source"):
            shapes["fetched sources"] += 1
        elif "hook" in name:
            shapes["setup hooks"] += 1
        elif name.startswith("bootstrap") or re.search(r"stage[0-2]", name):
            shapes["bootstrap toolchain"] += 1
        else:
            shapes["other"] += 1
    print(f"\n  the {len(rest)} in 'neither', by shape:")
    for label, n in shapes.most_common():
        print(f"    {label:22} {n:5}")

    cve = sorted({name_of(drvs[k]) for k in drvs if re.search(r"CVE-\d{4}-\d+", name_of(drvs[k]))})
    print(f"\n  derivations naming a CVE: {len(cve)}")
    for name in cve:
        print(f"    {name}")
    print(
        "\n  Those are backported security fixes applied to a version string that\n"
        "  does not change. Version-based vulnerability matching is wrong for\n"
        "  nixpkgs in both directions, which is what issue #1040 is about --\n"
        "  so the 'neither' bucket is not safely discardable as build noise."
    )


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "closure.json")
