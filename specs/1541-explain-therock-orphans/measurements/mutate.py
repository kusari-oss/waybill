"""Apply one named mutation for spec 1541 (T016).

Usage: mutate.py <repo> <id> | --check | --paths <id> | --ids
Each mutation is a list of (path, old, new) edits; all are applied or none.
"""
import sys

G = "waybill-cli/src/generate/graph_completeness/mod.rs"
C = "waybill-cli/src/cli/scan_cmd.rs"
H = "waybill-cli/tests/corpus_harness_195/invariants/checks/orphan_explained.rs"
O = "waybill-cli/src/generate/orphan_reason.rs"
U = "waybill-cli/src/scan_fs/package_db/pip/uv_lock.rs"
S = "waybill-cli/src/scan_fs/mod.rs"
D = "waybill-cli/src/scan_fs/binary/discover.rs"
CONTAIN = """        if let Some(parent) = &c.parent_purl {
            edges
                .entry(parent.clone())
                .or_default()
                .push(c.purl.as_str().to_string());
        }"""
M = {
 # M1 (FR-002): the classifier without the operator's root again.
 "M1": [(C, "            &mut components,\n            &relationships,\n            &root_override,\n",
            "            &mut components,\n            &relationships,\n            &crate::generate::RootComponentOverride::default(),\n")],
 # M2, M3 (FR-003): containment edges removed; reversed.
 "M2": [(G, CONTAIN, "        let _ = &c.parent_purl;")],
 "M3": [(G, CONTAIN, """        if let Some(parent) = &c.parent_purl {
            edges
                .entry(c.purl.as_str().to_string())
                .or_default()
                .push(parent.clone());
        }""")],
 # M4, M5 (I7 harness): nested components ignored; containment not reachability.
 "M4": [(H, "        cdx_components_with_containers(cdx).into_iter().map(|(c, _)| c).collect();",
            "        cdx_components(cdx).iter().collect();")],
 "M5": [(H, "            adj.entry(container).or_default().push(r);", "            let _ = (container, r);")],
 # M6..M11 (FR-004): each code's arm; the directory list; every -> any;
 # position; case-insensitive segments.
 "M6": [(O, "                    if all_under_test_data_dirs(&c.evidence.source_file_paths) {", "                    if false {")],
 "M7": [(O, "                    } else {\n                        OrphanReasonCode::UnownedBinary\n                    }",
            "                    } else {\n                        continue;\n                    }")],
 "M8": [(O, '    "test_assets",\n', '    "test_assets_x",\n')],
 "M9": [(O, "    !real.is_empty() && real.iter().all(|p| under(p))", "    real.iter().any(|p| under(p))")],
 "M10": [(O, "        let code = match (ecosystem.as_str(), has_reachable_sibling) {\n            (\"golang\", true)",
             "        let code = match (ecosystem.as_str(), has_reachable_sibling) {\n            _ if c.sbom_tier.as_deref() == Some(\"analyzed\") => OrphanReasonCode::UnownedBinary,\n            (\"golang\", true)")],
 "M11": [(O, ".is_some_and(|(_, dirs)| dirs.iter().any(|d| TEST_DATA_DIRS.contains(d)))",
             ".is_some_and(|(_, dirs)| dirs.iter().any(|d| TEST_DATA_DIRS.iter().any(|t| t.eq_ignore_ascii_case(d))))")],
 # M12..M17 (FR-005): the uv reader.
 "M12": [(U, "        let project_without_version = version.is_empty()\n            && matches!(source_shape, UvSourceShape::Editable | UvSourceShape::Virtual);",
             "        let project_without_version = false && version.is_empty();")],
 "M13": [(U, "                                if added.insert(to.clone()) {\n                                    depends.push(to);\n                                }",
             "                                let _ = added.insert(to);")],
 "M14": [(U, "        Some(pinned.cloned().unwrap_or_else(|| name.to_string()))", "        let _ = pinned;\n        Some(name.to_string())")],
 "M15": [(U, "                if !own_project && !extra_is_active(name, extra_name) {", "                if !extra_is_active(name, extra_name) {")],
 "M16": [(U, "                if !own_project && !extra_is_active(name, extra_name) {", "                if false && !own_project && !extra_is_active(name, extra_name) {")],
 "M17": [(U, '                "waybill:unresolved-reason".to_string(),', '                "waybill:unresolved-reason-x".to_string(),')],
 # M18, M19 (FR-011): the whole-tree rule back; the installed prefix ignored.
 "M18": [(S, """        let skip_test_inputs = installed_tree != test_scope::InstalledTree::Image
            && os_claimed_paths.is_empty()
            && !include_test_binaries;""",
             "        let skip_test_inputs = installed_tree == test_scope::InstalledTree::NotRecognised && !include_test_binaries;")],
 "M19": [(D, "                if is_under_test_input_dir(&rel) && !installed {", "                if is_under_test_input_dir(&rel) {")],
 # M20 (FR-002): reachability keyed by PURL again, so a top-level copy of a
 # nested PURL counts as reached (ROCm SDK `gotcha`).
 "M20": [(G, '        Some(parent) => format!("{}#{parent}", c.purl.as_str()),', '        Some(_parent) => c.purl.as_str().to_string(),')],
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
