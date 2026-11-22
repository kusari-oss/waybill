# Research: A requirements file belongs to the project in its directory, not to a lockfile entry beside it

File references are to `waybill-cli/src/` at `main` `79b3350b`. "Traced" means read in code. "Measured" means run; the runs are in `measurements/probe.md` (script `measurements/probe.sh`).

## R1. Why two packages claim one directory (measured, traced)

- **Measured:** on #1640's tree, `waybill-fixture-devtool` has `source-files ["tool/uv.lock"]` and workspace-member `["tool"]` with `tool/uv.lock` present, and `["tool/devtool"]` for both without it. The owner of `tool/` follows name order: the edge into `waybill-fixture-shared` comes from the dev package as `devtool`, and from `waybill-fixture-tool` as `zdevtool`.
- **Traced:**
  - **`scan_fs/package_db/pip/mod.rs::read`** runs the lockfile readers per project root (`merge_without_override`), then builds one main module per `pyproject.toml` (`build_pip_main_module_entry`).
  - **The augment-existing branch.** When an entry of the same PURL exists, the branch copies the synthesized entry's annotations (`or_insert`), `parent_purl`, `depends`, `sbom_tier` (if unset) and `licenses` (if empty). It does **not** copy `source_path`. The lockfile entry's `source_path`, `tool/uv.lock`, survives.
  - **`scan_fs/mod.rs`** builds `evidence.source_file_paths` as `source_path` followed by `extra_source_paths`.
  - **`scan_fs/declared_attach.rs::OwnerIndex::build`** takes a main module's directory from `derive_workspace_root(source_file_paths.first())`. So does the `waybill:workspace-member` tagger. For `tool/uv.lock` that is `tool`, for both packages.
  - Both candidates are `pkg:pypi` main modules (`RANK_PYPI_MAIN_MODULE`), so `best` keeps the lower `(rank, purl)`, which is the lower PURL.
- **Not the uv.lock reader.** `uv_lock.rs` sets `main-module` only on `[tool.uv.workspace]` members (`detect_workspace`). #1640's tree has no workspace block. The role comes from the pip main-module pass.

## R2. FR-001: keep the manifest's directory through the merge

- **Decision:** in the augment-existing branch, when the existing entry is **not installed** (`sbom_tier` is not `deployed`):
  - set `existing.source_path` to the synthesized main module's `source_path`, the manifest's directory;
  - move the old `source_path` into `existing.extra_source_paths` (if not already present), **first**, ahead of any extras it already had.

  `source_file_paths` then reads `[<manifest dir>, <lockfile>, …]` (Clarifications Q1). `OwnerIndex` and the workspace-member tagger read the first entry, so both follow without change.
- **Installed entries stay as they are (planning amendment to FR-001).** A venv or `dist-info` entry merged with a manifest keeps its own path first.
  - `tag_components_with_layer_digest` (`scan_fs/mod.rs`) reads `source_file_paths[0]` to stamp `waybill:layer-digest`. Moving a deployed entry's path out of first place would drop the layer digest on image scans.
  - The deployed evidence is what the installed tier is for.

  The census (R6) counts installed-entry merges, so what this leaves behind is known. The spec's FR-001 is amended to say "lockfile" instead of "lockfile or installed".
- **The `editable` path is a cross-check, not the source.** FR-001 says the lockfile's `source.editable` path MUST agree with the manifest. The uv.lock reader records the editable path relative to the lock. When the augment branch merges a uv.lock entry, it compares that path, resolved against the lock's directory, with the manifest's directory. A disagreement is logged at debug level, and the manifest wins. This needs the editable path carried on the entry: a field on the parse result consumed in the same function, not a new annotation, so nothing new reaches the wire.
- **Poetry and Pipfile** entries go through the same branch, so they get the same fix. Their entries carry no editable path, so the cross-check does not apply to them.
- **Alternatives considered:**
  - **Use the editable path as the directory.** This only fixes uv. Poetry would still lose the directory, and the manifest is the authority (spec Assumptions).
  - **Fix only `OwnerIndex`** (spec Clarifications option A). Rejected in clarification: the annotations would keep reporting `tool`.

**Found at T007 (implementation amendment).** Spec 1542 sorts a merged component's `source_file_paths` in the deduplicator (`best.evidence.source_file_paths.sort()`), and `canonicalize_source_files_by_purl` sorts the union. So "first" is the lexicographically first raw path, and a lockfile's absolute path (`/…/uv.lock`) sorts before a main module's `path+file://…` directory. The reorder alone therefore does not decide ownership. `OwnerIndex` instead takes a main module's directory from **the first source path that names a directory in the tree** (`project_path`), falling back to the first path (CMake and Go record a manifest file). The pip reader's swap still matters: without it the manifest's directory is not among the paths at all. C120 already lists the workspace root of every path, so it gains the manifest's directory beside the lock's.

## R3. FR-003: the tie-break in one directory

