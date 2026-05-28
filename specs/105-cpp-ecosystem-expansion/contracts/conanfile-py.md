# Contract: `conanfile.py` reader extension (US2)

**Maps to**: FR-003, FR-004 | **Source-mechanism**: `conan-recipe` (same enum value as existing `conanfile.txt` reader)

## Trigger

A file named `conanfile.py` appears anywhere under the scan root.

## Parsing strategy

Regex + AST-light heuristics. No Python execution. No `rustpython` dependency. The parser is line-oriented over the raw file with awareness of multi-line tuples/lists.

### Recognized declaration shapes

```python
# Class-attribute form
class MyRecipe(ConanFile):
    requires = ("zlib/1.3.1", "openssl/3.0.0")
    build_requires = ("ninja/1.11.1",)
    tool_requires = ("cmake/3.27.7",)

# Method form
class MyRecipe(ConanFile):
    def requirements(self):
        self.requires("zlib/1.3.1")
        self.tool_requires("cmake/3.27.7")
        if self.settings.os == "Linux":
            self.requires("libudev/255")

# Tuple/list mixed quoting
    requires = [
        "boost/1.84.0",
        'foo/1.2.3',  # comment ignored
    ]
```

### Lifecycle-scope mapping (FR-004)

| Conan kind | `mikebom:lifecycle-scope` value |
|---|---|
| `requires` (or `self.requires`) | `"runtime"` |
| `build_requires` (or `self.build_requires`) | `"build"` |
| `tool_requires` (or `self.tool_requires`) | `"build"` |

### Skipped shapes (with `tracing::warn!`)

- `self.requires(f"{name}/{version}")` — f-string with non-literal content
- `self.requires(get_dep_name())` — function-call argument
- `requires = COMPUTED_LIST` — variable-name reference (not a literal tuple/list)

Each warn names the file, line number, and a snippet of the offending source.

## PURL derivation

For every parsed `<name>/<version>` token:

```
pkg:conan/<name>@<version>
```

## Annotations emitted

| Annotation | Value |
|---|---|
| `mikebom:source-mechanism` | `"conan-recipe"` |
| `mikebom:source-files` | absolute path of `conanfile.py` |
| `mikebom:lifecycle-scope` | `"runtime"` or `"build"` per table above |
| `mikebom:lifecycle-scope-guard` | (optional) string of the conditional guard if declared inside `if self.settings...:` block |

## Dedup interaction

A directory may contain both `conanfile.txt` AND `conanfile.py`. Both are
parsed; both emit `DetectionRecord`s with the same `conan-recipe`
source-mechanism. The dedup pipeline (FR-015) collapses duplicates by
canonical PURL — no double-counting (per US2 scenario 4).

## Test cases (US2 acceptance scenarios mapped)

| US2 Scenario | Fixture | Asserted PURL |
|---|---|---|
| 1 (class-attribute requires) | `golden_inputs/conanfile_py/class_attr/` | `pkg:conan/zlib@1.3.1`, `pkg:conan/openssl@3.0.0` |
| 2 (method-form `self.requires`) | `golden_inputs/conanfile_py/method_form/` | `pkg:conan/foo@1.2.3` |
| 3 (lifecycle-scope split) | `golden_inputs/conanfile_py/mixed_kinds/` | runtime + build scopes correctly tagged |
| 4 (both .txt and .py) | `golden_inputs/conanfile_py/dual_recipes/` | each dep appears once (dedup) |
