"""Apply one named mutation for spec 1540 (T014).

Usage: mutate.py <repo> <id> | --check | --paths <id> | --ids
Each mutation is a list of (path, old, new) edits; all are applied or none.
"""
import sys

W = "waybill-cli/src/scan_fs/package_db/cmake/wrappers.rs"
M = {
 # M1..M3 (FR-001): each shape condition removed.
 "M1": [(W, '.position(|c| c.name == "cmake_parse_arguments" && parse_args(&c.args).is_some_and(|p| p.prefix == "DL_ARGS"))',
             '.position(|c| c.name == "cmake_parse_arguments" && parse_args(&c.args).is_some())')],
 "M2": [(W, '        Ok(Some(resolved)) => parse_args(&resolved).is_some_and(|p| p.one.iter().any(|k| k == "PROJ")),',
             '        Ok(Some(_)) => true,')],
 "M3": [(W, "            && c.args.first().is_some_and(|a| a.rsplit('/').next() == Some(DOWNLOAD_PROJECT_TEMPLATE))", "")],
 # M4: the name check made case-sensitive.
 "M4": [(W, '    if !d.name.eq_ignore_ascii_case("download_project") {', '    if d.name != "download_project" {')],
 # M5 (FR-004): named <PROJ>-download.
 "M5": [(W, 'const DOWNLOAD_PROJECT_CALL: [&str; 2] = ["${DL_ARGS_PROJ}", "${DL_ARGS_UNPARSED_ARGUMENTS}"];',
             'const DOWNLOAD_PROJECT_CALL: [&str; 2] = ["${DL_ARGS_PROJ}-download", "${DL_ARGS_UNPARSED_ARGUMENTS}"];')],
 # M6: the forwarded arguments dropped.
 "M6": [(W, 'const DOWNLOAD_PROJECT_CALL: [&str; 2] = ["${DL_ARGS_PROJ}", "${DL_ARGS_UNPARSED_ARGUMENTS}"];',
             'const DOWNLOAD_PROJECT_CALL: [&str; 2] = ["${DL_ARGS_PROJ}", "${DL_ARGS_SOURCE_DIR}"];')],
 # M7: the template's BUILD_PROJECT condition put on the fetch.
 "M7": [(W, "        if_conditions: Vec::new(),\n    };\n    d.name_var", "        if_conditions: vec![\"${DL_ARGS_BUILD_PROJECT}\".to_string()],\n    };\n    d.name_var")],
 # M8 (FR-002): keyword lists read literally again.
 "M8": [(W, "                    if let Some(p) = s.keyword_lists(&cmd.args)?.as_deref().and_then(parse_args) {",
             "                    if let Some(p) = parse_args(&cmd.args) {")],
 # M9: keyword lists spliced as separate arguments.
 "M9": [(W, "        let mut out = args.to_vec();\n        for i in lists {",
             "        let spliced = self.substitute(args);\n        if spliced.iter().any(|a| a.contains(\"${\")) {\n            return Err(Unbound::UnknownKeywords);\n        }\n        let _ = &lists;\n        let mut out = spliced;\n        for i in 0..0 {")],
 # M10: an unresolved list bound as empty instead of unresolved.
 "M10": [(W, "            if v.contains(\"${\") {\n                return Err(Unbound::UnknownKeywords);\n            }",
              "            if v.contains(\"${\") {\n                out[i] = String::new();\n                continue;\n            }")],
 # M11 (FR-007): a call with no PROJ binds anyway.
 "M11": [(W, "        if let Some(v) = &d.name_var {", "        if let Some(v) = d.name_var.as_ref().filter(|_| false) {")],
 # M12: an unresolvable keyword list fails the shape check, so the call goes unread.
 "M12": [(W, "        Ok(None) => false,\n        Err(_) => true,", "        Ok(None) => false,\n        Err(_) => false,")],
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
print(" ".join(sorted(files)))
