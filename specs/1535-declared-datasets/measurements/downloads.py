"""Census of CMake `file(DOWNLOAD ...)` in TheRock's pinned submodules (waybill spec 1535, #1565).

Usage: python3 -I -B downloads.py <checkout-root> <out.json>

<checkout-root> holds one directory per submodule (sparse checkouts of
`*.cmake` and `CMakeLists.txt` at the pinned commit). For every
`file(DOWNLOAD <url> ...)` it records the file, the raw URL, the variables
the URL uses, whether each is a literal list defined in the same file and
iterated in an enclosing foreach (FR-001), and whether a digest list is
paired. A measuring script, not a model of the reader.
"""

import json
import os
import re
import sys

root, out_path = sys.argv[1:3]
CMD = re.compile(r"(?is)\b(\w+)\s*\((.*?)\)")


def commands(text):
    text = re.sub(r"(?m)#.*$", "", text)
    # Balanced-enough: CMake commands rarely nest parens in these files.
    depth, start, name_start = 0, None, None
    out = []
    i = 0
    while i < len(text):
        m = re.compile(r"\b([A-Za-z_]\w*)\s*\(").match(text, i)
        if m and depth == 0:
            name = m.group(1)
            j = m.end()
            d = 1
            k = j
            while k < len(text) and d:
                if text[k] == "(":
                    d += 1
                elif text[k] == ")":
                    d -= 1
                k += 1
            out.append((name.lower(), text[j:k - 1]))
            i = k
            continue
        i += 1
    return out


recs = []
for sub in sorted(os.listdir(root)):
    for d, _, fs in os.walk(os.path.join(root, sub)):
        for f in fs:
            if not (f.endswith(".cmake") or f == "CMakeLists.txt"):
                continue
            p = os.path.join(d, f)
            try:
                text = open(p, errors="replace").read()
            except OSError:
                continue
            if "DOWNLOAD" not in text.upper():
                continue
            cmds = commands(text)
            lists = {}
            for name, args in cmds:
                if name == "set":
                    a = args.split()
                    if len(a) >= 3 and all("${" not in x for x in a[1:]):
                        lists[a[0]] = a[1:]
            loop_stack = []
            listget = {}
            for name, args in cmds:
                a = args.split()
                if name == "foreach":
                    loop_stack.append(a)
                elif name == "endforeach" and loop_stack:
                    loop_stack.pop()
                elif name == "list" and len(a) >= 4 and a[0].upper() == "GET":
                    listget[a[3]] = a[1]
                elif name == "file" and a and a[0].upper() == "DOWNLOAD" and len(a) >= 2:
                    url = a[1].strip('"')
                    vars_ = re.findall(r"\$\{(\w+)\}", url)
                    envs = re.findall(r"\$ENV\{", url)
                    resolved = []
                    for v in vars_:
                        src = listget.get(v)
                        if src and src in lists:
                            resolved.append(f"{v}<-{src}[{len(lists[src])}]")
                        elif any(len(l) >= 3 and l[0] == v and "IN" in [x.upper() for x in l] for l in loop_stack):
                            resolved.append(f"{v}<-foreach")
                        else:
                            resolved.append(None)
                    ok = not envs and all(resolved)
                    recs.append({
                        "submodule": sub,
                        "file": os.path.relpath(p, os.path.join(root, sub)),
                        "url": url[:200],
                        "literal": not vars_ and not envs,
                        "fr001_resolvable": ok,
                        "vars": vars_,
                        "resolved_by": resolved,
                        "in_loop": bool(loop_stack),
                        "digest_lists": sorted(k for k in lists if re.search(r"(MD5|SHA|HASH)", k, re.I)),
                    })
json.dump(recs, open(out_path, "w"), indent=1, sort_keys=True)
res = [r for r in recs if r["fr001_resolvable"]]
print(f"## file(DOWNLOAD) census: {len(recs)} calls in {len({r['file'] for r in recs})} files; FR-001-resolvable {len(res)}\n")
for r in res:
    print(f"- `{r['submodule']}/{r['file']}`: `{r['url']}` literal={r['literal']} by={r['resolved_by']} digests={r['digest_lists']}")
print("\n### Not resolvable (first 40)\n")
for r in [r for r in recs if not r["fr001_resolvable"]][:40]:
    print(f"- `{r['submodule']}/{r['file']}`: `{r['url']}` vars={r['vars']}")
