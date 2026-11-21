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
- Public corpus: `public-corpus.yml` run 44916104278 (`branch=1639-also-detected-via`, `regen_goldens=true`). Every job's conclusion: `Run public corpus` `success`, `Report corpus failure` `skipped`. The run summary's `SCANNED_SHA` is `244e8c3676249386ff83eca2c474f29da36bbf57`. The `corpus-goldens-regen` artifact holds 67 files; `cmp` against the committed `waybill-cli/tests/fixtures/public_corpus/` found **0 differing**.

## The census (T005)

`kusari-sandbox/waybill-fixture-therock-sdk` commit `1aa4e67` adds `census/merges.py` and `.github/workflows/census-merges.yml`. Run 44916219886 dispatched with `waybill_ref=244e8c3676249386ff83eca2c474f29da36bbf57`.

**Every job's conclusion:** `census (therock)`, `census (cmake-nlohmann-json)`, `census (python-pytorch)` and `census (cpp-mongo)` all `success`. Each job's `refs.txt` names waybill `244e8c3676249386ff83eca2c474f29da36bbf57`. Trees as scanned: TheRock `b1cae390`, nlohmann-json `55f93686` (the pin `65ee6845` is the annotated tag `v3.12.0`), pytorch `2b3ec348`, mongo `d100bf19` (the pin `41a57524` is the annotated tag `r8.3.8`).

**Events and merge groups by site.** A merge group is (site, survivor PURL).

| Target | events | groups by site | groups that would record C56 | groups with no survivor C55 (R5) |
|---|---|---|---|---|
| TheRock | 1,141 | S1 69, S3 6, S5 34, S6 3, S8 3, S9 68 | S1 6, S5 8, S6 3, S8 3, S9 68 (**88**) | S3 3 |
| python-pytorch | 33 | S1 11, S3 1, S5 2, S9 4 | S9 4 (**4**) | none |
| cmake-nlohmann-json | 10 | S5 1, S9 1 | S9 1 (**1**) | none |
| cpp-mongo | 0 | none | none | none |

**Mechanism pairs** (kept → absorbed: groups).

TheRock:
- S1: `cmake-find-package` → `cmake-pkg-check-modules` 6;
- S5: `cmake-externalproject` → `cmake-fetchcontent-url` 6, `cmake-fetchcontent-git` → `cmake-externalproject` 2;
- S6: `git-submodule` → `cmake-fetchcontent-git` 2, `git-submodule` → `cmake-externalproject` 1;
- S8: `git-submodule` → `cmake-project` 3;
- S9: `cmake-project` → `cmake-find-package` 57, `cmake-project` → `cmake-pkg-check-modules` 2, `git-submodule` → `cmake-find-package` 10, `git-submodule` → `cmake-pkg-check-modules` 1.

python-pytorch:
- S9: `git-submodule` → `cmake-find-package` 3, `cmake-project` → `cmake-find-package` 1.

cmake-nlohmann-json:
- S9: `cmake-project` → `cmake-find-package` 1.

**The ranking FR-003 declined** (research R2):
- On S1 groups it would have chosen a different survivor in **0** of 69 groups (TheRock), 0 of 11 (pytorch), and none elsewhere.
- **238** of TheRock's 249 S1 events and all 17 of pytorch's were `unranked`, because at least one side's C55 is outside its 16 variants, or absent.
- Keeping today's rule changes no C55 on these targets, and the ranking could not have ordered most merges at all.

**Fingerprint collision under the old key:** 0 on every target.

**S2, S4 and S7 logged nothing.**
- S7 needs `--include-vendored`, which these scans do not pass.
- S2 is the Maven declared-not-cached fold. Only python-pytorch has Maven components (4), and none of them was a declared-not-cached entry with an on-disk twin.
- S4 (first declaration wins) merges a `FetchContent_Declare` into an ancestor's of the same name. No such pair met in these trees as scanned.

A-9 covers S7, and the unit test `s1639_the_maven_fold_records_a_different_mechanism` covers S2.

**cpp-mongo's zero** is not a broken probe: its scan emitted 419 components, and the same build logged events on the other three targets.

**What changes, by site, on TheRock:** 68 of the 88 groups are package searches folded into what they name (S9). Spec FR-004 includes them by decision (analysis I2).

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
