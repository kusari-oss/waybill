# Data model: test scope for installed trees

All state is in process for one scan.

## TestDeclaration (phase 1, from the shared walker)

One record per declaring file. Two kinds:

| Kind | Source | Fields |
|---|---|---|
| `ExportedTargets` | an exported-targets file (#1563) whose base package name ends in `-tests`, `_tests`, `-test` or `_test` (case-insensitive) | the #1563 `TargetsFile` it came from; the package name |
| `CTest` | an installed `CTestTestfile.cmake` | `dir`; for each `add_test` (positional or `NAME … COMMAND …`), its command words; the count of `include()` lines, which are never followed (research R1) |

Both are parsed textually with #1531's CMake tokenizer (`cmake_version::commands`). Nothing is evaluated.

An OS-owned declaring file is skipped and counted, as with #1531 (FR-013).

## Installed-tree recognition (FR-015)

`InstalledTree` is one of:
- `Image` for every `--image` scan;
- `Recognised` for a `--path` scan with either kind of evidence (research R5):
  - at least one #1531 pkg-config or CMake-version record in a recognised install location that is not OS-owned;
  - or an OS package database that claimed at least one path;
- `NotRecognised` otherwise.

Only `Image` and `Recognised` enable the path and name rules. Declarations apply in all three.

## Declared locations (phase 2)

Every location string a declaration names is resolved once:
- **Exported targets:** #1563's own resolution (`${_IMPORT_PREFIX}` and the raise count). It is reused, not repeated.
- **CTest:** relative to the CTest file's directory, inside the scan root (#1531 `location::resolve_in_root`).

A location that uses a variable, a generator expression, or an absolute path outside the scan root is `unresolved`. One that escapes the root is `outside-root`. One that resolves to nothing is `not-shipped`. Otherwise it is `resolved`, and it joins the set `declared_test_paths` (root-relative).

An interpreter command (`python3`, `python`, `bash`, `sh`, `perl`, `env`) is never itself a location. Each of its non-option arguments that resolves to a shipped file is a location instead (US1 scenario 3, research R1). Each `include()` in a CTest file counts as one `unresolved` location.

## TestRule and its strength

Strongest first. The C62 value is what the component carries.

| Rule | Strength | C62 `waybill:lifecycle-scope-derivation` |
|---|---|---|
| exported targets of a test package | 1 | `exported-targets-test-package` |
| installed CTest file | 1 | `ctest-declaration` |
| path convention | 2 | `test-directory` |

When the two declarations both name a file, `exported-targets-test-package` is recorded, because it also scopes the package component.

## Which components are scoped (FR-008)

The pass runs once, after file-tier emission and before `--exclude-scope` (research R2). It reads each component's own files:
- **Binary file component:** `evidence.source_file_paths`.
- **File-tier component:** every path in `waybill:file-paths`.
- **The installed-library component of a declared test package:** scoped by the declaration itself, not by a path.

**All paths must match.** A component is scoped only if *every* path it records matches a rule, and the recorded reason is the weakest rule among those paths (research R6). Content found both under `tests/` and in `lib/` is product.

**`--include-test-binaries` (FR-012, research R10):** a binary file component whose every path lies under #1524's `TEST_INPUT_DIRS` is scoped `test-directory` regardless of recognition.

**Never scoped by this pass:**
- a component that already carries a lifecycle scope;
- an OS-package-owned component: any of its paths is in the OS claim set (`os_claimed_paths`, or `os_claimed_inodes` on unix), carried out of `scan_path`. Binary components for claimed paths are never emitted, but under `--file-inventory=full` a file-tier component for a claimed file can exist;
- the scan subject;
- any component of an ecosystem other than binary file components, file-tier components and #1531 installed-library components. A pip, npm or cargo component's scope belongs to its reader.

**A scoped component gets:**
- `lifecycle_scope = Test`;
- C62 = the strongest rule's value.

Nothing else changes on the component, except SPDX 3's purpose at emission (contracts/wire.md).

## TestScopeCoverage (C226, document scope)

```json
{"declarations":{"ctest":N,"exported-targets":N,"owned-by-os-package":N},
 "installed-tree":"image|recognised|not-recognised",
 "locations":{"not-shipped":N,"outside-root":N,"resolved":N,"unresolved":N},
 "scoped":{"ctest-declaration":N,"exported-targets-test-package":N,"test-directory":N}}
```

The JSON keys are sorted. The annotation is emitted only when at least one declaration was read or at least one component was scoped. Otherwise it is absent, and the document is byte-identical to today's (FR-011).
