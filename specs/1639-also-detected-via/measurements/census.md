# T004–T006: log-only build, census, and the sites not wired

## The log-only commit (T004)

Commit `244e8c3676249386ff83eca2c474f29da36bbf57`. Every merge site S1–S9 (research R3) calls `also_detected::observe`, which only emits a `waybill::also_detected` debug event; `record` is not called anywhere.

**Positive controls**, on a scratch super-project scanned by this build with `RUST_LOG=waybill::also_detected=debug`:

| Tree | Event logged |
|---|---|
| submodule of `waybill-org/waybill-fixture-dep` at `aaaa…` plus a required `FetchContent_Declare` of the same repository at the same commit | one `S1` event: kept `cmake-fetchcontent-git`, absorbed `git-submodule`, `rank_winner="survivor"` |
| the same submodule plus `find_package(waybill-fixture-dep 1.0)` and no fetch | one `S9` event: kept `git-submodule`, absorbed `cmake-find-package` |

The first tree with a `find_package` of the fetched name logged no S9 event. That is correct: the search is classified `in-tree`, because the name is declared by a fetch in the tree, and removed with no target.

**The events carry ANSI colour codes** even when stderr is a file. `census/merges.py` strips them, and was run against both control logs before being committed. It reported exactly one group, one recorded mechanism, for each.

**Output unchanged.**
- A-ctl and every suite listed in T014 pass on this commit.
- Public corpus: `public-corpus.yml` run 44916104278 (`branch=1639-also-detected-via`, `regen_goldens=true`). Result below.

## The census (T005)

`kusari-sandbox/waybill-fixture-therock-sdk` commit `1aa4e67` adds `census/merges.py` and `.github/workflows/census-merges.yml`. Run 44916219886 dispatched with `waybill_ref=244e8c3676249386ff83eca2c474f29da36bbf57`.

Results below, once the run completes. No prediction is recorded.

## The sites not wired (T006)

**Method.**
- `grep -rn` for the key `"waybill:source-mechanism"` written with `insert(`, `.to_string()` or `.into()`, and for every constant naming it.
- A file counts as a production setter only if the write is above its first `#[cfg(test)]`.
- No `head`; the full lists were read.

**Production setters** are all readers, in `scan_fs/package_db/`:
- `bazel.rs`, `cmake.rs`, `cmake/fetches.rs`, `cmake/projects.rs`, `conan.rs`;
- `git_submodule/mod.rs`, `helm.rs`, `installed_lib/resolve.rs`;
- `ipk_file.rs`, `maven.rs`, `nuget/deps_json.rs`, `nuget/pe_clr.rs`, `opkg.rs`;
- `pip/requirements_txt.rs`, `vcpkg.rs`, `yocto/manifest.rs`, `yocto/recipe.rs`.

**Other files that write the key do so only in test modules**:
- `generate/root_selector.rs:1764`;
- `resolve/reconciler.rs:449`;
- `scan_fs/binary/source_binding/cmake_observer.rs:188`;
- `scan_fs/cmake_fetch_attach.rs:84, 109`;
- `scan_fs/declared_attach.rs:329`;
- `scan_fs/mod.rs:2896, 3764`;
- `scan_fs/test_scope.rs:456`.

So research R4's invariant holds: no merge pass writes C55, except S1's existing fold.

**Result: no setter reaches a site that is not wired.**
- The Nix closure merges and the Nix twin merge handle Nix components, which no reader above gives C55.
- The supplement merge handles operator input.
- Go, gem and Haskell merge within their own readers, none of which sets C55.
- `pe_clr::absorb` aggregates assemblies inside the `dotnet-assembly-metadata` reader, so both sides have the same mechanism.
- Its cross-reader meeting with `dotnet-deps-json` goes through S1.
- The Python and JDK binary collapsers produce entries; they do not merge components.

Nothing is added to T004.
