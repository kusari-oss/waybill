"""Apply one named mutation for spec 1539 (T021).

Usage: mutate.py <repo> <id>          apply mutation <id> in place
       mutate.py <repo> --check       verify every anchor occurs exactly once
       mutate.py <repo> --paths <id>  print the files <id> touches
       mutate.py <repo> --ids         print every mutation id
Each mutation is a list of (path, old, new) edits; all are applied or none.
"""
import sys

C = "waybill-cli/src/scan_fs/package_db/cmake.rs"
F = "waybill-cli/src/scan_fs/package_db/cmake/fetches.rs"
W = "waybill-cli/src/scan_fs/package_db/cmake/wrappers.rs"
M = {
 # M1..M5 (R1): each forwarding shape no longer detected.
 "M1": [(W, "s.insert(ForwardingShape::Argn);", "let _ = ForwardingShape::Argn;")],
 "M2": [(W, "s.insert(ForwardingShape::Argv);", "let _ = ForwardingShape::Argv;")],
 "M3": [(W, "s.insert(ForwardingShape::Unparsed);", "let _ = ForwardingShape::Unparsed;")],
 "M4": [(W, "s.insert(ForwardingShape::ParsedVariable);", "let _ = ForwardingShape::ParsedVariable;")],
 "M5": [(W, "s.insert(ForwardingShape::Parameter);", "let _ = ForwardingShape::Parameter;")],
 # M6 (R2): a body call with its own identity becomes a wrapper.
 "M6": [(W, "                if has_identity(&d.file, kind, &d.body[i].args) {", "                if false && has_identity(&d.file, kind, &d.body[i].args) {")],
 # M7 (W1): a multi-value keyword takes one word, so it no longer swallows URL.
 "M7": [(W, "                    Cur::Multi(k) => multi.entry(k).or_default().push(a.clone()),",
            "                    Cur::Multi(k) => {\n                        multi.entry(k).or_default().push(a.clone());\n                        cur = Cur::None;\n                    }")],
 # M8 (W4): a one-value keyword keeps its first value.
 "M8": [(W, "                        one.insert(k, a.clone());", "                        one.entry(k).or_insert(a.clone());")],
 # M9 (W8): a NOT DEFINED default overrides the caller's value.
 "M9": [(W, "                        Some(Val::Known(_)) => Some(false),", "                        Some(Val::Known(_)) => Some(true),")],
 # M10 (W2, W3): the last URL is read instead of the first.
 "M10": [(C, "    let value = re.captures(body)?.get(1)?.as_str();", "    let value = re.captures_iter(body).last()?.get(1)?.as_str();")],
 # M11 (R5): the first definition wins instead of agreement.
 "M11": [(W, "            if keys.windows(2).any(|w| w[0] != w[1]) {", "            if false && keys.windows(2).any(|w| w[0] != w[1]) {")],
 # M12 (FR-004): the recursion guard removed (the depth cap still bounds it).
 "M12": [(W, "        if chain.iter().any(|c| c.eq_ignore_ascii_case(&lower)) {", "        if false && chain.iter().any(|c| c.eq_ignore_ascii_case(&lower)) {")],
 # M13 (FR-004): the depth cap removed.
 "M13": [(W, "        if chain.len() >= MAX_DEPTH {", "        if false && chain.len() >= MAX_DEPTH {")],
 # M14 (FR-005): the name no longer tied to the URL's version.
 "M14": [(C, '    let re = Regex::new(&format!(r"^(.+?)[-_]v?{}$", regex::escape(version))).ok()?;',
             '    let _ = version;\n    let re = Regex::new(r"^(.+?)[-_]v?[0-9][^/]*$").ok()?;')],
 # M15 (FR-005, FR-011): archive naming applied to direct calls.
 "M15": [(F, "            if !chain.is_empty() {\n                name_from_archive(&mut entry, &decl_args, vars);",
             "            name_from_archive(&mut entry, &decl_args, vars);\n            if !chain.is_empty() {")],
 # M16 (FR-008): template bodies read as declarations.
 "M16": [(F, "            if open.iter().any(|&o| defs.is_template(rel, o)) {\n                continue;\n            }", "")],
 # M17 (R7): a wrapper call inside a wrapper's body read as a call site.
 "M17": [(F, "            if open.iter().any(|&o| defs.is_template(rel, o)) {", "            if builtin.is_some() && open.iter().any(|&o| defs.is_template(rel, o)) {")],
 # M18 (FR-009): the C234 union in one_per_purl dropped.
 "M18": [(F, "            chains.extend(wrapper_chains(&other));", "            let _ = wrapper_chains(&other);")],
 # M19 (W-22): definitions keyed case-sensitively.
 "M19": [(W, "            out.by_name.entry(d.name.to_ascii_lowercase()).or_default().push(d);", "            out.by_name.entry(d.name.clone()).or_default().push(d);")],
 # M20 (R8): the wrapper's body taken as a function body for the shape.
 "M20": [(F, "fallback::classify(kind, &decl_args, &if_conds, blocks.function().is_some(), ctx)",
             "fallback::classify(kind, &decl_args, &if_conds, blocks.function().is_some() || !chain.is_empty(), ctx)")],
 # M21 (FR-006): conditions inside wrapper bodies dropped.
 "M21": [(F, "            if_conds.extend(body_conditions.iter().cloned());\n", ""),
         (F, "            conditions.extend(body_conditions);", "            let _ = body_conditions;")],
 # M22..M24 (FR-008): the accounting.
 "M22": [(F, "                                cov.wrapper_ambiguous_calls += 1;", "                                cov.wrapper_ambiguous_calls += 0;")],
 "M23": [(F, "                    cov.wrapper_calls += 1;", "                    cov.wrapper_calls += 0;")],
 "M24": [(F, "                            cov.unresolved_identity += 1;\n                            if e == Unbound::Ambiguous {",
             "                            cov.unresolved_identity += 0;\n                            if e == Unbound::Ambiguous {")],
 # M25 (FR-009): C234 not emitted.
 "M25": [(F, "                entry.extra_annotations.insert(ANN_CMAKE_FETCH_WRAPPER.to_string(), serde_json::json!([chain]));",
             "                let _ = &chain;")],
 # M26, M27 (I-7): the `wrappers` key never or always present.
 "M26": [(F, "            if self.wrapper_definitions > 0 {", "            if false && self.wrapper_definitions > 0 {")],
 "M27": [(F, "            if self.wrapper_definitions > 0 {", "            if true || self.wrapper_definitions > 0 {")],
 # M28: a variable another command changes keeps its stale value.
 "M28": [(W, "                        if s.vars.contains_key(a) && !fixed.contains(a) {", "                        if false && s.vars.contains_key(a) && !fixed.contains(a) {")],
 # M29: a conditional set() taken as unconditional.
 "M29": [(W, "            None => {\n                self.vars.insert(var.clone(), Val::Unknown);\n            }",
             "            None => {\n                self.vars.insert(var.clone(), Val::Known(value));\n            }")],
 # M30: the call site's variables not expanded before binding (TheRock's
 # openmpi, simde, xz and sqlite URLs use a same-file set()).
 "M30": [(F, "                    let call: Vec<String> = args.iter().map(|a| super::expand_known_vars(a, vars)).collect();",
             "                    let call: Vec<String> = args.clone();")],
}

repo, arg = sys.argv[1], sys.argv[2]
if arg == "--ids":
    print(" ".join(M))
    sys.exit(0)
if arg == "--paths":
    print(" ".join(sorted({p for p, _, _ in M[sys.argv[3]]})))
    sys.exit(0)
if arg == "--check":
    bad = 0
    for mid, edits in M.items():
        for path, old, _ in edits:
            n = open(f"{repo}/{path}").read().count(old)
            if n != 1:
                print(f"{mid}: anchor found {n} times in {path}")
                bad += 1
    print("anchors ok" if not bad else f"{bad} bad anchors")
    sys.exit(1 if bad else 0)
files = {}
for path, old, new in M[arg]:
    s = files.get(path) or open(f"{repo}/{path}").read()
    if s.count(old) != 1:
        sys.exit(f"{arg}: anchor found {s.count(old)} times in {path}")
    files[path] = s.replace(old, new)
for path, s in files.items():
    open(f"{repo}/{path}", "w").write(s)
print(" ".join(sorted(files)))
