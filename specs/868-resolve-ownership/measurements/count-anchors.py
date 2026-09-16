#!/usr/bin/env python3
"""Declared requirements vs derived graph roots, per Pants resolve.

Run from a checkout of the measured target:

    python3 count-anchors.py /path/to/backend.ai

Supports the argument in specs/868-resolve-ownership/measurements/README.md
§"Deviation from tasks.md T013": the lockfile's declared `requirements` array
names more anchors than deriving "packages nothing else depends on", and the
derived answer is not stable under marker-evaluation policy.
"""
import json
import os
import re
import sys


def strip_pants_front_matter(raw: bytes) -> str:
    """Pants <= 2.29 prefixes the JSON body with `//` comment lines."""
    return "\n".join(
        l for l in raw.decode("utf-8", "replace").splitlines() if not l.startswith("//")
    )


def normalize(name: str) -> str:
    return re.sub(r"[-_.]+", "-", name).lower()


def project_name(req: str) -> str:
    out = []
    for c in req.strip():
        if c in " \t[(<>=!~;,":
            break
        out.append(c)
    return normalize("".join(out))


def main(root: str) -> int:
    resolves = {}
    for line in open(os.path.join(root, "pants.toml")):
        m = re.match(r'^([a-z0-9-]+)\s*=\s*"([^"]+\.lock)"\s*$', line.strip())
        if m:
            resolves[m.group(1)] = m.group(2)

    totals = [0, 0, 0]
    rows = []
    for name, rel in resolves.items():
        path = os.path.join(root, rel)
        if not os.path.exists(path):
            rows.append((name, 0, 0, 0, 0))
            continue
        doc = json.loads(strip_pants_front_matter(open(path, "rb").read()))
        declared = {project_name(r) for r in doc.get("requirements", []) if project_name(r)}
        locked = {}
        for lr in doc.get("locked_resolves", []):
            for p in lr.get("locked_requirements", []):
                locked[normalize(p["project_name"])] = p.get("requires_dists", [])

        depended_naive, depended_no_extras = set(), set()
        for deps in locked.values():
            for d in deps:
                n = project_name(d)
                if not n:
                    continue
                depended_naive.add(n)
                if "extra ==" not in d:
                    depended_no_extras.add(n)

        roots_naive = {p for p in locked if p not in depended_naive}
        roots_no_extras = {p for p in locked if p not in depended_no_extras}
        rows.append((name, len(declared), len(roots_naive), len(roots_no_extras), len(locked)))
        totals[0] += len(declared)
        totals[1] += len(roots_naive)
        totals[2] += len(roots_no_extras)

    print(f"{'resolve':<16}{'declared':>9}{'roots(naive)':>14}{'roots(no-extras)':>18}{'locked':>8}")
    for r in rows:
        print(f"{r[0]:<16}{r[1]:>9}{r[2]:>14}{r[3]:>18}{r[4]:>8}")
    print(f"{'TOTAL':<16}{totals[0]:>9}{totals[1]:>14}{totals[2]:>18}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "."))
