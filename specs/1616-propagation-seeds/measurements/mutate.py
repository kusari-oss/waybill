"""Spec 1616 mutations: apply one by name, by exact string replacement.

Usage: python3 -I -B mutate.py <name>   (from the repository root). Prints
the mutated file. Each anchor must occur exactly once.
"""
import sys

SP = "waybill-cli/src/resolve/scope_propagation.rs"
MUTATIONS = {
    "M1-no-added-seeds": (SP,
        "        .filter(|c| is_runtime_seed_candidate(c, roots))\n",
        "        .filter(|c| is_runtime_seed_candidate(c, roots) && false)\n"),
    "M2-non-runtime-main-modules-seeded": (SP,
        "    !c.lifecycle_scope.is_some_and(|s| s.is_non_runtime())\n        && ",
        "    true\n        && "),
    "M3-dot-directory-main-modules-seeded": (SP,
        "        && !crate::scan_fs::ci_scope::declared_only_under_listed_dirs(c, roots)\n",
        "        && true\n"),
    "M4-added-seeds-replace-seeds": (SP,
        "        let mut immune = runtime_reachable(components, &adj, seeds);\n        // Spec 1616",
        "        let mut immune = runtime_reachable(components, &adj, if added_seeds.is_empty() { seeds } else { added_seeds });\n        // Spec 1616"),
    "M5-count-includes-base-protection": (SP,
        "                    if added_only.contains(purl) && !scope_by_purl.contains_key(purl) {",
        "                    if !scope_by_purl.contains_key(purl) {"),
    "M6-added-seeds-cross-excluded-edges": (SP,
        "            runtime_reachable(components, &adj, &all)",
        "            {\n                let mut full: HashMap<&str, Vec<&str>> = HashMap::new();\n                for r in relationships {\n                    full.entry(r.from.as_str()).or_default().push(r.to.as_str());\n                }\n                runtime_reachable(components, &full, &all)\n            }"),
    "M4b-base-seeds-dropped": (SP,
        "        let mut immune = runtime_reachable(components, &adj, seeds);\n        // Spec 1616",
        "        let seeds = if added_seeds.is_empty() { seeds } else { added_seeds };\n        let mut immune = runtime_reachable(components, &adj, seeds);\n        // Spec 1616"),
}

name = sys.argv[1]
path, old, new = MUTATIONS[name]
text = open(path).read()
assert text.count(old) == 1, f"{name}: anchor occurs {text.count(old)} times in {path}"
open(path, "w").write(text.replace(old, new))
print(path)
