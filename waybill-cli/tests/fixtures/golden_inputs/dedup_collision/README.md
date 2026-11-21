# `dedup_collision` fixture (milestone 105 T024)

Synthetic two-reader collision fixture. Used by `tests/dedup_precedence_determinism.rs` to verify the FR-015 / SC-010 dedup determinism guarantee end-to-end.

## Shape

The same library (`abseil`) is declared by two distinct manifest sources:

- **`conanfile.txt`** — declares `abseil/20240722.0` under `[requires]`. The conan reader (active since alpha.41) emits `pkg:conan/abseil@20240722.0` with `waybill:source-mechanism: "conan-recipe"`.
- **`.gitmodules`** — declares `third_party/abseil` pointing at `https://github.com/abseil/abseil-cpp.git`. The git-submodule reader (introduced by milestone 105 US6) will emit `pkg:github/abseil/abseil-cpp@<sha>` with `waybill:source-mechanism: "git-submodule"` and a `waybill:build-reference` annotation.

## Behaviour since spec 1530 (#1515)

Milestone 105 expected the two readers' components to merge after an unspecified "canonicalization" of `pkg:conan/abseil@…` and `pkg:github/abseil/abseil-cpp@…`. None was ever specified. A package-manager name and a repository are different identities, and mapping one to the other would be a guess. Spec 1530 (FR-009) merges a submodule with another reader's component **only on an identical canonical PURL**, so this fixture yields two components:

- `pkg:conan/abseil@20240722.0`, with `waybill:source-mechanism: conan-recipe`;
- `pkg:github/abseil/abseil-cpp@<commit>`, with `waybill:source-mechanism: git-submodule` and `waybill:build-reference: declared-and-used` (via `find_package(Abseil)`).

The in-tree copy cannot carry a gitlink, because a gitlink is a tree entry of the repository holding it. So `tests/dedup_precedence_determinism.rs` copies these files into a temporary repository and adds the gitlink with `git update-index --cacheinfo 160000,…`. Scanned in place, inside waybill's own checkout, the fixture's `.gitmodules` has no enclosing repository at or below the scan root, and is counted under `no-repository-data` in `waybill:git-submodule-coverage` (C222).

The two have different PURLs, so they do not merge and neither carries `waybill:also-detected-via` (C56). C56 is recorded only when components merge (spec 1639, #1558).

## Files

```
dedup_collision/
├── README.md        # this file
├── CMakeLists.txt   # `find_package(Abseil)` for build-reference correlation
├── conanfile.txt    # `[requires]\nabseil/20240722.0`
└── .gitmodules      # `[submodule "third_party/abseil"]` block
```

`third_party/abseil/` holds only a `.gitkeep`. The commit comes from the gitlink the test adds.
