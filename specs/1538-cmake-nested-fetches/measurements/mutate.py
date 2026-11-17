"""Apply one named mutation for spec 1538 (T025).

Usage: mutate.py <repo> <id>          apply mutation <id> in place
       mutate.py <repo> --check       verify every anchor occurs exactly once
       mutate.py <repo> --paths <id>  print the files <id> touches
Each mutation is a list of (path, old, new) edits; all are applied or none.
"""
import sys

C = "waybill-cli/src/scan_fs/package_db/cmake.rs"
F = "waybill-cli/src/scan_fs/package_db/cmake/fetches.rs"
FB = "waybill-cli/src/scan_fs/package_db/cmake/fallback.rs"
B = "waybill-cli/src/scan_fs/package_db/cmake/blocks.rs"
SM = "waybill-cli/src/scan_fs/mod.rs"
M = {
 # M1 (FR-001): discovery back to today's file set.
 "M1": [(C, "    let mut fetch_files = walked.unwrap_or(cmake_files);",
             "    let _ = walked;\n    let mut fetch_files = cmake_files;")],
 # M2: comments not stripped before tokenising.
 "M2": [(F, "commands_checked(&strip_comments(&content))", "commands_checked(&content)")],
 # M3: else() does not mark the branch.
 "M3": [(B, "                    } else if !condition.starts_with(\"else: \") {\n                        *condition = format!(\"else: {condition}\");",
             "                    } else if !condition.starts_with(\"else: \") {\n                        let _ = &condition;")],
 # M4..M10 (FR-003, R3): each shape removed in turn, or its scope rule.
 "M4": [(FB, "    if guards.iter().any(|c| not_target(c)) {", "    if false && guards.iter().any(|c| not_target(c)) {")],
 "M5": [(FB, "    if guards.iter().any(|c| not_found(c)) {", "    if false && guards.iter().any(|c| not_found(c)) {")],
 "M6": [(FB, "&& args.iter().any(|a| a == \"FIND_PACKAGE_ARGS\")", "&& args.iter().any(|a| a == \"FIND_PACKAGE_ARGS\") && false")],
 "M7": [(FB, "    if fc && ctx.mode != TryMode::Never && args", "    if fc && args")],
 "M8": [(FB, "    if fc && ctx.mode == TryMode::Always {", "    if false && fc && ctx.mode == TryMode::Always {")],
 "M9": [(FB, ".any(|a| a == \"FETCHCONTENT_MAKEAVAILABLE_SERIAL\")", ".any(|_| true)")],
 "M10": [(FB, "    if in_function {", "    if false && in_function {")],
 # M11: mode looked up in the declaring directory only, not its ancestors.
 "M11": [(F, "    dir.ancestors().find_map(|d| modes.get(d).copied()).unwrap_or_default()",
              "    std::iter::once(dir).find_map(|d| modes.get(d).copied()).unwrap_or_default()")],
 # M12 (R7): Test does not outrank Optional.
 "M12": [(F, "    if let Some(derivation) = test {", "    if let Some(derivation) = test.filter(|_| shape.is_none()) {")],
 # M13 (R4): a required declaration does not keep its PURL required.
 "M13": [(F, "            None => 0,\n            Some(LifecycleScope::Optional) => 1,", "            None => 3,\n            Some(LifecycleScope::Optional) => 1,")],
 # M14 (R5): first declaration wins between siblings.
 "M14": [(F, "                    od != dir && dir.starts_with(od)", "                    od != dir")],
 # M15 (R3 P6): first declaration wins for ExternalProject.
 "M15": [(F, "        if d.kind != FetchKind::FetchContent {\n            continue;\n        }\n        let dir",
              "        let dir"),
         (F, "                *j != i && o.kind == FetchKind::FetchContent && o.name",
              "                *j != i && o.name")],
 # M16 (R9): unconditional fetches merge into an existing component too.
 "M16": [(F, "    let conditional = |e: &PackageDbEntry| {\n        is_fetch(e)\n            && e.lifecycle_scope",
              "    let conditional = |e: &PackageDbEntry| {\n        is_fetch(e)\n            || e.lifecycle_scope")],
 # M17 (R6): owner edges not added.
 "M17": [(SM, "        relationships.extend(fetches.edges);", "        let _ = fetches.edges;")],
 # M18 (R8): unresolved identity not counted (breaks V1).
 "M18": [(F, "                cov.unresolved_identity += 1;", "                cov.unresolved_identity += 0;")],
}

repo, arg = sys.argv[1], sys.argv[2]
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
