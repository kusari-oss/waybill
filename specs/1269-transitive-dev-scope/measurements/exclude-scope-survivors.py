#!/usr/bin/env python3
"""T043 — what `--exclude-scope <s>` left behind.

Reads the pair of CycloneDX documents `probe-exclude-scope.sh` writes
and reports, for the excluded scope:

  * how many components carried it in the baseline, split by what
    assigned it (reader-declared vs C62 `transitive` vs the Go readers'
    own `test-only-closure` / `go-mod-why`);
  * how many of each survived the filter.

The split is the whole point. A count of total drops cannot tell a
correctly-placed filter from one that ran before the propagation pass,
because in the second case the components it should have dropped come
out carrying **no scope at all** — the filter removed the seeds Phase B
propagates from, so the scope was never assigned. On the wire a missing
`waybill:lifecycle-scope` is indistinguishable from a runtime
dependency, which is why "nothing is scoped `test` any more" reads as
success in both worlds and only the per-derivation survivor count
separates them.

Usage: exclude-scope-survivors.py <probe-output-dir> <dev|build|test>
"""
import collections
import json
import sys

SCOPE = "waybill:lifecycle-scope"
DERIVATION = "waybill:lifecycle-scope-derivation"
# `--exclude-scope` spells them short; the emitted property does not.
FLAG_TO_PROPERTY = {"dev": "development", "build": "build", "test": "test"}


def load(path):
    doc = json.load(open(path))
    rows = []
    for component in doc.get("components", []):
        props = {p["name"]: p["value"] for p in component.get("properties", [])}
        rows.append((component.get("purl"), props.get(SCOPE), props.get(DERIVATION)))
    edges = sum(len(d.get("dependsOn") or []) for d in doc.get("dependencies", []))
    return rows, edges


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    out_dir, flag = sys.argv[1], sys.argv[2]
    scope = FLAG_TO_PROPERTY.get(flag, flag)

    base, base_edges = load(f"{out_dir}/baseline.cdx.json")
    excl, excl_edges = load(f"{out_dir}/excl.cdx.json")
    present = collections.Counter(r[0] for r in excl)

    print(f"== {out_dir}   --exclude-scope {flag}  (scope property {scope!r})")
    print(f"baseline : {len(base):>5} components  {base_edges:>5} edges")
    print(f"excluded : {len(excl):>5} components  {excl_edges:>5} edges")
    print(f"baseline scope histogram: "
          f"{dict(collections.Counter(r[1] for r in base))}")
    print(f"excluded scope histogram: "
          f"{dict(collections.Counter(r[1] for r in excl))}")

    by_derivation = collections.defaultdict(set)
    for purl, got, derivation in base:
        if got == scope:
            by_derivation[derivation].add(purl)

    print(f"\ncomponents scoped {scope!r} in the baseline, by what assigned it:")
    survivors_total = 0
    for derivation in sorted(by_derivation, key=lambda d: (d is None, d)):
        members = by_derivation[derivation]
        survivors = sorted(p for p in members if present[p])
        survivors_total += len(survivors)
        label = "reader-declared (no marker)" if derivation is None else f"C62 {derivation!r}"
        print(f"  {label:<34} {len(members):>4}  survived the filter: {len(survivors)}")
        for purl in survivors[:10]:
            print(f"        SURVIVOR {purl}")
        if len(survivors) > 10:
            print(f"        ... and {len(survivors) - 10} more")

    print(f"\ntotal survivors: {survivors_total}  (0 is the only passing value)")
    return 0 if survivors_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
