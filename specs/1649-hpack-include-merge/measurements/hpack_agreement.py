"""Spec 1649 T004 (research R7): does waybill's dependency set for each hpack
package equal the `build-depends` hpack itself generates?

Both sides come from the real tools. hpack renders each `package.yaml` in a
dedicated throwaway copy of the repository, never in the copy waybill
scans: hpack writes a `.cabal` beside every `package.yaml`, which would
put back the generated files a no-generated-cabal copy exists to lack
(analyze U1). waybill's side is read from the CycloneDX document it emitted
for the no-generated-cabal copy. This script only compares names.

usage: python3 -I -B hpack_agreement.py REPO_COPY WAYBILL_CDX HPACK_WORK_DIR

Prints one line per package and a summary; exits 0 regardless, since a
difference is a finding to read, not a crash.
"""
import json
import pathlib
import re
import shutil
import subprocess
import sys

REPO = pathlib.Path(sys.argv[1]).resolve()
CDX = pathlib.Path(sys.argv[2])
WORK = pathlib.Path(sys.argv[3]).resolve()

NAME_RE = re.compile(r"^name:\s*(\S+)", re.M)
FIELD_RE = re.compile(r"^(\s*)([A-Za-z][A-Za-z0-9-]*)\s*:(.*)$")


def build_depends_names(cabal_text):
    """Package names in every `build-depends:` and `build-tool-depends:`
    field of a .cabal, fields continued on more-indented lines, version
    constraints dropped. A build tool `pkg:exe` contributes `pkg`."""
    names = set()
    lines = cabal_text.splitlines()
    i = 0
    while i < len(lines):
        m = FIELD_RE.match(lines[i])
        if m and m.group(2).lower() in ("build-depends", "build-tool-depends"):
            indent = len(m.group(1))
            chunk = [m.group(3)]
            i += 1
            while i < len(lines) and (not lines[i].strip() or len(lines[i]) - len(lines[i].lstrip()) > indent):
                chunk.append(lines[i])
                i += 1
            for entry in " ".join(chunk).split(","):
                entry = entry.strip()
                if entry:
                    names.add(re.split(r"[\s<>=^:{]", entry, maxsplit=1)[0])
            continue
        i += 1
    return names


def hpack_side():
    if WORK.exists():
        shutil.rmtree(WORK)
    shutil.copytree(REPO, WORK, symlinks=True, ignore=shutil.ignore_patterns(".git"))
    out = {}
    failed = {}
    for manifest in sorted(WORK.rglob("package.yaml")):
        rel = manifest.relative_to(WORK)
        r = subprocess.run(["nix", "run", "--quiet", "nixpkgs#hpack", "--", "--force"],
                           cwd=manifest.parent, capture_output=True, text=True)
        cabals = [c for c in manifest.parent.glob("*.cabal")]
        if r.returncode != 0 or not cabals:
            failed[str(rel)] = (r.stderr or r.stdout).strip().splitlines()[-1:] or ["no .cabal written"]
            continue
        for c in cabals:
            text = c.read_text(errors="replace")
            m = NAME_RE.search(text)
            if m:
                # A package's own name is its components depending on its
                # library; waybill emits no edge from a package to itself.
                deps = build_depends_names(text) - {m.group(1)}
                out.setdefault(m.group(1), {"manifest": str(rel), "deps": set()})["deps"] |= deps
    shutil.rmtree(WORK)
    return out, failed


def waybill_side():
    doc = json.loads(CDX.read_text())
    comps = [doc.get("metadata", {}).get("component", {})]
    stack = list(doc.get("components", []))
    while stack:
        c = stack.pop()
        comps.append(c)
        stack.extend(c.get("components", []))
    by_ref = {c.get("bom-ref"): c for c in comps if c.get("bom-ref")}

    def props(c):
        return {p.get("name"): p.get("value") for p in c.get("properties", [])}

    def is_main(c):
        p = props(c)
        return p.get("waybill:component-role") == "main-module" or "waybill:demoted-from-main-module" in p

    def hackage_name(purl):
        return purl.split("/", 1)[1].split("@", 1)[0].split("?", 1)[0] if purl.startswith("pkg:hackage/") else None

    edges = {d.get("ref"): d.get("dependsOn", []) for d in doc.get("dependencies", [])}
    out = {}
    qualified = set()
    for c in comps:
        purl = c.get("purl") or ""
        name = hackage_name(purl)
        if not name or not is_main(c):
            continue
        deps = set()
        for t in edges.get(c.get("bom-ref"), []):
            target = by_ref.get(t, {}).get("purl") or t
            n = hackage_name(target)
            if n:
                # `pkg:exe` names a build tool by package and executable;
                # waybill keeps it whole (a separate defect, counted here).
                if ":" in n:
                    qualified.add(n)
                    n = n.split(":", 1)[0]
                deps.add(n)
        out.setdefault(name, set()).update(deps)
    return out, qualified


def main():
    hp, failed = hpack_side()
    wb, qualified = waybill_side()
    agree = differ = 0
    no_manifest = sorted(set(wb) - set(hp))
    for pkg in sorted(hp):
        h = hp[pkg]["deps"]
        w = wb.get(pkg)
        if w is None:
            print(f"{pkg}: MISSING from waybill (hpack {len(h)} deps; {hp[pkg]['manifest']})")
            differ += 1
        elif h == w:
            print(f"{pkg}: agree ({len(h)} deps)")
            agree += 1
        else:
            print(f"{pkg}: DIFFER hpack-only={sorted(h - w)} waybill-only={sorted(w - h)}")
            differ += 1
    for m, err in sorted(failed.items()):
        print(f"hpack failed on {m}: {err}")
    print(f"not compared (no package.yaml; a hand-written .cabal): {no_manifest}")
    print(f"waybill names kept as pkg:exe: {sorted(qualified)}")
    print(f"SUMMARY hpack-packages={len(hp)} agree={agree} differ={differ} hpack-failed={len(failed)}")


if __name__ == "__main__":
    main()