- **Step 1 holds by construction after R2 (planning amendment).**
  - A candidate's directory is derived from its own first source path.
  - R2 makes that path the manifest's directory for every pip main module merged with a lockfile.
  - A pip main module exists only where the main-module pass read a manifest: `build_pip_main_module_entry`, or `uv_lock.rs` for workspace members whose `pyproject.toml` `detect_workspace` read.

  So no main-module candidate is left whose first path is a lockfile, and a key for step 1 would be unreachable. The census (R6) verifies this, counting candidates whose first path is a lockfile file name (expected 0 after R2). The lockfile names live in the census script, not in a second Rust list (the readers hard-code `uv.lock`, `poetry.lock` and `Pipfile.lock` at their own call sites).
- **Decision:** `OwnerIndex`'s candidate key goes from `(rank, purl)` to `(rank, non_runtime, purl)`:
  - `non_runtime`: the candidate's `lifecycle_scope` is non-runtime (step 2). No scanned tree is known to produce two same-rank pip candidates after R2, so this step is tested at unit level (T008).
  - `purl`: as on `main` (step 3).
- **The log.** When two candidates tie on `(rank, non_runtime)` and the PURL decides, emit one `debug` event on target `waybill::owner_index` with the directory and every candidate.
- **The census counts how often each step decides.** After R2, the measured #1640 tie no longer exists. Step 2 is a defence for trees the census may find, so it gets a unit test and a mutation even if the census finds 0.
- **Alternative rejected: rank by the editable path being `.`.** It is uv-only, and it is unavailable once the entry is merged.

## R4. FR-004: why a root-declared package becomes non-runtime (measured)

- **Measured** (probe R4, three runs each):
  - `requirements.txt` at the scan root declares `waybill-fixture-shared==2.0`;
  - `app/` is a project whose `uv.lock` has a dev-group registry package, `waybill-fixture-devlib`, that depends on `waybill-fixture-shared`;
  - the document root is `pkg:pypi/waybill-fixture-app@1.0.0`, and it has a direct edge to `waybill-fixture-shared`;
  - **result:** `waybill-fixture-shared` is `excluded`, `lifecycle-scope=optional`, C62 `transitive`, in 3 of 3 runs;
  - **control,** with only the requirements file moved into `app/`: `required`, in 3 of 3 runs. The emitted edges are the same in both: root and `devlib`.

  The edge the document shows is the same, but only the anchored one is lost.
- **Traced cause:**
  - A requirements file with no owning project below the scan root becomes a **declared root anchor** (spec 1537 R4). `cli/scan_cmd.rs` installs anchors, top-level git submodules included, through `supplement::install_reader_root_anchors`. They become the root's edges at emit time.
  - `resolve/scope_propagation.rs::propagate_non_runtime_scope_seeded` asserts it sees no anchoring edges (`assert_no_anchoring_edges`). Its immunity search starts from `component_root_seeds` (the selected root main module and one main module per ecosystem) plus spec 1616's `added_seeds` (runtime main modules).
  - An anchor target is neither. So a root-declared package with a non-runtime path into it has no runtime path that protects it, and Phase B gives it the non-runtime scope.
  - In the control, `app` owns the file, so the edge `app → shared` is a relationship from a seed, and `shared` is immune.
- **This is #1640's large-tree observation** (five root-declared packages `excluded`, C62 `transitive`). Whether those five are exactly this mechanism is confirmed by the census (R6), not assumed.
- **The minimal #1640 tree does not reproduce it** because the dev package there is itself a project without a dependency path from a seed. The propagation logs `assigned nothing` (`immune=3`).

## R5. FR-005: anchors protect what they anchor

- **Decision:** the reader root anchors (declared root anchors plus top-level submodules, the list `scan_cmd` already builds before the propagation) join spec 1616's `added_seeds`, under two filters:
  - **The declaring file.** A declared root anchor is a seed only if at least one *runtime* requirements file declares it: `classify_requirements_scope_under` returns no Optional scope for that file. `declared_attach::attach` records these as `runtime_root_anchors` beside `root_anchors`. **Measured** on main's binary: with `requirements-dev.txt` at the scan root and the same package reached through a dev-group lockfile entry, the merged component carries **no** reader scope. The lockfile entry's absent scope wins the merge, and the scope it has comes from the propagation (`optional`, C62 `transitive`). A filter on the component's scope alone would make that dev anchor a seed, and report a dev-only package as runtime (case A-3).
  - **The component.** `is_runtime_seed_candidate`: the anchor target's own scope is runtime, and it is not under a listed CI or docs dot-directory.

  Top-level submodule anchors pass the first filter: a submodule declares nothing by file.
  - **Only the immunity search widens.** Graph completeness and the FR-012 rooted-ecosystem gate keep `seeds`, exactly as spec 1616 set it up.
  - **Every ecosystem** (Clarifications Q2). The anchor list is ecosystem-neutral.
  - **The summary** gains `immune_by_root_anchors`, counted apart from `immune_by_added_seeds`, as spec 1616 FR-006 did for main modules.
