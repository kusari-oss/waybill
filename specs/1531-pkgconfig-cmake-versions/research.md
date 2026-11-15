# Research: installed library versions from pkg-config and CMake version files

All line numbers are at `96ad6804` (main after #1559). **Read** means the claim was seen in the code; **inferred** means reasoned from it. Each inferred claim that a task depends on is re-verified by that task (memory rule: re-verify research claims at implement time).

## R1 — How the reader receives files

**Read.** The shared walker (`scan_fs/walk_registry/`) dispatches by basename glob. A `ReaderRegistration` carries `patterns: GlobSet`, optional `state`, and `on_file` (`walk_registry/mod.rs:390-419`). The git-submodule reader is the newest example: it registers `**/.gitmodules` and collects paths into an `Arc<Mutex<BTreeSet<PathBuf>>>` (`package_db/git_submodule/mod.rs:107-145`). A registration can carry several patterns; the CMake reader uses `**/CMakeLists.txt` and `**/*.cmake` (`cmake.rs:107-110`). Several readers may match one file (`walk_registry/dispatch.rs:40-60`).

The walker skips any dot-directory and `node_modules`, `bower_components`, `vendor`, `target`, `dist`, `build`, `out`, `coverage`, `__pycache__` and `venv` (`project_roots.rs:38-55`), to depth 16 (`package_db/mod.rs:2783`). It has no size filter.

**Decision.** A new reader registers `*.pc`, `*ConfigVersion.cmake` and `*-config-version.cmake`. Its `on_file` only collects candidate paths; nothing is parsed in the walker callback.
- **Build trees.** They are not read, because the walker already skips `build/`, `out/` and `dist/`. This satisfies the spec's edge case for source and build trees without a reader rule.
- **Python virtual environments (amended during analysis: C1).** The walker also skips every dot-directory and `venv`, so a wheel installed into `.venv/` would be invisible. The pip reader avoids that only because it lists its own locations (`pip/dist_info.rs:153-174`). This reader therefore sets `descend_into` to `.venv` and `venv`, the C10 mechanism that go_binary already uses for `venv` (`go_binary.rs:624-637`). Dispatch under a subtree opened this way goes only to the readers that requested it (`walk_registry/mod.rs:398-418`), so other readers' output is unchanged.
  - **Cost.** Every file under a project's virtual environment is now visited. *Inferred:* a large ML environment holds tens of thousands of files. Task T040 measures the walk time with and without the override on a synthetic `.venv` of 50,000 empty files, taking 3 runs each.
  - **Alternative rejected.** Reusing `candidate_site_packages_roots` and walking each site-packages directory separately. That duplicates the walker's symlink-loop, depth and exclusion handling for one reader.
- **Shared match with the CMake reader.** `*-config-version.cmake` and `*ConfigVersion.cmake` also match the CMake reader's `*.cmake`. *Inferred:* the CMake reader emits nothing from a file holding only `set(PACKAGE_VERSION …)` and `if()` blocks. Task T005 verifies this with a fixture before any reader code exists.

## R2 — Where ownership, binaries and the reader's records meet

**Read.** The order inside `package_db::read_all`:
1. The dpkg, apk, alpm and opkg readers and their claims (`mod.rs:1692-1782`).
2. The shared-walker pilot and every walker reader's finalize (`mod.rs:1829`).
3. The pip `RECORD` claims (`mod.rs:1928`).
4. The rpm reader and its claims (`mod.rs:2044-2054`).
5. `go_binary::finalize`, the only finalize given the claims (`mod.rs:2076`).

`scan_fs::scan_path` then calls `binary::read(root, &claimed_paths, &claimed_inodes, …)` and extends `db_entries` (`scan_fs/mod.rs:505-521`). After that it builds the name index (~`:618`), converts entries to components and edges (`:903-1290`), and deduplicates (`:1329`).

**Decision. Two phases, like `go_binary`.**
- **Phase 1:** the walker pass collects candidate paths, and the reader's finalize parses them into records. Parsing is pure and needs no claims.
- **Phase 2:** a new post-pass in `scan_path`, called right after `binary::read` and before the name index. It holds the OS-ownership claims, the binary file components and the parsed records. It:
  1. drops OS-owned records (R3);
  2. resolves `Requires` and `Requires.private` within each prefix (R6);
  3. builds the entries;
  4. lists matching binaries in their evidence (R9);
  5. computes the coverage record (R8).

**Alternative rejected.** Moving the rpm claims ahead of the pilot. That reorders a reader every image scan depends on, for one consumer.

## R3 — "Owned by an OS package" (FR-015)

**Read.**
- **The claim set.** `DbScanResult.claimed_paths: HashSet<PathBuf>` and `claimed_inodes` (`package_db/mod.rs:325-339`) are filled by dpkg (`.list` files, `dpkg.rs:175-218`), apk (`F:`/`R:` lines, `apk.rs:81-133`), alpm (`%FILES%`, `alpm.rs:486-543`), opkg (`*.list`, `opkg.rs:141-185`) and rpm (rpmdb basenames and dirnames, `rpm.rs:148-179`).
- **The lookup.** `binary::is_path_claimed` (`binary/mod.rs:188-215`) matches the raw path, then the canonical path, then `(dev, ino)`.
- **The same set holds language claims.** pip `RECORD` paths (`mod.rs:1928`) and ipk archive claims (`mod.rs:1777`) go into it too. It is a flat set with no owner.
- **Homebrew claims nothing.** It records its file-claim integration as deferred (`mod.rs:1747-1750`).

**Decision.**
- **A separate OS-only set.** `DbScanResult` gains an `os_claimed` set (paths and inodes) with the same shape. The dpkg, apk, alpm, opkg and rpm call sites insert into it, as well as into the existing set; pip and ipk do not. The existing set and its users are unchanged, so output is byte-identical with no `.pc` present.
- **Why not reuse the shared set.** It contains pip `RECORD` paths. Using it would skip the `.pc` files inside a ROCm wheel, the case FR-015 says must be read.
- **Homebrew.** A record whose canonical path lies under `<brew-prefix>/Cellar/<name>/<version>/`, where that directory holds `INSTALL_RECEIPT.json`, is OS-owned. The prefixes are those `brew.rs` already uses (`HOMEBREW_PREFIXES`, `brew.rs:53-57`). This is a check local to the reader, not a new claim source, so Homebrew's deferred claim integration stays deferred.

**Resolution inside the root (amended during analysis: G1).** `binary::is_path_claimed` calls `std::fs::canonicalize` on the path it is given (`binary/mod.rs:197-199`). In an extracted image, an absolute symlink then resolves against the **host**. For a claim lookup that is harmless, since the claim set holds only paths under the root, but it must not be used to decide where to read a file.
- **Reading.** The reader reads a `.pc` or CMake version file only after resolving it lexically against the scan root: an absolute link target is joined to the root, `..` is normalised, a component leaving the root rejects the file, and links are followed up to 40 hops.
- **The Homebrew keg check.** It uses the same root-relative resolution, not `canonicalize`.
- **Ownership.** It still uses `is_path_claimed`, since claim lookups read nothing.

**Limitations, recorded and not fixed here.** Each of these has no ownership data, so FR-015 reads the file:
- **rpm BDB databases.** `rpmdb_bdb.rs` is a stub that emits no packages (`rpmdb_bdb.rs:23-28`), so no duplicate identity exists either.
- **dpkg `status.d`-only (distroless) images.** Claims read `info/*.list` only (`dpkg.rs:180`). A distroless `-dev` package's `.pc` would be read and duplicate its deb component. *Inferred:* this is rare, since distroless images ship no `-dev` packages. It is filed as a follow-up.
- **`--fast-container-extract`.** `.pc` files are not extracted at all (`docker_image.rs:55-70`), so nothing is read.

## R4 — How components merge (FR-009, SC-003)

**Read.**
- **The group key is not the PURL.** `deduplicate` groups by `(ecosystem, name, version, parent_purl, pants-namespace)` (`resolve/deduplicator.rs:13, 44-53`). The winner has the highest evidence confidence, and every package-db entry has `0.85` (`scan_fs/mod.rs:76, 1023`). With equal confidence the first in insertion order wins, since `sort_by` is stable.
- **What survives a merge.** The winner keeps its C55 and its `sbom_tier`. A loser's annotation is added only for a key the winner lacks (`deduplicator.rs:297`). `source_file_paths` and occurrences are unioned (`:81-94`).
- **C56 `also-detected-via`.** It is still never emitted: `scan_fs::dedup::dedup` has no caller, and `builder.rs:1507-1513` passes `None, &[]`.
- **The embedded-version scanner.** It emits `pkg:generic/<slug>@<version>` with tier `analyzed`, using fixed lowercase slugs: `zlib`, `expat`, `ncurses`, … (`binary/entry.rs:216-259`, `binary/version_strings.rs:85-103`).
- **`find_package`.** It emits `pkg:generic/<lowercased>` with **no version** (#1520), so it never merges with a versioned component (`cmake.rs:228-285`).

**Decision.**
- **Order the entries so the reader wins.** The post-pass inserts its entries into `db_entries` ahead of the binary entries, with `.pc` entries before CMake ones. Then a merge keeps `deployed` tier and the reader's C55, with `pkg-config` ahead of `cmake-package-version`, ahead of the embedded-version scanner's `analyzed`.
- **Spec amendments.** SC-003 and US1 scenario 5 said a merged component carries "both mechanisms". That requires C56, which is deferred to #1558, as spec 1530 also found. They are amended to "one component, with the first reader's C55 and the union of both readers' evidence paths".

**Re-verified at `d8723f62` (T001).**
- `deduplicate` groups by `(ecosystem, name, version, parent_purl, pants-namespace)` (`resolve/deduplicator.rs:13, 44-53`).
- It sorts by confidence with a stable `sort_by`, so the first of equal-confidence entries wins (`:65-72`).

Unchanged.

## R5 — Identity

**Decision.**
- **PURL.** `pkg:generic/<name>@<version>`, with the name lowercased and both segments encoded by `encode_purl_segment`. That is the slug shape of the embedded-version scanner and the lowercase rule of `find_package`. purl-spec has no pkg-config or CMake type (FR-003).
- **Component name.**
  - For `.pc`: the file stem, which is the module name `Requires` refers to.
  - For CMake: the file name minus `ConfigVersion.cmake` or `-config-version.cmake`.

**Case.** `ZLIBConfigVersion.cmake` and `zlib.pc` both give `zlib`. The declared case is kept in a `waybill:cmake-package-name` annotation only when it differs, mirroring `waybill:cmake-find-package-name`. **Decided (T003): reuse C103.** Its definition is "the original casing … emitted only when it differs from the lowercased PURL name segment". That is exactly this value. The definition is widened to name the package version file as a second source, and no new row is added.

## R6 — Edges (FR-004, FR-011)

**Read.**
- **Exact-PURL dependencies.** `resolve_exact_purl_dep` (`scan_fs/mod.rs:227-232`) turns a `depends` string that is a `pkg:` PURL present in `emitted_purls` into a `DependsOn` edge. Spec 1530 added it, and the git-submodule reader uses it (`git_submodule/mod.rs:336-346`).
- **A missing target.** A `pkg:` string that names nothing falls through to the name lookup and becomes C115 `waybill:unresolved-declared-dep`.

**Decision.**
- **Resolution.** The post-pass resolves each `Requires` and `Requires.private` module name itself: first in the same prefix, then across the scan, and only when exactly one record has that name. It writes the exact target PURL into `depends`.
- **Unresolved names.** These are not written to `depends`. Writing a bare name would let the generic name lookup bind it to an unrelated `pkg:generic` component, such as a versionless `find_package`. They are counted in the coverage record and logged.
- **Private-only edges.** They are listed in a per-component annotation (R8). A module in both lists is not private.
- **Dedup.** `depends` is deduplicated and sorted, so output is byte-identical.

## R7 — Parsing

**pkg-config `.pc`.** This follows pkg-config's `parse.c` semantics:
- `name=value` lines define variables, and `Key: value` lines are fields.
- `#` starts a comment, and a trailing `\` continues the line.
- `${var}` expands recursively from the file's own variables. `$$` is a literal `$`.
- A cycle or an undefined variable leaves `${…}` in place, which then triggers `unresolved-version`.
- `Requires` entries are split on commas and whitespace. A token that is one of `=`, `!=`, `<`, `<=`, `>`, `>=` consumes the token after it as a version. Neither token is a module name.
- A missing `Version:` field gives `no-version`. Non-UTF-8 content, or a file over 1 MiB, gives `malformed`. 1 MiB is far above any real `.pc`, which is a few hundred bytes.

**CMake version file.**
- The first `set(` (case-insensitive) whose first argument is `PACKAGE_VERSION` and whose second argument is a literal (quoted, or a bare token without `${`) gives the version.
- Anything else gives `computed-version`, and that includes `set(PACKAGE_VERSION ${X})` or no such `set`.
- No CMake is evaluated. `write_basic_package_version_file` output always has the literal form. *Inferred:* this comes from CMake's `BasicConfigVersion-*.cmake.in` templates, and is verified by task T002's fixture taken from a real generated file.

**Locations (FR-001, FR-006).**
- **For `.pc`:** the parent directory is `pkgconfig`, and its parent is `lib`, `lib64`, `lib32`, `libx32` or `share`, or is a multiarch triplet directory whose own parent is `lib`.
- **The multiarch triplet:** three or four `-`-separated lowercase alphanumeric or `_` fields, such as `x86_64-linux-gnu` or `aarch64-linux-gnu`.
- **For a CMake version file:** some ancestor named `cmake` or `CMake` has a parent of `lib*` or `share` (or `lib/<triplet>`). Alternatively, the file is in `<prefix>/<Pkg>*/cmake/` or `<prefix>/<Pkg>*/CMake/`.
- **Anywhere else:** the file gives `unrecognised-location`.
- **The prefix:** the directory above that `lib*`, `share` or `lib/<triplet>` level.

## R8 — New catalog rows

**Read.** The last row is C222 (`docs/reference/sbom-format-mapping.md`). A new row needs extractors in all three formats in the same change. That gate lives in the lib target (memory: `feedback_sbom_format_mapping_extractor_gate`).

**Decision.** Two rows, numbered at implementation time as the next free numbers (expected C223 and C224):
- `waybill:installed-library-coverage`: document scope, a JSON object of counts (contracts/wire.md).
- `waybill:pkg-config-private-requires`: component scope, a JSON array of exact PURLs, sorted.

The C55 catalog text gains two values, `pkg-config` and `cmake-package-version`.

## R9 — Binaries (FR-010, US3)

**Read.**
- **No soname is read.** DT_SONAME is not read anywhere in `scan_fs/binary/`. The ELF code reads only DT_NEEDED and RPATH/RUNPATH (`binary/elf.rs:180-218`), and Mach-O skips the image's own `LC_ID_DYLIB` (`binary/macho.rs:255-292`).
- **The file component.** A shared library's component is `pkg:generic/<filename>?file-sha256=…` with `binary_role = SharedLibrary` (`binary/entry.rs:573-665`, `binary/role.rs:21-65`).
- **Claimed binaries.** They get no file component (`binary/mod.rs:217-226`).

**Decision.**
- **Match by file name only.** The match is against shared-library file components in `db_entries` whose path is in the record's prefix: `lib<module>.so`, `lib<module>.so.<N…>`, `lib<module>.dylib`, `lib<module>.<N…>.dylib`, or `<module>.so*` when the module name already starts with `lib`.
- **Spec amendment.** FR-010 is amended to drop "soname" (not read today, and adding it is out of scope) and `.a`. The binary scan emits no file component for static archives, which is #1534's territory.
- **An ambiguous match** (two modules) lists the binary under neither.
- **Evidence.** The matched path goes into the library entry's `extra_source_paths`. It becomes a C18 source file and a CycloneDX occurrence, and merges like any other evidence path (R4).

**Risk (inferred, to measure).** `manifest_occurrence` hashes every `extra_source_paths` file, capped at 256 MiB (`scan_fs/mod.rs:1754-1808`). An SDK with large shared libraries would be hashed a second time. Task T030 times the US3 fixture with a large sparse file. SC-006's CI run records the scan time against a baseline without this feature. If hashing dominates, the occurrence takes the hash from the binary component's `file-sha256` qualifier.

## R10 — Which goldens move (SC-007)

**Measured** (`measurements/corpus-pc-cmake.tsv`, `corpus-pc-cmake-control.tsv` and `corpus-pc-cmake.py`):
- **Git targets.** All 20 public-corpus git targets were checked at their pinned commits through GitHub's tree API. None of the trees was truncated, and every target has 0 `.pc` files and 0 CMake version files.
- **Positive control.** `ensemblepourladifference/pamoja-opencellular@45102cf0` gives 12 `.pc` files and 1 version file, so the zeros are real.
- **The image target.** `image-postgres16` was not pulled. Its committed goldens already name `/usr/lib/x86_64-linux-gnu/pkgconfig/nss_wrapper.pc` and `/usr/lib/x86_64-linux-gnu/cmake/nss_wrapper/nss_wrapper-config-version.cmake`. *Inferred:* both are owned by a dpkg package (libnss-wrapper), so FR-015 skips them.

**Prediction.** Only `image-postgres16`'s three goldens change. Each gains a coverage record with both files under `owned-by-os-package`, and no component. Task T034 verifies this with CI regeneration and a normalised diff (memory: `feedback_verify_golden_churn_normalized`).

## R11 — ROCm measurement (SC-006)

**Decision.** The measurement is taken only in CI, on a published ROCm SDK payload pinned by digest, never locally. A branch-only workflow step:
- fetches the payload;
- counts the `.pc` and version files;
- scans with this feature and without it;
- records both component counts and the scan time in `measurements/rocm-sdk.md`.

Which payload to use is chosen in task T036, with a pinned digest and nothing executed from it.

## R7 addendum — the CMake version-file fixture (T002)

Generated with CMake 4.4.5 by running `write_basic_package_version_file(… VERSION 4.5.6 COMPATIBILITY SameMajorVersion)` on a three-line project. It is saved as `installed_lib/testdata/BasicConfigVersion-SameMajor.cmake.txt`; the `.txt` suffix keeps it out of the CMake reader's `*.cmake` glob in a self-scan.

The file confirms the literal form `set(PACKAGE_VERSION "4.5.6")`. It also holds `set(PACKAGE_VERSION_COMPATIBLE …)` lines and `PACKAGE_VERSION` inside `#` comments. So the parser matches the first argument exactly, not as a prefix, and skips comments.
