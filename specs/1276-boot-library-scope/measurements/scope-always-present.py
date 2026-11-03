#!/usr/bin/env python3
"""#1340 groundwork — who carries a lifecycle scope, and who is always present.

#1340 says it "needs a decision rather than a patch", and names the open
question: does exempting GHC boot libraries from optional propagation imply
the same for other "always present" classes — OS packages in a rootfs scan,
language stdlibs collapsed by the file tier?

This answers it from the committed corpus goldens, with no scans. It reports
BOTH the optional/excluded population and the no-scope population per class,
because a class that never carries a scope at all is not a participant in the
question, and a bare zero cannot tell those two apart
(`feedback_zero_results_can_mean_broken`).

Usage:  python3 -I scope-always-present.py [<repo root>]
"""
import collections
import glob
import json
import os
import sys

root = sys.argv[1] if len(sys.argv) > 1 else "."
pattern = os.path.join(root, "waybill-cli/tests/fixtures/public_corpus/*/cdx.json")

OS_PREFIXES = ("pkg:apk/", "pkg:deb/", "pkg:rpm/", "pkg:alpine/", "pkg:opkg/")


def classify(component):
    """The 'always present' class a component belongs to, or None."""
    props = {p["name"]: p["value"] for p in component.get("properties", [])}
    if props.get("waybill:haskell-version-unresolved-reason") == "compiler-supplied":
        return "haskell-boot"
    if component.get("purl", "").startswith(OS_PREFIXES):
        return "os-package"
    if props.get("waybill:component-tier") == "file":
        return "file-tier"
    return None


def scope_of(component):
    props = {p["name"]: p["value"] for p in component.get("properties", [])}
    return component.get("scope") or props.get("waybill:lifecycle-scope") or "no-scope"


by_class = collections.Counter()
by_target = collections.defaultdict(collections.Counter)
scoped_total = 0

files = sorted(glob.glob(pattern))
if not files:
    sys.exit(f"no corpus goldens under {pattern}")

for path in files:
    target = os.path.basename(os.path.dirname(path))
    doc = json.load(open(path))
    for component in doc.get("components", []):
        scope = scope_of(component)
        if scope in ("optional", "excluded"):
            scoped_total += 1
        cls = classify(component)
        if cls:
            by_class[(cls, scope)] += 1
            if scope in ("optional", "excluded"):
                by_target[target][cls] += 1

print(f"corpus targets read: {len(files)}")
print(f"components carrying optional/excluded, all classes: {scoped_total}\n")
print(f"{'class':14}{'scope':14}{'n':>6}")
for (cls, scope), n in sorted(by_class.items()):
    print(f"{cls:14}{scope:14}{n:>6}")
print("\nthe defect population, by target:")
for target, counts in sorted(by_target.items()):
    print(f"  {target:30} {dict(counts)}")
if not by_target:
    print("  none")