- **The rest of FR-005 already holds:**
  - **The root by relationship:** when the root is a main module it is a seed (`component_root_seeds`).
  - **A runtime main module:** spec 1616 seeds it.

  The new invariant test covers all three kinds of direct dependent, so a regression in any of them fails.
- **Order:** the anchors are computed before the propagation in `scan_cmd` (the reader-anchor block precedes the propagation block), so the list exists when the seeds are formed. It must be kept as an owned `Vec` past `install_reader_root_anchors`.
- **Alternatives considered:**
  - **Add the anchors to `relationships` before the propagation.** This would break `assert_no_anchoring_edges`, whose reason (an anchor makes a non-runtime closure look runtime) is about graph-top anchoring, and it would change graph completeness.
  - **A post-pass that withdraws the scope.** The scope would be assigned and then taken back, and C62 bookkeeping would need a second path. The seed is where immunity is already decided.

## R6. The census (SC-001)

- **A log-only commit** emits `debug` events on target `waybill::owner_census`, with output unchanged:
  - `owner_candidates`: per directory with more than one candidate at the same rank, the candidates with `(rank, non_runtime, purl)`, each candidate's first source path (for R3's lockfile check), and which step decided;
  - `merge_source`: per augment-branch merge, the existing entry's tier, its source path, the manifest directory, and the editable path if any. This covers R2's lockfile and installed counts, and the editable cross-check;
  - `dev_only_main_module`: per main module that a dependency group alone lists, and what keys on it (root selection, workspace-member, owner candidacy, `added_seeds`), for FR-002;
  - `anchor_immunity`: per component that R5's anchor seeds would make immune and that is non-runtime today, with its ecosystem and the kind of direct dependent.

  `dev_only_main_module` is detected from the graph: a main module whose only inbound edges are `DevDependsOn`/`OptionalDependsOn` and whose C122 names a PEP 735 group. It does not read the lockfile's group tables. Every event records a fact available on `main`. `anchor_immunity` reruns the immunity search with the anchors added, on a copy, and assigns nothing.

  **Owner changes are not simulated.** Which requirements packages, CMake fetches and searches change owner is counted from the wired commit's per-component diff against `main` (SC-004). Recomputing ownership in the census script would be a second model of `OwnerIndex`, one that could drift from the binary.
- **Where it runs:**
  - **TheRock and the 18 quality-corpus targets:** a new workflow, `census-owners.yml`, in the TheRock fixture repository, modelled on `census-conditions.yml`. It takes the waybill commit as a full SHA and clones TheRock with top-level submodules. Quality targets are cloned at their pins, from `xtask/corpus/quality-corpus.toml`. `census/owners.py` reads the log. TheRock in CI only.
  - **The public corpus** (planning amendment to SC-001): its harness does not keep scan logs. At the log-only commit, `regen_goldens` must give 0 differing files, which confirms the commit is log-only. Its census is then the regenerated diff at the wired commit, traced component by component (SC-004). This is how spec 1641 measured the public corpus.
- **Decisions the census can overturn:**
  - **FR-002's default:** keep the main-module role. This is revisited if a dev-only main module still owns a directory, or is selected as a root, after R2.
  - **FR-003's order:** revisited if step 3 still decides a requirements file's owner anywhere.
  - **R2's installed-entry exemption:** revisited if an installed merge changes a requirements owner.

## R7. Tests

- **Unit:**
  - `declared_attach.rs`: the key order (R3), with one candidate pair per step;
  - `pip/mod.rs`: the augment branch keeps the manifest's directory first, and leaves an installed entry alone (R2);
  - `scope_propagation.rs`: a runtime anchor makes its target immune, and a non-runtime anchor does not (R5).
- **Integration:** `waybill-cli/tests/requirements_owner.rs`, with the cases in `contracts/wire.md`. Each case checks the edges, scope, `source-files` and workspace-member across the three formats, parity-check, CycloneDX validity, unique `bom-ref`s, and 10 byte-identical scans.
- **Control:** the cases land at the log-only commit. Every case that this spec changes must fail there, with `main`'s output.
- **Mutations** (SC-006), each with its killing test:
  - drop R2's `source_path` swap;
  - apply R2 to installed entries too (must break layer-digest or the installed case);
  - remove FR-003's `non_runtime` key;
  - drop the anchor seeds;
  - drop the `is_runtime_seed_candidate` filter on anchors.

  Restore with `cp`, rebuild after the last one, and use an `^error\[E` guard.

## R8. What the wire carries

No new catalog row. The changes appear in fields that already exist:
- edges (CycloneDX `dependencies`, SPDX relationships);
- `scope`, `waybill:lifecycle-scope` and C62 `waybill:lifecycle-scope-derivation`, where FR-005 removes a transitive scope;
- `waybill:source-files` (C18) and `waybill:workspace-member`, where R2 puts the manifest directory first.

Parity extractors for these rows already exist, and their directionality is unchanged.
