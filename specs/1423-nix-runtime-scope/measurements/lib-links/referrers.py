"""#1455 lib/links rule — who refers to a package, and through which files.

usage: python3 -I -B referrers.py <pathinfo.json> <name-version>...

For each runtime path that references the named package, lists the files in
the referrer that contain the package's store hash (`lib/links/...` entries
are symlinks; other files are scanned for the hash), so a reference made
only by the link farm is visible as such.
"""
import json, os, sys

pi = json.load(open(sys.argv[1]))
if isinstance(pi, dict) and "info" in pi:
    pi = pi["info"]
refs = {os.path.basename(p): {os.path.basename(r) for r in v["references"]} for p, v in pi.items()}
S = "/nix/store/"
for want in sys.argv[2:]:
    target = next((p for p in refs if p[33:] == want), None)
    if not target:
        print(f"{want}: not in the runtime closure")
        continue
    h = target[:32].encode()
    print(f"== {want}")
    for o in sorted(p for p, rs in refs.items() if target in rs and p != target):
        hits = []
        for dp, dns, fns in os.walk(S + o):
            for f in fns + [d for d in dns if os.path.islink(os.path.join(dp, d))]:
                fp = os.path.join(dp, f)
                if os.path.islink(fp):
                    if h in os.readlink(fp).encode():
                        hits.append(os.path.relpath(fp, S + o))
                else:
                    with open(fp, "rb") as fh:
                        if h in fh.read():
                            hits.append(os.path.relpath(fp, S + o))
        only_links = all(x.startswith("lib/links/") for x in hits)
        print(f"  <- {o[33:]}: {'lib/links only' if only_links else 'ELSEWHERE'}: {sorted(hits)[:4]}")
