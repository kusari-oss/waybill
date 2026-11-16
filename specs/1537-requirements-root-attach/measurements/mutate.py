"""Apply one named mutation for spec 1537 (T022).

Usage: mutate.py <repo> <id>       apply mutation <id> in place
       mutate.py <repo> --check    verify every anchor occurs exactly once
Each mutation is a list of (path, old, new) edits; all are applied or none.
"""
import sys

RE = "waybill-cli/src/generate/root_edges.rs"
DA = "waybill-cli/src/scan_fs/declared_attach.rs"
M = {
 # M1: anchors close the gate (the CycloneDX/SPDX 2.3 defect of #1566).
 "M1": [(RE, "    if !root_ref.is_empty() && fallback_open(components, relationships, root_ref) {",
             "    if !root_ref.is_empty() && fallback_open(components, relationships, root_ref) && anchors.is_empty() {")],
 # M2: the fallback only for a synthetic (placeholder) root.
 "M2": [(RE, "    if !root_ref.is_empty() && fallback_open(components, relationships, root_ref) {",
             "    if root_ref.starts_with(\"pkg:generic/\") && fallback_open(components, relationships, root_ref) {")],
 # M3: a lockfile-resolve anchor closes the gate (#894 regression).
 "M3": [(RE, "        .any(|r| r.from == root_ref && !resolve.contains(r.to.as_str()))",
             "        .any(|r| r.from == root_ref && resolve.len() < usize::MAX)")],
 # M4: nested components counted as graph-tops.
 "M4": [(RE, "                .filter(|c| c.parent_purl.is_none())\n", "")],
 # M5: completeness ignores anchors.
 "M5": [("waybill-cli/src/generate/graph_completeness/mod.rs",
         "        let anchors = crate::supplement::current_root_anchors();\n        let targets = crate::generate::root_edges::root_targets(\n            components,",
         "        let anchors: Vec<String> = Vec::new();\n        let targets = crate::generate::root_edges::root_targets(\n            components,")],
 # M6 (FR-002): the owner walk stops below the scan root, so a scan-root
 # file goes to the subject beside a scan-root main module (the original
 # FR-002, which flattened the quality corpus's python-ansible).
 "M6": [(DA, "        dir.ancestors()\n            .find_map(",
             "        dir.ancestors()\n            .take_while(|d| !d.as_os_str().is_empty())\n            .find_map(")],
 # M7: precedence reversed: a submodule outranks a main module.
 "M7": [(DA, "const RANK_SUBMODULE: u8 = 2;", "const RANK_SUBMODULE: u8 = 0;"),
        (DA, "const RANK_PYPI_MAIN_MODULE: u8 = 0;", "const RANK_PYPI_MAIN_MODULE: u8 = 3;"),
        (DA, "const RANK_OTHER_MAIN_MODULE: u8 = 1;", "const RANK_OTHER_MAIN_MODULE: u8 = 4;")],
 # M8: the walk stops at the file's own directory.
 "M8": [(DA, "        dir.ancestors()\n            .find_map(",
             "        dir.ancestors()\n            .take(1)\n            .find_map(")],
 # M9: the pip duplicate merge drops the second declaring file.
 "M9": [("waybill-cli/src/scan_fs/package_db/pip/mod.rs",
         "                    kept.extra_source_paths.push(a.source_path);",
         "                    let _ = (&kept, a.source_path);")],
 # M10: the pass runs after lifecycle typing.
 "M10": [("waybill-cli/src/scan_fs/mod.rs",
          "    package_db::nix::attach_inputs_to_projects(&components, &mut relationships);\n",
          "    package_db::nix::attach_inputs_to_projects(&components, &mut relationships);\n    apply_lifecycle_scope_to_edges(&components, &mut relationships);\n"),
         ("waybill-cli/src/scan_fs/mod.rs",
          "        relationships.extend(declared.edges);\n    }\n\n    apply_lifecycle_scope_to_edges(&components, &mut relationships);\n",
          "        relationships.extend(declared.edges);\n    }\n\n")],
 # M11: declared root anchors are not installed.
 "M11": [("waybill-cli/src/cli/scan_cmd.rs",
          "        anchors.extend(declared_root_anchors.iter().cloned());\n",
          "        let _ = &declared_root_anchors;\n")],
 # M12: a non-pip (cget) requirements.txt component is accepted.
 "M12": [(DA, "        if c.purl.ecosystem() != \"pypi\" || is_main_module(c) {",
              "        if is_main_module(c) {")],
 # M13 (FR-006): SPDX 2.3 bypasses root_targets.
 "M13": [("waybill-cli/src/generate/spdx/document.rs",
          "    if let Some(root_id) = root_ids.first() {\n        let anchors = crate::supplement::current_root_anchors();\n        let targets = crate::generate::root_edges::root_targets(",
          "    if let Some(root_id) = root_ids.first().filter(|_| false) {\n        let anchors = crate::supplement::current_root_anchors();\n        let targets = crate::generate::root_edges::root_targets(")],
 # M14 (FR-009): submodule anchors are not installed.
 # M15 (#1234): the pip reader ignores `# via`.
 "M15": [("waybill-cli/src/scan_fs/package_db/pip/requirements_txt.rs",
          "fn link_via_annotations(entries: &mut [PackageDbEntry], via: &BTreeMap<String, Vec<String>>) {\n    if via.is_empty() {",
          "fn link_via_annotations(entries: &mut [PackageDbEntry], via: &BTreeMap<String, Vec<String>>) {\n    if via.is_empty() || entries.len() < usize::MAX {")],
 # M16 (#1234): every pin of a file is direct, whatever its siblings say.
 "M16": [(DA, "        .filter(|m| tops.contains(&m.as_str()) || !reached.contains(m.as_str()))",
              "        .filter(|m| tops.contains(&m.as_str()) || !reached.contains(m.as_str()) || !m.is_empty())")],
 "M14": [("waybill-cli/src/cli/scan_cmd.rs",
          "        let mut anchors: Vec<String> =\n            submodules.iter().filter(|p| !nested.contains(*p)).map(|p| p.to_string()).collect();",
          "        let mut anchors: Vec<String> = { let _ = (&submodules, &nested); Vec::new() };")],
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
edits = M[arg]
files = {}
for path, old, new in edits:
    s = files.get(path) or open(f"{repo}/{path}").read()
    if s.count(old) != 1:
        sys.exit(f"{arg}: anchor found {s.count(old)} times in {path}")
    files[path] = s.replace(old, new)
for path, s in files.items():
    open(f"{repo}/{path}", "w").write(s)
print(" ".join(sorted(files)))
