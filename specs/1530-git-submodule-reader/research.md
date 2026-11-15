# Research: Git submodules as components

Spec: [spec.md](spec.md). Probes and their outputs are in `measurements/`. Every number below was produced by them on 2026-11-15 (host aarch64-darwin, git 2.56.0) unless marked as inference.

## R1 — Reading a gitlink without running a program the scanned repository chose

**Decision.** Read gitlinks with:

```
git -C <repo> -c core.fsmonitor=false -c core.hooksPath=/dev/null --no-pager ls-tree -r -z --full-tree HEAD
```

It runs with stdin null, `GIT_TERMINAL_PROMPT=0`, `GIT_OPTIONAL_LOCKS=0` and a time budget. Only entries of mode `160000` are kept. Never use a command that reads the index (`ls-files`, `status`, `describe --dirty`, `diff`).

**Measured** (`measurements/git-safety.txt`, probe `measurements/probe/git-safety.sh`). The probe gave a throwaway repository a hostile `.git/config`, setting `core.fsmonitor`, `core.pager` and `core.hooksPath` to a script that writes a marker:

| command | ran the repository's program |
| --- | --- |
| `ls-files --stage` | **YES** |
| `status --porcelain` | **YES** |
| `describe --tags --always --dirty` (control) | **YES** |
| `ls-tree -r -z HEAD` | no |
| `rev-parse --show-toplevel` | no |
| `-c core.fsmonitor=false ls-files --stage` | no |
| `-c core.fsmonitor=false -c core.hooksPath=/dev/null status` | no |
| `log -1`, with `core.pager` set and stdout not a tty | no |

Any command that loads the index runs `core.fsmonitor`. That is code execution chosen by the scanned repository, and scanning untrusted repositories is this reader's normal case. `ls-tree` reads objects only. The `-c` overrides are defence in depth, in case a later change adds an index-reading command.

**The git commands waybill already runs in scanned repositories** were probed the same way: `describe --tags [--exact-match] [--always]` (golang and gem readers), `rev-parse HEAD`, `remote get-url origin` and `config --get remote.origin.url` (binding auto-detect). None ran the program. The `--dirty` control shows the probe would have caught it if they had.

