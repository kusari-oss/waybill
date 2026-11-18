"""Apply one named mutation for spec 1542 (T018).

Usage: mutate.py <repo> <id> | --check | --paths <id> | --ids
"""
import sys

DD = "waybill-cli/src/resolve/deduplicator.rs"
GC = "waybill-cli/src/generate/graph_completeness/mod.rs"
M = {
 # M1 (R1): the content hash dropped from the key.
 "M1": [(DD, "            content_hash(&component.purl),\n        );", "            None::<String>,\n        );")],
 # M2 (R1): the content-hash parse always None.
 "M2": [(DD, "    purl.as_str()\n        .split_once(\"file-sha256=\")", "    let _ = purl;\n    None::<&str>\n        .and_then(|s| s.split_once(\"file-sha256=\"))")],
 # M3 (R2): the snapshot reverted to the live map.
 "M3": [(GC, "        if let Some(own) = dependency_edges.get(c.purl.as_str()).cloned() {", "        if let Some(own) = edges.get(c.purl.as_str()).cloned() {")],
 # M4 (R3): the output sort back to PURL only.
 "M4": [(DD, "    result.sort_by(total_order);", "    result.sort_by(|a, b| a.purl.as_str().cmp(b.purl.as_str()));")],
 # M5 (R3): the container tie-break removed from the total order.
 "M5": [(DD, "        .then_with(|| a.parent_purl.cmp(&b.parent_purl))\n", "")],
 # M6 (R3): merged path lists left unsorted.
 "M6": [(DD, "        best.evidence.source_file_paths.sort();\n", "")],
}

repo, arg = sys.argv[1], sys.argv[2]
if arg == "--ids":
    print(" ".join(M)); sys.exit(0)
if arg == "--paths":
    print(" ".join(sorted({p for p, _, _ in M[sys.argv[3]]}))); sys.exit(0)
if arg == "--check":
    bad = 0
    for mid, edits in M.items():
        for path, old, _ in edits:
            n = open(f"{repo}/{path}").read().count(old)
            if n != 1:
                print(f"{mid}: anchor found {n} times in {path}"); bad += 1
    print("anchors ok" if not bad else f"{bad} bad anchors"); sys.exit(1 if bad else 0)
files = {}
for path, old, new in M[arg]:
    s = files.get(path) or open(f"{repo}/{path}").read()
    if s.count(old) != 1:
        sys.exit(f"{arg}: anchor found {s.count(old)} times in {path}")
    files[path] = s.replace(old, new)
for path, s in files.items():
    open(f"{repo}/{path}", "w").write(s)
