"""Spec 1615 mutations: apply one by name, by exact string replacement.

Usage: python3 -I -B mutate.py <name>   (from the repository root). Prints
the mutated file. Each anchor must occur exactly once; the script refuses
otherwise.
"""
import sys

R = "waybill-cli/src/scan_fs/package_db/pip/requirements_txt.rs"
MUTATIONS = {
    "M1-no-doc-word": (R,
        '    ("docs", &["doc", "docs", "documentation"]),',
        '    ("docs", &["docs", "documentation"]),'),
    "M2-no-long-forms": (R,
        '    ("dev", &["dev", "development"]),\n    ("test", &["test", "tests", "testing"]),\n    ("docs", &["doc", "docs", "documentation"]),',
        '    ("dev", &["dev"]),\n    ("test", &["test", "tests"]),\n    ("docs", &["doc", "docs"]),'),
    "M3-no-tests-word": (R,
        '    ("test", &["test", "tests", "testing"]),',
        '    ("test", &["test", "testing"]),'),
    "M4-no-docs-directory-names": (R,
        '        "docs" | "doc" | "documentation" => Some("docs"),\n        "test" | "tests" => Some("test"),',
        '        "test" | "tests" => Some("test"),'),
    "M5-no-test-directory-names": (R,
        '        "docs" | "doc" | "documentation" => Some("docs"),\n        "test" | "tests" => Some("test"),',
        '        "docs" | "doc" | "documentation" => Some("docs"),'),
    "M6-farthest-first": (R,
        "    rel.parent()?.components().rev().filter_map(|c| c.as_os_str().to_str()).find_map(|name| {",
        "    rel.parent()?.components().filter_map(|c| c.as_os_str().to_str()).find_map(|name| {"),
    "M7-dot-directories-searched-first": (R,
        "    rel.parent()?.components().rev().filter_map(|c| c.as_os_str().to_str()).find_map(|name| {\n        listed_dot_dir_class(name).map(|c| c.requirements_scope()).or_else(|| docs_or_test_dir_scope(name))\n    })",
        "    listed_ancestor_class(path, root).map(|c| c.requirements_scope()).or_else(|| {\n        rel.parent()?.components().rev().filter_map(|c| c.as_os_str().to_str()).find_map(docs_or_test_dir_scope)\n    })"),
    "M8-root-name-counts": (R,
        "    let Ok(rel) = path.strip_prefix(root) else {",
        "    let Ok(rel) = path.strip_prefix(root.parent().unwrap_or(root)) else {"),
    "M9-directory-prefix-match": (R,
        "    match name.to_ascii_lowercase().as_str() {\n        \"docs\"",
        "    let n = name.to_ascii_lowercase();\n    if n.starts_with(\"doc\") { return Some(\"docs\"); }\n    if n.starts_with(\"test\") { return Some(\"test\"); }\n    match n.as_str() {\n        \"docs\""),
    "M10-reader-keeps-old-words": (R,
        "    if name_scope(name).is_some() {\n        return true;\n    }",
        "    if [\"dev\", \"test\", \"docs\", \"ci\"].iter().any(|s| matches_scope_filename(name, s)) {\n        return true;\n    }"),
    "M11-c155-kept-on-runtime": ("waybill-cli/src/scan_fs/ci_scope.rs",
        "            && c.extra_annotations.remove(ANN_REQ_FILE_SCOPE).is_some()",
        "            && c.extra_annotations.get(ANN_REQ_FILE_SCOPE).is_some() && false"),
    "M12-optional-beats-runtime-declaration": ("waybill-cli/src/resolve/deduplicator.rs",
        "        (Some(LifecycleScope::Optional), Some(LifecycleScope::Optional)) => {\n            Some(LifecycleScope::Optional)\n        }",
        "        (Some(LifecycleScope::Optional), _) | (_, Some(LifecycleScope::Optional)) => {\n            Some(LifecycleScope::Optional)\n        }"),
}

name = sys.argv[1]
path, old, new = MUTATIONS[name]
text = open(path).read()
assert text.count(old) == 1, f"{name}: anchor occurs {text.count(old)} times in {path}"
open(path, "w").write(text.replace(old, new))
print(path)
