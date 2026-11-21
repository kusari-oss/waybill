"""Spec 1638 mutations: apply one by name, by exact string replacement.

Usage: python3 -I -B mutate.py <name>   (from the repository root). Prints
the mutated file. Each anchor must occur exactly once.
"""
import sys

CM = "waybill-cli/src/scan_fs/package_db/cmake.rs"
FE = "waybill-cli/src/scan_fs/package_db/cmake/fetches.rs"
RC = "waybill-cli/src/resolve/reconciler.rs"
MUTATIONS = {
    # M1: the unresolved GitHub branch emits pkg:generic again.
    "M1-github-branch-off": (CM,
        "            let purl = match literal_repo.and_then(github_pair) {",
        "            let purl = match literal_repo.and_then(|_| None::<(String, String)>) {"),
    # M2: drop the repository-has-no-variable check.
    "M2-literal-repo-check-off": (CM,
        "(Some(g), Some(_)) if !has_var(&git_repo) && has_var(&git_tag) && !has_var(&url) => Some(g.as_str()),",
        "(Some(g), Some(_)) if has_var(&git_tag) && !has_var(&url) => Some(g.as_str()),"),
    # M3: no URL for the non-GitHub shape (and, as a side effect, none for GitHub).
    "M3-no-download-url": (CM,
        "build_cmake_entry(name, \"\", source_path, purl, literal_repo, url_hash, false, source_mechanism);",
        "build_cmake_entry(name, \"\", source_path, purl, None, url_hash, false, source_mechanism);"),
    # M4: remove the wrapper-recognition mask.
    "M4-wrapper-mask-off": (FE,
        "            .is_some_and(|d| !(d.name.is_empty() && d.entry.extra_annotations.contains_key(\"waybill:unresolved-reason\")))",
        "            .is_some()"),
    # M5: remove the reconciler guard.
    "M5-reconciler-guard-off": (RC,
        "        if is_versionless_github_fetch(design) {\n            continue;\n        }",
        "        if false && is_versionless_github_fetch(design) {\n            continue;\n        }"),
    # M6: pass the URL for an unresolved URL fetch.
    "M6-url-fetch-gets-url": (CM,
        "build_cmake_entry(name, \"\", source_path, purl, literal_repo, url_hash, false, source_mechanism);",
        "build_cmake_entry(name, \"\", source_path, purl, literal_repo.or(url.as_deref()), url_hash, false, source_mechanism);"),
}

name = sys.argv[1]
path, old, new = MUTATIONS[name]
text = open(path).read()
assert text.count(old) == 1, f"{name}: anchor occurs {text.count(old)} times in {path}"
open(path, "w").write(text.replace(old, new))
print(path)
