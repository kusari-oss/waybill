"""Spec 1614 mutations: apply one by name, by exact string replacement.

Usage: python3 -I -B mutate.py <name>   (from the repository root). Prints
the mutated file. Each anchor must occur exactly once; the script refuses
otherwise.
"""
import sys

C = "waybill-cli/src/scan_fs/package_db/cmake/"
S = "waybill-cli/src/scan_fs/"
MUTATIONS = {
    "M1-unconditional-edge-does-not-win": (C + "reach.rs",
        "                if path.is_empty() {\n                    unconditional = true;",
        "                if false && path.is_empty() {\n                    unconditional = true;"),
    "M2-first-path-not-union": (C + "reach.rs",
        "                union.get_or_insert_with(BTreeSet::new).extend(path);",
        "                if union.is_none() {\n                    union = Some(path);\n                }"),
    "M3-closed-cycle-conditional": (C + "reach.rs",
        "        by_file.entry(n.clone()).or_insert(Reach::Unconditional);",
        "        by_file.entry(n.clone()).or_insert(Reach::Conditional(BTreeSet::from([\"cycle\".to_string()])));"),
    "M4-narrow-file-set": (C[:-1] + ".rs",
        "    let tree_wide = walked.is_some();",
        "    let tree_wide = false && walked.is_some();"),
    "M5-include-reached-files-not-read": (C + "fetches.rs",
        "    let files: Vec<PathBuf> = files.iter().cloned().chain(reach.extra_files.iter().cloned()).collect();",
        "    let files: Vec<PathBuf> = files.to_vec();"),
    "M6-reach-seed-not-applied": (C + "fetches.rs",
        "        let mut blocks = super::blocks::BlockStack::seeded(reach.seed(path));",
        "        let mut blocks = super::blocks::BlockStack::seeded(Vec::new());"),
    "M7-search-scope-ignores-conditions": (C + "searches.rs",
        "        } else if s.conditions.is_empty() {\n            a.runtime = true;",
        "        } else if true {\n            a.runtime = true;"),
    "M8-tool-list-emptied": (C + "searches.rs",
        'const TOOLS: [&str; 5] = ["threads", "pkgconfig", "git", "doxygen", "pythoninterp"];',
        'const TOOLS: [&str; 5] = ["waybill-x1", "waybill-x2", "waybill-x3", "waybill-x4", "waybill-x5"];'),
    "M9-python-rule-ignores-development": (C + "searches.rs",
        "    python && interpreter && !development",
        "    python && interpreter && (development || !development)"),
    "M10-fetch-pairing-removed": (C[:-1] + ".rs",
        "            .partition(|s| decls.fetch_purls.contains_key(&s.name.to_lowercase()));",
        "            .partition(|s| false && decls.fetch_purls.contains_key(&s.name.to_lowercase()));"),
    "M11-installed-pairing-removed": (S + "cmake_search_pair.rs",
        "            records.entry(c.name.to_lowercase()).or_default().push(c.purl.as_str().to_string());",
        "            records.entry(format!(\"waybill-x-{}\", c.name.to_lowercase())).or_default().push(c.purl.as_str().to_string());"),
    "M12-submodule-pairing-removed": (S + "cmake_search_pair.rs",
        "        .filter_map(|(dir, purl)| Some((dir.file_name()?.to_string_lossy().to_lowercase(), purl.clone())))",
        "        .filter_map(|(dir, purl)| Some((format!(\"waybill-x-{}\", dir.file_name()?.to_string_lossy().to_lowercase()), purl.clone())))"),
    "M13-in-tree-before-pairing": (S + "cmake_search_pair.rs",
        "        let (targets, installed): (Vec<String>, bool) = if let Some(r) = records.get(&name) {",
        "        let (targets, installed): (Vec<String>, bool) = if cov.built_in_tree.contains(&name) {\n            outcome.insert(name, \"in-tree\");\n            gone.insert(c.purl.as_str().to_string());\n            continue;\n        } else if let Some(r) = records.get(&name) {"),
    "M14-in-tree-rule-removed": (S + "cmake_search_pair.rs",
        "        } else if cov.built_in_tree.contains(&name) {",
        "        } else if false && cov.built_in_tree.contains(&name) {"),
    "M15-config-template-patterns-removed": (C[:-1] + ".rs",
        '        "**/*Config.cmake.in",\n        "**/*-config.cmake.in",\n',
        ""),
    "M16-search-attachment-removed": (S + "cmake_fetch_attach.rs",
        '                || m == "cmake-find-package"\n                || m == "cmake-pkg-check-modules"\n',
        ""),
    "M17-c57-not-widened": (S + "cmake_search_pair.rs",
        "        if used {\n            c.extra_annotations.insert(ANN_BUILD_REFERENCE.to_string(), serde_json::json!(\"declared-and-used\"));",
        "        if false && used {\n            c.extra_annotations.insert(ANN_BUILD_REFERENCE.to_string(), serde_json::json!(\"declared-and-used\"));"),
    "M18-record-scope-ignores-searches": (S + "mod.rs",
        "        cmake_search_pair::apply_scope(&mut components, &scopes.optional);",
        "        cmake_search_pair::apply_scope(&mut components, &Default::default());"),
}

name = sys.argv[1]
path, old, new = MUTATIONS[name]
text = open(path).read()
assert text.count(old) == 1, f"{name}: anchor occurs {text.count(old)} times in {path}"
open(path, "w").write(text.replace(old, new))
print(path)
