"""Spec 1618 mutations: apply one by name, by exact string replacement.

Usage: python3 -I -B mutate.py <name>   (from the repository root). Prints
the mutated file. Each anchor must occur exactly once.
"""
import sys

RS = "waybill-cli/src/generate/root_selector.rs"
DA = "waybill-cli/src/scan_fs/declared_attach.rs"
SP = "waybill-cli/src/scan_fs/cmake_search_pair.rs"
PR = "waybill-cli/src/scan_fs/package_db/cmake/projects.rs"
CM = "waybill-cli/src/scan_fs/package_db/cmake.rs"
SE = "waybill-cli/src/scan_fs/package_db/cmake/searches.rs"
CB = "waybill-cli/src/generate/cyclonedx/builder.rs"
MUTATIONS = {
    "M1-cmake-always-root-candidate": (RS,
        "        .filter(|&i| !is_cmake(i) || (!others && components[i].lifecycle_scope.is_none()))",
        "        .filter(|&i| { let _ = others; !is_cmake(i) || true })"),
    "M2-cmake-rank-above-main-modules": (DA,
        "const RANK_CMAKE_PROJECT: u8 = 2;",
        "const RANK_CMAKE_PROJECT: u8 = 0;"),
    "M3-no-enclosing-edge": (SP,
        "            if let Some(owner) = index.enclosing(dir, purl) {",
        "            if let Some(owner) = None::<&str> {"),
    "M4-no-search-pairing": (SP,
        "        } else if let Some(p) = projects.get(&name) {",
        "        } else if let Some(p) = None::<&Vec<String>> {"),
    "M5-expression-emitted-as-absence": (PR,
        "                e.extra_annotations.insert(ANN_DECLARED_VERSION_EXPRESSION.to_string(), serde_json::json!(x));",
        "                let _ = x;\n                e.extra_annotations.insert(ANN_VERSION_ABSENCE.to_string(), serde_json::json!(\"not-declared\"));"),
    "M6-test-directory-unscoped": (PR,
        "            Some(LifecycleScope::Test)\n        } else if segs",
        "            None\n        } else if segs"),
    "M7-reach-ignored": (PR,
        "        if g.decls.iter().all(|d| d.reached == Some(false)) {",
        "        if false {"),
    "M8-include-vendored-not-merged": (CM,
        "        if e.extra_annotations.get(\"waybill:source-mechanism\").and_then(|v| v.as_str()) != Some(\"cmake-vendored\") {\n            return true;\n        }",
        "        if true {\n            return true;\n        }"),
    "M9-conditional-search-makes-optional": (SP,
        "        let built_in_tree = ps.iter().any(|p| p.project) && !all_test;",
        "        let built_in_tree = false && ps.iter().any(|p| p.project) && !all_test;"),
    "M10-paired-project-serialised-at-zero": (SE,
        "            if self.paired_project > 0 {",
        "            if true {"),
    "M11-collisions-not-dropped": (SP,
        "        if !is_cmake_project(c) {\n            return true;\n        }\n        let name = c.name.to_lowercase();",
        "        if true {\n            return true;\n        }\n        let name = c.name.to_lowercase();"),
    "M12-vendored-rule-either-signal": (PR,
        "        if signals.contains(SIGNAL_PATH_SEGMENT) && signals.contains(SIGNAL_SUBMODULE_BOUNDARY) {",
        "        if signals.contains(SIGNAL_PATH_SEGMENT) || signals.contains(SIGNAL_SUBMODULE_BOUNDARY) {"),
    "M14-same-directory-projects-enclose": (DA,
        ".and_then(|v| v.iter().find(|(rank, p)| p != own && *rank != RANK_CMAKE_PROJECT))",
        ".and_then(|v| v.iter().find(|(rank, p)| { let _ = rank; p != own }))"),
    "M13-cmake-root-not-held-back": (CB,
        "                || subject_cmake_project == Some(c.purl.as_str())",
        "                || (false && subject_cmake_project == Some(c.purl.as_str()))"),
}

name = sys.argv[1]
path, old, new = MUTATIONS[name]
text = open(path).read()
assert text.count(old) == 1, f"{name}: anchor occurs {text.count(old)} times in {path}"
open(path, "w").write(text.replace(old, new))
print(path)
