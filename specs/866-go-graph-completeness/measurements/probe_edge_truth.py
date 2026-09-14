#!/usr/bin/env python3
"""Probe: does every emitted Go dependency edge correspond to a require
actually declared in some go.mod in the scanned tree?

A CycloneDX `dependencies[].dependsOn` entry states a DIRECT dependency.
When the Go module graph cannot be resolved, waybill knows the module
*set* from go.sum but not its topology, and attaching the whole set to
the main module asserts direct relationships that were never read from
any manifest. They are true transitively and false as stated.

This probe reads the scanned tree's go.mod files itself rather than
trusting any signal in the document — the document is the thing under
test.

Usage:
  probe_edge_truth.py <scanned-repo-path> <cdx.json>

Exit 1 if any emitted edge is not backed by a declared require.
"""
import json, re, sys, pathlib


def declared_requires(repo):
    """module path -> set of directly-required module paths, from every
    go.mod in the tree. Handles both `require (...)` blocks and
    single-line `require x v1`."""
    out = {}
    for gomod in pathlib.Path(repo).rglob("go.mod"):
        text = gomod.read_text(errors="replace")
        m = re.search(r"^module\s+(\S+)", text, re.M)
        if not m:
            continue
        owner, reqs = m.group(1), set()
        # Block form.
        for block in re.findall(r"^require\s*\((.*?)^\)", text, re.M | re.S):
            for line in block.splitlines():
                line = line.split("//")[0].strip()
                mm = re.match(r"(\S+)\s+v\S+", line)
                if mm:
                    reqs.add(mm.group(1))
        # Single-line form.
        for line in re.findall(r"^require\s+(\S+)\s+v\S+", text, re.M):
            reqs.add(line)
        out.setdefault(owner, set()).update(reqs)
    return out


def module_path(ref):
    """pkg:golang/github.com/x/y@v1 -> github.com/x/y"""
    if not ref.startswith("pkg:golang/"):
        return None
    return ref[len("pkg:golang/"):].split("@")[0]


def main(repo, cdx):
    declared = declared_requires(repo)
    d = json.load(open(cdx))
    root = d.get("metadata", {}).get("component", {}).get("bom-ref")
    deps = {e["ref"]: e.get("dependsOn", []) for e in d.get("dependencies", [])}

    # Which components carry the existing provenance marker, for
    # comparison against what is actually unbacked.
    marked = set()
    for c in d.get("components", []):
        ref = c.get("bom-ref") or c.get("purl")
        for p in c.get("properties", []) or []:
            if p["name"] == "waybill:orphan-reason":
                marked.add(ref)

    total = backed = synthetic = 0
    unbacked = []
    for src, targets in deps.items():
        if src == root:
            continue                      # root->mainmodule is a scan artefact
        owner = module_path(src)
        if owner is None:
            continue
        owned = declared.get(owner)
        if owned is None:
            continue                      # module's go.mod not in this tree
        for t in targets:
            tp = module_path(t)
            if tp is None:
                continue
            total += 1
            if tp == "stdlib":
                synthetic += 1
            elif tp in owned:
                backed += 1
            else:
                unbacked.append((owner, tp, t in marked))

    print(f"go.mod files parsed        : {len(declared)}")
    print(f"emitted golang edges       : {total}")
    print(f"  backed by a declared require : {backed}")
    print(f"  synthetic stdlib node        : {synthetic}")
    print(f"  NOT backed (asserted, never read): {len(unbacked)}")
    if unbacked:
        print("\nedges asserting an undeclared direct dependency:")
        for owner, tp, was_marked in unbacked:
            flag = "" if was_marked else "   <- carries NO orphan-reason marker"
            print(f"  {owner} -> {tp}{flag}")
        mk = sum(1 for _, _, m in unbacked if m)
        print(f"\n{mk}/{len(unbacked)} unbacked edges point at a component carrying the")
        print("existing orphan-reason marker, so that marker does not identify them.")
    return 1 if unbacked else 0


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(sys.argv[1], sys.argv[2]))
