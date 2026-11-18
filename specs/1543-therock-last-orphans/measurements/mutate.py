"""Apply one named mutation for spec 1543 (T024).

Usage: mutate.py <repo> <id> | --check | --paths <id> | --ids
"""
import sys

SF = "waybill-cli/src/scan_fs/mod.rs"
RS = "waybill-cli/src/generate/root_selector.rs"
LK = "waybill-cli/src/scan_fs/package_db/cmake/links.rs"
LA = "waybill-cli/src/scan_fs/cmake_link_attach.rs"
BL = "waybill-cli/src/scan_fs/package_db/npm/bun_lock.rs"
M = {
 # M1 (R1): no workspace-root anchors.
 "M1": [(SF, "        anchors.extend(workspace_root_anchors(&components, &relationships));\n", "")],
 # M2 (R2): the subject rung never fires.
 "M2": [(RS, "        if let Some(picked) = workspace_root_at_scan_root(components) {", "        if let Some(picked) = workspace_root_at_scan_root(components).filter(|_| false) {")],
 # M3 (R2): the rung reports an existing C69 name.
 "M3": [(RS, "                heuristic: Some(RootSelectionHeuristic::WorkspaceRoot),", "                heuristic: Some(RootSelectionHeuristic::RepoRoot),")],
 # M4 (R3): an included file starts with no conditions.
 "M4": [(LK, "                    let seed_here = conditions(&blocks);", "                    let seed_here: Vec<String> = Vec::new();")],
 # M5 (R4): the prefix test removed.
 "M5": [(LA, "                l.directory.starts_with(prefix)\n                    && ", "                ")],
 # M6 (R4): the name/file test removed.
 "M6": [(LA, "                    && (c.name.eq_ignore_ascii_case(&l.name) || holds_library(&l.directory, &l.name))", "")],
 # M7 (R6): a DVC pointer is not the library.
 "M7": [(LA, "        let lib = file.strip_suffix(\".dvc\").unwrap_or(&file);", "        let lib = file.as_str();")],
 # M8 (R5): the owner lookup forced to the root.
 "M8": [(LA, "        match index.owner_of(&rel) {", "        match None::<&str> {")],
 # M9 (R5): one unconditional link no longer keeps the scope.
 "M9": [(LA, "        if unconditional || scoped || targeted.contains(purl.as_str()) {", "        let _ = unconditional;\n        if scoped || targeted.contains(purl.as_str()) {")],
 # M10 (R4): the outermost prefix wins.
 "M10": [(LA, "            .max_by_key(|(prefix, _)| prefix.components().count());", "            .min_by_key(|(prefix, _)| prefix.components().count());")],
 # M11 (R1): bun members back to bare names.
 "M11": [(BL, "                .map(|n| workspace_member_purls.get(n).cloned().unwrap_or_else(|| n.clone()))", "                .cloned()")],
 # M12 (R5): the Optional scope never applied.
 "M12": [(SF, "        cmake_link_attach::apply_scope(&mut components, &links.conditional);\n", "")],
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
