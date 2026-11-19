"""Spec 1613 mutations: apply one by name, by exact string replacement.

Usage: python3 -I -B mutate.py <name>   (run from the repository root)
Each anchor must occur exactly once; the script refuses otherwise.
"""
import sys

P = "waybill-cli/src/scan_fs/package_db/"
MUTATIONS = {
    "M1-pip-no-descend": (P + "pip/mod.rs",
        "        descend_into: Some(super::project_roots::listed_dot_dir_globset()?),",
        "        descend_into: None,"),
    "M2-case-sensitive": (P + "project_roots.rs",
        "    crate::scan_fs::walk_registry::globset_from_patterns_case_insensitive(&LISTED_DOT_DIRECTORIES)",
        "    crate::scan_fs::walk_registry::globset_from_patterns(&LISTED_DOT_DIRECTORIES)"),
    "M3-venv-listed": (P + "project_roots.rs",
        'pub(crate) const LISTED_DOT_DIRECTORIES: [&str; 7] =\n    [".github", ".ci",',
        'pub(crate) const LISTED_DOT_DIRECTORIES: [&str; 8] =\n    [".venv", ".github", ".ci",'),
    "M4-no-ancestor-rule": (P + "pip/requirements_txt.rs",
        "        RequirementsScope::Main => listed_ancestor_class(path, root)",
        "        RequirementsScope::Main => None::<super::super::project_roots::DotDirClass>"),
    "M5-selector-not-widened": (P + "pip/requirements_txt.rs",
        '    if ["dev", "test", "docs", "ci"].iter().any(|s| matches_scope_filename(name, s)) {\n        return true;\n    }\n    name.to_ascii_lowercase()',
        '    if false {\n        return true;\n    }\n    false && name.to_ascii_lowercase()'),
    "M6-tox-accepted": (P + "pip/requirements_txt.rs",
        '    if ["dev", "test", "docs", "ci"].iter().any(|s| matches_scope_filename(name, s)) {',
        '    if ["dev", "test", "docs", "ci", "tox"].iter().any(|s| matches_scope_filename(name, s)) {'),
    "M7-no-r5-pass": ("waybill-cli/src/scan_fs/ci_scope.rs",
        "        if runtime && !c.evidence.source_file_paths.is_empty() {",
        "        if false && runtime && !c.evidence.source_file_paths.is_empty() {"),
    "M8-any-not-all": ("waybill-cli/src/scan_fs/ci_scope.rs",
        "                c.evidence.source_file_paths.iter().map(|p| class_of(p, roots)).collect();",
        "                Some(c.evidence.source_file_paths.iter().filter_map(|p| class_of(p, roots)).collect::<Vec<_>>()).filter(|v: &Vec<DotDirClass>| !v.is_empty());"),
    "M9-overwrites-scope": ("waybill-cli/src/scan_fs/ci_scope.rs",
        "        let runtime = matches!(c.lifecycle_scope, None | Some(LifecycleScope::Runtime));\n        if runtime &&",
        "        let runtime = true;\n        if runtime &&"),
    "M10-optional-pair-not-kept": ("waybill-cli/src/resolve/deduplicator.rs",
        "        (Some(LifecycleScope::Optional), Some(LifecycleScope::Optional)) => {\n            Some(LifecycleScope::Optional)\n        }\n",
        ""),
    "M11-cross-root-first-wins": (P + "pip/mod.rs",
        "                kept.lifecycle_scope = crate::resolve::deduplicator::merge_lifecycle_scope(\n                    kept.lifecycle_scope,\n                    a.lifecycle_scope,\n                );\n",
        ""),
    "M12-explicit-runtime-skipped": ("waybill-cli/src/scan_fs/ci_scope.rs",
        "        let runtime = matches!(c.lifecycle_scope, None | Some(LifecycleScope::Runtime));\n        if runtime &&",
        "        let runtime = matches!(c.lifecycle_scope, None);\n        if runtime &&"),
    "M13-pypi-key-declared-spelling": ("waybill-cli/src/resolve/deduplicator.rs",
        "        let name = if component.purl.ecosystem() == \"pypi\" {",
        "        let name = if false {"),
    "M14-every-ecosystem-normalised": ("waybill-cli/src/resolve/deduplicator.rs",
        "        let name = if component.purl.ecosystem() == \"pypi\" {",
        "        let name = if true {"),
    "M15-scoped-file-covers-runtime": (P + "pip/mod.rs",
        "                || covered_only_by_scoped_requirements(&entries, &entry)",
        "                || false && covered_only_by_scoped_requirements(&entries, &entry)"),
}

name = sys.argv[1]
path, old, new = MUTATIONS[name]
text = open(path).read()
assert text.count(old) == 1, f"{name}: anchor occurs {text.count(old)} times in {path}"
open(path, "w").write(text.replace(old, new))
print(path)
