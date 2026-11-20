"""Spec 1617 mutations: apply one by name, by exact string replacement.

Usage: python3 -I -B mutate.py <name>   (from the repository root). Prints
the mutated file. Each anchor must occur exactly once.
"""
import sys

SP = "waybill-cli/src/scan_fs/cmake_search_pair.rs"
MUTATIONS = {
    "M1-ships-check-always-false": (SP,
        "    c.evidence.source_file_paths.iter().any(|p| {",
        "    false && c.evidence.source_file_paths.iter().any(|p| {"),
    "M2-shipping-record-required": (SP,
        "            out.conditions_only.insert(purl.to_string(), conds);",
        "            out.required.insert(purl.to_string());\n            let _ = conds;"),
    "M3-conditions-only-writes-no-c236": (SP,
        "        } else if let Some(conds) = scopes.conditions_only.get(&purl) {\n            conditions(c, conds);\n        }",
        "        } else if let Some(conds) = scopes.conditions_only.get(&purl) {\n            let _ = conds;\n        }"),
    "M4-test-records-made-optional": (SP,
        "            c.lifecycle_scope = Some(LifecycleScope::Test);",
        "            c.lifecycle_scope = Some(LifecycleScope::Optional);"),
    "M5-derivation-precedence-reversed": (SP,
        "            let derivation = if ps.iter().any(|p| p.test_derivation.as_deref() == Some(DERIVATION_SOURCE_TEST_DIRECTORY)) {\n                DERIVATION_SOURCE_TEST_DIRECTORY\n            } else {\n                DERIVATION_CMAKE_BUILD_TESTING\n            };",
        "            let derivation = if ps.iter().any(|p| p.test_derivation.as_deref() == Some(DERIVATION_CMAKE_BUILD_TESTING)) {\n                DERIVATION_CMAKE_BUILD_TESTING\n            } else {\n                DERIVATION_SOURCE_TEST_DIRECTORY\n            };"),
    "M6-static-archive-not-counted": (SP,
        "[\".so\", \".a\", \".dylib\", \".dll\", \".lib\"]",
        "[\".so\", \".dylib\", \".dll\", \".lib\"]"),
    "M7-optional-pairing-made-test": (SP,
        "        } else if ps.iter().all(|p| p.test_derivation.is_some()) {",
        "        } else if ps.iter().any(|p| p.test_derivation.is_some()) {"),
}

name = sys.argv[1]
path, old, new = MUTATIONS[name]
text = open(path).read()
assert text.count(old) == 1, f"{name}: anchor occurs {text.count(old)} times in {path}"
open(path, "w").write(text.replace(old, new))
print(path)