**Alternatives rejected.**
- *Reading `.git/modules/<name>/HEAD`* (milestone 105's design). It is absent on a fresh clone: R3 measured `.git/modules exists: no` after `git clone` without `--recurse-submodules`.
- *Parsing the object store in Rust.* That means packfiles, zlib and deltas, a new Cargo dependency or a lot of code, for no gain over `ls-tree`.
- *`git ls-files --stage`.* It runs `core.fsmonitor`.

## R2 — A repository git refuses (`safe.directory`)

**Decision.** Never pass `safe.directory`. When git refuses a repository with "detected dubious ownership" (exit 128), record the skip under a new reason, `git-refused`, and warn with the remedy (`git config --global --add safe.directory <path>`).

**Measured.** With `GIT_TEST_ASSUME_DIFFERENT_OWNER=1`, both `ls-tree` and `rev-parse` exit 128 with `fatal: detected dubious ownership in repository at '<path>'`.

**Rationale.** `safe.directory` exists to stop exactly R1's risk: a foreign-owned repository's config running code. Overriding it on the operator's behalf would undo git's own protection. The reason is distinct from `git-unavailable` because the remedy is different. That makes this a spec amendment: FR-011 gains `git-refused`.

## R3 — The gitlink is present on a fresh clone

**Measured.** After `git clone` of a super-project, without initialising the submodule:
- `ls-tree -r HEAD` lists `160000 commit 9ada6e3b… third_party/waybill-fixture-dep`, the submodule repository's own commit;
- `.git/modules` does not exist;
- the submodule directory is empty.

FR-003 is satisfiable as written.

## R4 — Cost of reading the whole tree

**Measured** (`measurements/ls-tree-timing.txt`, 5 runs each, on trusted local clones):

| repository | entries | output | median | max |
| --- | --- | --- | --- | --- |
| malicious-packages | 227,970 | 25.7 MB | 0.314 s | 0.318 s |
| grafana | 21,988 | 2.5 MB | 0.024 s | 0.024 s |
| mikebom | 6,599 | 0.76 MB | 0.012 s | 0.012 s |

**Decision.** One full `ls-tree -r` per repository: the super-project and each initialised submodule. A per-call budget of 30 s is two orders of magnitude above the largest measurement.

**Gate.** The reader runs `git` only when the shared walker saw a `.gitmodules` file under the scan root. A tree without one pays nothing and changes no output (R13).

## R5 — Attaching a top-level submodule to the document root (FR-010)

**Finding.**
- **The parentless fallback.** All three emitters fall back to "root depends on every parentless component" only when the root has no outgoing edges (`cyclonedx/dependencies.rs`, and the SPDX twins in `spdx/document.rs` and `spdx/v3_document.rs`). A super-project with any main module would therefore leave its submodules orphaned. That is the defect #1009 found for supplement components: 43 of 44 orphaned on mongo.
- **The anchor channel.** #1009 fixed it with an unconditional root-anchor channel, `supplement::current_root_anchors()`, which all three emitters already honour.

**Decision.** Widen the anchor channel's source. The anchors become the supplement's anchors plus every component with `waybill:source-mechanism: git-submodule` that no other submodule depends on. Byte-identity on the default path holds: the set is empty when there is no supplement and no submodule.

**Alternative rejected.** Attaching submodules to a main module. Many super-builds (CMake-only) have none, and the root is chosen only at emission time.

## R6 — Nested submodule edges (FR-010)

**Finding.** A reader's `depends` entries are resolved by bare name within the entry's ecosystem (`scan_fs/mod.rs`, at the `for dep_name in &entry.depends` loop). That cannot express a nested submodule edge. The child may have a different PURL type from its parent (`github` inside `generic`), and two GitHub repositories with the same name under different owners collide on the name key.

**Decision.** A `depends` entry that is a canonical PURL (starts with `pkg:` and matches an emitted component's PURL exactly) resolves by exact match, before the name lookup.

**Measured safety.**
- `grep` finds no reader in `scan_fs/` that puts a `pkg:` string into `depends`.
- Such an entry resolves to nothing today, because no name key starts with `pkg:`. The change therefore only adds edges that currently do not exist.

Implementation re-runs that grep, so the finding is checked again at that point and not trusted from here.

## R7 — Parsing `.gitmodules`

**Decision.** Hand-written, git-config subset:
- `[submodule "<name>"]` sections, with case-insensitive `path` and `url` keys;
- quoted values with `\"` and `\\` escapes;
- `#` and `;` comments, and line continuations.

`git config -f` is not used. It would be one more subprocess, and git-config files can carry `include` directives.

**Rejected values** (FR-012, reason `unsafe-path-or-url`):
- a `path` that is absolute, contains a `..` component, or contains a NUL or newline;
- a `url` beginning with `-`, the CVE-2018-17456 class of option injection.

Values reach `git` only as data after `--`, never as options.

## R8 — Relative submodule URLs

**Decision.** Resolve `./x` and `../x` the way git does, against the super-project's `remote.origin.url`, read with `git config --get remote.origin.url` (R1 measured it safe). Each leading `../` removes one trailing path component of the remote URL. With no `origin`, the URL stays relative in `vcs_url`, and `<name>` is its last segment.

## R9 — Forge mapping

**Decision.** Use one host table, shared with the Yocto reader (clarification 3). Sharing the table does not make every identity match. Yocto truncates commit versions to 12 characters (`yocto/recipe.rs`), so a Yocto component and a submodule at the same commit still differ in version. Casing is R16. The table: `github.com` → `github`, `gitlab.com` → `gitlab`, `bitbucket.org` → `bitbucket`, `codeberg.org` → `codeberg`.
- Lift the `match host` from `yocto/recipe.rs::parse_git_src_uri` into a shared function that both readers call, so the mapping cannot drift. Yocto's output must stay byte-identical, and its tests guard that.
- The submodule reader adds the URL forms Yocto does not see: scp style (`git@host:owner/repo.git`), `ssh://`, and `https://` with or without `.git`.
- A path with more than two segments (nested GitLab groups) falls back to generic, as Yocto's parser already does.

## R16 — Forge PURL case (FR-004, FR-015; analysis remediation)

**Measured.** purl-spec's `types/` directory (43 files, 2026-11-15) has `github-definition`, `bitbucket-definition` and `generic-definition`, and no `gitlab`, `codeberg` or `sourcehut` definition. `nix/identity.rs` records from the `github` and `bitbucket` definitions: "It is not case sensitive and shall be lowercased", for both namespace and name.

**Finding.** `Purl::new` does not fold case. The CMake reader builds `pkg:github/<owner>/<repo>` from the URL as written (`cmake.rs`, `parse_fetch_block`), and so does the Yocto reader. Only the Nix reader lowercases.

**Decision** (user, analysis remediation).
- Submodule PURLs lowercase `github` and `bitbucket` owner and repository, and keep the declared case for `gitlab` and `codeberg`.
- The CMake reader's `pkg:github` and `pkg:bitbucket` PURLs are lowercased in this feature, so a submodule and a CMake fetch of `ROCm/x` at one commit merge (SC-011).
- Yocto is filed separately.
- The `golden/*/cmake.*` goldens are unaffected: their one GitHub PURL is `google/googletest`, already lowercase.

## R10 — C57 path references (FR-008)

**Decision.** A second collector beside the existing `collect_find_package_targets` (`cmake.rs:514`, currently `#[allow(dead_code)]`, which this feature wires up) collects:
- `add_subdirectory(<dir> …)`;
- the `SOURCE_DIR <dir>` argument of `ExternalProject_Add` and `FetchContent_Declare`.

Each path is resolved against the referencing file's directory, after substituting `${CMAKE_CURRENT_SOURCE_DIR}`, `${CMAKE_CURRENT_LIST_DIR}`, `${PROJECT_SOURCE_DIR}` and `${CMAKE_SOURCE_DIR}` (the last two as the scan root). A path that still holds `${` after substitution is ignored. A submodule is `declared-and-used` when its path equals, or is an ancestor of, a collected path.

**Known limit.** Both collectors read only the files `discover_cmake_files` returns: the root, `cmake/`, `Modules/` and `third_party/`. That is #1516's limitation. A super-build whose `add_subdirectory` calls live in nested projects reads `declared-only` until #1516 lands. This is recorded, not worked around.

## R11 — The coverage record (FR-011)

**Decision.** New catalog row **C222**, `waybill:git-submodule-coverage`. It is document-scope and present only when a `.gitmodules` file was seen. Its value is a JSON object with sorted keys:

```json
{"pinned": 3, "skipped": {"git-refused": 0, "git-unavailable": 0, "no-gitlink": 1, "no-repository-data": 0, "no-url": 0, "unsafe-path-or-url": 0}}
```

All six reasons are always present. Extractors go in all three formats, following the existing document-scope rows. C222 is the next free row: the catalog's highest is C221.

## R12 — Merging with other readers (FR-009)

**Finding.** `scan_fs/dedup.rs` already ranks `GitSubmodule` in the filesystem tier and groups by canonical PURL. A CMake `FetchContent_Declare(… GIT_REPOSITORY https://github.com/o/r GIT_TAG <40-hex commit>)` produces `pkg:github/o/r@<commit>`, identical to the submodule's PURL, so the existing pass merges them and records `git-submodule` in C56. No dedup change is needed. The `dedup_collision` test is rewritten to assert two components (Conan against GitHub), with a second fixture for the merging case.

## R13 — Which committed goldens move (SC-007)

**Measured** (`measurements/corpus-gitmodules.tsv`, `measurements/corpus-gitlinks.tsv`):
- **Gitlinks.** Each of the 20 public-corpus Git targets was checked at its pinned commit through GitHub's tree API. 0 targets have a gitlink. The control `grpc/grpc@v1.69.0` gives 16 gitlinks and a 16-path `.gitmodules`.
- **`.gitmodules` files.** 19 targets have none (HTTP 404). `haskell-language-server` has a comment-only `.gitmodules`: 453 bytes, removal instructions, 0 `path` lines.
- **Prediction.** Only the three `haskell-language-server` corpus goldens change, each gaining a C222 record of `pinned: 0` with every reason at 0. No corpus golden gains a component.
- **Spec amendment.** SC-007 is restated in terms of `.gitmodules` files, not gitlinks. The record is emitted whenever a `.gitmodules` file is seen, and that target has a file with no gitlinks.
- **First attempt invalid.** The first run of this check returned zero `path` lines for every target, because the shell loop assigned a variable named `path`, which zsh ties to `PATH`. That output was discarded and re-measured in Python with the positive control above.

## R14 — Which repositories are read; scanning a subdirectory

**Decision.** The reader takes the `.gitmodules` files the shared walker found under the scan root. For each one whose directory is a repository's top level, it reads that repository's gitlinks. It recurses into each gitlink whose directory is an initialised repository. Repositories are deduplicated by canonical top-level path.

**Spec amendment.** When the scan root is a subdirectory of a working tree whose `.gitmodules` lies above the scan root, nothing is emitted. The reader does not read a repository above the scan root, as the golang reader's `git describe` ladder already avoids a parent `.git` (`golang/legacy.rs`), so that a fixture scanned inside a host checkout never takes the host's data. This replaces the spec edge case that emitted gitlinks under a subdirectory scan.

## R15 — The subprocess runner

**Decision.** Generalise `nix/eval/invoke.rs::run_bounded` to take the program name: it currently hard-codes `nix`, and the drain-on-threads and kill-on-deadline logic is exactly what is needed. `run_bounded` keeps its signature and delegates, so the Nix tier is unchanged. A small `argv_is_safe` counterpart asserts at the call site that no argument derived from `.gitmodules` starts with `-` and that `--` precedes pathspecs.
